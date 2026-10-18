use crate::{AnalyzerCandidate, rust};
use camino::Utf8Path;
use hoimin_core::MutationProfile;
use hoimin_core::{ByteSpan, MutationOperator};
use proptest::prelude::*;
use rust::payload::{DEFAULT_LIMIT, PayloadBudget};
use rust::{AnalysisError, AnalyzeRequest, CandidatePrefix};

fn tiny_candidate(start: u64, replacement: &str) -> AnalyzerCandidate {
    AnalyzerCandidate {
        path: "a.py".into(),
        span: ByteSpan { start, length: 1 },
        original: "x".into(),
        replacement: replacement.into(),
        operator: MutationOperator::BooleanLiteral,
        line: 1,
        column: 0,
        symbol: None,
    }
}

fn analyze(
    payload: &PayloadBudget,
    source: &str,
    names: &[&str],
) -> Result<rust::AnalyzerOutput, AnalysisError> {
    let operators = serde_json::from_value(serde_json::json!(names)).unwrap();
    let request = AnalyzeRequest {
        payload: payload.clone(),
        path: Utf8Path::new("a.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    rust::analyze_source_cancellable(&request, source, || false)
}

#[test]
fn exact_payload_limit_admits_candidate_and_identity() {
    let budget = PayloadBudget::new(7);
    let mut prefix = CandidatePrefix::new(1, budget.clone());

    let result = prefix.push(tiny_candidate(1, "y"));
    let retained = budget.snapshot();
    drop(prefix);

    assert!(result.is_ok());
    assert_eq!(retained.used, 7);
    assert_eq!(retained.peak, 7);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn payload_limit_plus_one_rejects_without_partial_success() {
    let budget = PayloadBudget::new(6);
    let mut prefix = CandidatePrefix::new(1, budget.clone());

    let result = prefix.push(tiny_candidate(1, "y"));
    let finished = prefix.finish().err();

    assert_eq!(result, Err(AnalysisError::PayloadLimit { limit: 6 }));
    assert_eq!(finished, Some(AnalysisError::PayloadLimit { limit: 6 }));
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn empty_prefix_with_zero_bytes_finishes_empty() {
    let budget = PayloadBudget::new(0);
    let prefix = CandidatePrefix::new(10, budget.clone());

    let output = prefix.finish().unwrap();

    assert_eq!(output.candidates.len(), 0);
    assert!(!output.overflowed);
    assert_eq!(budget.snapshot().peak, 0);
}

#[test]
fn nonempty_candidate_with_zero_bytes_fails() {
    let budget = PayloadBudget::new(0);
    let mut prefix = CandidatePrefix::new(1, budget.clone());

    let result = prefix.push(tiny_candidate(0, "y"));

    assert_eq!(result, Err(AnalysisError::PayloadLimit { limit: 0 }));
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn payload_sum_overflow_is_explicit() {
    let budget = PayloadBudget::new(usize::MAX);

    let result = budget.reserve([usize::MAX, 1]);

    assert_eq!(
        result.err(),
        Some(AnalysisError::PayloadLimit { limit: usize::MAX })
    );
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn payload_accumulation_overflow_preserves_live_reservation() {
    let budget = PayloadBudget::new(usize::MAX);
    let retained = budget.reserve([usize::MAX]).unwrap();

    let result = budget.reserve([1]);
    let live = budget.snapshot();
    drop(retained);

    assert_eq!(
        result.err(),
        Some(AnalysisError::PayloadLimit { limit: usize::MAX })
    );
    assert_eq!(live.used, usize::MAX);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn duplicate_does_not_allocate_an_identity_or_consume_budget() {
    let budget = PayloadBudget::new(7);
    let mut prefix = CandidatePrefix::new(0, budget.clone());
    prefix.push(tiny_candidate(1, "y")).unwrap();
    let before = budget.snapshot();

    prefix.push(tiny_candidate(1, "y")).unwrap();
    let after = budget.snapshot();
    let output = prefix.finish().unwrap();

    assert_eq!(after.reservations, before.reservations);
    assert_eq!(after.used, 7);
    assert_eq!(output.candidates.len(), 1);
    assert!(!output.overflowed);
}

#[test]
fn nonwinner_does_not_consume_payload_budget() {
    let budget = PayloadBudget::new(7);
    let mut prefix = CandidatePrefix::new(0, budget.clone());
    prefix.push(tiny_candidate(1, "y")).unwrap();
    let before = budget.snapshot();

    prefix.push(tiny_candidate(2, &"z".repeat(100))).unwrap();
    let after = budget.snapshot();
    let output = prefix.finish().unwrap();

    assert_eq!(after.reservations, before.reservations);
    assert_eq!(after.peak, 7);
    assert_eq!(output.candidates[0].span.start, 1);
    assert!(output.overflowed);
}

#[test]
fn winning_eviction_releases_payload_before_admission() {
    let budget = PayloadBudget::new(7);
    let mut prefix = CandidatePrefix::new(0, budget.clone());
    prefix.push(tiny_candidate(2, "z")).unwrap();

    prefix.push(tiny_candidate(1, "y")).unwrap();
    let retained = budget.snapshot();
    let output = prefix.finish().unwrap();

    assert_eq!(retained.used, 7);
    assert_eq!(retained.peak, 7);
    assert_eq!(retained.admissions, 2);
    assert_eq!(output.candidates[0].replacement, "y");
}

#[test]
fn prefix_finish_transfers_candidate_while_releasing_identity() {
    let budget = PayloadBudget::new(7);
    let mut prefix = CandidatePrefix::new(1, budget.clone());
    prefix.push(tiny_candidate(1, "y")).unwrap();

    let output = prefix.finish().unwrap();
    let transferred = budget.snapshot();
    drop(output);

    assert_eq!(transferred.used, 6);
    assert_eq!(transferred.releases, 1);
    assert_eq!(budget.snapshot().used, 0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn payload_boundary_properties(replacement_len in 0usize..1000) {
        let replacement = "é".repeat(replacement_len);
        // a.py + x = 5 bytes; replacement and identity each cost 2 UTF-8 bytes per scalar.
        let exact = 5 + 4 * replacement_len;
        let budget = PayloadBudget::new(exact);
        let mut prefix = CandidatePrefix::new(1, budget.clone());

        let result = prefix.push(tiny_candidate(0, &replacement));

        prop_assert!(result.is_ok());
        prop_assert_eq!(budget.snapshot().used, exact);
    }

    #[test]
    fn payload_boundary_plus_one_properties(replacement_len in 0usize..1000) {
        let replacement = "é".repeat(replacement_len);
        let limit = 4 + 4 * replacement_len;
        let budget = PayloadBudget::new(limit);
        let mut prefix = CandidatePrefix::new(1, budget.clone());

        let result = prefix.push(tiny_candidate(0, &replacement));

        prop_assert_eq!(result, Err(AnalysisError::PayloadLimit { limit }));
        prop_assert_eq!(budget.snapshot().used, 0);
    }
}

#[test]
fn simultaneous_token_and_ast_producers_share_one_budget() {
    let source = "x = True\ny = [1,2]\n";
    let token_only = analyze(&PayloadBudget::new(42), source, &["boolean_literal"]).unwrap();
    let ast_only = analyze(
        &PayloadBudget::new(42),
        source,
        &["container_element_delete"],
    )
    .unwrap();
    let budget = PayloadBudget::new(42);

    let result = analyze(
        &budget,
        source,
        &["boolean_literal", "container_element_delete"],
    );

    assert_eq!(token_only.candidates.len(), 1);
    assert_eq!(ast_only.candidates.len(), 2);
    assert_eq!(
        result.err(),
        Some(AnalysisError::PayloadLimit { limit: 42 })
    );
    assert_eq!(budget.snapshot().used, 0);
    assert_eq!(budget.snapshot().admissions, 2);
}

#[test]
fn cancellation_releases_already_retained_candidates() {
    let budget = PayloadBudget::default();
    let operators = serde_json::from_str("[\"boolean_literal\"]").unwrap();
    let request = AnalyzeRequest {
        payload: budget.clone(),
        path: Utf8Path::new("a.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };

    let result = rust::analyze_source_cancellable(&request, "x = True\ny = False\n", || {
        budget.snapshot().admissions > 0
    });

    assert_eq!(result.err(), Some(AnalysisError::Cancelled));
    assert_eq!(budget.snapshot().admissions, 1);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn enum_borrowed_batch_stops_after_first_shared_budget_failure() {
    let budget = PayloadBudget::new(101);
    let retained = budget.reserve([100]).unwrap();
    let source = "from enum import Enum\nclass Color(Enum):\n A = 1\n B = 2\n C = 3\nx = Color.A\ny = Color.B\n";

    let result = analyze(&budget, source, &["enum_member_replace"]);
    let failed = budget.snapshot();
    drop(retained);

    assert_eq!(
        result.err(),
        Some(AnalysisError::PayloadLimit { limit: 101 })
    );
    assert_eq!(failed.generated, 1);
    assert_eq!(failed.peak, 100);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn hierarchy_borrowed_batch_stops_before_later_sites() {
    let budget = PayloadBudget::new(101);
    let retained = budget.reserve([100]).unwrap();
    let source = "class Root(Exception): pass\nclass First(Root): pass\nclass Second(Root): pass\nraise First()\nraise Second()\n";

    let result = analyze(&budget, source, &["exception_hierarchy"]);
    let failed = budget.snapshot();
    drop(retained);

    assert_eq!(
        result.err(),
        Some(AnalysisError::PayloadLimit { limit: 101 })
    );
    assert_eq!(failed.generated, 1);
    assert_eq!(failed.peak, 100);
    assert_eq!(budget.snapshot().used, 0);
}

fn large_population(count: usize) {
    let source = format!(
        "items = [{}]\n",
        (0..count)
            .map(|index| format!("'{index}{}'", "x".repeat(1_200_000 / count)))
            .collect::<Vec<_>>()
            .join(",")
    );
    let budget = PayloadBudget::default();
    rust::reset_payload_builder_work();

    let result = analyze(&budget, &source, &["container_element_delete"]);
    let work = rust::payload_builder_work();
    let stats = budget.snapshot();

    assert_eq!(
        result.err(),
        Some(AnalysisError::PayloadLimit {
            limit: DEFAULT_LIMIT
        })
    );
    assert!(stats.admissions < count);
    assert_eq!(stats.generated, stats.admissions + 1);
    assert_eq!(work.0, stats.admissions + 1);
    assert!(stats.peak <= DEFAULT_LIMIT);
    assert_eq!(stats.used, 0);
    eprintln!(
        "PAYLOAD_LARGE count={count} source={} peak={} admissions={} generated={} replacement_builds={} replacement_bytes={}",
        source.len(),
        stats.peak,
        stats.admissions,
        stats.generated,
        work.0,
        work.1
    );
}

#[test]
fn eight_large_candidates_stop_before_entire_population() {
    large_population(8);
}
#[test]
fn sixteen_large_candidates_stop_before_entire_population() {
    large_population(16);
}
#[test]
fn thirty_two_large_candidates_stop_before_entire_population() {
    large_population(32);
}

#[test]
fn annotation_failure_stops_later_annotation_construction() {
    let budget = PayloadBudget::new(1);

    let result = analyze(&budget, "first: int\nsecond: str\n", &["type_nullable_add"]);

    assert_eq!(result.err(), Some(AnalysisError::PayloadLimit { limit: 1 }));
    assert_eq!(budget.snapshot().generated, 1);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn annotation_producer_shares_budget_with_finished_token_and_ast_prefixes() {
    let budget = PayloadBudget::new(55);
    let source = "x = True\ny = [1,2]\nfirst: int\n";

    let result = analyze(
        &budget,
        source,
        &[
            "boolean_literal",
            "container_element_delete",
            "type_nullable_add",
        ],
    );

    assert_eq!(
        result.err(),
        Some(AnalysisError::PayloadLimit { limit: 55 })
    );
    assert_eq!(budget.snapshot().admissions, 3);
    assert_eq!(budget.snapshot().generated, 4);
    assert_eq!(budget.snapshot().peak, 55);
    assert_eq!(budget.snapshot().used, 0);
}
