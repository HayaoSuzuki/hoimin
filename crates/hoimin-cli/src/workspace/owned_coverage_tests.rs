use super::*;

fn deadline() -> std::time::Instant {
    std::time::Instant::now() + Duration::from_secs(2)
}

fn poison<T>(mutex: &Mutex<T>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = mutex.lock().unwrap();
        panic!("simulate an interrupted coordinator operation");
    }));
    assert!(result.is_err());
}

#[test]
fn poisoned_coordinator_locks_fail_without_recovering_unverified_state() {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
    // Keep the process-wide registry healthy; only this coordinator is poisoned.
    coordinator.local_lock = Arc::new(Mutex::new(()));
    poison(&coordinator.local_lock);
    assert!(matches!(
        CoordinatorLockGuard::try_acquire(&coordinator),
        Err(WorkspaceError::StatePoisoned)
    ));
    assert!(matches!(
        CoordinatorLockGuard::acquire_until(&coordinator, deadline()),
        Err(WorkspaceError::StatePoisoned)
    ));
    assert!(matches!(
        lock_local_until(&coordinator.local_lock, deadline()),
        Err(WorkspaceError::StatePoisoned)
    ));
    let registry = Mutex::new(HashMap::new());
    poison(&registry);
    assert!(matches!(
        lock_registry_until(&registry, &coordinator.path, deadline()),
        Err(WorkspaceError::StatePoisoned)
    ));
}

#[test]
fn conflicting_equal_generation_slots_are_rejected_without_rewriting() {
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join("coordinator");
    let mut bytes = initial_coordinator_bytes();
    let cursor = "run-00000000-0000-0000-0000-000000000001";
    bytes[513..].copy_from_slice(&coordinator_slot_bytes(0, cursor).unwrap());
    fs::write(&path, bytes).unwrap();
    let mut file = File::open(&path).unwrap();
    assert!(matches!(
        validate_coordinator_file(&mut file, &path),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(matches!(
        read_coordinator_state(&file, &path),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn coordinator_recovers_first_slot_when_second_is_corrupt_and_rejects_both_corrupt() {
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join("coordinator");
    let mut bytes = initial_coordinator_bytes();
    bytes[513] ^= 1;
    fs::write(&path, bytes).unwrap();
    let file = File::open(&path).unwrap();
    assert_eq!(
        read_coordinator_state(&file, &path).unwrap(),
        (
            0,
            CoordinatorSlot {
                generation: 0,
                cursor: String::new()
            }
        )
    );
    bytes[1] ^= 1;
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        read_coordinator_state(&file, &path),
        Err(WorkspaceError::InvalidPath { .. })
    ));
}

#[test]
fn coordinator_generation_overflow_keeps_persisted_bytes_unchanged() {
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join("coordinator");
    let bytes = initial_coordinator_bytes();
    fs::write(&path, bytes).unwrap();
    let file = File::options().read(true).write(true).open(&path).unwrap();
    let state = CoordinatorSlot {
        generation: u64::MAX,
        cursor: String::new(),
    };
    let error = persist_coordinator_cursor(&file, &path, 0, &state, "").unwrap_err();
    assert!(error.to_string().contains("generation overflow"));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn authenticated_slots_still_require_utf8_and_canonical_managed_names() {
    for cursor in [b"invalid".as_slice(), b"\xff".as_slice()] {
        let mut slot = initial_coordinator_slot();
        slot[20..22].copy_from_slice(&u16::try_from(cursor.len()).unwrap().to_le_bytes());
        slot[22..22 + cursor.len()].copy_from_slice(cursor);
        let digest = blake3::hash(&slot[..COORDINATOR_DIGEST_OFFSET]);
        slot[COORDINATOR_DIGEST_OFFSET..].copy_from_slice(digest.as_bytes());
        assert_eq!(decode_coordinator_slot(&slot), None);
    }
    for cursor in [
        "invalid".to_owned(),
        "x".repeat(COORDINATOR_CURSOR_BYTES + 1),
    ] {
        assert!(matches!(
            coordinator_slot_bytes(0, &cursor),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }
}

#[test]
fn expired_new_coordinator_initialization_removes_only_the_created_identity() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);
    let file = create_owned_file(&dir, COORDINATOR_FILE, OwnedFileAccess::ReadWrite).unwrap();
    let identity = file_identity(&file).unwrap();
    let error = initialize_new_coordinator(&dir, file, identity, &path, std::time::Instant::now())
        .unwrap_err();
    assert!(error.to_string().contains("deadline"));
    assert!(!path.exists());
}

#[test]
fn failed_initialization_preserves_replacement_and_reports_rollback_after_primary() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);
    let file = create_owned_file(&dir, COORDINATOR_FILE, OwnedFileAccess::ReadWrite).unwrap();
    let identity = file_identity(&file).unwrap();
    dir.rename(COORDINATOR_FILE, &dir, "original").unwrap();
    dir.write(COORDINATOR_FILE, b"replacement").unwrap();
    let error = initialize_new_coordinator(&dir, file, identity, &path, std::time::Instant::now())
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("deadline"));
    assert!(message.contains("rollback failed"));
    assert_eq!(dir.read(COORDINATOR_FILE).unwrap(), b"replacement");
}

#[test]
fn expired_rollback_reports_timeout_without_running_cleanup() {
    let now = std::time::Instant::now();
    let cleanup_called = std::cell::Cell::new(false);
    let cleanup = |_| {
        cleanup_called.set(true);
        Ok(())
    };
    let append = |primary: std::io::Error, secondary: std::io::Error| {
        created_directory_rollback_error(&primary, &secondary)
    };
    let primary = error_after_new_directory_rollback_with(
        std::io::Error::other("primary"),
        now + Duration::from_secs(1),
        || now,
        cleanup,
        append,
    );
    assert_eq!(primary.to_string(), "primary");
    assert!(cleanup_called.replace(false));
    let error = error_after_new_directory_rollback_with(
        std::io::Error::other("primary"),
        now,
        || now,
        cleanup,
        append,
    );
    assert!(!cleanup_called.get());
    assert!(error.to_string().starts_with("primary; secondary"));
    assert!(error.to_string().contains("deadline exhausted"));
}

#[test]
fn secondary_cleanup_error_preserves_non_io_primary_context() {
    let path = Utf8Path::new("managed-root");
    let error = append_workspace_new_directory_rollback_error(
        WorkspaceError::InvalidPath {
            path: path.to_owned(),
        },
        path,
        &std::io::Error::other("cleanup unavailable"),
    );
    assert!(
        matches!(&error, WorkspaceError::Io { operation: "construct managed directory", path: actual, .. } if actual == path)
    );
    let message = error.to_string();
    assert!(message.contains("managed-root"));
    assert!(message.contains("secondary new-directory rollback failure: cleanup unavailable"));
}

#[test]
fn newly_created_directory_must_match_both_metadata_and_opened_identity() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("regular", b"payload").unwrap();
    dir.create_dir("different").unwrap();
    let open_different = |parent: &cap_std::fs::Dir, _: &str| parent.open_dir("different");
    let rollback_identity = std::cell::Cell::new(None);
    let error = create_owned_directory_with(
        &dir,
        "regular",
        |_, _| Ok(()),
        |parent, name| parent.symlink_metadata(name),
        open_different,
        |_, _, identity, _| {
            rollback_identity.set(Some(identity));
            Ok(())
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed before it could be opened")
    );
    assert_eq!(
        rollback_identity.get(),
        Some(metadata_identity(&dir.symlink_metadata("regular").unwrap()))
    );

    dir.create_dir("created").unwrap();
    let error = create_owned_directory_with(
        &dir,
        "created",
        |_, _| Ok(()),
        |parent, name| parent.symlink_metadata(name),
        open_different,
        |_, _, identity, _| {
            rollback_identity.set(Some(identity));
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("identity changed while opening"));
    assert_eq!(
        rollback_identity.get(),
        Some(metadata_identity(&dir.symlink_metadata("created").unwrap()))
    );
    assert!(dir.exists("different"));
}

#[test]
fn coordinator_rejects_nonzero_layout_tag_and_directory_entries() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);
    let mut bytes = initial_coordinator_bytes();
    bytes[0] = 1;
    dir.write(COORDINATOR_FILE, bytes).unwrap();
    let mut file = File::open(&path).unwrap();
    assert!(matches!(
        validate_coordinator_file(&mut file, &path),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    drop(file);
    dir.remove_file(COORDINATOR_FILE).unwrap();
    dir.create_dir(COORDINATOR_FILE).unwrap();
    assert!(matches!(
        open_initialized_coordinator_until(&dir, &path, deadline()),
        Err(WorkspaceError::InvalidPath { .. })
    ));
}

fn coordinator_in(parent: &tempfile::TempDir) -> ManagedRootCoordinator {
    ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap()
}

#[test]
fn root_operations_fail_closed_when_lifecycle_and_marker_locks_are_poisoned() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    poison(&root.lifecycle);
    assert!(matches!(
        root.create_child("worker-"),
        Err(WorkspaceError::StatePoisoned)
    ));
    assert!(matches!(
        root.mark_cleanup_ready(),
        Err(WorkspaceError::StatePoisoned)
    ));
    let record = root.cleanup(Duration::from_secs(1));
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("lifecycle is poisoned"))
    );
    assert!(root.path.exists());

    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    poison(&root.heartbeat);
    assert!(matches!(
        root.refresh_heartbeat(),
        Err(WorkspaceError::StatePoisoned)
    ));
    poison(&root.lease);
    assert!(matches!(
        root.mark_cleanup_ready(),
        Err(WorkspaceError::StatePoisoned)
    ));
    assert!(matches!(
        root.take_owner_cleanup_capabilities(),
        Err(WorkspaceError::StatePoisoned)
    ));
}

#[test]
fn unavailable_markers_report_the_missing_capability() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery).unwrap();
    drop(root.heartbeat.lock().unwrap().take());
    assert!(
        root.refresh_heartbeat()
            .unwrap_err()
            .to_string()
            .contains("heartbeat is unavailable")
    );
    drop(root.lease.lock().unwrap().take());
    assert!(
        root.mark_cleanup_ready()
            .unwrap_err()
            .to_string()
            .contains("lease is unavailable")
    );
    let record = root.cleanup(Duration::from_secs(1));
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("lease is unavailable"))
    );
    assert!(root.path.exists());
}

#[test]
fn shared_lease_guard_defers_claim_and_restores_the_owner_slot() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let extra_guard = Arc::clone(root.lease.lock().unwrap().as_ref().unwrap());
    let record = root.cleanup(Duration::from_secs(1));
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("shared lease guard"))
    );
    assert!(root.lease.lock().unwrap().is_some());
    assert!(
        root.take_owner_cleanup_capabilities()
            .err()
            .unwrap()
            .to_string()
            .contains("shared lease guard")
    );
    assert!(root.lease.lock().unwrap().is_some());
    drop(extra_guard);
    assert_eq!(
        root.cleanup(Duration::from_secs(1)).disposition,
        CleanupDisposition::Clean
    );
}

#[test]
fn poisoned_coordinator_prevents_owner_cleanup_without_renaming_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let mut root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.coordinator_local_lock = Arc::new(Mutex::new(()));
    poison(&root.coordinator_local_lock);
    let record = root.cleanup(Duration::from_secs(1));
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("process lock is poisoned"))
    );
    assert!(root.path.exists());
    assert_eq!(
        NamespaceLifecycle::load(&root.namespace),
        NamespaceLifecycle::Active
    );
}

#[test]
fn serialization_failure_keeps_marker_error_context() {
    struct FailingMarker;
    impl Serialize for FailingMarker {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom(
                "injected marker serialization failure",
            ))
        }
    }
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let error = root
        .write_json_marker("failing-marker", &FailingMarker)
        .unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Io { operation: "write workspace marker", path, message } if path.ends_with("failing-marker") && message.contains("injected marker serialization failure"))
    );
}

#[test]
fn publication_marker_collisions_roll_back_the_staging_directory() {
    for marker_name in [LEASE_FILE, HEARTBEAT_FILE] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = coordinator_in(&parent);
        let error = ManagedRunRoot::create_with_publish_hook(
            &coordinator,
            OwnerKind::PublicExecution,
            &|boundary| {
                if boundary == PublishBoundary::StagingOpened {
                    let staging = fs::read_dir(&coordinator.path)
                        .unwrap()
                        .map(Result::unwrap)
                        .find(|entry| {
                            entry
                                .file_name()
                                .to_string_lossy()
                                .starts_with(STAGING_PREFIX)
                        })
                        .unwrap();
                    fs::write(staging.path().join(marker_name), b"preexisting").unwrap();
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            WorkspaceError::Io {
                operation: "create lease marker" | "create heartbeat marker",
                ..
            }
        ));
        assert_eq!(fs::read_dir(&coordinator.path).unwrap().count(), 1);
    }
}

#[cfg(unix)]
#[test]
fn publication_detects_an_absent_or_replaced_active_directory() {
    for replace in [false, true] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = coordinator_in(&parent);
        let captured_path = std::cell::RefCell::new(None);
        let error = ManagedRunRoot::create_with_publish_hook(
            &coordinator,
            OwnerKind::PublicExecution,
            &|boundary| {
                if boundary == PublishBoundary::RenamedActive {
                    let active = fs::read_dir(&coordinator.path)
                        .unwrap()
                        .map(Result::unwrap)
                        .find(|entry| {
                            entry
                                .file_name()
                                .to_string_lossy()
                                .starts_with(ACTIVE_PREFIX)
                        })
                        .unwrap();
                    fs::rename(active.path(), coordinator.path.join("original-root")).unwrap();
                    if replace {
                        fs::create_dir(active.path()).unwrap();
                        fs::write(active.path().join("replacement"), b"preserved").unwrap();
                    }
                    *captured_path.borrow_mut() = Some(active.path());
                }
                Ok(())
            },
        )
        .unwrap_err();
        if replace {
            assert!(matches!(error, WorkspaceError::InvalidPath { .. }));
            assert_eq!(
                fs::read(captured_path.borrow().as_ref().unwrap().join("replacement")).unwrap(),
                b"preserved"
            );
        } else {
            assert!(matches!(
                error,
                WorkspaceError::Io {
                    operation: "open published root",
                    ..
                }
            ));
        }
        assert!(coordinator.path.join("original-root").exists());
    }
}

#[test]
fn coordinator_initialization_write_failure_removes_its_owned_entry() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);
    fs::write(&path, b"").unwrap();
    let file = File::open(&path).unwrap();
    let identity = file_identity(&file).unwrap();
    let error = initialize_new_coordinator(&dir, file, identity, &path, deadline()).unwrap_err();
    assert!(matches!(
        error,
        WorkspaceError::Io {
            operation: "initialize coordinator",
            ..
        }
    ));
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn permission_repair_makes_cleanup_directory_owner_accessible() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o500)).unwrap();
    make_cleanup_directory_accessible(&dir, std::time::Instant::now(), Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        fs::metadata(parent.path()).unwrap().permissions().mode() & 0o700,
        0o700
    );
}

#[cfg(unix)]
fn unsupported_lock_file() -> File {
    #[cfg(target_os = "linux")]
    {
        File::from(
            rustix::fs::open(
                "/dev/null",
                rustix::fs::OFlags::PATH,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        File::from(std::os::fd::OwnedFd::from(stream))
    }
}

#[cfg(unix)]
#[test]
fn coordinator_lock_errors_do_not_retry_or_disguise_non_contention_failures() {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    coordinator.file = unsupported_lock_file();
    assert!(matches!(
        File::try_lock(&coordinator.file),
        Err(std::fs::TryLockError::Error(_))
    ));
    assert!(matches!(
        CoordinatorLockGuard::try_acquire(&coordinator),
        Err(WorkspaceError::Io {
            operation: "lock coordinator",
            ..
        })
    ));
    assert!(matches!(
        CoordinatorLockGuard::acquire_until(&coordinator, deadline()),
        Err(WorkspaceError::Io {
            operation: "lock coordinator",
            ..
        })
    ));
    assert!(matches!(
        lock_file_until(&coordinator.file, deadline(), &coordinator.path),
        Err(WorkspaceError::Io {
            operation: "lock coordinator",
            ..
        })
    ));
    // A failed attempt releases the local mutex rather than poisoning or retaining it.
    assert!(coordinator.local_lock.try_lock().is_ok());
}

#[cfg(unix)]
#[test]
fn owner_cleanup_retains_root_when_coordinator_file_cannot_be_locked() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let mut root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.coordinator_file = unsupported_lock_file();
    let record = root.cleanup(Duration::from_secs(1));
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("coordinator lock failed"))
    );
    assert!(root.path.exists());
    assert_eq!(
        NamespaceLifecycle::load(&root.namespace),
        NamespaceLifecycle::Active
    );
}

#[cfg(unix)]
#[test]
fn coordinator_creation_and_reopen_report_permission_failures() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = initialize_coordinator(&dir, path.parent().unwrap(), deadline());
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "create coordinator",
            ..
        })
    ));
    fs::write(&path, initial_coordinator_bytes()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let result = open_initialized_coordinator_until(&dir, &path, deadline());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "open coordinator",
            ..
        })
    ));
}

#[cfg(unix)]
#[test]
fn checked_cleanup_rejects_stale_file_and_directory_identities() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("file", b"preserved").unwrap();
    dir.create_dir("directory").unwrap();
    let started = std::time::Instant::now();
    let budget = Duration::from_secs(1);
    let file_error = remove_cleanup_entry_checked(
        &dir,
        std::ffi::OsStr::new("file"),
        Utf8Path::new("file"),
        (u64::MAX, u64::MAX),
        started,
        budget,
    )
    .unwrap_err();
    assert!(
        file_error
            .to_string()
            .contains("identity changed before removal")
    );
    let dir_error = remove_cleanup_directory_checked(
        &dir,
        std::ffi::OsStr::new("directory"),
        Utf8Path::new("directory"),
        (u64::MAX, u64::MAX),
        started,
        budget,
    )
    .unwrap_err();
    assert!(
        dir_error
            .to_string()
            .contains("identity changed before removal")
    );
    assert_eq!(dir.read("file").unwrap(), b"preserved");
    assert!(dir.exists("directory"));
    let error = open_cleanup_directory(&dir, std::ffi::OsStr::new("file"), None, started, budget)
        .unwrap_err();
    assert!(error.to_string().contains("not a real directory"));
    let error = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some((u64::MAX, u64::MAX)),
        started,
        budget,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("identity changed before reopening")
    );
}

#[cfg(unix)]
#[test]
fn cleanup_reopens_and_repairs_an_inaccessible_directory() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.create_dir("directory").unwrap();
    fs::set_permissions(
        parent.path().join("directory"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let child = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        None,
        std::time::Instant::now(),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        cap_std::fs::PermissionsExt::mode(&child.metadata(".").unwrap().permissions()) & 0o700,
        0o700
    );
}

#[test]
fn stale_evidence_rejects_mismatched_heartbeat_identity_and_timestamp_overflow() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let lease = LeaseMarker {
        schema: LEASE_SCHEMA,
        run_id: "run-id".into(),
        created_unix_seconds: 0,
        owner: OwnerKind::PublicExecution,
    };
    for marker in [
        LeaseMarker {
            schema: LEASE_SCHEMA + 1,
            ..lease.clone()
        },
        LeaseMarker {
            run_id: "different".into(),
            ..lease.clone()
        },
        LeaseMarker {
            owner: OwnerKind::PublicDelivery,
            ..lease.clone()
        },
    ] {
        dir.write(HEARTBEAT_FILE, serde_json::to_vec(&marker).unwrap())
            .unwrap();
        let failure = candidate_is_stale(&dir, &lease, SystemTime::now())
            .err()
            .unwrap();
        assert_eq!(failure.reason, JanitorReason::HeartbeatInvalid);
    }
    let failure = stale_timestamp(None, SystemTime::now()).err().unwrap();
    assert_eq!(failure.reason, JanitorReason::AgeUnverifiable);
    assert!(failure.cause.contains("overflow"));
}

#[test]
fn invalid_cleanup_ready_markers_retain_typed_diagnostics_and_fall_back_to_age() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let path = Utf8Path::from_path(parent.path()).unwrap();
    let marker = LeaseMarker {
        schema: LEASE_SCHEMA,
        run_id: "run-id".into(),
        created_unix_seconds: 0,
        owner: OwnerKind::PublicExecution,
    };
    dir.write(HEARTBEAT_FILE, serde_json::to_vec(&marker).unwrap())
        .unwrap();
    let lease = create_owned_file(&dir, LEASE_FILE, OwnedFileAccess::ReadWrite).unwrap();
    let mismatched = CleanupReadyMarker {
        schema: LEASE_SCHEMA,
        run_id: marker.run_id.clone(),
        owner: marker.owner,
        lease_device: u64::MAX,
        lease_inode: u64::MAX,
    };
    for bytes in [
        b"invalid json".to_vec(),
        serde_json::to_vec(&mismatched).unwrap(),
    ] {
        dir.write(CLEANUP_READY_FILE, bytes).unwrap();
        let mut ledger = ReclaimLedger::default();
        let failure =
            candidate_is_reclaimable(&dir, &marker, &lease, SystemTime::now(), &mut ledger, path)
                .err()
                .unwrap();
        assert_eq!(failure.reason, JanitorReason::Young);
        assert!(
            ledger
                .finish()
                .details
                .iter()
                .any(|detail| detail.reason == JanitorReason::MarkerInvalid)
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn staging_safety_rejects_non_utf8_entries_without_removing_them() {
    use std::os::unix::ffi::OsStrExt;
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let name = std::ffi::OsStr::from_bytes(b"non-utf8-\xff");
    dir.write(name, b"preserved").unwrap();
    let failure = staging_contents_are_safe(&dir).err().unwrap();
    assert_eq!(failure.reason, JanitorReason::UnsafeStaging);
    assert!(failure.cause.contains("non-UTF8"));
    assert_eq!(dir.read(name).unwrap(), b"preserved");
}

#[cfg(unix)]
#[test]
fn claim_distinguishes_missing_lease_absent_names_and_non_directory_names() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let mut missing_lease = None;
    let error = claim_managed_child(
        &coordinator.dir,
        "missing",
        "deleting",
        &root.run_id,
        root.owner,
        &root.dir,
        &mut missing_lease,
        &std::thread::yield_now,
        &|| Ok(()),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("managed root lease is unavailable")
    );
    let mut lease = Some(
        root.lease
            .lock()
            .unwrap()
            .as_deref()
            .unwrap()
            .try_clone()
            .unwrap(),
    );
    let result = claim_managed_child(
        &coordinator.dir,
        "missing",
        "deleting",
        &root.run_id,
        root.owner,
        &root.dir,
        &mut lease,
        &std::thread::yield_now,
        &|| Ok(()),
    )
    .unwrap();
    assert!(matches!(result, ClaimResult::Absent));
    coordinator.dir.write("file", b"preserved").unwrap();
    for (current, deleting) in [("file", "missing"), ("missing", "file")] {
        let result = claim_managed_child(
            &coordinator.dir,
            current,
            deleting,
            &root.run_id,
            root.owner,
            &root.dir,
            &mut lease,
            &std::thread::yield_now,
            &|| Ok(()),
        );
        assert!(matches!(
            result,
            Err(WorkspaceError::Io {
                operation: "open cleanup candidate",
                ..
            })
        ));
    }
    assert_eq!(coordinator.dir.read("file").unwrap(), b"preserved");
}

#[cfg(unix)]
struct FileDescriptorPressure {
    files: Vec<File>,
    previous: libc::rlimit,
}

#[cfg(unix)]
impl FileDescriptorPressure {
    fn start() -> Self {
        let mut previous = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: the kernel writes into a live rlimit value.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut previous) },
            0
        );
        let reduced = libc::rlimit {
            rlim_cur: previous.rlim_cur.min(128),
            rlim_max: previous.rlim_max,
        };
        // SAFETY: this isolated subprocess changes only its own soft descriptor limit.
        assert_eq!(
            unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &raw const reduced) },
            0
        );
        let mut files = Vec::new();
        while let Ok(file) = File::open("/dev/null") {
            files.push(file);
        }
        Self { files, previous }
    }
}

#[cfg(unix)]
impl Drop for FileDescriptorPressure {
    fn drop(&mut self) {
        self.files.clear();
        // SAFETY: restore the exact limit captured before this subprocess's test.
        let _ = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &raw const self.previous) };
    }
}

#[cfg(unix)]
#[test]
fn descriptor_exhaustion_preserves_primary_errors_and_owned_workspaces() {
    for mode in ["operations", "clone-coordinator", "clone-directory"] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "workspace::owned::coverage_tests::descriptor_exhaustion_child",
                "--ignored",
            ])
            .env("HOIMIN_DESCRIPTOR_FAILURE_MODE", mode)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "descriptor failure subprocess failed: {mode}"
        );
    }
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_exhaustion_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let mode = std::env::var("HOIMIN_DESCRIPTOR_FAILURE_MODE").unwrap();
    if mode == "operations" {
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let worker = super::super::root::WorkerRoot::open(root.path.clone()).unwrap();
        let pressure = FileDescriptorPressure::start();
        let enumeration = worker.entries();
        let cleanup = worker.clear_for_cleanup();
        let child = root.create_child("worker-");
        let retain = root.retain();
        let reopen = ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap());
        drop(pressure);
        assert!(matches!(
            enumeration,
            Err(WorkspaceError::Io {
                operation: "clone worker directory",
                ..
            })
        ));
        assert!(matches!(
            cleanup,
            Err(WorkspaceError::Io {
                operation: "clone worker root",
                ..
            })
        ));
        assert!(matches!(
            child,
            Err(WorkspaceError::Io {
                operation: "clone managed child parent",
                ..
            })
        ));
        assert!(matches!(
            retain,
            Err(WorkspaceError::Io {
                operation: "create workspace marker",
                ..
            })
        ));
        assert!(matches!(reopen, Err(WorkspaceError::Io { .. })));
        assert!(root.path.exists());
    } else {
        let boundary = if mode == "clone-coordinator" {
            PublishBoundary::ActiveOpened
        } else {
            PublishBoundary::CoordinatorOpened
        };
        let pressure = std::cell::RefCell::new(None);
        let result = ManagedRunRoot::create_with_publish_hook(
            &coordinator,
            OwnerKind::PublicExecution,
            &|observed| {
                if observed == boundary {
                    *pressure.borrow_mut() = Some(FileDescriptorPressure::start());
                }
                Ok(())
            },
        );
        drop(pressure.borrow_mut().take());
        let expected_operation = if mode == "clone-coordinator" {
            "clone coordinator"
        } else {
            "clone managed root"
        };
        assert!(
            matches!(result, Err(WorkspaceError::Io { operation, .. }) if operation == expected_operation)
        );
        let active = fs::read_dir(&coordinator.path)
            .unwrap()
            .map(Result::unwrap)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(ACTIVE_PREFIX)
            })
            .count();
        assert_eq!(
            active, 1,
            "a failed rollback leaves the published root reclaimable"
        );
    }
}

#[test]
fn cancellation_at_each_cleanup_checkpoint_preserves_exact_deletion_evidence() {
    let mut completed = false;
    let mut deferred = false;
    for stop_at in 0..160 {
        let parent = tempfile::tempdir().unwrap();
        let dir = cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority())
            .unwrap();
        dir.create_dir_all("root/inner").unwrap();
        dir.write("root/inner/payload", b"payload").unwrap();
        dir.write(format!("root/{RETAIN_FILE}"), b"marker").unwrap();
        let identity = metadata_identity(&dir.symlink_metadata("root").unwrap());
        let checks = std::cell::Cell::new(0);
        let stopped = || {
            let check = checks.get();
            checks.set(check + 1);
            check >= stop_at
        };
        let budget = CleanupBudget {
            duration: Duration::from_secs(5),
            stopped: Some(&stopped),
        };
        let mut capabilities = CleanupCapabilities::default();
        let result = remove_claimed_tree_slice_with_capabilities(
            &dir,
            "root",
            Some(identity),
            budget,
            &mut capabilities,
        );
        match result {
            Ok(slice) => {
                completed |= slice.complete;
                deferred |= !slice.complete;
            }
            Err(error) => {
                assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
                deferred = true;
            }
        }
        let remaining = [
            "root".to_owned(),
            "root/inner".to_owned(),
            "root/inner/payload".to_owned(),
            format!("root/{RETAIN_FILE}"),
        ]
        .iter()
        .filter(|path| dir.exists(path))
        .count();
        assert_eq!(
            capabilities.progress.removed_entries,
            u64::try_from(4 - remaining).unwrap(),
            "checkpoint {stop_at}"
        );
        assert_eq!(
            capabilities.progress.root_removed,
            !dir.exists("root"),
            "checkpoint {stop_at}"
        );
    }
    assert!(completed);
    assert!(deferred);
}

#[test]
fn exhausted_cleanup_budgets_preserve_root_and_report_rollback_timeout() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.create_dir("root").unwrap();
    let identity = metadata_identity(&dir.symlink_metadata("root").unwrap());
    let result = remove_claimed_tree_bounded(&dir, "root", Some(identity), Duration::ZERO).unwrap();
    assert!(!result.complete);
    assert_eq!(result.examined, 0);
    assert_eq!(result.removed, 0);
    let error = rollback_new_owned_directory(&dir, "root", identity, Duration::ZERO).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(dir.exists("root"));
}

#[cfg(unix)]
#[test]
fn claim_rejects_changed_lease_identity_and_mismatched_marker_fields() {
    for field in ["schema", "run_id", "owner", "inode"] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = coordinator_in(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let lease_path = root.path.join(LEASE_FILE);
        let mut marker: LeaseMarker =
            serde_json::from_slice(&fs::read(&lease_path).unwrap()).unwrap();
        match field {
            "schema" => marker.schema += 1,
            "run_id" => marker.run_id = "different".into(),
            "owner" => marker.owner = OwnerKind::PublicDelivery,
            _ => fs::rename(&lease_path, root.path.join("original-lease")).unwrap(),
        }
        fs::write(&lease_path, serde_json::to_vec(&marker).unwrap()).unwrap();
        let result = root.claim_with_handle_handoff(
            &coordinator.dir,
            root.path.file_name().unwrap(),
            &format!("{DELETING_PREFIX}{}", root.run_id),
            &|| Ok(()),
        );
        assert!(
            matches!(result, Err(WorkspaceError::InvalidPath { .. })),
            "field {field}"
        );
        assert!(root.path.exists());
        assert!(root.lease.lock().unwrap().is_some());
        assert_eq!(
            NamespaceLifecycle::load(&root.namespace),
            NamespaceLifecycle::Active
        );
    }
}

#[cfg(unix)]
#[test]
fn namespace_replacement_at_claim_checkpoints_never_deletes_replacement_contents() {
    let mut exercised_replacement = false;
    let mut exercised_success = false;
    for swap_at in 0..16 {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = coordinator_in(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.dir.write("payload", b"original").unwrap();
        let active_name = root.path.file_name().unwrap();
        let deleting_name = format!("{DELETING_PREFIX}{}", root.run_id);
        let original_identity = directory_identity(&root.dir).unwrap();
        let checks = std::cell::Cell::new(0);
        let swapped = std::cell::Cell::new(false);
        let before = || {
            let check = checks.get();
            checks.set(check + 1);
            if check == swap_at {
                coordinator
                    .dir
                    .rename(active_name, &coordinator.dir, "held-original")
                    .unwrap();
                coordinator.dir.create_dir(active_name).unwrap();
                fs::write(root.path.join("payload"), b"replacement").unwrap();
                swapped.set(true);
            }
            Ok(())
        };
        let result =
            root.claim_with_handle_handoff(&coordinator.dir, active_name, &deleting_name, &before);
        if swapped.get() {
            exercised_replacement = true;
            assert_eq!(
                coordinator.dir.read("held-original/payload").unwrap(),
                b"original"
            );
            let _ = remove_claimed_tree_bounded(
                &coordinator.dir,
                &deleting_name,
                Some(original_identity),
                Duration::from_secs(1),
            );
            let replacement_name = if coordinator.dir.exists(active_name) {
                active_name
            } else {
                &deleting_name
            };
            assert_eq!(
                coordinator
                    .dir
                    .read(format!("{replacement_name}/payload"))
                    .unwrap(),
                b"replacement"
            );
        } else {
            exercised_success = true;
            assert!(matches!(result, Ok(ClaimResult::Claimed)));
            assert_eq!(
                coordinator
                    .dir
                    .read(format!("{deleting_name}/payload"))
                    .unwrap(),
                b"original"
            );
        }
    }
    assert!(exercised_replacement);
    assert!(exercised_success);
}

fn reclaim_selected(coordinator: &ManagedRootCoordinator, names: &[String]) -> ReclaimReport {
    ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        coordinator,
        SystemTime::now(),
        std::time::Instant::now(),
        &|_| Duration::ZERO,
        &|_| Ok((names.to_vec(), None)),
    )
}

#[test]
fn janitor_selection_revalidates_names_and_candidate_types() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let file_name = format!("{ACTIVE_PREFIX}{}", uuid::Uuid::new_v4());
    let missing_name = format!("{ACTIVE_PREFIX}{}", uuid::Uuid::new_v4());
    coordinator.dir.write(&file_name, b"preserved").unwrap();
    let report = reclaim_selected(
        &coordinator,
        &[
            "unmanaged".into(),
            "run-invalid-uuid".into(),
            file_name.clone(),
            missing_name,
        ],
    );
    assert_eq!(report.inspected_roots, 4);
    assert_eq!(report.absent_roots, 1);
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::InvalidCandidate],
        2
    );
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::CandidateInspectionFailed],
        1
    );
    assert_eq!(coordinator.dir.read(file_name).unwrap(), b"preserved");
}

#[test]
fn janitor_retains_ready_and_empty_roots_when_coordinator_state_is_poisoned() {
    for empty in [true, false] {
        let parent = tempfile::tempdir().unwrap();
        let mut coordinator = coordinator_in(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.mark_cleanup_ready().unwrap();
        root.unlock_fixture_lease();
        let name = if empty {
            let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
            coordinator.dir.create_dir(&name).unwrap();
            name
        } else {
            root.path.file_name().unwrap().to_owned()
        };
        coordinator.local_lock = Arc::new(Mutex::new(()));
        poison(&coordinator.local_lock);
        let report = reclaim_selected(&coordinator, std::slice::from_ref(&name));
        assert_eq!(
            report.unresolved_by_reason[&JanitorReason::CoordinatorFailed],
            1
        );
        assert!(coordinator.dir.exists(name));
    }
}

#[cfg(unix)]
#[test]
fn janitor_reports_inaccessible_candidate_as_inspection_failure() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let name = format!("{ACTIVE_PREFIX}{}", uuid::Uuid::new_v4());
    coordinator.dir.create_dir(&name).unwrap();
    let path = coordinator.path.join(&name);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    let report = reclaim_selected(&coordinator, &[name]);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::CandidateInspectionFailed],
        1
    );
    assert!(path.exists());
}

#[cfg(unix)]
struct CleanupEntryCap(usize);

#[cfg(unix)]
impl CleanupEntryCap {
    fn new(entries: usize) -> Self {
        Self(TEST_CLEANUP_SLICE_ENTRIES.replace(entries))
    }
}

#[cfg(unix)]
impl Drop for CleanupEntryCap {
    fn drop(&mut self) {
        TEST_CLEANUP_SLICE_ENTRIES.set(self.0);
    }
}

#[cfg(unix)]
#[test]
fn janitor_requeues_progressing_slices_until_the_root_is_fully_removed() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    for index in 0..20 {
        root.dir
            .write(format!("payload-{index}"), b"payload")
            .unwrap();
    }
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let _cap = CleanupEntryCap::new(8);
    let report = reclaim_selected(&coordinator, &[root.path.file_name().unwrap().to_owned()]);
    assert_eq!(report.reclaimed_roots, 1);
    assert_eq!(report.removed_entries, 24);
    assert_eq!(report.unresolved_roots, 0);
    assert!(!root.path.exists());
}

#[cfg(unix)]
#[test]
fn janitor_stops_a_queue_round_when_every_slice_makes_no_progress() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let _cap = CleanupEntryCap::new(2);
    let report = reclaim_selected(&coordinator, &[root.path.file_name().unwrap().to_owned()]);
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::CleanupIncomplete],
        1
    );
    assert!(
        coordinator
            .path
            .join(format!("{DELETING_PREFIX}{}", root.run_id))
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn cleanup_ready_rejects_partial_marker_directories_and_reports_unlink_failure() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir.create_dir(CLEANUP_READY_FILE).unwrap();
    let result = root.mark_cleanup_ready();
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "open partial workspace marker",
            ..
        })
    ));
    assert!(
        root.dir
            .symlink_metadata(CLEANUP_READY_FILE)
            .unwrap()
            .is_dir()
    );
    root.dir.remove_dir(CLEANUP_READY_FILE).unwrap();
    root.dir.write(CLEANUP_READY_FILE, b"partial json").unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = root.mark_cleanup_ready();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "remove partial workspace marker",
            ..
        })
    ));
    assert_eq!(root.dir.read(CLEANUP_READY_FILE).unwrap(), b"partial json");
    assert!(!root.lifecycle.lock().unwrap().cleanup_ready);
    root.mark_cleanup_ready().unwrap();
    assert!(root.lifecycle.lock().unwrap().cleanup_ready);
}

#[test]
fn non_regular_cleanup_ready_evidence_falls_back_to_heartbeat_age() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir.create_dir(CLEANUP_READY_FILE).unwrap();
    let guard = root.lease.lock().unwrap();
    let lease = guard.as_deref().unwrap();
    let marker = read_json_file_checked::<LeaseMarker>(lease).unwrap();
    let mut ledger = ReclaimLedger::default();
    let result = candidate_is_reclaimable(
        &root.dir,
        &marker,
        lease,
        SystemTime::now(),
        &mut ledger,
        root.path(),
    );
    assert_eq!(result.err().unwrap().reason, JanitorReason::Young);
    assert!(
        ledger
            .finish()
            .details
            .iter()
            .any(|detail| detail.reason == JanitorReason::MarkerInvalid)
    );
    assert!(
        root.dir
            .symlink_metadata(CLEANUP_READY_FILE)
            .unwrap()
            .is_dir()
    );
}

#[cfg(unix)]
fn one_payload_slice(root: &ManagedRunRoot) -> (CleanupEntryCap, OsString) {
    let (index, name) = owned_directory_entries(&root.dir)
        .unwrap()
        .map(Result::unwrap)
        .enumerate()
        .find(|(_, name)| !is_root_control_marker(name))
        .unwrap();
    (CleanupEntryCap::new(index + 1), name)
}

#[cfg(unix)]
#[test]
fn namespace_replacement_between_janitor_slices_retains_prior_deletion_evidence() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    for index in 0..3 {
        root.dir
            .write(format!("payload-{index}"), b"original")
            .unwrap();
    }
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let (_cap, first_payload) = one_payload_slice(&root);
    let deleting_name = format!("{DELETING_PREFIX}{}", root.run_id);
    let replaced = std::cell::Cell::new(false);
    let elapsed = |_| {
        if !root.dir.exists(&first_payload) && !replaced.replace(true) {
            coordinator
                .dir
                .rename(&deleting_name, &coordinator.dir, "held-original")
                .unwrap();
            coordinator
                .dir
                .write(&deleting_name, b"replacement")
                .unwrap();
        }
        Duration::ZERO
    };
    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        SystemTime::now(),
        std::time::Instant::now(),
        &elapsed,
        &|_| Ok((vec![root.path.file_name().unwrap().to_owned()], None)),
    );
    assert!(replaced.get());
    assert_eq!(report.removed_entries, 1);
    assert_eq!(report.unresolved_by_reason[&JanitorReason::DeleteFailed], 1);
    assert_eq!(
        coordinator.dir.read(&deleting_name).unwrap(),
        b"replacement"
    );
    assert!(coordinator.dir.exists("held-original"));
}

#[cfg(unix)]
#[test]
fn janitor_deadline_between_queue_admission_and_dispatch_preserves_partial_progress() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    for index in 0..3 {
        root.dir
            .write(format!("payload-{index}"), b"original")
            .unwrap();
    }
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let (_cap, first_payload) = one_payload_slice(&root);
    let queue_checks = std::cell::Cell::new(0);
    let elapsed = |_| {
        if !root.dir.exists(&first_payload) {
            let checks = queue_checks.get();
            queue_checks.set(checks + 1);
            if checks != 0 {
                return JANITOR_CLEANUP_BUDGET;
            }
        }
        Duration::ZERO
    };
    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        SystemTime::now(),
        std::time::Instant::now(),
        &elapsed,
        &|_| Ok((vec![root.path.file_name().unwrap().to_owned()], None)),
    );
    assert_eq!(report.removed_entries, 1);
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::CleanupIncomplete],
        1
    );
    assert!(
        coordinator
            .dir
            .exists(format!("{DELETING_PREFIX}{}", root.run_id))
    );
}

#[cfg(target_os = "linux")]
#[test]
fn opath_cleanup_permission_repair_targets_pinned_inode_after_name_replacement() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let parent_dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    parent_dir.create_dir("original").unwrap();
    parent_dir.create_dir("outside").unwrap();
    let pinned = cap_std::fs::Dir::from_std_file(File::from(
        rustix::fs::openat(
            &parent_dir,
            "original",
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )
        .unwrap(),
    ));
    assert!(
        rustix::fs::fcntl_getfl(&pinned)
            .unwrap()
            .contains(rustix::fs::OFlags::PATH)
    );
    parent_dir.rename("original", &parent_dir, "held").unwrap();
    fs::set_permissions(
        parent.path().join("held"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    fs::set_permissions(
        parent.path().join("outside"),
        fs::Permissions::from_mode(0o500),
    )
    .unwrap();
    std::os::unix::fs::symlink("outside", parent.path().join("original")).unwrap();
    let result = make_cleanup_directory_accessible(
        &pinned,
        std::time::Instant::now(),
        Duration::from_secs(1),
    );
    let repaired_mode = pinned
        .try_clone()
        .unwrap()
        .into_std_file()
        .metadata()
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let outside_mode = fs::metadata(parent.path().join("outside"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    fs::set_permissions(
        parent.path().join("held"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    result.unwrap();
    assert_eq!(repaired_mode, 0o700);
    assert_eq!(outside_mode, 0o500);
    assert!(
        parent_dir
            .symlink_metadata("original")
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
