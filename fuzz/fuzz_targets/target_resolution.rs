#![no_main]

use std::collections::BTreeMap;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use hoimin_core::{
    DiscoveredFile, LineRange, LineSelection, LineSelectionIndex, Selection, SymbolSelection,
    TargetSlice, intersect_changed, normalize_changed, normalize_logical_path, resolve_explicit,
};
use libfuzzer_sys::fuzz_target;

fn reference_normalize(root: &Utf8Path, path: &Utf8Path) -> Option<Utf8PathBuf> {
    let relative = if path.is_absolute() {
        #[cfg(not(windows))]
        {
            path.strip_prefix(root).ok()?.to_owned()
        }
        #[cfg(windows)]
        {
            let root = root.as_str().replace('\\', "/");
            let path = path.as_str().replace('\\', "/");
            let root_parts = root.split('/').filter(|part| !part.is_empty());
            let mut path_parts = path.split('/').filter(|part| !part.is_empty());
            for root_part in root_parts {
                let path_part = path_parts.next()?;
                if !root_part.eq_ignore_ascii_case(path_part) {
                    return None;
                }
            }
            Utf8PathBuf::from(path_parts.collect::<Vec<_>>().join("/"))
        }
    } else {
        path.to_owned()
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Utf8Component::CurDir => {}
            Utf8Component::Normal(value) => parts.push(value),
            Utf8Component::ParentDir => {
                parts.pop()?;
            }
            Utf8Component::Prefix(_) | Utf8Component::RootDir => return None,
        }
    }
    Some(Utf8PathBuf::from(parts.join("/")))
}

fn reference_ranges(mut ranges: Vec<LineRange>) -> Vec<LineRange> {
    ranges.retain(|range| range.start > 0 && range.start <= range.end);
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<LineRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end.saturating_add(1)
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn reference_targets(selection: &Selection, discovered: &[DiscoveredFile]) -> Vec<TargetSlice> {
    let mut targets = BTreeMap::<Utf8PathBuf, TargetSlice>::new();
    for file in discovered {
        if file.is_python && file.path.as_str().starts_with("src/") {
            targets.insert(
                file.path.clone(),
                TargetSlice {
                    path: file.path.clone(),
                    lines: Vec::new(),
                    symbols: Vec::new(),
                },
            );
        }
    }
    for path in &selection.files {
        let file = discovered
            .iter()
            .find(|file| file.is_python && file.path == *path)
            .unwrap();
        targets.insert(
            file.path.clone(),
            TargetSlice {
                path: file.path.clone(),
                lines: Vec::new(),
                symbols: Vec::new(),
            },
        );
    }
    for line in &selection.lines {
        targets.get_mut(&line.path).unwrap().lines.push(line.range);
    }
    for symbol in &selection.symbols {
        let path = Utf8PathBuf::from(format!("src/{}.py", symbol.module));
        targets
            .get_mut(&path)
            .unwrap()
            .symbols
            .push(symbol.qualname.clone());
    }
    for target in targets.values_mut() {
        target.lines = reference_ranges(std::mem::take(&mut target.lines));
        target.symbols.sort();
        target.symbols.dedup();
    }
    targets.into_values().collect()
}

fn reference_changed(
    changed: &BTreeMap<Utf8PathBuf, Vec<LineRange>>,
) -> BTreeMap<Utf8PathBuf, Vec<LineRange>> {
    changed
        .iter()
        .filter_map(|(path, ranges)| {
            let ranges = reference_ranges(ranges.clone());
            (!ranges.is_empty()).then_some((path.clone(), ranges))
        })
        .collect()
}

fn reference_intersection(
    explicit: &[TargetSlice],
    changed: &BTreeMap<Utf8PathBuf, Vec<LineRange>>,
) -> Vec<TargetSlice> {
    if explicit.is_empty() {
        return changed
            .iter()
            .map(|(path, lines)| TargetSlice {
                path: path.clone(),
                lines: lines.clone(),
                symbols: Vec::new(),
            })
            .collect();
    }
    explicit
        .iter()
        .filter_map(|target| {
            let changed_lines = changed.get(&target.path)?;
            let lines = if target.lines.is_empty() {
                changed_lines.clone()
            } else {
                reference_ranges(
                    target
                        .lines
                        .iter()
                        .flat_map(|selected| {
                            changed_lines.iter().filter_map(move |changed| {
                                let start = selected.start.max(changed.start);
                                let end = selected.end.min(changed.end);
                                (start <= end).then_some(LineRange { start, end })
                            })
                        })
                        .collect(),
                )
            };
            (!lines.is_empty()).then(|| TargetSlice {
                path: target.path.clone(),
                lines,
                symbols: target.symbols.clone(),
            })
        })
        .collect()
}

fn reference_contains(selections: &[LineSelection], path: &Utf8Path, line: u32) -> bool {
    selections.iter().any(|selection| {
        selection.path == path
            && selection.range.start > 0
            && selection.range.start <= line
            && line <= selection.range.end
    })
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let root = Utf8Path::new("/repo");

    // Compare success, rejection, and the exact normalized path with an
    // independent component-stack implementation.
    let raw = String::from_utf8_lossy(data);
    let raw_path = Utf8Path::new(&raw);
    assert_eq!(
        normalize_logical_path(root, raw_path).ok(),
        reference_normalize(root, raw_path)
    );
    for (input, expected) in [
        ("src/./pkg/../anchor.py", Some("src/anchor.py")),
        ("../escape.py", None),
        ("/repo/src/anchor.py", Some("src/anchor.py")),
        ("/outside.py", None),
    ] {
        assert_eq!(
            normalize_logical_path(root, Utf8Path::new(input)).ok(),
            expected.map(Utf8PathBuf::from)
        );
    }

    // Always include a known file, line range, and symbol. The remaining
    // bounded inventory and selectors are derived from the fuzz input.
    let bytes = if data.is_empty() { &[0][..] } else { data };
    let count = bytes.len().min(32);
    let anchor = Utf8PathBuf::from("src/anchor.py");
    let mut discovered = vec![DiscoveredFile::python(anchor.clone())];
    let mut path_bytes = Vec::with_capacity(count);
    let mut selection = Selection {
        root: root.to_owned(),
        sources: vec!["src".into()],
        files: vec![anchor.clone()],
        lines: vec![LineSelection {
            path: anchor.clone(),
            range: LineRange { start: 3, end: 5 },
        }],
        symbols: vec![SymbolSelection {
            module: "anchor".into(),
            qualname: "entry".into(),
        }],
        ..Selection::default()
    };
    for (index, byte) in bytes.iter().copied().take(count).enumerate() {
        let stem = format!("module_{index}_{byte:02x}");
        let path = Utf8PathBuf::from(format!("src/{stem}.py"));
        let is_python = byte & 1 == 0;
        discovered.push(if is_python {
            DiscoveredFile::python(path.clone())
        } else {
            DiscoveredFile::regular(path.clone())
        });
        path_bytes.push((path.clone(), byte));
        if is_python && byte & 2 != 0 {
            selection.files.push(path.clone());
        }
        if is_python && byte & 4 != 0 {
            let start = u32::from(byte >> 3) + 1;
            selection.lines.push(LineSelection {
                path: path.clone(),
                range: LineRange {
                    start,
                    end: start.saturating_add(u32::from(byte & 3)),
                },
            });
        }
        if is_python && byte & 8 != 0 {
            selection.symbols.push(SymbolSelection {
                module: stem,
                qualname: format!("symbol_{byte}"),
            });
        }
    }

    let expected_targets = reference_targets(&selection, &discovered);
    let targets = resolve_explicit(&selection, &discovered).unwrap();
    assert_eq!(targets, expected_targets);

    let index = LineSelectionIndex::new(root, &selection.lines);
    for file in &discovered {
        for line in 0..=40 {
            assert_eq!(
                index.contains(&file.path, line),
                reference_contains(&selection.lines, &file.path, line)
            );
        }
    }
    assert!(!index.contains(Utf8Path::new("src/missing.py"), 3));

    let mut changed = BTreeMap::from([(
        anchor,
        vec![
            LineRange { start: 0, end: 2 },
            LineRange { start: 8, end: 7 },
            LineRange { start: 3, end: 4 },
            LineRange { start: 5, end: 5 },
            LineRange { start: 9, end: 10 },
        ],
    )]);
    for (path, byte) in path_bytes {
        let start = u32::from(byte & 15) + 1;
        changed.entry(path).or_default().extend([
            LineRange {
                start,
                end: start.saturating_add(u32::from(byte >> 4)),
            },
            LineRange {
                start: start.saturating_add(1),
                end: start.saturating_add(2),
            },
        ]);
    }
    let expected_changed = reference_changed(&changed);
    assert_eq!(normalize_changed(changed.clone()), expected_changed);
    assert_eq!(
        intersect_changed(&[], &changed),
        reference_intersection(&[], &expected_changed)
    );
    assert_eq!(
        intersect_changed(&targets, &changed),
        reference_intersection(&expected_targets, &expected_changed)
    );
});
