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

fn assert_matches_oracle(
    root: &Utf8Path,
    selections: &[LineSelection],
    candidate: &str,
    line: u32,
) {
    let index = LineSelectionIndex::new(root, selections);
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

#[rstest::rstest]
#[case::before(0, false)]
#[case::before_first(1, false)]
#[case::first_start(2, true)]
#[case::first_inside(3, true)]
#[case::overlap(4, true)]
#[case::second_inside(5, true)]
#[case::second_end(6, true)]
#[case::gap_start(7, false)]
#[case::gap_end(8, false)]
#[case::singleton(9, true)]
#[case::after(10, false)]
#[case::after_next(11, false)]
#[case::after_last(12, false)]
fn contains_matches_inclusive_endpoints_and_gaps(#[case] line: u32, #[case] expected: bool) {
    let selections = [
        selection("src/calc.py", 2, 4),
        selection("src/calc.py", 4, 6),
        selection("src/calc.py", 9, 9),
    ];
    let index = LineSelectionIndex::new(Utf8Path::new(""), &selections);

    let actual = index.contains(Utf8Path::new("src/calc.py"), line);

    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case::empty_zero(0, 0)]
#[case::empty_inside(0, 3)]
#[case::repeated_before(3, 2)]
#[case::repeated_start(3, 3)]
#[case::repeated_end(3, 5)]
fn contains_matches_the_selector_predicate_for_empty_and_repeated_selections(
    #[case] count: usize,
    #[case] line: u32,
) {
    let selections = vec![selection("src/calc.py", 3, 5); count];

    assert_matches_oracle(Utf8Path::new(""), &selections, "src/calc.py", line);
}

proptest::proptest! {
    #[test]
    fn generated_ranges_match_the_linear_selector_predicate(
        ranges in proptest::collection::vec((proptest::prelude::any::<u32>(), proptest::prelude::any::<u32>()), 0..64),
        line in proptest::prelude::any::<u32>(),
    ) {
        let selections: Vec<_> = ranges.into_iter().map(|(start, end)| selection("src/calc.py", start, end)).collect();
        let expected = selections.iter().any(|selected| selected.range.start <= line && line <= selected.range.end);
        let index = LineSelectionIndex::new(Utf8Path::new(""), &selections);

        let actual = index.contains(Utf8Path::new("src/calc.py"), line);

        proptest::prop_assert_eq!(actual, expected);
    }
}

#[rstest::rstest]
#[case::query_1("src/a.py", 1)]
#[case::query_2("src/a.py", 2)]
#[case::query_3("src/a.py", 9)]
#[case::query_4("src/a.py", 10)]
#[case::query_5("src/a.py", 20)]
#[case::query_6("src/a.py", 22)]
#[case::query_7("src/b.py", 0)]
#[case::query_8("src/b.py", 1)]
#[case::query_9("src/b.py", u32::MAX)]
#[case::query_10("src/c.py", 7)]
#[case::query_11("src/./c.py", 7)]
#[case::query_12("src/d.py", 3)]
#[case::query_13("src/inverted.py", 8)]
#[case::query_14("outside.py", 1)]
#[case::query_15("src/missing.py", 1)]
fn contains_matches_the_selector_predicate_for_merged_ranges_and_path_aliases(
    #[case] candidate: &str,
    #[case] line: u32,
) {
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

    assert_matches_oracle(root, &selections, candidate, line);
}

#[cfg(not(windows))]
#[rstest::rstest]
#[case::query_1("src/Calc.py", 3)]
#[case::query_2("src/calc.py", 3)]
#[case::query_3("src\\Calc.py", 3)]
fn contains_keeps_unix_case_and_backslash_path_differences(
    #[case] candidate: &str,
    #[case] line: u32,
) {
    let root = Utf8Path::new("");
    let selections = [selection("src/Calc.py", 3, 3)];

    assert_matches_oracle(root, &selections, candidate, line);
}

#[cfg(windows)]
#[rstest::rstest]
#[case::query_1("src/calc.py", 3)]
#[case::query_2("SRC/I.PY", 4)]
#[case::query_3("SRC/STRASSE.PY", 5)]
#[case::query_4("src/stra\u{00df}e.py", 5)]
fn contains_uses_windows_case_separator_and_simple_uppercase_rules(
    #[case] candidate: &str,
    #[case] line: u32,
) {
    let root = Utf8Path::new("C:/Workspace");
    let selections = [
        selection("c:\\workspace\\SRC\\Calc.py", 3, 3),
        selection("src/\u{0131}.py", 4, 4),
        selection("src/stra\u{00df}e.py", 5, 5),
    ];

    assert_matches_oracle(root, &selections, candidate, line);
}
