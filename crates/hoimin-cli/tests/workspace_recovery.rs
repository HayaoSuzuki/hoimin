#[cfg(windows)]
use std::collections::BTreeMap;
#[cfg(windows)]
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_cli::workspace::{CopyOptions, WorkspaceError, WorkspaceHandler};
use hoimin_core::{
    BudgetLedger, EffectFailure, EffectId, IntegrityCheckpoint, Preflight, ReservationId,
    ResetWorker, RunBudgets, VerifyOriginals, release_workspace_copy, reserve_workspace_copy,
};

const SUPPORTED_WORKER_TREE_DEPTH: usize = 128;

fn write(root: &Utf8Path, path: &str, bytes: &[u8]) {
    let destination = root.join(path);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, bytes).unwrap();
}

fn create_nested_directories(root: &Path, depth: usize) -> String {
    let mut directory =
        cap_primitives::fs::open_ambient_dir(root, cap_primitives::ambient_authority()).unwrap();
    let mut logical_path = String::new();
    for index in 0..depth {
        let name = format!("d{index}");
        cap_primitives::fs::create_dir(
            &directory,
            Path::new(&name),
            &cap_primitives::fs::DirOptions::new(),
        )
        .unwrap();
        directory = cap_primitives::fs::open_dir_nofollow(&directory, Path::new(&name)).unwrap();
        if !logical_path.is_empty() {
            logical_path.push('/');
        }
        logical_path.push_str(&name);
    }
    logical_path
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

fn link_created_or_platform_denied(result: std::io::Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            #[cfg(windows)]
            {
                assert!(
                    error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.kind() == std::io::ErrorKind::Unsupported
                        || error.raw_os_error() == Some(1314),
                    "unexpected Windows link setup failure: {error}"
                );
                false
            }
            #[cfg(not(windows))]
            panic!("link setup failed unexpectedly: {error}");
        }
    }
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
    assert_eq!(created.reservation_id, grant.reservation_id());
    assert_eq!(handler.worker_count(), 1);
}

#[test]
fn shell_rejects_a_core_capability_from_another_preflight() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first_root = Utf8Path::from_path(first.path()).unwrap();
    let second_root = Utf8Path::from_path(second.path()).unwrap();
    write(first_root, "pkg/a.py", b"original\n");
    write(second_root, "pkg/a.py", b"original\n");
    let mut first_handler = handler(first_root, 1);
    let mut second_handler = handler(second_root, 1);
    first_handler
        .handle_preflight(Preflight { id: EffectId(4) })
        .unwrap();
    let second_preflight = second_handler
        .handle_preflight(Preflight { id: EffectId(5) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 9,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &second_preflight).unwrap();

    let failed = first_handler
        .handle_create_worker(grant.create_worker(EffectId(6), 0).unwrap())
        .unwrap_err();

    assert!(matches!(
        failed.failure,
        EffectFailure::WorkspacePreflightMismatch { .. }
    ));
    assert_eq!(first_handler.worker_count(), 0);
}

#[test]
fn shell_distinguishes_allowance_and_reservation_mismatches_from_core_capabilities() {
    let small = tempfile::tempdir().unwrap();
    let large = tempfile::tempdir().unwrap();
    let small_root = Utf8Path::from_path(small.path()).unwrap();
    let large_root = Utf8Path::from_path(large.path()).unwrap();
    write(small_root, "pkg/a.py", b"original\n");
    write(large_root, "pkg/a.py", b"original!\n");
    let mut small_handler = handler(small_root, 1);
    let mut large_handler = handler(large_root, 1);
    small_handler
        .handle_preflight(Preflight { id: EffectId(7) })
        .unwrap();
    let large_preflight = large_handler
        .handle_preflight(Preflight { id: EffectId(7) })
        .unwrap();
    let mut large_ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 10,
        processes: 1,
    });
    let large_grant = reserve_workspace_copy(&mut large_ledger, &large_preflight).unwrap();
    let allowance_failed = small_handler
        .handle_create_worker(large_grant.create_worker(EffectId(8), 0).unwrap())
        .unwrap_err();
    assert!(matches!(
        allowance_failed.failure,
        EffectFailure::WorkspaceAllowanceMismatch { .. }
    ));

    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut bound_handler = handler(root, 2);
    let completed = bound_handler
        .handle_preflight(Preflight { id: EffectId(9) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 36,
        processes: 1,
    });
    let first_grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    let second_grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    bound_handler
        .handle_create_worker(first_grant.create_worker(EffectId(10), 0).unwrap())
        .unwrap();
    let reservation_failed = bound_handler
        .handle_create_worker(second_grant.create_worker(EffectId(11), 1).unwrap())
        .unwrap_err();
    assert!(matches!(
        reservation_failed.failure,
        EffectFailure::InvalidWorkspaceGrant { .. }
    ));
}

#[test]
fn failed_initial_copy_rolls_back_only_the_slot_and_keeps_the_bound_reservation() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let completed = handler
        .handle_preflight(Preflight { id: EffectId(14) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 18,
        processes: 1,
    });
    let winner = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    let foreign = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    fs::remove_file(root.join("pkg/a.py")).unwrap();

    let failed = handler
        .handle_create_worker(winner.create_worker(EffectId(15), 0).unwrap())
        .unwrap_err();

    assert!(matches!(failed.failure, EffectFailure::Io { .. }));
    assert_eq!(handler.worker_count(), 0);
    assert_eq!(handler.materialized_worker_slots(), 0);
    assert_eq!(handler.observed_copy_bytes(), 0);

    let rejected = handler
        .handle_create_worker(foreign.create_worker(EffectId(16), 0).unwrap())
        .unwrap_err();
    assert!(matches!(
        rejected.failure,
        EffectFailure::InvalidWorkspaceGrant {
            expected,
            received,
        } if expected == winner.reservation_id() && received == foreign.reservation_id()
    ));

    write(root, "pkg/a.py", b"original\n");
    handler
        .handle_create_worker(winner.create_worker(EffectId(17), 0).unwrap())
        .unwrap();
    assert_eq!(handler.materialized_worker_slots(), 1);
    assert_eq!(handler.observed_copy_bytes(), 9);

    let cleaned = handler
        .handle_cleanup(winner.cleanup(EffectId(18)))
        .unwrap();
    assert_eq!(cleaned.released_reservations, vec![winner.reservation_id()]);
    release_workspace_copy(&mut ledger, &cleaned).unwrap();
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 9);
}

#[test]
fn shell_reports_a_core_capability_worker_outside_its_plan_range() {
    let one_worker = tempfile::tempdir().unwrap();
    let two_workers = tempfile::tempdir().unwrap();
    let one_worker_root = Utf8Path::from_path(one_worker.path()).unwrap();
    let two_workers_root = Utf8Path::from_path(two_workers.path()).unwrap();
    write(one_worker_root, "pkg/a.py", b"123456789012345678");
    write(two_workers_root, "pkg/a.py", b"original\n");
    let mut one_worker_handler = handler(one_worker_root, 1);
    let mut two_worker_handler = handler(two_workers_root, 2);
    one_worker_handler
        .handle_preflight(Preflight { id: EffectId(12) })
        .unwrap();
    let two_worker_preflight = two_worker_handler
        .handle_preflight(Preflight { id: EffectId(12) })
        .unwrap();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 18,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &two_worker_preflight).unwrap();

    let failed = one_worker_handler
        .handle_create_worker(grant.create_worker(EffectId(13), 1).unwrap())
        .unwrap_err();

    assert!(matches!(
        failed.failure,
        EffectFailure::WorkspaceWorkerOutOfRange {
            worker: 1,
            requested_workers: 1,
        }
    ));
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
    assert_eq!(
        completed.released_reservations,
        vec![grant.reservation_id()]
    );
    release_workspace_copy(&mut ledger, &completed).unwrap();
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 0);
}

#[test]
fn cleanup_releases_state_when_the_temporary_wrapper_was_already_removed() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (mut ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(111), 0).unwrap())
        .unwrap();
    let wrapper = handler
        .worker(0)
        .unwrap()
        .root()
        .parent()
        .unwrap()
        .to_owned();
    fs::remove_dir_all(&wrapper).unwrap();

    let completed = handler
        .handle_cleanup(grant.cleanup(EffectId(112)))
        .unwrap();
    release_workspace_copy(&mut ledger, &completed).unwrap();

    assert_eq!(handler.worker_count(), 0);
    assert_eq!(handler.pending_cleanup_count(), 0);
    assert_eq!(handler.observed_copy_bytes(), 0);
    assert_eq!(handler.materialized_worker_slots(), 0);
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

    assert_eq!(
        completed.released_reservations,
        vec![grant.reservation_id()]
    );
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
    cleanup.reservations[0] = ReservationId(grant.reservation_id().0 + 1);

    let failed = handler.handle_cleanup(cleanup).unwrap_err();

    assert!(matches!(
        failed.failure,
        EffectFailure::InvalidWorkspaceGrant {
            expected,
            received,
        } if expected == grant.reservation_id() && received != expected
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
    if !link_created_or_platform_denied(create_dir_symlink(outside.path(), link.as_std_path())) {
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

#[test]
fn root_relative_file_apis_reject_final_link_without_touching_outside() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    fs::write(&sentinel, b"outside").unwrap();
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(136), 0).unwrap())
        .unwrap();
    let link = handler.worker(0).unwrap().root().join("sentinel-link");
    if !link_created_or_platform_denied(create_file_symlink(&sentinel, link.as_std_path())) {
        return;
    }

    assert!(matches!(
        handler
            .worker_mut(0)
            .unwrap()
            .write("sentinel-link", b"changed"),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert!(matches!(
        handler.worker_mut(0).unwrap().remove("sentinel-link"),
        Err(WorkspaceError::InvalidPath { .. })
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"outside");
}

#[test]
fn root_relative_file_apis_remain_bound_to_open_worker_root() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(137), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    let moved_root = worker_root.with_extension("moved");
    if let Err(error) = fs::rename(&worker_root, &moved_root) {
        #[cfg(windows)]
        {
            assert_eq!(error.raw_os_error(), Some(32));
            handler
                .worker_mut(0)
                .unwrap()
                .write("sentinel.txt", b"worker")
                .unwrap();
            assert_eq!(
                fs::read(worker_root.join("sentinel.txt")).unwrap(),
                b"worker"
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("worker-root rename failed unexpectedly: {error}");
    }
    fs::create_dir(&worker_root).unwrap();
    fs::write(worker_root.join("sentinel.txt"), b"outside").unwrap();

    handler
        .worker_mut(0)
        .unwrap()
        .write("sentinel.txt", b"worker")
        .unwrap();

    assert_eq!(
        fs::read(moved_root.join("sentinel.txt")).unwrap(),
        b"worker"
    );
    assert_eq!(
        fs::read(worker_root.join("sentinel.txt")).unwrap(),
        b"outside"
    );
    fs::remove_dir_all(&worker_root).unwrap();
    fs::rename(&moved_root, &worker_root).unwrap();
}

#[cfg(unix)]
fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[cfg(unix)]
fn create_file_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
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
fn reset_removes_worker_link_without_traversing_outside() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(143), 0).unwrap())
        .unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("sentinel"), b"outside").unwrap();
    let link = handler.worker(0).unwrap().root().join("unexpected");
    if !link_created_or_platform_denied(create_dir_symlink(outside.path(), link.as_std_path())) {
        return;
    }

    handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(144),
            worker: 0,
        })
        .unwrap();

    assert_eq!(
        fs::read(outside.path().join("sentinel")).unwrap(),
        b"outside"
    );
    assert!(fs::symlink_metadata(link).is_err());
    assert_eq!(
        handler.worker(0).unwrap().read("pkg/a.py").unwrap(),
        b"original\n"
    );
}

#[test]
fn reset_removes_nested_extras_and_restores_file_replaced_by_directory() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(147), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    fs::remove_file(worker_root.join("pkg/a.py")).unwrap();
    write(&worker_root, "pkg/a.py/nested.txt", b"extra");
    write(&worker_root, "extra/deep/file.txt", b"extra");

    handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(148),
            worker: 0,
        })
        .unwrap();

    assert_eq!(
        fs::read(worker_root.join("pkg/a.py")).unwrap(),
        b"original\n"
    );
    assert!(fs::symlink_metadata(worker_root.join("extra")).is_err());
}

#[cfg(unix)]
#[test]
fn reset_restores_original_unix_mode() {
    use std::os::unix::fs::PermissionsExt;

    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    fs::set_permissions(root.join("pkg/a.py"), fs::Permissions::from_mode(0o640)).unwrap();
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(149), 0).unwrap())
        .unwrap();
    let target = handler.worker(0).unwrap().root().join("pkg/a.py");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o777)).unwrap();

    handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(150),
            worker: 0,
        })
        .unwrap();

    assert_eq!(
        fs::metadata(target).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn reset_remains_bound_to_the_open_worker_root() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(145), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    let moved_root = worker_root.with_file_name("moved-worker");
    if let Err(error) = fs::rename(&worker_root, &moved_root) {
        #[cfg(windows)]
        {
            assert_eq!(error.raw_os_error(), Some(32));
            write(&worker_root, "pkg/a.py", b"changed!\n");
            handler
                .handle_reset_worker(ResetWorker {
                    id: EffectId(146),
                    worker: 0,
                })
                .unwrap();
            assert_eq!(
                fs::read(worker_root.join("pkg/a.py")).unwrap(),
                b"original\n"
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("worker-root rename failed unexpectedly: {error}");
    }
    write(&moved_root, "pkg/a.py", b"changed!\n");
    write(&worker_root, "pkg/a.py", b"outside!\n");

    handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(146),
            worker: 0,
        })
        .unwrap();

    assert_eq!(
        fs::read(moved_root.join("pkg/a.py")).unwrap(),
        b"original\n"
    );
    assert_eq!(
        fs::read(worker_root.join("pkg/a.py")).unwrap(),
        b"outside!\n"
    );
    fs::remove_dir_all(&worker_root).unwrap();
    fs::rename(&moved_root, &worker_root).unwrap();
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
    let first = root.join("first");
    let second = root.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let inherited = BTreeMap::from([
        (
            OsString::from("PYTHONPATH"),
            std::env::join_paths([first.as_std_path()]).unwrap(),
        ),
        (
            OsString::from("PythonPath"),
            std::env::join_paths([second.as_std_path()]).unwrap(),
        ),
        (OsString::from("KEEP_ME"), OsString::from("yes")),
    ]);

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
    assert_eq!(
        command.env.get(OsStr::new("KEEP_ME")),
        Some(&OsString::from("yes"))
    );
    let paths = std::env::split_paths(command.env.get(OsStr::new("PYTHONPATH")).unwrap())
        .collect::<Vec<_>>();
    assert!(
        paths.contains(&worker_root.join("first").into_std_path_buf()),
        "{paths:?}"
    );
    assert!(
        paths.contains(&worker_root.join("second").into_std_path_buf()),
        "{paths:?}"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_removes_a_mode_000_nested_directory() {
    use std::os::unix::fs::PermissionsExt;

    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(155), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    let nested = worker_root.join("locked/deep");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("generated.txt"), b"generated\n").unwrap();
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o000)).unwrap();
    fs::set_permissions(nested.parent().unwrap(), fs::Permissions::from_mode(0o000)).unwrap();

    handler
        .handle_cleanup(grant.cleanup(EffectId(156)))
        .unwrap();

    assert!(!worker_root.exists());
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
    assert_eq!(
        completed.released_reservations,
        vec![grant.reservation_id()]
    );
    release_workspace_copy(&mut ledger, &completed).unwrap();
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 0);
    assert!(!worker_root.exists());
}

#[cfg(windows)]
#[test]
fn reset_retains_failed_discard_until_cleanup_then_allows_recreation() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (mut ledger, grant) = preflight_and_grant(&mut handler, 9);
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
    assert_eq!(handler.pending_cleanup_count(), 1);
    assert_eq!(handler.observed_copy_bytes(), 9);
    assert_eq!(handler.materialized_worker_slots(), 1);
    assert!(worker_root.exists());

    write(root, "pkg/a.py", b"original\n");
    let retry = grant.create_worker(EffectId(173), 0).unwrap();
    let blocked = handler.handle_create_worker(retry.clone()).unwrap_err();
    assert!(matches!(blocked.failure, EffectFailure::Io { .. }));
    assert_eq!(handler.pending_cleanup_count(), 1);
    assert_eq!(handler.observed_copy_bytes(), 9);
    assert_eq!(handler.materialized_worker_slots(), 1);
    assert!(worker_root.exists());

    drop(locked);
    let mut foreign_handler =
        WorkspaceHandler::new(root.to_owned(), Vec::new(), 1, CopyOptions::default());
    let foreign_preflight = foreign_handler
        .handle_preflight(Preflight { id: EffectId(999) })
        .unwrap();
    let mut foreign_ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 9,
        processes: 1,
    });
    let foreign_grant = reserve_workspace_copy(&mut foreign_ledger, &foreign_preflight).unwrap();
    let unauthorized = handler
        .handle_create_worker(foreign_grant.create_worker(EffectId(998), 0).unwrap())
        .unwrap_err();
    assert!(matches!(
        unauthorized.failure,
        EffectFailure::WorkspacePreflightMismatch { .. }
    ));
    assert_eq!(handler.pending_cleanup_count(), 1);
    assert_eq!(handler.observed_copy_bytes(), 9);
    assert_eq!(handler.materialized_worker_slots(), 1);
    assert!(worker_root.exists());

    handler.handle_create_worker(retry).unwrap();
    assert_eq!(handler.pending_cleanup_count(), 0);
    assert_eq!(handler.observed_copy_bytes(), 9);
    assert_eq!(handler.materialized_worker_slots(), 1);
    assert!(!worker_root.exists());

    let completed = handler
        .handle_cleanup(grant.cleanup(EffectId(174)))
        .unwrap();
    release_workspace_copy(&mut ledger, &completed).unwrap();
    assert_eq!(ledger.reserved(hoimin_core::BudgetKind::Copy), 0);
}

#[test]
fn reset_preserves_depth_error_while_discard_cleanup_is_pending() {
    let project = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(project.path()).unwrap();
    write(root, "pkg/a.py", b"original\n");
    let mut handler = handler(root, 1);
    let (_ledger, grant) = preflight_and_grant(&mut handler, 9);
    handler
        .handle_create_worker(grant.create_worker(EffectId(181), 0).unwrap())
        .unwrap();
    let worker_root = handler.worker(0).unwrap().root().to_owned();
    let deepest =
        create_nested_directories(worker_root.as_std_path(), SUPPORTED_WORKER_TREE_DEPTH + 1);

    let failed = handler
        .handle_reset_worker(ResetWorker {
            id: EffectId(182),
            worker: 0,
        })
        .unwrap_err();

    assert!(matches!(
        failed.failure,
        EffectFailure::Io { ref code, .. } if code == "workspace.path.depth"
    ));
    assert_eq!(handler.worker_count(), 0);
    assert_eq!(handler.pending_cleanup_count(), 1);
    assert_eq!(handler.observed_copy_bytes(), 9);
    assert_eq!(handler.materialized_worker_slots(), 1);

    let retry = grant.create_worker(EffectId(183), 0).unwrap();
    let blocked = handler.handle_create_worker(retry.clone()).unwrap_err();
    assert!(matches!(
        blocked.failure,
        EffectFailure::Io { ref code, .. } if code == "workspace.path.depth"
    ));
    assert_eq!(handler.pending_cleanup_count(), 1);

    fs::remove_dir(worker_root.join(deepest)).unwrap();
    handler.handle_create_worker(retry).unwrap();
    assert_eq!(handler.pending_cleanup_count(), 0);
    assert_eq!(handler.worker_count(), 1);
    assert_eq!(handler.observed_copy_bytes(), 9);
    assert_eq!(handler.materialized_worker_slots(), 1);
}
