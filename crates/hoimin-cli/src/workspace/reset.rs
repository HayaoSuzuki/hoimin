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
