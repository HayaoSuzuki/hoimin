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

/// A finite read of the opened artifact, without imposing a fixed total-size cap.
///
/// Read at most the length observed on this descriptor, then probe one byte to
/// reject growth rather than accept a truncated prefix or chase an appending
/// producer. This is a resource boundary, not an atomic filesystem snapshot:
/// concurrent in-place rewrites are not detected. Callers must consume EOF.
pub(crate) struct SnapshotReader {
    file: File,
    remaining: u64,
    length: u64,
}

impl SnapshotReader {
    pub(crate) fn new(file: File) -> std::io::Result<Self> {
        let remaining = file.metadata()?.len();
        Ok(Self {
            file,
            remaining,
            length: remaining,
        })
    }

    /// Reuse the same opened artifact and byte budget for a routing probe.
    pub(crate) fn rewind(&mut self) -> std::io::Result<()> {
        std::io::Seek::rewind(&mut self.file)?;
        self.remaining = self.length;
        Ok(())
    }
}

impl Read for SnapshotReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut extra = [0];
            if self.file.read(&mut extra)? != 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "artifact grew while being read; retry with a completed file",
                ));
            }
            return Ok(0);
        }
        let limit = output
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        let count = self.file.read(&mut output[..limit])?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "artifact shrank while being read; retry with a completed file",
            ));
        }
        self.remaining -= count as u64;
        Ok(count)
    }
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

    #[test]
    fn snapshot_reader_rejects_append_without_consuming_growth() {
        use std::io::Write;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, b"null").unwrap();
        let mut reader = SnapshotReader::new(File::open(&path).unwrap()).unwrap();
        let mut writer = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writer.write_all(&[b' '; 4096]).unwrap();
        let mut bytes = [0; 8192];
        assert_eq!(reader.read(&mut bytes).unwrap(), 4);
        assert_eq!(&bytes[..4], b"null");
        assert_eq!(
            reader.read(&mut bytes).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        // One byte beyond the original length, rather than the whole append.
        assert_eq!(std::io::Seek::stream_position(&mut reader.file).unwrap(), 5);
    }

    #[test]
    fn snapshot_reader_rejects_shrink_and_accepts_exact_eof() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, b"null").unwrap();
        let mut exact = SnapshotReader::new(File::open(&path).unwrap()).unwrap();
        let mut bytes = Vec::new();
        exact.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"null");
        let mut shortened = SnapshotReader::new(File::open(&path).unwrap()).unwrap();
        File::create(&path).unwrap();
        assert_eq!(
            shortened.read(&mut [0; 8]).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }

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
