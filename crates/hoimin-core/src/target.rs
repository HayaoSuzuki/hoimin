use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use thiserror::Error;

use crate::{LineRange, TargetSlice, contract_ensure};

/// Keeps `2 * context` within a signed 32-bit C long in older Git versions.
pub const MAX_CHANGED_CONTEXT: u32 = (i32::MAX as u32) / 2;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    pub root: Utf8PathBuf,
    pub sources: Vec<Utf8PathBuf>,
    pub files: Vec<Utf8PathBuf>,
    pub lines: Vec<LineSelection>,
    pub symbols: Vec<SymbolSelection>,
    pub changed: bool,
    #[serde(default)]
    pub changed_context: u32,
    pub diff_base: Option<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LineSelection {
    pub path: Utf8PathBuf,
    pub range: LineRange,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LineSelectionIndex {
    ranges: BTreeMap<String, Vec<LineRange>>,
}

impl LineSelectionIndex {
    #[must_use]
    pub fn new(root: &Utf8Path, selections: &[LineSelection]) -> Self {
        let mut ranges = BTreeMap::<String, Vec<LineRange>>::new();
        for selection in selections {
            if selection.range.start > selection.range.end {
                continue;
            }
            if let Ok(path) = normalize_logical(root, &selection.path) {
                ranges
                    .entry(path_equality_key(&path).into_owned())
                    .or_default()
                    .push(selection.range);
            }
        }
        for ranges in ranges.values_mut() {
            normalize_ranges(ranges);
        }
        Self { ranges }
    }

    #[must_use]
    pub fn contains(&self, path: &Utf8Path, line: u32) -> bool {
        if self.ranges.is_empty() {
            return false;
        }
        let path = path_equality_key(path);
        let Some(ranges) = self.ranges.get(path.as_ref()) else {
            return false;
        };
        let end = ranges.partition_point(|range| range.start <= line);
        end.checked_sub(1)
            .is_some_and(|index| line <= ranges[index].end)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolSelection {
    pub module: String,
    pub qualname: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredFile {
    pub path: Utf8PathBuf,
    pub is_python: bool,
}

impl DiscoveredFile {
    pub fn python(path: impl Into<Utf8PathBuf>) -> Self {
        Self {
            path: path.into(),
            is_python: true,
        }
    }

    pub fn regular(path: impl Into<Utf8PathBuf>) -> Self {
        Self {
            path: path.into(),
            is_python: false,
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TargetError {
    #[error("target path escapes root: {0}")]
    PathOutsideRoot(Utf8PathBuf),
    #[error("file is outside all source roots: {0}")]
    FileOutsideSource(Utf8PathBuf),
    #[error("invalid line range: {0:?}")]
    InvalidLineRange(LineRange),
    #[error("target is missing or is not a Python file: {0}")]
    MissingOrNonPythonFile(Utf8PathBuf),
    #[error("symbol module cannot be resolved: {0}")]
    SymbolModuleNotFound(String),
    #[error("--changed requires a Git repository")]
    GitRepositoryRequired,
    #[error("Git target resolution failed: {0}")]
    GitFailed(String),
    #[error("target discovery failed: {0}")]
    DiscoveryFailed(String),
}

/// Resolves selectors against an already policy-filtered discovery inventory.
///
/// Callers must apply include/exclude globs, ignore rules and built-in exclusions
/// during discovery. This pure resolver does not reinterpret those patterns.
///
/// # Errors
///
/// Returns [`TargetError`] when a requested path, line range, or symbol cannot
/// be resolved to a discovered Python file within the configured roots.
pub fn resolve_explicit(
    selection: &Selection,
    discovered: &[DiscoveredFile],
) -> Result<Vec<TargetSlice>, TargetError> {
    let sources = selection
        .sources
        .iter()
        .map(|path| normalize_logical(&selection.root, path))
        .collect::<Result<Vec<_>, _>>()?;
    let available: BTreeMap<_, _> = discovered
        .iter()
        .filter_map(|file| {
            normalize_logical(&selection.root, &file.path)
                .ok()
                .map(|path| (path, file.is_python))
        })
        .collect();
    let available_python: BTreeMap<String, Utf8PathBuf> = available
        .iter()
        .filter(|(_, is_python)| **is_python)
        .fold(BTreeMap::new(), |mut indexed, (path, _)| {
            indexed
                .entry(path_equality_key(path).into_owned())
                .or_insert_with(|| path.clone());
            indexed
        });

    let mut targets = BTreeMap::<Utf8PathBuf, TargetSlice>::new();
    for (path, is_python) in &available {
        if *is_python && sources.iter().any(|source| is_within(path, source)) {
            targets.insert(path.clone(), whole_file(path.clone()));
        }
    }

    for raw_path in &selection.files {
        let requested = checked_explicit_path(selection, &sources, raw_path)?;
        let path = require_python(&available_python, &requested)?;
        targets.insert(path.clone(), whole_file(path));
    }

    for line in &selection.lines {
        if line.range.start == 0 || line.range.start > line.range.end {
            return Err(TargetError::InvalidLineRange(line.range));
        }
        let requested = checked_explicit_path(selection, &sources, &line.path)?;
        let path = require_python(&available_python, &requested)?;
        let entry = targets.entry(path.clone()).or_insert_with(|| TargetSlice {
            path,
            lines: Vec::new(),
            symbols: Vec::new(),
        });
        entry.lines.push(line.range);
    }

    for symbol in &selection.symbols {
        let path = resolve_symbol_path(symbol, &sources, &available_python)?;
        let entry = targets.entry(path.clone()).or_insert_with(|| TargetSlice {
            path,
            lines: Vec::new(),
            symbols: Vec::new(),
        });
        entry.symbols.push(symbol.qualname.clone());
    }

    for target in targets.values_mut() {
        if !target.lines.is_empty() {
            normalize_ranges(&mut target.lines);
        }
        if !target.symbols.is_empty() {
            #[cfg(test)]
            SYMBOL_NORMALIZATIONS.with(|count| count.set(count.get() + 1));
            target.symbols.sort();
            target.symbols.dedup();
        }
    }

    let targets: Vec<_> = targets.into_values().collect();
    contract_ensure!(
        "target.resolve.post",
        targets_are_normalized(&targets),
        &targets
    );
    Ok(targets)
}

fn checked_explicit_path(
    selection: &Selection,
    sources: &[Utf8PathBuf],
    raw_path: &Utf8Path,
) -> Result<Utf8PathBuf, TargetError> {
    let path = normalize_logical(&selection.root, raw_path)?;
    if !sources.is_empty() && !sources.iter().any(|source| is_within(&path, source)) {
        return Err(TargetError::FileOutsideSource(raw_path.to_owned()));
    }
    Ok(path)
}

fn require_python(
    available_python: &BTreeMap<String, Utf8PathBuf>,
    path: &Utf8Path,
) -> Result<Utf8PathBuf, TargetError> {
    available_python
        .get(path_equality_key(path).as_ref())
        .cloned()
        .ok_or_else(|| TargetError::MissingOrNonPythonFile(path.to_owned()))
}

fn whole_file(path: Utf8PathBuf) -> TargetSlice {
    TargetSlice {
        path,
        lines: Vec::new(),
        symbols: Vec::new(),
    }
}

fn normalize_logical(root: &Utf8Path, path: &Utf8Path) -> Result<Utf8PathBuf, TargetError> {
    let original = path.to_owned();
    let relative = if path.is_absolute() {
        strip_root(root, path).ok_or_else(|| TargetError::PathOutsideRoot(original.clone()))?
    } else {
        path.to_owned()
    };
    let mut parts = Vec::<&str>::new();
    for component in relative.components() {
        match component {
            Utf8Component::CurDir => {}
            Utf8Component::Normal(value) => parts.push(value),
            Utf8Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(TargetError::PathOutsideRoot(original));
                }
            }
            Utf8Component::Prefix(_) | Utf8Component::RootDir => {
                return Err(TargetError::PathOutsideRoot(original));
            }
        }
    }
    Ok(Utf8PathBuf::from(parts.join("/")))
}

/// Converts a user-supplied target path to the root-relative logical form used
/// by resolved targets and mutation candidates.
///
/// # Errors
///
/// Returns [`TargetError::PathOutsideRoot`] when an absolute path is outside
/// `root` or relative parent components escape it.
pub fn normalize_logical_path(
    root: &Utf8Path,
    path: &Utf8Path,
) -> Result<Utf8PathBuf, TargetError> {
    normalize_logical(root, path)
}

fn strip_root(root: &Utf8Path, path: &Utf8Path) -> Option<Utf8PathBuf> {
    if !cfg!(windows) {
        return path.strip_prefix(root).ok().map(Utf8Path::to_owned);
    }
    let root = root.as_str().replace('\\', "/");
    let path = path.as_str().replace('\\', "/");
    let root_parts: Vec<_> = root.split('/').filter(|part| !part.is_empty()).collect();
    let path_parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
    if path_parts.len() < root_parts.len()
        || !root_parts
            .iter()
            .zip(&path_parts)
            .all(|(root, path)| windows_case_key(root) == windows_case_key(path))
    {
        return None;
    }
    Some(Utf8PathBuf::from(path_parts[root_parts.len()..].join("/")))
}

fn is_within(path: &Utf8Path, directory: &Utf8Path) -> bool {
    if directory.as_str().is_empty() {
        return true;
    }
    if cfg!(windows) {
        let path = path_key(path.as_str());
        let directory = path_key(directory.as_str());
        path == directory || path.starts_with(&format!("{directory}/"))
    } else {
        path == directory || path.starts_with(directory.as_str().to_owned() + "/")
    }
}

fn paths_equal(left: &Utf8Path, right: &Utf8Path) -> bool {
    path_equality_key(left) == path_equality_key(right)
}

#[cfg(test)]
thread_local! {
    static PATH_KEY_DERIVATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn path_equality_key(path: &Utf8Path) -> Cow<'_, str> {
    #[cfg(test)]
    PATH_KEY_DERIVATIONS.with(|count| count.set(count.get() + 1));
    if cfg!(windows) {
        Cow::Owned(path_key(path.as_str()))
    } else {
        Cow::Borrowed(path.as_str())
    }
}

/// Compares normalized logical paths using the platform rules applied during
/// target resolution.
#[must_use]
pub fn logical_paths_equal(left: &Utf8Path, right: &Utf8Path) -> bool {
    paths_equal(left, right)
}

/// Returns the platform-specific comparison key used for normalized logical
/// target paths.
#[must_use]
pub fn logical_path_equality_key(path: &Utf8Path) -> Cow<'_, str> {
    path_equality_key(path)
}

fn path_key(value: &str) -> String {
    let value = value.replace('\\', "/");
    if cfg!(windows) {
        windows_case_key(&value)
    } else {
        value
    }
}

fn windows_case_key(value: &str) -> String {
    value.chars().map(simple_uppercase).collect()
}

fn simple_uppercase(value: char) -> char {
    let mut uppercase = value.to_uppercase();
    let first = uppercase.next().unwrap_or(value);
    if uppercase.next().is_some() {
        value
    } else {
        first
    }
}

fn resolve_symbol_path(
    symbol: &SymbolSelection,
    sources: &[Utf8PathBuf],
    available_python: &BTreeMap<String, Utf8PathBuf>,
) -> Result<Utf8PathBuf, TargetError> {
    let module = symbol.module.replace('.', "/");
    for source in sources {
        for suffix in [format!("{module}.py"), format!("{module}/__init__.py")] {
            let path = source.join(suffix);
            if let Ok(path) = require_python(available_python, &path) {
                return Ok(path);
            }
        }
    }
    Err(TargetError::SymbolModuleNotFound(symbol.module.clone()))
}

fn normalize_ranges(ranges: &mut Vec<LineRange>) {
    #[cfg(test)]
    RANGE_NORMALIZATIONS.with(|count| count.set(count.get() + 1));
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<LineRange> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end.saturating_add(1)
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    *ranges = merged;
}

#[cfg(test)]
thread_local! {
    static RANGE_NORMALIZATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static SYMBOL_NORMALIZATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[must_use]
pub fn normalize_changed(
    changed: BTreeMap<Utf8PathBuf, Vec<LineRange>>,
) -> BTreeMap<Utf8PathBuf, Vec<LineRange>> {
    let changed = changed
        .into_iter()
        .filter_map(|(path, mut ranges)| {
            ranges.retain(|range| range.start > 0 && range.start <= range.end);
            normalize_ranges(&mut ranges);
            (!ranges.is_empty()).then_some((path, ranges))
        })
        .collect();
    contract_ensure!(
        "target.changed.post",
        changed_is_normalized(&changed),
        &changed
    );
    changed
}

#[must_use]
pub fn intersect_changed(
    explicit: &[TargetSlice],
    changed: &BTreeMap<Utf8PathBuf, Vec<LineRange>>,
) -> Vec<TargetSlice> {
    let changed = normalize_changed(changed.clone());
    let targets: Vec<TargetSlice> = if explicit.is_empty() {
        changed
            .into_iter()
            .map(|(path, lines)| TargetSlice {
                path,
                lines,
                symbols: Vec::new(),
            })
            .collect()
    } else {
        let changed = changed_path_index(changed);
        explicit
            .iter()
            .filter_map(|target| {
                let key = path_equality_key(&target.path);
                let changed_lines = changed.get(key.as_ref())?;
                let mut lines = if target.lines.is_empty() {
                    changed_lines.clone()
                } else {
                    target
                        .lines
                        .iter()
                        .flat_map(|explicit_range| {
                            changed_lines.iter().filter_map(move |changed_range| {
                                let start = explicit_range.start.max(changed_range.start);
                                let end = explicit_range.end.min(changed_range.end);
                                (start <= end).then_some(LineRange { start, end })
                            })
                        })
                        .collect()
                };
                normalize_ranges(&mut lines);
                (!lines.is_empty()).then(|| TargetSlice {
                    path: target.path.clone(),
                    lines,
                    symbols: target.symbols.clone(),
                })
            })
            .collect()
    };
    contract_ensure!(
        "target.changed.post",
        targets_are_normalized(&targets),
        &targets
    );
    targets
}

fn changed_path_index(
    changed: BTreeMap<Utf8PathBuf, Vec<LineRange>>,
) -> BTreeMap<String, Vec<LineRange>> {
    let mut indexed = BTreeMap::<String, Vec<LineRange>>::new();
    for (path, ranges) in changed {
        indexed
            .entry(path_equality_key(&path).into_owned())
            .or_default()
            .extend(ranges);
    }
    for ranges in indexed.values_mut() {
        normalize_ranges(ranges);
    }
    indexed
}

#[must_use]
pub fn changed_is_normalized(changed: &BTreeMap<Utf8PathBuf, Vec<LineRange>>) -> bool {
    changed.values().all(|ranges| {
        !ranges.is_empty()
            && ranges
                .iter()
                .all(|range| range.start > 0 && range.start <= range.end)
            && ranges
                .windows(2)
                .all(|pair| pair[0].end.saturating_add(1) < pair[1].start)
    })
}

#[must_use]
pub fn targets_are_normalized(targets: &[TargetSlice]) -> bool {
    targets.windows(2).all(|pair| pair[0].path < pair[1].path)
        && targets.iter().all(|target| {
            target
                .lines
                .windows(2)
                .all(|pair| pair[0].end < pair[1].start)
                && target
                    .lines
                    .iter()
                    .all(|range| range.start > 0 && range.start <= range.end)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    fn explicit_file_lookups_do_not_scan_every_discovered_file() {
        let count = 128;
        let discovered = (0..count)
            .map(|index| DiscoveredFile::python(format!("src/file{index:03}.py")))
            .collect::<Vec<_>>();
        let selection = Selection {
            root: "/project".into(),
            files: discovered.iter().map(|file| file.path.clone()).collect(),
            ..Selection::default()
        };

        PATH_KEY_DERIVATIONS.with(|checks| checks.set(0));
        let targets = resolve_explicit(&selection, &discovered).unwrap();
        let key_derivations = PATH_KEY_DERIVATIONS.with(std::cell::Cell::get);

        assert_eq!(targets.len(), count);
        assert_eq!(key_derivations, count * 2);
    }

    #[test]
    fn line_and_symbol_lookups_share_the_discovered_file_index() {
        let count = 128;
        let discovered = (0..count)
            .map(|index| DiscoveredFile::python(format!("src/file{index:03}.py")))
            .collect::<Vec<_>>();
        let selection = Selection {
            root: "/project".into(),
            sources: vec!["src".into()],
            lines: vec![LineSelection {
                path: "src/file127.py".into(),
                range: LineRange { start: 1, end: 1 },
            }],
            symbols: vec![SymbolSelection {
                module: "file126".into(),
                qualname: "run".into(),
            }],
            ..Selection::default()
        };

        PATH_KEY_DERIVATIONS.with(|checks| checks.set(0));
        let targets = resolve_explicit(&selection, &discovered).unwrap();
        let key_derivations = PATH_KEY_DERIVATIONS.with(std::cell::Cell::get);

        assert_eq!(targets.len(), count);
        assert_eq!(key_derivations, count + 2);
    }

    #[test]
    fn missing_symbol_lookup_does_not_scan_discovered_files() {
        let count = 128;
        let discovered = (0..count)
            .map(|index| DiscoveredFile::python(format!("src/file{index:03}.py")))
            .collect::<Vec<_>>();
        let selection = Selection {
            root: "/project".into(),
            sources: vec!["src".into()],
            symbols: vec![SymbolSelection {
                module: "missing".into(),
                qualname: "run".into(),
            }],
            ..Selection::default()
        };

        PATH_KEY_DERIVATIONS.with(|checks| checks.set(0));
        let error = resolve_explicit(&selection, &discovered).unwrap_err();
        let key_derivations = PATH_KEY_DERIVATIONS.with(std::cell::Cell::get);

        assert_eq!(error, TargetError::SymbolModuleNotFound("missing".into()));
        assert_eq!(key_derivations, count + 2);
    }

    #[cfg(windows)]
    #[test]
    fn python_index_ignores_a_case_equivalent_non_python_entry() {
        let selection = Selection {
            root: "C:/project".into(),
            files: vec!["src/case.py".into()],
            ..Selection::default()
        };
        let discovered = [
            DiscoveredFile::regular("SRC/CASE.PY"),
            DiscoveredFile::python("src/case.py"),
        ];

        let targets = resolve_explicit(&selection, &discovered).unwrap();

        assert_eq!(targets, vec![whole_file("src/case.py".into())]);
    }

    fn sparse_selection(count: u32) -> Selection {
        Selection {
            root: "/project".into(),
            lines: (0..count)
                .rev()
                .map(|index| LineSelection {
                    path: "calc.py".into(),
                    range: LineRange {
                        start: index * 2 + 1,
                        end: index * 2 + 1,
                    },
                })
                .collect(),
            symbols: (0..count)
                .rev()
                .flat_map(|index| {
                    [
                        SymbolSelection {
                            module: "calc".into(),
                            qualname: format!("symbol{index:05}"),
                        },
                        SymbolSelection {
                            module: "calc".into(),
                            qualname: format!("symbol{index:05}"),
                        },
                    ]
                })
                .collect(),
            sources: vec![Utf8PathBuf::new()],
            ..Selection::default()
        }
    }

    #[test]
    fn explicit_ranges_are_normalized_once_per_file() {
        let count = 128;
        RANGE_NORMALIZATIONS.with(|calls| calls.set(0));

        let targets = resolve_explicit(
            &sparse_selection(count),
            &[DiscoveredFile::python("calc.py")],
        )
        .unwrap();
        let normalizations = RANGE_NORMALIZATIONS.with(std::cell::Cell::get);

        assert_eq!(normalizations, 1);
        assert_eq!(targets[0].lines.len(), count as usize);
        assert_eq!(targets[0].symbols.len(), count as usize);
        assert!(targets_are_normalized(&targets));
    }

    #[test]
    fn explicit_symbols_are_normalized_once_per_file() {
        let count = 128;
        SYMBOL_NORMALIZATIONS.with(|calls| calls.set(0));

        let targets = resolve_explicit(
            &sparse_selection(count),
            &[DiscoveredFile::python("calc.py")],
        )
        .unwrap();
        let normalizations = SYMBOL_NORMALIZATIONS.with(std::cell::Cell::get);

        assert_eq!(normalizations, 1);
        assert_eq!(targets[0].symbols.len(), count as usize);
    }

    #[test]
    fn explicit_groups_are_normalized_once_for_each_file() {
        let mut selection = sparse_selection(4);
        selection.lines.extend((0..4).map(|index| LineSelection {
            path: "other.py".into(),
            range: LineRange {
                start: index * 2 + 1,
                end: index * 2 + 1,
            },
        }));
        selection
            .symbols
            .extend((0..4).map(|index| SymbolSelection {
                module: "other".into(),
                qualname: format!("other{index}"),
            }));
        RANGE_NORMALIZATIONS.with(|calls| calls.set(0));
        SYMBOL_NORMALIZATIONS.with(|calls| calls.set(0));

        let targets = resolve_explicit(
            &selection,
            &[
                DiscoveredFile::python("calc.py"),
                DiscoveredFile::python("other.py"),
            ],
        )
        .unwrap();

        assert_eq!(targets.len(), 2);
        assert_eq!(RANGE_NORMALIZATIONS.with(std::cell::Cell::get), 2);
        assert_eq!(SYMBOL_NORMALIZATIONS.with(std::cell::Cell::get), 2);
    }

    #[test]
    fn explicit_range_normalization_preserves_boundaries() {
        let selection = Selection {
            root: "/project".into(),
            lines: vec![
                LineSelection {
                    path: "calc.py".into(),
                    range: LineRange {
                        start: u32::MAX,
                        end: u32::MAX,
                    },
                },
                LineSelection {
                    path: "calc.py".into(),
                    range: LineRange { start: 4, end: 5 },
                },
                LineSelection {
                    path: "calc.py".into(),
                    range: LineRange { start: 3, end: 4 },
                },
                LineSelection {
                    path: "calc.py".into(),
                    range: LineRange { start: 3, end: 4 },
                },
            ],
            ..Selection::default()
        };

        let targets = resolve_explicit(&selection, &[DiscoveredFile::python("calc.py")]).unwrap();

        assert_eq!(
            targets[0].lines,
            vec![
                LineRange { start: 3, end: 5 },
                LineRange {
                    start: u32::MAX,
                    end: u32::MAX,
                },
            ]
        );
    }

    #[test]
    #[ignore = "release performance evidence"]
    fn measure_explicit_file_lookup_scaling() {
        for count in [2_000, 4_000, 8_000] {
            let discovered = (0..count)
                .map(|index| DiscoveredFile::python(format!("src/file{index:05}.py")))
                .collect::<Vec<_>>();
            let selection = Selection {
                root: "/project".into(),
                files: discovered.iter().map(|file| file.path.clone()).collect(),
                ..Selection::default()
            };
            let mut samples = Vec::new();
            for _ in 0..5 {
                let started = Instant::now();
                let targets = black_box(resolve_explicit(
                    black_box(&selection),
                    black_box(&discovered),
                ))
                .unwrap();
                assert_eq!(targets.len(), count);
                samples.push(started.elapsed());
            }
            samples.sort_unstable();
            println!("files={count} selectors={count} median={:?}", samples[2]);
        }
    }

    #[test]
    #[ignore = "release performance evidence"]
    fn measure_explicit_range_scaling() {
        for count in [2_000, 4_000, 8_000] {
            let selection = sparse_selection(count);
            let mut samples = Vec::new();
            for _ in 0..5 {
                let started = Instant::now();
                let targets = black_box(resolve_explicit(
                    black_box(&selection),
                    black_box(&[DiscoveredFile::python("calc.py")]),
                ))
                .unwrap();
                assert_eq!(targets[0].lines.len(), count as usize);
                samples.push(started.elapsed());
            }
            samples.sort_unstable();
            println!("sparse_ranges={count} median={:?}", samples[2]);
        }
    }
}
