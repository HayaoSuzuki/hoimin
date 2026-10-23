//! Capability-relative source reads shared by discovery, analysis, planning and review.
//! Workspace lifecycle operations adapt these primitives to their own errors.
use camino::{Utf8Path, Utf8PathBuf};
#[cfg(unix)]
use cap_fs_ext::OpenOptionsFollowExt;
use cap_primitives::fs::FollowSymlinks;
#[cfg(unix)]
use cap_primitives::fs::OpenOptionsExt;
use hoimin_core::RelativePathPolicy;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
#[cfg(test)]
use std::sync::Arc;

mod reader;
pub(crate) use reader::{PortableFileReadError, PortableFileReader, hash_portable_file};
#[cfg(windows)]
pub(crate) mod windows;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum SourceFileError {
    #[error("source path is not a normalized root-relative file: {path}")]
    InvalidPath { path: Utf8PathBuf },
    #[error("{operation} failed for {path}: {message}")]
    Io {
        operation: &'static str,
        path: Utf8PathBuf,
        message: String,
    },
}
impl SourceFileError {
    fn io(operation: &'static str, path: &Utf8Path, error: impl std::fmt::Display) -> Self {
        Self::Io {
            operation,
            path: path.to_owned(),
            message: error.to_string(),
        }
    }
}

// A single test hook slot preserves race scheduling across source reads and
// workspace mutations. It is test instrumentation, not source-reader authority.
#[cfg(test)]
pub(crate) trait WorkspaceRaceHook: Send + Sync {
    fn parent_opened(&self, operation: &'static str, path: &Utf8Path);
    fn mutation_bytes_read(&self, _bytes: u64) {}
}

#[cfg(test)]
thread_local! {
    pub(crate) static WORKSPACE_RACE_HOOK: std::cell::RefCell<Option<Arc<dyn WorkspaceRaceHook>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct WorkspaceRaceHookGuard;

#[cfg(test)]
impl Drop for WorkspaceRaceHookGuard {
    fn drop(&mut self) {
        WORKSPACE_RACE_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(test)]
pub(crate) fn install_workspace_race_hook(
    hook: Arc<dyn WorkspaceRaceHook>,
) -> WorkspaceRaceHookGuard {
    WORKSPACE_RACE_HOOK.with(|slot| {
        assert!(slot.borrow_mut().replace(hook).is_none());
    });
    WorkspaceRaceHookGuard
}

#[cfg(test)]
pub(crate) fn parent_opened(operation: &'static str, path: &Utf8Path) {
    WORKSPACE_RACE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref() {
            hook.parent_opened(operation, path);
        }
    });
}

type ParentCreator<'a, E> = &'a mut dyn FnMut(&File, &Path) -> Result<(), E>;

/// The optional creation callback belongs to the workspace caller. Source readers
/// provide none, and never acquire directory creation or mutation authority.
pub(crate) fn open_parent<E: From<SourceFileError>>(
    root: &File,
    path: &Utf8Path,
    policy: RelativePathPolicy,
    missing_is_absent: bool,
    mut create: Option<ParentCreator<'_, E>>,
) -> Result<Option<(File, OsString)>, E> {
    if !policy.allows(path.as_str()) {
        return Err(SourceFileError::InvalidPath {
            path: path.to_owned(),
        }
        .into());
    }
    let components: Vec<_> = path.components().map(|part| part.as_str()).collect();
    let (name, parents) = components.split_last().expect("validated nonempty path");
    let mut parent = root
        .try_clone()
        .map_err(|error| SourceFileError::io("clone worker root", path, error))?;
    for component in parents {
        let component = Path::new(component);
        if cap_primitives::fs::stat(&parent, component, FollowSymlinks::No)
            .is_ok_and(|metadata| is_link_or_reparse(&metadata))
        {
            return Err(SourceFileError::InvalidPath {
                path: path.to_owned(),
            }
            .into());
        }
        parent = match cap_primitives::fs::open_dir_nofollow(&parent, component) {
            Ok(opened) => opened,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if let Some(create) = create.as_mut() {
                    create(&parent, component)?;
                    cap_primitives::fs::open_dir_nofollow(&parent, component)
                        .map_err(|error| map_entry_error("open worker parent", path, error))?
                } else if missing_is_absent {
                    return Ok(None);
                } else {
                    return Err(map_entry_error("open worker parent", path, error).into());
                }
            }
            Err(error) => return Err(map_entry_error("open worker parent", path, error).into()),
        };
    }
    Ok(Some((parent, OsString::from(name))))
}

pub(crate) fn with_read_file<T, E: From<SourceFileError>>(
    root: &File,
    path: &Utf8Path,
    policy: RelativePathPolicy,
    consume: impl FnOnce(File) -> Result<T, E>,
) -> Result<T, E> {
    let (parent, name) = open_parent::<SourceFileError>(root, path, policy, false, None)?
        .expect("missing parents return an error");
    #[cfg(test)]
    parent_opened("read", path);
    with_read_file_at(&parent, &name, path, consume)
}

/// Read through an already retained parent, preserving no-follow and opened-file
/// validation without traversing the path again.
pub(crate) fn with_read_file_at<T, E: From<SourceFileError>>(
    parent: &File,
    name: &OsStr,
    path: &Utf8Path,
    consume: impl FnOnce(File) -> Result<T, E>,
) -> Result<T, E> {
    let entry_is_non_file = || {
        cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No)
            .is_ok_and(|metadata| is_link_or_reparse(&metadata) || !metadata.is_file())
    };
    match cap_primitives::fs::stat(parent, Path::new(name), FollowSymlinks::No) {
        Ok(metadata) if is_link_or_reparse(&metadata) || !metadata.is_file() => {
            return Err(SourceFileError::InvalidPath {
                path: path.to_owned(),
            }
            .into());
        }
        Ok(_) => {}
        Err(error) => return Err(map_entry_error("read worker file", path, error).into()),
    }
    let consume_opened = || -> Result<T, E> {
        #[cfg(windows)]
        let opened = windows::open_read(parent, name);
        #[cfg(unix)]
        let opened = {
            let mut options = cap_primitives::fs::OpenOptions::new();
            options
                .read(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
            cap_primitives::fs::open(parent, Path::new(name), &options)
        };
        let file = match opened {
            Ok(file) => file,
            Err(_) if entry_is_non_file() => {
                return Err(SourceFileError::InvalidPath {
                    path: path.to_owned(),
                }
                .into());
            }
            #[cfg(unix)]
            Err(error) => return Err(map_entry_error("read worker file", path, error).into()),
            #[cfg(windows)]
            Err(error) => return Err(SourceFileError::io("read worker file", path, error).into()),
        };
        let metadata = file
            .metadata()
            .map_err(|error| SourceFileError::io("inspect worker file", path, error))?;
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if reparse || !metadata.is_file() {
            return Err(SourceFileError::InvalidPath {
                path: path.to_owned(),
            }
            .into());
        }
        consume(file)
    };
    #[cfg(windows)]
    {
        // Windows historically reclassifies errors after consuming the handle as
        // well, using the retained parent to detect a final-entry replacement.
        match consume_opened() {
            Err(_) if entry_is_non_file() => Err(SourceFileError::InvalidPath {
                path: path.to_owned(),
            }
            .into()),
            result => result,
        }
    }
    #[cfg(unix)]
    consume_opened()
}

pub(crate) fn read(
    root: &File,
    path: &Utf8Path,
    policy: RelativePathPolicy,
) -> Result<Vec<u8>, SourceFileError> {
    with_read_file(root, path, policy, |file| {
        let mut file = crate::finite_file::SnapshotReader::new(file)
            .map_err(|error| SourceFileError::io("inspect worker file", path, error))?;
        #[cfg(test)]
        parent_opened("read-budget", path);
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(|error| SourceFileError::io("read worker file", path, error))?;
        Ok(contents)
    })
}

pub(crate) fn hash(
    root: &File,
    path: &Utf8Path,
    policy: RelativePathPolicy,
) -> Result<blake3::Hash, SourceFileError> {
    with_read_file(root, path, policy, |file| {
        let file = crate::finite_file::SnapshotReader::new(file)
            .map_err(|error| SourceFileError::io("inspect worker file", path, error))?;
        #[cfg(test)]
        parent_opened("read-budget", path);
        let mut hasher = blake3::Hasher::new();
        hasher
            .update_reader(file)
            .map_err(|error| SourceFileError::io("read worker file", path, error))?;
        Ok(hasher.finalize())
    })
}

fn is_missing(
    root: &File,
    path: &Utf8Path,
    policy: RelativePathPolicy,
) -> Result<bool, SourceFileError> {
    let Some((parent, name)) = open_parent::<SourceFileError>(root, path, policy, true, None)?
    else {
        return Ok(true);
    };
    match cap_primitives::fs::stat(&parent, Path::new(&name), FollowSymlinks::No) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(map_entry_error("inspect worker file", path, error)),
    }
}

fn map_entry_error(operation: &'static str, path: &Utf8Path, error: io::Error) -> SourceFileError {
    if is_nofollow_link_error(&error) {
        SourceFileError::InvalidPath {
            path: path.to_owned(),
        }
    } else {
        SourceFileError::io(operation, path, error)
    }
}

pub(crate) fn open_retained_directory(path: &Path) -> io::Result<File> {
    let directory =
        cap_primitives::fs::open_ambient_dir(path, cap_primitives::ambient_authority())?;
    // Linux O_PATH handles cannot be used with fchmod. Retain a readable handle
    // before a worker can remove permissions from its directories.
    #[cfg(target_os = "linux")]
    let directory = open_readable_directory(&directory, Path::new("."))?;
    Ok(directory)
}
#[cfg(unix)]
pub(crate) fn open_readable_directory(parent: &File, name: &Path) -> io::Result<File> {
    let mut options = cap_primitives::fs::OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .custom_flags(libc::O_DIRECTORY | libc::O_NONBLOCK);
    // A readable descriptor is required for removal and fchmod; O_PATH is insufficient.
    cap_primitives::fs::open(parent, name, &options)
}

#[cfg(unix)]
pub(crate) fn is_nofollow_link_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(windows)]
pub(crate) fn is_nofollow_link_error(error: &io::Error) -> bool {
    error
        .raw_os_error()
        .and_then(|code| u32::try_from(code).ok())
        == Some(windows_sys::Win32::Foundation::ERROR_STOPPED_ON_SYMLINK)
}

#[cfg(unix)]
pub(crate) fn is_link_or_reparse(metadata: &cap_primitives::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
pub(crate) fn is_link_or_reparse(metadata: &cap_primitives::fs::Metadata) -> bool {
    use cap_primitives::fs::MetadataExt;

    metadata.file_type().is_symlink()
        || metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
}

#[cfg(any(windows, test))]
pub(crate) fn windows_final_name_units_are_valid(name: &[u16]) -> bool {
    !name.is_empty()
        && name != [u16::from(b'.')]
        && name != [u16::from(b'.'), u16::from(b'.')]
        && !name.contains(&u16::from(b'/'))
        && !name.contains(&u16::from(b'\\'))
        && !name.contains(&u16::from(b':'))
        && !name.contains(&0)
}
