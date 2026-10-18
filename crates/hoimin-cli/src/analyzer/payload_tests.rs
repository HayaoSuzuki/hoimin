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
