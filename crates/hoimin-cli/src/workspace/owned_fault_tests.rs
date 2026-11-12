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
#[test]
fn interrupted_cleanup_path_validation_rejects_namespace_replacement_before_or_after_stat() {
    for point in ["remaining.stat_candidate", "remaining.open"] {
        let parent = tempfile::tempdir().unwrap();
        let coordinator = setup(&parent);
        let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
        root.dir.write("payload", b"original").unwrap();
        let original_path = root.path.clone();
        let held = coordinator.path.join("held-original");
        let held_for_race = held.clone();
        let race = io_faults::arm_action(point, 0, move || {
            fs::rename(&original_path, held_for_race)?;
            fs::create_dir(&original_path)?;
            fs::write(original_path.join("payload"), b"replacement")
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
