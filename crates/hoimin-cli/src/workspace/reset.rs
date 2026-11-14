use std::collections::BTreeSet;
use std::io::Seek;

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
            self.snapshot.with_file(path, |reader| {
                let expected_size = self
                    .manifest
                    .entry(path)
                    .expect("snapshot is in manifest")
                    .size;
                if existing_files.contains(path)
                    && self.root.snapshot_matches_reader(
                        path,
                        reader,
                        expected_size,
                        snapshot.permission_fingerprint,
                    )?
                {
                    return Ok(());
                }
                crate::io_faults::checked_io!("reset.snapshot.rewind", reader.rewind())
                    .map_err(|error| WorkspaceError::io("rewind shared snapshot", path, error))?;
                self.root
                    .restore_from(path, reader, snapshot.permissions.clone())
            })?;
        }

        #[cfg(feature = "contracts")]
        {
            let matches = self.matches_snapshot()?;
            hoimin_core::contract_ensure!(
                "workspace.reset.post",
                matches,
                self.root.path().as_str(),
            );
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
            let matches = self.snapshot.with_file(path, |reader| {
                let expected_size = self
                    .manifest
                    .entry(path)
                    .expect("snapshot is in manifest")
                    .size;
                self.root.snapshot_matches_reader(
                    path,
                    reader,
                    expected_size,
                    snapshot.permission_fingerprint,
                )
            })?;
            if !matches {
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

    use super::super::{
        CopyOptions, ResetIoMetrics, WorkerWorkspace, WorkspaceError, WorkspacePlan,
        current_reset_io_metrics, reset_io_metrics,
    };
    use crate::source_file::{WorkspaceRaceHook, install_workspace_race_hook};

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

    #[rstest::rstest]
    #[case::enumeration("worker.walk.open")]
    #[case::compare_existing("worker.snapshot.inspect_entry")]
    #[case::rewind("reset.snapshot.rewind")]
    fn reset_io_failure_preserves_the_changed_file_and_allows_retry(#[case] point: &'static str) {
        let (_project, mut worker, _permissions) = changed_worker();
        let root = worker.root().to_owned();

        let fault = crate::io_faults::arm(point);

        let result = worker.reset();
        let fault_was_triggered = fault.triggered();

        let bytes_before_retry = worker.read("swap/target.py").unwrap();
        worker.reset().unwrap();

        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert!(
            fault_was_triggered,
            "reset I/O boundary was not checked: {point}"
        );
        assert!(
            matches!(result, Err(WorkspaceError::WorkspaceRestore { path, message }) if path == root && message.contains(point))
        );
        assert_eq!(bytes_before_retry, b"mutated!\n");
    }

    #[test]
    fn required_directory_recreation_failure_leaves_the_missing_directory_absent() {
        let (_project, mut worker, _permissions) = changed_worker();
        let root = worker.root().to_owned();
        fs::remove_dir_all(root.join("swap")).unwrap();

        let fault = crate::io_faults::arm("worker.directory.inspect");

        let result = worker.reset();
        let fault_was_triggered = fault.triggered();

        let required_directory_was_absent = !root.join("swap").exists();
        worker.reset().unwrap();

        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert!(fault_was_triggered);
        assert!(
            matches!(result, Err(WorkspaceError::WorkspaceRestore { path, message }) if path == root && message.contains("worker.directory.inspect"))
        );
        assert!(required_directory_was_absent);
    }

    #[test]
    fn reset_preserves_the_tree_depth_error_and_does_not_start_restoration() {
        let (_project, mut worker, _permissions) = changed_worker();
        let limit = super::super::MAX_WORKER_TREE_DEPTH;
        fs::create_dir_all(worker.root().join("d/".repeat(limit + 1))).unwrap();

        let result = worker.reset();
        fs::remove_dir_all(worker.root().join("d")).unwrap();

        let bytes_before_retry = worker.read("swap/target.py").unwrap();
        worker.reset().unwrap();

        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert!(
            matches!(result, Err(WorkspaceError::TreeDepthExceeded { limit: actual, .. }) if actual == limit)
        );
        assert_eq!(bytes_before_retry, b"mutated!\n");
    }

    #[rstest::rstest]
    #[case::unchanged(|_: &Utf8Path| {}, true)]
    #[case::extra_directory(|root: &Utf8Path| fs::create_dir(root.join("extra")).unwrap(), false)]
    #[case::extra_file(|root: &Utf8Path| fs::write(root.join("extra"), b"extra").unwrap(), false)]
    #[case::missing_file(|root: &Utf8Path| fs::remove_file(root.join("swap/target.py")).unwrap(), false)]
    #[case::changed_contents(|root: &Utf8Path| fs::write(root.join("swap/target.py"), b"different").unwrap(), false)]
    fn snapshot_comparison_checks_directory_file_and_content_equality(
        #[case] change: fn(&Utf8Path),
        #[case] expected: bool,
    ) {
        let (_project, mut worker, _permissions) = changed_worker();
        worker.reset().unwrap();
        change(worker.root());

        let matches = worker.matches_snapshot().unwrap();

        assert_eq!(matches, expected);
    }

    #[cfg(feature = "contracts")]
    #[test]
    fn a_concurrent_post_reset_change_triggers_the_restoration_contract() {
        let (_project, mut worker, _permissions) = changed_worker();
        let root = worker.root().to_owned();
        let changed_root = root.clone();

        let fault = crate::io_faults::arm_action("worker.walk.open", 1, move || {
            fs::write(changed_root.join("concurrent"), b"concurrent change").unwrap();
            Ok(())
        });
        let panic =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker.reset())).unwrap_err();
        let fault_was_triggered = fault.triggered();
        let message = panic
            .downcast::<String>()
            .expect("formatted contract panic has a String payload");

        assert!(message.contains("workspace.reset.post"));
        assert_eq!(
            fs::read(root.join("concurrent")).unwrap(),
            b"concurrent change"
        );
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert!(fault_was_triggered);
    }

    #[cfg(feature = "contracts")]
    #[test]
    fn a_post_reset_comparison_error_preserves_the_successful_restoration() {
        let (_project, mut worker, _permissions) = changed_worker();
        let root = worker.root().to_owned();

        let fault = crate::io_faults::arm_after("worker.snapshot.inspect_entry", 1);

        let result = worker.reset();
        let fault_was_triggered = fault.triggered();

        let bytes_before_retry = worker.read("swap/target.py").unwrap();
        worker.reset().unwrap();

        assert!(worker.matches_snapshot().unwrap());
        assert!(fault_was_triggered);
        assert!(
            matches!(result, Err(WorkspaceError::WorkspaceRestore { path, message }) if path == root && message.contains("worker.snapshot.inspect_entry"))
        );
        assert_eq!(bytes_before_retry, b"original\n");
    }

    #[cfg(unix)]
    #[test]
    fn an_unavailable_snapshot_is_reported_before_the_changed_worker_file_is_removed() {
        use std::os::unix::fs::PermissionsExt;
        let (_project, mut worker, _permissions) = changed_worker();
        let snapshot = worker.snapshot.root.join("swap/target.py");
        let parent = snapshot.parent().unwrap();
        let permissions = fs::metadata(parent).unwrap().permissions();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
        let held = parent.join("held-snapshot");
        fs::rename(&snapshot, &held).unwrap();

        let result = worker.reset();
        let restored = fs::rename(&held, &snapshot);
        let secured = fs::set_permissions(parent, permissions);
        restored.unwrap();
        secured.unwrap();

        let bytes_before_retry = worker.read("swap/target.py").unwrap();
        worker.reset().unwrap();

        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert!(
            matches!(result, Err(WorkspaceError::WorkspaceRestore { message, .. }) if message.contains("read shared snapshot"))
        );
        assert_eq!(bytes_before_retry, b"mutated!\n");
    }

    #[cfg(unix)]
    #[rstest::rstest]
    #[case::file("worker.cleanup.open_file", |root: &Utf8Path| fs::write(root.join("extra"), b"protected").unwrap())]
    #[case::directory("worker.removal.rmdir", |root: &Utf8Path| fs::create_dir(root.join("extra")).unwrap())]
    fn removal_failure_preserves_the_extra_entry_before_retry(
        #[case] point: &'static str,
        #[case] create: fn(&Utf8Path),
    ) {
        let (_project, mut worker, _permissions) = changed_worker();
        let root = worker.root().to_owned();
        create(&root);

        let fault = crate::io_faults::arm(point);

        let result = worker.reset();
        let fault_was_triggered = fault.triggered();

        let path_existed_before_retry = root.join("extra").exists();
        worker.reset().unwrap();

        assert!(!root.join("extra").exists());
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert!(fault_was_triggered);
        assert!(
            matches!(result, Err(WorkspaceError::WorkspaceRestore { path, message }) if path == root && message.contains(point))
        );
        assert!(path_existed_before_retry);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_removal_failure_never_touches_the_outside_target() {
        let (_project, mut worker, _permissions) = changed_worker();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("protected"), b"outside sentinel").unwrap();
        let link = worker.root().join("extra");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();

        let fault = crate::io_faults::arm("worker.link.remove");

        let result = worker.reset();
        let fault_was_triggered = fault.triggered();

        let failure_was_observed = fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink();
        let bytes_before_retry = fs::read(outside.path().join("protected")).unwrap();
        worker.reset().unwrap();

        assert!(fs::symlink_metadata(&link).is_err());
        assert_eq!(
            fs::read(outside.path().join("protected")).unwrap(),
            b"outside sentinel"
        );
        assert!(fault_was_triggered);
        assert!(
            matches!(result, Err(WorkspaceError::WorkspaceRestore { message, .. }) if message.contains("worker.link.remove"))
        );
        assert!(failure_was_observed);
        assert_eq!(bytes_before_retry, b"outside sentinel");
    }

    #[test]
    fn snapshot_comparison_preserves_a_directory_enumeration_error() {
        let (_project, worker, _permissions) = changed_worker();
        let fault = crate::io_faults::arm("worker.walk.open");
        let result = worker.matches_snapshot();
        assert!(fault.triggered());
        assert!(
            matches!(result, Err(WorkspaceError::Io { operation: "enumerate worker directory", message, .. }) if message.contains("worker.walk.open"))
        );
    }

    #[test]
    fn an_unrelated_operation_does_not_pause_reset_or_consume_its_gate() {
        let (opened, receiver) = sync_channel(0);
        let (_resume, resumed) = sync_channel(0);
        let hook = ResetPause {
            fired: AtomicBool::new(false),
            opened,
            resume: Mutex::new(resumed),
        };
        hook.parent_opened("snapshot", Utf8Path::new("swap/target.py"));
        assert!(!hook.fired.load(Ordering::SeqCst));
        assert_eq!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        );
    }

    #[test]
    fn a_missing_parent_replacement_reports_the_original_location() {
        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();
        let (target, permissions, error) = replace_paused_parent(root);
        assert_eq!(target, root.join("swap/target.py"));
        assert_eq!(permissions, Vec::<TestPermissionFingerprint>::new());
        assert_eq!(error.unwrap().kind(), std::io::ErrorKind::NotFound);
    }

    // APFS rejects these native names; Linux must exercise descriptor-relative removal.
    #[cfg(target_os = "linux")]
    #[test]
    fn reset_rejects_and_removes_a_non_utf8_extra_file_by_its_native_name() {
        use std::os::unix::ffi::OsStringExt;
        let (_project, mut worker, _permissions) = changed_worker();
        worker.reset().unwrap();
        let extra = worker
            .root()
            .as_std_path()
            .join(std::ffi::OsString::from_vec(b"native-\xff".to_vec()));
        fs::write(&extra, b"extra").unwrap();
        let failure_was_observed = !worker.matches_snapshot().unwrap();
        worker.reset().unwrap();

        assert!(!extra.exists());
        assert!(worker.matches_snapshot().unwrap());
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
        assert!(failure_was_observed);
    }

    #[test]
    fn reset_rereads_only_changed_snapshot_contents_for_restoration() {
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
                snapshot_bytes: fixture_bytes + b"original\n".len() as u64,
            }
        );
        #[cfg(feature = "contracts")]
        assert_eq!(
            metrics,
            ResetIoMetrics {
                tree_walks: 2,
                worker_bytes: 2 * fixture_bytes,
                snapshot_bytes: 2 * fixture_bytes + b"original\n".len() as u64,
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

        #[cfg(feature = "contracts")]
        let expected_reads = 18;
        #[cfg(not(feature = "contracts"))]
        let expected_reads = 9;
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

        worker.reset().unwrap();
        for _ in 1..CYCLES {
            worker.write("swap/target.py", b"mutated!\n").unwrap();
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

    fn replace_paused_parent(
        root: &Utf8Path,
    ) -> (
        camino::Utf8PathBuf,
        Vec<TestPermissionFingerprint>,
        Option<std::io::Error>,
    ) {
        match fs::rename(root.join("swap"), root.join("held")) {
            Ok(()) => {
                fs::create_dir(root.join("swap")).unwrap();
                let outside = root.join("swap/target.py");
                fs::write(&outside, b"outside\n").unwrap();
                make_read_only(&outside);
                (
                    root.join("held/target.py"),
                    vec![permission_fingerprint(&outside)],
                    None,
                )
            }
            Err(error) => (root.join("swap/target.py"), vec![], Some(error)),
        }
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
        let (operated_target, outside_permissions, rename_error) = replace_paused_parent(&root);
        resume_tx.send(()).unwrap();
        let (worker, result) = operation.join().unwrap();

        #[cfg(windows)]
        assert!(
            result.is_ok() || matches!(result, Err(WorkspaceError::WorkspaceRestore { .. })),
            "{result:?}"
        );
        #[cfg(not(windows))]
        result.unwrap();
        assert_eq!(fs::read(operated_target.clone()).unwrap(), b"original\n");
        assert_eq!(
            permission_fingerprint(&operated_target.clone()),
            snapshot_permissions
        );
        #[cfg(windows)]
        let expected_rename_error =
            [Some(Some(32)), None][usize::from(!outside_permissions.is_empty())];
        #[cfg(not(windows))]
        let expected_rename_error = None;
        assert_eq!(
            rename_error.as_ref().map(std::io::Error::raw_os_error),
            expected_rename_error
        );
        for permissions in &outside_permissions {
            let outside = root.join("swap/target.py");

            assert_eq!(fs::read(&outside).unwrap(), b"outside\n");
            assert_eq!(permission_fingerprint(&outside), *permissions);
        }
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
