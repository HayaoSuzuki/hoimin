use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use ignore::overrides::OverrideBuilder;
use ignore::{DirEntry, WalkBuilder};

use super::{CopyOptions, WorkspaceDiagnostic, WorkspaceError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    pub path: Utf8PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub blake3: blake3::Hash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceManifest {
    entries: Vec<ManifestEntry>,
    logical_bytes: u64,
}

impl WorkspaceManifest {
    #[cfg(all(test, windows))]
    pub(crate) fn from_entries_for_test(entries: Vec<ManifestEntry>) -> Self {
        let logical_bytes = entries.iter().map(|entry| entry.size).sum();
        Self {
            entries,
            logical_bytes,
        }
    }

    #[must_use]
    pub fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }

    #[must_use]
    pub fn entry(&self, path: &Utf8Path) -> Option<&ManifestEntry> {
        self.entries
            .binary_search_by(|entry| entry.path.as_path().cmp(path))
            .ok()
            .map(|index| &self.entries[index])
    }

    pub(crate) fn content_matches(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .zip(&other.entries)
                .all(|(left, right)| {
                    left.path == right.path
                        && left.size == right.size
                        && left.blake3 == right.blake3
                })
    }

    pub(crate) fn first_content_difference(&self, other: &Self) -> Option<Utf8PathBuf> {
        let left = self
            .entries
            .iter()
            .map(|entry| (&entry.path, entry))
            .collect::<BTreeMap<_, _>>();
        let right = other
            .entries
            .iter()
            .map(|entry| (&entry.path, entry))
            .collect::<BTreeMap<_, _>>();
        left.keys()
            .chain(right.keys())
            .find(|path| match (left.get(*path), right.get(*path)) {
                (Some(left), Some(right)) => left.size != right.size || left.blake3 != right.blake3,
                _ => true,
            })
            .map(|path| (*path).clone())
    }
}

pub fn build_manifest(
    root: &Utf8Path,
    options: &CopyOptions,
) -> Result<(WorkspaceManifest, Vec<WorkspaceDiagnostic>), WorkspaceError> {
    let metadata =
        fs::metadata(root).map_err(|error| WorkspaceError::io("read root", root, error))?;
    if !metadata.is_dir() {
        return Err(WorkspaceError::RootNotDirectory(root.to_owned()));
    }
    let mut entries = BTreeMap::<Utf8PathBuf, ManifestEntry>::new();
    let mut symlinks = BTreeSet::<Utf8PathBuf>::new();

    let normal_overrides = overrides(root.as_std_path(), &[], &options.excludes)?;
    let mut normal = WalkBuilder::new(root.as_std_path());
    normal
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .overrides(normal_overrides)
        .filter_entry(|entry| !default_excluded(entry));
    collect(normal, root, &mut entries, &mut symlinks)?;

    if !options.includes.is_empty() {
        let include_overrides =
            overrides(root.as_std_path(), &options.includes, &options.excludes)?;
        let mut included = WalkBuilder::new(root.as_std_path());
        included
            .hidden(false)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .parents(false)
            .follow_links(false)
            .overrides(include_overrides)
            .filter_entry(|entry| !default_excluded(entry));
        collect(included, root, &mut entries, &mut symlinks)?;
    }

    let logical_bytes = entries.values().try_fold(0_u64, |total, entry| {
        total
            .checked_add(entry.size)
            .ok_or(WorkspaceError::CopySizeOverflow)
    })?;
    Ok((
        WorkspaceManifest {
            entries: entries.into_values().collect(),
            logical_bytes,
        },
        symlinks
            .into_iter()
            .map(|path| WorkspaceDiagnostic::SymlinkSkipped { path })
            .collect(),
    ))
}

fn overrides(
    root: &Path,
    includes: &[String],
    excludes: &[String],
) -> Result<ignore::overrides::Override, WorkspaceError> {
    let mut builder = OverrideBuilder::new(root);
    if cfg!(windows) {
        builder
            .case_insensitive(true)
            .map_err(|error| WorkspaceError::InvalidGlob(error.to_string()))?;
    }
    for include in includes {
        builder
            .add(include)
            .map_err(|error| WorkspaceError::InvalidGlob(error.to_string()))?;
    }
    for exclude in excludes {
        builder
            .add(&format!("!{exclude}"))
            .map_err(|error| WorkspaceError::InvalidGlob(error.to_string()))?;
    }
    builder
        .build()
        .map_err(|error| WorkspaceError::InvalidGlob(error.to_string()))
}

// `WalkBuilder::build` consumes its builder, so borrowing would not avoid ownership.
#[allow(
    clippy::needless_pass_by_value,
    reason = "WalkBuilder::build consumes the builder"
)]
fn collect(
    builder: WalkBuilder,
    root: &Utf8Path,
    entries: &mut BTreeMap<Utf8PathBuf, ManifestEntry>,
    symlinks: &mut BTreeSet<Utf8PathBuf>,
) -> Result<(), WorkspaceError> {
    for result in builder.build() {
        let entry = result.map_err(|error| WorkspaceError::Walk(error.to_string()))?;
        if entry.path() == root.as_std_path() {
            continue;
        }
        let path = relative_utf8(root, entry.path())?;
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            symlinks.insert(path);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let bytes = fs::read(entry.path())
            .map_err(|error| WorkspaceError::io("read manifest file", &path, error))?;
        let metadata = entry
            .metadata()
            .map_err(|error| WorkspaceError::io("read manifest metadata", &path, error))?;
        let size = u64::try_from(bytes.len()).map_err(|_| WorkspaceError::CopySizeOverflow)?;
        entries.insert(
            path.clone(),
            ManifestEntry {
                path,
                size,
                modified: metadata.modified().ok(),
                blake3: blake3::hash(&bytes),
            },
        );
    }
    Ok(())
}

pub fn relative_utf8(root: &Utf8Path, path: &Path) -> Result<Utf8PathBuf, WorkspaceError> {
    let relative = path
        .strip_prefix(root.as_std_path())
        .map_err(|_| WorkspaceError::OutsideRoot)?;
    let relative = relative
        .to_str()
        .ok_or(WorkspaceError::NonUtf8Path)?
        .replace('\\', "/");
    Ok(Utf8PathBuf::from(relative))
}

fn default_excluded(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    matches!(
        name.as_ref(),
        ".git"
            | ".venv"
            | "venv"
            | "env"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".pyre"
            | ".pytype"
            | ".tox"
            | ".nox"
    )
}
