use std::ffi::OsString;
use std::fs::File;
#[cfg(unix)]
use std::io::Write;
use std::io::{self, Read};
use std::path::Path;
#[cfg(test)]
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
#[cfg(unix)]
use cap_fs_ext::OpenOptionsFollowExt;
use cap_primitives::fs::FollowSymlinks;

use super::WorkspaceError;

#[cfg(test)]
pub(super) trait WorkspaceRaceHook: Send + Sync {
    fn parent_opened(&self, operation: &'static str, path: &Utf8Path);
}

#[cfg(test)]
thread_local! {
    static WORKSPACE_RACE_HOOK: std::cell::RefCell<Option<Arc<dyn WorkspaceRaceHook>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct WorkspaceRaceHookGuard;

#[cfg(test)]
impl Drop for WorkspaceRaceHookGuard {
    fn drop(&mut self) {
        WORKSPACE_RACE_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(test)]
pub(super) fn install_workspace_race_hook(
    hook: Arc<dyn WorkspaceRaceHook>,
) -> WorkspaceRaceHookGuard {
    WORKSPACE_RACE_HOOK.with(|slot| {
        assert!(slot.borrow_mut().replace(hook).is_none());
    });
    WorkspaceRaceHookGuard
}

#[cfg(test)]
fn parent_opened(operation: &'static str, path: &Utf8Path) {
    WORKSPACE_RACE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref() {
            hook.parent_opened(operation, path);
        }
    });
}

#[cfg(windows)]
mod windows;

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowsCreateDisposition {
    Open,
    OpenIf,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowsFinalOperation {
    InspectForWrite,
    InspectMutation,
    WriteMutation,
    Read,
    Write,
    Remove,
    RemoveEntry,
}

#[cfg(any(windows, test))]
impl WindowsFinalOperation {
    const fn create_disposition(self) -> WindowsCreateDisposition {
        match self {
            Self::InspectForWrite
            | Self::InspectMutation
            | Self::WriteMutation
            | Self::Read
            | Self::Remove
            | Self::RemoveEntry => WindowsCreateDisposition::Open,
            Self::Write => WindowsCreateDisposition::OpenIf,
        }
    }

    #[cfg(test)]
    const fn needs_delete_access(self) -> bool {
        matches!(self, Self::Remove | Self::RemoveEntry)
    }

    #[cfg(test)]
    const fn accepts_directory_or_reparse(self) -> bool {
        matches!(self, Self::RemoveEntry)
    }
}

#[cfg(any(windows, test))]
fn windows_final_name_is_valid(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

#[derive(Debug)]
pub(crate) struct WorkerRoot {
    path: Utf8PathBuf,
    handle: File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerEntryKind {
    File,
    Directory,
    LinkOrReparse,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct WorkerEntry {
    pub(crate) path: Utf8PathBuf,
    pub(crate) kind: WorkerEntryKind,
}

pub(crate) struct MutationFile {
    file: File,
    reopen: Option<(File, OsString)>,
}

impl MutationFile {
    pub(crate) fn read_to_end(
        &mut self,
        contents: &mut Vec<u8>,
        path: &Utf8Path,
    ) -> Result<(), WorkspaceError> {
        self.file
            .read_to_end(contents)
            .map(|_| ())
            .map_err(|error| WorkspaceError::io("read mutation target", path, error))
    }

    pub(crate) fn into_writable(mut self, path: &Utf8Path) -> Result<File, WorkspaceError> {
        make_file_writable(&self.file, path)?;
        #[cfg(windows)]
        if let Some((parent, name)) = self.reopen {
            self.file = windows::reopen_mutation_file(&self.file, &parent, &name, path)?;
        }
        #[cfg(unix)]
        if let Some((parent, name)) = self.reopen {
            use std::os::unix::fs::MetadataExt;

            let inspected_metadata = self
                .file
                .metadata()
                .map_err(|error| WorkspaceError::io("inspect mutation target", path, error))?;
            let mut options = cap_primitives::fs::OpenOptions::new();
            options.read(true).write(true).follow(FollowSymlinks::No);
            let file =
                cap_primitives::fs::open(&parent, Path::new(&name), &options).map_err(|error| {
                    WorkerRoot::map_entry_error("open mutation target", path, error)
                })?;
            let metadata = file
                .metadata()
                .map_err(|error| WorkspaceError::io("inspect mutation target", path, error))?;
            if !metadata.is_file()
                || metadata.dev() != inspected_metadata.dev()
                || metadata.ino() != inspected_metadata.ino()
            {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            self.file = file;
        }
        Ok(self.file)
    }
}

#[allow(dead_code)]
impl WorkerRoot {
    pub(crate) fn open(path: Utf8PathBuf) -> Result<Self, WorkspaceError> {
        let handle = cap_primitives::fs::open_ambient_dir(
            path.as_std_path(),
            cap_primitives::ambient_authority(),
        )
        .map_err(|error| WorkspaceError::io("open worker root", &path, error))?;
        Ok(Self { path, handle })
    }

    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub(crate) fn read(&self, path: &Utf8Path) -> Result<Vec<u8>, WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        #[cfg(test)]
        parent_opened("read", path);
        #[cfg(windows)]
        {
            return windows::read(&parent, &name, path);
        }
        #[cfg(unix)]
        {
            Self::reject_link(&parent, &name, path, "read worker file")?;
            let mut options = cap_primitives::fs::OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let mut file = cap_primitives::fs::open(&parent, Path::new(&name), &options)
                .map_err(|error| Self::map_entry_error("read worker file", path, error))?;
            let metadata = file
                .metadata()
                .map_err(|error| WorkspaceError::io("inspect worker file", path, error))?;
            if !metadata.is_file() {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .map_err(|error| WorkspaceError::io("read worker file", path, error))?;
            Ok(contents)
        }
    }

    pub(crate) fn open_mutation_file(
        &self,
        path: &Utf8Path,
    ) -> Result<MutationFile, WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        #[cfg(test)]
        parent_opened("mutation", path);
        #[cfg(windows)]
        {
            return windows::open_mutation_file(&parent, &name, path).map(|file| MutationFile {
                file,
                reopen: Some((parent, name)),
            });
        }
        #[cfg(unix)]
        {
            Self::reject_link(&parent, &name, path, "open mutation target")?;
            let mut options = cap_primitives::fs::OpenOptions::new();
            options.read(true).write(true).follow(FollowSymlinks::No);
            let (file, reopen) = match cap_primitives::fs::open(&parent, Path::new(&name), &options)
            {
                Ok(file) => (file, None),
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    let mut inspect_options = cap_primitives::fs::OpenOptions::new();
                    inspect_options.read(true).follow(FollowSymlinks::No);
                    let file =
                        cap_primitives::fs::open(&parent, Path::new(&name), &inspect_options)
                            .map_err(|error| {
                                Self::map_entry_error("open mutation target", path, error)
                            })?;
                    (file, Some((parent, name)))
                }
                Err(error) => {
                    return Err(Self::map_entry_error("open mutation target", path, error));
                }
            };
            let metadata = file
                .metadata()
                .map_err(|error| WorkspaceError::io("inspect mutation target", path, error))?;
            if !metadata.is_file() {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            Ok(MutationFile { file, reopen })
        }
    }

    pub(crate) fn write(&self, path: &Utf8Path, contents: &[u8]) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, true)?;
        #[cfg(test)]
        parent_opened("write", path);
        Self::write_entry(&parent, &name, path, contents)
    }

    fn write_entry(
        parent: &File,
        name: &OsString,
        path: &Utf8Path,
        contents: &[u8],
    ) -> Result<(), WorkspaceError> {
        #[cfg(windows)]
        {
            make_directory_writable(parent, path)?;
            return windows::write(parent, name, path, contents);
        }
        #[cfg(unix)]
        {
            Self::reject_link_if_present(parent, name, path, "write worker file")?;
            make_directory_writable(parent, path)?;
            if cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No).is_ok() {
                let mut inspect_options = cap_primitives::fs::OpenOptions::new();
                inspect_options.read(true).follow(FollowSymlinks::No);
                let file = cap_primitives::fs::open(parent, Path::new(name), &inspect_options)
                    .map_err(|error| Self::map_entry_error("open worker file", path, error))?;
                make_file_writable(&file, path)?;
            }
            let mut options = cap_primitives::fs::OpenOptions::new();
            options
                .write(true)
                .create(true)
                .truncate(true)
                .follow(FollowSymlinks::No);
            let mut file = cap_primitives::fs::open(parent, Path::new(name), &options)
                .map_err(|error| Self::map_entry_error("write worker file", path, error))?;
            make_file_writable(&file, path)?;
            file.write_all(contents)
                .map_err(|error| WorkspaceError::io("write worker file", path, error))
        }
    }

    pub(crate) fn remove_file(&self, path: &Utf8Path) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        #[cfg(test)]
        parent_opened("remove", path);
        #[cfg(windows)]
        {
            make_directory_writable(&parent, path)?;
            return windows::remove_file(&parent, &name, path);
        }
        #[cfg(unix)]
        {
            Self::reject_link(&parent, &name, path, "remove worker file")?;
            make_directory_writable(&parent, path)?;
            let mut options = cap_primitives::fs::OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let file = cap_primitives::fs::open(&parent, Path::new(&name), &options)
                .map_err(|error| Self::map_entry_error("remove worker file", path, error))?;
            make_file_writable(&file, path)?;
            drop(file);
            cap_primitives::fs::remove_file(&parent, Path::new(&name))
                .map_err(|error| Self::map_entry_error("remove worker file", path, error))
        }
    }

    pub(crate) fn try_exists(&self, path: &Utf8Path) -> Result<bool, WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        match cap_primitives::fs::stat(&parent, Path::new(&name), FollowSymlinks::No) {
            Ok(metadata) if is_link_or_reparse(&metadata) => Err(WorkspaceError::InvalidPath {
                path: path.to_owned(),
            }),
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(Self::map_entry_error("inspect worker file", path, error)),
        }
    }

    pub(crate) fn entries(&self) -> Result<Vec<WorkerEntry>, WorkspaceError> {
        let mut entries = Vec::new();
        Self::collect_entries(&self.handle, Utf8Path::new(""), &mut entries)?;
        entries.sort_by(|left, right| {
            left.path
                .components()
                .count()
                .cmp(&right.path.components().count())
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(entries)
    }

    fn collect_entries(
        directory: &File,
        prefix: &Utf8Path,
        entries: &mut Vec<WorkerEntry>,
    ) -> Result<(), WorkspaceError> {
        let read_dir = cap_primitives::fs::read_base_dir(directory)
            .map_err(|error| WorkspaceError::io("enumerate worker directory", prefix, error))?;
        for entry in read_dir {
            let entry = entry
                .map_err(|error| WorkspaceError::io("enumerate worker directory", prefix, error))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| WorkspaceError::NonUtf8Path)?;
            let path = prefix.join(&name);
            let metadata =
                cap_primitives::fs::stat(directory, Path::new(&name), FollowSymlinks::No)
                    .map_err(|error| Self::map_entry_error("inspect worker entry", &path, error))?;
            let kind = if is_link_or_reparse(&metadata) {
                WorkerEntryKind::LinkOrReparse
            } else if metadata.is_dir() {
                WorkerEntryKind::Directory
            } else if metadata.is_file() {
                WorkerEntryKind::File
            } else {
                WorkerEntryKind::LinkOrReparse
            };
            entries.push(WorkerEntry {
                path: path.clone(),
                kind,
            });
            if kind == WorkerEntryKind::Directory {
                let child = cap_primitives::fs::open_dir_nofollow(directory, Path::new(&name))
                    .map_err(|error| {
                        Self::map_entry_error("open worker directory", &path, error)
                    })?;
                Self::collect_entries(&child, &path, entries)?;
            }
        }
        Ok(())
    }

    pub(crate) fn remove_any_if_exists(&self, path: &Utf8Path) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        #[cfg(test)]
        parent_opened("reset", path);
        Self::remove_entry_if_exists(&parent, &name, path)
    }

    fn remove_entry_if_exists(
        parent: &File,
        name: &OsString,
        logical_path: &Utf8Path,
    ) -> Result<(), WorkspaceError> {
        let metadata = match cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(Self::map_entry_error(
                    "inspect worker entry",
                    logical_path,
                    error,
                ));
            }
        };
        if is_link_or_reparse(&metadata) {
            return remove_link_or_reparse(parent, name, logical_path, &metadata);
        }
        if metadata.is_dir() {
            let directory = cap_primitives::fs::open_dir_nofollow(parent, Path::new(name))
                .map_err(|error| {
                    Self::map_entry_error("open worker directory", logical_path, error)
                })?;
            let read_dir = cap_primitives::fs::read_base_dir(&directory).map_err(|error| {
                WorkspaceError::io("enumerate worker directory", logical_path, error)
            })?;
            for child in read_dir {
                let child = child.map_err(|error| {
                    WorkspaceError::io("enumerate worker directory", logical_path, error)
                })?;
                let child_name = child.file_name();
                let child_utf8 = child_name.to_str().ok_or(WorkspaceError::NonUtf8Path)?;
                let child_path = logical_path.join(child_utf8);
                Self::remove_entry_if_exists(&directory, &child_name, &child_path)?;
            }
            make_directory_writable(&directory, logical_path)?;
            drop(directory);
            #[cfg(windows)]
            {
                return windows::remove_entry(parent, name, logical_path);
            }
            #[cfg(unix)]
            cap_primitives::fs::remove_dir(parent, Path::new(name)).map_err(|error| {
                Self::map_entry_error("remove worker directory", logical_path, error)
            })
        } else {
            #[cfg(windows)]
            {
                return windows::remove_file(parent, name, logical_path);
            }
            #[cfg(unix)]
            {
                let mut options = cap_primitives::fs::OpenOptions::new();
                options.read(true).follow(FollowSymlinks::No);
                let file = cap_primitives::fs::open(parent, Path::new(name), &options).map_err(
                    |error| Self::map_entry_error("open worker file", logical_path, error),
                )?;
                make_file_writable(&file, logical_path)?;
                drop(file);
                cap_primitives::fs::remove_file(parent, Path::new(name)).map_err(|error| {
                    Self::map_entry_error("remove worker file", logical_path, error)
                })
            }
        }
    }

    pub(crate) fn restore(
        &self,
        path: &Utf8Path,
        contents: &[u8],
        permissions: std::fs::Permissions,
    ) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, true)?;
        #[cfg(test)]
        parent_opened("reset", path);
        Self::remove_entry_if_exists(&parent, &name, path)?;
        Self::write_entry(&parent, &name, path, contents)?;
        #[cfg(windows)]
        {
            return windows::set_permissions(&parent, &name, path, permissions);
        }
        #[cfg(unix)]
        {
            let mut options = cap_primitives::fs::OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let file = cap_primitives::fs::open(&parent, Path::new(&name), &options)
                .map_err(|error| Self::map_entry_error("open restored file", path, error))?;
            file.set_permissions(permissions)
                .map_err(|error| WorkspaceError::io("restore worker permissions", path, error))
        }
    }

    pub(crate) fn snapshot_matches(
        &self,
        path: &Utf8Path,
        expected: &[u8],
        expected_permissions: super::PermissionFingerprint,
    ) -> Result<bool, WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        let metadata = match cap_primitives::fs::stat(&parent, Path::new(&name), FollowSymlinks::No)
        {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(Self::map_entry_error("verify restored file", path, error));
            }
        };
        if is_link_or_reparse(&metadata) || !metadata.is_file() {
            return Ok(false);
        }
        #[cfg(windows)]
        {
            let (bytes, permissions) = windows::snapshot(&parent, &name, path)?;
            Ok(bytes == expected
                && super::permission_fingerprint(&permissions) == expected_permissions)
        }
        #[cfg(unix)]
        {
            let mut options = cap_primitives::fs::OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let mut file = cap_primitives::fs::open(&parent, Path::new(&name), &options)
                .map_err(|error| Self::map_entry_error("verify restored file", path, error))?;
            let file_metadata = file
                .metadata()
                .map_err(|error| WorkspaceError::io("verify restored file", path, error))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|error| WorkspaceError::io("verify restored file", path, error))?;
            Ok(bytes == expected
                && super::permission_fingerprint(&file_metadata.permissions())
                    == expected_permissions)
        }
    }

    fn components(path: &Utf8Path) -> Result<Vec<&str>, WorkspaceError> {
        if !hoimin_core::normalized_relative_path(path.as_str()) {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_owned(),
            });
        }
        Ok(path.components().map(|part| part.as_str()).collect())
    }

    pub(crate) fn open_parent(
        &self,
        path: &Utf8Path,
        create: bool,
    ) -> Result<(File, OsString), WorkspaceError> {
        let components = Self::components(path)?;
        let (name, parents) = components.split_last().expect("validated nonempty path");
        let mut parent = self
            .handle
            .try_clone()
            .map_err(|error| WorkspaceError::io("clone worker root", path, error))?;

        for component in parents {
            let component_path = Path::new(component);
            if cap_primitives::fs::stat(
                &parent,
                component_path,
                cap_primitives::fs::FollowSymlinks::No,
            )
            .is_ok_and(|metadata| is_link_or_reparse(&metadata))
            {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            let opened = match cap_primitives::fs::open_dir_nofollow(&parent, component_path) {
                Ok(opened) => opened,
                Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                    make_directory_writable(&parent, path)?;
                    cap_primitives::fs::create_dir(
                        &parent,
                        component_path,
                        &cap_primitives::fs::DirOptions::new(),
                    )
                    .map_err(|error| Self::map_parent_error(path, error))?;
                    cap_primitives::fs::open_dir_nofollow(&parent, component_path)
                        .map_err(|error| Self::map_parent_error(path, error))?
                }
                Err(error) => return Err(Self::map_parent_error(path, error)),
            };
            parent = opened;
        }

        Ok((parent, OsString::from(name)))
    }

    fn map_parent_error(logical_path: &Utf8Path, error: io::Error) -> WorkspaceError {
        if is_nofollow_link_error(&error) {
            WorkspaceError::InvalidPath {
                path: logical_path.to_owned(),
            }
        } else {
            WorkspaceError::io("open worker parent", logical_path, error)
        }
    }

    fn reject_link(
        parent: &File,
        name: &OsString,
        logical_path: &Utf8Path,
        operation: &'static str,
    ) -> Result<(), WorkspaceError> {
        match cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No) {
            Ok(metadata) if is_link_or_reparse(&metadata) => Err(WorkspaceError::InvalidPath {
                path: logical_path.to_owned(),
            }),
            Ok(_) => Ok(()),
            Err(error) => Err(Self::map_entry_error(operation, logical_path, error)),
        }
    }

    fn reject_link_if_present(
        parent: &File,
        name: &OsString,
        logical_path: &Utf8Path,
        operation: &'static str,
    ) -> Result<(), WorkspaceError> {
        match cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No) {
            Ok(metadata) if is_link_or_reparse(&metadata) => Err(WorkspaceError::InvalidPath {
                path: logical_path.to_owned(),
            }),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Self::map_entry_error(operation, logical_path, error)),
        }
    }

    fn map_entry_error(
        operation: &'static str,
        logical_path: &Utf8Path,
        error: io::Error,
    ) -> WorkspaceError {
        if is_nofollow_link_error(&error) {
            WorkspaceError::InvalidPath {
                path: logical_path.to_owned(),
            }
        } else {
            WorkspaceError::io(operation, logical_path, error)
        }
    }
}

#[cfg(unix)]
fn make_file_writable(file: &File, path: &Utf8Path) -> Result<(), WorkspaceError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect worker file", path, error))?;
    let mut permissions = metadata.permissions();
    if permissions.mode() & 0o200 == 0 {
        permissions.set_mode(permissions.mode() | 0o200);
        file.set_permissions(permissions)
            .map_err(|error| WorkspaceError::io("prepare worker file", path, error))?;
    }
    Ok(())
}

#[cfg(windows)]
fn make_file_writable(file: &File, path: &Utf8Path) -> Result<(), WorkspaceError> {
    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect worker file", path, error))?;
    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        file.set_permissions(permissions)
            .map_err(|error| WorkspaceError::io("prepare worker file", path, error))?;
    }
    Ok(())
}

fn make_directory_writable(directory: &File, path: &Utf8Path) -> Result<(), WorkspaceError> {
    let metadata = directory
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect worker directory", path, error))?;
    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if permissions.mode() & 0o700 != 0o700 {
            permissions.set_mode(permissions.mode() | 0o700);
            directory
                .set_permissions(permissions)
                .map_err(|error| WorkspaceError::io("prepare worker directory", path, error))?;
        }
    }
    #[cfg(windows)]
    if permissions.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        directory
            .set_permissions(permissions)
            .map_err(|error| WorkspaceError::io("prepare worker directory", path, error))?;
    }
    Ok(())
}

fn remove_link_or_reparse(
    parent: &File,
    name: &OsString,
    path: &Utf8Path,
    metadata: &cap_primitives::fs::Metadata,
) -> Result<(), WorkspaceError> {
    #[cfg(windows)]
    {
        let _ = metadata;
        return windows::remove_entry(parent, name, path);
    }
    #[cfg(unix)]
    let is_directory = metadata.is_dir();
    #[cfg(unix)]
    let result = if is_directory {
        cap_primitives::fs::remove_dir(parent, Path::new(name))
    } else {
        cap_primitives::fs::remove_file(parent, Path::new(name))
    };
    #[cfg(unix)]
    result.map_err(|error| WorkerRoot::map_entry_error("remove worker link", path, error))
}

#[cfg(unix)]
fn is_nofollow_link_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(windows)]
fn is_nofollow_link_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_STOPPED_ON_SYMLINK as i32)
}

#[cfg(unix)]
fn is_link_or_reparse(metadata: &cap_primitives::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn is_link_or_reparse(metadata: &cap_primitives::fs::Metadata) -> bool {
    use cap_primitives::fs::MetadataExt;

    metadata.file_type().is_symlink()
        || metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use camino::{Utf8Path, Utf8PathBuf};

    use super::*;

    struct PausedParent {
        operation: &'static str,
        path: Utf8PathBuf,
        fired: AtomicBool,
        opened: SyncSender<()>,
        resume: Mutex<Receiver<()>>,
    }

    enum ParentReplacement {
        Replaced(TestPermissionFingerprint),
        #[cfg(windows)]
        Denied,
    }

    impl ParentReplacement {
        fn permissions(self) -> TestPermissionFingerprint {
            #[cfg(windows)]
            match self {
                Self::Replaced(permissions) => permissions,
                Self::Denied => unreachable!(),
            }
            #[cfg(not(windows))]
            {
                let Self::Replaced(permissions) = self;
                permissions
            }
        }
    }

    impl PausedParent {
        fn new(operation: &'static str, path: &str) -> (Arc<Self>, Receiver<()>, SyncSender<()>) {
            let (opened_tx, opened_rx) = sync_channel(0);
            let (resume_tx, resume_rx) = sync_channel(0);
            (
                Arc::new(Self {
                    operation,
                    path: path.into(),
                    fired: AtomicBool::new(false),
                    opened: opened_tx,
                    resume: Mutex::new(resume_rx),
                }),
                opened_rx,
                resume_tx,
            )
        }

        fn replace_parent(
            worker: &Utf8Path,
            opened: &Receiver<()>,
            resume: &SyncSender<()>,
        ) -> ParentReplacement {
            opened.recv_timeout(Duration::from_secs(5)).unwrap();
            if let Err(error) = fs::rename(worker.join("swap"), worker.join("held")) {
                #[cfg(windows)]
                {
                    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
                    resume.send(()).unwrap();
                    return ParentReplacement::Denied;
                }
                #[cfg(not(windows))]
                panic!("rename of opened parent failed unexpectedly: {error}");
            }
            fs::create_dir(worker.join("swap")).unwrap();
            let outside = worker.join("swap/target");
            fs::write(&outside, b"outside").unwrap();
            make_read_only(&outside);
            let permissions = permission_fingerprint(&outside);
            resume.send(()).unwrap();
            ParentReplacement::Replaced(permissions)
        }
    }

    impl WorkspaceRaceHook for PausedParent {
        fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
            if operation == self.operation
                && path == self.path
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

    #[cfg(unix)]
    type TestPermissionFingerprint = u32;
    #[cfg(windows)]
    type TestPermissionFingerprint = bool;

    fn make_read_only(path: &Utf8Path) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    fn permission_fingerprint(path: &Utf8Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode()
    }

    #[cfg(windows)]
    fn permission_fingerprint(path: &Utf8Path) -> bool {
        fs::metadata(path).unwrap().permissions().readonly()
    }

    #[test]
    fn windows_final_operations_open_without_destructive_dispositions() {
        assert_eq!(
            WindowsFinalOperation::InspectForWrite.create_disposition(),
            WindowsCreateDisposition::Open
        );
        assert_eq!(
            WindowsFinalOperation::InspectMutation.create_disposition(),
            WindowsCreateDisposition::Open
        );
        assert_eq!(
            WindowsFinalOperation::WriteMutation.create_disposition(),
            WindowsCreateDisposition::Open
        );
        assert_eq!(
            WindowsFinalOperation::Read.create_disposition(),
            WindowsCreateDisposition::Open
        );
        assert_eq!(
            WindowsFinalOperation::Write.create_disposition(),
            WindowsCreateDisposition::OpenIf
        );
        assert_eq!(
            WindowsFinalOperation::Remove.create_disposition(),
            WindowsCreateDisposition::Open
        );
        assert_eq!(
            WindowsFinalOperation::RemoveEntry.create_disposition(),
            WindowsCreateDisposition::Open
        );
        assert!(!WindowsFinalOperation::Read.needs_delete_access());
        assert!(!WindowsFinalOperation::InspectForWrite.needs_delete_access());
        assert!(!WindowsFinalOperation::InspectMutation.needs_delete_access());
        assert!(!WindowsFinalOperation::WriteMutation.needs_delete_access());
        assert!(!WindowsFinalOperation::Write.needs_delete_access());
        assert!(WindowsFinalOperation::Remove.needs_delete_access());
        assert!(WindowsFinalOperation::RemoveEntry.needs_delete_access());
        assert!(!WindowsFinalOperation::Remove.accepts_directory_or_reparse());
        assert!(WindowsFinalOperation::RemoveEntry.accepts_directory_or_reparse());
    }

    #[test]
    fn windows_final_names_are_single_components() {
        for valid in ["file.py", "a b", "日本語.txt"] {
            assert!(windows_final_name_is_valid(valid));
        }
        for invalid in ["", ".", "..", "a/b", r"a\b", "nul\0byte"] {
            assert!(!windows_final_name_is_valid(invalid));
        }
    }

    struct RootFixture {
        _temp: tempfile::TempDir,
        worker: Utf8PathBuf,
    }

    impl RootFixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let temp_path = Utf8Path::from_path(temp.path()).unwrap();
            let worker = temp_path.join("worker");
            fs::create_dir(&worker).unwrap();
            fs::create_dir(temp_path.join("outside")).unwrap();
            Self {
                _temp: temp,
                worker,
            }
        }

        fn worker_path(&self) -> Utf8PathBuf {
            self.worker.clone()
        }

        fn link_dir(&self, target: &str, link: &str) -> std::io::Result<()> {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(
                    self.worker.parent().unwrap().join(target),
                    self.worker.join(link),
                )
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_dir(
                    self.worker.parent().unwrap().join(target),
                    self.worker.join(link),
                )
            }
        }
    }

    #[test]
    fn opens_normal_nested_parent_components() {
        let fixture = RootFixture::new();
        fs::create_dir_all(fixture.worker.join("pkg/nested")).unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        let (_parent, name) = root
            .open_parent(Utf8Path::new("pkg/nested/file.py"), false)
            .unwrap();

        assert_eq!(name, OsStr::new("file.py"));
        assert_eq!(root.path(), fixture.worker);
    }

    #[test]
    fn creates_and_opens_missing_parent_components() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        let (_parent, name) = root
            .open_parent(Utf8Path::new("new/nested/file.py"), true)
            .unwrap();

        assert_eq!(name, OsStr::new("file.py"));
        assert!(fixture.worker.join("new/nested").is_dir());
    }

    #[test]
    fn missing_parent_is_created_only_when_requested() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(
            root.open_parent(Utf8Path::new("missing/file.py"), false)
                .is_err()
        );
        assert!(!fixture.worker.join("missing").exists());
    }

    #[test]
    fn rejects_non_normal_and_linked_parent_components() {
        let fixture = RootFixture::new();
        fixture.link_dir("outside", "linked").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        for path in ["", ".", "..", "../outside"] {
            assert!(matches!(
                root.open_parent(Utf8Path::new(path), false),
                Err(WorkspaceError::InvalidPath { .. })
            ));
        }
        let result = root.open_parent(Utf8Path::new("linked/secret.txt"), false);
        assert!(
            matches!(result, Err(WorkspaceError::InvalidPath { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn maps_regular_file_parent_failure_to_io() {
        let fixture = RootFixture::new();
        fs::write(fixture.worker.join("file"), b"contents").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(matches!(
            root.open_parent(Utf8Path::new("file/child"), false),
            Err(WorkspaceError::Io { .. })
        ));
    }

    #[test]
    fn parent_replacement_read_uses_the_opened_parent() {
        let fixture = RootFixture::new();
        fs::create_dir(fixture.worker.join("swap")).unwrap();
        fs::write(fixture.worker.join("swap/target"), b"worker").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let (hook, opened, resume) = PausedParent::new("read", "swap/target");
        let thread_hook = Arc::clone(&hook);
        let operation = thread::spawn(move || {
            let _guard = install_workspace_race_hook(thread_hook);
            root.read(Utf8Path::new("swap/target"))
        });

        let replacement = PausedParent::replace_parent(&fixture.worker, &opened, &resume);
        let result = operation.join().unwrap();

        assert_eq!(result.unwrap(), b"worker");
        #[cfg(windows)]
        if matches!(replacement, ParentReplacement::Denied) {
            assert_eq!(
                fs::read(fixture.worker.join("swap/target")).unwrap(),
                b"worker"
            );
            return;
        }
        let outside_permissions = replacement.permissions();
        let outside = fixture.worker.join("swap/target");
        assert_eq!(fs::read(&outside).unwrap(), b"outside",);
        assert_eq!(permission_fingerprint(&outside), outside_permissions);
    }

    #[test]
    fn parent_replacement_write_uses_the_opened_parent() {
        let fixture = RootFixture::new();
        fs::create_dir(fixture.worker.join("swap")).unwrap();
        fs::write(fixture.worker.join("swap/target"), b"worker").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let (hook, opened, resume) = PausedParent::new("write", "swap/target");
        let thread_hook = Arc::clone(&hook);
        let operation = thread::spawn(move || {
            let _guard = install_workspace_race_hook(thread_hook);
            root.write(Utf8Path::new("swap/target"), b"changed")
        });

        let replacement = PausedParent::replace_parent(&fixture.worker, &opened, &resume);
        let result = operation.join().unwrap();

        result.unwrap();
        #[cfg(windows)]
        if matches!(replacement, ParentReplacement::Denied) {
            assert_eq!(
                fs::read(fixture.worker.join("swap/target")).unwrap(),
                b"changed"
            );
            return;
        }
        let outside_permissions = replacement.permissions();
        let outside = fixture.worker.join("swap/target");
        assert_eq!(
            fs::read(fixture.worker.join("held/target")).unwrap(),
            b"changed"
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        assert_eq!(permission_fingerprint(&outside), outside_permissions);
    }

    #[test]
    fn parent_replacement_remove_uses_the_opened_parent() {
        let fixture = RootFixture::new();
        fs::create_dir(fixture.worker.join("swap")).unwrap();
        fs::write(fixture.worker.join("swap/target"), b"worker").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let (hook, opened, resume) = PausedParent::new("remove", "swap/target");
        let thread_hook = Arc::clone(&hook);
        let operation = thread::spawn(move || {
            let _guard = install_workspace_race_hook(thread_hook);
            root.remove_file(Utf8Path::new("swap/target"))
        });

        let replacement = PausedParent::replace_parent(&fixture.worker, &opened, &resume);
        let result = operation.join().unwrap();

        result.unwrap();
        #[cfg(windows)]
        if matches!(replacement, ParentReplacement::Denied) {
            assert!(!fixture.worker.join("swap/target").exists());
            return;
        }
        let outside_permissions = replacement.permissions();
        let outside = fixture.worker.join("swap/target");
        assert!(!fixture.worker.join("held/target").exists());
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        assert_eq!(permission_fingerprint(&outside), outside_permissions);
    }

    #[test]
    fn rejects_absolute_paths() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let absolute = fixture.worker.join("file.py");

        assert!(matches!(
            root.open_parent(&absolute, false),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }

    #[test]
    fn snapshot_matching_distinguishes_missing_kind_bytes_and_permissions() {
        let fixture = RootFixture::new();
        fs::write(fixture.worker.join("target"), b"expected").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let permissions = fs::metadata(fixture.worker.join("target"))
            .unwrap()
            .permissions();
        let fingerprint = super::super::permission_fingerprint(&permissions);

        assert!(
            root.snapshot_matches(Utf8Path::new("target"), b"expected", fingerprint)
                .unwrap()
        );
        assert!(
            !root
                .snapshot_matches(Utf8Path::new("target"), b"different", fingerprint)
                .unwrap()
        );
        assert!(
            !root
                .snapshot_matches(Utf8Path::new("missing"), b"expected", fingerprint)
                .unwrap()
        );

        fs::remove_file(fixture.worker.join("target")).unwrap();
        fs::create_dir(fixture.worker.join("target")).unwrap();
        assert!(
            !root
                .snapshot_matches(Utf8Path::new("target"), b"expected", fingerprint)
                .unwrap()
        );
        fs::remove_dir(fixture.worker.join("target")).unwrap();
        fs::write(fixture.worker.join("target"), b"expected").unwrap();
        let mut changed_permissions = permissions;
        changed_permissions.set_readonly(true);
        fs::set_permissions(fixture.worker.join("target"), changed_permissions).unwrap();
        assert!(
            !root
                .snapshot_matches(Utf8Path::new("target"), b"expected", fingerprint)
                .unwrap()
        );
    }

    #[test]
    fn missing_removal_is_a_noop_and_link_guards_reject_links() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        assert!(root.remove_any_if_exists(Utf8Path::new("missing")).is_ok());

        let link = fixture.worker.join("linked");
        if fixture.link_dir("outside", "linked").is_err() {
            return;
        }
        let (parent, name) = root.open_parent(Utf8Path::new("linked"), false).unwrap();
        assert!(matches!(
            WorkerRoot::reject_link(&parent, &name, Utf8Path::new("linked"), "test link",),
            Err(WorkspaceError::InvalidPath { .. })
        ));
        assert!(matches!(
            WorkerRoot::reject_link_if_present(
                &parent,
                &name,
                Utf8Path::new("linked"),
                "test link",
            ),
            Err(WorkspaceError::InvalidPath { .. })
        ));
        fs::remove_file(link)
            .or_else(|_| fs::remove_dir(fixture.worker.join("linked")))
            .unwrap();
        assert!(
            WorkerRoot::reject_link_if_present(
                &parent,
                &name,
                Utf8Path::new("linked"),
                "test link",
            )
            .is_ok()
        );
    }

    #[test]
    fn non_directory_parent_errors_are_not_treated_as_missing_entries() {
        let fixture = RootFixture::new();
        let regular = fixture.worker.join("regular");
        fs::write(&regular, b"contents").unwrap();
        let parent = File::open(regular).unwrap();
        let name = OsString::from("child");
        let path = Utf8Path::new("regular/child");

        assert!(WorkerRoot::remove_entry_if_exists(&parent, &name, path).is_err());
        assert!(
            WorkerRoot::reject_link_if_present(&parent, &name, path, "inspect test entry",)
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn mutation_writable_reopen_rejects_same_device_replacement() {
        use cap_fs_ext::OpenOptionsFollowExt;

        let fixture = RootFixture::new();
        let target = fixture.worker.join("target");
        fs::write(&target, b"original").unwrap();
        make_read_only(&target);
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let (parent, name) = root.open_parent(Utf8Path::new("target"), false).unwrap();
        let mut options = cap_primitives::fs::OpenOptions::new();
        options
            .read(true)
            .follow(cap_primitives::fs::FollowSymlinks::No);
        let inspected = cap_primitives::fs::open(&parent, Path::new(&name), &options).unwrap();
        let mutation_file = MutationFile {
            file: inspected,
            reopen: Some((parent, name)),
        };
        fs::remove_file(&target).unwrap();
        fs::write(&target, b"replacement").unwrap();

        assert!(matches!(
            mutation_file.into_writable(Utf8Path::new("target")),
            Err(WorkspaceError::InvalidPath { .. })
        ));
        assert_eq!(fs::read(target).unwrap(), b"replacement");
    }
}
