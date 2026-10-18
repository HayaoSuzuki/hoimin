use super::*;

#[test]
fn large_eight_element_population_fails_at_payload_admission() {
    let directory = tempfile::tempdir().unwrap();
    let elements = (0..8)
        .map(|index| format!("'{index}{}'", "x".repeat(150_000)))
        .collect::<Vec<_>>();
    let source = format!("items = [{}]\n", elements.join(","));
    std::fs::write(directory.path().join("source.py"), source).unwrap();
    let root = Utf8Path::from_path(directory.path()).unwrap();
    let target = TargetSlice {
        path: "source.py".into(),
        lines: vec![],
        symbols: vec![],
    };
    let operators = serde_json::from_str("[\"container_element_delete\"]").unwrap();
    let work = discovery_work(
        root,
        &[target],
        &operators,
        MutationProfile::Full,
        10_000,
        ProcessCancellation::new(),
        None,
    );

    let error = discover_targets_blocking(work).unwrap_err();

    assert_eq!(error.failure.code(), "analyzer.payload_limit");
}

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

fn boolean_candidate() -> AnalyzerCandidate {
    AnalyzerCandidate {
        path: "a.py".into(),
        span: ByteSpan {
            start: 4,
            length: 4,
        },
        original: "True".into(),
        replacement: "False".into(),
        operator: MutationOperator::BooleanLiteral,
        line: 1,
        column: 4,
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

#[test]
fn conversion_scratch_exact_limit_preserves_id_and_transfers_payload() {
    let budget = PayloadBudget::new(395);
    let validation = CandidateValidationContext::new(b"x = True\n").unwrap();
    let candidate = budget.retain(boolean_candidate()).unwrap();

    let converted = mutation_candidate(EffectId(0), &validation, candidate, 1).unwrap();
    let live = budget.snapshot();
    let id = converted.id.clone();
    drop(converted);

    assert_eq!(live.used, 159);
    assert_eq!(live.peak, 395);
    assert_eq!(id.len(), 67);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn conversion_scratch_limit_plus_one_fails_and_releases_payload() {
    let budget = PayloadBudget::new(394);
    let validation = CandidateValidationContext::new(b"x = True\n").unwrap();
    let candidate = budget.retain(boolean_candidate()).unwrap();

    let result = mutation_candidate(EffectId(0), &validation, candidate, 1);

    assert_eq!(result.unwrap_err().failure.code(), "analyzer.payload_limit");
    assert_eq!(budget.snapshot().used, 0);
}

fn plan_work(
    directory: &tempfile::TempDir,
    names: &[&str],
    budget: &PayloadBudget,
) -> DiscoveryWork {
    let targets = names
        .iter()
        .map(|name| TargetSlice {
            path: (*name).into(),
            lines: vec![],
            symbols: vec![],
        })
        .collect::<Vec<_>>();
    let operators = serde_json::from_str("[\"boolean_literal\"]").unwrap();
    let mut work = discovery_work(
        Utf8Path::from_path(directory.path()).unwrap(),
        &targets,
        &operators,
        MutationProfile::Full,
        100,
        ProcessCancellation::new(),
        None,
    );
    work.payload = budget.clone();
    work
}

#[test]
fn cross_file_plan_retains_converted_candidates_in_shared_budget() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("a.py"), "x = True\n").unwrap();
    std::fs::write(directory.path().join("b.py"), "x = True\n").unwrap();
    let budget = PayloadBudget::new(553);
    let work = plan_work(&directory, &["a.py", "b.py"], &budget);

    let result = discover_targets_blocking(work);

    assert_eq!(result.unwrap_err().failure.code(), "analyzer.payload_limit");
    assert_eq!(budget.snapshot().admissions, 2);
    assert_eq!(budget.snapshot().peak, 395);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn cross_file_plan_exact_limit_hands_off_complete_plain_output() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("a.py"), "x = True\n").unwrap();
    std::fs::write(directory.path().join("b.py"), "x = True\n").unwrap();
    let budget = PayloadBudget::new(554);
    let work = plan_work(&directory, &["a.py", "b.py"], &budget);

    let output = discover_targets_blocking(work).unwrap();

    assert_eq!(output.candidates.len(), 2);
    assert_eq!(output.candidates[1].sequence, 2);
    assert_eq!(budget.snapshot().peak, 554);
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
fn latin1_conversion_reserves_encoding_scratch_and_preserves_raw_span() {
    let budget = PayloadBudget::new(396);
    let validation = CandidateValidationContext::new(b"# coding: latin-1\nx = '\xe9'\n").unwrap();
    let candidate = AnalyzerCandidate {
        path: "a.py".into(),
        span: ByteSpan {
            start: 22,
            length: 4,
        },
        original: "'é'".into(),
        replacement: "\"\"".into(),
        operator: MutationOperator::StringLiteralEmpty,
        line: 2,
        column: 4,
        symbol: None,
    };
    let candidate = budget.retain(candidate).unwrap();

    let converted = mutation_candidate(EffectId(0), &validation, candidate, 1).unwrap();
    let live = budget.snapshot();
    let span = converted.span;
    drop(converted);

    assert_eq!(
        span,
        ByteSpan {
            start: 22,
            length: 3
        }
    );
    assert_eq!(live.used, 161);
    assert_eq!(live.peak, 396);
    assert_eq!(budget.snapshot().used, 0);
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
fn spool_buffer_overlap_fails_without_an_unusable_spool() {
    let directory = tempfile::tempdir().unwrap();
    let budget = PayloadBudget::new(395);
    let validation = CandidateValidationContext::new(b"x = True\n").unwrap();
    let candidate = mutation_candidate(
        EffectId(0),
        &validation,
        budget.retain(boolean_candidate()).unwrap(),
        1,
    )
    .unwrap();
    let mut store = CandidateStore::new_in(10, directory.path())
        .unwrap()
        .with_payload(budget.clone());

    let result = store.push(&candidate);
    let count = store.count();
    drop(store);
    drop(candidate);

    assert!(matches!(
        result,
        Err(StoreError::PayloadLimit { limit: 395 })
    ));
    assert_eq!(count, 0);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert_eq!(budget.snapshot().used, 0);
}

#[test]
fn successful_spool_write_releases_encoded_buffer_but_keeps_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let budget = PayloadBudget::default();
    let validation = CandidateValidationContext::new(b"x = True\n").unwrap();
    let candidate = mutation_candidate(
        EffectId(0),
        &validation,
        budget.retain(boolean_candidate()).unwrap(),
        1,
    )
    .unwrap();
    let record_bytes = CandidateStore::record_size(&candidate).unwrap();
    let mut store = CandidateStore::new_in(10, directory.path())
        .unwrap()
        .with_payload(budget.clone());

    store.push(&candidate).unwrap();
    let live = budget.snapshot();
    drop(candidate);
    let written = budget.snapshot();

    assert_eq!(store.count(), 1);
    assert_eq!(live.used, 159);
    assert_eq!(live.peak, 159 + record_bytes);
    assert_eq!(written.used, 0);
}

#[tokio::test]
async fn handler_payload_failure_deletes_partial_store() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("a.py"), "x = True\n").unwrap();
    let owner = Arc::new(tempfile::tempdir().unwrap());
    let mut handler = AnalyzerHandler::new(
        Utf8PathBuf::from_path_buf(directory.path().into()).unwrap(),
        serde_json::from_str("[\"boolean_literal\"]").unwrap(),
        MutationProfile::Full,
        NonZeroUsize::new(10).unwrap(),
    )
    .with_candidate_spool_owner(owner.clone());
    handler.payload = PayloadBudget::new(395);
    let request = AnalyzeFile {
        id: EffectId(1),
        target: TargetSlice {
            path: "a.py".into(),
            lines: vec![],
            symbols: vec![],
        },
        final_target: true,
    };

    let result = handler.handle(request).await;

    assert_eq!(result.unwrap_err().failure.code(), "analyzer.payload_limit");
    assert!(handler.store.is_none());
    assert_eq!(handler.payload.snapshot().used, 0);
    assert_eq!(std::fs::read_dir(owner.path()).unwrap().count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detached_worker_payload_remains_charged_during_handler_reuse() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("a.py"), "x = True\n").unwrap();
    let owner = Arc::new(tempfile::tempdir().unwrap());
    let mut handler = AnalyzerHandler::new(
        Utf8PathBuf::from_path_buf(directory.path().into()).unwrap(),
        serde_json::from_str("[\"boolean_literal\"]").unwrap(),
        MutationProfile::Full,
        NonZeroUsize::new(10).unwrap(),
    )
    .with_candidate_spool_owner(owner.clone());
    let budget = PayloadBudget::new(395);
    handler.payload = budget.clone();
    let worker_budget = budget.clone();
    let (entered_tx, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = std::sync::Mutex::new(release_rx);
    handler.analysis_hook = Some(Arc::new(move || {
        let retained = worker_budget.retain(tiny_candidate(0, "y")).unwrap();
        entered_tx.send(()).unwrap();
        release_rx
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        drop(retained);
    }));
    let request = AnalyzeFile {
        id: EffectId(1),
        target: TargetSlice {
            path: "a.py".into(),
            lines: vec![],
            symbols: vec![],
        },
        final_target: true,
    };
    let cancellation = ProcessCancellation::new();
    let cancel = cancellation.clone();
    let first_request = request.clone();

    let first = {
        let operation = handler.handle_with_cancellation(first_request, cancellation);
        tokio::pin!(operation);
        tokio::select! {
            biased;
            result = &mut operation => panic!("worker finished before controlled release: {result:?}"),
            () = async { entered_rx.recv().await.unwrap(); cancel.cancel(); } => {}
        }
        operation.await.unwrap_err()
    };
    let detached = budget.snapshot();
    handler.analysis_hook = None;
    let second = handler.handle(request).await.unwrap_err();
    let overlap = budget.snapshot();
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while std::fs::read_dir(owner.path()).unwrap().count() != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(first.failure.code(), "analyzer.cancelled");
    assert_eq!(detached.used, 6);
    assert_eq!(second.failure.code(), "analyzer.payload_limit");
    assert_eq!(overlap.used, 6);
    assert_eq!(budget.snapshot().used, 0);
}

#[tokio::test]
async fn conversion_payload_failure_preserves_run_effect_identity() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("a.py"), "x = True\n").unwrap();
    let mut handler = AnalyzerHandler::new(
        Utf8PathBuf::from_path_buf(directory.path().into()).unwrap(),
        serde_json::from_str("[\"boolean_literal\"]").unwrap(),
        MutationProfile::Full,
        NonZeroUsize::new(10).unwrap(),
    );
    handler.payload = PayloadBudget::new(394);
    let request = AnalyzeFile {
        id: EffectId(41),
        target: TargetSlice {
            path: "a.py".into(),
            lines: vec![],
            symbols: vec![],
        },
        final_target: true,
    };

    let error = handler.handle(request).await.unwrap_err();

    assert_eq!(error.failure.code(), "analyzer.payload_limit");
    assert_eq!(error.id, EffectId(41));
    assert_eq!(handler.payload.snapshot().used, 0);
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

#[test]
fn conversion_metadata_admission_fails_before_validation_scratch() {
    let budget = PayloadBudget::new(158);
    let validation = CandidateValidationContext::new(b"x = True\n").unwrap();
    let candidate = budget.retain(boolean_candidate()).unwrap();

    let result = mutation_candidate(EffectId(0), &validation, candidate, 1);

    assert_eq!(result.unwrap_err().failure.code(), "analyzer.payload_limit");
    assert_eq!(budget.snapshot().peak, 13);
    assert_eq!(budget.snapshot().reservations, 1);
    assert_eq!(budget.snapshot().used, 0);
}
