use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};

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
