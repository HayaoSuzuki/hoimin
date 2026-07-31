use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::FingerprintInputFile;
use ignore::WalkBuilder;
use ignore::overrides::{Override, OverrideBuilder};

use crate::workspace::{self, RootRelativeReadError, WorkspaceManifest};

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

#[derive(Debug, thiserror::Error)]
pub(crate) enum FingerprintInputRecheckError {
    #[error(transparent)]
    Resolution(#[from] FingerprintInputError),
    #[error("resolved fingerprint input records differ from prepared configuration")]
    RecordsChanged,
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
            selected.entry(path).or_insert(None);
        }
    }
    for file in files {
        let path = resolve_exact(file)?;
        let bytes = workspace::read_root_relative(root, &path).map_err(|error| match error {
            RootRelativeReadError::NotFound => FingerprintInputError::NotFound(file.clone()),
            RootRelativeReadError::Other(error) => {
                FingerprintInputError::ExactUnsupportedFile(format!("{path}: {error}"))
            }
        })?;
        selected.insert(path, Some(bytes));
    }

    selected
        .into_iter()
        .map(|(path, exact_bytes)| {
            let bytes = exact_bytes.map_or_else(
                || {
                    workspace::read_root_relative(root, &path).map_err(|error| match error {
                        RootRelativeReadError::NotFound => {
                            FingerprintInputError::UnsupportedFile(format!("{path}: not found"))
                        }
                        RootRelativeReadError::Other(error) => {
                            FingerprintInputError::UnsupportedFile(format!("{path}: {error}"))
                        }
                    })
                },
                Ok,
            )?;
            Ok(FingerprintInputFile {
                path,
                hash: blake3::hash(&bytes).to_hex().to_string(),
            })
        })
        .collect()
}

/// Re-resolves fingerprint inputs and compares them with the records frozen into configuration.
pub(crate) fn recheck(
    root: &Utf8Path,
    patterns: &[String],
    files: &[String],
    expected: &[FingerprintInputFile],
) -> Result<(), FingerprintInputRecheckError> {
    let current = resolve(root, patterns, files)?;
    if current == expected {
        Ok(())
    } else {
        Err(FingerprintInputRecheckError::RecordsChanged)
    }
}

pub(crate) fn recheck_manifest(
    root: &Utf8Path,
    patterns: &[String],
    files: &[String],
    expected: &[FingerprintInputFile],
    initial: &WorkspaceManifest,
    copied_at_start: &BTreeSet<Utf8PathBuf>,
) -> Result<(), FingerprintInputRecheckError> {
    let expected = expected
        .iter()
        .map(|record| (&record.path, record.hash.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut selected = BTreeMap::new();

    for pattern in patterns {
        let overrides = build_override(root, pattern)?;
        for entry in initial.entries() {
            if overrides
                .matched(root.join(&entry.path).as_std_path(), false)
                .is_whitelist()
            {
                selected.insert(&entry.path, entry.blake3.to_hex().to_string());
            }
        }
    }
    for file in files {
        let path = normalize_exact_path(file)?;
        if let Some(entry) = initial.entry(&path) {
            selected.insert(&entry.path, entry.blake3.to_hex().to_string());
        }
    }

    if selected
        .iter()
        .any(|(path, hash)| expected.get(path).copied() != Some(hash.as_str()))
        || copied_at_start
            .iter()
            .any(|path| initial.entry(path).is_none())
    {
        Err(FingerprintInputRecheckError::RecordsChanged)
    } else {
        Ok(())
    }
}

fn resolve_exact(input: &str) -> Result<Utf8PathBuf, FingerprintInputError> {
    normalize_exact_path(input)
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
