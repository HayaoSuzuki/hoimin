use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, MutationCandidate};
use proptest::prelude::*;

use super::selection::select_top_candidate_ids;
use super::{RankedPlanCandidate, RankingReason, RankingReasonCode};
use crate::cli::TopSelectionPolicy;

fn candidate(id: &str, path: &str, rank: usize, score: u32) -> RankedPlanCandidate {
    RankedPlanCandidate {
        candidate: MutationCandidate {
            id: id.to_owned(),
            sequence: 1,
            path: Utf8PathBuf::from(path),
            span: ByteSpan {
                start: rank as u64,
                length: 1,
            },
            original: "x".to_owned(),
            replacement: "y".to_owned(),
            operator: "binary_add_sub".to_owned(),
            line: u32::try_from(rank).expect("test fixture rank fits in u32"),
            column: 0,
            symbol: None,
            file_hash: "0".repeat(64),
        },
        rank,
        score,
        ranking_reasons: vec![RankingReason {
            code: RankingReasonCode::Arithmetic,
            score: 70,
        }],
    }
}

fn select_ids(
    candidates: &[RankedPlanCandidate],
    count: usize,
    policy: TopSelectionPolicy,
) -> Vec<String> {
    select_top_candidate_ids(candidates, NonZeroUsize::new(count).unwrap(), policy)
}

#[test]
fn strict_returns_the_saved_rank_prefix() {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 100),
        candidate("A2", "src/a.py", 2, 100),
        candidate("B1", "src/b.py", 3, 100),
        candidate("C1", "src/c.py", 4, 100),
        candidate("B2", "src/b.py", 5, 100),
    ];

    let selected = select_ids(&candidates, 4, TopSelectionPolicy::Strict);

    assert_eq!(selected, ["A1", "A2", "B1", "C1"]);
}

#[test]
fn diverse_round_robins_files_within_an_equal_score_tier() {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 100),
        candidate("A2", "src/a.py", 2, 100),
        candidate("B1", "src/b.py", 3, 100),
        candidate("C1", "src/c.py", 4, 100),
        candidate("B2", "src/b.py", 5, 100),
    ];

    let selected = select_ids(&candidates, 5, TopSelectionPolicy::Diverse);

    assert_eq!(selected, ["A1", "B1", "C1", "A2", "B2"]);
}

#[test]
fn diverse_completes_higher_score_tiers_before_lower_score_tiers() {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 900),
        candidate("A2", "src/a.py", 2, 900),
        candidate("B1", "src/b.py", 3, 800),
        candidate("C1", "src/c.py", 4, 800),
    ];

    let selected = select_ids(&candidates, 3, TopSelectionPolicy::Diverse);

    assert_eq!(selected, ["A1", "A2", "B1"]);
}

#[test]
fn diverse_keeps_round_robin_progress_until_count_is_reached() {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 100),
        candidate("A2", "src/a.py", 2, 100),
        candidate("A3", "src/a.py", 3, 100),
        candidate("B1", "src/b.py", 4, 100),
        candidate("B2", "src/b.py", 5, 100),
    ];

    let selected = select_ids(&candidates, 5, TopSelectionPolicy::Diverse);

    assert_eq!(selected, ["A1", "B1", "A2", "B2", "A3"]);
}

#[rstest::rstest]
#[case::a1_1(4, vec!["A1", "B1", "C1", "D1"])]
#[case::a1_2(5, vec!["A1", "B1", "C1", "D1", "D2"])]
#[case::a1_3(20, vec!["A1", "B1", "C1", "D1", "D2", "D3", "D4"])]
fn diverse_keeps_selecting_the_dense_file_after_singleton_files_exhaust(
    #[case] count: usize,
    #[case] expected: Vec<&str>,
) {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 100),
        candidate("B1", "src/b.py", 2, 100),
        candidate("C1", "src/c.py", 3, 100),
        candidate("D1", "src/d.py", 4, 100),
        candidate("D2", "src/d.py", 5, 100),
        candidate("D3", "src/d.py", 6, 100),
        candidate("D4", "src/d.py", 7, 100),
    ];

    let selected = select_ids(&candidates, count, TopSelectionPolicy::Diverse);

    assert_eq!(selected, expected, "count={count}");
}

#[test]
fn diverse_returns_no_ids_for_empty_candidates() {
    let selected = select_top_candidate_ids(
        &[],
        NonZeroUsize::new(1).unwrap(),
        TopSelectionPolicy::Diverse,
    );

    assert_eq!(selected, Vec::<String>::new());
}

#[test]
fn diverse_is_deterministic_and_selects_each_retained_candidate_once() {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 100),
        candidate("A2", "src/a.py", 2, 100),
        candidate("B1", "src/b.py", 3, 100),
        candidate("C1", "src/c.py", 4, 100),
        candidate("B2", "src/b.py", 5, 100),
    ];

    let first = select_ids(&candidates, 30, TopSelectionPolicy::Diverse);
    let second = select_ids(&candidates, 30, TopSelectionPolicy::Diverse);
    let strict = select_ids(&candidates, 30, TopSelectionPolicy::Strict);

    assert_eq!(first, second);
    assert_eq!(first.len(), candidates.len());
    assert_eq!(
        first.iter().collect::<BTreeSet<_>>(),
        strict.iter().collect::<BTreeSet<_>>()
    );
}

#[rstest::rstest]
#[case::a1_1(0, 2, vec!["A1", "B1"])]
#[case::a2_2(2, 2, vec!["A2", "B2"])]
#[case::a3_3(4, 2, vec!["A3", "C1"])]
#[case::c1_4(5, usize::MAX, vec!["C1"])]
#[case::input_6_1_vec_5(6, 1, vec![])]
#[case::usize_max_usize_max_vec_6(usize::MAX, usize::MAX, vec![])]
fn fixed_batch_diverse_slices_the_global_order_across_files_and_tiers(
    #[case] offset: usize,
    #[case] count: usize,
    #[case] expected: Vec<&str>,
) {
    let candidates = vec![
        candidate("A1", "a.py", 1, 100),
        candidate("A2", "a.py", 2, 100),
        candidate("A3", "a.py", 3, 100),
        candidate("B1", "b.py", 4, 100),
        candidate("B2", "b.py", 5, 100),
        candidate("C1", "c.py", 6, 90),
    ];
    let select = |offset, count| {
        super::selection::select_top_candidate_ids_at(
            &candidates,
            NonZeroUsize::new(count).unwrap(),
            TopSelectionPolicy::Diverse,
            offset,
        )
    };

    let selected = select(offset, count);

    assert_eq!(selected, expected, "offset={offset} count={count}");
}

fn assert_policy_page(offset: usize, count: NonZeroUsize, diverse: bool) {
    let candidates = vec![
        candidate("A1", "a.py", 1, 100),
        candidate("A2", "a.py", 2, 100),
        candidate("B1", "b.py", 3, 100),
        candidate("A3", "a.py", 4, 70),
        candidate("B2", "b.py", 5, 70),
        candidate("B3", "b.py", 6, 70),
    ];
    let (policy, order) = [
        (
            TopSelectionPolicy::Strict,
            ["A1", "A2", "B1", "A3", "B2", "B3"],
        ),
        (
            TopSelectionPolicy::Diverse,
            ["A1", "B1", "A2", "A3", "B2", "B3"],
        ),
    ][usize::from(diverse)];
    let expected = order
        .iter()
        .skip(offset)
        .take(count.get())
        .copied()
        .collect::<Vec<_>>();

    let actual = super::selection::select_top_candidate_ids_at(&candidates, count, policy, offset);

    assert_eq!(actual, expected, "{policy:?} offset={offset} count={count}");
    assert_eq!(
        actual.capacity(),
        actual.len(),
        "{policy:?} offset={offset} count={count}"
    );
}

#[rstest::rstest]
#[case::input_0_1_input_1_1(0, 1)]
#[case::input_0_1_input_2_2(0, 2)]
#[case::input_0_1_input_4_3(0, 4)]
#[case::input_0_1_usize_max_4(0, usize::MAX)]
#[case::input_1_2_input_1_1(1, 1)]
#[case::input_1_2_input_2_2(1, 2)]
#[case::input_1_2_input_4_3(1, 4)]
#[case::input_1_2_usize_max_4(1, usize::MAX)]
#[case::input_2_3_input_1_1(2, 1)]
#[case::input_2_3_input_2_2(2, 2)]
#[case::input_2_3_input_4_3(2, 4)]
#[case::input_2_3_usize_max_4(2, usize::MAX)]
#[case::input_3_4_input_1_1(3, 1)]
#[case::input_3_4_input_2_2(3, 2)]
#[case::input_3_4_input_4_3(3, 4)]
#[case::input_3_4_usize_max_4(3, usize::MAX)]
#[case::input_5_5_input_1_1(5, 1)]
#[case::input_5_5_input_2_2(5, 2)]
#[case::input_5_5_input_4_3(5, 4)]
#[case::input_5_5_usize_max_4(5, usize::MAX)]
#[case::input_6_6_input_1_1(6, 1)]
#[case::input_6_6_input_2_2(6, 2)]
#[case::input_6_6_input_4_3(6, 4)]
#[case::input_6_6_usize_max_4(6, usize::MAX)]
#[case::input_7_7_input_1_1(7, 1)]
#[case::input_7_7_input_2_2(7, 2)]
#[case::input_7_7_input_4_3(7, 4)]
#[case::input_7_7_usize_max_4(7, usize::MAX)]
#[case::usize_max_8_input_1_1(usize::MAX, 1)]
#[case::usize_max_8_input_2_2(usize::MAX, 2)]
#[case::usize_max_8_input_4_3(usize::MAX, 4)]
#[case::usize_max_8_usize_max_4(usize::MAX, usize::MAX)]
fn pages_preserve_policy_order_at_tier_and_integer_boundaries(
    #[case] offset: usize,
    #[case] count: usize,
    #[values(false, true)] diverse: bool,
) {
    assert_policy_page(offset, NonZeroUsize::new(count).unwrap(), diverse);
}

proptest! {
    #[test]
    fn pages_preserve_policy_order_for_arbitrary_offsets_and_counts(
        offset in prop_oneof![0_usize..12, any::<usize>()],
        count in prop_oneof![1_usize..16, 1_usize..=usize::MAX],
        diverse in any::<bool>(),
    ) {
        assert_policy_page(offset, NonZeroUsize::new(count).unwrap(), diverse);
    }
}

fn dense_line_candidates() -> Vec<RankedPlanCandidate> {
    let mut candidates = (1..=10)
        .map(|rank| {
            let mut value = candidate(&format!("dense-{rank}"), "a.py", rank, 100);
            value.candidate.line = 2;
            // Even a multi-line mutation belongs only to its start line.
            value.candidate.original = "first\nsecond\nthird".into();
            value.candidate.span.length = value.original.len() as u64;
            value
        })
        .collect::<Vec<_>>();
    let mut same_file = candidate("next-line", "a.py", 11, 100);
    same_file.candidate.line = 3;
    let mut other_file = candidate("other-file", "b.py", 12, 100);
    other_file.candidate.line = 2;
    candidates.extend([same_file, other_file]);
    candidates
}

#[test]
fn line_diverse_cycles_dense_start_lines_without_suppressing_candidates() {
    let candidates = dense_line_candidates();
    let policy = TopSelectionPolicy::LineDiverse;

    let full = select_ids(&candidates, usize::MAX, policy);
    let repeated = select_ids(&candidates, usize::MAX, policy);

    assert_eq!(full.len(), 12);
    assert_eq!(full.iter().collect::<BTreeSet<_>>().len(), 12);
    assert_eq!(&full[..3], ["dense-1", "next-line", "other-file"]);
    assert_eq!(
        &full[3..],
        (2..=10).map(|n| format!("dense-{n}")).collect::<Vec<_>>()
    );
    assert_eq!(repeated, full);
}

#[test]
fn line_diverse_top_three_represent_distinct_start_lines() {
    let candidates = dense_line_candidates();

    let selected = select_ids(&candidates, 3, TopSelectionPolicy::LineDiverse);

    assert_eq!(selected, ["dense-1", "next-line", "other-file"]);
}

#[rstest::rstest]
fn empty_policy_selection_returns_no_ids(
    #[values(
        TopSelectionPolicy::Strict,
        TopSelectionPolicy::Diverse,
        TopSelectionPolicy::LineDiverse
    )]
    policy: TopSelectionPolicy,
) {
    let count = NonZeroUsize::new(usize::MAX).unwrap();

    let selected = super::selection::select_top_candidate_ids_at(&[], count, policy, 0);

    assert_eq!(selected, Vec::<String>::new());
}

fn assert_line_diverse_page(offset: usize, count: usize) {
    let candidates = dense_line_candidates();
    let full = select_ids(&candidates, usize::MAX, TopSelectionPolicy::LineDiverse);
    let expected = full
        .iter()
        .skip(offset)
        .take(count)
        .cloned()
        .collect::<Vec<_>>();

    let actual = super::selection::select_top_candidate_ids_at(
        &candidates,
        NonZeroUsize::new(count).unwrap(),
        TopSelectionPolicy::LineDiverse,
        offset,
    );

    assert_eq!(actual, expected);
}

#[rstest::rstest]
fn line_diverse_pages_preserve_global_order(
    #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13)] offset: usize,
    #[values(1, 3, 20, usize::MAX)] count: usize,
) {
    assert_line_diverse_page(offset, count);
}

proptest! {
    #[test]
    fn line_diverse_pages_preserve_order_for_generated_offsets_and_counts(
        offset in prop_oneof![0usize..32, any::<usize>()], count in 1usize..=usize::MAX,
    ) {
        assert_line_diverse_page(offset, count);
    }
}

#[test]
fn line_diverse_exhausts_score_tiers_even_when_lower_tier_has_unseen_lines() {
    let mut candidates = vec![
        candidate("high-1", "a.py", 1, 100),
        candidate("high-2", "a.py", 2, 100),
        candidate("low-1", "a.py", 3, 70),
        candidate("low-2", "a.py", 4, 70),
        candidate("low-3", "b.py", 5, 70),
    ];
    candidates[1].candidate.line = 1;
    candidates[3].candidate.line = 3;
    candidates[4].candidate.line = 3;
    let selected = select_ids(&candidates, 99, TopSelectionPolicy::LineDiverse);

    assert_eq!(selected, ["high-1", "high-2", "low-1", "low-3", "low-2"]);
}
