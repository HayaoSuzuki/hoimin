use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

use crate::{LineRange, TargetSlice, contract_ensure};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    pub root: Utf8PathBuf,
    pub sources: Vec<Utf8PathBuf>,
    pub files: Vec<Utf8PathBuf>,
    pub lines: Vec<LineSelection>,
    pub symbols: Vec<SymbolSelection>,
    pub changed: bool,
    pub diff_base: Option<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LineSelection {
    pub path: Utf8PathBuf,
    pub range: LineRange,
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
    let excludes: BTreeSet<_> = selection
        .excludes
        .iter()
        .map(|value| path_key(value))
        .collect();
    let available: BTreeMap<_, _> = discovered
        .iter()
        .filter_map(|file| {
            normalize_logical(&selection.root, &file.path)
                .ok()
                .map(|path| (path, file.is_python))
        })
        .filter(|(path, _)| !excludes.contains(&path_key(path.as_str())))
        .collect();

    let mut targets = BTreeMap::<Utf8PathBuf, TargetSlice>::new();
    for (path, is_python) in &available {
        if *is_python && sources.iter().any(|source| is_within(path, source)) {
            targets.insert(path.clone(), whole_file(path.clone()));
        }
    }

    for line in &selection.lines {
        if line.range.start == 0 || line.range.start > line.range.end {
            return Err(TargetError::InvalidLineRange(line.range));
        }
        let requested = checked_explicit_path(selection, &sources, &line.path)?;
        let path = require_python(&available, &requested)?;
        let entry = targets.entry(path.clone()).or_insert_with(|| TargetSlice {
            path,
            lines: Vec::new(),
            symbols: Vec::new(),
        });
        entry.lines.push(line.range);
        entry.symbols.clear();
        normalize_ranges(&mut entry.lines);
    }

    for symbol in &selection.symbols {
        let path = resolve_symbol_path(symbol, &sources, &available)?;
        let entry = targets.entry(path.clone()).or_insert_with(|| TargetSlice {
            path,
            lines: Vec::new(),
            symbols: Vec::new(),
        });
        entry.symbols.push(symbol.qualname.clone());
        entry.symbols.sort();
        entry.symbols.dedup();
    }

    for raw_path in &selection.files {
        let requested = checked_explicit_path(selection, &sources, raw_path)?;
        let path = require_python(&available, &requested)?;
        targets.insert(path.clone(), whole_file(path));
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
    available: &BTreeMap<Utf8PathBuf, bool>,
    path: &Utf8Path,
) -> Result<Utf8PathBuf, TargetError> {
    available
        .iter()
        .find(|(candidate, is_python)| **is_python && paths_equal(candidate, path))
        .map(|(candidate, _)| candidate.clone())
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

fn path_equality_key(path: &Utf8Path) -> Cow<'_, str> {
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
    available: &BTreeMap<Utf8PathBuf, bool>,
) -> Result<Utf8PathBuf, TargetError> {
    let module = symbol.module.replace('.', "/");
    for source in sources {
        for suffix in [format!("{module}.py"), format!("{module}/__init__.py")] {
            let path = source.join(suffix);
            if let Ok(path) = require_python(available, &path) {
                return Ok(path);
            }
        }
    }
    Err(TargetError::SymbolModuleNotFound(symbol.module.clone()))
}

fn normalize_ranges(ranges: &mut Vec<LineRange>) {
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
