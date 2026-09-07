#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use ignore::overrides::OverrideBuilder;
use ignore::{DirEntry, WalkBuilder};

use crate::portable_path;

use super::{CopyOptions, WorkspaceDiagnostic, WorkspaceError};

#[cfg(test)]
thread_local! {
    static BUILD_METRICS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
    static SOURCE_CONTENT_METRICS: Cell<SourceContentMetrics> = const {
        Cell::new(SourceContentMetrics {
            reads: 0,
            read_bytes: 0,
            hashes: 0,
            hash_bytes: 0,
        })
    };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceContentMetrics {
    pub(crate) reads: u64,
    pub(crate) read_bytes: u64,
    pub(crate) hashes: u64,
    pub(crate) hash_bytes: u64,
}

#[cfg(test)]
pub(crate) fn reset_build_metrics() {
    BUILD_METRICS.with(|metrics| metrics.set((0, 0)));
}

#[cfg(test)]
pub(crate) fn build_metrics() -> (u64, u64) {
    BUILD_METRICS.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_source_content_metrics() {
    SOURCE_CONTENT_METRICS.with(|metrics| {
        metrics.set(SourceContentMetrics {
            reads: 0,
            read_bytes: 0,
            hashes: 0,
            hash_bytes: 0,
        });
    });
}

#[cfg(test)]
pub(crate) fn source_content_metrics() -> SourceContentMetrics {
    SOURCE_CONTENT_METRICS.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn record_source_content_read(bytes: usize) {
    SOURCE_CONTENT_METRICS.with(|metrics| {
        let mut current = metrics.get();
        current.reads += 1;
        current.read_bytes += bytes as u64;
        metrics.set(current);
    });
}

#[cfg(test)]
pub(crate) fn record_source_content_hash(bytes: usize) {
    SOURCE_CONTENT_METRICS.with(|metrics| {
        let mut current = metrics.get();
        current.hashes += 1;
        current.hash_bytes += bytes as u64;
        metrics.set(current);
    });
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    pub path: Utf8PathBuf,
    pub size: u64,
    pub blake3: blake3::Hash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceManifest {
    entries: Vec<ManifestEntry>,
    logical_bytes: u64,
}

impl WorkspaceManifest {
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
    build_manifest_with_contents(root, options, |_, _| Ok(()))
}

pub(crate) fn build_manifest_with_contents(
    root: &Utf8Path,
    options: &CopyOptions,
    mut observe: impl FnMut(&ManifestEntry, &[u8]) -> Result<(), WorkspaceError>,
) -> Result<(WorkspaceManifest, Vec<WorkspaceDiagnostic>), WorkspaceError> {
    #[cfg(test)]
    BUILD_METRICS.with(|metrics| {
        let (builds, bytes) = metrics.get();
        metrics.set((builds + 1, bytes));
    });
    let mut entries = BTreeMap::<Utf8PathBuf, ManifestEntry>::new();
    let symlinks = walk_selected_files(root, options, &mut |path, native_path| {
        let bytes = fs::read(native_path)
            .map_err(|error| WorkspaceError::io("read manifest file", &path, error))?;
        #[cfg(test)]
        record_source_content_read(bytes.len());
        #[cfg(test)]
        BUILD_METRICS.with(|metrics| {
            let (builds, total_bytes) = metrics.get();
            metrics.set((builds, total_bytes + bytes.len() as u64));
        });
        let size = u64::try_from(bytes.len()).map_err(|_| WorkspaceError::CopySizeOverflow)?;
        let blake3 = blake3::hash(&bytes);
        #[cfg(test)]
        record_source_content_hash(bytes.len());
        let entry = ManifestEntry { path, size, blake3 };
        observe(&entry, &bytes)?;
        entries.insert(entry.path.clone(), entry);
        Ok(())
    })?;

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

pub(crate) fn inventory_logical_bytes(
    root: &Utf8Path,
    options: &CopyOptions,
) -> Result<u64, WorkspaceError> {
    let mut sizes = BTreeMap::<Utf8PathBuf, u64>::new();
    walk_selected_files(root, options, &mut |path, native_path| {
        let size = fs::metadata(native_path)
            .map_err(|error| WorkspaceError::io("read inventory metadata", &path, error))?
            .len();
        sizes.insert(path, size);
        Ok(())
    })?;
    sizes.values().try_fold(0_u64, |total, size| {
        total
            .checked_add(*size)
            .ok_or(WorkspaceError::CopySizeOverflow)
    })
}

fn walk_selected_files(
    root: &Utf8Path,
    options: &CopyOptions,
    visit: &mut impl FnMut(Utf8PathBuf, &Path) -> Result<(), WorkspaceError>,
) -> Result<BTreeSet<Utf8PathBuf>, WorkspaceError> {
    let metadata =
        fs::metadata(root).map_err(|error| WorkspaceError::io("read root", root, error))?;
    if !metadata.is_dir() {
        return Err(WorkspaceError::RootNotDirectory(root.to_owned()));
    }
    let mut symlinks = BTreeSet::<Utf8PathBuf>::new();

    let normal_overrides = overrides(root.as_std_path(), &[], &options.excludes)?;
    let mut normal = WalkBuilder::new(root.as_std_path());
    normal
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .overrides(normal_overrides)
        .filter_entry(|entry| !default_excluded(entry));
    collect(normal, root, &mut symlinks, visit)?;

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
        collect(included, root, &mut symlinks, visit)?;
    }

    Ok(symlinks)
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
    symlinks: &mut BTreeSet<Utf8PathBuf>,
    visit: &mut impl FnMut(Utf8PathBuf, &Path) -> Result<(), WorkspaceError>,
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
        visit(path, entry.path())?;
    }
    Ok(())
}

pub fn relative_utf8(root: &Utf8Path, path: &Path) -> Result<Utf8PathBuf, WorkspaceError> {
    let relative = path
        .strip_prefix(root.as_std_path())
        .map_err(|_| WorkspaceError::OutsideRoot)?;
    let relative = relative.to_str().ok_or(WorkspaceError::NonUtf8Path)?;
    let relative =
        portable_path::from_native(relative).map_err(|error| WorkspaceError::InvalidPath {
            path: Utf8PathBuf::from(error.into_value()),
        })?;
    Ok(Utf8PathBuf::from(relative.into_owned()))
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
