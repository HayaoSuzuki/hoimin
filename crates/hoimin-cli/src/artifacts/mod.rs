//! Shared artifact contracts; command-specific selection and comparison stay in callers.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(crate) mod run_report;

pub(crate) const READ_ONLY_ARTIFACT_LIMIT: u64 = 64 * 1024 * 1024;

/// SQLite companion names in the database's supplied spelling, without filesystem I/O.
/// Callers own path resolution, protection policy, and validation order.
pub(crate) struct SqliteCompanions {
    pub(crate) wal: PathBuf,
    pub(crate) shm: PathBuf,
    pub(crate) journal: PathBuf,
}

impl SqliteCompanions {
    pub(crate) fn new(database: &Path) -> Self {
        let companion = |suffix: &str| {
            let mut name = database.as_os_str().to_owned();
            name.push(suffix);
            PathBuf::from(name)
        };
        Self {
            wal: companion("-wal"),
            shm: companion("-shm"),
            journal: companion("-journal"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ArtifactReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

/// Open without waiting for a FIFO writer, then validate the obtained descriptor.
/// Input symlinks are followed; callers own any separate no-symlink policy.
pub(crate) fn open_regular_file(path: &Path) -> std::io::Result<Option<File>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Inspecting the path before opening would leave a replacement race.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    Ok(file.metadata()?.is_file().then_some(file))
}

/// Read a regular file, bounding both the initial size and growth during reading.
/// The caller owns format validation and its command's error presentation.
pub(crate) fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, ArtifactReadError> {
    let invalid = || {
        ArtifactReadError::Invalid(format!(
            "input exceeds size limit or is not a file: {}",
            path.display()
        ))
    };
    let file = open_regular_file(path)?.ok_or_else(invalid)?;
    if file.metadata()?.len() > limit {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(ArtifactReadError::Invalid(
            "input exceeds size limit".into(),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn exact_size_files_preserve_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..=1024)) {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("input");
            std::fs::write(&path, &bytes).unwrap();

            let actual = bounded_read(&path, u64::try_from(bytes.len()).unwrap()).unwrap();

            prop_assert_eq!(actual, bytes);
        }
    }

    #[cfg(unix)]
    #[test]
    fn regular_file_symlink_preserves_bytes() {
        // Arrange
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        let link = directory.path().join("link");
        std::fs::write(&target, b"1\xff\0").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        // Act
        let bytes = bounded_read(&link, 3).unwrap();

        // Assert
        assert_eq!(bytes, b"1\xff\0");
    }

    #[test]
    fn oversized_file_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, b"1234").unwrap();

        let result = bounded_read(&path, 3);

        assert!(matches!(result, Err(ArtifactReadError::Invalid(_))));
    }

    #[test]
    fn missing_file_preserves_io_error_category() {
        let directory = tempfile::tempdir().unwrap();

        let result = bounded_read(&directory.path().join("missing"), 3);

        assert!(matches!(result, Err(ArtifactReadError::Io(_))));
    }
}
