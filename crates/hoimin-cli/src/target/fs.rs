use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use camino::Utf8PathBuf;
use hoimin_core::{DiscoveredFile, Selection};
use ignore::overrides::OverrideBuilder;
use ignore::{DirEntry, Walk, WalkBuilder};

use crate::portable_path;

#[derive(Debug)]
pub enum FsTargetError {
    InvalidGlob(ignore::Error),
    Walk(ignore::Error),
    NonUtf8Path,
    OutsideRoot,
    UnsupportedPath(String),
}

impl fmt::Display for FsTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGlob(error) => write!(formatter, "invalid include/exclude glob: {error}"),
            Self::Walk(error) => write!(formatter, "target discovery failed: {error}"),
            Self::NonUtf8Path => formatter.write_str("target path must be valid UTF-8"),
            Self::OutsideRoot => formatter.write_str("discovered path is outside root"),
            Self::UnsupportedPath(path) => {
                write!(formatter, "target path cannot be represented portably: {path}")
            }
        }
    }
}

impl std::error::Error for FsTargetError {}

/// Discovers explicitly selected files from the filesystem.
///
/// # Errors
///
/// Returns an error when a glob is invalid, traversal fails, or a path is unsuitable.
pub fn discover_explicit(selection: &Selection) -> Result<Vec<DiscoveredFile>, FsTargetError> {
    let root = selection.root.as_std_path();
    let mut files = BTreeMap::<Utf8PathBuf, DiscoveredFile>::new();

    let excludes = build_overrides(root, &[], &selection.excludes)?;
    let mut normal = WalkBuilder::new(root);
    normal.overrides(excludes);
    collect(normal.build(), root, &mut files)?;

    if !selection.includes.is_empty() {
        let includes = build_overrides(root, &selection.includes, &selection.excludes)?;
        let mut restored = WalkBuilder::new(root);
        restored
            .hidden(false)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .parents(false)
            .overrides(includes);
        collect(restored.build(), root, &mut files)?;
    }

    Ok(files.into_values().collect())
}

fn build_overrides(
    root: &Path,
    includes: &[String],
    excludes: &[String],
) -> Result<ignore::overrides::Override, FsTargetError> {
    let mut builder = OverrideBuilder::new(root);
    if cfg!(windows) {
        builder
            .case_insensitive(true)
            .map_err(FsTargetError::InvalidGlob)?;
    }
    for include in includes {
        builder.add(include).map_err(FsTargetError::InvalidGlob)?;
    }
    for exclude in excludes {
        builder
            .add(&format!("!{exclude}"))
            .map_err(FsTargetError::InvalidGlob)?;
    }
    builder.build().map_err(FsTargetError::InvalidGlob)
}

fn collect(
    builder: Walk,
    root: &Path,
    files: &mut BTreeMap<Utf8PathBuf, DiscoveredFile>,
) -> Result<(), FsTargetError> {
    for entry in builder {
        let entry = entry.map_err(FsTargetError::Walk)?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| FsTargetError::OutsideRoot)?;
            let relative = relative.to_str().ok_or(FsTargetError::NonUtf8Path)?;
            let relative = portable_path::from_native(relative)
                .map_err(|error| FsTargetError::UnsupportedPath(error.into_value()))?;
            let path = Utf8PathBuf::from(relative.into_owned());
            files.insert(
                path.clone(),
                DiscoveredFile {
                    is_python: is_python(&entry),
                    path,
                },
            );
        }
    }
    Ok(())
}

fn is_python(entry: &DirEntry) -> bool {
    entry
        .path()
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
}
