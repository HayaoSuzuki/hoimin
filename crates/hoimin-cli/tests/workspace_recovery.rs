use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::workspace::{CopyOptions, WorkspaceError, WorkspaceHandler};
use hoimin_core::{
    BudgetLedger, EffectFailure, EffectId, IntegrityCheckpoint, Preflight, ReservationId,
    ResetWorker, RunBudgets, VerifyOriginals, release_workspace_copy, reserve_workspace_copy,
};

fn write(root: &Utf8Path, path: &str, bytes: &[u8]) {
    let destination = root.join(path);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, bytes).unwrap();
}

fn handler(root: &Utf8Path, workers: u32) -> WorkspaceHandler {
    WorkspaceHandler::new(root.to_owned(), Vec::new(), workers, CopyOptions::default())
}

fn preflight_and_grant(
    handler: &mut WorkspaceHandler,
    max_copy: u64,
) -> (BudgetLedger, hoimin_core::WorkspaceCopyGrant) {
    let completed = handler
        .handle_preflight(Preflight { id: EffectId(100) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: max_copy,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    (ledger, grant)
}

#[test]
fn shell_materializes_only_from_a_matching_core_copy_grant() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 2);
    let completed = handler
        .handle_preflight(Preflight { id: EffectId(1) })
        .unwrap();
    assert_eq!(completed.per_worker_logical_bytes, 9);
    assert_eq!(completed.requested_workers, 2);
    assert_eq!(completed.aggregate_logical_bytes, 18);
    assert_eq!(handler.worker_count(), 0);

    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 18,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    let created = handler
        .handle_create_worker(grant.create_worker(EffectId(2), 0).unwrap())
        .unwrap();
    assert_eq!(created.worker, 0);
    assert_eq!(created.reservation_id, grant.reservation_id);

    let mut forged = grant.create_worker(EffectId(3), 1).unwrap();
    forged.granted_allowance += 1;
    let failed = handler.handle_create_worker(forged).unwrap_err();
    assert_eq!(failed.id, EffectId(3));
    assert!(matches!(failed.failure, EffectFailure::CopyLimit { .. }));
    assert_eq!(handler.worker_count(), 1);
}

#[test]
fn shell_rejects_a_forged_reservation_id_as_a_typed_grant_failure() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 2);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 18);
    handler
        .handle_create_worker(grant.create_worker(EffectId(4), 0).unwrap())
        .unwrap();

    let mut forged = grant.create_worker(EffectId(5), 1).unwrap();
    forged.reservation_id = ReservationId(grant.reservation_id.0 + 1);
    let failed = handler.handle_create_worker(forged).unwrap_err();

    assert_eq!(failed.id, EffectId(5));
    assert!(matches!(
        failed.failure,
        EffectFailure::InvalidWorkspaceGrant { .. }
    ));
    assert_eq!(handler.worker_count(), 1);
}

#[test]
fn cleanup_reports_reservation_only_after_worker_directory_is_deleted() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (mut ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(101), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();

    let completed = handler
        .handle_cleanup(grant.cleanup(EffectId(102)))
        .unwrap();

    assert!(!worker_root.exists());
    assert_eq!(completed.released_reservations, vec![grant.reservation_id]);
    release_workspace_copy(&mut ledger, &completed).unwrap();
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 0);
}

#[test]
fn cleanup_releases_a_core_reservation_even_when_no_worker_was_created() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (mut ledger, grant) = preflight_and_grant(&mut handler, 9);

    let completed = handler
        .handle_cleanup(grant.cleanup(EffectId(105)))
        .unwrap();
    release_workspace_copy(&mut ledger, &completed).unwrap();

    assert_eq!(completed.released_reservations, vec![grant.reservation_id]);
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 0);
}

#[test]
fn cleanup_rejects_a_reservation_that_is_not_bound_to_the_plan() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(106), 0).unwrap())
        .unwrap();
    let mut cleanup = grant.cleanup(EffectId(107));
    cleanup.reservations[0] = ReservationId(grant.reservation_id.0 + 1);

    let failed = handler.handle_cleanup(cleanup).unwrap_err();

    assert!(matches!(
        failed.failure,
        EffectFailure::InvalidWorkspaceGrant {
            expected,
            received,
        } if expected == grant.reservation_id && received != expected
    ));
    assert_eq!(handler.worker_count(), 1);
}

#[test]
fn typed_failure_keeps_io_category_and_original_effect_id() {
    let mut handler = handler(Utf8Path::new("definitely/missing/workspace/root"), 1);

    let failed = handler
        .handle_preflight(Preflight { id: EffectId(110) })
        .unwrap_err();

    assert_eq!(failed.id, EffectId(110));
    assert!(matches!(failed.failure, EffectFailure::Io { .. }));
}

#[test]
fn reset_failure_discards_worker_and_allows_recreate_under_same_reservation() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    let create = grant.create_worker(EffectId(121), 0).unwrap();
    handler.handle_create_worker(create.clone()).unwrap();
    write(root, "pkg/a.py", b"changed!\n");

    let failed = handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(122),
            worker: 0,
        })
        .unwrap_err();

    assert!(matches!(
        failed.failure,
        EffectFailure::OriginalChanged { .. }
    ));
    assert!(handler.worker(0).is_none());
    write(root, "pkg/a.py", b"original\n");
    handler.handle_create_worker(create).unwrap();
    assert!(handler.worker(0).is_some());
}

#[test]
fn root_relative_file_apis_reject_non_normal_paths_before_io() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(131), 0).unwrap())
        .unwrap();
    let absolute = root.join("pkg/a.py");

    for invalid in [
        Utf8PathBuf::new(),
        Utf8PathBuf::from("../outside"),
        Utf8PathBuf::from("./pkg/a.py"),
        absolute,
    ] {
        assert!(matches!(
            handler.worker(0).unwrap().read(&invalid),
            Err(WorkspaceError::InvalidPath { .. })
        ));
        assert!(matches!(
            handler.worker_mut(0).unwrap().write(&invalid, b"x"),
            Err(WorkspaceError::InvalidPath { .. })
        ));
        assert!(matches!(
            handler.worker_mut(0).unwrap().remove(&invalid),
            Err(WorkspaceError::InvalidPath { .. })
        ));
        assert!(matches!(
            handler.worker(0).unwrap().exists(&invalid),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }
}

#[test]
fn root_relative_file_apis_reject_symlink_escape() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(135), 0).unwrap())
        .unwrap();
    let link = handler.worker(0).unwrap().root().join("escape");
    if create_dir_symlink(outside.path(), link.as_std_path()).is_err() {
        return;
    }

    assert!(matches!(
        handler.worker(0).unwrap().read("escape/secret.txt"),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(matches!(
        handler
            .worker_mut(0)
            .unwrap()
            .write("escape/new.txt", b"no"),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(!outside.path().join("new.txt").exists());
}

#[cfg(unix)]
fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[test]
fn reset_restores_permissions_even_when_bytes_are_unchanged() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(141), 0).unwrap())
        .unwrap();
    let worker_path = handler.worker(0).unwrap().root().join("pkg/a.py");
    let mut permissions = fs::metadata(&worker_path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&worker_path, permissions).unwrap();

    handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(142),
            worker: 0,
        })
        .unwrap();

    assert!(!fs::metadata(worker_path).unwrap().permissions().readonly());
}

#[test]
fn explicit_original_integrity_checkpoint_returns_typed_completion_or_failure() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    handler
        .handle_preflight(Preflight { id: EffectId(150) })
        .unwrap();
    let completed = handler
        .handle_verify_originals(VerifyOriginals {
            id: EffectId(151),
            checkpoint: IntegrityCheckpoint::PreAnalysis,
        })
        .unwrap();
    assert_eq!(completed.checkpoint, IntegrityCheckpoint::PreAnalysis);
    write(root, "pkg/a.py", b"changed!\n");

    let failed = handler
        .handle_verify_originals(VerifyOriginals {
            id: EffectId(152),
            checkpoint: IntegrityCheckpoint::PreFinalReport,
        })
        .unwrap_err();
    assert!(matches!(
        failed.failure,
        EffectFailure::OriginalChanged { .. }
    ));
}

#[cfg(windows)]
#[test]
fn pythonpath_environment_key_is_replaced_case_insensitively() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    let worker = tempfile::tempdir().unwrap();
    let worker_root = Utf8Path::from_path(worker.path()).unwrap();
    let inherited = BTreeMap::from([(
        OsString::from("PythonPath"),
        std::env::join_paths([root.as_std_path()]).unwrap(),
    )]);

    let command =
        hoimin_cli::workspace::build_command_environment(root, worker_root, &[], &inherited)
            .unwrap();

    assert_eq!(
        command
            .env
            .keys()
            .filter(|key| key.to_string_lossy().eq_ignore_ascii_case("PYTHONPATH"))
            .count(),
        1
    );
    assert!(command.env.contains_key(OsStr::new("PYTHONPATH")));
}

#[cfg(windows)]
#[test]
fn cleanup_lock_returns_typed_failure_keeps_worker_and_succeeds_on_retry() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (mut ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(161), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    let locked = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(worker_root.join("pkg/a.py"))
        .unwrap();

    let failed = handler
        .handle_cleanup(grant.cleanup(EffectId(162)))
        .unwrap_err();

    assert_eq!(failed.id, EffectId(162));
    assert!(matches!(
        failed.failure,
        EffectFailure::Io { ref operation, .. } if operation == "remove worker workspace"
    ));
    assert_eq!(handler.worker_count(), 1);
    assert!(worker_root.exists());
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 9);

    drop(locked);
    let completed = handler
        .handle_cleanup(grant.cleanup(EffectId(163)))
        .unwrap();
    assert_eq!(completed.released_reservations, vec![grant.reservation_id]);
    release_workspace_copy(&mut ledger, &completed).unwrap();
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 0);
    assert!(!worker_root.exists());
}

#[cfg(windows)]
#[test]
fn reset_reports_a_failed_discard_cleanup_and_still_allows_recreation() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(171), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    let locked = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(worker_root.join("pkg/a.py"))
        .unwrap();
    write(root, "pkg/a.py", b"changed!\n");

    let failed = handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(172),
            worker: 0,
        })
        .unwrap_err();

    assert!(matches!(
        failed.failure,
        EffectFailure::WorkspaceRestore { ref message, .. }
            if message.contains("discard cleanup failed")
    ));
    assert_eq!(handler.worker_count(), 0);

    write(root, "pkg/a.py", b"original\n");
    drop(locked);
    fs::remove_dir_all(worker_root.parent().unwrap()).unwrap();
    handler
        .handle_create_worker(grant.create_worker(EffectId(173), 0).unwrap())
        .unwrap();
    handler
        .handle_cleanup(grant.cleanup(EffectId(174)))
        .unwrap();
}
