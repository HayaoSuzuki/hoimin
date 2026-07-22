use std::collections::BTreeMap;
use std::io::ErrorKind;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::FingerprintInputFile;
use ignore::WalkBuilder;
use ignore::overrides::{Override, OverrideBuilder};

#[derive(Debug, thiserror::Error)]
pub enum FingerprintInputError {
    #[error("fingerprint.include.invalid_glob: {0}")]
    InvalidGlob(String),
    #[error("fingerprint.include.unmatched: {0}")]
    Unmatched(String),
    #[error("fingerprint.include.unsupported_file: {0}")]
    UnsupportedFile(String),
    #[error("fingerprint.file.invalid_path: {0}")]
    InvalidPath(String),
    #[error("fingerprint.file.not_found: {0}")]
    NotFound(String),
    #[error("fingerprint.file.unsupported_file: {0}")]
    ExactUnsupportedFile(String),
}

/// Resolves root-relative fingerprint input glob patterns to sorted, hashed file records.
///
/// # Errors
///
/// Returns an error when a pattern is unsafe or invalid, matches no files, or selects a
/// non-regular, non-UTF-8, unreadable, or outside-root path.
pub fn resolve(
    root: &Utf8Path,
    patterns: &[String],
    files: &[String],
) -> Result<Vec<FingerprintInputFile>, FingerprintInputError> {
    let mut selected = BTreeMap::new();
    for pattern in patterns {
        validate_pattern(root, pattern)?;
        let matched = resolve_one(root, pattern)?;
        if matched.is_empty() {
            return Err(FingerprintInputError::Unmatched(pattern.clone()));
        }
        for path in matched {
            selected.entry(path).or_insert(false);
        }
    }
    for file in files {
        selected.insert(resolve_exact(root, file)?, true);
    }

    selected
        .into_iter()
        .map(|(path, exact)| {
            let bytes = std::fs::read(root.join(&path)).map_err(|error| {
                if exact {
                    FingerprintInputError::ExactUnsupportedFile(format!("{path}: {error}"))
                } else {
                    FingerprintInputError::UnsupportedFile(format!("{path}: {error}"))
                }
            })?;
            Ok(FingerprintInputFile {
                path,
                hash: blake3::hash(&bytes).to_hex().to_string(),
            })
        })
        .collect()
}

fn resolve_exact(root: &Utf8Path, input: &str) -> Result<Utf8PathBuf, FingerprintInputError> {
    let path = normalize_exact_path(input)?;
    let metadata = std::fs::symlink_metadata(root.join(&path)).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            FingerprintInputError::NotFound(input.to_owned())
        } else {
            FingerprintInputError::ExactUnsupportedFile(format!("{path}: {error}"))
        }
    })?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() || !file_type.is_file() {
        return Err(FingerprintInputError::ExactUnsupportedFile(
            path.as_str().to_owned(),
        ));
    }
    Ok(path)
}

fn normalize_exact_path(input: &str) -> Result<Utf8PathBuf, FingerprintInputError> {
    let path = input.replace('\\', "/");
    let first = path.split('/').next().unwrap_or_default();
    if path.is_empty()
        || input.contains('\0')
        || path.starts_with('/')
        || first.contains(':')
        || path.split('/').any(|component| component == "..")
    {
        return Err(FingerprintInputError::InvalidPath(input.to_owned()));
    }
    let normalized = path
        .split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        return Err(FingerprintInputError::InvalidPath(input.to_owned()));
    }
    Ok(normalized.into())
}

fn validate_pattern(root: &Utf8Path, pattern: &str) -> Result<(), FingerprintInputError> {
    let slash_pattern = pattern.replace('\\', "/");
    if pattern.contains('\0')
        || Utf8Path::new(pattern).is_absolute()
        || slash_pattern.starts_with('/')
        || slash_pattern
            .split('/')
            .next()
            .is_some_and(|component| component.contains(':'))
        || slash_pattern.split('/').any(|component| component == "..")
    {
        return Err(FingerprintInputError::InvalidGlob(pattern.to_owned()));
    }
    let _ = build_override(root, pattern)?;
    Ok(())
}

fn resolve_one(root: &Utf8Path, pattern: &str) -> Result<Vec<Utf8PathBuf>, FingerprintInputError> {
    let overrides = build_override(root, pattern)?;
    let mut builder = WalkBuilder::new(root.as_std_path());
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .follow_links(false)
        .overrides(overrides.clone());

    let mut matched = Vec::new();
    for result in builder.build() {
        let entry =
            result.map_err(|error| FingerprintInputError::UnsupportedFile(error.to_string()))?;
        if entry.path() == root.as_std_path() {
            continue;
        }
        let is_directory = entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir());
        if !overrides.matched(entry.path(), is_directory).is_whitelist() {
            continue;
        }
        let relative = entry.path().strip_prefix(root.as_std_path()).map_err(|_| {
            FingerprintInputError::UnsupportedFile("selected path is outside root".to_owned())
        })?;
        let relative = Utf8PathBuf::from_path_buf(relative.to_path_buf()).map_err(|_| {
            FingerprintInputError::UnsupportedFile("selected path must be valid UTF-8".to_owned())
        })?;
        let relative = Utf8PathBuf::from(relative.as_str().replace('\\', "/"));
        let file_type = entry
            .file_type()
            .ok_or_else(|| FingerprintInputError::UnsupportedFile(relative.as_str().to_owned()))?;
        if file_type.is_dir() || file_type.is_symlink() || !file_type.is_file() {
            return Err(FingerprintInputError::UnsupportedFile(
                relative.as_str().to_owned(),
            ));
        }
        matched.push(relative);
    }
    Ok(matched)
}

fn build_override(root: &Utf8Path, pattern: &str) -> Result<Override, FingerprintInputError> {
    let mut builder = OverrideBuilder::new(root.as_std_path());
    builder
        .add(pattern)
        .map_err(|error| FingerprintInputError::InvalidGlob(error.to_string()))?;
    builder
        .build()
        .map_err(|error| FingerprintInputError::InvalidGlob(error.to_string()))
}
