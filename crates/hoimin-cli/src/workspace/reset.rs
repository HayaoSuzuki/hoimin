use std::collections::BTreeSet;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use ignore::WalkBuilder;

use super::manifest::{build_manifest, relative_utf8};
use super::{WorkerWorkspace, WorkspaceError, permission_fingerprint};

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
                    path: self.root.clone(),
                    message: error.to_string(),
                }
            }
        })
    }

    fn reset_from_snapshot(&self) -> Result<(), WorkspaceError> {
        let existing = collect_worker_entries(&self.root)?;
        for (path, is_dir) in existing.iter().rev() {
            if *is_dir {
                if !required_directory(path, &self.manifest) {
                    remove_directory_if_empty(&self.root.join(path))?;
                }
                continue;
            }
            if self.manifest.entry(path).is_none() {
                remove_any(&self.root.join(path))?;
            }
        }

        for (path, snapshot) in &self.snapshot {
            let destination = self.root.join(path);
            let unchanged = fs::symlink_metadata(&destination)
                .ok()
                .filter(|metadata| metadata.file_type().is_file())
                .filter(|metadata| {
                    permission_fingerprint(&metadata.permissions())
                        == snapshot.permission_fingerprint
                })
                .and_then(|_| fs::read(&destination).ok())
                .is_some_and(|bytes| bytes == snapshot.bytes);
            if unchanged {
                continue;
            }
            if fs::symlink_metadata(&destination).is_ok() {
                remove_any(&destination)?;
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| WorkspaceError::io("restore directory", parent, error))?;
            }
            fs::write(&destination, &snapshot.bytes)
                .map_err(|error| WorkspaceError::io("restore worker file", path, error))?;
            fs::set_permissions(&destination, snapshot.permissions.clone())
                .map_err(|error| WorkspaceError::io("restore worker permissions", path, error))?;
        }

        let matches = self.matches_snapshot()?;
        hoimin_core::contract_ensure!("workspace.reset.post", matches, self.root.as_str(),);
        if matches {
            Ok(())
        } else {
            Err(WorkspaceError::WorkspaceRestore {
                path: self.root.clone(),
                message: "post-reset manifest comparison failed".to_owned(),
            })
        }
    }

    fn matches_snapshot(&self) -> Result<bool, WorkspaceError> {
        let entries = collect_worker_entries(&self.root)?;
        let actual_files = entries
            .iter()
            .filter(|(_, is_dir)| !*is_dir)
            .map(|(path, _)| path)
            .collect::<BTreeSet<_>>();
        let expected_files = self.snapshot.keys().collect::<BTreeSet<_>>();
        if actual_files != expected_files {
            return Ok(false);
        }
        for (path, snapshot) in &self.snapshot {
            let destination = self.root.join(path);
            let metadata = fs::symlink_metadata(&destination)
                .map_err(|error| WorkspaceError::io("verify restored file", path, error))?;
            if !metadata.file_type().is_file() {
                return Ok(false);
            }
            let bytes = fs::read(&destination)
                .map_err(|error| WorkspaceError::io("verify restored file", path, error))?;
            if bytes != snapshot.bytes {
                return Ok(false);
            }
            if permission_fingerprint(&metadata.permissions()) != snapshot.permission_fingerprint {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn collect_worker_entries(root: &Utf8Path) -> Result<Vec<(Utf8PathBuf, bool)>, WorkspaceError> {
    let mut builder = WalkBuilder::new(root.as_std_path());
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .follow_links(false);
    let mut entries = Vec::new();
    for result in builder.build() {
        let entry = result.map_err(|error| WorkspaceError::Walk(error.to_string()))?;
        if entry.path() == root.as_std_path() {
            continue;
        }
        let path = relative_utf8(root, entry.path())?;
        let is_dir = entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir());
        entries.push((path, is_dir));
    }
    entries.sort_by(|(left, _), (right, _)| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    Ok(entries)
}

fn required_directory(path: &Utf8Path, manifest: &super::WorkspaceManifest) -> bool {
    manifest
        .entries()
        .iter()
        .any(|entry| entry.path.starts_with(path) && entry.path != path)
}

fn remove_directory_if_empty(path: &Utf8Path) -> Result<(), WorkspaceError> {
    if !path.exists() {
        return Ok(());
    }
    make_writable(path)?;
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(error) => Err(WorkspaceError::io("remove worker directory", path, error)),
    }
}

fn remove_any(path: &Utf8Path) -> Result<(), WorkspaceError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| WorkspaceError::io("inspect worker path", path, error))?;
    if !metadata.file_type().is_symlink() {
        make_writable(path)?;
    }
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
            .map_err(|error| WorkspaceError::io("remove worker directory", path, error))
    } else {
        fs::remove_file(path).map_err(|error| WorkspaceError::io("remove worker file", path, error))
    }
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
pub fn make_writable(path: &Utf8Path) -> Result<(), WorkspaceError> {
    let mut permissions = fs::symlink_metadata(path)
        .map_err(|error| WorkspaceError::io("read permissions", path, error))?
        .permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)
            .map_err(|error| WorkspaceError::io("make path writable", path, error))?;
    }
    Ok(())
}

#[cfg(unix)]
pub fn make_writable(path: &Utf8Path) -> Result<(), WorkspaceError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::symlink_metadata(path)
        .map_err(|error| WorkspaceError::io("read permissions", path, error))?
        .permissions();
    let mode = permissions.mode();
    if mode & 0o200 == 0 {
        permissions.set_mode(mode | 0o200);
        fs::set_permissions(path, permissions)
            .map_err(|error| WorkspaceError::io("make path writable", path, error))?;
    }
    Ok(())
}
