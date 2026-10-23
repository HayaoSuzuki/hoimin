use camino::Utf8Path;

use super::{SourceFileError, open_retained_directory};
use hoimin_core::RelativePathPolicy;
use std::fs::File;

#[derive(Debug, thiserror::Error)]
pub(crate) enum PortableFileReadError {
    #[error("root-relative file was not found")]
    NotFound,
    #[error(transparent)]
    Other(#[from] SourceFileError),
}

/// Reads portable source and fingerprint paths through a capability-relative root.
/// Source read/hash reject length changes; bounded hierarchy reads keep their
/// separate fixed-cap policy. Workspace copying uses its native path policy.
#[derive(Debug)]
pub(crate) struct PortableFileReader {
    root: File,
}

impl PortableFileReader {
    pub(crate) fn open(root: &Utf8Path) -> Result<Self, SourceFileError> {
        open_retained_directory(root.as_std_path())
            .map(|root| Self { root })
            .map_err(|error| SourceFileError::io("open worker root", root, error))
    }

    pub(crate) fn read(&self, path: &Utf8Path) -> Result<Vec<u8>, PortableFileReadError> {
        Self::validate_portable_path(path)?;
        self.classify_read(
            path,
            super::read(&self.root, path, RelativePathPolicy::Portable),
        )
    }

    pub(crate) fn read_bounded(
        &self,
        path: &Utf8Path,
        limit: usize,
    ) -> Result<Vec<u8>, PortableFileReadError> {
        use std::io::Read;
        Self::validate_portable_path(path)?;
        self.classify_read(
            path,
            super::with_read_file(&self.root, path, RelativePathPolicy::Portable, |file| {
                let exceeds_limit = || {
                    SourceFileError::io(
                        "read hierarchy input",
                        path,
                        std::io::Error::other("exception hierarchy source byte limit exceeded"),
                    )
                };
                // Inspect the already-opened handle so oversized inputs are rejected
                // before allocation without introducing a path replacement race.
                let metadata = file
                    .metadata()
                    .map_err(|e| SourceFileError::io("inspect bounded input", path, e))?;
                if metadata.len() > limit as u64 {
                    return Err(exceeds_limit());
                }
                let mut bytes = Vec::new();
                file.take(limit as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| SourceFileError::io("read hierarchy input", path, e))?;
                if bytes.len() > limit {
                    return Err(exceeds_limit());
                }
                Ok(bytes)
            }),
        )
    }

    pub(crate) fn hash(&self, path: &Utf8Path) -> Result<blake3::Hash, PortableFileReadError> {
        Self::validate_portable_path(path)?;
        self.classify_read(
            path,
            super::hash(&self.root, path, RelativePathPolicy::Portable),
        )
    }

    fn validate_portable_path(path: &Utf8Path) -> Result<(), SourceFileError> {
        if !hoimin_core::RelativePathPolicy::Portable.allows(path.as_str()) {
            return Err(SourceFileError::InvalidPath {
                path: path.to_owned(),
            });
        }
        Ok(())
    }

    fn classify_read<T>(
        &self,
        path: &Utf8Path,
        result: Result<T, SourceFileError>,
    ) -> Result<T, PortableFileReadError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => match super::is_missing(&self.root, path, RelativePathPolicy::Portable) {
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
    PortableFileReader::open(root)?.hash(path)
}
