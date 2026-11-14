//! Descriptor-length read budget shared by workspace and artifact consumers.

use std::fs::File;
use std::io::Read;

/// A finite read of the opened file, without imposing a fixed total-size cap.
///
/// Read at most the length observed on this descriptor, then probe one byte to
/// reject growth rather than accept a truncated prefix or chase an appending
/// producer. This is a resource boundary, not an atomic filesystem snapshot:
/// concurrent in-place rewrites are not detected. Finite bytes do not impose a
/// filesystem I/O time bound. Callers must consume EOF.
pub(crate) struct SnapshotReader<R = File> {
    file: R,
    remaining: u64,
}

impl SnapshotReader {
    pub(crate) fn new(file: File) -> std::io::Result<Self> {
        let remaining = crate::io_faults::checked_io!("snapshot.metadata", file.metadata())?.len();
        Ok(Self { file, remaining })
    }
}

impl<'file> SnapshotReader<&'file mut File> {
    /// Borrow an opened descriptor after its observed length has been checked.
    pub(crate) fn borrowed(file: &'file mut File, length: u64) -> Self {
        Self {
            file,
            remaining: length,
        }
    }
}

#[derive(Debug)]
enum SnapshotLengthChange {
    Grew,
    Shrank,
}

impl std::fmt::Display for SnapshotLengthChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Grew => "file grew while being read; retry with a completed file",
            Self::Shrank => "file shrank while being read; retry with a completed file",
        })
    }
}

impl std::error::Error for SnapshotLengthChange {}

/// Distinguish descriptor length changes from ordinary filesystem read errors.
pub(crate) fn is_snapshot_length_change(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<SnapshotLengthChange>)
}

impl<R: Read> Read for SnapshotReader<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut extra = [0];
            if self.file.read(&mut extra)? != 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    SnapshotLengthChange::Grew,
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
                SnapshotLengthChange::Shrank,
            ));
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let growth = reader.read(&mut bytes).unwrap_err();
        assert_eq!(growth.kind(), std::io::ErrorKind::InvalidData);
        assert!(is_snapshot_length_change(&growth));
        // One byte beyond the original length, rather than the whole append.
        assert_eq!(std::io::Seek::stream_position(&mut reader.file).unwrap(), 5);
    }

    #[test]
    fn snapshot_reader_accepts_exact_eof() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, b"null").unwrap();
        let mut reader = SnapshotReader::new(File::open(&path).unwrap()).unwrap();
        let mut bytes = Vec::new();

        reader.read_to_end(&mut bytes).unwrap();

        assert_eq!(bytes, b"null");
    }

    #[test]
    fn snapshot_reader_rejects_shrink() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, b"null").unwrap();
        let mut reader = SnapshotReader::new(File::open(&path).unwrap()).unwrap();
        File::create(&path).unwrap();

        let shrink = reader.read(&mut [0; 8]).unwrap_err();

        assert_eq!(shrink.kind(), std::io::ErrorKind::UnexpectedEof);
        assert!(is_snapshot_length_change(&shrink));
    }

    #[rstest::rstest]
    #[case::ordinary_invalid_data(std::io::ErrorKind::InvalidData)]
    #[case::ordinary_eof(std::io::ErrorKind::UnexpectedEof)]
    fn ordinary_filesystem_errors_are_not_snapshot_length_changes(
        #[case] kind: std::io::ErrorKind,
    ) {
        let error = std::io::Error::new(kind, "ordinary filesystem error");

        let actual = is_snapshot_length_change(&error);

        assert!(!actual);
    }

    #[rstest::rstest]
    #[case::before_eof(b"null".as_slice())]
    #[case::at_eof(b"".as_slice())]
    fn an_empty_read_never_reads_or_probes_the_underlying_file(#[case] payload: &[u8]) {
        use std::io::Seek;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, payload).unwrap();
        let file = File::options().write(true).open(path).unwrap();
        let mut reader = SnapshotReader::new(file).unwrap();

        let actual_0 = reader.read(&mut []);
        let actual_2 = reader.file.stream_position();

        assert_eq!(actual_0.unwrap(), 0);
        assert_eq!(reader.remaining, u64::try_from(payload.len()).unwrap());
        assert_eq!(actual_2.unwrap(), 0);
    }

    #[rstest::rstest]
    #[case::payload(b"null".as_slice())]
    #[case::eof_probe(b"".as_slice())]
    fn underlying_read_errors_preserve_the_budget_and_io_cause(#[case] payload: &[u8]) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("input");
        std::fs::write(&path, payload).unwrap();
        let file = File::options().write(true).open(path).unwrap();
        let expected = file.try_clone().unwrap().read(&mut [7]).unwrap_err();
        let mut reader = SnapshotReader::new(file).unwrap();
        let mut bytes = [7];

        let error = reader.read(&mut bytes).unwrap_err();
        let actual_2 = is_snapshot_length_change(&error);

        assert_eq!(error.kind(), expected.kind());
        assert_eq!(error.raw_os_error(), expected.raw_os_error());
        assert!(!actual_2);
        assert_eq!(bytes, [7]);
        assert_eq!(reader.remaining, u64::try_from(payload.len()).unwrap());
    }
}
