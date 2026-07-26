use std::collections::BTreeSet;

use camino::Utf8Path;

use super::manifest::build_manifest;
use super::root::WorkerEntryKind;
use super::{WorkerWorkspace, WorkspaceError};

impl WorkerWorkspace {
    /// Confirms that the original workspace still matches this worker's manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if the source cannot be scanned or its contents changed.
    pub fn verify_originals(&self) -> Result<(), WorkspaceError> {
        let (current, _) = build_manifest(&self.original_root, &self.options)?;
        if self.manifest.content_matches(&current) {
            Ok(())
        } else {
            Err(WorkspaceError::OriginalChanged {
                path: self
                    .manifest
                    .first_content_difference(&current)
                    .unwrap_or_default(),
            })
        }
    }

    /// Restores the worker filesystem from its original snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the original workspace changed or restoration cannot complete.
    pub fn reset(&mut self) -> Result<(), WorkspaceError> {
        self.verify_originals()?;
        self.reset_from_snapshot().map_err(|error| {
            if matches!(error, WorkspaceError::WorkspaceRestore { .. }) {
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
        for entry in existing.iter().rev() {
            match entry.kind {
                WorkerEntryKind::Directory => {
                    if !required_directory(&entry.path, self.snapshot.keys()) {
                        self.root.remove_any_if_exists(&entry.path)?;
                    }
                }
                WorkerEntryKind::File => {
                    if !self.snapshot.contains_key(&entry.path) {
                        self.root.remove_any_if_exists(&entry.path)?;
                    }
                }
                WorkerEntryKind::LinkOrReparse => {
                    self.root.remove_any_if_exists(&entry.path)?;
                }
            }
        }

        for (path, snapshot) in &self.snapshot {
            if self
                .root
                .snapshot_matches(path, &snapshot.bytes, snapshot.permission_fingerprint)?
            {
                continue;
            }
            self.root
                .restore(path, &snapshot.bytes, snapshot.permissions.clone())?;
        }

        let matches = self.matches_snapshot()?;
        hoimin_core::contract_ensure!("workspace.reset.post", matches, self.root.path().as_str(),);
        if matches {
            Ok(())
        } else {
            Err(WorkspaceError::WorkspaceRestore {
                path: self.root.path().to_owned(),
                message: "post-reset manifest comparison failed".to_owned(),
            })
        }
    }

    fn matches_snapshot(&self) -> Result<bool, WorkspaceError> {
        let entries = self.root.entries()?;
        let actual_files = entries
            .iter()
            .filter(|entry| entry.kind != WorkerEntryKind::Directory)
            .map(|entry| &entry.path)
            .collect::<BTreeSet<_>>();
        let expected_files = self.snapshot.keys().collect::<BTreeSet<_>>();
        if actual_files != expected_files {
            return Ok(false);
        }
        for (path, snapshot) in &self.snapshot {
            if !self.root.snapshot_matches(
                path,
                &snapshot.bytes,
                snapshot.permission_fingerprint,
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn required_directory<'a>(
    path: &Utf8Path,
    mut snapshot_paths: impl Iterator<Item = &'a camino::Utf8PathBuf>,
) -> bool {
    snapshot_paths.any(|file| file.starts_with(path) && file != path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use camino::Utf8Path;
    use hoimin_core::{BudgetLedger, EffectId, RunBudgets, reserve_workspace_copy};

    use super::super::root::{WorkspaceRaceHook, install_workspace_race_hook};
    use super::super::{CopyOptions, WorkerWorkspace, WorkspaceError, WorkspacePlan};

    struct ResetPause {
        fired: AtomicBool,
        opened: Barrier,
        resume: Barrier,
    }

    impl WorkspaceRaceHook for ResetPause {
        fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
            if operation == "reset"
                && path == Utf8Path::new("swap/target.py")
                && !self.fired.swap(true, Ordering::SeqCst)
            {
                self.opened.wait();
                self.resume.wait();
            }
        }
    }

    fn changed_worker() -> (tempfile::TempDir, WorkerWorkspace) {
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
        let mut worker = plan
            .create_worker(&reservation.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        worker
            .write("swap/target.py", b"changed contents\n")
            .unwrap();
        (project, worker)
    }

    #[test]
    fn parent_replacement_reset_restore_uses_the_opened_parent() {
        let (_project, worker) = changed_worker();
        let root = worker.root().to_owned();
        let permissions = fs::metadata(root.join("swap/target.py"))
            .unwrap()
            .permissions();
        let hook = Arc::new(ResetPause {
            fired: AtomicBool::new(false),
            opened: Barrier::new(2),
            resume: Barrier::new(2),
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

        hook.opened.wait();
        fs::rename(root.join("swap"), root.join("held")).unwrap();
        fs::create_dir(root.join("swap")).unwrap();
        let outside = root.join("swap/target.py");
        fs::write(&outside, b"outside\n").unwrap();
        let outside_readonly = fs::metadata(&outside).unwrap().permissions().readonly();
        hook.resume.wait();
        let (worker, result) = operation.join().unwrap();

        assert!(
            result.is_ok() || matches!(result, Err(WorkspaceError::WorkspaceRestore { .. })),
            "{result:?}"
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside\n");
        assert_eq!(
            fs::metadata(&outside).unwrap().permissions().readonly(),
            outside_readonly
        );
        drop(worker);
    }
}
