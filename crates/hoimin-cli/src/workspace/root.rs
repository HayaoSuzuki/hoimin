use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use cap_fs_ext::OpenOptionsFollowExt;
use cap_primitives::fs::FollowSymlinks;

use super::WorkspaceError;

#[derive(Debug)]
pub(crate) struct WorkerRoot {
    path: Utf8PathBuf,
    handle: File,
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

    pub(crate) fn write(&self, path: &Utf8Path, contents: &[u8]) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, true)?;
        Self::reject_link_if_present(&parent, &name, path, "write worker file")?;
        make_directory_writable(&parent, path)?;
        if cap_primitives::fs::stat(&parent, Path::new(&name), FollowSymlinks::No).is_ok() {
            let mut inspect_options = cap_primitives::fs::OpenOptions::new();
            inspect_options.read(true).follow(FollowSymlinks::No);
            let file = cap_primitives::fs::open(&parent, Path::new(&name), &inspect_options)
                .map_err(|error| Self::map_entry_error("open worker file", path, error))?;
            make_file_writable(&file, path)?;
        }
        let mut options = cap_primitives::fs::OpenOptions::new();
        options
            .write(true)
            .create(true)
            .truncate(true)
            .follow(FollowSymlinks::No);
        let mut file = cap_primitives::fs::open(&parent, Path::new(&name), &options)
            .map_err(|error| Self::map_entry_error("write worker file", path, error))?;
        make_file_writable(&file, path)?;
        file.write_all(contents)
            .map_err(|error| WorkspaceError::io("write worker file", path, error))
    }

    pub(crate) fn remove_file(&self, path: &Utf8Path) -> Result<(), WorkspaceError> {
        let (parent, name) = self.open_parent(path, false)?;
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

    use camino::{Utf8Path, Utf8PathBuf};

    use super::*;

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
    fn rejects_absolute_paths() {
        let fixture = RootFixture::new();
        let root = WorkerRoot::open(fixture.worker_path()).unwrap();
        let absolute = fixture.worker.join("file.py");

        assert!(matches!(
            root.open_parent(&absolute, false),
            Err(WorkspaceError::InvalidPath { .. })
        ));
    }
}
