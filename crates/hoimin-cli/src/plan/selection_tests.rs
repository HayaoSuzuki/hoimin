use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, MutationCandidate};

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

    assert_eq!(
        select_ids(&candidates, 4, TopSelectionPolicy::Strict),
        ["A1", "A2", "B1", "C1"]
    );
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

    assert_eq!(
        select_ids(&candidates, 5, TopSelectionPolicy::Diverse),
        ["A1", "B1", "C1", "A2", "B2"]
    );
}

#[test]
fn diverse_completes_higher_score_tiers_before_lower_score_tiers() {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 900),
        candidate("A2", "src/a.py", 2, 900),
        candidate("B1", "src/b.py", 3, 800),
        candidate("C1", "src/c.py", 4, 800),
    ];

    assert_eq!(
        select_ids(&candidates, 3, TopSelectionPolicy::Diverse),
        ["A1", "A2", "B1"]
    );
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

    assert_eq!(
        select_ids(&candidates, 5, TopSelectionPolicy::Diverse),
        ["A1", "B1", "A2", "B2", "A3"]
    );
}

#[test]
fn diverse_keeps_selecting_the_dense_file_after_singleton_files_exhaust() {
    let candidates = vec![
        candidate("A1", "src/a.py", 1, 100),
        candidate("B1", "src/b.py", 2, 100),
        candidate("C1", "src/c.py", 3, 100),
        candidate("D1", "src/d.py", 4, 100),
        candidate("D2", "src/d.py", 5, 100),
        candidate("D3", "src/d.py", 6, 100),
        candidate("D4", "src/d.py", 7, 100),
    ];

    assert_eq!(
        select_ids(&candidates, 4, TopSelectionPolicy::Diverse),
        ["A1", "B1", "C1", "D1"]
    );
    assert_eq!(
        select_ids(&candidates, 5, TopSelectionPolicy::Diverse),
        ["A1", "B1", "C1", "D1", "D2"]
    );
    assert_eq!(
        select_ids(&candidates, 20, TopSelectionPolicy::Diverse),
        ["A1", "B1", "C1", "D1", "D2", "D3", "D4"]
    );
}

#[test]
fn diverse_returns_no_ids_for_empty_candidates() {
    assert_eq!(
        select_top_candidate_ids(
            &[],
            NonZeroUsize::new(1).unwrap(),
            TopSelectionPolicy::Diverse,
        ),
        Vec::<String>::new()
    );
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

#[test]
fn fixed_batch_diverse_slices_the_global_order_across_files_and_tiers() {
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
    assert_eq!(select(0, 2), ["A1", "B1"]);
    assert_eq!(select(2, 2), ["A2", "B2"]);
    assert_eq!(select(4, 2), ["A3", "C1"]);
    assert_eq!(select(5, usize::MAX), ["C1"]);
    assert_eq!(select(6, 1), Vec::<String>::new());
    assert_eq!(select(usize::MAX, usize::MAX), Vec::<String>::new());
}

#[test]
fn pages_preserve_policy_order_at_tier_and_integer_boundaries() {
    let candidates = vec![
        candidate("A1", "a.py", 1, 100),
        candidate("A2", "a.py", 2, 100),
        candidate("B1", "b.py", 3, 100),
        candidate("A3", "a.py", 4, 70),
        candidate("B2", "b.py", 5, 70),
        candidate("B3", "b.py", 6, 70),
    ];
    for (policy, order) in [
        (
            TopSelectionPolicy::Strict,
            ["A1", "A2", "B1", "A3", "B2", "B3"],
        ),
        (
            TopSelectionPolicy::Diverse,
            ["A1", "B1", "A2", "A3", "B2", "B3"],
        ),
    ] {
        for offset in [0, 1, 2, 3, 5, 6, 7, usize::MAX] {
            for count in [1, 2, 4, usize::MAX] {
                let actual = super::selection::select_top_candidate_ids_at(
                    &candidates,
                    NonZeroUsize::new(count).unwrap(),
                    policy,
                    offset,
                );
                let expected = order
                    .iter()
                    .skip(offset)
                    .take(count)
                    .copied()
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "{policy:?} offset={offset} count={count}");
                assert_eq!(
                    actual.capacity(),
                    actual.len(),
                    "{policy:?} offset={offset} count={count}"
                );
            }
        }
        assert_eq!(
            super::selection::select_top_candidate_ids_at(
                &[],
                NonZeroUsize::new(usize::MAX).unwrap(),
                policy,
                0,
            ),
            Vec::<String>::new()
        );
    }
}

#[test]
fn line_diverse_cycles_dense_start_lines_without_suppressing_candidates() {
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
    let policy = TopSelectionPolicy::LineDiverse;
    assert_eq!(
        select_ids(&candidates, 3, policy),
        ["dense-1", "next-line", "other-file"]
    );
    let full = select_ids(&candidates, usize::MAX, policy);
    assert_eq!(full.len(), 12);
    assert_eq!(full.iter().collect::<BTreeSet<_>>().len(), 12);
    assert_eq!(
        &full[3..],
        (2..=10).map(|n| format!("dense-{n}")).collect::<Vec<_>>()
    );
    for offset in 0..=13 {
        for count in [1, 3, 20, usize::MAX] {
            let actual = super::selection::select_top_candidate_ids_at(
                &candidates,
                NonZeroUsize::new(count).unwrap(),
                policy,
                offset,
            );
            assert_eq!(
                actual,
                full.iter()
                    .skip(offset)
                    .take(count)
                    .cloned()
                    .collect::<Vec<_>>()
            );
        }
    }
    assert_eq!(select_ids(&candidates, usize::MAX, policy), full);
    assert!(select_ids(&[], usize::MAX, policy).is_empty());
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
    assert_eq!(
        select_ids(&candidates, 99, TopSelectionPolicy::LineDiverse),
        ["high-1", "high-2", "low-1", "low-3", "low-2"]
    );
}
