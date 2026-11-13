use super::*;
use hoimin_core::{BudgetLedger, RunBudgets, reserve_workspace_copy};

#[test]
fn missing_snapshot_manifest_entry_releases_worker_slot_and_keeps_reservation() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("target.py"), b"payload").unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    let mut plan =
        WorkspacePlan::preflight(source_root, EffectId(7), 1, CopyOptions::default()).unwrap();
    let completed = plan.completed();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    let request = grant.create_worker(EffectId(8), 0).unwrap();
    let snapshot = Arc::get_mut(&mut plan.snapshot).unwrap();
    let original = snapshot.files.remove(Utf8Path::new("target.py")).unwrap();
    let error = plan.create_worker(&request).unwrap_err();
    assert!(
        matches!(error, WorkspaceError::WorkspaceRestore { path, message } if path == "target.py" && message == "shared snapshot is missing a manifest entry")
    );
    assert_eq!(plan.materialized_workers(), 0);
    assert_eq!(plan.observed_copy_bytes(), 0);
    assert_eq!(plan.reservation_id(), Some(grant.reservation_id()));
    Arc::get_mut(&mut plan.snapshot)
        .unwrap()
        .files
        .insert("target.py".into(), original);
    let worker = plan.create_worker(&request).unwrap();
    assert_eq!(worker.read("target.py").unwrap(), b"payload");
}

#[test]
fn snapshot_writer_reports_destination_directory_and_file_errors_without_accounting_bytes() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    let destination_root = Utf8Path::from_path(destination.path()).unwrap();
    fs::write(source_root.join("permissions"), b"").unwrap();
    let permissions = fs::metadata(source_root.join("permissions"))
        .unwrap()
        .permissions();
    fs::write(destination_root.join("blocked"), b"existing").unwrap();
    let mut writer =
        SnapshotWriter::new(source_root, destination_root, 1, PreflightLimits::default());
    let error = writer
        .write_reader(
            Utf8Path::new("blocked/target"),
            &mut std::io::Cursor::new(b"payload"),
            7,
            permissions.clone(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        WorkspaceError::Io {
            operation: "create shared snapshot directory",
            ..
        }
    ));
    assert_eq!(writer.logical_bytes, 0);
    assert!(writer.files.is_empty());
    let error = writer
        .write_reader(
            Utf8Path::new("blocked"),
            &mut std::io::Cursor::new(b"payload"),
            7,
            permissions,
        )
        .unwrap_err();
    assert!(matches!(error, WorkspaceError::SnapshotPathCollision { path } if path == "blocked"));
    assert_eq!(
        fs::read(destination_root.join("blocked")).unwrap(),
        b"existing"
    );
    assert_eq!(writer.logical_bytes, 0);
}

#[test]
fn snapshot_writer_reports_invalid_destination_root() {
    let source = tempfile::tempdir().unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    let invalid_root = source_root.join("file");
    fs::write(&invalid_root, b"existing").unwrap();
    let permissions = fs::metadata(&invalid_root).unwrap().permissions();
    let mut writer = SnapshotWriter::new(source_root, &invalid_root, 1, PreflightLimits::default());
    let error = writer
        .write_reader(
            Utf8Path::new("target"),
            &mut std::io::Cursor::new(b"payload"),
            7,
            permissions,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        WorkspaceError::Io {
            operation: "create shared snapshot directory",
            ..
        }
    ));
    assert_eq!(writer.logical_bytes, 0);
}

#[test]
fn materialization_pause_debug_identifies_the_target_worker() {
    let (pause, _controller) = MaterializationPause::new(12);
    assert_eq!(
        format!("{pause:?}"),
        "MaterializationPause { worker: 12, .. }"
    );
}

#[cfg(feature = "contracts")]
#[test]
fn corrupted_shared_snapshot_releases_charges_and_allows_repair() {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("target.py"), b"original").unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    let plan =
        WorkspacePlan::preflight(source_root, EffectId(7), 1, CopyOptions::default()).unwrap();
    let completed = plan.completed();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    let request = grant.create_worker(EffectId(8), 0).unwrap();
    let snapshot_path = plan.snapshot.root.join("target.py");
    fs::write(&snapshot_path, b"tampered").unwrap();
    let error = plan.create_worker(&request).unwrap_err();
    assert!(
        matches!(error, WorkspaceError::WorkspaceRestore { path, message } if path == "target.py" && message == "shared snapshot does not match its manifest")
    );
    assert_eq!(plan.observed_copy_bytes(), 0);
    assert_eq!(plan.materialized_workers(), 0);
    fs::write(snapshot_path, b"original").unwrap();
    assert_eq!(
        plan.create_worker(&request)
            .unwrap()
            .read("target.py")
            .unwrap(),
        b"original"
    );
}

#[test]
fn worker_directory_creation_failure_removes_partial_workspace_without_charging() {
    let source = tempfile::tempdir().unwrap();
    fs::create_dir(source.path().join("nested")).unwrap();
    fs::write(source.path().join("nested/target.py"), b"payload").unwrap();
    let plan = WorkspacePlan::preflight(
        Utf8Path::from_path(source.path()).unwrap(),
        EffectId(7),
        1,
        CopyOptions::default(),
    )
    .unwrap();
    let mut opened_path = None;
    let error = plan
        .materialize_worker_with_root_opener(0, |path| {
            fs::write(path.join("nested"), b"blocks directory creation").unwrap();
            opened_path = Some(path.clone());
            WorkerRoot::open(path)
        })
        .unwrap_err();
    assert!(matches!(error, WorkspaceError::Io { .. }));
    assert_eq!(plan.observed_copy_bytes(), 0);
    assert!(!opened_path.unwrap().exists());
}

#[cfg(unix)]
#[test]
fn snapshot_writer_reports_destination_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    let destination_root = Utf8Path::from_path(destination.path()).unwrap();
    fs::write(source_root.join("permissions"), b"").unwrap();
    let permissions = fs::metadata(source_root.join("permissions"))
        .unwrap()
        .permissions();
    fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let mut writer =
        SnapshotWriter::new(source_root, destination_root, 1, PreflightLimits::default());
    let result = writer.write_reader(
        Utf8Path::new("target"),
        &mut std::io::Cursor::new(b"payload"),
        7,
        permissions,
    );
    fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "write shared snapshot",
            ..
        })
    ));
    assert_eq!(writer.logical_bytes, 0);
    assert!(writer.files.is_empty());
}

#[test]
fn temporary_storage_failures_keep_snapshot_and_worker_operation_context() {
    let parent = tempfile::tempdir().unwrap();
    let source = parent.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("target.py"), b"payload").unwrap();
    for mode in ["snapshot", "worker"] {
        let temporary_root = parent.path().join(mode);
        if mode == "snapshot" {
            fs::write(&temporary_root, b"not a directory").unwrap();
        } else {
            fs::create_dir(&temporary_root).unwrap();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "workspace::copy::coverage_tests::temporary_storage_failure_child",
                "--ignored",
            ])
            .env("HOIMIN_STORAGE_FAILURE_MODE", mode)
            .env("HOIMIN_STORAGE_FAILURE_SOURCE", &source)
            .env("TMPDIR", &temporary_root)
            .env("TMP", &temporary_root)
            .env("TEMP", &temporary_root)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "storage failure subprocess failed: {mode}"
        );
    }
}

#[test]
#[ignore = "isolated temporary-directory failure fixture; invoked by parent test"]
fn temporary_storage_failure_child() {
    let mode = std::env::var("HOIMIN_STORAGE_FAILURE_MODE").unwrap();
    let source = Utf8PathBuf::from(std::env::var("HOIMIN_STORAGE_FAILURE_SOURCE").unwrap());
    let plan = WorkspacePlan::preflight(&source, EffectId(7), 1, CopyOptions::default());
    if mode == "snapshot" {
        assert!(matches!(
            plan,
            Err(WorkspaceError::Io {
                operation: "create shared snapshot",
                ..
            })
        ));
    } else {
        let plan = plan.unwrap();
        let temporary_root = std::env::temp_dir();
        let moved_root = temporary_root.with_extension("moved");
        fs::rename(&temporary_root, moved_root).unwrap();
        fs::write(&temporary_root, b"not a directory").unwrap();
        assert!(matches!(
            plan.materialize_worker(0),
            Err(WorkspaceError::Io {
                operation: "create worker",
                ..
            })
        ));
        assert_eq!(plan.observed_copy_bytes(), 0);
    }
}

#[cfg(unix)]
struct FileSizeLimit {
    previous: libc::rlimit,
    previous_handler: libc::sighandler_t,
}

#[cfg(unix)]
impl FileSizeLimit {
    fn start(bytes: libc::rlim_t) -> Self {
        let mut previous = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: the kernel writes into a live rlimit value in this isolated subprocess.
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &raw mut previous) },
            0
        );
        // SAFETY: ignoring SIGXFSZ lets writes report EFBIG; the previous handler is restored.
        let previous_handler = unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) };
        let reduced = libc::rlimit {
            rlim_cur: bytes,
            rlim_max: previous.rlim_max,
        };
        // SAFETY: change only this fixture subprocess's soft file-size limit.
        assert_eq!(
            unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &raw const reduced) },
            0
        );
        Self {
            previous,
            previous_handler,
        }
    }
}

#[cfg(unix)]
impl Drop for FileSizeLimit {
    fn drop(&mut self) {
        // SAFETY: restore the captured process limit and signal handler before reporting results.
        unsafe {
            let _ = libc::setrlimit(libc::RLIMIT_FSIZE, &raw const self.previous);
            libc::signal(libc::SIGXFSZ, self.previous_handler);
        }
    }
}

#[cfg(unix)]
#[test]
fn file_size_limit_failures_rollback_snapshot_and_worker_accounting() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "workspace::copy::coverage_tests::file_size_limit_failure_child",
            "--ignored",
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated file-size limit fixture; invoked by parent test"]
fn file_size_limit_failure_child() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let source_root = Utf8Path::from_path(source.path()).unwrap();
    let destination_root = Utf8Path::from_path(destination.path()).unwrap();
    fs::write(source_root.join("target.py"), b"payload").unwrap();
    let permissions = fs::metadata(source_root.join("target.py"))
        .unwrap()
        .permissions();
    let mut writer =
        SnapshotWriter::new(source_root, destination_root, 1, PreflightLimits::default());
    let limit = FileSizeLimit::start(0);
    let result = writer.write_reader(
        Utf8Path::new("target.py"),
        &mut std::io::Cursor::new(b"payload"),
        7,
        permissions,
    );
    drop(limit);
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "write shared snapshot",
            ..
        })
    ));
    assert_eq!(writer.logical_bytes, 0);
    assert!(writer.files.is_empty());
    assert_eq!(
        fs::metadata(destination_root.join("target.py"))
            .unwrap()
            .len(),
        0
    );

    let restore_root = WorkerRoot::open(destination_root.to_owned()).unwrap();
    let permissions = fs::metadata(source_root.join("target.py"))
        .unwrap()
        .permissions();
    let limit = FileSizeLimit::start(0);
    let restore = restore_root.restore(Utf8Path::new("target.py"), b"payload", permissions);
    drop(limit);
    assert!(matches!(
        restore,
        Err(WorkspaceError::Io {
            operation: "write worker file",
            ..
        })
    ));

    let plan =
        WorkspacePlan::preflight(source_root, EffectId(7), 1, CopyOptions::default()).unwrap();
    let completed = plan.completed();
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: completed.aggregate_logical_bytes,
        processes: 1,
    });
    let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
    let request = grant.create_worker(EffectId(8), 0).unwrap();
    let limit = FileSizeLimit::start(0);
    let result = plan.create_worker(&request);
    drop(limit);
    assert!(matches!(
        result,
        Err(WorkspaceError::Io {
            operation: "copy worker file",
            ..
        })
    ));
    assert_eq!(plan.observed_copy_bytes(), 0);
    assert_eq!(plan.materialized_workers(), 0);
    assert_eq!(plan.reservation_id(), Some(grant.reservation_id()));
    assert_eq!(
        plan.create_worker(&request)
            .unwrap()
            .read("target.py")
            .unwrap(),
        b"payload"
    );
}

#[cfg(unix)]
#[test]
fn marker_file_size_failures_keep_serialization_and_newline_errors_reviewable() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "workspace::copy::coverage_tests::marker_file_size_failure_child",
            "--ignored",
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
#[test]
#[ignore = "isolated marker file-size limit fixture; invoked by parent test"]
fn marker_file_size_failure_child() {
    use super::super::owned::{ManagedRootCoordinator, OwnerKind};
    use std::os::unix::fs::MetadataExt;
    let parent = tempfile::tempdir().unwrap();
    let coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
    let root = ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap();
    let run_id = root
        .path()
        .file_name()
        .unwrap()
        .strip_prefix("run-")
        .unwrap();
    let marker = serde_json::json!({ "schema": 1, "run_id": run_id, "created_unix_seconds": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), "owner": "public_execution" });
    let marker_len = u64::try_from(serde_json::to_vec(&marker).unwrap().len()).unwrap();
    for limit_bytes in [0, marker_len] {
        let limit = FileSizeLimit::start(limit_bytes);
        let result = root.retain();
        drop(limit);
        assert!(matches!(
            result,
            Err(WorkspaceError::Io {
                operation: "write workspace marker",
                ..
            })
        ));
        fs::remove_file(root.path().join(".hoimin-retain.json")).unwrap();
    }
    root.retain().unwrap();
    let lease = fs::metadata(root.path().join(".hoimin-lease.json")).unwrap();
    let marker = serde_json::json!({ "schema": 1, "run_id": run_id, "owner": "public_execution", "lease_device": lease.dev(), "lease_inode": lease.ino() });
    let marker_len = u64::try_from(serde_json::to_vec(&marker).unwrap().len()).unwrap();
    for limit_bytes in [0, marker_len] {
        let limit = FileSizeLimit::start(limit_bytes);
        let result = root.mark_cleanup_ready();
        drop(limit);
        assert!(matches!(
            result,
            Err(WorkspaceError::Io {
                operation: "write workspace marker",
                ..
            })
        ));
        fs::remove_file(root.path().join(".hoimin-cleanup-ready.json")).unwrap();
    }
    root.mark_cleanup_ready().unwrap();
}
