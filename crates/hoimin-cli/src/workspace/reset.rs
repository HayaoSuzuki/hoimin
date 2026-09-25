use std::collections::BTreeSet;

use super::root::WorkerEntryKind;
use super::{WorkerWorkspace, WorkspaceError};

impl WorkerWorkspace {
    /// Restores the worker filesystem from its private preflight snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if restoration cannot complete.
    pub fn reset(&mut self) -> Result<(), WorkspaceError> {
        self.reset_from_snapshot().map_err(|error| {
            if matches!(
                error,
                WorkspaceError::WorkspaceRestore { .. } | WorkspaceError::TreeDepthExceeded { .. }
            ) {
                error
            } else {
                WorkspaceError::WorkspaceRestore {
                    path: self.root.path().to_owned(),
                    message: error.to_string(),
                }
            }
        })
    }

    fn reset_from_snapshot(&self) -> Result<(), WorkspaceError> {
        let existing = self.root.entries()?;
        let required_directories = self.manifest.directories().iter().collect::<BTreeSet<_>>();
        let existing_files = existing
            .iter()
            .filter(|entry| entry.kind == WorkerEntryKind::File)
            .filter_map(|entry| entry.logical_path.clone())
            .collect::<BTreeSet<_>>();
        for entry in existing.iter().rev() {
            let remove = || {
                self.root
                    .remove_native_if_exists(&entry.native_path, entry.logical_path.as_deref())
            };
            match entry.kind {
                WorkerEntryKind::Directory => {
                    if entry
                        .logical_path
                        .as_ref()
                        .is_none_or(|path| !required_directories.contains(&path))
                    {
                        remove()?;
                    }
                }
                WorkerEntryKind::File => {
                    if entry
                        .logical_path
                        .as_ref()
                        .is_none_or(|path| !self.snapshot.files.contains_key(path))
                    {
                        remove()?;
                    }
                }
                WorkerEntryKind::LinkOrReparse => {
                    remove()?;
                }
            }
        }

        for directory in self.manifest.directories() {
            self.root.ensure_directory(directory)?;
        }
        for (path, snapshot) in &self.snapshot.files {
            let bytes = self.snapshot.read(path)?;
            if existing_files.contains(path)
                && self
                    .root
                    .snapshot_matches(path, &bytes, snapshot.permission_fingerprint)?
            {
                continue;
            }
            self.root
                .restore(path, &bytes, snapshot.permissions.clone())?;
        }

        #[cfg(feature = "contracts")]
        {
            let matches = self.matches_snapshot()?;
            hoimin_core::contract_ensure!(
                "workspace.reset.post",
                matches,
                self.root.path().as_str(),
            );
            if !matches {
                return Err(WorkspaceError::WorkspaceRestore {
                    path: self.root.path().to_owned(),
                    message: "post-reset manifest comparison failed".to_owned(),
                });
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "contracts"))]
    fn matches_snapshot(&self) -> Result<bool, WorkspaceError> {
        let entries = self.root.entries()?;
        if entries.iter().any(|entry| entry.logical_path.is_none()) {
            return Ok(false);
        }
        let actual_directories = entries
            .iter()
            .filter(|entry| entry.kind == WorkerEntryKind::Directory)
            .filter_map(|entry| entry.logical_path.as_ref())
            .collect::<BTreeSet<_>>();
        if actual_directories != self.manifest.directories().iter().collect() {
            return Ok(false);
        }
        let actual_files = entries
            .iter()
            .filter(|entry| entry.kind != WorkerEntryKind::Directory)
            .filter_map(|entry| entry.logical_path.as_ref())
            .collect::<BTreeSet<_>>();
        let expected_files = self.snapshot.files.keys().collect::<BTreeSet<_>>();
        if actual_files != expected_files {
            return Ok(false);
        }
        for (path, snapshot) in &self.snapshot.files {
            let bytes = self.snapshot.read(path)?;
            if !self
                .root
                .snapshot_matches(path, &bytes, snapshot.permission_fingerprint)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::sync_channel;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use camino::Utf8Path;
    use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

    use super::super::root::{WorkspaceRaceHook, install_workspace_race_hook};
    use super::super::{
        CopyOptions, ResetIoMetrics, WorkerWorkspace, WorkspaceError, WorkspacePlan,
        current_reset_io_metrics, reset_io_metrics,
    };

    #[cfg(unix)]
    type TestPermissionFingerprint = u32;
    #[cfg(windows)]
    type TestPermissionFingerprint = bool;

    #[cfg(unix)]
    fn permission_fingerprint(path: &Utf8Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode()
    }

    #[cfg(windows)]
    fn permission_fingerprint(path: &Utf8Path) -> bool {
        fs::metadata(path).unwrap().permissions().readonly()
    }

    fn make_read_only(path: &Utf8Path) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
    }

    struct ResetPause {
        fired: AtomicBool,
        opened: std::sync::mpsc::SyncSender<()>,
        resume: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl WorkspaceRaceHook for ResetPause {
        fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
            if operation == "reset"
                && path == Utf8Path::new("swap/target.py")
                && !self.fired.swap(true, Ordering::SeqCst)
            {
                self.opened.send(()).unwrap();
                self.resume
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
        }
    }

    fn changed_worker() -> (
        tempfile::TempDir,
        WorkerWorkspace,
        TestPermissionFingerprint,
    ) {
        changed_worker_with_padding(0)
    }

    fn changed_worker_with_padding(
        padding_bytes: usize,
    ) -> (
        tempfile::TempDir,
        WorkerWorkspace,
        TestPermissionFingerprint,
    ) {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("swap")).unwrap();
        fs::write(project.path().join("swap/target.py"), b"original\n").unwrap();
        if padding_bytes > 0 {
            fs::write(
                project.path().join("swap/padding.bin"),
                vec![b'x'; padding_bytes],
            )
            .unwrap();
        }
        let root = Utf8Path::from_path(project.path()).unwrap();
        let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: plan.aggregate_bytes(),
            processes: 1,
        });
        let reservation = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
        let mut worker = plan
            .create_worker(&reservation.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let snapshot_permissions = permission_fingerprint(&worker.root().join("swap/target.py"));
        worker.write("swap/target.py", b"mutated!\n").unwrap();
        (project, worker, snapshot_permissions)
    }

    #[test]
    fn reset_reads_each_snapshot_and_worker_file_once() {
        const PADDING_BYTES: usize = 1024 * 1024;
        let (_project, mut worker, _snapshot_permissions) =
            changed_worker_with_padding(PADDING_BYTES);
        reset_io_metrics();

        worker.reset().unwrap();

        let fixture_bytes = (PADDING_BYTES + b"original\n".len()) as u64;
        let metrics = current_reset_io_metrics();
        #[cfg(not(feature = "contracts"))]
        assert_eq!(
            metrics,
            ResetIoMetrics {
                tree_walks: 1,
                worker_bytes: fixture_bytes,
                snapshot_bytes: fixture_bytes,
            }
        );
        #[cfg(feature = "contracts")]
        assert_eq!(
            metrics,
            ResetIoMetrics {
                tree_walks: 2,
                worker_bytes: 2 * fixture_bytes,
                snapshot_bytes: 2 * fixture_bytes,
            }
        );
    }

    #[test]
    fn reset_skips_reading_shorter_worker_contents() {
        assert_size_mismatch_skips_worker_read(4);
    }

    #[test]
    fn reset_skips_reading_larger_worker_contents() {
        assert_size_mismatch_skips_worker_read(2 * 1024 * 1024);
    }

    #[test]
    fn reset_restores_empty_worker_contents() {
        assert_size_mismatch_skips_worker_read(0);
    }

    fn assert_size_mismatch_skips_worker_read(changed_bytes: usize) {
        const PADDING_BYTES: usize = 1024 * 1024;
        let (_project, mut worker, snapshot_permissions) =
            changed_worker_with_padding(PADDING_BYTES);
        worker
            .write("swap/target.py", &vec![b'x'; changed_bytes])
            .unwrap();
        reset_io_metrics();

        worker.reset().unwrap();

        let fixture_bytes = (PADDING_BYTES + b"original\n".len()) as u64;
        let verification_passes = u64::from(cfg!(feature = "contracts"));
        assert_eq!(
            current_reset_io_metrics(),
            ResetIoMetrics {
                tree_walks: 1 + verification_passes,
                worker_bytes: PADDING_BYTES as u64 + verification_passes * fixture_bytes,
                snapshot_bytes: (1 + verification_passes) * fixture_bytes,
            }
        );
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert_eq!(
            permission_fingerprint(&worker.root().join("swap/target.py")),
            snapshot_permissions
        );
    }

    #[test]
    fn reset_compares_same_size_contents_with_preserved_mtime() {
        let (_project, mut worker, _snapshot_permissions) = changed_worker();
        worker.reset().unwrap();
        let path = worker.root().join("swap/target.py");
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, b"mutated!\n").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        reset_io_metrics();

        worker.reset().unwrap();

        let expected_reads = if cfg!(feature = "contracts") { 18 } else { 9 };
        assert_eq!(current_reset_io_metrics().worker_bytes, expected_reads);
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
    }

    #[test]
    #[ignore = "manual before/after performance evidence"]
    fn benchmark_workspace_reset_io() {
        const CYCLES: u64 = 10;
        const PADDING_BYTES: usize = 8 * 1024 * 1024;
        let (_project, mut worker, _snapshot_permissions) =
            changed_worker_with_padding(PADDING_BYTES);
        reset_io_metrics();
        let started = Instant::now();

        for cycle in 0..CYCLES {
            if cycle > 0 {
                worker.write("swap/target.py", b"mutated!\n").unwrap();
            }
            worker.reset().unwrap();
        }

        let metrics = current_reset_io_metrics();
        eprintln!(
            "cycles={CYCLES} fixture_bytes={} tree_walks={} worker_bytes={} snapshot_bytes={} elapsed_ms={}",
            PADDING_BYTES + b"original\n".len(),
            metrics.tree_walks,
            metrics.worker_bytes,
            metrics.snapshot_bytes,
            started.elapsed().as_millis()
        );
    }

    #[test]
    fn parent_replacement_reset_restore_uses_the_opened_parent() {
        let (_project, worker, snapshot_permissions) = changed_worker();
        let root = worker.root().to_owned();
        let permissions = fs::metadata(root.join("swap/target.py"))
            .unwrap()
            .permissions();
        let (opened_tx, opened_rx) = sync_channel(0);
        let (resume_tx, resume_rx) = sync_channel(0);
        let hook = Arc::new(ResetPause {
            fired: AtomicBool::new(false),
            opened: opened_tx,
            resume: Mutex::new(resume_rx),
        });
        let thread_hook = Arc::clone(&hook);
        let operation = thread::spawn(move || {
            let _guard = install_workspace_race_hook(thread_hook);
            let result =
                worker
                    .root
                    .restore(Utf8Path::new("swap/target.py"), b"original\n", permissions);
            (worker, result)
        });

        opened_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if let Err(error) = fs::rename(root.join("swap"), root.join("held")) {
            #[cfg(windows)]
            {
                assert_eq!(error.raw_os_error(), Some(32));
                resume_tx.send(()).unwrap();
                let (worker, result) = operation.join().unwrap();
                assert!(
                    result.is_ok()
                        || matches!(result, Err(WorkspaceError::WorkspaceRestore { .. })),
                    "{result:?}"
                );
                assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
                return;
            }
            #[cfg(not(windows))]
            panic!("rename of opened reset parent failed unexpectedly: {error}");
        }
        fs::create_dir(root.join("swap")).unwrap();
        let outside = root.join("swap/target.py");
        fs::write(&outside, b"outside\n").unwrap();
        make_read_only(&outside);
        let outside_permissions = permission_fingerprint(&outside);
        resume_tx.send(()).unwrap();
        let (worker, result) = operation.join().unwrap();

        assert!(
            result.is_ok() || matches!(result, Err(WorkspaceError::WorkspaceRestore { .. })),
            "{result:?}"
        );
        assert_eq!(
            fs::read(root.join("held/target.py")).unwrap(),
            b"original\n"
        );
        assert_eq!(
            permission_fingerprint(&root.join("held/target.py")),
            snapshot_permissions
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside\n");
        assert_eq!(permission_fingerprint(&outside), outside_permissions);
        drop(worker);
    }

    #[test]
    fn reset_recreates_a_deleted_snapshot_parent_directory() {
        let (_project, mut worker, snapshot_permissions) = changed_worker();
        worker
            .root
            .remove_any_if_exists(Utf8Path::new("swap"))
            .unwrap();

        worker.reset().unwrap();

        let restored = worker.root().join("swap/target.py");
        assert_eq!(fs::read(&restored).unwrap(), b"original\n");
        assert_eq!(permission_fingerprint(&restored), snapshot_permissions);
    }

    #[test]
    fn matches_snapshot_detects_changed_bytes() {
        let (_project, worker, _snapshot_permissions) = changed_worker();

        assert!(!worker.matches_snapshot().unwrap());
    }

    #[test]
    fn shared_snapshot_restores_after_original_source_changes() {
        let (project, worker, _snapshot_permissions) = changed_worker();
        fs::write(project.path().join("swap/target.py"), b"external change\n").unwrap();

        worker.reset_from_snapshot().unwrap();

        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
    }

    #[cfg(unix)]
    #[test]
    fn unchanged_reset_keeps_the_existing_file_object() {
        use std::os::unix::fs::MetadataExt;

        let (_project, mut worker, _snapshot_permissions) = changed_worker();
        worker.reset().unwrap();
        let target = worker.root().join("swap/target.py");
        let inode = fs::metadata(&target).unwrap().ino();

        worker.reset().unwrap();

        assert_eq!(fs::metadata(target).unwrap().ino(), inode);
    }
}
