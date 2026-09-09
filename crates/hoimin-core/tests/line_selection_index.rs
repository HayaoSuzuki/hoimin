use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    LineRange, LineSelection, LineSelectionIndex, logical_paths_equal, normalize_logical_path,
};

fn selection(path: &str, start: u32, end: u32) -> LineSelection {
    LineSelection {
        path: Utf8PathBuf::from(path),
        range: LineRange { start, end },
    }
}

fn assert_matches_oracle(root: &Utf8Path, selections: &[LineSelection], queries: &[(&str, u32)]) {
    let index = LineSelectionIndex::new(root, selections);
    for &(candidate, line) in queries {
        let candidate = Utf8Path::new(candidate);
        let expected = selections.iter().any(|selected| {
            normalize_logical_path(root, &selected.path)
                .is_ok_and(|normalized| logical_paths_equal(&normalized, candidate))
                && selected.range.start <= line
                && line <= selected.range.end
        });
        assert_eq!(
            index.contains(candidate, line),
            expected,
            "{candidate}:{line}"
        );
    }
}

#[test]
fn contains_matches_inclusive_endpoints_and_gaps() {
    let root = Utf8Path::new("");
    let selections = [
        selection("src/calc.py", 2, 4),
        selection("src/calc.py", 4, 6),
        selection("src/calc.py", 9, 9),
    ];
    let candidate_path = Utf8Path::new("src/calc.py");
    let index = LineSelectionIndex::new(root, &selections);

    for line in 0..=12 {
        let expected = selections.iter().any(|selected| {
            normalize_logical_path(root, &selected.path)
                .is_ok_and(|normalized| logical_paths_equal(&normalized, candidate_path))
                && selected.range.start <= line
                && line <= selected.range.end
        });
        assert_eq!(
            index.contains(candidate_path, line),
            expected,
            "line {line}"
        );
    }
}

#[test]
fn contains_matches_the_selector_predicate_for_merged_ranges_and_path_aliases() {
    let root = Utf8Path::new("/workspace");
    let selections = [
        selection("src/a.py", 5, 5),
        selection("src/a.py", 2, 4),
        selection("src/a.py", 4, 8),
        selection("src/a.py", 9, 9),
        selection("src/a.py", 20, 21),
        selection("src/a.py", 21, 21),
        selection("src/b.py", 0, 0),
        selection("src/b.py", u32::MAX, u32::MAX),
        selection("./src/c.py", 7, 7),
        selection("/workspace/src/d.py", 3, 3),
        selection("src/inverted.py", 9, 8),
        selection("../outside.py", 1, 10),
        selection("/outside.py", 1, 10),
    ];

    assert_matches_oracle(
        root,
        &selections,
        &[
            ("src/a.py", 1),
            ("src/a.py", 2),
            ("src/a.py", 9),
            ("src/a.py", 10),
            ("src/a.py", 20),
            ("src/a.py", 22),
            ("src/b.py", 0),
            ("src/b.py", 1),
            ("src/b.py", u32::MAX),
            ("src/c.py", 7),
            ("src/./c.py", 7),
            ("src/d.py", 3),
            ("src/inverted.py", 8),
            ("outside.py", 1),
            ("src/missing.py", 1),
        ],
    );
}

#[cfg(not(windows))]
#[test]
fn contains_keeps_unix_case_and_backslash_path_differences() {
    let root = Utf8Path::new("");
    let selections = [selection("src/Calc.py", 3, 3)];

    assert_matches_oracle(
        root,
        &selections,
        &[("src/Calc.py", 3), ("src/calc.py", 3), ("src\\Calc.py", 3)],
    );
}

#[cfg(windows)]
#[test]
fn contains_uses_windows_case_separator_and_simple_uppercase_rules() {
    let root = Utf8Path::new("C:/Workspace");
    let selections = [
        selection("c:\\workspace\\SRC\\Calc.py", 3, 3),
        selection("src/\u{0131}.py", 4, 4),
        selection("src/stra\u{00df}e.py", 5, 5),
    ];

    assert_matches_oracle(
        root,
        &selections,
        &[
            ("src/calc.py", 3),
            ("SRC/I.PY", 4),
            ("SRC/STRASSE.PY", 5),
            ("src/stra\u{00df}e.py", 5),
        ],
    );
}
