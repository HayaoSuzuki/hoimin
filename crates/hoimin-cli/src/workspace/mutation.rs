use std::io::{Seek, SeekFrom, Write};

use hoimin_core::{
    CandidateDescriptor, CandidateValidationContext, CandidateValidationError, MutationCandidate,
    validate_candidate_with_context,
};

use super::{WorkerWorkspace, WorkspaceError};

impl WorkerWorkspace {
    /// Applies a candidate only when the worker target still matches its preflight manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when the worker target is invalid or the mutated file cannot be written.
    pub fn apply_mutation(&mut self, candidate: &MutationCandidate) -> Result<(), WorkspaceError> {
        let expected = self.manifest.entry(&candidate.path).ok_or_else(|| {
            WorkspaceError::MutationTargetMissing {
                path: candidate.path.clone(),
            }
        })?;
        let mut target = self.root.open_mutation_file(&candidate.path)?;
        let mut bytes = Vec::new();
        target.read_to_end(&mut bytes, expected.size, &candidate.path)?;
        let validation = mutation_context(
            &bytes,
            candidate,
            expected.blake3,
            CandidateValidationContext::new,
        )?;
        let stable_id =
            validate_candidate_with_context(&validation, &CandidateDescriptor::from(candidate))
                .map_err(|error| match error {
                    CandidateValidationError::FileHashMismatch => {
                        WorkspaceError::MutationHashMismatch {
                            path: candidate.path.clone(),
                        }
                    }
                    CandidateValidationError::OriginalMismatch => {
                        WorkspaceError::MutationOriginalMismatch {
                            path: candidate.path.clone(),
                        }
                    }
                    CandidateValidationError::SourceTooLarge
                    | CandidateValidationError::UnsupportedSchema
                    | CandidateValidationError::InvalidPath
                    | CandidateValidationError::SpanOutOfBounds
                    | CandidateValidationError::LocationMismatch
                    | CandidateValidationError::InvalidEncoding(_)
                    | CandidateValidationError::InvalidUtf8
                    | CandidateValidationError::InvalidMutation => {
                        WorkspaceError::MutationSpanInvalid {
                            path: candidate.path.clone(),
                        }
                    }
                })?;
        if stable_id.as_str() != candidate.id {
            return Err(WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            });
        }

        let start = usize::try_from(candidate.span.start).map_err(|_| {
            WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            }
        })?;
        let length = usize::try_from(candidate.span.length).map_err(|_| {
            WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            }
        })?;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            })?;
        let encoding = validation
            .decoded_source()
            .map_err(|_| WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            })?
            .encoding();
        let replacement = encoding.encode(&candidate.replacement).map_err(|_| {
            WorkspaceError::MutationSpanInvalid {
                path: candidate.path.clone(),
            }
        })?;
        // Release every borrow of the source before reusing its owned buffer.
        drop(validation);
        // Contract builds retain independent pre-mutation evidence. Normal
        // builds do not keep a second full body for a disabled postcondition.
        #[cfg(feature = "contracts")]
        let original = bytes.clone();
        drop(bytes.splice(start..end, replacement.iter().copied()));
        let mut file = target.into_writable(&candidate.path)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;
        file.set_len(0)
            .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;
        file.write_all(&bytes)
            .and_then(|()| file.flush())
            .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;

        hoimin_core::contract_ensure!(
            "workspace.mutation.post",
            bytes[..start] == original[..start]
                && bytes[start + replacement.len()..] == original[end..],
            &candidate.path,
        );
        Ok(())
    }
}

/// Build source facts once while preserving hash-mismatch precedence even when
/// construction fails on the raw/decoded size boundary. The private factory
/// lets tests exercise that branch without allocating multi-gigabyte sources.
fn mutation_context<'source>(
    source: &'source [u8],
    candidate: &MutationCandidate,
    expected_hash: blake3::Hash,
    build: impl FnOnce(
        &'source [u8],
    ) -> Result<CandidateValidationContext<'source>, CandidateValidationError>,
) -> Result<CandidateValidationContext<'source>, WorkspaceError> {
    let expected = expected_hash.to_hex();
    if candidate.file_hash != expected.as_str() {
        return Err(WorkspaceError::MutationHashMismatch {
            path: candidate.path.clone(),
        });
    }
    let validation = build(source);
    let matches = match &validation {
        Ok(context) => context.file_hash() == expected.as_str(),
        // A failed constructor has not produced a usable hash. Only this
        // exceptional path needs a separate raw hash before choosing the error.
        Err(_) => blake3::hash(source) == expected_hash,
    };
    if !matches {
        return Err(WorkspaceError::MutationHashMismatch {
            path: candidate.path.clone(),
        });
    }
    validation.map_err(|_| WorkspaceError::MutationSpanInvalid {
        path: candidate.path.clone(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::sync_channel;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{
        BudgetLedger, ByteSpan, CandidateDescriptor, EffectId, MutationCandidate, RunBudgets,
        reserve_workspace_copy,
    };

    use super::super::manifest::{build_metrics, reset_build_metrics};
    use super::super::root::{WorkspaceRaceHook, install_workspace_race_hook};
    use super::super::{CopyOptions, WorkerWorkspace, WorkspacePlan};

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

    struct MutationPause {
        fired: AtomicBool,
        opened: std::sync::mpsc::SyncSender<()>,
        resume: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl WorkspaceRaceHook for MutationPause {
        fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
            if operation == "mutation"
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

    fn worker_and_candidate() -> (tempfile::TempDir, WorkerWorkspace, MutationCandidate) {
        worker_and_candidate_with_padding(0)
    }

    fn worker_and_candidate_with_padding(
        padding_bytes: usize,
    ) -> (tempfile::TempDir, WorkerWorkspace, MutationCandidate) {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("swap")).unwrap();
        fs::write(project.path().join("swap/target.py"), b"original\n").unwrap();
        fs::write(
            project.path().join("swap/padding.bin"),
            vec![b'x'; padding_bytes],
        )
        .unwrap();
        let root = Utf8Path::from_path(project.path()).unwrap();
        let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: plan.aggregate_bytes(),
            processes: 1,
        });
        let reservation = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
        let worker = plan
            .create_worker(&reservation.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let hash = worker
            .manifest()
            .entry(Utf8Path::new("swap/target.py"))
            .unwrap()
            .blake3
            .to_hex()
            .to_string();
        let mut candidate = MutationCandidate {
            id: "race".into(),
            sequence: 0,
            path: Utf8PathBuf::from("swap/target.py"),
            span: ByteSpan {
                start: 0,
                length: 8,
            },
            original: "original".into(),
            replacement: "mutated!".into(),
            operator: "test".into(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: hash,
        };
        candidate.id =
            hoimin_core::validate_candidate(b"original\n", &CandidateDescriptor::from(&candidate))
                .unwrap()
                .to_string();
        (project, worker, candidate)
    }

    fn source_worker(
        source: &[u8],
        span: ByteSpan,
        original: &str,
        replacement: &str,
        location: (u32, u32),
    ) -> (tempfile::TempDir, WorkerWorkspace, MutationCandidate) {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("source.py"), source).unwrap();
        let root = Utf8Path::from_path(project.path()).unwrap();
        let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: plan.aggregate_bytes(),
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
        let worker = plan
            .create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let mut candidate = MutationCandidate {
            id: String::new(),
            sequence: 0,
            path: "source.py".into(),
            span,
            original: original.into(),
            replacement: replacement.into(),
            operator: "test".into(),
            line: location.0,
            column: location.1,
            symbol: None,
            file_hash: blake3::hash(source).to_hex().to_string(),
        };
        candidate.id = hoimin_core::stable_mutant_id(&hoimin_core::CandidateIdentity::from(
            &CandidateDescriptor::from(&candidate),
        ))
        .to_string();
        (project, worker, candidate)
    }

    struct MutationReadObservation {
        target: Utf8PathBuf,
        changed: Vec<u8>,
        consumed: std::sync::atomic::AtomicU64,
    }

    impl WorkspaceRaceHook for MutationReadObservation {
        fn parent_opened(&self, operation: &'static str, _path: &Utf8Path) {
            if operation == "mutation-budget" {
                // The writer runs only after the reader has fixed its descriptor budget.
                if self.changed.len() > b"original\n".len() {
                    use std::io::Write;
                    fs::OpenOptions::new()
                        .append(true)
                        .open(&self.target)
                        .unwrap()
                        .write_all(&self.changed[b"original\n".len()..])
                        .unwrap();
                } else {
                    fs::OpenOptions::new()
                        .write(true)
                        .open(&self.target)
                        .unwrap()
                        .set_len(self.changed.len() as u64)
                        .unwrap();
                }
                make_read_only(&self.target);
            }
        }

        fn mutation_bytes_read(&self, bytes: u64) {
            self.consumed.store(bytes, Ordering::SeqCst);
        }
    }

    #[test]
    fn issue_1082_apply_rejects_append_and_truncate_with_finite_descriptor_read() {
        let mut appended = b"original\n".to_vec();
        appended.extend_from_slice(&[b'x'; 4096]);
        for changed in [appended, b"ori".to_vec()] {
            let (project, mut worker, candidate) = worker_and_candidate();
            let target = worker.root().join(&candidate.path);
            let hook = Arc::new(MutationReadObservation {
                target: target.clone(),
                changed: changed.clone(),
                consumed: std::sync::atomic::AtomicU64::new(u64::MAX),
            });
            let guard = install_workspace_race_hook(hook.clone());
            let result = worker.apply_mutation(&candidate);
            drop(guard);
            assert!(matches!(
                result,
                Err(super::WorkspaceError::MutationHashMismatch { .. })
            ));
            assert_eq!(fs::read(&target).unwrap(), changed);
            assert!(fs::metadata(&target).unwrap().permissions().readonly());
            assert_eq!(
                fs::read(project.path().join(&candidate.path)).unwrap(),
                b"original\n"
            );
            let consumed = hook.consumed.load(Ordering::SeqCst);
            assert!(
                consumed <= b"original\n".len() as u64 + 1,
                "consumed {consumed} bytes after a 9-byte budget"
            );
        }
    }

    #[test]
    fn source_reuse_preserves_growth_shrink_unicode_bom_and_latin1() {
        for (source, original, replacement, start, length, location, expected) in [
            (
                &b"# old\n"[..],
                "old",
                "longer",
                2,
                3,
                (1, 2),
                &b"# longer\n"[..],
            ),
            (
                &b"# original\n"[..],
                "original",
                "x",
                2,
                8,
                (1, 2),
                &b"# x\n"[..],
            ),
            (
                "# éx\n".as_bytes(),
                "é",
                "顔",
                2,
                2,
                (1, 2),
                "# 顔x\n".as_bytes(),
            ),
            (
                &b"\xef\xbb\xbf# old\n"[..],
                "old",
                "xy",
                5,
                3,
                (1, 2),
                &b"\xef\xbb\xbf# xy\n"[..],
            ),
            (
                &b"# coding: latin-1\n# \xe9x\n"[..],
                "é",
                "àà",
                20,
                1,
                (2, 2),
                &b"# coding: latin-1\n# \xe0\xe0x\n"[..],
            ),
        ] {
            let (project, mut worker, candidate) = source_worker(
                source,
                ByteSpan { start, length },
                original,
                replacement,
                location,
            );
            worker.apply_mutation(&candidate).unwrap();
            assert_eq!(worker.read("source.py").unwrap(), expected);
            assert_eq!(fs::read(project.path().join("source.py")).unwrap(), source);
            worker.reset().unwrap();
            assert_eq!(worker.read("source.py").unwrap(), source);
        }
    }

    #[test]
    fn unrepresentable_replacement_preserves_worker_bytes_and_permissions() {
        let source = b"# coding: latin-1\n# \xe9x\n";
        let (project, mut worker, candidate) = source_worker(
            source,
            ByteSpan {
                start: 20,
                length: 1,
            },
            "é",
            "顔",
            (2, 2),
        );
        let target = worker.root().join("source.py");
        make_read_only(&target);
        let permissions = permission_fingerprint(&target);
        assert!(matches!(
            worker.apply_mutation(&candidate),
            Err(super::WorkspaceError::MutationSpanInvalid { .. })
        ));
        assert_eq!(worker.read("source.py").unwrap(), source);
        assert_eq!(fs::read(project.path().join("source.py")).unwrap(), source);
        assert_eq!(permission_fingerprint(&target), permissions);
    }

    #[test]
    fn hash_mismatch_precedes_invalid_span_and_location_without_writing() {
        for changed_worker in [false, true] {
            let (_project, mut worker, mut candidate) = worker_and_candidate();
            candidate.span.start = u64::MAX;
            candidate.line = u32::MAX;
            if changed_worker {
                fs::write(worker.root().join(&candidate.path), b"changed\n").unwrap();
            } else {
                candidate.file_hash = "0".repeat(64);
            }
            let before = worker.read(&candidate.path).unwrap();
            assert!(matches!(
                worker.apply_mutation(&candidate),
                Err(super::WorkspaceError::MutationHashMismatch { .. })
            ));
            assert_eq!(worker.read(&candidate.path).unwrap(), before);
        }
    }

    #[test]
    fn mutation_context_failure_keeps_hash_mismatch_precedence() {
        let (_project, _worker, mut candidate) = worker_and_candidate();
        let source = b"original\n";
        let current = blake3::hash(source);
        let different = blake3::hash(b"changed\n");
        for (expected, candidate_hash, hash_failure) in [
            (current, current, false),
            (different, different, true),
            (current, different, true),
        ] {
            candidate.file_hash = candidate_hash.to_hex().to_string();
            let result = super::mutation_context(source, &candidate, expected, |_| {
                Err(hoimin_core::CandidateValidationError::SourceTooLarge)
            });
            if hash_failure {
                assert!(matches!(
                    result,
                    Err(super::WorkspaceError::MutationHashMismatch { .. })
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(super::WorkspaceError::MutationSpanInvalid { .. })
                ));
            }
        }
    }

    #[test]
    fn mutation_and_reset_do_not_rebuild_the_original_manifest() {
        let (_project, mut worker, candidate) = worker_and_candidate();
        reset_build_metrics();

        worker.apply_mutation(&candidate).unwrap();
        worker.reset().unwrap();

        assert_eq!(build_metrics(), (0, 0));
    }

    #[test]
    #[ignore = "manual before/after performance evidence"]
    fn benchmark_original_manifest_work_per_mutant_cycle() {
        const CYCLES: u64 = 10;
        const PADDING_BYTES: usize = 8 * 1024 * 1024;
        let (_project, mut worker, candidate) = worker_and_candidate_with_padding(PADDING_BYTES);
        reset_build_metrics();
        let started = Instant::now();

        for _ in 0..CYCLES {
            worker.apply_mutation(&candidate).unwrap();
            worker.reset().unwrap();
        }

        let (builds, bytes) = build_metrics();
        eprintln!(
            "cycles={CYCLES} fixture_bytes={} manifest_builds={builds} manifest_bytes={bytes} elapsed_ms={}",
            PADDING_BYTES + b"original\n".len(),
            started.elapsed().as_millis()
        );
    }

    #[test]
    fn mutation_rejects_candidate_hash_when_worker_bytes_still_match() {
        let (_project, mut worker, mut candidate) = worker_and_candidate();
        candidate.file_hash = "not-the-manifest-hash".into();

        assert!(matches!(
            worker.apply_mutation(&candidate),
            Err(super::WorkspaceError::MutationHashMismatch { .. })
        ));
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n",);
    }

    #[derive(Clone, Copy)]
    enum SharedReferent {
        Original,
        Sentinel,
    }

    fn assert_shared_mutation_rejected(
        referent: SharedReferent,
        reset: bool,
        readonly: bool,
        extra_links: u8,
    ) {
        // Arrange: a baseline command can replace the worker copy without changing its bytes.
        let (project, mut worker, candidate) = worker_and_candidate();
        let outside = tempfile::tempdir().unwrap();
        let source = match referent {
            SharedReferent::Original => Utf8Path::from_path(project.path())
                .unwrap()
                .join("swap/target.py"),
            SharedReferent::Sentinel => {
                let source = Utf8Path::from_path(outside.path())
                    .unwrap()
                    .join("sentinel.py");
                fs::write(&source, b"original\n").unwrap();
                source
            }
        };
        let initial_permissions = fs::metadata(&source).unwrap().permissions();
        if readonly {
            make_read_only(&source);
        }
        let permissions = permission_fingerprint(&source);
        let target = worker.root().join(&candidate.path);
        fs::remove_file(&target).unwrap();
        fs::hard_link(&source, &target).unwrap();
        for index in 0..extra_links {
            fs::hard_link(&source, outside.path().join(format!("alias-{index}"))).unwrap();
        }
        if reset {
            worker.reset().unwrap();
        }

        // Act.
        let result = worker.apply_mutation(&candidate);

        // Assert: rejection must precede both permission changes and mutation writes.
        assert_eq!(permission_fingerprint(&source), permissions);
        assert_eq!(fs::read(&source).unwrap(), b"original\n");
        let error = result.expect_err("shared mutation target was accepted");
        assert!(matches!(
            &error,
            super::WorkspaceError::MutationTargetShared { path } if path == &candidate.path
        ));
        assert_eq!(error.code(), "workspace.mutation.shared");
        assert_eq!(fs::read(&target).unwrap(), b"original\n");

        // Cleanup may fail on filesystems that cannot unlink readonly shared files safely.
        let cleanup = worker.try_cleanup();
        #[cfg(unix)]
        cleanup.unwrap();
        #[cfg(windows)]
        let _ = cleanup;
        drop(worker);
        assert_eq!(permission_fingerprint(&source), permissions);
        assert_eq!(fs::read(&source).unwrap(), b"original\n");
        // Restore test-owned setup after checking the invariant, for Windows TempDir cleanup.
        fs::set_permissions(&source, initial_permissions).unwrap();
    }

    #[test]
    fn shared_mutation_rejects_original_after_baseline() {
        assert_shared_mutation_rejected(SharedReferent::Original, false, false, 0);
    }

    #[test]
    fn shared_mutation_rejects_original_after_reset() {
        assert_shared_mutation_rejected(SharedReferent::Original, true, false, 0);
    }

    #[test]
    fn shared_mutation_rejects_external_sentinel_after_baseline() {
        assert_shared_mutation_rejected(SharedReferent::Sentinel, false, false, 0);
    }

    #[test]
    fn shared_mutation_rejects_external_sentinel_after_reset() {
        assert_shared_mutation_rejected(SharedReferent::Sentinel, true, false, 0);
    }

    #[test]
    fn shared_mutation_preserves_readonly_external_sentinel() {
        assert_shared_mutation_rejected(SharedReferent::Sentinel, false, true, 0);
    }

    #[test]
    fn removing_shared_readonly_target_preserves_external_permissions() {
        // Arrange.
        let (_project, mut worker, candidate) = worker_and_candidate();
        let outside = tempfile::tempdir().unwrap();
        let source = Utf8Path::from_path(outside.path())
            .unwrap()
            .join("sentinel.py");
        fs::write(&source, b"original\n").unwrap();
        let initial_permissions = fs::metadata(&source).unwrap().permissions();
        make_read_only(&source);
        let permissions = permission_fingerprint(&source);
        let target = worker.root().join(&candidate.path);
        fs::remove_file(&target).unwrap();
        fs::hard_link(&source, &target).unwrap();

        // Act.
        let result = worker.remove(&candidate.path);

        // Assert: unlinking a worker entry must not chmod its external referent.
        assert_eq!(permission_fingerprint(&source), permissions);
        assert_eq!(fs::read(&source).unwrap(), b"original\n");
        #[cfg(unix)]
        result.unwrap();
        #[cfg(windows)]
        let _ = result;
        drop(worker);
        assert_eq!(permission_fingerprint(&source), permissions);
        assert_eq!(fs::read(&source).unwrap(), b"original\n");
        fs::set_permissions(&source, initial_permissions).unwrap();
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(16))]

        #[test]
        fn shared_mutation_rejects_additional_aliases(extra_links in 1_u8..=8) {
            assert_shared_mutation_rejected(SharedReferent::Sentinel, false, false, extra_links);
        }
    }

    #[test]
    fn mutation_rejects_location_mismatch_without_changing_worker_bytes() {
        let (_project, mut worker, mut candidate) = worker_and_candidate();
        candidate.column = 1;

        assert!(matches!(
            worker.apply_mutation(&candidate),
            Err(super::WorkspaceError::MutationSpanInvalid { .. })
        ));
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
    }

    #[test]
    fn mutation_rejects_stable_id_mismatch_without_changing_worker_bytes() {
        let (_project, mut worker, mut candidate) = worker_and_candidate();
        candidate.id = "m1_corrupted".to_owned();

        assert!(matches!(
            worker.apply_mutation(&candidate),
            Err(super::WorkspaceError::MutationSpanInvalid { .. })
        ));
        assert_eq!(worker.read("swap/target.py").unwrap(), b"original\n");
    }

    fn replace_paused_parent(
        root: &Utf8Path,
    ) -> (
        Utf8PathBuf,
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
    fn parent_replacement_mutation_uses_the_opened_parent() {
        let (_project, mut worker, candidate) = worker_and_candidate();
        let root = worker.root().to_owned();
        let target = root.join("swap/target.py");
        make_read_only(&target);
        assert!(fs::metadata(&target).unwrap().permissions().readonly());
        let (opened_tx, opened_rx) = sync_channel(0);
        let (resume_tx, resume_rx) = sync_channel(0);
        let hook = Arc::new(MutationPause {
            fired: AtomicBool::new(false),
            opened: opened_tx,
            resume: Mutex::new(resume_rx),
        });
        let thread_hook = Arc::clone(&hook);
        let operation = thread::spawn(move || {
            let _guard = install_workspace_race_hook(thread_hook);
            let result = worker.apply_mutation(&candidate);
            (worker, result)
        });

        opened_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (operated_target, outside_permissions, rename_error) = replace_paused_parent(&root);
        resume_tx.send(()).unwrap();
        let (worker, result) = operation.join().unwrap();

        result.unwrap();
        assert_eq!(fs::read(operated_target.clone()).unwrap(), b"mutated!\n");
        assert!(
            !fs::metadata(operated_target.clone())
                .unwrap()
                .permissions()
                .readonly()
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
}
