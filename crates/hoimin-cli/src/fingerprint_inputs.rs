use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::FingerprintInputFile;
use ignore::WalkBuilder;
use ignore::overrides::{Override, OverrideBuilder};

use crate::portable_path;
use crate::workspace::{self, RootRelativeReadError, WorkspaceManifest};

#[derive(Clone, Debug, thiserror::Error)]
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
    let mut selected = resolve_patterns(root, patterns)?.selected;
    for file in files {
        let path = resolve_exact(file)?;
        let digest = workspace::hash_root_relative(root, &path).map_err(|error| match error {
            RootRelativeReadError::NotFound => FingerprintInputError::NotFound(file.clone()),
            RootRelativeReadError::Other(error) => {
                FingerprintInputError::ExactUnsupportedFile(format!("{path}: {error}"))
            }
        })?;
        selected.insert(path, Some(digest));
    }

    selected
        .into_iter()
        .map(|(path, exact_digest)| {
            let digest = exact_digest.map_or_else(
                || {
                    workspace::hash_root_relative(root, &path).map_err(|error| match error {
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
                hash: digest.to_hex().to_string(),
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
    let path = portable_path::from_native(input)
        .map_err(|_| FingerprintInputError::InvalidPath(input.to_owned()))?;
    if path.is_empty()
        || input.contains('\0')
        || path.starts_with('/')
        || path.split('/').any(|component| component.contains(':'))
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
            .any(|component| component.contains(':'))
        || slash_pattern.split('/').any(|component| component == "..")
    {
        return Err(FingerprintInputError::InvalidGlob(pattern.to_owned()));
    }
    let _ = build_override(root, pattern)?;
    Ok(())
}

struct PatternState {
    pattern: String,
    overrides: Override,
    matched: bool,
    error: Option<FingerprintInputError>,
}

struct PatternResolution {
    selected: BTreeMap<Utf8PathBuf, Option<blake3::Hash>>,
    #[cfg(test)]
    walks: usize,
}

#[allow(
    clippy::too_many_lines,
    reason = "pattern compilation, one traversal, and ordered error replay stay together"
)]
fn resolve_patterns(
    root: &Utf8Path,
    patterns: &[String],
) -> Result<PatternResolution, FingerprintInputError> {
    let mut states = Vec::new();
    let mut deferred_error = None;
    for pattern in patterns {
        if let Err(error) = validate_pattern(root, pattern) {
            deferred_error = Some(error);
            break;
        }
        states.push(PatternState {
            pattern: pattern.clone(),
            overrides: build_override(root, pattern)?,
            matched: false,
            error: None,
        });
    }
    if states.is_empty() {
        return deferred_error.map_or_else(
            || {
                Ok(PatternResolution {
                    selected: BTreeMap::new(),
                    #[cfg(test)]
                    walks: 0,
                })
            },
            Err,
        );
    }

    let mut union_builder = OverrideBuilder::new(root.as_std_path());
    for state in &states {
        if !state.pattern.starts_with('!') {
            union_builder
                .add(&state.pattern)
                .map_err(|error| FingerprintInputError::InvalidGlob(error.to_string()))?;
        }
    }
    let union = union_builder
        .build()
        .map_err(|error| FingerprintInputError::InvalidGlob(error.to_string()))?;
    let mut builder = WalkBuilder::new(root.as_std_path());
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .follow_links(false)
        .overrides(union);

    let mut selected = BTreeMap::new();
    #[cfg(test)]
    let mut walks = 0;
    #[cfg(test)]
    {
        walks += 1;
    }
    for result in builder.build() {
        let entry =
            result.map_err(|error| FingerprintInputError::UnsupportedFile(error.to_string()))?;
        if entry.path() == root.as_std_path() {
            continue;
        }
        let is_directory = entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir());
        let matching = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| {
                state
                    .overrides
                    .matched(entry.path(), is_directory)
                    .is_whitelist()
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        for &index in &matching {
            states[index].matched = true;
        }
        let relative = entry
            .path()
            .strip_prefix(root.as_std_path())
            .map_err(|_| {
                FingerprintInputError::UnsupportedFile("selected path is outside root".to_owned())
            })
            .and_then(|path| {
                Utf8PathBuf::from_path_buf(path.to_path_buf()).map_err(|_| {
                    FingerprintInputError::UnsupportedFile(
                        "selected path must be valid UTF-8".to_owned(),
                    )
                })
            })
            .and_then(|path| {
                portable_path::from_native(path.as_str())
                    .map(|path| Utf8PathBuf::from(path.into_owned()))
                    .map_err(|error| FingerprintInputError::UnsupportedFile(error.into_value()))
            });
        let relative = match relative {
            Ok(relative) => relative,
            Err(error) => {
                for index in matching {
                    states[index].error.get_or_insert_with(|| error.clone());
                }
                continue;
            }
        };
        let file_type = entry
            .file_type()
            .ok_or_else(|| FingerprintInputError::UnsupportedFile(relative.as_str().to_owned()))?;
        if file_type.is_dir() || file_type.is_symlink() || !file_type.is_file() {
            let error = FingerprintInputError::UnsupportedFile(relative.as_str().to_owned());
            for index in matching {
                states[index].error.get_or_insert_with(|| error.clone());
            }
            continue;
        }
        selected.entry(relative).or_insert(None);
    }
    for state in states {
        if let Some(error) = state.error {
            return Err(error);
        }
        if !state.matched {
            return Err(FingerprintInputError::Unmatched(state.pattern));
        }
    }
    if let Some(error) = deferred_error {
        return Err(error);
    }
    Ok(PatternResolution {
        selected,
        #[cfg(test)]
        walks,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_count_and_unrelated_tree_size_do_not_multiply_walks() {
        for unrelated in [0, 64] {
            let directory = tempfile::tempdir().unwrap();
            std::fs::write(directory.path().join("a.toml"), "a").unwrap();
            std::fs::create_dir(directory.path().join("unrelated")).unwrap();
            for index in 0..unrelated {
                std::fs::write(
                    directory
                        .path()
                        .join("unrelated")
                        .join(format!("{index}.txt")),
                    "x",
                )
                .unwrap();
            }
            let root = Utf8Path::from_path(directory.path()).unwrap();
            assert_eq!(resolve_patterns(root, &[]).unwrap().walks, 0);
            for count in [1, 2, 4] {
                let patterns = std::iter::repeat_n("*.toml".to_owned(), count).collect::<Vec<_>>();
                let resolution = resolve_patterns(root, &patterns).unwrap();
                assert_eq!(
                    resolution.walks, 1,
                    "patterns={count}, unrelated={unrelated}"
                );
                assert_eq!(
                    resolution.selected.keys().collect::<Vec<_>>(),
                    [Utf8Path::new("a.toml")]
                );
            }
        }
    }
}
