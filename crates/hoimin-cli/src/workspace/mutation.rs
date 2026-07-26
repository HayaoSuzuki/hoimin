use std::io::{Seek, SeekFrom, Write};

use hoimin_core::MutationCandidate;

use super::{WorkerWorkspace, WorkspaceError};

impl WorkerWorkspace {
    /// Applies a candidate only when the original workspace and target bytes still match.
    ///
    /// # Errors
    ///
    /// Returns an error when the original workspace changed, the target is invalid, or the
    /// mutated file cannot be written.
    pub fn apply_mutation(&mut self, candidate: &MutationCandidate) -> Result<(), WorkspaceError> {
        self.verify_originals()?;
        let expected = self.manifest.entry(&candidate.path).ok_or_else(|| {
            WorkspaceError::MutationTargetMissing {
                path: candidate.path.clone(),
            }
        })?;
        let mut target = self.root.open_mutation_file(&candidate.path)?;
        let mut bytes = Vec::new();
        target.read_to_end(&mut bytes, &candidate.path)?;
        let actual_hash = blake3::hash(&bytes);
        if candidate.file_hash != expected.blake3.to_hex().as_str()
            || actual_hash != expected.blake3
        {
            return Err(WorkspaceError::MutationHashMismatch {
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
        if bytes.get(start..end) != Some(candidate.original.as_bytes()) {
            return Err(WorkspaceError::MutationOriginalMismatch {
                path: candidate.path.clone(),
            });
        }

        let mut mutated = Vec::with_capacity(bytes.len() - length + candidate.replacement.len());
        mutated.extend_from_slice(&bytes[..start]);
        mutated.extend_from_slice(candidate.replacement.as_bytes());
        mutated.extend_from_slice(&bytes[end..]);
        let mut file = target.into_writable(&candidate.path)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;
        file.set_len(0)
            .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;
        file.write_all(&mutated)
            .and_then(|()| file.flush())
            .map_err(|error| WorkspaceError::io("write mutation target", &candidate.path, error))?;

        hoimin_core::contract_ensure!(
            "workspace.mutation.post",
            mutated[..start] == bytes[..start]
                && mutated[start + candidate.replacement.len()..] == bytes[end..],
            &candidate.path,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use camino::{Utf8Path, Utf8PathBuf};
    use hoimin_core::{
        BudgetLedger, ByteSpan, EffectId, MutationCandidate, RunBudgets, reserve_workspace_copy,
    };

    use super::super::root::{WorkspaceRaceHook, install_workspace_race_hook};
    use super::super::{CopyOptions, WorkerWorkspace, WorkspacePlan};

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
        opened: Barrier,
        resume: Barrier,
    }

    impl WorkspaceRaceHook for MutationPause {
        fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
            if operation == "mutation"
                && path == Utf8Path::new("swap/target.py")
                && !self.fired.swap(true, Ordering::SeqCst)
            {
                self.opened.wait();
                self.resume.wait();
            }
        }
    }

    fn worker_and_candidate() -> (tempfile::TempDir, WorkerWorkspace, MutationCandidate) {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("swap")).unwrap();
        fs::write(project.path().join("swap/target.py"), b"original\n").unwrap();
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
        let candidate = MutationCandidate {
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
        (project, worker, candidate)
    }

    #[test]
    fn parent_replacement_mutation_uses_the_opened_parent() {
        let (_project, mut worker, candidate) = worker_and_candidate();
        let root = worker.root().to_owned();
        let target = root.join("swap/target.py");
        make_read_only(&target);
        assert!(fs::metadata(&target).unwrap().permissions().readonly());
        let hook = Arc::new(MutationPause {
            fired: AtomicBool::new(false),
            opened: Barrier::new(2),
            resume: Barrier::new(2),
        });
        let thread_hook = Arc::clone(&hook);
        let operation = thread::spawn(move || {
            let _guard = install_workspace_race_hook(thread_hook);
            let result = worker.apply_mutation(&candidate);
            (worker, result)
        });

        hook.opened.wait();
        fs::rename(root.join("swap"), root.join("held")).unwrap();
        fs::create_dir(root.join("swap")).unwrap();
        let outside = root.join("swap/target.py");
        fs::write(&outside, b"outside\n").unwrap();
        make_read_only(&outside);
        let outside_permissions = permission_fingerprint(&outside);
        hook.resume.wait();
        let (worker, result) = operation.join().unwrap();

        result.unwrap();
        assert_eq!(
            fs::read(root.join("held/target.py")).unwrap(),
            b"mutated!\n"
        );
        assert!(
            !fs::metadata(root.join("held/target.py"))
                .unwrap()
                .permissions()
                .readonly()
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside\n");
        assert_eq!(permission_fingerprint(&outside), outside_permissions);
        drop(worker);
    }
}
