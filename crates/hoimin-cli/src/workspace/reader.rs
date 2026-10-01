use camino::{Utf8Path, Utf8PathBuf};

use super::{WorkerRoot, WorkspaceError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum PortableFileReadError {
    #[error("root-relative file was not found")]
    NotFound,
    #[error(transparent)]
    Other(#[from] WorkspaceError),
}

/// Reads portable source and fingerprint paths through a capability-relative root.
/// Workspace copying uses `WorkerRoot` internally with its native path policy.
#[derive(Debug)]
pub(crate) struct PortableFileReader {
    root: WorkerRoot,
}

impl PortableFileReader {
    pub(crate) fn open(root: Utf8PathBuf) -> Result<Self, WorkspaceError> {
        WorkerRoot::open(root).map(|root| Self { root })
    }

    pub(crate) fn read(&self, path: &Utf8Path) -> Result<Vec<u8>, PortableFileReadError> {
        Self::validate_portable_path(path)?;
        self.classify_read(path, self.root.read(path))
    }

    fn hash(&self, path: &Utf8Path) -> Result<blake3::Hash, PortableFileReadError> {
        Self::validate_portable_path(path)?;
        self.classify_read(path, self.root.hash(path))
    }

    fn validate_portable_path(path: &Utf8Path) -> Result<(), WorkspaceError> {
        if !hoimin_core::RelativePathPolicy::Portable.allows(path.as_str()) {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_owned(),
            });
        }
        Ok(())
    }

    fn classify_read<T>(
        &self,
        path: &Utf8Path,
        result: Result<T, WorkspaceError>,
    ) -> Result<T, PortableFileReadError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => match self.root.is_missing(path) {
                Ok(true) => Err(PortableFileReadError::NotFound),
                Ok(false) | Err(_) => Err(PortableFileReadError::Other(error)),
            },
        }
    }
}

pub(crate) fn hash_portable_file(
    root: &Utf8Path,
    path: &Utf8Path,
) -> Result<blake3::Hash, PortableFileReadError> {
    PortableFileReader::open(root.to_owned())?.hash(path)
}
