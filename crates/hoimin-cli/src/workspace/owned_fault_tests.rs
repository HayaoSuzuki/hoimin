use super::*;

fn setup(parent: &tempfile::TempDir) -> ManagedRootCoordinator {
    ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap()
}

#[cfg(unix)]
#[test]
fn constructor_io_failures_roll_back_every_verified_staging_or_published_root() {
    for (point, operation) in [
        ("staging.inspect_opened", "identify staging root"),
        ("lease.create", "create lease marker"),
        ("lease.write", "write lease marker"),
        ("lease.newline", "write lease marker"),
        ("lease.flush", "flush lease marker"),
        ("lease.lock", "lock lease marker"),
        ("heartbeat.create", "create heartbeat marker"),
        ("heartbeat.write", "write heartbeat marker"),
        ("heartbeat.newline", "write heartbeat marker"),
        ("heartbeat.flush", "flush heartbeat marker"),
        ("staging.inspect_before_publish", "identify staging root"),
        ("active.open", "open published root"),
        ("active.inspect", "identify published root"),
        ("coordinator.clone", "clone coordinator"),
        ("coordinator.dir_clone", "clone managed root"),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let fault = io_faults::arm(point);
        let result = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution);
        assert!(fault.triggered(), "boundary {point} was not reached");
        let error = result.unwrap_err();
        assert!(
            matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation),
            "{point}: {error}"
        );
        // TryLockError's Display omits its inner I/O message.
        if point != "lease.lock" {
            assert!(error.to_string().contains(point), "{point}: {error}");
        }
        drop(fault);
        ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
        assert_eq!(
            fs::read_dir(&coordinator.path).unwrap().count(),
            1,
            "{point} left an unreclaimed root"
        );
        let recovered = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        assert_eq!(
            recovered.cleanup(Duration::from_secs(1)).disposition,
            CleanupDisposition::Clean
        );
    }
}

#[test]
fn cleanup_ready_io_failures_leave_the_lease_live_and_retry_publishes_valid_evidence() {
    for (point, operation, partial) in [
        (
            "ready.inspect_partial",
            "open partial workspace marker",
            true,
        ),
        (
            "ready.partial_identity",
            "identify partial workspace marker",
            true,
        ),
        (
            "ready.remove_partial",
            "remove partial workspace marker",
            true,
        ),
        ("ready.create", "create workspace marker", false),
        ("ready.write", "write workspace marker", false),
        ("ready.newline", "write workspace marker", false),
        ("ready.flush", "flush workspace marker", false),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery).unwrap();
        if partial {
            root.dir
                .write(CLEANUP_READY_FILE, b"interrupted marker")
                .unwrap();
        }
        let fault = io_faults::arm(point);
        let error = root.mark_cleanup_ready().unwrap_err();
        assert!(fault.triggered(), "boundary {point} was not reached");
        assert!(
            matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation),
            "{point}: {error}"
        );
        assert!(error.to_string().contains(point));
        assert!(!root.lifecycle.lock().unwrap().cleanup_ready);
        assert!(root.lease.lock().unwrap().is_some());
        assert!(root.path.exists());
        drop(fault);
        root.mark_cleanup_ready().unwrap();
        let ready = read_cleanup_ready_marker(&root.dir).unwrap();
        assert_eq!(ready.run_id, root.run_id);
        assert_eq!(ready.owner, OwnerKind::PublicDelivery);
        let lease = root.lease.lock().unwrap();
        assert_eq!(
            (ready.lease_device, ready.lease_inode),
            file_identity(lease.as_deref().unwrap()).unwrap()
        );
        drop(lease);
        assert!(root.lifecycle.lock().unwrap().cleanup_ready);
        assert_eq!(
            root.cleanup(Duration::from_secs(1)).disposition,
            CleanupDisposition::Clean
        );
    }
}

#[test]
fn retain_marker_write_failures_preserve_the_root_and_can_be_recovered_explicitly() {
    for (point, operation) in [
        ("control.write", "write workspace marker"),
        ("control.newline", "write workspace marker"),
        ("control.flush", "flush workspace marker"),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let fault = io_faults::arm(point);
        let error = root.retain().unwrap_err();
        assert!(fault.triggered());
        assert!(
            matches!(error, WorkspaceError::Io { operation: actual, .. } if actual == operation)
        );
        assert!(root.path.exists());
        assert!(root.lease.lock().unwrap().is_some());
        drop(fault);
        root.dir.remove_file(RETAIN_FILE).unwrap();
        root.retain().unwrap();
        let marker = read_bounded_json::<LeaseMarker>(&root.dir, RETAIN_FILE).unwrap();
        assert_eq!(marker.run_id, root.run_id);
        assert_eq!(
            root.cleanup(Duration::from_secs(1)).disposition,
            CleanupDisposition::Clean
        );
    }
}

#[test]
fn marker_writer_fault_is_one_shot_and_successful_bytes_are_not_lost() {
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("marker");
    let mut file = File::options()
        .create_new(true)
        .write(true)
        .open(&path)
        .unwrap();
    let mut writer = io_faults::Writer::new(&mut file, "writer.recovery");
    writer.write_all(b"before").unwrap();
    let fault = io_faults::arm("writer.recovery");
    assert!(writer.write_all(b"rejected").is_err());
    assert!(fault.triggered());
    writer.write_all(b"after").unwrap();
    writer.flush().unwrap();
    assert_eq!(fs::read(path).unwrap(), b"beforeafter");
}

#[test]
fn serializer_failure_before_any_write_does_not_consume_an_io_fault() {
    struct InvalidMarker;
    impl Serialize for InvalidMarker {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("marker rejected before writing"))
        }
    }
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let fault = io_faults::arm("control.write");
    let error = root
        .write_json_marker("invalid-marker", &InvalidMarker)
        .unwrap_err();
    assert!(error.to_string().contains("marker rejected before writing"));
    assert!(!fault.triggered());
    drop(fault);
    root.retain().unwrap();
}

#[test]
fn bootstrap_io_failures_preserve_the_namespace_and_allow_a_fresh_coordinator_retry() {
    for (point, operation) in [
        ("bootstrap.canonicalize", "canonicalize managed-root parent"),
        ("bootstrap.open_parent", "open managed-root parent"),
        ("bootstrap.create_root", "create managed root"),
        ("bootstrap.inspect_root", "inspect managed root"),
        ("bootstrap.open_root", "open managed root"),
        ("bootstrap.root_identity", "identify managed root"),
        ("bootstrap.root_inspect", "inspect managed root"),
        ("bootstrap.root_verify", "verify managed root"),
        ("bootstrap.coordinator_inspect", "inspect coordinator"),
        ("bootstrap.coordinator_verify", "verify coordinator"),
        ("bootstrap.coordinator_create", "create coordinator"),
        ("bootstrap.coordinator_write", "initialize coordinator"),
        ("bootstrap.coordinator_flush", "flush coordinator"),
        ("bootstrap.coordinator_unlock", "unlock coordinator"),
        ("bootstrap.validate_metadata", "inspect coordinator"),
        ("bootstrap.validate_seek", "seek coordinator"),
        ("bootstrap.validate_read", "read coordinator"),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let fault = io_faults::arm(point);
        let error =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap_err();
        assert!(fault.triggered(), "boundary {point} was not reached");
        assert!(
            matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation),
            "{point}: {error}"
        );
        assert!(error.to_string().contains(point));
        drop(fault);
        let coordinator = setup(&parent);
        assert_eq!(fs::read_dir(&coordinator.path).unwrap().count(), 1);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        assert_eq!(
            root.cleanup(Duration::from_secs(1)).disposition,
            CleanupDisposition::Clean
        );
    }
}

#[test]
fn unverified_new_coordinator_identity_is_not_adopted_or_removed() {
    let parent = tempfile::tempdir().unwrap();
    let fault = io_faults::arm("bootstrap.coordinator_new_identity");
    let error =
        ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        error,
        WorkspaceError::Io {
            operation: "identify new coordinator",
            ..
        }
    ));
    drop(fault);
    let managed = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(MANAGED_DIR);
    let coordinator_path = managed.join(COORDINATOR_FILE);
    assert_eq!(fs::metadata(&coordinator_path).unwrap().len(), 0);
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&managed, cap_std::ambient_authority()).unwrap();
    let retry = open_initialized_coordinator_until(
        &directory,
        &coordinator_path,
        std::time::Instant::now() + Duration::from_millis(20),
    );
    assert!(retry.is_err());
    assert_eq!(fs::metadata(&coordinator_path).unwrap().len(), 0);
    directory.remove_file(COORDINATOR_FILE).unwrap();
    setup(&parent);
}

#[cfg(unix)]
#[test]
fn coordinator_unlock_failures_before_and_after_publication_leave_no_orphaned_root() {
    for skip in [0, 1] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let fault = io_faults::arm_after("coordinator.unlock", skip);
        let error = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap_err();
        assert!(fault.triggered());
        assert!(matches!(
            error,
            WorkspaceError::Io {
                operation: "unlock coordinator",
                ..
            }
        ));
        drop(fault);
        ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
        assert_eq!(fs::read_dir(&coordinator.path).unwrap().count(), 1);
        assert!(
            CoordinatorLockGuard::try_acquire(&coordinator)
                .unwrap()
                .is_some()
        );
    }
}

#[cfg(unix)]
#[test]
fn owner_cleanup_io_failures_retain_exact_progress_and_resume_the_same_root() {
    for point in [
        "owner.root_identity",
        "claim.candidate_identity",
        "claim.expected_identity",
        "claim.lease_identity",
        "claim.stat_source",
        "claim.rename",
        "cleanup.inspect_entry",
        "cleanup.inspect_marker",
        "cleanup.stat_directory",
        "cleanup.open_directory",
        "cleanup.verify_directory",
        "cleanup.unlink_file",
        "cleanup.unlink_directory",
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        let leaf = if point == "cleanup.unlink_directory" {
            root.dir.create_dir("child").unwrap();
            "child"
        } else {
            root.dir.write("payload", b"preserved").unwrap();
            "payload"
        };
        let fault = io_faults::arm(point);
        let record = root.cleanup(Duration::from_secs(1));
        assert!(fault.triggered(), "boundary {point} was not reached");
        assert_eq!(
            record.disposition,
            CleanupDisposition::Failed,
            "{point}: {record:?}"
        );
        assert!(
            record
                .evidence
                .details
                .iter()
                .any(|detail| detail.contains(point)),
            "{point}: {record:?}"
        );
        let path = record.evidence.remaining_root.as_ref().unwrap();
        assert!(path.exists());
        let remaining = 1 + [leaf, LEASE_FILE, HEARTBEAT_FILE]
            .iter()
            .filter(|name| root.dir.exists(name))
            .count();
        assert_eq!(
            record.evidence.removed_entries,
            u64::try_from(4 - remaining).unwrap(),
            "{point}"
        );
        assert!(root.lease.lock().unwrap().is_some());
        drop(fault);
        let retry = root.cleanup(Duration::from_secs(1));
        assert_eq!(
            retry.disposition,
            CleanupDisposition::Clean,
            "{point}: {retry:?}"
        );
        assert_eq!(
            record.evidence.removed_entries + retry.evidence.removed_entries,
            4,
            "{point}"
        );
        assert!(!path.exists());
    }
}

#[cfg(unix)]
#[test]
fn janitor_identity_inspection_failure_retains_ready_root_and_retry_reclaims_it() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let fault = io_faults::arm("janitor.candidate_identity");
    let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert!(fault.triggered());
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::IdentityInspectionFailed],
        1
    );
    assert_eq!(report.removed_entries, 0);
    assert!(root.path.exists());
    drop(fault);
    let retry = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert_eq!(retry.reclaimed_roots, 1);
    assert!(!root.path.exists());
}

#[cfg(unix)]
#[test]
fn permissions_failures_preserve_existing_root_and_coordinator_then_retry_secures_them() {
    use std::os::unix::fs::PermissionsExt;
    for (point, operation, coordinator_file) in [
        ("bootstrap.root_permissions", "secure managed root", false),
        (
            "bootstrap.coordinator_permissions",
            "secure coordinator",
            true,
        ),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let managed = Utf8Path::from_path(parent.path())
            .unwrap()
            .join(MANAGED_DIR);
        let target = if coordinator_file {
            drop(setup(&parent));
            managed.join(COORDINATOR_FILE)
        } else {
            fs::create_dir(&managed).unwrap();
            managed.clone()
        };
        let insecure_mode = if coordinator_file { 0o644 } else { 0o755 };
        fs::set_permissions(&target, fs::Permissions::from_mode(insecure_mode)).unwrap();
        let fault = io_faults::arm(point);
        let error =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap_err();
        assert!(fault.triggered());
        assert!(
            matches!(error, WorkspaceError::Io { operation: actual, .. } if actual == operation)
        );
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            insecure_mode
        );
        drop(fault);
        drop(setup(&parent));
        let secure_mode = if coordinator_file { 0o600 } else { 0o700 };
        assert_eq!(
            fs::metadata(target).unwrap().permissions().mode() & 0o777,
            secure_mode
        );
    }
}

#[cfg(unix)]
#[test]
fn permission_verification_rejects_a_real_permission_race_after_successful_chmod() {
    use std::os::unix::fs::PermissionsExt;
    for (point, coordinator_file) in [
        ("bootstrap.root_verify", false),
        ("bootstrap.coordinator_verify", true),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let managed = Utf8Path::from_path(parent.path())
            .unwrap()
            .join(MANAGED_DIR);
        let target = if coordinator_file {
            drop(setup(&parent));
            managed.join(COORDINATOR_FILE)
        } else {
            fs::create_dir(&managed).unwrap();
            managed.clone()
        };
        let insecure_mode = if coordinator_file { 0o644 } else { 0o755 };
        fs::set_permissions(&target, fs::Permissions::from_mode(insecure_mode)).unwrap();
        let changed_path = target.clone();
        let race = io_faults::arm_action(point, 0, move || {
            fs::set_permissions(changed_path, fs::Permissions::from_mode(insecure_mode))
        });
        let result = ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap());
        assert!(race.triggered());
        assert!(matches!(result, Err(WorkspaceError::InvalidPath { .. })));
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            insecure_mode
        );
        drop(race);
        drop(setup(&parent));
        assert_eq!(
            fs::metadata(target).unwrap().permissions().mode() & 0o777,
            if coordinator_file { 0o600 } else { 0o700 }
        );
    }
}

#[cfg(unix)]
#[test]
fn managed_object_ownership_checks_reject_foreign_system_capabilities_before_chmod() {
    use cap_fs_ext::OsMetadataExt as _;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let foreign_dir =
        cap_std::fs::Dir::open_ambient_dir("/", cap_std::ambient_authority()).unwrap();
    let directory_metadata = foreign_dir.metadata(".").unwrap();
    // The permission suite runs unprivileged; assert this before attempting either secure API.
    assert_ne!(directory_metadata.uid(), unsafe { libc::geteuid() });
    let foreign_file = File::open("/dev/null").unwrap();
    let file_metadata = foreign_file.metadata().unwrap();
    assert_ne!(file_metadata.uid(), unsafe { libc::geteuid() });
    let before_mode = file_metadata.permissions().mode();
    assert_eq!(
        verify_current_user_owned_metadata(&directory_metadata)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        verify_current_user_owned_directory(&foreign_dir)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        verify_current_user_owned_file(&foreign_file)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert!(matches!(
        secure_managed_root(&foreign_dir, Utf8Path::new("foreign-root")),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(matches!(
        secure_coordinator_file(&foreign_file, Utf8Path::new("foreign-coordinator")),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(
        foreign_file.metadata().unwrap().permissions().mode(),
        before_mode
    );
}

#[cfg(unix)]
#[test]
fn retain_marker_inspection_error_never_reclaims_a_retained_cleanup_ready_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicDelivery).unwrap();
    root.dir
        .write("delivered-result", b"must remain retained")
        .unwrap();
    root.retain().unwrap();
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let fault = io_faults::arm("janitor.retain_stat");
    let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert!(fault.triggered());
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::CandidateInspectionFailed],
        1
    );
    assert!(report.details.iter().any(|detail| detail.reason
        == JanitorReason::CandidateInspectionFailed
        && detail.message.contains("janitor.retain_stat")));
    assert_eq!(
        root.dir.read("delivered-result").unwrap(),
        b"must remain retained"
    );
    assert!(root.path.exists());
    drop(fault);
    let retry = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert_eq!(retry.reclaimed_roots, 0);
    assert_eq!(retry.preserved_by_reason[&JanitorReason::RetainMarker], 1);
    assert!(root.path.exists());
    root.dir.remove_file(RETAIN_FILE).unwrap();
    let released = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert_eq!(released.reclaimed_roots, 1);
    assert!(!root.path.exists());
}

#[cfg(unix)]
#[test]
fn janitor_lease_and_claim_io_failures_hold_the_root_and_release_locks_for_retry() {
    for (point, reason) in [
        ("janitor.lease_stat", JanitorReason::LeaseInvalid),
        ("marker.stat", JanitorReason::LeaseInvalid),
        ("marker.open", JanitorReason::LeaseInvalid),
        ("marker.metadata", JanitorReason::LeaseInvalid),
        ("marker.identity", JanitorReason::LeaseInvalid),
        ("marker.owner", JanitorReason::LeaseInvalid),
        ("janitor.lease_lock", JanitorReason::LeaseLockFailed),
        ("janitor.lease_read", JanitorReason::MarkerInvalid),
        ("claim.candidate_identity", JanitorReason::ClaimFailed),
        ("claim.expected_identity", JanitorReason::ClaimFailed),
        ("claim.lease_identity", JanitorReason::ClaimFailed),
        ("claim.stat_source", JanitorReason::ClaimFailed),
        ("claim.rename", JanitorReason::ClaimFailed),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.dir.write("payload", b"preserved").unwrap();
        root.mark_cleanup_ready().unwrap();
        root.unlock_fixture_lease();
        let fault = io_faults::arm(point);
        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
        assert!(fault.triggered(), "{point}");
        assert_eq!(
            report.unresolved_by_reason[&reason], 1,
            "{point}: {report:?}"
        );
        assert_eq!(report.removed_entries, 0);
        assert!(root.path.exists());
        assert_eq!(root.dir.read("payload").unwrap(), b"preserved");
        drop(fault);
        let retry = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
        assert_eq!(retry.reclaimed_roots, 1, "{point}: {retry:?}");
        assert!(!root.path.exists());
    }
}

#[cfg(unix)]
#[test]
fn invalid_ready_io_evidence_falls_back_to_young_heartbeat_without_deleting_the_root() {
    for (point, diagnostic) in [
        ("janitor.ready_read", JanitorReason::MarkerInvalid),
        (
            "janitor.ready_identity",
            JanitorReason::IdentityInspectionFailed,
        ),
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.mark_cleanup_ready().unwrap();
        root.unlock_fixture_lease();
        let fault = io_faults::arm(point);
        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
        assert!(fault.triggered());
        assert_eq!(report.preserved_by_reason[&JanitorReason::Young], 1);
        assert!(
            report
                .details
                .iter()
                .any(|detail| detail.reason == diagnostic)
        );
        assert!(root.path.exists());
        drop(fault);
        assert_eq!(
            ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now()).reclaimed_roots,
            1
        );
    }
}

#[cfg(unix)]
#[test]
fn heartbeat_metadata_error_cannot_establish_staleness_and_retry_reclaims_an_old_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.unlock_fixture_lease();
    let future = SystemTime::now() + Duration::from_hours(25);
    let fault = io_faults::arm("janitor.heartbeat_metadata");
    let report = ManagedRunRoot::reclaim_abandoned(&coordinator, future);
    assert!(fault.triggered());
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::AgeUnverifiable],
        1
    );
    assert!(root.path.exists());
    drop(fault);
    assert_eq!(
        ManagedRunRoot::reclaim_abandoned(&coordinator, future).reclaimed_roots,
        1
    );
}

#[cfg(unix)]
#[test]
fn cursor_read_failures_prevent_selection_without_mutating_any_root() {
    for point in ["cursor.read_clone", "cursor.read_seek", "cursor.read"] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.mark_cleanup_ready().unwrap();
        root.unlock_fixture_lease();
        let before = fs::read(coordinator.path.join(COORDINATOR_FILE)).unwrap();
        let fault = io_faults::arm(point);
        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
        assert!(fault.triggered());
        assert_eq!(
            report.scan.stop_reason,
            Some(JanitorReason::SelectionFailed)
        );
        assert_eq!(report.inspected_roots, 0);
        assert_eq!(report.removed_entries, 0);
        assert_eq!(
            fs::read(coordinator.path.join(COORDINATOR_FILE)).unwrap(),
            before
        );
        assert!(root.path.exists());
        drop(fault);
        assert_eq!(
            ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now()).reclaimed_roots,
            1
        );
    }
}

#[cfg(unix)]
#[test]
fn cursor_persistence_failure_keeps_the_selected_root_cleanup_and_valid_slot_state() {
    for point in [
        "cursor.clone",
        "cursor.seek",
        "cursor.write",
        "cursor.flush",
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.mark_cleanup_ready().unwrap();
        root.unlock_fixture_lease();
        let fault = io_faults::arm(point);
        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
        assert!(fault.triggered());
        assert_eq!(report.reclaimed_roots, 1, "{point}: {report:?}");
        assert!(
            report
                .details
                .iter()
                .any(|detail| detail.reason == JanitorReason::CursorPersistenceFailed)
        );
        assert!(!root.path.exists());
        drop(fault);
        let (_, state) =
            read_coordinator_state(&coordinator.file, &coordinator.path.join(COORDINATOR_FILE))
                .unwrap();
        assert_eq!(state.generation, u64::from(point == "cursor.flush"));
        assert_eq!(
            ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now()).inspected_roots,
            0
        );
    }
}

#[test]
fn interrupted_cleanup_inspection_errors_do_not_publish_an_unverified_remaining_path() {
    for point in [
        "remaining.root_identity",
        "remaining.stat_candidate",
        "remaining.open",
        "remaining.candidate_identity",
    ] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.dir.write("payload", b"preserved").unwrap();
        let fault = io_faults::arm(point);
        let record = root.interrupted_cleanup_record(true, "interrupted wait".into());
        assert!(fault.triggered());
        assert_eq!(record.disposition, CleanupDisposition::Deferred(None));
        assert_eq!(record.evidence.remaining_root, None);
        assert!(
            record
                .evidence
                .details
                .iter()
                .any(|detail| detail.contains("integrity unverifiable") && detail.contains(point))
        );
        assert_eq!(root.dir.read("payload").unwrap(), b"preserved");
        assert!(root.path.exists());
        drop(fault);
        let verified = root.interrupted_cleanup_record(true, "retry evidence".into());
        assert_eq!(verified.disposition, CleanupDisposition::Failed);
        assert_eq!(verified.evidence.remaining_root, Some(root.path.clone()));
        assert_eq!(
            root.cleanup(Duration::from_secs(1)).disposition,
            CleanupDisposition::Clean
        );
    }
}

#[cfg(unix)]
#[rstest::rstest]
#[case("remaining.stat_candidate")]
#[case("remaining.open")]
fn interrupted_cleanup_path_validation_rejects_namespace_replacement_before_or_after_stat(
    #[case] point: &'static str,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir.write("payload", b"original").unwrap();
    let original_path = root.path.clone();
    let held = coordinator.path.join("held-original");
    let held_for_race = held.clone();
    let race = io_faults::arm_action(point, 0, move || {
        fs::rename(&original_path, held_for_race)
            .expect("park the original root at the race boundary");
        fs::create_dir(&original_path).expect("create the replacement root at the race boundary");
        fs::write(original_path.join("payload"), b"replacement")
            .expect("populate the replacement root");
        Ok(())
    });
    let record = root.interrupted_cleanup_record(true, "interrupted wait".into());
    assert!(race.triggered());
    assert_eq!(record.disposition, CleanupDisposition::Deferred(None));
    assert_eq!(record.evidence.remaining_root, None);
    assert!(
        record
            .evidence
            .details
            .iter()
            .any(|detail| detail.contains("identity"))
    );
    assert_eq!(fs::read(held.join("payload")).unwrap(), b"original");
    assert_eq!(fs::read(root.path.join("payload")).unwrap(), b"replacement");
}

#[cfg(unix)]
#[test]
fn retain_stat_error_reports_uncertainty_until_a_clean_absence_observation() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.mark_cleanup_ready().unwrap();
    root.unlock_fixture_lease();
    let fault = io_faults::arm("janitor.retain_stat");
    let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert!(fault.triggered());
    assert_eq!(
        report.unresolved_by_reason[&JanitorReason::CandidateInspectionFailed],
        1
    );
    assert!(
        !report
            .preserved_by_reason
            .contains_key(&JanitorReason::RetainMarker)
    );
    assert!(root.path.exists());
    drop(fault);
    assert_eq!(
        ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now()).reclaimed_roots,
        1
    );
}

#[test]
fn staging_entry_read_failure_preserves_the_entry_and_reports_unsafe_staging() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write(LEASE_FILE, b"preserved").unwrap();
    let fault = io_faults::arm("janitor.staging_entry");
    let failure = staging_contents_are_safe(&dir).err().unwrap();
    assert!(fault.triggered());
    assert_eq!(failure.reason, JanitorReason::UnsafeStaging);
    assert!(failure.cause.contains("janitor.staging_entry"));
    assert_eq!(dir.read(LEASE_FILE).unwrap(), b"preserved");
}

#[test]
fn empty_probe_entry_failure_propagates_without_certifying_empty() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"preserved").unwrap();
    let fault = io_faults::arm("janitor.empty_entry");
    let error = inspect_empty_directory(&dir).unwrap_err();
    assert!(fault.triggered());
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains("janitor.empty_entry"));
    assert_eq!(dir.read("payload").unwrap(), b"preserved");
}

#[rstest::rstest]
#[case("janitor.deleting_first")]
#[case("janitor.deleting_second")]
fn deleting_probe_entry_failure_propagates_without_certifying_a_safe_root(
    #[case] point: &'static str,
) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write(LEASE_FILE, b"preserved").unwrap();
    let fault = io_faults::arm(point);
    let error = inspect_deleting_contents(&dir).unwrap_err();
    assert!(fault.triggered());
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains(point));
    assert_eq!(dir.read(LEASE_FILE).unwrap(), b"preserved");
}

#[rstest::rstest]
#[case("reopen.stat", "inspect coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.open", "open coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.identity", "identify coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.metadata", "inspect coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.unlock_empty", "unlock coordinator", |dir: &cap_std::fs::Dir| dir.write(COORDINATOR_FILE, b"").unwrap())]
#[case("reopen.unlock_valid", "unlock coordinator", |_: &cap_std::fs::Dir| {})]
fn reopening_coordinator_io_errors_preserve_bytes_and_release_locks_for_retry(
    #[case] point: &'static str,
    #[case] operation: &str,
    #[case] configure: fn(&cap_std::fs::Dir),
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    configure(&coordinator.dir);
    let path = coordinator.path.join(COORDINATOR_FILE);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let fault = io_faults::arm(point);
    let error = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation));
    assert!(error.to_string().contains(point));
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
    drop(fault);
    coordinator
        .dir
        .write(COORDINATOR_FILE, initial_coordinator_bytes())
        .unwrap();
    let file = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        read_coordinator_state(&file, &path).unwrap().1.generation,
        0
    );
    File::try_lock(&file).unwrap();
    File::unlock(&file).unwrap();
}

#[rstest::rstest]
#[case("marker.clone")]
#[case("marker.seek")]
#[case("marker.read")]
fn marker_reader_io_failure_preserves_bytes_and_allows_a_retry(#[case] point: &'static str) {
    let parent = tempfile::tempdir().unwrap();
    let path = parent.path().join("marker");
    let marker = LeaseMarker {
        schema: LEASE_SCHEMA,
        run_id: "sentinel-run".into(),
        created_unix_seconds: 123,
        owner: OwnerKind::PublicExecution,
    };
    let bytes = serde_json::to_vec(&marker).unwrap();
    fs::write(&path, &bytes).unwrap();
    let file = File::open(&path).unwrap();
    let fault = io_faults::arm(point);
    let error = read_json_file_checked::<LeaseMarker>(&file).unwrap_err();
    assert!(fault.triggered());
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains(point));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    drop(fault);
    assert_eq!(
        read_json_file_checked::<LeaseMarker>(&file).unwrap().run_id,
        marker.run_id
    );
}

#[test]
fn initial_marker_clock_failure_rolls_back_the_unpublished_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let fault = io_faults::arm("lease.time");
    let error = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "read system time",
            ..
        }
    ));
    assert!(error.to_string().contains("lease.time"));
    assert_eq!(fs::read_dir(&coordinator.path).unwrap().count(), 1);
    drop(fault);
    let retry = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    assert_eq!(
        retry.cleanup(Duration::from_secs(1)).disposition,
        CleanupDisposition::Clean
    );
}

#[test]
fn retention_clock_failure_leaves_no_marker_and_can_be_retried() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let fault = io_faults::arm("control.time");
    let error = root.retain().unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "read system time",
            ..
        }
    ));
    assert!(error.to_string().contains("control.time"));
    assert!(!root.dir.exists(RETAIN_FILE));
    assert!(root.path.exists());
    drop(fault);
    root.retain().unwrap();
    assert_eq!(
        read_bounded_json::<LeaseMarker>(&root.dir, RETAIN_FILE)
            .unwrap()
            .run_id,
        root.run_id
    );
}

#[test]
fn staging_directory_creation_error_leaves_no_unpublished_root_and_can_be_retried() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let fault = io_faults::arm("staging.create_directory");
    let error = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create staging root",
            ..
        }
    ));
    assert!(error.to_string().contains("staging.create_directory"));
    assert_eq!(fs::read_dir(&coordinator.path).unwrap().count(), 1);
    drop(fault);
    let retry = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    assert_eq!(
        retry.cleanup(Duration::from_secs(1)).disposition,
        CleanupDisposition::Clean
    );
}

#[cfg(unix)]
#[test]
fn empty_selected_root_deletion_error_retains_the_root_and_allows_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    coordinator.dir.create_dir(&name).unwrap();
    let fault = io_faults::arm("cleanup.unlink_directory");
    let selected = |_: &mut JanitorScan| Ok((vec![name.clone()], None));
    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        SystemTime::now(),
        std::time::Instant::now(),
        &|_| Duration::ZERO,
        &selected,
    );
    assert!(fault.triggered());
    assert_eq!(
        report
            .unresolved_by_reason
            .get(&JanitorReason::DeleteFailed),
        Some(&1)
    );
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert!(coordinator.dir.exists(&name));
    drop(fault);
    let retry = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        SystemTime::now(),
        std::time::Instant::now(),
        &|_| Duration::ZERO,
        &selected,
    );
    assert_eq!(retry.reclaimed_roots, 1);
    assert_eq!(retry.removed_entries, 1);
    assert!(!coordinator.dir.exists(&name));
}

#[cfg(unix)]
#[test]
fn lease_evidence_identity_metadata_error_retains_its_path_and_allows_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let path = root.path.join(LEASE_FILE);
    let file = File::open(&path).unwrap();
    let original = fs::read(&path).unwrap();
    let fault = io_faults::arm("file.identity_metadata");
    let error = inspect_marker_evidence(&file, &path).unwrap_err();
    assert!(fault.triggered());
    assert!(
        matches!(&error, WorkspaceError::Io { operation: "identify workspace lease", path: actual, .. } if actual == &path)
    );
    assert!(error.to_string().contains("file.identity_metadata"));
    assert_eq!(fs::read(&path).unwrap(), original);
    drop(fault);
    assert_eq!(
        inspect_marker_evidence(&file, &path).unwrap().marker.run_id,
        root.run_id
    );
}

#[cfg(unix)]
#[test]
fn lease_owner_metadata_error_cannot_certify_a_regular_marker() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original = root.dir.read(LEASE_FILE).unwrap();
    let fault = io_faults::arm("file.owner_metadata");
    let error = open_regular_file_nofollow(&root.dir, LEASE_FILE).unwrap_err();
    assert!(fault.triggered());
    assert!(
        matches!(&error, WorkspaceError::Io { operation: "verify cleanup lease owner", path, .. } if path == Utf8Path::new(LEASE_FILE))
    );
    assert!(error.to_string().contains("file.owner_metadata"));
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), original);
    drop(fault);
    let file = open_regular_file_nofollow(&root.dir, LEASE_FILE).unwrap();
    assert_eq!(read_marker_file(&file).unwrap().run_id, root.run_id);
}

#[rstest::rstest]
#[case("reopen.stat", "inspect coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.open", "open coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.identity", "identify coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.metadata", "inspect coordinator", |_: &cap_std::fs::Dir| {})]
#[case("reopen.unlock_empty", "open coordinator", |dir: &cap_std::fs::Dir| dir.write(COORDINATOR_FILE, b"").unwrap())]
#[case("reopen.unlock_valid", "unlock coordinator", |_: &cap_std::fs::Dir| {})]
fn coordinator_reopen_checks_elapsed_time_after_each_io_boundary(
    #[case] point: &'static str,
    #[case] operation: &str,
    #[case] configure: fn(&cap_std::fs::Dir),
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    configure(&coordinator.dir);
    let path = coordinator.path.join(COORDINATOR_FILE);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let fault = io_faults::arm_action(point, 0, || {
        std::thread::sleep(Duration::from_millis(1050));
        Ok(())
    });
    let error = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(
        matches!(&error, WorkspaceError::Io { operation: actual, path: actual_path, .. } if *actual == operation && actual_path == &path)
    );
    assert!(error.to_string().contains("deadline"));
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
    let independent = File::options().read(true).write(true).open(&path).unwrap();
    File::try_lock(&independent).expect("timeout must release any acquired coordinator lock");
    File::unlock(&independent).unwrap();
    drop(fault);
    coordinator
        .dir
        .write(COORDINATOR_FILE, initial_coordinator_bytes())
        .unwrap();
    let retry = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        read_coordinator_state(&retry, &path).unwrap().1.generation,
        0
    );
}

#[test]
fn publication_rename_failure_rolls_back_its_owned_staging_root() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let fault = io_faults::arm("claim.rename");
    let error = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "publish managed root",
            ..
        }
    ));
    assert!(error.to_string().contains("claim.rename"));
    assert_eq!(fs::read_dir(&coordinator.path).unwrap().count(), 1);
    drop(fault);
    let retry = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    assert_eq!(
        retry.cleanup(Duration::from_secs(1)).disposition,
        CleanupDisposition::Clean
    );
}

#[cfg(unix)]
#[test]
fn publication_rejects_active_identity_replacement_before_opening_it() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let dir = coordinator.dir.try_clone().unwrap();
    let name = std::rc::Rc::new(std::cell::RefCell::new(None));
    let observed_name = std::rc::Rc::clone(&name);
    let fault = io_faults::arm_action("active.open", 0, move || {
        let active = dir
            .entries()
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(ACTIVE_PREFIX)
            })
            .unwrap()
            .file_name();
        dir.open_dir(&active)
            .unwrap()
            .write("original", b"original data")
            .unwrap();
        dir.rename(&active, &dir, "held-original")
            .expect("park the just-published root");
        dir.create_dir(&active)
            .expect("install the replacement root");
        dir.open_dir(&active)
            .unwrap()
            .write("replacement", b"replacement data")
            .unwrap();
        *observed_name.borrow_mut() = Some(active);
        Ok(())
    });
    let error = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap_err();
    assert!(fault.triggered());
    assert!(
        matches!(error, WorkspaceError::InvalidPath { path } if path == coordinator.path.join(name.borrow().as_ref().unwrap().to_str().unwrap()))
    );
    assert_eq!(
        coordinator.dir.read("held-original/original").unwrap(),
        b"original data"
    );
    let replacement = coordinator
        .dir
        .open_dir(name.borrow().as_ref().unwrap())
        .unwrap();
    assert_eq!(
        replacement.read("replacement").unwrap(),
        b"replacement data"
    );
}

#[cfg(unix)]
#[test]
fn child_identity_failure_preserves_the_tree_for_a_cleanup_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let child = root.create_child("worker-").unwrap();
    fs::write(child.path.join("payload"), b"keep-child").unwrap();
    let fault = io_faults::arm("directory.identity_metadata");
    let error = child.cleanup().unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "identify managed child",
            ..
        }
    ));
    assert!(error.to_string().contains("directory.identity_metadata"));
    assert_eq!(fs::read(child.path.join("payload")).unwrap(), b"keep-child");
    drop(fault);
    child.cleanup().unwrap();
    assert!(!child.path.exists());
}

#[cfg(unix)]
#[rstest::rstest]
#[case(0, "create managed child", 0)]
#[case(1, "identify managed child", 1)]
#[case(2, "identify managed child", 0)]
fn child_identity_io_errors_preserve_the_verified_construction_evidence(
    #[case] successful_calls: usize,
    #[case] operation: &str,
    #[case] unverified_directories: usize,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let fault = io_faults::arm_after("directory.identity_metadata", successful_calls);
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation));
    assert!(error.to_string().contains("directory.identity_metadata"));
    assert_eq!(
        error.to_string().contains("unverified name left untouched"),
        unverified_directories == 1
    );
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(
        root.dir.entries().unwrap().count(),
        controls + unverified_directories
    );
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
}

#[rstest::rstest]
#[case("bootstrap.canonicalize", "canonicalize managed-root parent")]
#[case("bootstrap.open_parent", "open managed-root parent")]
#[case("bootstrap.create_root", "create managed root")]
#[case("bootstrap.inspect_root", "inspect managed root")]
#[case("bootstrap.open_root", "open managed root")]
fn bootstrap_elapsed_io_deadlines_preserve_a_retryable_namespace(
    #[case] point: &'static str,
    #[case] operation: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let fault = io_faults::arm_action(point, 0, || {
        std::thread::sleep(JANITOR_SELECTION_BUDGET + Duration::from_millis(50));
        Ok(())
    });
    let error =
        ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation));
    assert!(error.to_string().contains("deadline"));
    drop(fault);
    let coordinator = setup(&parent);
    assert_eq!(coordinator.dir.entries().unwrap().count(), 1);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    assert_eq!(
        root.cleanup(Duration::from_secs(1)).disposition,
        CleanupDisposition::Clean
    );
}

#[cfg(unix)]
#[test]
fn cleanup_ready_marker_identity_error_preserves_the_existing_lease() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let lease = root.lease.lock().unwrap();
    let lease = lease.as_ref().unwrap();
    let original = root.dir.read(LEASE_FILE).unwrap();
    let fault = io_faults::arm("file.identity_metadata");
    let error = write_cleanup_ready_marker(&root.dir, &root.path, &root.run_id, root.owner, lease)
        .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "inspect workspace lease",
            ..
        }
    ));
    assert!(error.to_string().contains("file.identity_metadata"));
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), original);
    assert!(!root.path.join(CLEANUP_READY_FILE).exists());
    drop(fault);
    write_cleanup_ready_marker(&root.dir, &root.path, &root.run_id, root.owner, lease).unwrap();
    assert!(root.path.join(CLEANUP_READY_FILE).is_file());
}

#[test]
fn missing_heartbeat_is_reported_without_modifying_the_lease() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original = root.dir.read(LEASE_FILE).unwrap();
    let marker: LeaseMarker = serde_json::from_slice(&original).unwrap();
    root.dir.remove_file(HEARTBEAT_FILE).unwrap();
    let error = candidate_is_stale(&root.dir, &marker, SystemTime::now()).unwrap_err();
    assert_eq!(error.reason, JanitorReason::HeartbeatInvalid);
    assert_ne!(error.cause, "");
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), original);
    root.dir.write(HEARTBEAT_FILE, original).unwrap();
    root.retain().unwrap();
}

#[test]
fn child_removal_verification_io_error_preserves_retryable_completion() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let child = root.create_child("worker-").unwrap();
    child
        .dir
        .as_ref()
        .unwrap()
        .write("payload", b"remove-original")
        .unwrap();
    let fault = io_faults::arm("child.verify_removal");
    let error = child.cleanup().unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "verify managed child removal",
            ..
        }
    ));
    assert!(error.to_string().contains("child.verify_removal"));
    assert!(!child.path.exists());
    drop(fault);
    child.cleanup().unwrap();
}

#[test]
fn child_removal_verification_preserves_a_new_replacement_directory() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let child = root.create_child("worker-").unwrap();
    child
        .dir
        .as_ref()
        .unwrap()
        .write("payload", b"remove-original")
        .unwrap();
    let boundary_parent = root.dir.try_clone().unwrap();
    let boundary_name = child.name.clone();
    let fault = io_faults::arm_action("child.verify_removal", 0, move || {
        boundary_parent
            .create_dir(&boundary_name)
            .expect("install the replacement directory after original removal");
        boundary_parent
            .write(format!("{boundary_name}/replacement"), b"keep-replacement")
            .expect("populate the replacement directory fixture");
        Ok(())
    });
    let error = child.cleanup().unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "verify managed child removal",
            ..
        }
    ));
    assert!(error.to_string().contains("path remains after cleanup"));
    assert_eq!(
        fs::read(child.path.join("replacement")).unwrap(),
        b"keep-replacement"
    );
    drop(fault);
    root.dir
        .remove_file(format!("{}/replacement", child.name))
        .unwrap();
    root.dir.remove_dir(&child.name).unwrap();
    child.cleanup().unwrap();
}

#[cfg(unix)]
#[test]
#[ignore = "waits for the production 60-second owner cleanup deadline"]
fn child_cleanup_budget_exhaustion_reports_partial_deletion_and_allows_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let child = root.create_child("worker-").unwrap();
    let child_dir = child.dir.as_ref().unwrap();
    child_dir.write("payload", b"remove-payload").unwrap();
    child_dir.write(RETAIN_FILE, b"remove-control").unwrap();
    let original_lease = root.dir.read(LEASE_FILE).unwrap();
    // The ordinary payload is removed first. Delay removal of the following
    // control marker so the slice returns real progress after the owner budget.
    let fault = io_faults::arm_action("cleanup.unlink_file", 1, || {
        std::thread::sleep(OWNER_CLEANUP_BUDGET + Duration::from_millis(250));
        Ok(())
    });
    let error = child.cleanup().unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "remove managed child",
            ..
        }
    ));
    assert!(error.to_string().contains("cleanup budget exhausted"));
    assert!(child.path.is_dir());
    assert!(!child.path.join("payload").exists());
    assert!(!child.path.join(RETAIN_FILE).exists());
    assert_eq!(child_dir.entries().unwrap().count(), 0);
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 1);
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), original_lease);
    drop(fault);
    child.cleanup().unwrap();
    assert!(!child.path.exists());
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), original_lease);
}

#[test]
fn heartbeat_timestamp_error_preserves_age_evidence_and_allows_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let original = root.dir.read(HEARTBEAT_FILE).unwrap();
    let marker: LeaseMarker = serde_json::from_slice(&original).unwrap();
    let stale_now = SystemTime::now() + STALE_AFTER + Duration::from_secs(1);
    let fault = io_faults::arm("janitor.heartbeat_modified");
    let error = candidate_is_stale(&root.dir, &marker, stale_now).unwrap_err();
    assert!(fault.triggered());
    assert_eq!(error.reason, JanitorReason::AgeUnverifiable);
    assert!(error.cause.contains("janitor.heartbeat_modified"));
    assert_eq!(root.dir.read(HEARTBEAT_FILE).unwrap(), original);
    drop(fault);
    assert!(candidate_is_stale(&root.dir, &marker, stale_now).is_ok());
    root.retain().unwrap();
}

#[test]
fn failed_reclaim_removal_records_a_hold_without_queueing_or_deleting_payload() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    coordinator.dir.create_dir(&name).unwrap();
    coordinator
        .dir
        .write(format!("{name}/payload"), b"keep-delete-failure")
        .unwrap();
    let identity = metadata_identity(&coordinator.dir.symlink_metadata(&name).unwrap());
    let mut ledger = ReclaimLedger::default();
    let index = ledger.begin();
    let mut pending = VecDeque::new();
    let fault = io_faults::arm("cleanup.unlink_file");
    let elapsed: &dyn Fn(std::time::Instant) -> Duration = &|_| Duration::ZERO;
    record_or_queue_reclaim_removal(
        &mut ledger,
        &mut pending,
        &coordinator,
        &name,
        identity,
        CleanupCapabilities::default(),
        std::time::Instant::now(),
        index,
        &elapsed,
    );
    assert!(fault.triggered());
    assert!(pending.is_empty());
    let report = ledger.finish();
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert!(
        report
            .details
            .iter()
            .any(|detail| detail.reason == JanitorReason::DeleteFailed
                && detail.message.contains("cleanup.unlink_file"))
    );
    assert_eq!(
        coordinator.dir.read(format!("{name}/payload")).unwrap(),
        b"keep-delete-failure"
    );
    drop(fault);
    assert!(
        remove_claimed_tree_bounded(
            &coordinator.dir,
            &name,
            Some(identity),
            Duration::from_secs(1)
        )
        .unwrap()
        .complete
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case("janitor.deleting_first")]
#[case("janitor.deleting_second")]
fn deleting_probe_error_is_reported_before_preserving_the_unverified_root(
    #[case] point: &'static str,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let lease_bytes = root.dir.read(LEASE_FILE).unwrap();
    root.dir.remove_file(HEARTBEAT_FILE).unwrap();
    let deleting_name = format!("{DELETING_PREFIX}{}", root.run_id);
    let deleting_path = coordinator.path.join(&deleting_name);
    fs::rename(&root.path, &deleting_path).unwrap();
    root.unlock_fixture_lease();
    let fault = io_faults::arm(point);
    let report = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert!(fault.triggered());
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert!(report.details.iter().any(|detail| detail.reason
        == JanitorReason::CandidateInspectionFailed
        && detail.message.contains("inspect deleting root")
        && detail.message.contains(point)));
    assert_eq!(
        fs::read(deleting_path.join(LEASE_FILE)).unwrap(),
        lease_bytes
    );
    drop(fault);
    let retry = ManagedRunRoot::reclaim_abandoned(&coordinator, SystemTime::now());
    assert_eq!(retry.reclaimed_roots, 1);
    assert_eq!(retry.removed_entries, 2);
    assert!(!deleting_path.exists());
}

#[cfg(unix)]
#[rstest::rstest]
#[case("coordinator.acquire_try_lock", false, "lock coordinator", |coordinator: &ManagedRootCoordinator, deadline: std::time::Instant| CoordinatorLockGuard::acquire_until(coordinator, deadline).map(drop))]
#[case("coordinator.acquire_try_lock", true, "lock coordinator", |coordinator: &ManagedRootCoordinator, deadline: std::time::Instant| CoordinatorLockGuard::acquire_until(coordinator, deadline).map(drop))]
#[case("coordinator.bootstrap_try_lock", false, "lock coordinator bootstrap", |coordinator: &ManagedRootCoordinator, deadline: std::time::Instant| lock_file_until(&coordinator.file, deadline, &coordinator.path))]
#[case("coordinator.bootstrap_try_lock", true, "lock coordinator bootstrap", |coordinator: &ManagedRootCoordinator, deadline: std::time::Instant| lock_file_until(&coordinator.file, deadline, &coordinator.path))]
fn coordinator_lock_elapsed_io_deadlines_release_acquired_locks(
    #[case] point: &'static str,
    #[case] contended: bool,
    #[case] operation: &str,
    #[case] acquire: fn(&ManagedRootCoordinator, std::time::Instant) -> Result<(), WorkspaceError>,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let lock_path = coordinator.path.join(COORDINATOR_FILE);
    let held = contended.then(|| {
        File::options()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap()
    });
    let _held_lock = held.as_ref().map(File::try_lock).transpose().unwrap();
    let fault = io_faults::arm_action(point, 0, || {
        std::thread::sleep(Duration::from_millis(1050));
        Ok(())
    });
    let error = acquire(
        &coordinator,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation));
    assert!(error.to_string().contains("deadline"));
    drop(fault);
    drop(held);
    let independent = File::options()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    File::try_lock(&independent).expect("deadline failure must release any acquired OS lock");
    File::unlock(&independent).unwrap();
    assert!(coordinator.local_lock.try_lock().is_ok());
}

#[cfg(unix)]
#[test]
fn owner_claim_observes_both_names_absent_without_deleting_an_external_relocation() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir
        .write("payload", b"keep-external-relocation")
        .unwrap();
    let old_path = root.path.clone();
    let external_path = coordinator.path.join("externally-relocated");
    let moved_path = external_path.clone();
    let fault = io_faults::arm_action("owner.root_identity", 0, move || {
        fs::rename(old_path, moved_path)
    });
    let record = root.cleanup(Duration::from_secs(1));
    assert!(fault.triggered());
    assert_eq!(record.disposition, CleanupDisposition::Clean);
    assert_eq!(record.evidence.removed_entries, 0);
    assert_eq!(record.evidence.remaining_root, None);
    assert_eq!(
        NamespaceLifecycle::load(&root.namespace),
        NamespaceLifecycle::Complete
    );
    assert!(!root.path.exists());
    assert_eq!(
        fs::read(external_path.join("payload")).unwrap(),
        b"keep-external-relocation"
    );
    drop(fault);
    let retry = root.cleanup(Duration::from_secs(1));
    assert_eq!(retry.disposition, CleanupDisposition::Clean);
    assert_eq!(retry.evidence.removed_entries, 0);
    assert_eq!(
        fs::read(external_path.join("payload")).unwrap(),
        b"keep-external-relocation"
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case("coordinator.bootstrap_try_lock", "lock coordinator bootstrap")]
#[case("bootstrap.coordinator_write", "initialize coordinator")]
#[case("bootstrap.coordinator_flush", "flush coordinator")]
#[case("bootstrap.coordinator_verify", "secure coordinator")]
#[case("bootstrap.validate_read", "validate coordinator")]
#[case("bootstrap.coordinator_unlock", "unlock coordinator")]
fn new_coordinator_elapsed_io_deadlines_remove_only_the_verified_new_file(
    #[case] point: &'static str,
    #[case] operation: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let path = Utf8Path::from_path(parent.path())
        .unwrap()
        .join(COORDINATOR_FILE);
    let file = create_owned_file(&dir, COORDINATOR_FILE, OwnedFileAccess::ReadWrite).unwrap();
    let identity = file_identity(&file).unwrap();
    let fault = io_faults::arm_action(point, 0, || {
        std::thread::sleep(Duration::from_millis(1050));
        Ok(())
    });
    let error = initialize_new_coordinator(
        &dir,
        file,
        identity,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation));
    assert!(error.to_string().contains("deadline"));
    assert!(!path.exists());
    assert_eq!(dir.entries().unwrap().count(), 0);
    drop(fault);
    let retry_file = create_owned_file(&dir, COORDINATOR_FILE, OwnedFileAccess::ReadWrite).unwrap();
    let retry_identity = file_identity(&retry_file).unwrap();
    let retry = initialize_new_coordinator(
        &dir,
        retry_file,
        retry_identity,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        read_coordinator_state(&retry, &path).unwrap().1.generation,
        0
    );
}

#[test]
fn new_coordinator_initially_expired_deadline_removes_the_verified_new_file() {
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
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create coordinator",
            ..
        }
    ));
    assert!(error.to_string().contains("deadline"));
    assert!(!path.exists());
    assert_eq!(dir.entries().unwrap().count(), 0);
}

#[test]
fn coordinator_reopen_rejects_an_identity_replacement_without_removing_either_file() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let path = coordinator.path.join(COORDINATOR_FILE);
    let parked = coordinator.path.join("parked-coordinator");
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let original_identity = file_identity(&coordinator.file).unwrap();
    let replace_path = path.clone();
    let parked_path = parked.clone();
    let replacement_bytes = original.clone();
    let fault = io_faults::arm_action("reopen.open", 0, move || {
        fs::rename(&replace_path, parked_path).expect("park the coordinator before its reopen");
        fs::write(replace_path, replacement_bytes)
            .expect("install the replacement coordinator fixture");
        Ok(())
    });
    let error = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(&error, WorkspaceError::InvalidPath { path: actual } if actual == &path));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(&parked).unwrap(), original);
    let replacement_identity =
        metadata_identity(&coordinator.dir.symlink_metadata(COORDINATOR_FILE).unwrap());
    assert_ne!(replacement_identity, original_identity);
    assert_eq!(
        metadata_identity(
            &coordinator
                .dir
                .symlink_metadata("parked-coordinator")
                .unwrap()
        ),
        original_identity
    );
    drop(fault);
    let retry = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(file_identity(&retry).unwrap(), replacement_identity);
}

#[cfg(unix)]
#[rstest::rstest]
#[case("coordinator.bootstrap_try_lock", "lock coordinator bootstrap")]
#[case("bootstrap.coordinator_verify", "secure coordinator")]
#[case("bootstrap.validate_read", "validate coordinator")]
fn coordinator_reopen_internal_deadlines_preserve_the_existing_file_and_release_locks(
    #[case] point: &'static str,
    #[case] operation: &str,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let path = coordinator.path.join(COORDINATOR_FILE);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let identity = file_identity(&coordinator.file).unwrap();
    let fault = io_faults::arm_action(point, 0, || {
        std::thread::sleep(Duration::from_millis(1050));
        Ok(())
    });
    let error = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(&error, WorkspaceError::Io { operation: actual, .. } if *actual == operation));
    assert!(error.to_string().contains("deadline"));
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
    assert_eq!(file_identity(&coordinator.file).unwrap(), identity);
    drop(fault);
    let independent = File::options().read(true).write(true).open(&path).unwrap();
    File::try_lock(&independent).expect("reopen deadline must release the existing file lock");
    File::unlock(&independent).unwrap();
    let retry = open_initialized_coordinator_until(
        &coordinator.dir,
        &path,
        std::time::Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(file_identity(&retry).unwrap(), identity);
}

#[cfg(unix)]
#[test]
fn owned_entry_stream_io_error_does_not_advance_or_modify_the_directory() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write("payload", b"keep-stream").unwrap();
    let mut entries = owned_directory_entries(&dir).unwrap();
    let fault = io_faults::arm("owned.entries_next");
    let error = entries.next().unwrap().unwrap_err();
    assert!(fault.triggered());
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains("owned.entries_next"));
    assert_eq!(dir.read("payload").unwrap(), b"keep-stream");
    drop(fault);
    assert_eq!(
        entries.map(Result::unwrap).collect::<Vec<_>>(),
        vec![OsString::from("payload")]
    );
}

#[cfg(unix)]
#[test]
fn selection_entry_stream_io_error_preserves_the_coordinator_cursor() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let now = std::time::Instant::now();
    let mut scan = JanitorScan::default();
    let fault = io_faults::arm("owned.entries_next");
    let error = select_reclaim_candidates_locked_until_with_hooks(
        &coordinator,
        &mut scan,
        now + Duration::from_secs(1),
        &|| now,
        &<() as Default>::default,
        &<() as Default>::default,
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "enumerate managed roots",
            ..
        }
    ));
    assert!(error.to_string().contains("owned.entries_next"));
    assert!(!scan.directory_scan_complete);
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
    drop(fault);
    assert_eq!(
        select_reclaim_candidates_locked_until_with_hooks(
            &coordinator,
            &mut scan,
            now + Duration::from_secs(1),
            &|| now,
            &<() as Default>::default,
            &<() as Default>::default
        )
        .unwrap(),
        (Vec::new(), None)
    );
}

#[cfg(unix)]
#[test]
fn child_admission_entry_stream_io_error_preserves_the_namespace_for_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let fault = io_faults::arm("owned.entries_next");
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "count managed children",
            ..
        }
    ));
    assert!(error.to_string().contains("owned.entries_next"));
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), controls);
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
}

#[test]
fn selection_entry_open_io_error_preserves_the_coordinator_cursor() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let original = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let now = std::time::Instant::now();
    let mut scan = JanitorScan::default();
    let fault = io_faults::arm("owned.entries_open");
    let error = select_reclaim_candidates_locked_until_with_hooks(
        &coordinator,
        &mut scan,
        now + Duration::from_secs(1),
        &|| now,
        &<() as Default>::default,
        &<() as Default>::default,
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "enumerate managed roots",
            ..
        }
    ));
    assert!(error.to_string().contains("owned.entries_open"));
    assert!(!scan.directory_scan_complete);
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), original);
    drop(fault);
    assert_eq!(
        select_reclaim_candidates_locked_until_with_hooks(
            &coordinator,
            &mut scan,
            now + Duration::from_secs(1),
            &|| now,
            &<() as Default>::default,
            &<() as Default>::default
        )
        .unwrap(),
        (Vec::new(), None)
    );
}

#[test]
fn child_admission_entry_open_io_error_preserves_the_namespace_for_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let fault = io_faults::arm("owned.entries_open");
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "count managed children",
            ..
        }
    ));
    assert!(error.to_string().contains("owned.entries_open"));
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), controls);
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
}

#[test]
fn child_metadata_inspection_error_rolls_back_the_verified_unpublished_directory() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let fault = io_faults::arm("child.inspect");
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "inspect managed child",
            ..
        }
    ));
    assert!(error.to_string().contains("child.inspect"));
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), controls);
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
}

#[cfg(unix)]
fn unpublished_child_name(parent: &cap_std::fs::Dir) -> OsString {
    parent
        .entries()
        .expect("enumerate the child replacement fixture")
        .map(|entry| {
            entry
                .expect("read the child replacement fixture entry")
                .file_name()
        })
        .find(|name| {
            name.to_str()
                .expect("generated child fixture names are UTF-8")
                .starts_with("worker-")
        })
        .expect("the Created child exists before metadata inspection")
}

#[cfg(unix)]
#[rstest::rstest]
#[case::file(|parent: &cap_std::fs::Dir, name: &std::ffi::OsStr| {
    parent.write(name, b"keep-replacement").expect("install the replacement file fixture");
    std::path::PathBuf::from(name)
})]
#[case::directory(|parent: &cap_std::fs::Dir, name: &std::ffi::OsStr| {
    parent.create_dir(name).expect("install the replacement directory fixture");
    let payload = std::path::PathBuf::from(name).join("payload");
    parent.write(&payload, b"keep-replacement").expect("populate the replacement directory fixture");
    payload
})]
fn child_metadata_identity_change_preserves_the_replacement_and_live_child_count(
    #[case] replace: fn(&cap_std::fs::Dir, &std::ffi::OsStr) -> std::path::PathBuf,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let boundary_parent = root.dir.try_clone().unwrap();
    let payload = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored_payload = std::rc::Rc::clone(&payload);
    let fault = io_faults::arm_action("child.inspect", 0, move || {
        let name = unpublished_child_name(&boundary_parent);
        boundary_parent
            .remove_dir(&name)
            .expect("remove only the original unpublished directory fixture");
        *stored_payload.borrow_mut() = Some(replace(&boundary_parent, &name));
        Ok(())
    });
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "construct managed directory",
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("secondary new-directory rollback failure")
    );
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), controls + 1);
    let payload = payload.borrow().clone().unwrap();
    let replacement_name = payload.iter().next().unwrap().to_str().unwrap();
    let primary = WorkspaceError::InvalidPath {
        path: root.path.join(replacement_name),
    };
    assert!(error.to_string().contains(&primary.to_string()));
    assert_eq!(root.dir.read(&payload).unwrap(), b"keep-replacement");
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
    assert_eq!(root.dir.read(&payload).unwrap(), b"keep-replacement");
}

#[cfg(unix)]
#[test]
fn janitor_budget_expiration_during_claim_preserves_active_payload_and_lease_for_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    root.dir.write("payload", b"keep-before-claim").unwrap();
    root.mark_cleanup_ready().unwrap();
    let lease_bytes = root.dir.read(LEASE_FILE).unwrap();
    root.unlock_fixture_lease();
    let expired = std::rc::Rc::new(std::cell::Cell::new(false));
    let boundary_expired = std::rc::Rc::clone(&expired);
    let fault = io_faults::arm_action("claim.candidate_identity", 0, move || {
        boundary_expired.set(true);
        Ok(())
    });
    let elapsed = |_: std::time::Instant| {
        [Duration::ZERO, JANITOR_CLEANUP_BUDGET][usize::from(expired.get())]
    };
    let report =
        ManagedRunRoot::reclaim_abandoned_with_clock(&coordinator, SystemTime::now(), &elapsed);
    assert!(fault.triggered());
    assert_eq!(
        report.unresolved_by_reason.get(&JanitorReason::ClaimFailed),
        Some(&1)
    );
    assert_eq!(report.reclaimed_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert!(report.details.iter().any(|detail| {
        detail.reason == JanitorReason::ClaimFailed
            && detail
                .message
                .contains("janitor cleanup deadline exceeded before claim")
    }));
    assert_eq!(root.dir.read("payload").unwrap(), b"keep-before-claim");
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), lease_bytes);
    assert!(root.path.is_dir());
    drop(fault);
    expired.set(false);
    let retry =
        ManagedRunRoot::reclaim_abandoned_with_clock(&coordinator, SystemTime::now(), &elapsed);
    assert_eq!(retry.reclaimed_roots, 1);
    assert_eq!(retry.removed_entries, 5);
    assert!(!root.path.exists());
}

#[cfg(unix)]
#[test]
fn cleanup_slice_root_identity_error_preserves_payload_and_progress_for_retry() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    coordinator.dir.create_dir(&name).unwrap();
    coordinator
        .dir
        .write(format!("{name}/payload"), b"keep-slice-identity")
        .unwrap();
    let identity = metadata_identity(&coordinator.dir.symlink_metadata(&name).unwrap());
    let mut capabilities = CleanupCapabilities::default();
    let fault = io_faults::arm_after("directory.identity_metadata", 1);
    let error = remove_one_claimed_entry_with_capabilities(
        &coordinator.dir,
        &name,
        Some(identity),
        10,
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut capabilities,
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(error.to_string().contains("directory.identity_metadata"));
    assert_eq!(capabilities.progress.examined_entries, 0);
    assert_eq!(capabilities.progress.removed_entries, 0);
    assert!(!capabilities.progress.root_removed);
    assert_eq!(
        coordinator.dir.read(format!("{name}/payload")).unwrap(),
        b"keep-slice-identity"
    );
    drop(fault);
    let retry = remove_claimed_tree_bounded_with_capabilities(
        &coordinator.dir,
        &name,
        Some(identity),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut capabilities,
    )
    .unwrap();
    assert!(retry.complete);
    assert_eq!(capabilities.progress.removed_entries, 2);
    assert!(capabilities.progress.root_removed);
    assert!(!coordinator.dir.exists(&name));
}

#[cfg(unix)]
#[rstest::rstest]
#[case::transient_missing(std::io::ErrorKind::NotFound, true)]
#[case::other_error(std::io::ErrorKind::Other, false)]
fn cleanup_slice_entry_stream_failure_preserves_or_counts_only_actual_deletion(
    #[case] kind: std::io::ErrorKind,
    #[case] completes: bool,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let name = format!("{DELETING_PREFIX}{}", uuid::Uuid::new_v4());
    coordinator.dir.create_dir(&name).unwrap();
    coordinator
        .dir
        .write(format!("{name}/payload"), b"keep-stream-error")
        .unwrap();
    let identity = metadata_identity(&coordinator.dir.symlink_metadata(&name).unwrap());
    let mut capabilities = CleanupCapabilities::default();
    let fault = io_faults::arm_action("owned.entries_next", 0, move || Err(kind.into()));
    let result = remove_claimed_tree_bounded_with_capabilities(
        &coordinator.dir,
        &name,
        Some(identity),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut capabilities,
    );
    assert!(fault.triggered());
    assert_eq!(result.is_ok(), completes);
    assert_eq!(
        result
            .as_ref()
            .ok()
            .map(|slice| (slice.complete, slice.removed)),
        completes.then_some((true, 2))
    );
    assert_eq!(
        result.as_ref().err().map(std::io::Error::kind),
        (!completes).then_some(kind)
    );
    assert_eq!(capabilities.progress.root_removed, completes);
    assert_eq!(
        capabilities.progress.removed_entries,
        u64::from(completes) * 2
    );
    assert_eq!(coordinator.dir.exists(&name), !completes);
    assert_eq!(
        coordinator
            .dir
            .read(format!("{name}/payload"))
            .ok()
            .as_deref(),
        (!completes).then_some(b"keep-stream-error".as_slice())
    );
    drop(fault);
    let retry = remove_claimed_tree_bounded_with_capabilities(
        &coordinator.dir,
        &name,
        Some(identity),
        CleanupBudget::from(Duration::from_secs(1)),
        &mut capabilities,
    )
    .unwrap();
    assert!(retry.complete);
    assert_eq!(capabilities.progress.removed_entries, 2);
    assert!(!coordinator.dir.exists(&name));
}

#[cfg(unix)]
#[test]
fn marker_open_rejects_an_identity_replacement_without_modifying_either_file() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    dir.write(LEASE_FILE, b"keep-original").unwrap();
    let original_identity = metadata_identity(&dir.symlink_metadata(LEASE_FILE).unwrap());
    let boundary_parent = dir.try_clone().unwrap();
    let fault = io_faults::arm_action("marker.open", 0, move || {
        boundary_parent
            .rename(LEASE_FILE, &boundary_parent, "parked-lease")
            .expect("park the inspected lease fixture before reopening it");
        boundary_parent
            .write(LEASE_FILE, b"keep-replacement")
            .expect("install a distinct lease replacement fixture");
        Ok(())
    });
    let error = inspect_regular_file_nofollow(&dir, LEASE_FILE)
        .err()
        .unwrap();
    assert!(fault.triggered());
    assert!(!error.absent);
    assert!(
        matches!(&error.error, WorkspaceError::InvalidPath { path } if path == Utf8Path::new(LEASE_FILE))
    );
    assert_eq!(dir.read("parked-lease").unwrap(), b"keep-original");
    assert_eq!(dir.read(LEASE_FILE).unwrap(), b"keep-replacement");
    let replacement_identity = metadata_identity(&dir.symlink_metadata(LEASE_FILE).unwrap());
    assert_ne!(replacement_identity, original_identity);
    drop(fault);
    let retry = inspect_regular_file_nofollow(&dir, LEASE_FILE)
        .ok()
        .unwrap();
    assert_eq!(file_identity(&retry).unwrap(), replacement_identity);
}

#[cfg(unix)]
#[test]
fn claim_candidate_lease_open_error_preserves_active_identity_and_restores_its_capability() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let active = root.path.file_name().unwrap();
    let deleting = format!("{DELETING_PREFIX}{}", root.run_id);
    let identity = directory_identity(&root.dir).unwrap();
    let lease_bytes = root.dir.read(LEASE_FILE).unwrap();
    let fault = io_faults::arm("marker.open");
    let error = root
        .claim_with_handle_handoff(&coordinator.dir, active, &deleting, &|| Ok(()))
        .err()
        .unwrap();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "open cleanup lease",
            ..
        }
    ));
    assert!(error.to_string().contains("marker.open"));
    assert_eq!(
        NamespaceLifecycle::load(&root.namespace),
        NamespaceLifecycle::Active
    );
    assert_eq!(directory_identity(&root.dir).unwrap(), identity);
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), lease_bytes);
    assert!(root.lease.lock().unwrap().is_some());
    assert!(!coordinator.dir.exists(&deleting));
    drop(fault);
    let retry = root
        .claim_with_handle_handoff(&coordinator.dir, active, &deleting, &|| Ok(()))
        .unwrap();
    assert!(matches!(retry, ClaimResult::Claimed));
    assert_eq!(
        NamespaceLifecycle::load(&root.namespace),
        NamespaceLifecycle::Deleting
    );
    assert_eq!(
        coordinator
            .dir
            .read(format!("{deleting}/{LEASE_FILE}"))
            .unwrap(),
        lease_bytes
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case("directory.inspect_created", 1)]
#[case("directory.open_created", 0)]
fn child_directory_construction_io_errors_preserve_only_unverified_names(
    #[case] point: &'static str,
    #[case] unverified_directories: usize,
) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let original = root.dir.read(LEASE_FILE).unwrap();
    let fault = io_faults::arm(point);
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create managed child",
            ..
        }
    ));
    assert!(error.to_string().contains(point));
    assert_eq!(
        error.to_string().contains("unverified name left untouched"),
        unverified_directories == 1
    );
    assert_eq!(
        root.dir.entries().unwrap().count(),
        controls + unverified_directories
    );
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.read(LEASE_FILE).unwrap(), original);
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
    assert_eq!(
        root.dir.entries().unwrap().count(),
        controls + unverified_directories
    );
}

#[cfg(unix)]
#[test]
fn child_directory_type_replacement_before_first_inspection_preserves_the_new_file() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let boundary_parent = root.dir.try_clone().unwrap();
    let name = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored_name = std::rc::Rc::clone(&name);
    let fault = io_faults::arm_action("directory.inspect_created", 0, move || {
        let name = unpublished_child_name(&boundary_parent);
        boundary_parent
            .remove_dir(&name)
            .expect("remove the uninspected directory fixture");
        boundary_parent
            .write(&name, b"keep-type-replacement")
            .expect("install the replacement file fixture");
        *stored_name.borrow_mut() = Some(name);
        Ok(())
    });
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create managed child",
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("new owned directory changed before it could be opened")
    );
    assert!(error.to_string().contains("rollback"));
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), controls + 1);
    let name = name.borrow().clone().unwrap();
    assert_eq!(root.dir.read(&name).unwrap(), b"keep-type-replacement");
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
    assert_eq!(root.dir.read(&name).unwrap(), b"keep-type-replacement");
}

#[cfg(unix)]
#[test]
fn child_directory_identity_replacement_before_open_preserves_both_directories() {
    let parent = tempfile::tempdir().unwrap();
    let coordinator = setup(&parent);
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let controls = root.dir.entries().unwrap().count();
    let boundary_parent = root.dir.try_clone().unwrap();
    let name = std::rc::Rc::new(std::cell::RefCell::new(None));
    let stored_name = std::rc::Rc::clone(&name);
    let fault = io_faults::arm_action("directory.open_created", 0, move || {
        let name = unpublished_child_name(&boundary_parent);
        boundary_parent
            .rename(&name, &boundary_parent, "parked-before-open")
            .expect("park the inspected directory fixture before opening");
        boundary_parent
            .create_dir(&name)
            .expect("install the replacement directory fixture");
        boundary_parent
            .open_dir(&name)
            .expect("open the replacement fixture")
            .write("payload", b"keep-identity-replacement")
            .expect("populate the replacement fixture");
        *stored_name.borrow_mut() = Some(name);
        Ok(())
    });
    let error = root.create_child("worker-").unwrap_err();
    assert!(fault.triggered());
    assert!(matches!(
        &error,
        WorkspaceError::Io {
            operation: "create managed child",
            ..
        }
    ));
    assert!(
        error
            .to_string()
            .contains("new owned directory identity changed while opening it")
    );
    assert!(error.to_string().contains("rollback"));
    assert_eq!(root.lifecycle.lock().unwrap().live_children, 0);
    assert_eq!(root.dir.entries().unwrap().count(), controls + 2);
    let name = name.borrow().clone().unwrap();
    let replacement = root.dir.open_dir(&name).unwrap();
    assert_eq!(
        replacement.read("payload").unwrap(),
        b"keep-identity-replacement"
    );
    assert_ne!(
        metadata_identity(&root.dir.symlink_metadata(&name).unwrap()),
        metadata_identity(&root.dir.symlink_metadata("parked-before-open").unwrap())
    );
    assert_eq!(
        root.dir
            .open_dir("parked-before-open")
            .unwrap()
            .entries()
            .unwrap()
            .count(),
        0
    );
    drop(fault);
    let child = root.create_child("worker-").unwrap();
    child.cleanup().unwrap();
    assert_eq!(
        replacement.read("payload").unwrap(),
        b"keep-identity-replacement"
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case("cleanup.repair_metadata")]
#[case("cleanup.reopen_repaired")]
fn permission_repair_io_error_preserves_identity_and_payload_for_retry(
    #[case] point: &'static str,
) {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let child = create_owned_directory(&dir, "directory").unwrap();
    child.write("payload", b"keep-repair-error").unwrap();
    let identity = directory_identity(&child).unwrap();
    let nested = std::rc::Rc::new(std::cell::RefCell::new(None));
    let nested_slot = std::rc::Rc::clone(&nested);
    let fault = io_faults::arm_action("cleanup.open_directory", 0, move || {
        *nested_slot.borrow_mut() = Some(io_faults::arm(point));
        Err(std::io::ErrorKind::PermissionDenied.into())
    });
    let error = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(nested.borrow().as_ref().unwrap().triggered());
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains(point));
    assert_eq!(directory_identity(&child).unwrap(), identity);
    assert_eq!(child.read("payload").unwrap(), b"keep-repair-error");
    drop(fault);
    drop(nested.borrow_mut().take());
    let retry = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap();
    assert_eq!(directory_identity(&retry).unwrap(), identity);
    assert_eq!(retry.read("payload").unwrap(), b"keep-repair-error");
}

#[cfg(unix)]
#[test]
fn permission_repair_reinspection_rejects_an_identity_replacement_without_deletion() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let child = create_owned_directory(&dir, "directory").unwrap();
    child.write("payload", b"keep-original").unwrap();
    let identity = directory_identity(&child).unwrap();
    let boundary_parent = dir.try_clone().unwrap();
    let nested = std::rc::Rc::new(std::cell::RefCell::new(None));
    let nested_slot = std::rc::Rc::clone(&nested);
    let fault = io_faults::arm_action("cleanup.open_directory", 0, move || {
        *nested_slot.borrow_mut() = Some(io_faults::arm_action(
            "cleanup.repair_metadata",
            0,
            move || {
                boundary_parent
                    .rename("directory", &boundary_parent, "parked")
                    .expect("park the repaired directory fixture");
                boundary_parent
                    .create_dir("directory")
                    .expect("install the replacement after permission repair");
                boundary_parent
                    .write("directory/payload", b"keep-replacement")
                    .expect("populate the replacement after permission repair");
                Ok(())
            },
        ));
        Err(std::io::ErrorKind::PermissionDenied.into())
    });
    let error = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert!(nested.borrow().as_ref().unwrap().triggered());
    assert_eq!(
        error.to_string(),
        "cleanup directory identity changed while repairing permissions"
    );
    assert_eq!(dir.read("parked/payload").unwrap(), b"keep-original");
    assert_eq!(dir.read("directory/payload").unwrap(), b"keep-replacement");
    drop(fault);
    drop(nested.borrow_mut().take());
    let retry = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap_err();
    assert_eq!(
        retry.to_string(),
        "cleanup directory identity changed before reopening"
    );
    assert_eq!(dir.read("parked/payload").unwrap(), b"keep-original");
    assert_eq!(dir.read("directory/payload").unwrap(), b"keep-replacement");
}

#[cfg(unix)]
#[test]
fn cleanup_directory_open_rejects_an_identity_replacement_without_deletion() {
    let parent = tempfile::tempdir().unwrap();
    let dir =
        cap_std::fs::Dir::open_ambient_dir(parent.path(), cap_std::ambient_authority()).unwrap();
    let child = create_owned_directory(&dir, "directory").unwrap();
    child.write("payload", b"keep-original").unwrap();
    let identity = directory_identity(&child).unwrap();
    let boundary_parent = dir.try_clone().unwrap();
    let fault = io_faults::arm_action("cleanup.open_directory", 0, move || {
        boundary_parent
            .rename("directory", &boundary_parent, "parked")
            .expect("park the inspected directory before opening");
        boundary_parent
            .create_dir("directory")
            .expect("install the replacement before opening");
        boundary_parent
            .write("directory/payload", b"keep-replacement")
            .expect("populate the replacement before opening");
        Ok(())
    });
    let error = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap_err();
    assert!(fault.triggered());
    assert_eq!(
        error.to_string(),
        "cleanup directory identity changed while opening"
    );
    assert_eq!(dir.read("parked/payload").unwrap(), b"keep-original");
    assert_eq!(dir.read("directory/payload").unwrap(), b"keep-replacement");
    drop(fault);
    let retry = open_cleanup_directory(
        &dir,
        std::ffi::OsStr::new("directory"),
        Some(identity),
        std::time::Instant::now(),
        CleanupBudget::from(Duration::from_secs(1)),
    )
    .unwrap_err();
    assert_eq!(
        retry.to_string(),
        "cleanup directory identity changed before reopening"
    );
}
