use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use camino::Utf8PathBuf;
use hoimin_core::{DiscoveredFile, Selection, logical_path_equality_key, normalize_logical_path};
use ignore::overrides::OverrideBuilder;
use ignore::{DirEntry, Walk, WalkBuilder};

use crate::{copy_policy::default_excluded, portable_path};

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
                write!(
                    formatter,
                    "target path cannot be represented portably: {path}"
                )
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
    discover_explicit_with_stats(selection, &DiscoveryStats::default())
}

fn discover_explicit_with_stats(
    selection: &Selection,
    stats: &DiscoveryStats,
) -> Result<Vec<DiscoveredFile>, FsTargetError> {
    let root = selection.root.as_std_path();
    let mut files = BTreeMap::<Utf8PathBuf, DiscoveredFile>::new();
    let scope = DiscoveryScope::new(selection).map(Arc::new);

    let excludes = build_overrides(root, &[], &selection.excludes)?;
    let mut normal = WalkBuilder::new(root);
    let normal_filter = ScopeFilter::new(root, scope.clone(), stats.clone());
    normal
        .require_git(false)
        .overrides(excludes)
        .filter_entry(move |entry| normal_filter.keeps(entry));
    collect(normal.build(), root, &mut files, stats)?;

    if !selection.includes.is_empty() {
        let includes = build_overrides(root, &selection.includes, &selection.excludes)?;
        let mut restored = WalkBuilder::new(root);
        let restored_filter = ScopeFilter::new(root, scope, stats.clone());
        restored
            .hidden(false)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .parents(false)
            .overrides(includes)
            .filter_entry(move |entry| restored_filter.keeps(entry));
        collect(restored.build(), root, &mut files, stats)?;
    }

    Ok(files.into_values().collect())
}

#[derive(Clone, Default)]
struct DiscoveryStats {
    #[cfg(test)]
    visited_entries: Arc<AtomicUsize>,
    #[cfg(test)]
    collected_records: Arc<AtomicUsize>,
    #[cfg(test)]
    scope_key_lookups: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct DiscoveryScope {
    sources: BTreeSet<String>,
    files: BTreeSet<String>,
    directories: BTreeSet<String>,
    native_files: BTreeSet<PathBuf>,
    native_directories: BTreeSet<PathBuf>,
}

impl DiscoveryScope {
    fn new(selection: &Selection) -> Option<Self> {
        if selection.sources.is_empty() && !selection.symbols.is_empty() {
            return None;
        }
        let normalized_sources = selection
            .sources
            .iter()
            .map(|path| normalize_logical_path(&selection.root, path))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if normalized_sources
            .iter()
            .any(|path| path.as_str().is_empty())
        {
            return None;
        }
        let normalized_files = selection
            .files
            .iter()
            .chain(selection.lines.iter().map(|line| &line.path))
            .map(|path| normalize_logical_path(&selection.root, path))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if normalized_files.is_empty() && normalized_sources.is_empty() {
            return None;
        }
        let files = normalized_files
            .iter()
            .map(|path| logical_path_equality_key(path).into_owned())
            .collect();
        let native_files = normalized_files
            .iter()
            .map(|path| path.as_std_path().to_owned())
            .collect();
        let mut directories = BTreeSet::new();
        let mut native_directories = BTreeSet::new();
        for file in normalized_files.iter().chain(&normalized_sources) {
            let mut directory = file.parent();
            while let Some(path) = directory {
                directories.insert(logical_path_equality_key(path).into_owned());
                native_directories.insert(path.as_std_path().to_owned());
                directory = path.parent();
            }
        }
        Some(Self {
            sources: normalized_sources
                .iter()
                .map(|path| logical_path_equality_key(path).into_owned())
                .collect(),
            files,
            directories,
            native_files,
            native_directories,
        })
    }

    fn in_source(&self, mut key: &str) -> bool {
        if self.sources.is_empty() {
            return false;
        }
        loop {
            if self.sources.contains(key) {
                return true;
            }
            let Some((parent, _)) = key.rsplit_once('/') else {
                return false;
            };
            key = parent;
        }
    }

    fn native_in_source(&self, native: &Path) -> bool {
        // A malformed child still belongs to a selected valid ancestor. Use the
        // normal platform key on UTF-8 ancestors, even when the child is not portable.
        !self.sources.is_empty()
            && native.ancestors().filter_map(Path::to_str).any(|path| {
                self.sources
                    .contains(logical_path_equality_key(camino::Utf8Path::new(path)).as_ref())
            })
    }

    fn keeps(&self, entry: &DirEntry, root: &Path, stats: &DiscoveryStats) -> bool {
        #[cfg(not(test))]
        let _ = stats;
        if entry.depth() == 0 {
            return true;
        }
        let Some(path) = portable_relative(entry.path(), root) else {
            let Ok(native) = entry.path().strip_prefix(root) else {
                return false;
            };
            if self.native_in_source(native) {
                return true;
            }
            return if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                self.native_directories.contains(native)
            } else {
                self.native_files.contains(native)
            };
        };
        #[cfg(test)]
        stats.scope_key_lookups.fetch_add(1, Ordering::Relaxed);
        let key = logical_path_equality_key(&path);
        if self.in_source(&key) {
            return true;
        }
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            self.directories.contains(key.as_ref())
        } else {
            self.files.contains(key.as_ref())
        }
    }
}

struct ScopeFilter {
    root: PathBuf,
    scope: Option<Arc<DiscoveryScope>>,
    stats: DiscoveryStats,
}

impl ScopeFilter {
    fn new(root: &Path, scope: Option<Arc<DiscoveryScope>>, stats: DiscoveryStats) -> Self {
        Self {
            root: root.to_owned(),
            scope,
            stats,
        }
    }

    fn keeps(&self, entry: &DirEntry) -> bool {
        #[cfg(test)]
        self.stats.visited_entries.fetch_add(1, Ordering::Relaxed);
        !default_excluded(entry)
            && self
                .scope
                .as_ref()
                .is_none_or(|scope| scope.keeps(entry, &self.root, &self.stats))
    }
}

fn portable_relative(path: &Path, root: &Path) -> Option<Utf8PathBuf> {
    let relative = path.strip_prefix(root).ok()?.to_str()?;
    let relative = portable_path::from_native(relative).ok()?;
    Some(Utf8PathBuf::from(relative.into_owned()))
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
    stats: &DiscoveryStats,
) -> Result<(), FsTargetError> {
    #[cfg(not(test))]
    let _ = stats;
    for entry in builder {
        let entry = entry.map_err(FsTargetError::Walk)?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            #[cfg(test)]
            stats.collected_records.fetch_add(1, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use hoimin_core::{LineRange, LineSelection};
    use std::fs;

    #[test]
    fn source_subtrees_prune_unrelated_descendants_and_records() {
        for count in [0, 1_000, 5_000] {
            let temp = tempfile::tempdir().unwrap();
            fs::create_dir_all(temp.path().join("selected")).unwrap();
            fs::create_dir_all(temp.path().join("unrelated")).unwrap();
            fs::write(temp.path().join("selected/app.py"), "value = 1 + 2\n").unwrap();
            fs::write(temp.path().join("selected/data.bin"), b"fixture").unwrap();
            for index in 0..count {
                fs::write(
                    temp.path().join("unrelated").join(format!("{index}.txt")),
                    b"x",
                )
                .unwrap();
            }
            let selection = Selection {
                root: Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
                sources: vec!["selected".into()],
                ..Selection::default()
            };
            let broad = Selection {
                sources: vec![".".into()],
                ..selection.clone()
            };
            let broad_stats = DiscoveryStats::default();
            let broad_files = discover_explicit_with_stats(&broad, &broad_stats).unwrap();
            assert_eq!(broad_files.len(), count + 2);
            let stats = DiscoveryStats::default();
            let files = discover_explicit_with_stats(&selection, &stats).unwrap();
            assert_eq!(
                hoimin_core::resolve_explicit(&selection, &files).unwrap(),
                hoimin_core::resolve_explicit(&selection, &broad_files).unwrap()
            );
            eprintln!(
                "unrelated={count} visited={} records={} broad_visited={} broad_records={}",
                stats.visited_entries.load(Ordering::Relaxed),
                stats.collected_records.load(Ordering::Relaxed),
                broad_stats.visited_entries.load(Ordering::Relaxed),
                broad_stats.collected_records.load(Ordering::Relaxed)
            );
            assert_eq!(files.len(), 2, "unrelated records must not be retained");
            assert!(
                stats.visited_entries.load(Ordering::Relaxed) <= 5,
                "unrelated descendants must not be visited"
            );
            assert_eq!(stats.collected_records.load(Ordering::Relaxed), 2);
        }
    }

    #[test]
    fn source_include_walk_prunes_other_ignored_subtrees() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir_all(temp.path().join("ignored/selected")).unwrap();
        fs::create_dir_all(temp.path().join("ignored/unrelated")).unwrap();
        fs::write(temp.path().join("ignored/selected/app.py"), "x = 1\n").unwrap();
        for index in 0..1_000 {
            fs::write(
                temp.path()
                    .join("ignored/unrelated")
                    .join(format!("{index}.txt")),
                b"x",
            )
            .unwrap();
        }
        let selection = Selection {
            root: Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap(),
            sources: vec!["ignored/selected".into()],
            includes: vec!["ignored/**".into()],
            ..Selection::default()
        };
        let stats = DiscoveryStats::default();
        assert_eq!(
            discover_explicit_with_stats(&selection, &stats).unwrap(),
            [DiscoveredFile::python("ignored/selected/app.py")]
        );
        assert_eq!(stats.collected_records.load(Ordering::Relaxed), 1);
        assert!(stats.visited_entries.load(Ordering::Relaxed) < 12);
        let broad = Selection {
            sources: vec![".".into()],
            ..selection
        };
        assert_eq!(discover_explicit(&broad).unwrap().len(), 1_001);
    }

    #[test]
    fn exact_file_prunes_unrelated_subtrees_and_records() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("calc.py"), "value = 1 + 2\n").unwrap();
        for index in 0..1_000 {
            let path = temp.path().join("unrelated").join(format!("{index}.txt"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, []).unwrap();
        }
        let stats = DiscoveryStats::default();
        let selection = Selection {
            root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
            files: vec!["calc.py".into()],
            ..Selection::default()
        };

        let files = discover_explicit_with_stats(&selection, &stats).unwrap();

        assert_eq!(files, [DiscoveredFile::python("calc.py")]);
        assert!(stats.visited_entries.load(Ordering::Relaxed) <= 4);
        assert_eq!(stats.collected_records.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn exact_lines_keep_only_their_shared_ancestor_paths() {
        let temp = tempfile::tempdir().unwrap();
        for path in ["pkg/a.py", "pkg/nested/b.py", "pkg/unrelated/c.py"] {
            let path = temp.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "value = 1\n").unwrap();
        }
        let stats = DiscoveryStats::default();
        let selection = Selection {
            root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
            lines: vec![
                LineSelection {
                    path: "pkg/a.py".into(),
                    range: LineRange { start: 1, end: 1 },
                },
                LineSelection {
                    path: "pkg/nested/b.py".into(),
                    range: LineRange { start: 1, end: 1 },
                },
            ],
            ..Selection::default()
        };

        let files = discover_explicit_with_stats(&selection, &stats).unwrap();

        assert_eq!(
            files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["pkg/a.py", "pkg/nested/b.py"]
        );
        assert_eq!(stats.collected_records.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn many_exact_files_use_one_scope_key_lookup_per_visited_entry() {
        let temp = tempfile::tempdir().unwrap();
        let mut selected = Vec::new();
        for index in 0..256 {
            let relative = format!("pkg/{index:03}.py");
            let path = temp.path().join(&relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "value = 1\n").unwrap();
            selected.push(relative.into());
        }
        let stats = DiscoveryStats::default();
        let selection = Selection {
            root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
            files: selected,
            ..Selection::default()
        };

        let files = discover_explicit_with_stats(&selection, &stats).unwrap();

        assert_eq!(files.len(), 256);
        assert_eq!(stats.collected_records.load(Ordering::Relaxed), 256);
        assert_eq!(
            stats.scope_key_lookups.load(Ordering::Relaxed),
            stats.visited_entries.load(Ordering::Relaxed)
        );
    }

    #[test]
    #[ignore = "release performance evidence"]
    fn measure_exact_file_discovery_scaling() {
        for count in [2_000, 4_000, 8_000] {
            let temp = tempfile::tempdir().unwrap();
            fs::write(temp.path().join("calc.py"), "value = 1 + 2\n").unwrap();
            for index in 0..count {
                let path = temp.path().join("unrelated").join(format!("{index}.txt"));
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, []).unwrap();
            }
            let selection = Selection {
                root: Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap(),
                files: vec!["calc.py".into()],
                ..Selection::default()
            };
            let mut samples = Vec::new();
            for _ in 0..5 {
                let started = std::time::Instant::now();
                let files = discover_explicit(&selection).unwrap();
                samples.push(started.elapsed());
                assert_eq!(files, [DiscoveredFile::python("calc.py")]);
            }
            samples.sort_unstable();
            println!("unrelated={count} median={:?}", samples[2]);
        }
    }
}
