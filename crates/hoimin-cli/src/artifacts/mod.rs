//! Shared artifact contracts; command-specific selection and comparison stay in callers.

use std::fs::File;
use std::io::Read;
use std::path::Path;

pub(crate) mod run_report;

pub(crate) const READ_ONLY_ARTIFACT_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ArtifactReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

/// Read a regular file, bounding both the initial size and growth during reading.
/// The caller owns format validation and its command's error presentation.
pub(crate) fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, ArtifactReadError> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > limit {
        return Err(ArtifactReadError::Invalid(format!(
            "input exceeds size limit or is not a file: {}",
            path.display()
        )));
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
