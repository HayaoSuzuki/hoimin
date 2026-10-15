use proptest::prelude::*;

use super::{LineRange, physical_line_count, translate};

fn mixed_newlines() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop_oneof![Just('\r'), Just('\n'), any::<char>()], 0..256)
        .prop_map(|characters| characters.into_iter().collect())
}

/// A materialized text oracle, independent of the production byte cursor and
/// core line-index helpers. Git rows cannot divide a CRLF pair.
fn python_rows(git_row: &str) -> u32 {
    u32::try_from(
        git_row
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split_inclusive('\n')
            .count(),
    )
    .unwrap()
}

fn selected_intervals(source: &str, selected: &[bool]) -> (Vec<LineRange>, Vec<LineRange>) {
    let mut git_ranges: Vec<LineRange> = Vec::new();
    let mut python_ranges: Vec<LineRange> = Vec::new();
    let mut python_start = 1;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        let git_row = u32::try_from(index + 1).unwrap();
        let python_end = python_start + python_rows(text) - 1;
        if selected[index % selected.len()] {
            if let Some(previous) = git_ranges
                .last_mut()
                .filter(|range| range.end + 1 == git_row)
            {
                previous.end = git_row;
                python_ranges.last_mut().unwrap().end = python_end;
            } else {
                git_ranges.push(LineRange {
                    start: git_row,
                    end: git_row,
                });
                python_ranges.push(LineRange {
                    start: python_start,
                    end: python_end,
                });
            }
        }
        python_start = python_end + 1;
    }
    (git_ranges, python_ranges)
}

proptest! {
    #![proptest_config(ProptestConfig {
        max_shrink_iters: 2_048,
        ..ProptestConfig::default()
    })]

    #[test]
    fn boundary_property_disjoint_git_rows_match_normalized_text(
        source in mixed_newlines(),
        selected in proptest::collection::vec(any::<bool>(), 1..64),
    ) {
        let (ranges, expected) = selected_intervals(&source, &selected);

        let actual = translate(source.as_bytes(), &ranges)?;

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn boundary_property_physical_count_matches_normalized_text(source in mixed_newlines()) {
        let expected: u32 = source.split_inclusive('\n').map(python_rows).sum();

        let actual = physical_line_count(source.as_bytes())?;

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn boundary_property_git_start_past_eof_is_rejected(
        source in mixed_newlines(),
        beyond in 1u32..32,
    ) {
        let rows = u32::try_from(source.split_inclusive('\n').count()).unwrap();
        let outside = rows + beyond;
        let ranges = [LineRange { start: outside, end: outside }];

        let actual = translate(source.as_bytes(), &ranges);

        prop_assert!(actual.is_err(), "Git interval starts past EOF");
    }

    #[test]
    fn boundary_property_git_end_past_eof_is_rejected(
        source in mixed_newlines(),
        beyond in 1u32..32,
    ) {
        let rows = u32::try_from(source.split_inclusive('\n').count()).unwrap();
        let ranges = [LineRange { start: 1, end: rows + beyond }];

        let actual = translate(source.as_bytes(), &ranges);

        prop_assert!(actual.is_err(), "Git interval ends past EOF");
    }
}
