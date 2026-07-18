use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
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
}

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
        .map(|value| value.replace('\\', "/"))
        .collect();
    let available: BTreeMap<_, _> = discovered
        .iter()
        .filter_map(|file| {
            normalize_logical(&selection.root, &file.path)
                .ok()
                .map(|path| (path, file.is_python))
        })
        .filter(|(path, _)| !excludes.contains(path.as_str()))
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

fn strip_root(root: &Utf8Path, path: &Utf8Path) -> Option<Utf8PathBuf> {
    if !cfg!(windows) {
        return path.strip_prefix(root).ok().map(Utf8Path::to_owned);
    }
    let root = root.as_str().replace('\\', "/");
    let root = root.trim_end_matches('/');
    let path = path.as_str().replace('\\', "/");
    if path.eq_ignore_ascii_case(root) {
        return Some(Utf8PathBuf::new());
    }
    let prefix = format!("{root}/");
    path.get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(&prefix))
        .and_then(|_| path.get(prefix.len()..))
        .map(Utf8PathBuf::from)
}

fn is_within(path: &Utf8Path, directory: &Utf8Path) -> bool {
    if cfg!(windows) {
        let path = path.as_str().replace('\\', "/").to_ascii_lowercase();
        let directory = directory.as_str().replace('\\', "/").to_ascii_lowercase();
        path == directory || path.starts_with(&format!("{directory}/"))
    } else {
        path == directory || path.starts_with(directory.as_str().to_owned() + "/")
    }
}

fn paths_equal(left: &Utf8Path, right: &Utf8Path) -> bool {
    if cfg!(windows) {
        left.as_str().eq_ignore_ascii_case(right.as_str())
    } else {
        left == right
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
        if let Some(previous) = merged.last_mut() {
            if range.start <= previous.end.saturating_add(1) {
                previous.end = previous.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    *ranges = merged;
}

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
