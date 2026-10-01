use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
#[cfg(unix)]
use cap_fs_ext::OpenOptionsFollowExt;
use cap_primitives::fs::FollowSymlinks;
#[cfg(unix)]
use cap_primitives::fs::OpenOptionsExt;

use super::{MAX_WORKER_TREE_DEPTH, WorkspaceError};

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
pub(super) fn parent_opened(operation: &'static str, path: &Utf8Path) {
    WORKSPACE_RACE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref() {
            hook.parent_opened(operation, path);
        }
    });
}

#[cfg(windows)]
pub(crate) mod windows;

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
fn windows_final_name_units_are_valid(name: &[u16]) -> bool {
    !name.is_empty()
        && name != [u16::from(b'.')]
        && name != [u16::from(b'.'), u16::from(b'.')]
        && !name.contains(&u16::from(b'/'))
        && !name.contains(&u16::from(b'\\'))
        && !name.contains(&u16::from(b':'))
        && !name.contains(&0)
}

#[derive(Debug)]
pub(crate) struct WorkerRoot {
    path: Utf8PathBuf,
    handle: Option<File>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerEntryKind {
    File,
    Directory,
    LinkOrReparse,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct WorkerEntry {
    pub(crate) native_path: PathBuf,
    pub(crate) logical_path: Option<Utf8PathBuf>,
    pub(crate) kind: WorkerEntryKind,
}

struct CollectionFrame {
    directory: File,
    entries: cap_primitives::fs::ReadDir,
    native_prefix: PathBuf,
    logical_prefix: Option<Utf8PathBuf>,
    error_path: Utf8PathBuf,
    depth: usize,
}

struct RemovalFrame {
    name: OsString,
    logical_path: Option<Utf8PathBuf>,
    error_path: Utf8PathBuf,
    directory: File,
    entries: cap_primitives::fs::ReadDir,
    depth: usize,
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
            options
                .read(true)
                .write(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
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
        let handle = open_retained_directory(path.as_std_path())
            .map_err(|error| WorkspaceError::io("open worker root", &path, error))?;
        Ok(Self {
            path,
            handle: Some(handle),
        })
    }

    pub(crate) fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub(crate) fn close(&mut self) {
        self.handle = None;
    }

    fn handle(&self) -> &File {
        self.handle
            .as_ref()
            .expect("worker root capability is open")
    }

    pub(crate) fn ensure_directory(&self, path: &Utf8Path) -> Result<(), WorkspaceError> {
        // Opening a synthetic child's parent creates and checks every directory component;
        // no child file is created, and all opens retain the existing no-follow policy.
        self.open_parent(&path.join(".hoimin-directory-probe"), true)
            .map(|_| ())
    }

    pub(crate) fn read(&self, path: &Utf8Path) -> Result<Vec<u8>, WorkspaceError> {
        self.with_read_file(path, |mut file| {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .map_err(|error| WorkspaceError::io("read worker file", path, error))?;
            Ok(contents)
        })
    }

    pub(crate) fn hash(&self, path: &Utf8Path) -> Result<blake3::Hash, WorkspaceError> {
        self.with_read_file(path, |file| {
            let mut hasher = blake3::Hasher::new();
            hasher
                .update_reader(file)
                .map_err(|error| WorkspaceError::io("read worker file", path, error))?;
            Ok(hasher.finalize())
        })
    }

    pub(super) fn with_read_file<T>(
        &self,
        path: &Utf8Path,
        consume: impl FnOnce(File) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        #[cfg(test)]
        parent_opened("read", path);
        Self::reject_non_file(&parent, &name, path, "read worker file")?;
        #[cfg(windows)]
        {
            match windows::open_read(&parent, &name, path).and_then(consume) {
                Ok(contents) => Ok(contents),
                Err(_) if Self::entry_is_non_file(&parent, &name) => {
                    Err(WorkspaceError::InvalidPath {
                        path: path.to_owned(),
                    })
                }
                Err(error) => Err(error),
            }
        }
        #[cfg(unix)]
        {
            let mut options = cap_primitives::fs::OpenOptions::new();
            options
                .read(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
            let file = match cap_primitives::fs::open(&parent, Path::new(&name), &options) {
                Ok(file) => file,
                Err(error) => {
                    if Self::entry_is_non_file(&parent, &name) {
                        return Err(WorkspaceError::InvalidPath {
                            path: path.to_owned(),
                        });
                    }
                    return Err(Self::map_entry_error("read worker file", path, error));
                }
            };
            let metadata = file
                .metadata()
                .map_err(|error| WorkspaceError::io("inspect worker file", path, error))?;
            if !metadata.is_file() {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            consume(file)
        }
    }

    pub(crate) fn is_missing(&self, path: &Utf8Path) -> Result<bool, WorkspaceError> {
        let Some((parent, name)) = self.open_parent_if_present(path)? else {
            return Ok(true);
        };

        match cap_primitives::fs::stat(&parent, Path::new(&name), FollowSymlinks::No) {
            Ok(_) => Ok(false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(Self::map_entry_error("inspect worker file", path, error)),
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
            windows::open_mutation_file(&parent, &name, path).map(|file| MutationFile {
                file,
                reopen: Some((parent, name)),
            })
        }
        #[cfg(unix)]
        {
            Self::reject_link(&parent, &name, path, "open mutation target")?;
            let mut options = cap_primitives::fs::OpenOptions::new();
            options
                .read(true)
                .write(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
            let (file, reopen) = match cap_primitives::fs::open(&parent, Path::new(&name), &options)
            {
                Ok(file) => (file, None),
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    let mut inspect_options = cap_primitives::fs::OpenOptions::new();
                    inspect_options
                        .read(true)
                        .follow(FollowSymlinks::No)
                        .custom_flags(libc::O_NONBLOCK);
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
        Self::write_entry_with(parent, name, path, |file| {
            file.write_all(contents)
                .map_err(|error| WorkspaceError::io("write worker file", path, error))
        })
    }

    pub(super) fn with_write_file<T>(
        &self,
        path: &Utf8Path,
        write: impl FnOnce(&mut File) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        let (parent, name) = self.open_parent(path, true)?;
        Self::write_entry_with(&parent, &name, path, write)
    }

    fn write_entry_with<T>(
        parent: &File,
        name: &OsString,
        path: &Utf8Path,
        write: impl FnOnce(&mut File) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        #[cfg(windows)]
        {
            make_directory_writable(parent, path)?;
            windows::write_with(parent, name, path, write)
        }
        #[cfg(unix)]
        {
            Self::reject_link_if_present(parent, name, path, "write worker file")?;
            make_directory_writable(parent, path)?;
            if cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No).is_ok() {
                let mut inspect_options = cap_primitives::fs::OpenOptions::new();
                inspect_options
                    .read(true)
                    .follow(FollowSymlinks::No)
                    .custom_flags(libc::O_NONBLOCK);
                let file = cap_primitives::fs::open(parent, Path::new(name), &inspect_options)
                    .map_err(|error| Self::map_entry_error("open worker file", path, error))?;
                if !file
                    .metadata()
                    .map_err(|error| WorkspaceError::io("inspect worker file", path, error))?
                    .is_file()
                {
                    return Err(WorkspaceError::InvalidPath {
                        path: path.to_owned(),
                    });
                }
                make_file_writable(&file, path)?;
            }
            let mut options = cap_primitives::fs::OpenOptions::new();
            options
                .write(true)
                .create(true)
                .truncate(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
            let mut file = cap_primitives::fs::open(parent, Path::new(name), &options)
                .map_err(|error| Self::map_entry_error("write worker file", path, error))?;
            if !file
                .metadata()
                .map_err(|error| WorkspaceError::io("inspect worker file", path, error))?
                .is_file()
            {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            make_file_writable(&file, path)?;
            write(&mut file)
        }
    }

    pub(crate) fn remove_file(&self, path: &Utf8Path) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
        #[cfg(test)]
        parent_opened("remove", path);
        #[cfg(windows)]
        {
            make_directory_writable(&parent, path)?;
            windows::remove_file(&parent, &name, path)
        }
        #[cfg(unix)]
        {
            Self::reject_link(&parent, &name, path, "remove worker file")?;
            make_directory_writable(&parent, path)?;
            let mut options = cap_primitives::fs::OpenOptions::new();
            options
                .read(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
            let file = cap_primitives::fs::open(&parent, Path::new(&name), &options)
                .map_err(|error| Self::map_entry_error("remove worker file", path, error))?;
            if !file
                .metadata()
                .map_err(|error| WorkspaceError::io("inspect worker file", path, error))?
                .is_file()
            {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            make_file_writable(&file, path)?;
            drop(file);
            cap_primitives::fs::remove_file(&parent, Path::new(&name))
                .map_err(|error| Self::map_entry_error("remove worker file", path, error))
        }
    }

    pub(crate) fn try_exists(&self, path: &Utf8Path) -> Result<bool, WorkspaceError> {
        let Some((parent, name)) = self.open_parent_if_present(path)? else {
            return Ok(false);
        };
        match cap_primitives::fs::stat(&parent, Path::new(&name), FollowSymlinks::No) {
            Ok(metadata) if is_link_or_reparse(&metadata) => Err(WorkspaceError::InvalidPath {
                path: path.to_owned(),
            }),
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(Self::map_entry_error("inspect worker file", path, error)),
        }
    }

    fn open_parent_if_present(
        &self,
        path: &Utf8Path,
    ) -> Result<Option<(File, OsString)>, WorkspaceError> {
        let components = Self::components(path)?;
        let (name, parents) = components.split_last().expect("validated nonempty path");
        let mut parent = self
            .handle()
            .try_clone()
            .map_err(|error| WorkspaceError::io("clone worker root", path, error))?;

        for component in parents {
            let component = Path::new(component);
            if cap_primitives::fs::stat(&parent, component, FollowSymlinks::No)
                .is_ok_and(|metadata| is_link_or_reparse(&metadata))
            {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            match cap_primitives::fs::open_dir_nofollow(&parent, component) {
                Ok(opened) => parent = opened,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(Self::map_parent_error(path, error)),
            }
        }

        Ok(Some((parent, OsString::from(name))))
    }

    pub(crate) fn entries(&self) -> Result<Vec<WorkerEntry>, WorkspaceError> {
        #[cfg(test)]
        super::record_reset_tree_walk();
        let mut entries = Vec::new();
        Self::collect_entries(self.handle(), self.path(), &mut entries)?;
        entries.sort_by(|left, right| {
            left.native_path
                .components()
                .count()
                .cmp(&right.native_path.components().count())
                .then_with(|| left.native_path.cmp(&right.native_path))
        });
        Ok(entries)
    }

    fn collect_entries(
        directory: &File,
        worker_root: &Utf8Path,
        entries: &mut Vec<WorkerEntry>,
    ) -> Result<(), WorkspaceError> {
        let directory = directory
            .try_clone()
            .map_err(|error| WorkspaceError::io("clone worker directory", worker_root, error))?;
        let read_dir = cap_primitives::fs::read_base_dir(&directory).map_err(|error| {
            WorkspaceError::io("enumerate worker directory", worker_root, error)
        })?;
        let mut stack = vec![CollectionFrame {
            directory,
            entries: read_dir,
            native_prefix: PathBuf::new(),
            logical_prefix: Some(Utf8PathBuf::new()),
            error_path: worker_root.to_owned(),
            depth: 0,
        }];
        while !stack.is_empty() {
            let entry = {
                let frame = stack.last_mut().expect("checked nonempty stack");
                match frame.entries.next() {
                    Some(entry) => Some(entry.map_err(|error| {
                        WorkspaceError::io("enumerate worker directory", &frame.error_path, error)
                    })?),
                    None => None,
                }
            };
            let Some(entry) = entry else {
                stack.pop();
                continue;
            };

            let name = entry.file_name();
            let frame = stack.last().expect("entry belongs to current frame");
            let native_path = Self::native_child_path(&frame.native_prefix, &name);
            let logical_path =
                Self::logical_child_path_if_utf8(frame.logical_prefix.as_deref(), &name);
            let error_path = logical_path
                .clone()
                .unwrap_or_else(|| worker_root.to_owned());
            let depth = frame.depth + 1;
            if depth > MAX_WORKER_TREE_DEPTH {
                return Err(WorkspaceError::TreeDepthExceeded {
                    path: error_path,
                    limit: MAX_WORKER_TREE_DEPTH,
                });
            }
            let metadata =
                cap_primitives::fs::stat(&frame.directory, Path::new(&name), FollowSymlinks::No)
                    .map_err(|error| {
                        Self::map_entry_error("inspect worker entry", &error_path, error)
                    })?;
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
                native_path: native_path.clone(),
                logical_path: logical_path.clone(),
                kind,
            });
            if kind == WorkerEntryKind::Directory {
                let child =
                    cap_primitives::fs::open_dir_nofollow(&frame.directory, Path::new(&name))
                        .map_err(|error| {
                            Self::map_entry_error("open worker directory", &error_path, error)
                        })?;
                let child_entries = cap_primitives::fs::read_base_dir(&child).map_err(|error| {
                    WorkspaceError::io("enumerate worker directory", &error_path, error)
                })?;
                stack.push(CollectionFrame {
                    directory: child,
                    entries: child_entries,
                    native_prefix: native_path,
                    logical_prefix: logical_path,
                    error_path,
                    depth,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn remove_any_if_exists(&self, path: &Utf8Path) -> Result<(), WorkspaceError> {
        self.remove_native_if_exists(path.as_std_path(), Some(path))
    }

    pub(crate) fn clear_for_cleanup(&self) -> Result<(), WorkspaceError> {
        make_directory_writable(self.handle(), self.path())?;
        let directory = self
            .handle()
            .try_clone()
            .map_err(|error| WorkspaceError::io("clone worker root", self.path(), error))?;
        let entries = cap_primitives::fs::read_base_dir(&directory).map_err(|error| {
            WorkspaceError::io("enumerate worker directory", self.path(), error)
        })?;
        let names = entries
            .map(|entry| {
                entry.map(|entry| entry.file_name()).map_err(|error| {
                    WorkspaceError::io("enumerate worker directory", self.path(), error)
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for name in names {
            let logical_path = Self::logical_child_path_if_utf8(Some(self.path()), &name);
            let error_path = logical_path.as_deref().unwrap_or(self.path());
            Self::remove_entry_if_exists(
                &directory,
                &name,
                logical_path.as_deref(),
                error_path,
                1,
                Some("cleanup-entry"),
            )?;
        }
        Ok(())
    }

    pub(crate) fn remove_native_if_exists(
        &self,
        native_path: &Path,
        logical_path: Option<&Utf8Path>,
    ) -> Result<(), WorkspaceError> {
        let error_path = logical_path.unwrap_or(self.path());
        let (parent, name) = self.open_native_parent(native_path)?;
        #[cfg(test)]
        parent_opened("reset", error_path);
        Self::remove_entry_if_exists(
            &parent,
            &name,
            logical_path,
            error_path,
            native_path.components().count(),
            None,
        )
    }

    fn remove_entry_if_exists(
        parent: &File,
        name: &OsString,
        logical_path: Option<&Utf8Path>,
        error_path: &Utf8Path,
        depth: usize,
        checkpoint: Option<&'static str>,
    ) -> Result<(), WorkspaceError> {
        #[cfg(not(test))]
        let _ = checkpoint;
        let metadata = match cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Self::validate_missing_entry_parent(parent, error_path);
            }
            Err(error) => {
                return Err(Self::map_entry_error(
                    "inspect worker entry",
                    error_path,
                    error,
                ));
            }
        };
        #[cfg(test)]
        if let Some(operation) = checkpoint {
            parent_opened(operation, error_path);
        }
        if is_link_or_reparse(&metadata) {
            return remove_link_or_reparse(parent, name, error_path, &metadata);
        }
        if !metadata.is_dir() {
            return Self::remove_non_directory(parent, name, error_path, &metadata);
        }

        Self::remove_directory_tree(parent, name, logical_path, error_path, depth, &metadata)
    }

    fn remove_directory_tree(
        parent: &File,
        name: &OsString,
        logical_path: Option<&Utf8Path>,
        error_path: &Utf8Path,
        depth: usize,
        metadata: &cap_primitives::fs::Metadata,
    ) -> Result<(), WorkspaceError> {
        let directory = open_removal_directory(parent, name, error_path, metadata)?;
        make_directory_writable(&directory, error_path)?;
        let read_dir = cap_primitives::fs::read_base_dir(&directory)
            .map_err(|error| WorkspaceError::io("enumerate worker directory", error_path, error))?;
        let mut stack = vec![RemovalFrame {
            name: name.clone(),
            logical_path: logical_path.map(Utf8Path::to_owned),
            error_path: error_path.to_owned(),
            directory,
            entries: read_dir,
            depth,
        }];

        while !stack.is_empty() {
            let child = {
                let frame = stack.last_mut().expect("checked nonempty stack");
                match frame.entries.next() {
                    Some(entry) => Some(entry.map_err(|error| {
                        WorkspaceError::io("enumerate worker directory", &frame.error_path, error)
                    })?),
                    None => None,
                }
            };
            if let Some(child) = child {
                Self::process_removal_child(&mut stack, &child)?;
                continue;
            }

            Self::finish_removal_directory(parent, &mut stack)?;
        }
        Ok(())
    }

    fn process_removal_child(
        stack: &mut Vec<RemovalFrame>,
        child: &cap_primitives::fs::DirEntry,
    ) -> Result<(), WorkspaceError> {
        let child_name = child.file_name();
        let frame = stack.last().expect("entry belongs to current frame");
        let child_path =
            Self::logical_child_path_if_utf8(frame.logical_path.as_deref(), &child_name);
        let child_error_path = child_path
            .clone()
            .unwrap_or_else(|| frame.error_path.clone());
        let child_depth = frame.depth + 1;
        if child_depth > MAX_WORKER_TREE_DEPTH {
            return Err(WorkspaceError::TreeDepthExceeded {
                path: child_error_path,
                limit: MAX_WORKER_TREE_DEPTH,
            });
        }
        let child_metadata =
            cap_primitives::fs::stat(&frame.directory, Path::new(&child_name), FollowSymlinks::No)
                .map_err(|error| {
                    Self::map_entry_error("inspect worker entry", &child_error_path, error)
                })?;
        if is_link_or_reparse(&child_metadata) {
            return remove_link_or_reparse(
                &frame.directory,
                &child_name,
                &child_error_path,
                &child_metadata,
            );
        }
        if !child_metadata.is_dir() {
            return Self::remove_non_directory(
                &frame.directory,
                &child_name,
                &child_error_path,
                &child_metadata,
            );
        }

        let directory = open_removal_directory(
            &frame.directory,
            &child_name,
            &child_error_path,
            &child_metadata,
        )?;
        make_directory_writable(&directory, &child_error_path)?;
        let entries = cap_primitives::fs::read_base_dir(&directory).map_err(|error| {
            WorkspaceError::io("enumerate worker directory", &child_error_path, error)
        })?;
        stack.push(RemovalFrame {
            name: child_name,
            logical_path: child_path,
            error_path: child_error_path,
            directory,
            entries,
            depth: child_depth,
        });
        Ok(())
    }

    fn finish_removal_directory(
        root_parent: &File,
        stack: &mut Vec<RemovalFrame>,
    ) -> Result<(), WorkspaceError> {
        let frame = stack.pop().expect("checked nonempty stack");
        drop(frame.entries);
        drop(frame.directory);
        let parent = stack.last().map_or(root_parent, |parent| &parent.directory);
        #[cfg(windows)]
        windows::remove_entry(parent, &frame.name, &frame.error_path)?;
        #[cfg(unix)]
        cap_primitives::fs::remove_dir(parent, Path::new(&frame.name)).map_err(|error| {
            Self::map_entry_error("remove worker directory", &frame.error_path, error)
        })?;
        Ok(())
    }

    fn remove_non_directory(
        parent: &File,
        name: &OsString,
        logical_path: &Utf8Path,
        metadata: &cap_primitives::fs::Metadata,
    ) -> Result<(), WorkspaceError> {
        #[cfg(windows)]
        {
            let _ = metadata;
            windows::remove_file(parent, name, logical_path)
        }
        #[cfg(unix)]
        {
            if metadata.is_file() {
                let expected_identity = (
                    cap_fs_ext::MetadataExt::dev(metadata),
                    cap_fs_ext::MetadataExt::ino(metadata),
                );
                let mut options = cap_primitives::fs::OpenOptions::new();
                options
                    .read(true)
                    .follow(FollowSymlinks::No)
                    .custom_flags(libc::O_NONBLOCK);
                match cap_primitives::fs::open(parent, Path::new(name), &options) {
                    Ok(file) => {
                        use std::os::unix::fs::MetadataExt;

                        let opened = file.metadata().map_err(|error| {
                            WorkspaceError::io("inspect worker file", logical_path, error)
                        })?;
                        if !opened.is_file()
                            || opened.dev() != expected_identity.0
                            || opened.ino() != expected_identity.1
                        {
                            return Err(WorkspaceError::InvalidPath {
                                path: logical_path.to_owned(),
                            });
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                        let verified =
                            cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No)
                                .map_err(|error| {
                                Self::map_entry_error("inspect worker file", logical_path, error)
                            })?;
                        if is_link_or_reparse(&verified)
                            || !verified.is_file()
                            || cap_fs_ext::MetadataExt::dev(&verified) != expected_identity.0
                            || cap_fs_ext::MetadataExt::ino(&verified) != expected_identity.1
                        {
                            return Err(WorkspaceError::InvalidPath {
                                path: logical_path.to_owned(),
                            });
                        }
                    }
                    Err(error) => {
                        return Err(Self::map_entry_error(
                            "open worker file",
                            logical_path,
                            error,
                        ));
                    }
                }
            }
            let operation = if metadata.is_file() {
                "remove worker file"
            } else {
                "remove worker special entry"
            };
            cap_primitives::fs::remove_file(parent, Path::new(name))
                .map_err(|error| Self::map_entry_error(operation, logical_path, error))
        }
    }

    #[cfg(test)]
    pub(crate) fn restore(
        &self,
        path: &Utf8Path,
        contents: &[u8],
        permissions: std::fs::Permissions,
    ) -> Result<(), WorkspaceError> {
        self.restore_from(path, &mut io::Cursor::new(contents), permissions)
    }

    pub(super) fn restore_from(
        &self,
        path: &Utf8Path,
        reader: &mut impl Read,
        permissions: std::fs::Permissions,
    ) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, true)?;
        #[cfg(test)]
        parent_opened("reset", path);
        Self::remove_entry_if_exists(
            &parent,
            &name,
            Some(path),
            path,
            path.components().count(),
            None,
        )?;
        Self::write_entry_with(&parent, &name, path, |file| {
            super::stream::chunks(reader, path, "read shared snapshot", |bytes| {
                file.write_all(bytes)
                    .map_err(|error| WorkspaceError::io("write worker file", path, error))
            })?;
            file.set_permissions(permissions)
                .map_err(|error| WorkspaceError::io("restore worker permissions", path, error))
        })
    }

    #[cfg(test)]
    pub(crate) fn snapshot_matches(
        &self,
        path: &Utf8Path,
        expected: &[u8],
        expected_permissions: super::PermissionFingerprint,
    ) -> Result<bool, WorkspaceError> {
        self.snapshot_matches_reader(
            path,
            &mut io::Cursor::new(expected),
            expected.len() as u64,
            expected_permissions,
        )
    }

    pub(super) fn snapshot_matches_reader(
        &self,
        path: &Utf8Path,
        expected: &mut impl Read,
        expected_size: u64,
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
        // A size mismatch proves inequality; matching metadata never proves equal contents.
        if metadata.len() != expected_size {
            return Ok(false);
        }
        #[cfg(windows)]
        let mut file = windows::open_read(&parent, &name, path)?;
        #[cfg(unix)]
        let mut file = {
            let mut options = cap_primitives::fs::OpenOptions::new();
            options
                .read(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
            cap_primitives::fs::open(&parent, Path::new(&name), &options)
                .map_err(|error| Self::map_entry_error("verify restored file", path, error))?
        };
        let file_metadata = file
            .metadata()
            .map_err(|error| WorkspaceError::io("verify restored file", path, error))?;
        if !file_metadata.is_file() || file_metadata.len() != expected_size {
            return Ok(false);
        }
        let equal = super::stream::equal(expected, &mut file, |_, count| {
            #[cfg(test)]
            super::record_reset_worker_bytes(count);
            #[cfg(not(test))]
            let _ = count;
        })
        .map_err(|error| WorkspaceError::io("verify restored file", path, error))?;
        Ok(equal
            && super::permission_fingerprint(&file_metadata.permissions()) == expected_permissions)
    }

    fn components(path: &Utf8Path) -> Result<Vec<&str>, WorkspaceError> {
        if !super::WORKSPACE_PATH_POLICY.allows(path.as_str()) {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_owned(),
            });
        }
        Ok(path.components().map(|part| part.as_str()).collect())
    }

    fn logical_child_path(parent: &Utf8Path, name: &str) -> Utf8PathBuf {
        if parent.as_str().is_empty() {
            Utf8PathBuf::from(name)
        } else {
            Utf8PathBuf::from(format!("{parent}/{name}"))
        }
    }

    fn native_child_path(parent: &Path, name: &std::ffi::OsStr) -> PathBuf {
        let mut path = parent.to_owned();
        path.push(name);
        path
    }

    fn logical_child_path_if_utf8(
        parent: Option<&Utf8Path>,
        name: &std::ffi::OsStr,
    ) -> Option<Utf8PathBuf> {
        Some(Self::logical_child_path(parent?, name.to_str()?))
    }

    fn open_native_parent(&self, path: &Path) -> Result<(File, OsString), WorkspaceError> {
        let components = path
            .components()
            .map(|component| match component {
                Component::Normal(component) => Ok(component.to_owned()),
                _ => Err(WorkspaceError::InvalidPath {
                    path: self.path.clone(),
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (name, parents) =
            components
                .split_last()
                .ok_or_else(|| WorkspaceError::InvalidPath {
                    path: self.path.clone(),
                })?;
        let mut parent = self
            .handle()
            .try_clone()
            .map_err(|error| WorkspaceError::io("clone worker root", self.path(), error))?;
        for component in parents {
            let component = Path::new(component);
            if cap_primitives::fs::stat(&parent, component, FollowSymlinks::No)
                .is_ok_and(|metadata| is_link_or_reparse(&metadata))
            {
                return Err(WorkspaceError::InvalidPath {
                    path: self.path.clone(),
                });
            }
            parent = cap_primitives::fs::open_dir_nofollow(&parent, component)
                .map_err(|error| Self::map_parent_error(self.path(), error))?;
        }
        Ok((parent, name.clone()))
    }

    fn validate_missing_entry_parent(
        parent: &File,
        logical_path: &Utf8Path,
    ) -> Result<(), WorkspaceError> {
        let metadata = parent
            .metadata()
            .map_err(|error| WorkspaceError::io("inspect worker parent", logical_path, error))?;
        if metadata.is_dir() {
            Ok(())
        } else {
            Err(WorkspaceError::io(
                "inspect worker entry",
                logical_path,
                io::Error::new(
                    io::ErrorKind::NotADirectory,
                    "worker parent is not a directory",
                ),
            ))
        }
    }

    pub(crate) fn open_parent(
        &self,
        path: &Utf8Path,
        create: bool,
    ) -> Result<(File, OsString), WorkspaceError> {
        let components = Self::components(path)?;
        let (name, parents) = components.split_last().expect("validated nonempty path");
        let mut parent = self
            .handle()
            .try_clone()
            .map_err(|error| WorkspaceError::io("clone worker root", path, error))?;

        for component in parents {
            let component_path = Path::new(component);
            if cap_primitives::fs::stat(&parent, component_path, FollowSymlinks::No)
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

    fn reject_non_file(
        parent: &File,
        name: &OsString,
        logical_path: &Utf8Path,
        operation: &'static str,
    ) -> Result<(), WorkspaceError> {
        match cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No) {
            Ok(metadata) if is_link_or_reparse(&metadata) || !metadata.is_file() => {
                Err(WorkspaceError::InvalidPath {
                    path: logical_path.to_owned(),
                })
            }
            Ok(_) => Ok(()),
            Err(error) => Err(Self::map_entry_error(operation, logical_path, error)),
        }
    }

    fn entry_is_non_file(parent: &File, name: &OsString) -> bool {
        cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No)
            .is_ok_and(|metadata| is_link_or_reparse(&metadata) || !metadata.is_file())
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
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Self::validate_missing_entry_parent(parent, logical_path)
            }
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

pub(super) fn open_cleanup_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FILE_WRITE_ATTRIBUTES,
        };

        // Retain attribute-write access before worker code makes the wrapper readonly.
        // As with ordinary retained directories, deny delete sharing to pin its identity.
        let directory = std::fs::OpenOptions::new()
            .access_mode(FILE_GENERIC_READ | FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(path)?;
        if !directory.metadata()?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "cleanup wrapper is not a directory",
            ));
        }
        Ok(directory)
    }
    #[cfg(not(windows))]
    open_retained_directory(path)
}

pub(super) fn open_retained_directory(path: &Path) -> io::Result<File> {
    let directory =
        cap_primitives::fs::open_ambient_dir(path, cap_primitives::ambient_authority())?;
    // Linux capability opens return O_PATH descriptors, which cannot be used with fchmod.
    // Bind a readable handle now, before worker code can remove directory permissions.
    #[cfg(target_os = "linux")]
    let directory = open_readable_directory(&directory, Path::new("."))?;
    Ok(directory)
}

#[cfg(unix)]
fn open_readable_directory(parent: &File, name: &Path) -> io::Result<File> {
    let mut options = cap_primitives::fs::OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .custom_flags(libc::O_DIRECTORY | libc::O_NONBLOCK);
    // Do not use open_dir_nofollow: on Linux it requests O_PATH and succeeds even for
    // mode-000 directories. Removal needs a readable, fchmod-capable descriptor.
    cap_primitives::fs::open(parent, name, &options)
}

#[cfg(unix)]
fn open_removal_directory(
    parent: &File,
    name: &OsString,
    path: &Utf8Path,
    metadata: &cap_primitives::fs::Metadata,
) -> Result<File, WorkspaceError> {
    let expected_identity = (
        cap_fs_ext::MetadataExt::dev(metadata),
        cap_fs_ext::MetadataExt::ino(metadata),
    );
    let directory = match open_readable_directory(parent, Path::new(name)) {
        Ok(directory) => directory,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            #[cfg(test)]
            parent_opened("cleanup-inaccessible-directory", path);
            make_inaccessible_directory_accessible(parent, name, metadata, path)?;
            let verified = cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No)
                .map_err(|error| {
                    WorkerRoot::map_entry_error("inspect worker directory", path, error)
                })?;
            if is_link_or_reparse(&verified)
                || !verified.is_dir()
                || cap_fs_ext::MetadataExt::dev(&verified) != expected_identity.0
                || cap_fs_ext::MetadataExt::ino(&verified) != expected_identity.1
            {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_owned(),
                });
            }
            open_readable_directory(parent, Path::new(name)).map_err(|error| {
                WorkerRoot::map_entry_error("open worker directory", path, error)
            })?
        }
        Err(error) => {
            return Err(WorkerRoot::map_entry_error(
                "open worker directory",
                path,
                error,
            ));
        }
    };
    let opened_metadata = directory
        .metadata()
        .map_err(|error| WorkspaceError::io("inspect worker directory", path, error))?;
    if !opened_metadata.is_dir()
        || std::os::unix::fs::MetadataExt::dev(&opened_metadata) != expected_identity.0
        || std::os::unix::fs::MetadataExt::ino(&opened_metadata) != expected_identity.1
    {
        return Err(WorkspaceError::InvalidPath {
            path: path.to_owned(),
        });
    }
    Ok(directory)
}

#[cfg(unix)]
fn make_inaccessible_directory_accessible(
    parent: &File,
    name: &OsString,
    metadata: &cap_primitives::fs::Metadata,
    path: &Utf8Path,
) -> Result<(), WorkspaceError> {
    use cap_primitives::fs::PermissionsExt;

    let mut permissions = metadata.permissions();
    if permissions.mode() & 0o700 == 0o700 {
        return Err(WorkspaceError::io(
            "open worker directory",
            path,
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "worker directory is inaccessible despite owner access bits",
            ),
        ));
    }
    permissions.set_mode(permissions.mode() | 0o700);
    // Require a directory at the permission effect itself: a preceding stat cannot prevent
    // replacement with a hard link to an outside regular file.
    let mut directory_name = name.clone();
    directory_name.push("/");
    #[cfg(target_os = "linux")]
    let result =
        cap_primitives::fs::set_permissions(parent, Path::new(&directory_name), permissions);
    #[cfg(target_os = "macos")]
    let result: io::Result<()> = {
        // Darwin's AT_SYMLINK_NOFOLLOW alone follows a link with a trailing slash. The
        // SDK's AT_SYMLINK_NOFOLLOW_ANY rejects links in every component, including that
        // case. libc/rustix do not yet expose the constant. Unsupported kernels fail closed.
        const AT_SYMLINK_NOFOLLOW_ANY: u32 = 0x0800;
        rustix::fs::chmodat(
            parent,
            Path::new(&directory_name),
            rustix::fs::Mode::from_bits_truncate(permissions.mode() as rustix::fs::RawMode),
            rustix::fs::AtFlags::from_bits_retain(AT_SYMLINK_NOFOLLOW_ANY),
        )
        .map_err(Into::into)
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let result: io::Result<()> = Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "safe inaccessible-directory permission repair is unavailable",
    ));
    result.map_err(|error| WorkspaceError::io("prepare worker directory", path, error))
}

#[cfg(windows)]
fn open_removal_directory(
    parent: &File,
    name: &OsString,
    path: &Utf8Path,
    _metadata: &cap_primitives::fs::Metadata,
) -> Result<File, WorkspaceError> {
    cap_primitives::fs::open_dir_nofollow(parent, Path::new(name))
        .map_err(|error| WorkerRoot::map_entry_error("open worker directory", path, error))
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
        windows::remove_entry(parent, name, path)
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
    error
        .raw_os_error()
        .and_then(|code| u32::try_from(code).ok())
        == Some(windows_sys::Win32::Foundation::ERROR_STOPPED_ON_SYMLINK)
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
                    assert_eq!(error.raw_os_error(), Some(32));
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
            assert!(windows_final_name_units_are_valid(
                &valid.encode_utf16().collect::<Vec<_>>()
            ));
        }
        for invalid in ["", ".", "..", "a/b", r"a\b", "name:stream", "nul\0byte"] {
            assert!(!windows_final_name_units_are_valid(
                &invalid.encode_utf16().collect::<Vec<_>>()
            ));
        }
        assert!(windows_final_name_units_are_valid(&[
            u16::from(b'x'),
            0xD800,
        ]));
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

        fn link_dir(&self, target: &str, link: &str) -> io::Result<()> {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(
                    self.worker.parent().unwrap().join(target),
                    self.worker.join(link),
                )
            }
            #[cfg(windows)]
            {
                let target = self.worker.parent().unwrap().join(target);
                let link = self.worker.join(link);
                match std::os::windows::fs::symlink_dir(&target, &link) {
                    Ok(()) => Ok(()),
                    Err(error)
                        if error.kind() == std::io::ErrorKind::PermissionDenied
                            || error.kind() == std::io::ErrorKind::Unsupported
                            || error.raw_os_error() == Some(1314) =>
                    {
                        let output = std::process::Command::new("cmd")
                            .arg("/C")
                            .arg("mklink")
                            .arg("/J")
                            .arg(&link)
                            .arg(&target)
                            .output()?;
                        if output.status.success() {
                            Ok(())
                        } else {
                            Err(std::io::Error::other(format!(
                                "failed to create Windows test junction: {}",
                                String::from_utf8_lossy(&output.stderr)
                            )))
                        }
                    }
                    Err(error) => Err(error),
                }
            }
        }

        fn create_nested_directories(&self, root_name: &str, depth: usize) {
            let root = cap_primitives::fs::open_ambient_dir(
                self.worker.as_std_path(),
                cap_primitives::ambient_authority(),
            )
            .unwrap();
            cap_primitives::fs::create_dir(
                &root,
                Path::new(root_name),
                &cap_primitives::fs::DirOptions::new(),
            )
            .unwrap();
            let mut directory =
                cap_primitives::fs::open_dir_nofollow(&root, Path::new(root_name)).unwrap();
            for index in 1..depth {
                let name = OsString::from(format!("d{index}"));
                cap_primitives::fs::create_dir(
                    &directory,
                    Path::new(&name),
                    &cap_primitives::fs::DirOptions::new(),
                )
                .unwrap();
                directory =
                    cap_primitives::fs::open_dir_nofollow(&directory, Path::new(&name)).unwrap();
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
    fn try_exists_returns_true_for_a_regular_file() {
        let fixture = RootFixture::new();
        fs::write(fixture.worker.join("present.py"), b"contents").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(root.try_exists(Utf8Path::new("present.py")).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn read_rejects_a_fifo() {
        let fixture = RootFixture::new();
        let output = std::process::Command::new("mkfifo")
            .arg(fixture.worker.join("fifo.py"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(matches!(
            root.read(Utf8Path::new("fifo.py")),
            Err(WorkspaceError::InvalidPath { .. })
        ));
        assert!(matches!(
            root.hash(Utf8Path::new("fifo.py")),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }

    #[test]
    fn try_exists_returns_false_when_the_final_entry_is_missing() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(!root.try_exists(Utf8Path::new("missing.py")).unwrap());
    }

    #[test]
    fn try_exists_returns_false_when_an_intermediate_parent_is_missing() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(!root.try_exists(Utf8Path::new("missing/file.py")).unwrap());
    }

    #[test]
    fn try_exists_preserves_non_directory_parent_errors() {
        let fixture = RootFixture::new();
        fs::write(fixture.worker.join("regular"), b"contents").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(matches!(
            root.try_exists(Utf8Path::new("regular/child")),
            Err(WorkspaceError::Io { .. })
        ));
    }

    #[test]
    fn try_exists_rejects_linked_parents() {
        let fixture = RootFixture::new();
        fixture.link_dir("outside", "linked").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(matches!(
            root.try_exists(Utf8Path::new("linked/secret.txt")),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }

    #[test]
    fn try_exists_rejects_non_normal_paths() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        assert!(matches!(
            root.try_exists(Utf8Path::new("../outside")),
            Err(WorkspaceError::InvalidPath { .. })
        ));
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
    fn parent_replacement_hash_uses_the_opened_parent() {
        let fixture = RootFixture::new();
        fs::create_dir(fixture.worker.join("swap")).unwrap();
        fs::write(fixture.worker.join("swap/target"), b"worker").unwrap();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let (hook, opened, resume) = PausedParent::new("read", "swap/target");
        let thread_hook = Arc::clone(&hook);
        let operation = thread::spawn(move || {
            let _guard = install_workspace_race_hook(thread_hook);
            root.hash(Utf8Path::new("swap/target"))
        });

        let replacement = PausedParent::replace_parent(&fixture.worker, &opened, &resume);
        let result = operation.join().unwrap();

        assert_eq!(result.unwrap(), blake3::hash(b"worker"));
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
    fn logical_child_paths_always_use_portable_separators() {
        assert_eq!(
            WorkerRoot::logical_child_path(Utf8Path::new("parent"), "child").as_str(),
            "parent/child"
        );
        assert_eq!(
            WorkerRoot::logical_child_path(Utf8Path::new(""), "child").as_str(),
            "child"
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_child_identity_does_not_require_utf8() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let name = OsString::from_vec(vec![b'n', 0x80]);
        let native = WorkerRoot::native_child_path(Path::new("parent"), &name);

        assert_eq!(native.as_os_str().as_bytes(), b"parent/n\x80");
        assert_eq!(WorkerRoot::logical_child_path_if_utf8(None, &name), None);
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

        assert!(
            WorkerRoot::remove_entry_if_exists(
                &parent,
                &name,
                Some(path),
                path,
                path.components().count(),
                None,
            )
            .is_err()
        );
        assert!(
            WorkerRoot::reject_link_if_present(&parent, &name, path, "inspect test entry",)
                .is_err()
        );
    }

    #[test]
    fn post_order_removal_handles_a_tree_at_the_supported_depth() {
        let fixture = RootFixture::new();
        fixture.create_nested_directories("deep", 128);
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        root.remove_any_if_exists(Utf8Path::new("deep")).unwrap();

        assert!(!fixture.worker.join("deep").exists());
    }

    #[test]
    fn post_order_removal_reports_the_shared_depth_limit() {
        let fixture = RootFixture::new();
        fixture.create_nested_directories("deep", 129);
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();

        let error = root
            .remove_any_if_exists(Utf8Path::new("deep"))
            .unwrap_err();

        assert!(matches!(
            error,
            WorkspaceError::TreeDepthExceeded {
                limit: MAX_WORKER_TREE_DEPTH,
                ..
            }
        ));
        assert!(fixture.worker.join("deep").exists());
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
        options.read(true).follow(FollowSymlinks::No);
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
