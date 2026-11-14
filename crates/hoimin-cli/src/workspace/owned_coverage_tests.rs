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

#[rstest::rstest]
#[case::try_acquire(|coordinator: &ManagedRootCoordinator| CoordinatorLockGuard::try_acquire(coordinator).map(drop))]
#[case::acquire_until(|coordinator: &ManagedRootCoordinator| CoordinatorLockGuard::acquire_until(coordinator, deadline()).map(drop))]
#[case::local_until(|coordinator: &ManagedRootCoordinator| lock_local_until(&coordinator.local_lock, deadline()).map(drop))]
fn poisoned_coordinator_locks_fail_without_recovering_unverified_state(
    #[case] action: fn(&ManagedRootCoordinator) -> Result<(), WorkspaceError>,
) {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    coordinator.local_lock = Arc::new(Mutex::new(()));
    poison(&coordinator.local_lock);

    let result = action(&coordinator);

    assert!(matches!(result, Err(WorkspaceError::StatePoisoned)));
}

#[test]
fn poisoned_registry_lock_does_not_recover_unverified_state() {
    let registry = Mutex::new(HashMap::new());
    poison(&registry);
    let path = Utf8Path::new("coordinator");

    let result = lock_registry_until(&registry, path, deadline());

    assert!(matches!(result, Err(WorkspaceError::StatePoisoned)));
}

#[rstest::rstest]
#[case::validate(validate_coordinator_file)]
#[case::read(|file: &mut File, path: &Utf8Path| read_coordinator_state(file, path).map(drop))]
fn conflicting_equal_generation_slots_are_rejected_without_rewriting(
    #[case] action: fn(&mut File, &Utf8Path) -> Result<(), WorkspaceError>,
) {
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join("coordinator");
    let mut bytes = initial_coordinator_bytes();
    let cursor = "run-00000000-0000-0000-0000-000000000001";
    bytes[513..].copy_from_slice(&coordinator_slot_bytes(0, cursor).unwrap());
    fs::write(&path, bytes).unwrap();
    let mut file = File::open(&path).unwrap();

    let result = action(&mut file, &path);

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[rstest::rstest]
#[case::second_slot_corrupt(|bytes: &mut [u8]| bytes[513] ^= 1, &Ok((0, CoordinatorSlot { generation: 0, cursor: String::new() })))]
#[case::both_slots_corrupt(|bytes: &mut [u8]| { bytes[513] ^= 1; bytes[1] ^= 1; }, &Err(()))]
fn coordinator_recovers_first_slot_when_second_is_corrupt_and_rejects_both_corrupt(
    #[case] corrupt: fn(&mut [u8]),
    #[case] expected: &Result<(usize, CoordinatorSlot), ()>,
) {
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join("coordinator");
    let mut bytes = initial_coordinator_bytes();
    corrupt(&mut bytes);
    fs::write(&path, bytes).unwrap();
    let file = File::open(&path).unwrap();

    let result = read_coordinator_state(&file, &path);
    let invalid_path = result
        .as_ref()
        .err()
        .is_some_and(|error| matches!(error, WorkspaceError::InvalidPath { .. }));

    assert_eq!(invalid_path, expected.is_err());
    assert_eq!(&result.map_err(|_| ()), expected);
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

#[rstest::rstest]
#[case(b"invalid".as_slice())]
#[case(b"\xff".as_slice())]
fn authenticated_slots_require_utf8_and_canonical_managed_names(#[case] cursor: &[u8]) {
    let mut slot = initial_coordinator_slot();
    slot[20..22].copy_from_slice(&u16::try_from(cursor.len()).unwrap().to_le_bytes());
    slot[22..22 + cursor.len()].copy_from_slice(cursor);
    let digest = blake3::hash(&slot[..COORDINATOR_DIGEST_OFFSET]);
    slot[COORDINATOR_DIGEST_OFFSET..].copy_from_slice(digest.as_bytes());

    let observed = decode_coordinator_slot(&slot);

    assert_eq!(observed, None);
}

#[rstest::rstest]
#[case("invalid")]
#[case(&"x".repeat(COORDINATOR_CURSOR_BYTES + 1))]
fn cursor_encoding_rejects_noncanonical_or_oversized_names(#[case] cursor: &str) {
    let result = coordinator_slot_bytes(0, cursor);

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
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

#[rstest::rstest]
#[case::funded(Duration::from_secs(1), true, "primary")]
#[case::expired(Duration::ZERO, false, "deadline exhausted")]
fn expired_rollback_reports_timeout_without_running_cleanup(
    #[case] remaining: Duration,
    #[case] expected_cleanup: bool,
    #[case] expected_message: &str,
) {
    let now = std::time::Instant::now();
    let cleanup_called = std::cell::Cell::new(false);
    let cleanup = |_| {
        cleanup_called.set(true);
        Ok(())
    };
    let append = |primary: std::io::Error, secondary: std::io::Error| {
        created_directory_rollback_error(&primary, &secondary)
    };

    let error = error_after_new_directory_rollback_with(
        std::io::Error::other("primary"),
        now + remaining,
        || now,
        cleanup,
        append,
    );

    assert_eq!(cleanup_called.get(), expected_cleanup);
    assert!(error.to_string().contains(expected_message));
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

    let message = error.to_string();

    assert!(message.contains("managed-root"));
    assert!(message.contains("secondary new-directory rollback failure: cleanup unavailable"));
    assert!(
        matches!(&error, WorkspaceError::Io { operation: "construct managed directory", path: actual, .. } if actual == path)
    );
}

#[rstest::rstest]
#[case::file_metadata(|dir: &cap_std::fs::Dir| dir.write("created", b"payload").unwrap(), "changed before it could be opened")]
#[case::different_directory(|dir: &cap_std::fs::Dir| dir.create_dir("created").unwrap(), "identity changed while opening")]
fn newly_created_directory_must_match_both_metadata_and_opened_identity(
    #[case] prepare: fn(&cap_std::fs::Dir),
    #[case] expected_message: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    prepare(&dir);
    dir.create_dir("different").unwrap();
    let rollback_identity = std::cell::Cell::new(None);
    let expected_identity = metadata_identity(&dir.symlink_metadata("created").unwrap());

    let error = create_owned_directory_with(
        &dir,
        "created",
        |_, _| Ok(()),
        |parent, name| parent.symlink_metadata(name),
        |parent, _| parent.open_dir("different"),
        |_, _, identity, _| {
            rollback_identity.set(Some(identity));
            Ok(())
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains(expected_message));
    assert_eq!(rollback_identity.get(), Some(expected_identity));
    assert!(dir.exists("different"));
}

#[rstest::rstest]
#[case::layout_tag(|dir: &cap_std::fs::Dir, path: &Utf8Path| { let mut bytes = initial_coordinator_bytes(); bytes[0] = 1; dir.write(COORDINATOR_FILE, bytes).unwrap(); let mut file = File::open(path).unwrap(); validate_coordinator_file(&mut file, path) })]
#[case::directory_entry(|dir: &cap_std::fs::Dir, path: &Utf8Path| { dir.create_dir(COORDINATOR_FILE).unwrap(); open_initialized_coordinator_until(dir, path, deadline()).map(drop) })]
fn coordinator_rejects_nonzero_layout_tag_and_directory_entries(
    #[case] action: fn(&cap_std::fs::Dir, &Utf8Path) -> Result<(), WorkspaceError>,
) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);

    let result = action(&dir, &path);

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
}

fn coordinator_in(parent: &tempfile::TempDir) -> ManagedRootCoordinator {
    ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap()
}

#[rstest::rstest]
#[case::child_lifecycle(|root: &ManagedRunRoot| poison(&root.lifecycle), |root: &ManagedRunRoot| root.create_child("worker-").map(drop))]
#[case::ready_lifecycle(|root: &ManagedRunRoot| poison(&root.lifecycle), ManagedRunRoot::mark_cleanup_ready)]
#[case::heartbeat(|root: &ManagedRunRoot| poison(&root.heartbeat), ManagedRunRoot::refresh_heartbeat)]
#[case::ready_lease(|root: &ManagedRunRoot| poison(&root.lease), ManagedRunRoot::mark_cleanup_ready)]
#[case::cleanup_capability(|root: &ManagedRunRoot| poison(&root.lease), |root: &ManagedRunRoot| root.take_owner_cleanup_capabilities().map(drop))]
fn root_operations_fail_closed_when_lifecycle_and_marker_locks_are_poisoned(
    #[case] poison_lock: fn(&ManagedRunRoot),
    #[case] action: fn(&ManagedRunRoot) -> Result<(), WorkspaceError>,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    poison_lock(&root);

    let result = action(&root);

    assert!(matches!(result, Err(WorkspaceError::StatePoisoned)));
    assert!(root.path.exists());
}

#[test]
fn owner_cleanup_reports_a_poisoned_lifecycle_without_removing_the_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    poison(&root.lifecycle);

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
}

#[rstest::rstest]
#[case::heartbeat(|root: &ManagedRunRoot| drop(root.heartbeat.lock().unwrap().take()), ManagedRunRoot::refresh_heartbeat, "heartbeat is unavailable")]
#[case::lease(|root: &ManagedRunRoot| drop(root.lease.lock().unwrap().take()), ManagedRunRoot::mark_cleanup_ready, "lease is unavailable")]
fn unavailable_markers_report_the_missing_capability(
    #[case] remove: fn(&ManagedRunRoot),
    #[case] action: fn(&ManagedRunRoot) -> Result<(), WorkspaceError>,
    #[case] expected_message: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery).unwrap();
    remove(&root);

    let error = action(&root).unwrap_err();

    assert!(error.to_string().contains(expected_message));
    assert!(root.path.exists());
}

#[test]
fn owner_cleanup_reports_an_unavailable_lease_without_removing_the_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery).unwrap();
    drop(root.lease.lock().unwrap().take());

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

    let capability_was_preserved = root.lease.lock().unwrap().is_some();
    let initial_diagnostic_matched = root
        .take_owner_cleanup_capabilities()
        .err()
        .unwrap()
        .to_string()
        .contains("shared lease guard");
    let capability_was_preserved_2 = root.lease.lock().unwrap().is_some();
    drop(extra_guard);
    let cleanup_disposition = root.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("shared lease guard"))
    );
    assert!(capability_was_preserved);
    assert!(initial_diagnostic_matched);
    assert!(capability_was_preserved_2);
}

#[test]
fn poisoned_coordinator_prevents_owner_cleanup_without_renaming_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let mut root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.coordinator_local_lock = Arc::new(Mutex::new(()));
    poison(&root.coordinator_local_lock);

    let record = root.cleanup(Duration::from_secs(1));

    let path_existed_before_retry = root.path.exists();
    let observed = NamespaceLifecycle::load(&root.namespace);

    assert_eq!(observed, NamespaceLifecycle::Active);
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("process lock is poisoned"))
    );
    assert!(path_existed_before_retry);
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

#[rstest::rstest]
#[case(LEASE_FILE)]
#[case(HEARTBEAT_FILE)]
fn publication_marker_collisions_roll_back_the_staging_directory(#[case] marker_name: &str) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);

    let error = ManagedRunRoot::create_with_publish_hook(
        &coordinator,
        OwnerKind::PublicExecution,
        &|boundary| {
            install_publication_marker_collision(&coordinator, boundary, marker_name);
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

#[cfg(unix)]
#[rstest::rstest]
#[case::absent(|_: &std::path::Path| {}, |error: &WorkspaceError| matches!(error, WorkspaceError::Io { operation: "open published root", .. }), None)]
#[case::replacement(|path: &std::path::Path| { fs::create_dir(path).unwrap(); fs::write(path.join("replacement"), b"preserved").unwrap(); }, |error: &WorkspaceError| matches!(error, WorkspaceError::InvalidPath { .. }), Some(b"preserved".as_slice()))]
fn publication_detects_an_absent_or_replaced_active_directory(
    #[case] replace: fn(&std::path::Path),
    #[case] expected_error: fn(&WorkspaceError) -> bool,
    #[case] expected_payload: Option<&[u8]>,
) {
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
                replace(&active.path());
                *captured_path.borrow_mut() = Some(active.path());
            }
            Ok(())
        },
    )
    .unwrap_err();
    let payload = fs::read(captured_path.borrow().as_ref().unwrap().join("replacement")).ok();

    assert!(expected_error(&error), "{error}");
    assert_eq!(payload.as_deref(), expected_payload);
    assert!(coordinator.path.join("original-root").exists());
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
#[rstest::rstest]
#[case::try_acquire(|coordinator: &ManagedRootCoordinator| CoordinatorLockGuard::try_acquire(coordinator).map(drop))]
#[case::acquire_until(|coordinator: &ManagedRootCoordinator| CoordinatorLockGuard::acquire_until(coordinator, deadline()).map(drop))]
#[case::file_until(|coordinator: &ManagedRootCoordinator| lock_file_until(&coordinator.file, deadline(), &coordinator.path))]
fn coordinator_lock_errors_do_not_retry_or_disguise_non_contention_failures(
    #[case] action: fn(&ManagedRootCoordinator) -> Result<(), WorkspaceError>,
) {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    coordinator.file = unsupported_lock_file();
    let initial_result_matched = matches!(
        File::try_lock(&coordinator.file),
        Err(std::fs::TryLockError::Error(_))
    );

    let result = action(&coordinator);

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "lock coordinator",
            ..
        })
    ));
    assert!(coordinator.local_lock.try_lock().is_ok());
    assert!(initial_result_matched);
}

#[cfg(unix)]
#[test]
fn owner_cleanup_retains_root_when_coordinator_file_cannot_be_locked() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let mut root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.coordinator_file = unsupported_lock_file();

    let record = root.cleanup(Duration::from_secs(1));

    let initial_diagnostic_matched = record
        .evidence
        .details
        .iter()
        .any(|detail| detail.contains("coordinator lock failed"));
    let path_existed_before_retry = root.path.exists();
    let observed = NamespaceLifecycle::load(&root.namespace);

    assert_eq!(observed, NamespaceLifecycle::Active);
    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert!(initial_diagnostic_matched);
    assert!(path_existed_before_retry);
}

#[cfg(unix)]
#[rstest::rstest]
#[case::create(|_: &cap_std::fs::Dir| {}, |path: &Utf8Path| path.parent().unwrap().to_owned(), 0o500, 0o700, |dir: &cap_std::fs::Dir, path: &Utf8Path| initialize_coordinator(dir, path.parent().unwrap(), deadline()).map(drop), "create coordinator")]
#[case::reopen(|dir: &cap_std::fs::Dir| dir.write(COORDINATOR_FILE, initial_coordinator_bytes()).unwrap(), Utf8Path::to_owned, 0o400, 0o600, |dir: &cap_std::fs::Dir, path: &Utf8Path| open_initialized_coordinator_until(dir, path, deadline()).map(drop), "open coordinator")]
fn coordinator_creation_and_reopen_report_permission_failures(
    #[case] prepare: fn(&cap_std::fs::Dir),
    #[case] target: fn(&Utf8Path) -> Utf8PathBuf,
    #[case] restricted_mode: u32,
    #[case] restored_mode: u32,
    #[case] action: fn(&cap_std::fs::Dir, &Utf8Path) -> Result<(), WorkspaceError>,
    #[case] expected_operation: &str,
) {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    prepare(&dir);
    let target = target(&path);
    fs::set_permissions(&target, fs::Permissions::from_mode(restricted_mode)).unwrap();

    let result = action(&dir, &path);
    fs::set_permissions(&target, fs::Permissions::from_mode(restored_mode)).unwrap();

    assert!(
        matches!(result, Err(WorkspaceError::Io { operation, .. }) if operation == expected_operation)
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case::file_removal(|dir: &cap_std::fs::Dir| dir.write("target", b"preserved").unwrap(), |dir: &cap_std::fs::Dir| remove_cleanup_entry_checked(dir, std::ffi::OsStr::new("target"), Utf8Path::new("target"), (u64::MAX, u64::MAX), std::time::Instant::now(), Duration::from_secs(1)), "identity changed before removal", Some(b"preserved".as_slice()))]
#[case::directory_removal(|dir: &cap_std::fs::Dir| dir.create_dir("target").unwrap(), |dir: &cap_std::fs::Dir| remove_cleanup_directory_checked(dir, std::ffi::OsStr::new("target"), Utf8Path::new("target"), (u64::MAX, u64::MAX), std::time::Instant::now(), Duration::from_secs(1)), "identity changed before removal", None)]
#[case::file_reopen(|dir: &cap_std::fs::Dir| dir.write("target", b"preserved").unwrap(), |dir: &cap_std::fs::Dir| open_cleanup_directory(dir, std::ffi::OsStr::new("target"), None, std::time::Instant::now(), Duration::from_secs(1)).map(drop), "not a real directory", Some(b"preserved".as_slice()))]
#[case::directory_reopen(|dir: &cap_std::fs::Dir| dir.create_dir("target").unwrap(), |dir: &cap_std::fs::Dir| open_cleanup_directory(dir, std::ffi::OsStr::new("target"), Some((u64::MAX, u64::MAX)), std::time::Instant::now(), Duration::from_secs(1)).map(drop), "identity changed before reopening", None)]
fn checked_cleanup_rejects_stale_file_and_directory_identities(
    #[case] prepare: fn(&cap_std::fs::Dir),
    #[case] action: fn(&cap_std::fs::Dir) -> std::io::Result<()>,
    #[case] expected_message: &str,
    #[case] expected_payload: Option<&[u8]>,
) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    prepare(&dir);

    let error = action(&dir).unwrap_err();

    assert!(error.to_string().contains(expected_message));
    assert!(dir.exists("target"));
    assert_eq!(dir.read("target").ok().as_deref(), expected_payload);
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

#[rstest::rstest]
#[case(|marker: &mut LeaseMarker| marker.schema += 1)]
#[case(|marker: &mut LeaseMarker| marker.run_id = "different".into())]
#[case(|marker: &mut LeaseMarker| marker.owner = OwnerKind::PublicDelivery)]
fn stale_evidence_rejects_mismatched_heartbeat_identity(#[case] configure: fn(&mut LeaseMarker)) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let lease = LeaseMarker {
        schema: LEASE_SCHEMA,
        run_id: "run-id".into(),
        created_unix_seconds: 0,
        owner: OwnerKind::PublicExecution,
    };
    let mut marker = lease.clone();
    configure(&mut marker);

    dir.write(HEARTBEAT_FILE, serde_json::to_vec(&marker).unwrap())
        .unwrap();

    let failure = candidate_is_stale(&dir, &lease, SystemTime::now())
        .err()
        .unwrap();

    assert_eq!(failure.reason, JanitorReason::HeartbeatInvalid);
}

#[test]
fn stale_timestamp_overflow_is_unverifiable() {
    let failure = stale_timestamp(None, SystemTime::now()).err().unwrap();

    assert_eq!(failure.reason, JanitorReason::AgeUnverifiable);
    assert!(failure.cause.contains("overflow"));
}

#[rstest::rstest]
#[case(|_: &CleanupReadyMarker| b"invalid json".to_vec())]
#[case(|marker: &CleanupReadyMarker| serde_json::to_vec(marker).unwrap())]
fn invalid_cleanup_ready_markers_retain_typed_diagnostics_and_fall_back_to_age(
    #[case] encode: fn(&CleanupReadyMarker) -> Vec<u8>,
) {
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

    dir.write(CLEANUP_READY_FILE, encode(&mismatched)).unwrap();
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

#[cfg(unix)]
#[test]
fn staging_entry_validation_rejects_non_utf8_names() {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::OsStr::from_bytes(b"non-utf8-\xff");

    let failure = validate_staging_entry(name).err().unwrap();

    assert_eq!(failure.reason, JanitorReason::UnsafeStaging);
    assert_eq!(failure.cause, "non-UTF8 staging entry");
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
fn clone_fixture_lease(root: &ManagedRunRoot) -> File {
    root.lease
        .lock()
        .unwrap()
        .as_deref()
        .unwrap()
        .try_clone()
        .unwrap()
}

#[cfg(unix)]
#[rstest::rstest]
#[case::missing_lease("missing", "deleting", |_: &ManagedRunRoot| None, |result: &Result<ClaimResult, WorkspaceError>| result.as_ref().err().is_some_and(|error| error.to_string().contains("managed root lease is unavailable")))]
#[case::absent_names("missing", "deleting", |root: &ManagedRunRoot| Some(clone_fixture_lease(root)), |result: &Result<ClaimResult, WorkspaceError>| matches!(result, Ok(ClaimResult::Absent)))]
#[case::current_file("file", "missing", |root: &ManagedRunRoot| Some(clone_fixture_lease(root)), |result: &Result<ClaimResult, WorkspaceError>| matches!(result, Err(WorkspaceError::Io { operation: "open cleanup candidate", .. })))]
#[case::deleting_file("missing", "file", |root: &ManagedRunRoot| Some(clone_fixture_lease(root)), |result: &Result<ClaimResult, WorkspaceError>| matches!(result, Err(WorkspaceError::Io { operation: "open cleanup candidate", .. })))]
fn claim_distinguishes_missing_lease_absent_names_and_non_directory_names(
    #[case] current: &str,
    #[case] deleting: &str,
    #[case] lease: fn(&ManagedRunRoot) -> Option<File>,
    #[case] expected: fn(&Result<ClaimResult, WorkspaceError>) -> bool,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    coordinator.dir.write("file", b"preserved").unwrap();
    let mut lease = lease(&root);

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

    assert!(expected(&result));
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
#[rstest::rstest]
#[case("descriptor_enumeration_child")]
#[case("descriptor_cleanup_child")]
#[case("descriptor_admission_child")]
#[case("descriptor_retention_child")]
#[case("descriptor_reopen_child")]
#[case("descriptor_clone_coordinator_child")]
#[case("descriptor_clone_directory_child")]
fn descriptor_exhaustion_preserves_primary_errors_and_owned_workspaces(#[case] child: &str) {
    let test = format!("workspace::owned::coverage_tests::{child}");

    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test, "--ignored"])
        .status()
        .unwrap();

    assert!(
        status.success(),
        "descriptor failure subprocess failed: {child}"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_enumeration_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let worker = super::super::root::WorkerRoot::open(root.path.clone()).unwrap();
    let pressure = FileDescriptorPressure::start();

    let result = worker.entries();
    drop(pressure);

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "clone worker directory",
            ..
        })
    ));
    assert!(root.path.exists());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_cleanup_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let worker = super::super::root::WorkerRoot::open(root.path.clone()).unwrap();
    let pressure = FileDescriptorPressure::start();

    let result = worker.clear_for_cleanup();
    drop(pressure);

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "clone worker root",
            ..
        })
    ));
    assert!(root.path.exists());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_admission_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let pressure = FileDescriptorPressure::start();

    let result = root.create_child("worker-");
    drop(pressure);

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "clone managed child parent",
            ..
        })
    ));
    assert!(root.path.exists());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_retention_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let pressure = FileDescriptorPressure::start();

    let result = root.retain();
    drop(pressure);

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "create workspace marker",
            ..
        })
    ));
    assert!(root.path.exists());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_reopen_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let pressure = FileDescriptorPressure::start();

    let result = ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap());
    drop(pressure);

    assert!(matches!(result, Err(WorkspaceError::Io { .. })));
    assert!(root.path.exists());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_clone_coordinator_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let pressure = std::cell::RefCell::new(None);

    let result = ManagedRunRoot::create_with_publish_hook(
        &coordinator,
        OwnerKind::PublicExecution,
        &|observed| {
            if observed == PublishBoundary::ActiveOpened {
                *pressure.borrow_mut() = Some(FileDescriptorPressure::start());
            }
            Ok(())
        },
    );
    drop(pressure.borrow_mut().take());
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

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "clone coordinator",
            ..
        })
    ));
    assert_eq!(
        active, 1,
        "a failed rollback leaves the published root reclaimable"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "isolated descriptor exhaustion fixture; invoked by parent test"]
fn descriptor_clone_directory_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let pressure = std::cell::RefCell::new(None);

    let result = ManagedRunRoot::create_with_publish_hook(
        &coordinator,
        OwnerKind::PublicExecution,
        &|observed| {
            if observed == PublishBoundary::CoordinatorOpened {
                *pressure.borrow_mut() = Some(FileDescriptorPressure::start());
            }
            Ok(())
        },
    );
    drop(pressure.borrow_mut().take());
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

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "clone managed root",
            ..
        })
    ));
    assert_eq!(
        active, 1,
        "a failed rollback leaves the published root reclaimable"
    );
}

struct CleanupCheckpointObservation {
    result: std::io::Result<RemovalSlice>,
    progress: DeletionProgress,
    remaining: usize,
    root_exists: bool,
}

fn cleanup_at_checkpoint(stop_at: usize) -> CleanupCheckpointObservation {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
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
    let remaining = [
        "root".to_owned(),
        "root/inner".to_owned(),
        "root/inner/payload".to_owned(),
        format!("root/{RETAIN_FILE}"),
    ]
    .iter()
    .filter(|path| dir.exists(path))
    .count();

    CleanupCheckpointObservation {
        result,
        progress: capabilities.progress,
        remaining,
        root_exists: dir.exists("root"),
    }
}

#[rstest::rstest]
#[case::immediately_cancelled(0, false)]
#[case::fully_completed(159, true)]
fn cleanup_checkpoint_boundaries_exercise_cancellation_and_completion(
    #[case] stop_at: usize,
    #[case] expected_complete: bool,
) {
    let observation = cleanup_at_checkpoint(stop_at);

    assert_eq!(
        observation
            .result
            .as_ref()
            .is_ok_and(|slice| slice.complete),
        expected_complete
    );
    assert_eq!(
        observation.progress.removed_entries,
        u64::try_from(4 - observation.remaining).unwrap()
    );
    assert_eq!(observation.progress.root_removed, !observation.root_exists);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 160, failure_persistence: None, ..proptest::test_runner::Config::default() })]

    #[test]
    fn cancellation_at_each_cleanup_checkpoint_preserves_exact_deletion_evidence(stop_at in 0usize..160) {
        let observation = cleanup_at_checkpoint(stop_at);

        proptest::prop_assert!(observation.result.as_ref().err().is_none_or(|error| error.kind() == std::io::ErrorKind::TimedOut));
        proptest::prop_assert_eq!(observation.progress.removed_entries, u64::try_from(4 - observation.remaining).unwrap());
        proptest::prop_assert_eq!(observation.progress.root_removed, !observation.root_exists);
    }
}

#[test]
fn exhausted_cleanup_budget_preserves_root_without_examining_entries() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.create_dir("root").unwrap();
    let identity = metadata_identity(&dir.symlink_metadata("root").unwrap());

    let result = remove_claimed_tree_bounded(&dir, "root", Some(identity), Duration::ZERO).unwrap();

    assert!(!result.complete);
    assert_eq!(result.examined, 0);
    assert_eq!(result.removed, 0);
    assert!(dir.exists("root"));
}

#[test]
fn exhausted_rollback_budget_preserves_root_and_reports_timeout() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.create_dir("root").unwrap();
    let identity = metadata_identity(&dir.symlink_metadata("root").unwrap());

    let error = rollback_new_owned_directory(&dir, "root", identity, Duration::ZERO).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(dir.exists("root"));
}

#[cfg(unix)]
#[rstest::rstest]
#[case::schema(|_: &ManagedRunRoot, marker: &mut LeaseMarker| marker.schema += 1)]
#[case::run_id(|_: &ManagedRunRoot, marker: &mut LeaseMarker| marker.run_id = "different".into())]
#[case::owner(|_: &ManagedRunRoot, marker: &mut LeaseMarker| marker.owner = OwnerKind::PublicDelivery)]
#[case::inode(|root: &ManagedRunRoot, _: &mut LeaseMarker| fs::rename(root.path.join(LEASE_FILE), root.path.join("original-lease")).unwrap())]
fn claim_rejects_changed_lease_identity_and_mismatched_marker_fields(
    #[case] configure: fn(&ManagedRunRoot, &mut LeaseMarker),
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let lease_path = root.path.join(LEASE_FILE);
    let mut marker: LeaseMarker = serde_json::from_slice(&fs::read(&lease_path).unwrap()).unwrap();
    configure(&root, &mut marker);
    fs::write(&lease_path, serde_json::to_vec(&marker).unwrap()).unwrap();

    let result = root.claim_with_handle_handoff(
        &coordinator.dir,
        root.path.file_name().unwrap(),
        &format!("{DELETING_PREFIX}{}", root.run_id),
        &|| Ok(()),
    );

    let path_existed_before_retry = root.path.exists();
    let capability_was_preserved = root.lease.lock().unwrap().is_some();
    let observed = NamespaceLifecycle::load(&root.namespace);

    assert_eq!(observed, NamespaceLifecycle::Active);
    assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
    assert!(path_existed_before_retry);
    assert!(capability_was_preserved);
}

#[cfg(unix)]
struct ClaimCheckpointObservation {
    swapped: bool,
    claimed: bool,
    original_payload: Option<Vec<u8>>,
    payload_after_cleanup: Option<Vec<u8>>,
}

#[cfg(unix)]
fn claim_at_checkpoint(swap_at: usize) -> ClaimCheckpointObservation {
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
    let original_payload = coordinator
        .dir
        .read("held-original/payload")
        .or_else(|_| coordinator.dir.read(format!("{deleting_name}/payload")))
        .ok();
    let _cleanup = remove_claimed_tree_bounded(
        &coordinator.dir,
        &deleting_name,
        Some(original_identity),
        Duration::from_secs(1),
    );
    let payload_after_cleanup = coordinator
        .dir
        .read(format!("{active_name}/payload"))
        .or_else(|_| coordinator.dir.read(format!("{deleting_name}/payload")))
        .ok();

    ClaimCheckpointObservation {
        swapped: swapped.get(),
        claimed: matches!(result, Ok(ClaimResult::Claimed)),
        original_payload,
        payload_after_cleanup,
    }
}

#[cfg(unix)]
#[rstest::rstest]
#[case::replaced_before_inspection(0, true)]
#[case::claim_finishes_before_replacement(15, false)]
fn claim_checkpoint_boundaries_exercise_replacement_and_success(
    #[case] swap_at: usize,
    #[case] expected_swap: bool,
) {
    let observation = claim_at_checkpoint(swap_at);

    assert_eq!(observation.swapped, expected_swap);
    assert!(observation.swapped || observation.claimed);
    assert_eq!(
        observation.original_payload.as_deref(),
        Some(b"original".as_slice())
    );
    assert_eq!(
        observation.payload_after_cleanup,
        expected_swap.then(|| b"replacement".to_vec())
    );
}

#[cfg(unix)]
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 64, failure_persistence: None, ..proptest::test_runner::Config::default() })]

    #[test]
    fn namespace_replacement_at_claim_checkpoints_never_deletes_replacement_contents(swap_at in 0usize..16) {
        let observation = claim_at_checkpoint(swap_at);

        proptest::prop_assert!(observation.swapped || observation.claimed);
        proptest::prop_assert_eq!(observation.original_payload.as_deref(), Some(b"original".as_slice()));
        proptest::prop_assert_eq!(observation.payload_after_cleanup, observation.swapped.then(|| b"replacement".to_vec()));
    }
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

#[rstest::rstest]
#[case::empty(|coordinator: &ManagedRootCoordinator, _: &ManagedRunRoot| { let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4()); coordinator.dir.create_dir(&name).unwrap(); name })]
#[case::ready(|_: &ManagedRootCoordinator, root: &ManagedRunRoot| root.path.file_name().unwrap().to_owned())]
fn janitor_retains_ready_and_empty_roots_when_coordinator_state_is_poisoned(
    #[case] candidate: fn(&ManagedRootCoordinator, &ManagedRunRoot) -> String,
) {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let name = candidate(&coordinator, &root);
    coordinator.local_lock = Arc::new(Mutex::new(()));
    poison(&coordinator.local_lock);

    let report = reclaim_selected(&coordinator, std::slice::from_ref(&name));

    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::CoordinatorFailed],
        1
    );
    assert!(coordinator.dir.exists(name));
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
fn cleanup_ready_rejects_a_partial_marker_directory_without_removing_it() {
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
    assert!(!root.lifecycle.lock().unwrap().cleanup_ready);
}

#[cfg(unix)]
#[test]
fn cleanup_ready_unlink_failure_keeps_the_partial_marker_until_retry() {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir.write(CLEANUP_READY_FILE, b"partial json").unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500)).unwrap();

    let result = root.mark_cleanup_ready();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let partial_bytes = root.dir.read(CLEANUP_READY_FILE).unwrap();
    let cleanup_ready_after_failure = root.lifecycle.lock().unwrap().cleanup_ready;
    root.mark_cleanup_ready().unwrap();

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "remove partial workspace marker",
            ..
        })
    ));
    assert_eq!(partial_bytes, b"partial json");
    assert!(!cleanup_ready_after_failure);
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
    let initial_diagnostic_matched = rustix::fs::fcntl_getfl(&pinned)
        .unwrap()
        .contains(rustix::fs::OFlags::PATH);
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
    assert!(initial_diagnostic_matched);
}

#[test]
fn expired_coordinator_acquisition_leaves_both_locks_available() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);

    let result = CoordinatorLockGuard::acquire_until(&coordinator, std::time::Instant::now());

    let failure_was_observed = coordinator.local_lock.try_lock().is_ok();
    let failure_was_observed_2 = File::try_lock(&coordinator.file).is_ok();
    File::unlock(&coordinator.file).unwrap();

    assert!(
        matches!(result, Err(WorkspaceError::Io { operation: "lock coordinator", message, .. })
        if message.contains("deadline"))
    );
    assert!(failure_was_observed);
    assert!(failure_was_observed_2);
}

#[test]
fn immediate_coordinator_acquisition_preserves_a_poisoned_lock_error() {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    coordinator.local_lock = Arc::new(Mutex::new(()));
    poison(&coordinator.local_lock);

    let result = CoordinatorLockGuard::acquire(&coordinator);

    assert!(matches!(result, Err(WorkspaceError::StatePoisoned)));
}

#[test]
fn a_fresh_process_reopens_the_existing_coordinator_without_rewriting_it() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let before = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "workspace::owned::tests::fresh_process_bootstrap_child",
            "--ignored",
        ])
        .env("HOIMIN_BOOTSTRAP_TEST_PARENT", parent.path())
        .env_remove("HOIMIN_BOOTSTRAP_TEST_BARRIER")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), before);
}

#[rstest::rstest]
#[case(0, NamespaceLifecycle::Active)]
#[case(1, NamespaceLifecycle::Deleting)]
#[case(2, NamespaceLifecycle::Complete)]
fn namespace_lifecycle_loads_each_persisted_state(
    #[case] raw: u8,
    #[case] expected: NamespaceLifecycle,
) {
    let observed = NamespaceLifecycle::load(&AtomicU8::new(raw));

    assert_eq!(observed, expected);
}

#[test]
fn cleanup_evidence_caps_details_and_counts_each_omitted_entry() {
    let mut evidence = CleanupEvidence {
        details: vec!["kept".into(); MAX_DIAGNOSTIC_DETAILS],
        omitted_detail_count: 7,
        ..CleanupEvidence::default()
    };
    evidence.push_detail("must be omitted".into());

    assert_eq!(
        evidence.details,
        vec!["kept".to_owned(); MAX_DIAGNOSTIC_DETAILS]
    );
    assert_eq!(evidence.omitted_detail_count, 8);
}

#[test]
fn cleanup_ready_repeated_publication_retains_the_original_marker() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.mark_cleanup_ready().unwrap();
    let original = root.dir.read(CLEANUP_READY_FILE).unwrap();
    root.mark_cleanup_ready().unwrap();

    assert_eq!(root.dir.read(CLEANUP_READY_FILE).unwrap(), original);
    assert!(root.lifecycle.lock().unwrap().cleanup_ready);
}

#[test]
fn invalid_heartbeat_json_cannot_prove_candidate_staleness() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let lease = read_marker_file(root.lease.lock().unwrap().as_deref().unwrap()).unwrap();
    root.dir.write(HEARTBEAT_FILE, b"invalid json").unwrap();

    let failure = candidate_is_stale(&root.dir, &lease, SystemTime::now())
        .err()
        .unwrap();

    assert_eq!(failure.reason, JanitorReason::HeartbeatInvalid);
    assert!(failure.cause.contains("expected value"));
}

#[test]
fn staging_contents_reject_an_unrecognized_entry() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("unexpected", b"preserved").unwrap();

    let failure = staging_contents_are_safe(&dir).err().unwrap();

    assert_eq!(failure.reason, JanitorReason::UnsafeStaging);
    assert_eq!(failure.cause, "unexpected staging entry unexpected");
    assert_eq!(dir.read("unexpected").unwrap(), b"preserved");
}

#[test]
fn marker_evidence_rejects_a_malformed_lease() {
    let parent = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(parent.path()).unwrap().join("lease");
    fs::write(&path, b"invalid json").unwrap();
    let file = File::open(&path).unwrap();

    let result = inspect_marker_evidence(&file, &path);

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { path: actual }) if actual == path));
}

#[test]
fn staging_enumeration_failure_cannot_certify_a_safe_directory() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();

    let fault = io_faults::arm("janitor.staging_entries");

    let failure = staging_contents_are_safe(&dir).err().unwrap();

    assert_eq!(failure.reason, JanitorReason::UnsafeStaging);
    assert!(failure.cause.contains("janitor.staging_entries"));
    assert!(fault.triggered());
}

#[test]
fn empty_directory_probe_propagates_an_enumeration_failure() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();

    let fault = io_faults::arm("janitor.empty_entries");

    let error = inspect_empty_directory(&dir).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains("janitor.empty_entries"));
    assert!(fault.triggered());
}

fn install_publication_marker_collision(
    coordinator: &ManagedRootCoordinator,
    boundary: PublishBoundary,
    marker_name: &str,
) {
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
}

#[rstest::rstest]
#[case(usize::MAX, 1)]
#[case(usize::MAX, 0)]
fn cleanup_cursor_rejects_arithmetic_overflow(#[case] cursor: usize, #[case] component: usize) {
    let observed = advance_cleanup_cursor(0, cursor, component)
        .unwrap_err()
        .to_string();

    assert_eq!(observed, "cleanup cursor size overflow");
}

#[test]
fn oversized_marker_reader_rejects_bytes_beyond_the_limit() {
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("marker");
    fs::write(
        &path,
        vec![b' '; usize::try_from(MAX_MARKER_BYTES).unwrap() + 1],
    )
    .unwrap();
    let file = File::open(path).unwrap();

    assert_eq!(
        read_json_file_checked::<LeaseMarker>(&file)
            .unwrap_err()
            .to_string(),
        "marker exceeds 64 KiB"
    );
}

#[cfg(unix)]
#[test]
fn marker_reader_propagates_unseekable_descriptor_errors() {
    let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let descriptor: std::os::fd::OwnedFd = stream.into();
    let file = File::from(descriptor);

    assert_eq!(
        read_json_file_checked::<LeaseMarker>(&file)
            .unwrap_err()
            .raw_os_error(),
        Some(libc::ESPIPE)
    );
}

#[cfg(unix)]
#[test]
fn marker_reader_propagates_directory_read_errors() {
    let parent = tempfile::tempdir().unwrap();
    let file = File::open(parent.path()).unwrap();

    assert_eq!(
        read_json_file_checked::<LeaseMarker>(&file)
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EISDIR)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn mount_identity_rejects_a_descriptor_without_a_filesystem_mount() {
    let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let descriptor: std::os::fd::OwnedFd = stream.into();
    let dir = cap_std::fs::Dir::from_std_file(File::from(descriptor));

    let error = macos_directory_mount_identity(&dir).unwrap_err();

    assert_ne!(
        error
            .raw_os_error()
            .expect("fstatfs failure retains its OS cause"),
        0
    );
}

#[test]
fn completed_owner_cleanup_is_idempotent_without_new_deletion_evidence() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let disposition_before_retry = root.cleanup(Duration::from_secs(1)).disposition;
    let repeated = root.cleanup(Duration::from_secs(1));

    assert_eq!(repeated.disposition, CleanupDisposition::Clean);
    assert_eq!(repeated.evidence, CleanupEvidence::default());
    assert_eq!(disposition_before_retry, CleanupDisposition::Clean);
}

#[test]
fn zero_budget_owner_cleanup_retains_the_root_until_a_funded_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let deferred = root.cleanup(Duration::ZERO);
    let disposition_before_retry = deferred.disposition;
    let observed_before_retry = deferred.evidence.remaining_root.as_ref();
    let observed_before_retry_expected = Some(&root.path);
    let removed_before_retry = deferred.evidence.removed_entries;
    let path_existed_before_retry = root.path.exists();
    let cleanup_disposition = root.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert_eq!(disposition_before_retry, CleanupDisposition::Deferred(None));
    assert_eq!(observed_before_retry, observed_before_retry_expected);
    assert_eq!(removed_before_retry, 0);
    assert!(path_existed_before_retry);
}

#[rstest::rstest]
#[case(std::io::ErrorKind::NotFound, CleanupDisposition::Failed)]
#[case(std::io::ErrorKind::TimedOut, CleanupDisposition::Deferred(None))]
#[case(std::io::ErrorKind::PermissionDenied, CleanupDisposition::Failed)]
fn failed_owner_removal_retains_the_live_capabilities_for_a_retry(
    #[case] kind: std::io::ErrorKind,
    #[case] disposition: CleanupDisposition,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let capabilities = root.take_owner_cleanup_capabilities().unwrap();

    let record = CleanupRecord {
        disposition: CleanupDisposition::Failed,
        evidence: CleanupEvidence {
            remaining_root: Some(root.path.clone()),
            ..CleanupEvidence::default()
        },
    };

    let failure = root.finish_owner_removal(
        record,
        capabilities,
        root.path.file_name().unwrap(),
        Err(std::io::Error::new(kind, "injected removal failure")),
    );

    let capability_was_preserved = root.lease.lock().unwrap().is_some();
    let cleanup_disposition = root.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert_eq!(failure.disposition, disposition);
    assert_eq!(failure.evidence.details, ["injected removal failure"]);
    assert_eq!(failure.evidence.remaining_root.as_ref(), Some(&root.path));
    assert!(capability_was_preserved);
}

#[cfg(target_os = "macos")]
#[test]
fn heartbeat_refresh_reports_descriptor_errors_and_recovers_the_original_marker() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original_bytes = root.dir.read(HEARTBEAT_FILE).unwrap();
    let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let descriptor: std::os::fd::OwnedFd = stream.into();
    let original = root
        .heartbeat
        .lock()
        .unwrap()
        .replace(File::from(descriptor));

    let error = root.refresh_heartbeat().unwrap_err();

    let bytes_before_retry = root.dir.read(HEARTBEAT_FILE).unwrap();
    *root.heartbeat.lock().unwrap() = original;
    root.refresh_heartbeat().unwrap();

    assert_eq!(root.dir.read(HEARTBEAT_FILE).unwrap(), original_bytes);
    assert!(matches!(
        error,
        WorkspaceError::Io {
            operation: "refresh workspace heartbeat",
            ..
        }
    ));
    assert_eq!(bytes_before_retry, original_bytes);
}

#[cfg(unix)]
#[rstest::rstest]
#[case(|_: &ManagedRunRoot, parent: &tempfile::TempDir| Utf8Path::from_path(parent.path()).unwrap().join("outside"))]
#[case(|root: &ManagedRunRoot, _: &tempfile::TempDir| root.path.parent().unwrap().join(".."))]
fn identity_replacement_move_rejects_invalid_destinations_without_moving_the_root(
    #[case] destination: fn(&ManagedRunRoot, &tempfile::TempDir) -> Utf8PathBuf,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let destination = destination(&root, &parent);

    let result = root.move_for_identity_replacement_test(&destination);

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { path }) if path == destination));
    assert!(root.path.exists());
    assert!(root.dir.exists(LEASE_FILE));
}

#[cfg(unix)]
#[rstest::rstest]
#[case(|root: &mut ManagedRunRoot| { root.coordinator_local_lock = Arc::new(Mutex::new(())); poison(&root.coordinator_local_lock); })]
#[case(|root: &mut ManagedRunRoot| root.coordinator_file = unsupported_lock_file())]
fn identity_replacement_move_preserves_the_root_when_locking_fails(
    #[case] configure: fn(&mut ManagedRunRoot),
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let mut root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    configure(&mut root);
    let destination = coordinator.path.join("parked-root");

    let result = root.move_for_identity_replacement_test(&destination);

    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "lock coordinator",
            ..
        })
    ));
    assert!(root.path.exists());
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn root_construction_does_not_create_staging_state_when_coordinator_locking_fails() {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    let original = std::mem::replace(&mut coordinator.file, unsupported_lock_file());
    let initial_result_matched = matches!(
        ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution),
        Err(WorkspaceError::Io {
            operation: "lock coordinator",
            ..
        })
    );
    let entries_before_retry = fs::read_dir(&coordinator.path).unwrap().count();
    coordinator.file = original;
    let retry = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let cleanup_disposition = retry.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert!(initial_result_matched);
    assert_eq!(entries_before_retry, 1);
}

#[test]
fn expired_shared_lock_lookup_rejects_the_deadline_without_poisoning_the_registry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);

    let error = shared_coordinator_lock(
        &coordinator.dir,
        &coordinator.path,
        std::time::Instant::now(),
    )
    .unwrap_err();

    let retry = shared_coordinator_lock(&coordinator.dir, &coordinator.path, deadline()).unwrap();

    assert!(Arc::ptr_eq(&retry, &coordinator.local_lock));
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "lock coordinator registry",
            ..
        }
    ));
    assert!(error.to_string().contains("deadline"));
}

#[test]
fn identity_bound_rollback_preserves_the_replacement_and_both_error_causes() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let original = create_owned_directory(&dir, "created").unwrap();
    let identity = directory_identity(&original).unwrap();
    dir.rename("created", &dir, "original").unwrap();
    dir.create_dir("created").unwrap();
    dir.write("created/payload", b"replacement").unwrap();
    let path = Utf8Path::from_path(parent.path()).unwrap().join("created");

    let error = error_after_identity_bound_new_directory_rollback(
        &dir,
        "created",
        identity,
        deadline(),
        &path,
        WorkspaceError::io("primary creation stage", &path, "primary failure"),
    );

    assert!(
        matches!(&error, WorkspaceError::Io { operation: "primary creation stage", path: actual, .. } if actual == &path)
    );
    assert!(error.to_string().contains("primary failure"));
    assert!(
        error
            .to_string()
            .contains("secondary new-directory rollback failure")
    );
    assert_eq!(dir.read("created/payload").unwrap(), b"replacement");
    assert!(dir.exists("original"));
}

struct CallbackDropWitness(std::rc::Rc<std::cell::Cell<usize>>);

impl Drop for CallbackDropWitness {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[rstest::rstest]
#[case(Err("selection-error".into()), None, Some(JanitorReason::SelectionFailed), &["select"])]
#[case(Err("selection-error".into()), Some(JanitorReason::SelectionDeadline), Some(JanitorReason::SelectionDeadline), &["select"])]
#[case(Ok(vec!["run-00000000-0000-0000-0000-000000000001".into()]), None, None, &["select", "elapsed"])]
fn janitor_borrows_callbacks_and_preserves_selection_clock_order(
    #[case] selection: Result<Vec<String>, Utf8PathBuf>,
    #[case] stop_reason: Option<JanitorReason>,
    #[case] expected_stop: Option<JanitorReason>,
    #[case] expected_trace: &[&str],
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let trace = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let drops = std::rc::Rc::new(std::cell::Cell::new(0));
    let selection_witness = CallbackDropWitness(std::rc::Rc::clone(&drops));
    let selection_trace = std::rc::Rc::clone(&trace);
    let select = move |scan: &mut JanitorScan| {
        std::hint::black_box(&selection_witness);
        selection_trace.borrow_mut().push("select");
        scan.stop_reason = stop_reason;
        selection
            .clone()
            .map(|names| (names, None))
            .map_err(|path| WorkspaceError::InvalidPath { path })
    };
    let clock_witness = CallbackDropWitness(std::rc::Rc::clone(&drops));
    let clock_trace = std::rc::Rc::clone(&trace);
    let elapsed = move |_| {
        std::hint::black_box(&clock_witness);
        clock_trace.borrow_mut().push("elapsed");
        Duration::ZERO
    };

    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        SystemTime::now(),
        std::time::Instant::now(),
        &elapsed,
        &select,
    );
    let callback_trace = trace.borrow().clone();

    let drops_before_callback_release = drops.get();
    drop((select, elapsed));

    assert_eq!(drops.get(), 2);
    assert_eq!(callback_trace, expected_trace);
    assert_eq!(report.scan.stop_reason, expected_stop);
    assert_eq!(drops_before_callback_release, 0);
}

#[test]
fn taking_an_empty_owner_slot_does_not_manufacture_a_lease() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original = root.take_owner_cleanup_capabilities().unwrap();
    let empty = root.take_owner_cleanup_capabilities().unwrap();
    let failure_was_observed = empty.lease_guard.is_none();
    let observed_before_retry = empty.progress.examined_entries;
    let removed_before_retry = empty.progress.removed_entries;
    root.restore_owner_cleanup_capabilities(original);
    let cleanup_disposition = root.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert!(failure_was_observed);
    assert_eq!(observed_before_retry, 0);
    assert_eq!(removed_before_retry, 0);
}

#[test]
fn expired_local_lock_deadline_leaves_the_mutex_available() {
    let lock = Mutex::new(());

    let error = lock_local_until(&lock, std::time::Instant::now()).unwrap_err();

    drop(lock.try_lock().unwrap());

    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "lock coordinator bootstrap",
            ..
        }
    ));
    assert!(error.to_string().contains("deadline"));
}

#[test]
fn expired_file_lock_deadline_leaves_the_file_available() {
    let file = tempfile::tempfile().unwrap();
    let path = Utf8Path::new("deadline-lock");

    let error = lock_file_until(&file, std::time::Instant::now(), path).unwrap_err();

    File::try_lock(&file).unwrap();
    File::unlock(&file).unwrap();

    assert!(
        matches!(&error, WorkspaceError::Io { operation: "lock coordinator bootstrap", path: actual, .. } if actual == path)
    );
    assert!(error.to_string().contains("deadline"));
}

#[cfg(unix)]
#[test]
fn owner_removal_observes_external_absence_without_inventing_deleted_entries() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let capabilities = root.take_owner_cleanup_capabilities().unwrap();

    let record = CleanupRecord {
        disposition: CleanupDisposition::Failed,
        evidence: CleanupEvidence {
            remaining_root: Some(root.path.clone()),
            ..CleanupEvidence::default()
        },
    };
    fs::remove_dir_all(&root.path).unwrap();
    let completed = root.finish_owner_removal(
        record,
        capabilities,
        root.path.file_name().unwrap(),
        Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
    );
    let disposition_before_retry = completed.disposition;
    let observed_before_retry = completed.evidence.remaining_root;
    let removed_before_retry = completed.evidence.removed_entries;
    let namespace_after_failure = NamespaceLifecycle::load(&root.namespace);
    let cleanup_disposition = root.cleanup(Duration::ZERO).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert_eq!(disposition_before_retry, CleanupDisposition::Clean);
    assert_eq!(observed_before_retry, None);
    assert_eq!(removed_before_retry, 0);
    assert_eq!(namespace_after_failure, NamespaceLifecycle::Complete);
}

fn empty_deleting_candidate(coordinator: &ManagedRootCoordinator) -> String {
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    coordinator.dir.create_dir(&name).unwrap();
    name
}

fn ready_abandoned_candidate(coordinator: &ManagedRootCoordinator) -> String {
    let root = ManagedRunRoot::create(coordinator, OwnerKind::PublicExecution).unwrap();
    root.mark_cleanup_ready().unwrap();
    root.path.file_name().unwrap().to_owned()
}

#[cfg(unix)]
#[rstest::rstest]
#[case(empty_deleting_candidate)]
#[case(ready_abandoned_candidate)]
fn selected_janitor_candidate_waits_for_coordinator_contention_then_recovers(
    #[case] prepare: fn(&ManagedRootCoordinator) -> String,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let name = prepare(&coordinator);
    let held = coordinator.local_lock.lock().unwrap();

    let report = reclaim_selected(&coordinator, std::slice::from_ref(&name));

    let candidate_existed_after_failure = coordinator.dir.exists(&name);
    drop(held);

    let observed = reclaim_selected(&coordinator, std::slice::from_ref(&name)).reclaimed_roots;

    assert_eq!(observed, 1);
    assert!(!coordinator.dir.exists(&name));
    assert!(
        report
            .details
            .iter()
            .any(|detail| detail.reason == JanitorReason::CoordinatorBusy)
    );
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert!(candidate_existed_after_failure);
}

#[cfg(unix)]
#[rstest::rstest]
#[case(empty_deleting_candidate)]
#[case(ready_abandoned_candidate)]
fn selected_janitor_candidate_retains_typed_coordinator_io_failure_until_retry(
    #[case] prepare: fn(&ManagedRootCoordinator) -> String,
) {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    let name = prepare(&coordinator);
    let original = std::mem::replace(&mut coordinator.file, unsupported_lock_file());

    let report = reclaim_selected(&coordinator, std::slice::from_ref(&name));
    let initial_diagnostic_matched = report.details.iter().any(|detail| {
        detail.reason == JanitorReason::CoordinatorFailed
            && detail.message.contains("lock coordinator")
    });

    let candidate_existed_after_failure = coordinator.dir.exists(&name);
    coordinator.file = original;

    let observed = reclaim_selected(&coordinator, std::slice::from_ref(&name)).reclaimed_roots;

    assert_eq!(observed, 1);
    assert!(!coordinator.dir.exists(&name));
    assert!(initial_diagnostic_matched);
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert!(candidate_existed_after_failure);
}

#[rstest::rstest]
#[case(|marker: &mut LeaseMarker| marker.schema += 1)]
#[case(|marker: &mut LeaseMarker| marker.run_id = "different-run".into())]
fn selected_janitor_candidate_rejects_mismatched_lease_schema_or_identity(
    #[case] change: fn(&mut LeaseMarker),
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let mut marker = read_marker_file(root.lease.lock().unwrap().as_deref().unwrap()).unwrap();
    let name = root.path.file_name().unwrap().to_owned();
    drop(root);
    change(&mut marker);
    let dir = coordinator.dir.open_dir(&name).unwrap();
    let bytes = serde_json::to_vec(&marker).unwrap();
    dir.write(LEASE_FILE, &bytes).unwrap();

    let report = reclaim_selected(&coordinator, std::slice::from_ref(&name));

    assert_eq!(
        report
            .unresolved_by_reason
            .get(&JanitorReason::MarkerInvalid),
        Some(&1)
    );
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert_eq!(dir.read(LEASE_FILE).unwrap(), bytes);
    assert!(coordinator.dir.exists(&name));
}

#[cfg(unix)]
#[test]
fn selected_janitor_candidate_observes_external_absence_before_claim() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let name = ready_abandoned_candidate(&coordinator);
    let path = coordinator.path.join(&name);

    let race = io_faults::arm_action("janitor.ready_identity", 0, move || {
        fs::remove_dir_all(&path)
            .expect("remove the selected root at the external-deletion boundary");
        Ok(())
    });

    let report = reclaim_selected(&coordinator, std::slice::from_ref(&name));

    assert!(race.triggered());
    assert_eq!(report.inspected_roots, 1);
    assert_eq!(report.absent_roots, 1);
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert!(!coordinator.dir.exists(&name));
}

#[cfg(unix)]
#[rstest::rstest]
#[case(empty_deleting_candidate, |name: &str| name.to_owned(), 1, false, 0)]
#[case(ready_abandoned_candidate, |name: &str| format!("{DELETING_PREFIX}{}", name.strip_prefix(ACTIVE_PREFIX).unwrap()), 0, true, 4)]
fn selected_janitor_unlock_error_preserves_observed_progress_and_retry_authority(
    #[case] prepare: fn(&ManagedRootCoordinator) -> String,
    #[case] next_name: fn(&str) -> String,
    #[case] reclaimed: u64,
    #[case] remaining: bool,
    #[case] retry_removed: u64,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let name = prepare(&coordinator);

    let fault = io_faults::arm("coordinator.unlock");

    let report = reclaim_selected(&coordinator, std::slice::from_ref(&name));
    let fault_was_triggered = fault.triggered();
    let initial_diagnostic_matched = report.details.iter().any(|detail| {
        detail.reason == JanitorReason::CoordinatorFailed
            && detail.message.contains("coordinator.unlock")
    });

    let next_name = next_name(&name);
    let observed_before_retry = coordinator.dir.exists(&next_name);
    drop(fault);
    let retry = reclaim_selected(&coordinator, std::slice::from_ref(&next_name));

    assert_eq!(retry.reclaimed_roots, u64::from(remaining));
    assert_eq!(retry.removed_entries, retry_removed);
    assert!(!coordinator.dir.exists(&next_name));
    assert!(fault_was_triggered);
    assert!(initial_diagnostic_matched);
    assert_eq!(report.reclaimed_roots, reclaimed);
    assert_eq!(report.removed_entries, reclaimed);
    assert_eq!(observed_before_retry, remaining);
}

fn mismatch_live_lease_owner(root: &ManagedRunRoot) {
    let mut marker = read_marker_file(root.lease.lock().unwrap().as_deref().unwrap()).unwrap();
    marker.owner = OwnerKind::PublicDelivery;
    root.dir
        .write(LEASE_FILE, serde_json::to_vec(&marker).unwrap())
        .unwrap();
}

#[cfg(unix)]
#[rstest::rstest]
#[case(|_: &ManagedRunRoot| {}, NamespaceLifecycle::Deleting)]
#[case(mismatch_live_lease_owner, NamespaceLifecycle::Active)]
fn owner_unlock_failure_releases_the_os_lock_and_preserves_cleanup_retry(
    #[case] configure: fn(&ManagedRunRoot),
    #[case] expected_namespace: NamespaceLifecycle,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original_lease = root.dir.read(LEASE_FILE).unwrap();
    configure(&root);

    let fault = io_faults::arm("coordinator.unlock");
    let failed = root.cleanup(Duration::from_secs(1));
    let fault_was_triggered = fault.triggered();
    let disposition_before_retry = failed.disposition;
    let namespace_after_failure = NamespaceLifecycle::load(&root.namespace);
    let observed_before_retry_2 = failed.evidence.remaining_root;
    let observed_before_retry_2_expected = root.verified_remaining_root().unwrap();
    let initial_diagnostic_matched = failed
        .evidence
        .details
        .iter()
        .any(|detail| detail.contains("coordinator.unlock"));
    let independent = File::options()
        .read(true)
        .write(true)
        .open(coordinator.path.join(COORDINATOR_FILE))
        .unwrap();
    File::try_lock(&independent)
        .expect("failed unlock must not leave the coordinator OS lock held");
    File::unlock(&independent).unwrap();
    drop(fault);
    root.dir.write(LEASE_FILE, original_lease).unwrap();
    let cleanup_disposition = root.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert!(fault_was_triggered);
    assert_eq!(disposition_before_retry, CleanupDisposition::Failed);
    assert_eq!(namespace_after_failure, expected_namespace);
    assert_eq!(observed_before_retry_2, observed_before_retry_2_expected);
    assert!(initial_diagnostic_matched);
}

#[cfg(unix)]
struct LongCleanupCursor {
    leaf: cap_std::fs::Dir,
    component: String,
    depth: usize,
}

#[cfg(unix)]
impl LongCleanupCursor {
    fn new(root: &cap_std::fs::Dir, depth: usize) -> Self {
        let component = "d".repeat(255);
        let mut leaf = root.try_clone().unwrap();
        for _ in 0..depth {
            leaf = create_owned_directory(&leaf, &component)
                .expect("create the fd-relative cursor fixture");
        }
        leaf.write("payload", b"preserved").unwrap();
        Self {
            leaf,
            component,
            depth,
        }
    }
}

#[cfg(unix)]
impl Drop for LongCleanupCursor {
    fn drop(&mut self) {
        use rustix::fs::{Mode, OFlags};
        // Follow exactly the fixture's known parent count; keep only a few descriptors live.
        let _ = self.leaf.remove_file("payload");
        for _ in 0..self.depth {
            let parent = rustix::fs::openat(
                &self.leaf,
                "..",
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .expect("open the fixture's retained parent");
            let parent = cap_std::fs::Dir::from_std_file(File::from(parent));
            let _ = parent.remove_dir(&self.component);
            self.leaf = parent;
        }
    }
}

#[cfg(unix)]
#[test]
fn cleanup_propagates_a_real_cursor_limit_without_deleting_the_leaf_payload() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let root = create_owned_directory(&dir, "root").unwrap();
    // 257 components of 255 bytes plus separators exceed the 64 KiB cursor limit.
    let fixture = LongCleanupCursor::new(&root, 257);

    let error = remove_one_claimed_entry(
        &dir,
        "root",
        Some(directory_identity(&root).unwrap()),
        u64::MAX,
        std::time::Instant::now(),
        Duration::from_secs(10),
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "cleanup cursor exceeds 65536 bytes");
    assert_eq!(fixture.leaf.read("payload").unwrap(), b"preserved");
    assert!(root.exists(&fixture.component));
}

#[cfg(unix)]
#[test]
fn deleting_owner_cleanup_preserves_a_shared_capability_until_its_release() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let deleting = coordinator
        .path
        .join(format!("{DELETING_PREFIX}{}", root.run_id));
    root.move_for_identity_replacement_test(&deleting).unwrap();
    let shared = Arc::clone(root.lease.lock().unwrap().as_ref().unwrap());
    let failed = root.cleanup(Duration::from_secs(1));
    let disposition_before_retry = failed.disposition;
    let observed_before_retry = failed.evidence.remaining_root;
    let initial_diagnostic_matched = failed
        .evidence
        .details
        .iter()
        .any(|detail| detail.contains("shared lease guard"));
    let path_existed_before_retry = deleting.exists();
    drop(shared);
    let cleanup_disposition = root.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert!(!deleting.exists());
    assert_eq!(disposition_before_retry, CleanupDisposition::Failed);
    assert_eq!(observed_before_retry, Some(deleting.clone()));
    assert!(initial_diagnostic_matched);
    assert!(path_existed_before_retry);
}

#[cfg(unix)]
#[rstest::rstest]
#[case("claim.candidate_identity", NamespaceLifecycle::Active, Some(true))]
#[case("claim.rename", NamespaceLifecycle::Deleting, None)]
fn owner_cleanup_preserves_the_actual_namespace_when_its_io_exhausts_the_budget(
    #[case] point: &'static str,
    #[case] expected_namespace: NamespaceLifecycle,
    #[case] deadline_detail: Option<bool>,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();

    let fault = io_faults::arm_action(point, 0, || {
        std::thread::sleep(Duration::from_millis(1050));
        Ok(())
    });
    let deferred = root.cleanup(Duration::from_secs(1));
    let fault_was_triggered = fault.triggered();
    let disposition_before_retry = deferred.disposition;
    let removed_before_retry = deferred.evidence.removed_entries;
    let namespace_after_failure = NamespaceLifecycle::load(&root.namespace);
    let observed_before_retry_2 = deferred.evidence.remaining_root;
    let observed_before_retry_2_expected = root.verified_remaining_root().unwrap();
    let observed_before_retry_3 = deferred
        .evidence
        .details
        .first()
        .map(|detail| detail.contains("owner cleanup deadline exceeded"));
    drop(fault);
    let cleanup_disposition = root.cleanup(Duration::from_secs(1)).disposition;

    assert_eq!(cleanup_disposition, CleanupDisposition::Clean);
    assert!(fault_was_triggered);
    assert_eq!(disposition_before_retry, CleanupDisposition::Deferred(None));
    assert_eq!(removed_before_retry, 0);
    assert_eq!(namespace_after_failure, expected_namespace);
    assert_eq!(observed_before_retry_2, observed_before_retry_2_expected);
    assert_eq!(observed_before_retry_3, deadline_detail);
}

#[cfg(unix)]
#[rstest::rstest]
#[case(|_: &ManagedRunRoot| {}, "workspace.io", true, true)]
#[case(mismatch_live_lease_owner, "workspace.path.invalid", false, false)]
fn identity_move_unlock_failure_releases_the_lock_and_keeps_primary_error_priority(
    #[case] configure: fn(&ManagedRunRoot),
    #[case] code: &str,
    #[case] unlock_is_primary: bool,
    #[case] moved: bool,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original_lease = root.dir.read(LEASE_FILE).unwrap();
    root.dir.write("payload", b"preserved").unwrap();
    configure(&root);
    let destination = coordinator.path.join("held-root");

    let fault = io_faults::arm("coordinator.unlock");

    let error = root
        .move_for_identity_replacement_test(&destination)
        .unwrap_err();
    let fault_was_triggered = fault.triggered();

    let observed_before_retry = error.to_string().contains("coordinator.unlock");
    let observed_before_retry_2 = destination.exists();
    let bytes_before_retry = root.dir.read("payload").unwrap();
    let independent = File::options()
        .read(true)
        .write(true)
        .open(coordinator.path.join(COORDINATOR_FILE))
        .unwrap();
    File::try_lock(&independent).expect("failed move unlock must release the coordinator OS lock");
    File::unlock(&independent).unwrap();
    drop(fault);
    root.dir.write(LEASE_FILE, original_lease).unwrap();
    root.move_for_identity_replacement_test(&destination)
        .unwrap();

    assert_eq!(root.dir.read("payload").unwrap(), b"preserved");
    assert!(fault_was_triggered);
    assert_eq!(error.code(), code);
    assert_eq!(observed_before_retry, unlock_is_primary);
    assert_eq!(observed_before_retry_2, moved);
    assert_eq!(bytes_before_retry, b"preserved");
}

#[cfg(unix)]
#[test]
fn identity_move_reports_a_disappeared_source_without_creating_the_destination() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let destination = coordinator.path.join("held-root");
    fs::remove_dir_all(&root.path).unwrap();

    let result = root.move_for_identity_replacement_test(&destination);

    assert!(matches!(result, Err(WorkspaceError::InvalidPath { path }) if path == root.path));
    assert!(!destination.exists());
}

struct FilledNamespace {
    dir: cap_std::fs::Dir,
    entries: usize,
}

impl FilledNamespace {
    fn new(dir: &cap_std::fs::Dir, entries: usize) -> Self {
        let mut fixture = Self {
            dir: dir.try_clone().unwrap(),
            entries: 0,
        };
        for index in 0..entries {
            fixture
                .dir
                .write(format!("noise-{index}"), b"")
                .expect("populate the namespace admission fixture");
            fixture.entries += 1;
        }
        fixture
    }
}

impl Drop for FilledNamespace {
    fn drop(&mut self) {
        for index in 0..self.entries {
            let _ = self.dir.remove_file(format!("noise-{index}"));
        }
    }
}

#[test]
fn child_admission_rejects_a_real_namespace_at_the_entry_limit() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let _files = FilledNamespace::new(&root.dir, MAX_MANAGED_CHILDREN - controls);
    let retry_outcome = matches!(
        root.create_child("worker-"),
        Err(WorkspaceError::OwnedWorkspaceLimit {
            planned: 100_001,
            limit: 100_000
        })
    );

    assert!(retry_outcome);
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), MAX_MANAGED_CHILDREN);
}

#[test]
fn janitor_selection_rejects_a_real_oversized_namespace_without_advancing_the_cursor() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let _files = FilledNamespace::new(&coordinator.dir, MAX_MANAGED_CHILDREN);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let _guard = CoordinatorLockGuard::acquire(&coordinator).unwrap();
    let now = std::time::Instant::now();
    let mut scan = JanitorScan::default();

    let error = select_reclaim_candidates_locked_until_with_hooks(
        &coordinator,
        &mut scan,
        now + Duration::from_secs(1),
        &|| now,
        &<() as Default>::default,
        &<() as Default>::default,
    )
    .unwrap_err();

    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "select abandoned workspaces",
            ..
        }
    ));
    assert!(error.to_string().contains("managed child limit exceeded"));
    assert_eq!(scan.stop_reason, Some(JanitorReason::SelectionChildLimit));
    assert!(!scan.directory_scan_complete);
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
}

#[test]
fn child_counter_overflow_rolls_back_the_unpublished_directory() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    root.lifecycle.lock().unwrap().live_children = u64::MAX;
    let initial_result_matched = matches!(
        root.create_child("worker-"),
        Err(WorkspaceError::CopySizeOverflow)
    );
    let live_children_after_failure = root.lifecycle.lock().unwrap().live_children;
    let entries_before_retry = root.dir.entries().unwrap().count();
    root.lifecycle.lock().unwrap().live_children = 0;
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();

    assert!(initial_result_matched);
    assert_eq!(live_children_after_failure, u64::MAX);
    assert_eq!(entries_before_retry, controls);
}

#[test]
fn unavailable_parent_lease_rolls_back_the_unpublished_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let lease = root.lease.lock().unwrap().take().unwrap();

    let error = root.create_child("worker-").unwrap_err();

    let initial_diagnostic_matched = error
        .to_string()
        .contains("managed root lease is unavailable");
    let live_children_after_failure = root.lifecycle.lock().unwrap().live_children;
    let entries_before_retry = root.dir.entries().unwrap().count();
    *root.lease.lock().unwrap() = Some(lease);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();

    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create managed child",
            ..
        }
    ));
    assert!(initial_diagnostic_matched);
    assert_eq!(live_children_after_failure, 0);
    assert_eq!(entries_before_retry, controls);
}

#[rstest::rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
#[case(6)]
#[case(7)]
#[case(8)]
#[case(9)]
#[case(10)]
fn selection_deadline_at_each_checkpoint_preserves_the_coordinator_cursor(
    #[case] expired_call: usize,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let now = std::time::Instant::now();
    let deadline = now + Duration::from_secs(1);
    let calls = std::cell::Cell::new(0);
    let clock = || {
        calls.set(calls.get() + 1);
        [now, deadline][usize::from(calls.get() >= expired_call)]
    };
    let mut scan = JanitorScan::default();

    let error = select_reclaim_candidates_locked_until_with_hooks(
        &coordinator,
        &mut scan,
        deadline,
        &clock,
        &<() as Default>::default,
        &<() as Default>::default,
    )
    .unwrap_err();

    assert_eq!(calls.get(), expired_call);
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "select abandoned workspaces",
            ..
        }
    ));
    assert!(error.to_string().contains("selection deadline exceeded"));
    assert_eq!(scan.stop_reason, Some(JanitorReason::SelectionDeadline));
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn cleanup_child_permission_repair_rejects_a_nul_name_before_touching_the_file() {
    use std::os::unix::ffi::OsStrExt;
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-permissions").unwrap();
    let metadata = dir.metadata("payload").unwrap();

    let error = make_cleanup_child_accessible(
        &dir,
        std::ffi::OsStr::from_bytes(b"bad\0name"),
        &metadata,
        std::time::Instant::now(),
        Duration::from_secs(1),
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "cleanup name contains NUL");
    assert_eq!(dir.read("payload").unwrap(), b"keep-permissions");
    assert_eq!(
        dir.metadata("payload").unwrap().permissions(),
        metadata.permissions()
    );
}

#[cfg(unix)]
#[test]
fn cleanup_child_permission_repair_propagates_a_missing_entry_error() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-permissions").unwrap();
    let metadata = dir.metadata("payload").unwrap();

    let error = make_cleanup_child_accessible(
        &dir,
        std::ffi::OsStr::new("absent"),
        &metadata,
        std::time::Instant::now(),
        Duration::from_secs(1),
    )
    .unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(dir.read("payload").unwrap(), b"keep-permissions");
    assert_eq!(
        dir.metadata("payload").unwrap().permissions(),
        metadata.permissions()
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
#[case(6)]
#[case(7)]
#[case(8)]
#[case(9)]
#[case(10)]
#[case(11)]
#[case(12)]
#[case(13)]
#[case(14)]
fn permission_repair_cancellation_at_each_checkpoint_preserves_the_directory(
    #[case] stop_at: usize,
) {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.create_dir("directory").unwrap();
    dir.write("directory/payload", b"keep-directory").unwrap();
    let identity = metadata_identity(&dir.symlink_metadata("directory").unwrap());
    fs::set_permissions(
        parent.path().join("directory"),
        fs::Permissions::from_mode(0o0),
    )
    .unwrap();
    let checks = std::cell::Cell::new(0);
    let stopped = || {
        checks.set(checks.get() + 1);
        checks.get() >= stop_at
    };

    let fault = io_faults::arm_action("cleanup.open_directory", 0, || {
        Err(std::io::ErrorKind::PermissionDenied.into())
    });

    let result = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        CleanupBudget {
            duration: Duration::from_secs(60),
            stopped: Some(&stopped),
        },
    );
    fs::set_permissions(
        parent.path().join("directory"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();

    let error = result.unwrap_err();

    let observed_before_retry = checks.get();
    let namespace_identity_after_failure =
        metadata_identity(&dir.symlink_metadata("directory").unwrap());
    let bytes_before_retry = dir.read("directory/payload").unwrap();
    drop(fault);
    let reopened = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(reopened.read("payload").unwrap(), b"keep-directory");
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(observed_before_retry, stop_at);
    assert_eq!(namespace_identity_after_failure, identity);
    assert_eq!(bytes_before_retry, b"keep-directory");
}

#[test]
fn selection_deadline_overflow_preserves_the_coordinator_cursor() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let mut scan = JanitorScan::default();
    let now = std::time::Instant::now();

    let error = select_reclaim_candidates_locked_with_hooks(
        &coordinator,
        &mut scan,
        Duration::MAX,
        &|| now,
        &<() as Default>::default,
    )
    .unwrap_err();

    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "select abandoned workspaces",
            ..
        }
    ));
    assert!(error.to_string().contains("selection deadline overflow"));
    assert_eq!(scan.observed_matching_entries, 0);
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
}

#[cfg(unix)]
#[rstest::rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
fn directory_permission_cancellation_at_each_checkpoint_preserves_payload(#[case] stop_at: usize) {
    use std::os::unix::fs::PermissionsExt;
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-directory").unwrap();
    let identity = directory_identity(&dir).unwrap();
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o0)).unwrap();
    let checks = std::cell::Cell::new(0);
    let stopped = || {
        checks.set(checks.get() + 1);
        checks.get() >= stop_at
    };

    let result = make_cleanup_directory_accessible(
        &dir,
        std::time::Instant::now(),
        CleanupBudget {
            duration: Duration::from_secs(60),
            stopped: Some(&stopped),
        },
    );
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o700)).unwrap();

    let observed_before_retry = checks.get();
    let observed_before_retry_2 = directory_identity(&dir).unwrap();
    let bytes_before_retry = dir.read("payload").unwrap();
    make_cleanup_directory_accessible(&dir, std::time::Instant::now(), Duration::from_secs(1))
        .unwrap();

    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(observed_before_retry, stop_at);
    assert_eq!(observed_before_retry_2, identity);
    assert_eq!(bytes_before_retry, b"keep-directory");
}

#[cfg(unix)]
#[rstest::rstest]
#[case(1, "cancel claim checkpoint")]
#[case(2, "cancel claim checkpoint")]
#[case(3, "cancel claim checkpoint")]
#[case(4, "cancel claim checkpoint")]
#[case(5, "cancel claim checkpoint")]
#[case(6, "cancel claim checkpoint")]
#[case(7, "claim managed workspace")]
#[case(8, "claim managed workspace")]
#[case(9, "claim managed workspace")]
fn claim_cancellation_at_each_checkpoint_preserves_active_identity_and_lease(
    #[case] stop_at: usize,
    #[case] operation: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let current_name = root.path.file_name().unwrap();
    let deleting_name = format!("{DELETING_PREFIX}{}", root.run_id);
    let identity = directory_identity(&root.dir).unwrap();
    let lease_bytes = root.dir.read(LEASE_FILE).unwrap();
    let mut lease = Some(
        root.lease
            .lock()
            .unwrap()
            .as_deref()
            .unwrap()
            .try_clone()
            .unwrap(),
    );
    let checks = std::cell::Cell::new(0);
    let before = || {
        checks.set(checks.get() + 1);
        (checks.get() < stop_at)
            .then_some(())
            .ok_or_else(|| WorkspaceError::io("cancel claim checkpoint", &root.path, "cancelled"))
    };
    let deleting = std::cell::Cell::new(false);
    let on_deleting = || deleting.set(true);

    let error = claim_managed_child(
        &coordinator.dir,
        current_name,
        &deleting_name,
        &root.run_id,
        root.owner,
        &root.dir,
        &mut lease,
        &on_deleting,
        &before,
    )
    .err()
    .unwrap();

    let observed_before_retry = checks.get();
    let failure_was_observed = !deleting.get();
    let root_was_directory_after_failure = root.path.is_dir();
    let path_existed_before_retry = !coordinator.path.join(&deleting_name).exists();
    let root_identity_after_failure = directory_identity(&root.dir).unwrap();
    let bytes_before_retry = root.dir.read(LEASE_FILE).unwrap();
    let capability_was_preserved = lease.is_some();
    let retry = claim_managed_child(
        &coordinator.dir,
        current_name,
        &deleting_name,
        &root.run_id,
        root.owner,
        &root.dir,
        &mut lease,
        &on_deleting,
        &|| Ok(()),
    )
    .unwrap();

    assert!(matches!(retry, ClaimResult::Claimed));
    assert!(deleting.get());
    assert!(!root.path.exists());
    assert_eq!(
        coordinator
            .dir
            .read(format!("{deleting_name}/{LEASE_FILE}"))
            .unwrap(),
        lease_bytes
    );
    assert!(matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation));
    assert!(error.to_string().contains("cancel claim checkpoint"));
    assert_eq!(observed_before_retry, stop_at);
    assert!(failure_was_observed);
    assert!(root_was_directory_after_failure);
    assert!(path_existed_before_retry);
    assert_eq!(root_identity_after_failure, identity);
    assert_eq!(bytes_before_retry, lease_bytes);
    assert!(capability_was_preserved);
}

#[test]
fn controlled_selection_expired_budget_preserves_the_cursor() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let mut scan = JanitorScan::default();
    let remaining: fn() -> Duration = || Duration::ZERO;

    let error = select_reclaim_candidates_controlled_with_hooks(
        &coordinator,
        &mut scan,
        &remaining,
        &<() as Default>::default,
    )
    .unwrap_err();

    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "select abandoned workspaces",
            ..
        }
    ));
    assert!(error.to_string().contains("selection deadline exceeded"));
    assert_eq!(scan.stop_reason, Some(JanitorReason::SelectionDeadline));
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
}

#[test]
fn controlled_selection_file_contention_preserves_the_cursor_and_allows_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let path = coordinator.path.join(COORDINATOR_FILE);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let independent = File::options().read(true).write(true).open(&path).unwrap();
    File::try_lock(&independent).unwrap();
    let mut scan = JanitorScan::default();
    let remaining: fn() -> Duration = || Duration::from_secs(1);
    let selection = select_reclaim_candidates_controlled_with_hooks(
        &coordinator,
        &mut scan,
        &remaining,
        &<() as Default>::default,
    )
    .unwrap();
    let observed_before_retry = selection;
    let observed_before_retry_2 = scan.stop_reason;
    let bytes_before_retry = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    File::unlock(&independent).unwrap();
    let mut retry_scan = JanitorScan::default();

    let observed = select_reclaim_candidates_controlled_with_hooks(
        &coordinator,
        &mut retry_scan,
        &remaining,
        &<() as Default>::default,
    )
    .unwrap();

    assert_eq!(observed, (Vec::new(), None));
    assert_eq!(observed_before_retry, (Vec::new(), None));
    assert_eq!(
        observed_before_retry_2,
        Some(JanitorReason::CoordinatorBusy)
    );
    assert_eq!(bytes_before_retry, original);
}

#[cfg(unix)]
#[test]
fn controlled_selection_lock_io_error_preserves_the_cursor_and_local_mutex() {
    let parent = tempfile::tempdir().unwrap();
    let mut coordinator = coordinator_in(&parent);
    let original_bytes = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let original = std::mem::replace(&mut coordinator.file, unsupported_lock_file());
    let mut scan = JanitorScan::default();
    let remaining: fn() -> Duration = || Duration::from_secs(1);

    let error = select_reclaim_candidates_controlled_with_hooks(
        &coordinator,
        &mut scan,
        &remaining,
        &<() as Default>::default,
    )
    .unwrap_err();

    let failure_was_observed = coordinator.local_lock.try_lock().is_ok();
    let bytes_before_retry = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    coordinator.file = original;

    let observed = select_reclaim_candidates_controlled_with_hooks(
        &coordinator,
        &mut scan,
        &remaining,
        &<() as Default>::default,
    )
    .unwrap();

    assert_eq!(observed, (Vec::new(), None));
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "lock coordinator",
            ..
        }
    ));
    assert!(failure_was_observed);
    assert_eq!(bytes_before_retry, original_bytes);
}

#[test]
fn controlled_selection_unlock_error_preserves_the_cursor_and_releases_the_lock() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let mut scan = JanitorScan::default();
    let remaining: fn() -> Duration = || Duration::from_secs(1);

    let fault = io_faults::arm("coordinator.unlock");

    let error = select_reclaim_candidates_controlled_with_hooks(
        &coordinator,
        &mut scan,
        &remaining,
        &<() as Default>::default,
    )
    .unwrap_err();
    let fault_was_triggered = fault.triggered();

    let initial_diagnostic_matched = error.to_string().contains("coordinator.unlock");
    let bytes_before_retry = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    drop(fault);
    let independent = File::options()
        .read(true)
        .write(true)
        .open(coordinator.path.join(COORDINATOR_FILE))
        .unwrap();
    File::try_lock(&independent).unwrap();
    File::unlock(&independent).unwrap();

    let observed = select_reclaim_candidates_controlled_with_hooks(
        &coordinator,
        &mut scan,
        &remaining,
        &<() as Default>::default,
    )
    .unwrap();

    assert_eq!(observed, (Vec::new(), None));
    assert!(fault_was_triggered);
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "unlock coordinator",
            ..
        }
    ));
    assert!(initial_diagnostic_matched);
    assert_eq!(bytes_before_retry, original);
}

struct MutexAttempt {
    result: Result<(), WorkspaceError>,
    excluded_contender: bool,
    registry_empty: bool,
}

fn observe_mutex_attempt<T>(
    result: Result<T, WorkspaceError>,
    inspect: impl FnOnce(&T) -> (bool, bool),
) -> MutexAttempt {
    match result {
        Ok(guard) => {
            let (excluded_contender, registry_empty) = inspect(&guard);
            drop(guard);
            MutexAttempt {
                result: Ok(()),
                excluded_contender,
                registry_empty,
            }
        }
        Err(error) => MutexAttempt {
            result: Err(error),
            excluded_contender: false,
            registry_empty: false,
        },
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 1024, failure_persistence: None, ..proptest::test_runner::Config::default() })]

    #[test]
    fn native_mutex_registry_deadline_preserves_guard_ownership_and_retry(
        nanos in 0u64..1_000,
        held in proptest::arbitrary::any::<bool>(),
    ) {
        let registry: LocalCoordinatorRegistry = Mutex::new(HashMap::new());
        let held_guard = held.then(|| registry.lock().unwrap());
        let path = Utf8Path::new("registry-property");
        let inspect = |guard: &LocalCoordinatorRegistryGuard<'_>| (matches!(registry.try_lock(), Err(std::sync::TryLockError::WouldBlock)), guard.is_empty());

        let initial = observe_mutex_attempt(lock_registry_until(&registry, path, std::time::Instant::now() + Duration::from_nanos(nanos)), inspect);
        drop(held_guard);
        let retry = observe_mutex_attempt(lock_registry_until(&registry, path, std::time::Instant::now() + Duration::from_secs(1)), inspect);

        proptest::prop_assert!(!held || initial.result.is_err());
        proptest::prop_assert!(initial.result.is_err() || initial.excluded_contender);
        proptest::prop_assert!(initial.result.is_err() || initial.registry_empty);
        proptest::prop_assert!(initial.result.as_ref().err().is_none_or(|error| matches!(error, WorkspaceError::Io { operation: "lock coordinator registry", path: actual, .. } if actual.as_path() == path)), "unexpected mutex error context");
        proptest::prop_assert!(initial.result.as_ref().err().is_none_or(|error| error.to_string().contains("deadline")));
        proptest::prop_assert!(retry.result.is_ok());
        proptest::prop_assert!(retry.excluded_contender);
        proptest::prop_assert!(retry.registry_empty);
    }

    #[test]
    fn native_mutex_local_deadline_preserves_guard_ownership_and_retry(
        nanos in 0u64..1_000,
        held in proptest::arbitrary::any::<bool>(),
    ) {
        let lock = Mutex::new(());
        let held_guard = held.then(|| lock.lock().unwrap());
        let inspect = |_: &std::sync::MutexGuard<'_, ()>| (matches!(lock.try_lock(), Err(std::sync::TryLockError::WouldBlock)), true);

        let initial = observe_mutex_attempt(lock_local_until(&lock, std::time::Instant::now() + Duration::from_nanos(nanos)), inspect);
        drop(held_guard);
        let retry = observe_mutex_attempt(lock_local_until(&lock, std::time::Instant::now() + Duration::from_secs(1)), inspect);

        proptest::prop_assert!(!held || initial.result.is_err());
        proptest::prop_assert!(initial.result.is_err() || initial.excluded_contender);
        proptest::prop_assert!(initial.result.as_ref().err().is_none_or(|error| matches!(error, WorkspaceError::Io { operation: "lock coordinator bootstrap", path, .. } if path.as_path() == Utf8Path::new(COORDINATOR_FILE))), "unexpected mutex error context");
        proptest::prop_assert!(initial.result.as_ref().err().is_none_or(|error| error.to_string().contains("deadline")));
        proptest::prop_assert!(retry.result.is_ok());
        proptest::prop_assert!(retry.excluded_contender);
    }
}

#[rstest::rstest]
#[case(1)]
#[case(2)]
fn empty_root_removal_cancellation_preserves_identity_before_unlink(#[case] stop_at: usize) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    let candidate = create_owned_directory(&coordinator.dir, &name).unwrap();
    let identity = directory_identity(&candidate).unwrap();
    let checks = std::cell::Cell::new(0);
    let stopped = || {
        checks.set(checks.get() + 1);
        checks.get() >= stop_at
    };
    let mut progress = DeletionProgress::default();

    let error = remove_empty_unmarked_directory(
        &coordinator.dir,
        &name,
        candidate,
        identity,
        std::time::Instant::now(),
        CleanupBudget {
            duration: Duration::from_secs(60),
            stopped: Some(&stopped),
        },
        &mut progress,
    )
    .unwrap_err();

    let observed_before_retry = checks.get();
    let removed_before_retry = progress.removed_entries;
    let failure_was_observed = !progress.root_removed;
    let namespace_identity_after_failure =
        metadata_identity(&coordinator.dir.symlink_metadata(&name).unwrap());
    let retry = open_owned_directory(&coordinator.dir, &name).unwrap();
    remove_empty_unmarked_directory(
        &coordinator.dir,
        &name,
        retry,
        identity,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut progress,
    )
    .unwrap();

    assert!(progress.root_removed);
    assert_eq!(progress.removed_entries, 1);
    assert!(!coordinator.dir.exists(&name));
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(observed_before_retry, stop_at);
    assert_eq!(removed_before_retry, 0);
    assert!(failure_was_observed);
    assert_eq!(namespace_identity_after_failure, identity);
}

#[rstest::rstest]
#[case::identity_mismatch(true, |_: &cap_std::fs::Dir| {}, None)]
#[case::nonempty(false, |candidate: &cap_std::fs::Dir| candidate.write("payload", b"keep").unwrap(), Some(b"keep".as_slice()))]
fn empty_root_removal_rejects_unverified_or_nonempty_candidates_without_progress(
    #[case] mismatch: bool,
    #[case] configure: fn(&cap_std::fs::Dir),
    #[case] payload: Option<&[u8]>,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    let candidate = create_owned_directory(&coordinator.dir, &name).unwrap();
    configure(&candidate);
    let identity = directory_identity(&candidate).unwrap();
    let expected = [identity, (identity.0, identity.1.wrapping_add(1))][usize::from(mismatch)];
    let mut progress = DeletionProgress::default();

    let error = remove_empty_unmarked_directory(
        &coordinator.dir,
        &name,
        candidate,
        expected,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut progress,
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "empty deleting root changed before removal"
    );
    assert_eq!(progress.removed_entries, 0);
    assert!(!progress.root_removed);
    assert_eq!(
        metadata_identity(&coordinator.dir.symlink_metadata(&name).unwrap()),
        identity
    );
    assert_eq!(
        coordinator
            .dir
            .read(format!("{name}/payload"))
            .ok()
            .as_deref(),
        payload
    );
}

#[cfg(unix)]
#[test]
fn empty_root_identity_io_failure_preserves_the_payload_and_allows_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    let candidate = create_owned_directory(&coordinator.dir, &name).unwrap();
    candidate.write("payload", b"keep-identity-error").unwrap();
    let identity = directory_identity(&candidate).unwrap();
    let mut progress = DeletionProgress::default();

    let fault = io_faults::arm("directory.identity_metadata");

    let error = remove_empty_unmarked_directory(
        &coordinator.dir,
        &name,
        candidate,
        identity,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut progress,
    )
    .unwrap_err();
    let fault_was_triggered = fault.triggered();

    let removed_before_retry = progress.removed_entries;
    let failure_was_observed = !progress.root_removed;
    let bytes_before_retry = coordinator.dir.read(format!("{name}/payload")).unwrap();
    drop(fault);
    let retry = open_owned_directory(&coordinator.dir, &name).unwrap();
    retry.remove_file("payload").unwrap();
    remove_empty_unmarked_directory(
        &coordinator.dir,
        &name,
        retry,
        identity,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut progress,
    )
    .unwrap();

    assert_eq!(progress.removed_entries, 1);
    assert!(progress.root_removed);
    assert!(fault_was_triggered);
    assert!(error.to_string().contains("directory.identity_metadata"));
    assert_eq!(removed_before_retry, 0);
    assert!(failure_was_observed);
    assert_eq!(bytes_before_retry, b"keep-identity-error");
}

#[test]
fn child_admission_stops_after_owner_cleanup_defers_for_a_live_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let child = root.create_child("worker-").unwrap();
    child
        .dir
        .as_ref()
        .unwrap()
        .write("payload", b"keep-live-child")
        .unwrap();
    let entries = root.dir.entries().unwrap().count();
    let deferred = root.cleanup(Duration::from_secs(1));
    let disposition_before_retry = deferred.disposition;
    let removed_before_retry = deferred.evidence.removed_entries;

    let error = root.create_child("worker-").unwrap_err();

    let initial_diagnostic_matched = error
        .to_string()
        .contains("managed root cleanup has started");
    let live_children_after_failure = root.lifecycle.lock().unwrap().live_children;
    let entries_before_retry = root.dir.entries().unwrap().count();
    let bytes_before_retry = child.dir.as_ref().unwrap().read("payload").unwrap();
    let bytes_before_retry_expected = b"keep-live-child";
    drop(child);
    let retry = root.cleanup(Duration::from_secs(1));

    assert_eq!(retry.disposition, CleanupDisposition::Clean);
    assert_eq!(retry.evidence.removed_entries, 5);
    assert!(!root.path.exists());
    assert_eq!(disposition_before_retry, CleanupDisposition::Deferred(None));
    assert_eq!(removed_before_retry, 0);
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create managed child",
            ..
        }
    ));
    assert!(initial_diagnostic_matched);
    assert_eq!(live_children_after_failure, 1);
    assert_eq!(entries_before_retry, entries);
    assert_eq!(bytes_before_retry, bytes_before_retry_expected);
}

#[test]
fn child_construction_rolls_back_when_the_parent_lease_mutex_is_poisoned() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original = root.dir.read(LEASE_FILE).unwrap();
    let controls = root.dir.entries().unwrap().count();
    poison(&root.lease);
    let retry_outcome = matches!(
        root.create_child("worker-"),
        Err(WorkspaceError::StatePoisoned)
    );

    assert!(retry_outcome);
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), controls);
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn owner_claim_lease_mutex_poison_preserves_active_protection_and_unlocks_the_coordinator() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir.write("payload", b"keep-poisoned-lease").unwrap();
    poison(&root.lease);

    let record = root.cleanup(Duration::from_secs(1));

    let namespace_after_failure = NamespaceLifecycle::load(&root.namespace);
    let bytes_before_retry = root.dir.read("payload").unwrap();
    let bytes_before_retry_expected = b"keep-poisoned-lease";
    let lease = File::open(root.path.join(LEASE_FILE)).unwrap();
    let initial_result_matched = matches!(
        File::try_lock(&lease),
        Err(std::fs::TryLockError::WouldBlock)
    );
    let coordinator_file = File::options()
        .read(true)
        .write(true)
        .open(coordinator.path.join(COORDINATOR_FILE))
        .unwrap();
    File::try_lock(&coordinator_file).expect("claim failure must release the coordinator lock");
    File::unlock(&coordinator_file).unwrap();

    assert_eq!(record.disposition, CleanupDisposition::Failed);
    assert_eq!(record.evidence.removed_entries, 0);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("poisoned"))
    );
    assert_eq!(namespace_after_failure, NamespaceLifecycle::Active);
    assert_eq!(bytes_before_retry, bytes_before_retry_expected);
    assert!(initial_result_matched);
}

#[rstest::rstest]
#[case("")]
#[case("-")]
#[case("worker")]
#[case("bad/name-")]
#[case("../worker-")]
#[case("é-")]
fn child_admission_rejects_invalid_prefixes_before_opening_or_modifying_the_namespace(
    #[case] prefix: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original = root.dir.read(LEASE_FILE).unwrap();
    let controls = root.dir.entries().unwrap().count();

    let fault = io_faults::arm("owned.entries_open");

    let error = root.create_child(prefix).unwrap_err();

    let fault_was_not_consumed = !fault.triggered();
    let live_children_after_failure = root.lifecycle.lock().unwrap().live_children;
    let entries_before_retry = root.dir.entries().unwrap().count();
    let bytes_before_retry = root.dir.read(LEASE_FILE).unwrap();
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();

    assert!(matches!(&error, WorkspaceError::InvalidPath { path } if path.as_str() == prefix));
    assert!(fault_was_not_consumed);
    assert_eq!(live_children_after_failure, 0);
    assert_eq!(entries_before_retry, controls);
    assert_eq!(bytes_before_retry, original);
}

#[cfg(target_os = "macos")]
#[test]
fn child_admission_accepts_the_apfs_component_length_boundary() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    // The UUID occupies 36 of APFS's 255 ASCII component bytes.
    let prefix = format!("{}-", "x".repeat(218));
    let child = root.create_child(&prefix).unwrap();
    let observed_before_retry = child.name.len();
    let live_children_after_failure = root.lifecycle.lock().unwrap().live_children;
    let observed_before_retry_3 = directory_identity(child.dir.as_ref().unwrap()).unwrap();
    let namespace_identity_after_failure =
        metadata_identity(&root.dir.symlink_metadata(&child.name).unwrap());
    child.cleanup().unwrap();

    assert!(!child.path.exists());
    assert_eq!(observed_before_retry, 255);
    assert_eq!(live_children_after_failure, 1);
    assert_eq!(observed_before_retry_3, namespace_identity_after_failure);
}

#[cfg(target_os = "macos")]
#[test]
fn child_admission_component_overflow_leaves_no_directory_or_live_child() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let original = root.dir.read(LEASE_FILE).unwrap();
    let prefix = format!("{}-", "x".repeat(219));

    let error = root.create_child(&prefix).unwrap_err();

    let live_children_after_failure = root.lifecycle.lock().unwrap().live_children;
    let entries_before_retry = root.dir.entries().unwrap().count();
    let bytes_before_retry = root.dir.read(LEASE_FILE).unwrap();
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();

    assert!(
        matches!(&error, WorkspaceError::Io { operation: "create managed child", path, .. } if path == &root.path)
    );
    assert_eq!(live_children_after_failure, 0);
    assert_eq!(entries_before_retry, controls);
    assert_eq!(bytes_before_retry, original);
}

#[cfg(target_os = "macos")]
#[test]
fn cleanup_mount_guard_rejects_an_existing_distinct_mount_without_mutation() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-mount-boundary").unwrap();
    let devices = cap_std::fs::Dir::open_ambient_dir("/dev", cap_std::ambient_authority()).unwrap();
    let observed_before_retry = macos_directory_mount_identity(&dir).unwrap();

    let error = ensure_same_cleanup_mount(
        &dir,
        &devices,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "cleanup refuses to cross a mount boundary"
    );
    assert_eq!(dir.read("payload").unwrap(), b"keep-mount-boundary");
    assert_eq!(dir.entries().unwrap().count(), 1);
    assert_ne!(
        observed_before_retry,
        macos_directory_mount_identity(&devices).unwrap()
    );
}

#[cfg(target_os = "macos")]
#[rstest::rstest]
#[case(1)]
#[case(2)]
#[case(3)]
fn cleanup_mount_cancellation_at_each_checkpoint_preserves_payload(#[case] stop_at: usize) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-mount-cancellation").unwrap();
    let checks = std::cell::Cell::new(0);
    let stopped = || {
        checks.set(checks.get() + 1);
        checks.get() >= stop_at
    };

    let error = ensure_same_cleanup_mount(
        &dir,
        &dir,
        std::time::Instant::now(),
        CleanupBudget {
            duration: Duration::from_secs(60),
            stopped: Some(&stopped),
        },
    )
    .unwrap_err();

    let observed_before_retry = checks.get();
    let bytes_before_retry = dir.read("payload").unwrap();
    ensure_same_cleanup_mount(
        &dir,
        &dir,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap();

    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(observed_before_retry, stop_at);
    assert_eq!(bytes_before_retry, b"keep-mount-cancellation");
}

#[cfg(target_os = "macos")]
#[rstest::rstest]
#[case::parent(0)]
#[case::child(1)]
fn cleanup_mount_guard_preserves_the_native_error_from_either_descriptor(#[case] role: usize) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-mount-error").unwrap();
    let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    let descriptor: std::os::fd::OwnedFd = stream.into();
    let socket = cap_std::fs::Dir::from_std_file(File::from(descriptor));
    let (parent_dir, child_dir) = [(&socket, &dir), (&dir, &socket)][role];

    let error = ensure_same_cleanup_mount(
        parent_dir,
        child_dir,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap_err();

    assert_ne!(
        error
            .raw_os_error()
            .expect("fstatfs failures retain their native cause"),
        0
    );
    assert_eq!(dir.read("payload").unwrap(), b"keep-mount-error");
    assert_eq!(dir.entries().unwrap().count(), 1);
}

#[rstest::rstest]
#[case::entry(|dir: &cap_std::fs::Dir, name: &std::ffi::OsStr| inspect_cleanup_entry(dir, name).map(drop))]
#[case::owned_identity(|dir: &cap_std::fs::Dir, name: &std::ffi::OsStr| cleanup_entry_identity_if_owned(dir, name).map(drop))]
fn cleanup_inspection_missing_name_preserves_existing_payload(
    #[case] inspect: fn(&cap_std::fs::Dir, &std::ffi::OsStr) -> std::io::Result<()>,
) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-missing-inspection").unwrap();
    let identity = directory_identity(&dir).unwrap();

    let error = inspect(&dir, std::ffi::OsStr::new("missing")).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(directory_identity(&dir).unwrap(), identity);
    assert_eq!(dir.read("payload").unwrap(), b"keep-missing-inspection");
    assert_eq!(dir.entries().unwrap().count(), 1);
}

#[test]
fn missing_new_coordinator_rollback_preserves_unrelated_payload() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-rollback-neighbor").unwrap();

    let error = remove_new_coordinator_if_identity(&dir, (0, 0)).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(dir.read("payload").unwrap(), b"keep-rollback-neighbor");
    assert_eq!(dir.entries().unwrap().count(), 1);
}

#[test]
fn initially_expired_coordinator_initialization_does_not_create_a_file() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-before-initialization").unwrap();
    let path = Utf8Path::from_path(parent.path()).unwrap();

    let error = initialize_coordinator(&dir, path, std::time::Instant::now()).unwrap_err();

    let failure_was_observed = !dir.exists(COORDINATOR_FILE);
    let bytes_before_retry = dir.read("payload").unwrap();
    let retry = initialize_coordinator(&dir, path, deadline()).unwrap();

    assert_eq!(
        read_coordinator_state(&retry, &path.join(COORDINATOR_FILE))
            .unwrap()
            .1
            .generation,
        0
    );
    assert_eq!(dir.read("payload").unwrap(), b"keep-before-initialization");
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create coordinator",
            ..
        }
    ));
    assert!(error.to_string().contains("deadline"));
    assert!(failure_was_observed);
    assert_eq!(bytes_before_retry, b"keep-before-initialization");
}

#[rstest::rstest]
#[case::invalid_name("../escape")]
#[case::oversized(&"x".repeat(COORDINATOR_CURSOR_BYTES + 1))]
fn cursor_persistence_rejects_invalid_names_before_writing_the_coordinator(#[case] cursor: &str) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let path = coordinator.path.join(COORDINATOR_FILE);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let (slot, state) = read_coordinator_state(&coordinator.file, &path).unwrap();

    let error =
        persist_coordinator_cursor(&coordinator.file, &path, slot, &state, cursor).unwrap_err();

    assert!(matches!(&error, WorkspaceError::InvalidPath { path } if path.as_str() == cursor));
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
    assert_eq!(
        read_coordinator_state(&coordinator.file, &path)
            .unwrap()
            .1
            .generation,
        state.generation
    );
}

#[test]
fn expired_unpublished_directory_rollback_keeps_the_primary_error_and_original_tree() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let candidate = create_owned_directory(&dir, "candidate").unwrap();
    candidate
        .write("payload", b"keep-expired-rollback")
        .unwrap();
    let identity = directory_identity(&candidate).unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join("candidate");
    let primary = WorkspaceError::io("create managed child", &path, "primary constructor failure");

    let error = error_after_unpublished_directory_failure(
        &dir,
        "candidate",
        candidate,
        Some(identity),
        std::time::Instant::now(),
        &path,
        primary,
    );

    let bytes_before_retry = dir.read("candidate/payload").unwrap();
    let namespace_identity_after_failure =
        metadata_identity(&dir.symlink_metadata("candidate").unwrap());
    rollback_new_owned_directory(&dir, "candidate", identity, Duration::from_secs(1)).unwrap();

    assert!(!dir.exists("candidate"));
    assert!(
        matches!(&error, WorkspaceError::Io { operation: "create managed child", path: actual, .. } if actual == &path)
    );
    assert!(
        error
            .to_string()
            .contains("primary constructor failure; secondary new-directory rollback failure")
    );
    assert!(
        error
            .to_string()
            .contains("new-directory rollback deadline exhausted before cleanup")
    );
    assert_eq!(bytes_before_retry, b"keep-expired-rollback");
    assert_eq!(namespace_identity_after_failure, identity);
}

#[rstest::rstest]
#[case::stopped(|| true, Duration::from_secs(30), 0, 0, Some(JanitorReason::SelectionDeadline))]
#[case::expired(|| false, Duration::ZERO, 0, 0, Some(JanitorReason::SelectionDeadline))]
#[case::active_lease(|| false, Duration::from_secs(30), 1, 1, None)]
fn controlled_reclamation_preserves_active_payload_for_stop_and_deadline_states(
    #[case] stopped: fn() -> bool,
    #[case] budget: Duration,
    #[case] inspected: u64,
    #[case] preserved: u64,
    #[case] reason: Option<JanitorReason>,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = coordinator_in(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir.write("payload", b"keep-controlled-root").unwrap();
    let lease = root.dir.read(LEASE_FILE).unwrap();

    let report = ManagedRunRoot::reclaim_abandoned_controlled(
        &coordinator,
        SystemTime::now(),
        tokio::time::Instant::now() + budget,
        &stopped,
    );

    assert_eq!(report.inspected_roots, inspected);
    assert_eq!(report.preserved_roots, preserved);
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.unresolved_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert_eq!(report.scan.stop_reason, reason);
    assert_eq!(root.dir.read("payload").unwrap(), b"keep-controlled-root");
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), lease);
    assert!(root.path.is_dir());
}
