#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use ignore::overrides::OverrideBuilder;
use ignore::{DirEntry, WalkBuilder};

use crate::{copy_policy::default_excluded, portable_path};

use super::{CopyOptions, LiteralExclusion, WorkspaceDiagnostic, WorkspaceError};

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
    BUILD_METRICS.set((0, 0));
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
pub(crate) fn record_source_content_read(bytes: u64) {
    SOURCE_CONTENT_METRICS.with(|metrics| {
        let mut current = metrics.get();
        current.reads += 1;
        current.read_bytes += bytes;
        metrics.set(current);
    });
}

#[cfg(test)]
pub(crate) fn record_source_content_hash(bytes: u64) {
    SOURCE_CONTENT_METRICS.with(|metrics| {
        let mut current = metrics.get();
        current.hashes += 1;
        current.hash_bytes += bytes;
        metrics.set(current);
    });
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    pub path: Utf8PathBuf,
    pub size: u64,
    pub blake3: blake3::Hash,
}

/// Immutable selected-file metadata. Clones cheaply share the same snapshot data.
///
/// Metadata remains valid while any handle lives, independently of filesystem owners.
/// It contains no worker root, copy reservation, charge, or cleanup authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceManifest {
    storage: Arc<ManifestStorage>,
}

#[derive(Debug, Eq, PartialEq)]
struct ManifestStorage {
    entries: Vec<ManifestEntry>,
    directories: Vec<Utf8PathBuf>,
    logical_bytes: u64,
}

impl WorkspaceManifest {
    /// Selected directories, including the ancestors of copied files and directories.
    #[must_use]
    pub fn directories(&self) -> &[Utf8PathBuf] {
        &self.storage.directories
    }

    #[must_use]
    pub fn entries(&self) -> &[ManifestEntry] {
        &self.storage.entries
    }

    #[must_use]
    pub fn logical_bytes(&self) -> u64 {
        self.storage.logical_bytes
    }

    #[must_use]
    pub fn entry(&self, path: &Utf8Path) -> Option<&ManifestEntry> {
        self.storage
            .entries
            .binary_search_by(|entry| entry.path.as_path().cmp(path))
            .ok()
            .map(|index| &self.storage.entries[index])
    }

    pub(crate) fn content_matches(&self, other: &Self) -> bool {
        self.storage.directories == other.storage.directories
            && self.storage.entries.len() == other.storage.entries.len()
            && self
                .storage
                .entries
                .iter()
                .zip(&other.storage.entries)
                .all(|(left, right)| {
                    left.path == right.path
                        && left.size == right.size
                        && left.blake3 == right.blake3
                })
    }

    pub(crate) fn first_content_difference(&self, other: &Self) -> Option<Utf8PathBuf> {
        let left_dirs = self.storage.directories.iter().collect::<BTreeSet<_>>();
        let right_dirs = other.storage.directories.iter().collect::<BTreeSet<_>>();
        if let Some(path) = left_dirs.symmetric_difference(&right_dirs).next() {
            return Some((*path).clone());
        }

        let left = self
            .storage
            .entries
            .iter()
            .map(|entry| (&entry.path, entry))
            .collect::<BTreeMap<_, _>>();
        let right = other
            .storage
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

fn insert_directory_ancestors(directories: &mut BTreeSet<Utf8PathBuf>, path: &Utf8Path) {
    let mut current = Some(path);
    while let Some(directory) = current {
        if directory.as_str().is_empty() || directories.contains(directory) {
            break;
        }
        directories.insert(directory.to_owned());
        current = directory.parent();
    }
}

pub fn build_manifest(
    root: &Utf8Path,
    options: &CopyOptions,
) -> Result<(WorkspaceManifest, Vec<WorkspaceDiagnostic>), WorkspaceError> {
    build_manifest_with_contents(root, options, |path, file| {
        let mut file = crate::finite_file::SnapshotReader::new(file)
            .map_err(|error| WorkspaceError::io("read original metadata", path, error))?;
        #[cfg(test)]
        super::root::parent_opened("manifest-budget", path);
        hash_contents(path, &mut file, |_| Ok(()))
    })
}

pub(crate) fn build_manifest_with_contents(
    root: &Utf8Path,
    options: &CopyOptions,
    mut process: impl FnMut(&Utf8Path, fs::File) -> Result<ManifestEntry, WorkspaceError>,
) -> Result<(WorkspaceManifest, Vec<WorkspaceDiagnostic>), WorkspaceError> {
    #[cfg(test)]
    BUILD_METRICS.with(|metrics| {
        let (builds, bytes) = metrics.get();
        metrics.set((builds + 1, bytes));
    });
    let source_root = super::WorkerRoot::open(root.to_owned())?;
    let mut entries = BTreeMap::<Utf8PathBuf, ManifestEntry>::new();
    let mut directories = BTreeSet::new();
    let symlinks =
        walk_selected_entries(root, options, &mut |path, _native_path, is_directory| {
            if is_directory {
                insert_directory_ancestors(&mut directories, &path);
                return Ok(());
            }
            if let Some(parent) = path.parent() {
                insert_directory_ancestors(&mut directories, parent);
            }
            let entry = source_root.with_read_file(&path, |file| process(&path, file))?;
            #[cfg(test)]
            BUILD_METRICS.with(|metrics| {
                let (builds, total_bytes) = metrics.get();
                metrics.set((builds, total_bytes + entry.size));
            });
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
            storage: Arc::new(ManifestStorage {
                entries: entries.into_values().collect(),
                directories: directories.into_iter().collect(),
                logical_bytes,
            }),
        },
        symlinks
            .into_iter()
            .map(|path| WorkspaceDiagnostic::SymlinkSkipped { path })
            .collect(),
    ))
}

pub(super) fn hash_contents(
    path: &Utf8Path,
    reader: &mut impl std::io::Read,
    mut consume: impl FnMut(&[u8]) -> Result<(), WorkspaceError>,
) -> Result<ManifestEntry, WorkspaceError> {
    let mut hasher = blake3::Hasher::new();
    let size = super::stream::chunks(reader, path, "read manifest file", |bytes| {
        hasher.update(bytes);
        consume(bytes)
    })?;
    #[cfg(test)]
    {
        record_source_content_read(size);
        record_source_content_hash(size);
    }
    Ok(ManifestEntry {
        path: path.to_owned(),
        size,
        blake3: hasher.finalize(),
    })
}

pub(crate) fn inventory_logical_bytes(
    root: &Utf8Path,
    options: &CopyOptions,
) -> Result<u64, WorkspaceError> {
    let mut sizes = BTreeMap::<Utf8PathBuf, u64>::new();
    walk_selected_entries(root, options, &mut |path, native_path, is_directory| {
        if is_directory {
            return Ok(());
        }
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

fn walk_selected_entries(
    root: &Utf8Path,
    options: &CopyOptions,
    visit: &mut impl FnMut(Utf8PathBuf, &Path, bool) -> Result<(), WorkspaceError>,
) -> Result<BTreeSet<Utf8PathBuf>, WorkspaceError> {
    let metadata =
        fs::metadata(root).map_err(|error| WorkspaceError::io("read root", root, error))?;
    if !metadata.is_dir() {
        return Err(WorkspaceError::RootNotDirectory(root.to_owned()));
    }
    for exclusion in &options.literal_exclusions {
        let path = exclusion.path();
        if !super::WORKSPACE_PATH_POLICY.allows(path.as_str()) {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_owned(),
            });
        }
    }
    let mut symlinks = BTreeSet::<Utf8PathBuf>::new();

    let normal_overrides = overrides(root.as_std_path(), &[], &options.excludes)?;
    let mut normal = WalkBuilder::new(root.as_std_path());
    normal
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .overrides(normal_overrides)
        .filter_entry(selection_filter(root, options));
    collect(normal, root, &mut symlinks, None, visit)?;

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
            .overrides(include_overrides.clone())
            .filter_entry(selection_filter(root, options));
        collect(
            included,
            root,
            &mut symlinks,
            Some(&include_overrides),
            visit,
        )?;
    }

    Ok(symlinks)
}

fn selection_filter(
    root: &Utf8Path,
    options: &CopyOptions,
) -> impl Fn(&DirEntry) -> bool + Send + Sync + 'static {
    let root = root.to_owned();
    let exclusions = options.literal_exclusions.clone();
    move |entry| {
        if default_excluded(entry) {
            return false;
        }
        if exclusions.is_empty() {
            return true;
        }
        let Some(relative) = super::relative_inside(entry.path(), root.as_std_path()) else {
            return true;
        };
        let is_directory = entry.file_type().is_some_and(|kind| kind.is_dir());
        !exclusions.iter().any(|exclusion| {
            let Some(tail) = super::relative_inside(&relative, exclusion.path().as_std_path())
            else {
                return false;
            };
            match exclusion {
                LiteralExclusion::File(_) => tail.as_os_str().is_empty() && !is_directory,
                LiteralExclusion::Tree(_) => !tail.as_os_str().is_empty() || is_directory,
            }
        })
    }
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
    included: Option<&ignore::overrides::Override>,
    visit: &mut impl FnMut(Utf8PathBuf, &Path, bool) -> Result<(), WorkspaceError>,
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
        if file_type.is_dir() {
            if included.is_none_or(|rules| rules.matched(entry.path(), true).is_whitelist()) {
                visit(path, entry.path(), true)?;
            }
        } else if file_type.is_file() {
            visit(path, entry.path(), false)?;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    use crate::workspace::{MaterializationPause, WorkspacePlan};
    use hoimin_core::{
        BudgetLedger, EffectId, RunBudgets, WorkspaceCopyGrant, reserve_workspace_copy,
    };
    use std::time::Duration;

    fn prepared_plan() -> (tempfile::TempDir, WorkspacePlan, WorkspaceCopyGrant) {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("a.py"), b"original\n").unwrap();
        let root = Utf8Path::from_path(source.path()).unwrap();
        let plan = WorkspacePlan::preflight(root, EffectId(1), 1, CopyOptions::default()).unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: plan.aggregate_bytes(),
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &plan.completed()).unwrap();
        (source, plan, grant)
    }

    #[test]
    fn last_manifest_handle_releases_metadata_after_filesystem_owners_drop() {
        // Arrange: observe metadata privately without giving production a Weak API.
        let (source, plan, grant) = prepared_plan();
        let worker = plan
            .create_worker(&grant.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let handle = plan.manifest().clone();
        let metadata = Arc::downgrade(&handle.storage);

        // Act: retain just standalone metadata, independently of disk/snapshot owners.
        drop(worker);
        drop(plan);
        drop(source);

        // Assert: data survives, then its allocation is released with the final handle.
        assert_eq!(handle.entries()[0].path, "a.py");
        assert_eq!(handle.entries()[0].size, 9);
        assert_eq!(handle.entries()[0].blake3, blake3::hash(b"original\n"));
        assert_eq!(metadata.strong_count(), 1);
        drop(handle);
        assert!(metadata.upgrade().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_waiter_leaves_manifest_owned_by_paused_materialization() {
        // Arrange: real materialization owns the final plan while its awaiter is cancelled.
        let (_source, plan, grant) = prepared_plan();
        let metadata = Arc::downgrade(&plan.manifest().storage);
        let (pause, mut controller) = MaterializationPause::new(0);
        let plan = plan.with_materialization_pause(Some(pause));
        let request = grant.create_worker(EffectId(2), 0).unwrap();
        let (sent, received) = tokio::sync::oneshot::channel();
        let blocking = tokio::task::spawn_blocking(move || {
            let worker = plan.create_worker(&request).unwrap();
            drop(plan);
            sent.send(worker).unwrap();
        });
        let waiting = tokio::spawn(async move { blocking.await.unwrap() });
        controller
            .wait_until_entered(Duration::from_secs(5))
            .unwrap();
        let release = controller.release_guard();

        // Act: aborting the awaiter detaches an already running blocking copy.
        waiting.abort();
        assert!(waiting.await.unwrap_err().is_cancelled());

        // Assert: metadata survives both detached copy and resulting worker, then releases.
        assert_eq!(metadata.strong_count(), 1);
        drop(release);
        let worker = tokio::time::timeout(Duration::from_secs(5), received)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(worker.read("a.py").unwrap(), b"original\n");
        assert_eq!(
            worker.manifest().entries()[0].blake3,
            blake3::hash(b"original\n")
        );
        drop(worker);
        assert!(metadata.upgrade().is_none());
    }

    fn assert_handle_views(
        files: &[(&str, &[u8])],
        directories: &[&str],
        drop_original_first: bool,
    ) {
        // Arrange: literal contents and a selected empty directory.
        let source = tempfile::tempdir().unwrap();
        fs::create_dir(source.path().join("empty")).unwrap();
        for (path, bytes) in files {
            let path = source.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let root = Utf8Path::from_path(source.path()).unwrap();
        let (manifest, _) = build_manifest(root, &CopyOptions::default()).unwrap();
        let (independent, _) = build_manifest(root, &CopyOptions::default()).unwrap();
        let clone = manifest.clone();

        // Act: either handle may be the last remaining snapshot metadata owner.
        let surviving = if drop_original_first {
            drop(manifest);
            clone
        } else {
            drop(clone);
            manifest
        };
        drop(source);

        // Assert: structural equality, sorted paths, sizes, hashes and empty directories.
        assert_eq!(surviving, independent);
        assert_eq!(surviving.entries().len(), files.len());
        assert_eq!(
            surviving.directories(),
            directories
                .iter()
                .map(|path| Utf8PathBuf::from(*path))
                .collect::<Vec<_>>()
        );
        let mut expected = files.to_vec();
        expected.sort_by_key(|(path, _)| *path);
        for (entry, (path, bytes)) in surviving.entries().iter().zip(expected) {
            assert_eq!(entry.path, path);
            assert_eq!(entry.size, bytes.len() as u64);
            assert_eq!(entry.blake3, blake3::hash(bytes));
            assert_eq!(surviving.entry(Utf8Path::new(path)), Some(entry));
        }
        assert_eq!(surviving.entry(Utf8Path::new("missing.py")), None);
        assert_eq!(
            surviving.logical_bytes(),
            files
                .iter()
                .map(|(_, bytes)| bytes.len() as u64)
                .sum::<u64>()
        );
    }

    #[test]
    fn empty_manifest_handle_survives_original_drop() {
        assert_handle_views(&[], &["empty"], true);
    }

    #[test]
    fn empty_file_manifest_survives_clone_drop() {
        assert_handle_views(&[("empty.py", b"")], &["empty"], false);
    }

    #[test]
    fn nested_manifest_survives_original_drop() {
        assert_handle_views(
            &[("pkg/z.py", b"abc"), ("a.py", b"\0\xff")],
            &["empty", "pkg"],
            true,
        );
    }

    #[test]
    fn nested_manifest_survives_clone_drop() {
        assert_handle_views(
            &[("pkg/z.py", b"abc"), ("a.py", b"\0\xff")],
            &["empty", "pkg"],
            false,
        );
    }

    #[test]
    fn independent_manifest_equality_detects_same_size_content_change() {
        // Arrange.
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("a.py"), b"before").unwrap();
        let root = Utf8Path::from_path(source.path()).unwrap();
        let (before, _) = build_manifest(root, &CopyOptions::default()).unwrap();
        fs::write(source.path().join("a.py"), b"after!").unwrap();

        // Act.
        let (after, _) = build_manifest(root, &CopyOptions::default()).unwrap();

        // Assert: comparison reads contents, independently of handle identity.
        assert_ne!(before, after);
        assert!(!before.content_matches(&after));
        assert_eq!(
            before.first_content_difference(&after),
            Some(Utf8PathBuf::from("a.py"))
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]
        #[test]
        fn generated_manifest_matches_file_data(data in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..65), 0..9)) {
            // Arrange: bounded real files, with independent expected byte sequences.
            let source = tempfile::tempdir().unwrap();
            fs::create_dir(source.path().join("pkg")).unwrap();
            for (index, bytes) in data.iter().enumerate() {
                fs::write(source.path().join(format!("pkg/file_{index}.py")), bytes).unwrap();
            }
            let root = Utf8Path::from_path(source.path()).unwrap();

            // Act.
            let (manifest, _) = build_manifest(root, &CopyOptions::default()).unwrap();
            let retained = manifest.clone();
            drop(manifest);
            drop(source);

            // Assert: file count, ordering, sizes, hashes and totals come from input data.
            prop_assert_eq!(retained.entries().len(), data.len());
            prop_assert_eq!(retained.directories(), &[Utf8PathBuf::from("pkg")]);
            prop_assert_eq!(retained.logical_bytes(), data.iter().map(|bytes| bytes.len() as u64).sum::<u64>());
            for (index, (entry, bytes)) in retained.entries().iter().zip(&data).enumerate() {
                prop_assert_eq!(&entry.path, &Utf8PathBuf::from(format!("pkg/file_{index}.py")));
                prop_assert_eq!(entry.size, bytes.len() as u64);
                prop_assert_eq!(entry.blake3, blake3::hash(bytes));
            }
        }
    }
}
