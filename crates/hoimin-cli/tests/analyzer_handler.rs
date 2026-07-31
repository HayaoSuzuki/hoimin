use camino::Utf8PathBuf;
use hoimin_cli::analyzer::{
    AnalyzerDiagnostic, AnalyzerDiagnosticCode, AnalyzerHandler, AnalyzerProtocol, AnalyzerRecord,
    CandidateStore, ProtocolError, StoreError, discover_targets,
};
use hoimin_core::{
    AnalyzeFile, ByteSpan, EffectId, MutationCandidate, MutationOperatorSelection, MutationProfile,
    TargetSlice,
};
use std::fs;

fn candidate(sequence: u64) -> MutationCandidate {
    MutationCandidate {
        id: format!("m1_{sequence}"),
        sequence,
        path: Utf8PathBuf::from("pkg/calc.py"),
        span: ByteSpan {
            start: sequence * 3,
            length: 1,
        },
        original: "+".into(),
        replacement: "-".into(),
        operator: "binary_add_sub".into(),
        line: 1,
        column: u32::try_from(sequence).unwrap(),
        symbol: None,
        file_hash: "00".repeat(32),
    }
}

fn protocol() -> AnalyzerProtocol {
    AnalyzerProtocol::new(EffectId(7))
}

fn analysis_request(id: u64, path: &str, final_target: bool, max_candidates: u64) -> AnalyzeFile {
    AnalyzeFile {
        id: EffectId(id),
        target: TargetSlice {
            path: path.into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
        final_target,
        max_candidates,
    }
}

fn handler(root: Utf8PathBuf) -> AnalyzerHandler {
    AnalyzerHandler::new(root).unwrap()
}

#[cfg(unix)]
fn create_dir_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_dir_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    match std::os::windows::fs::symlink_dir(target, link) {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.kind() == std::io::ErrorKind::Unsupported
                || error.raw_os_error() == Some(1314) =>
        {
            let output = std::process::Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()?;
            if output.status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(format!(
                    "failed to create Windows test junction: {}",
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        }
        Err(error) => Err(error),
    }
}

fn two_target_fixture() -> (
    tempfile::TempDir,
    Utf8PathBuf,
    Vec<TargetSlice>,
    MutationOperatorSelection,
) {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/first.py"),
        "def first():\n    return 1 + 2\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/second.py"),
        "def second():\n    return 3 + 4\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let targets = ["src/first.py", "src/second.py"]
        .into_iter()
        .map(|path| TargetSlice {
            path: path.into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        })
        .collect();
    (
        directory,
        root,
        targets,
        MutationOperatorSelection::default(),
    )
}

async fn replay_runtime_candidates(
    root: Utf8PathBuf,
    targets: Vec<TargetSlice>,
    operators: MutationOperatorSelection,
    profile: MutationProfile,
) -> Vec<MutationCandidate> {
    let mut handler = handler(root);
    let target_count = targets.len();
    let mut spool = None;
    for (index, target) in targets.into_iter().enumerate() {
        let finished = handler
            .handle(
                AnalyzeFile {
                    id: EffectId(u64::try_from(index).unwrap() + 100),
                    target,
                    final_target: index + 1 == target_count,
                    max_candidates: 100,
                },
                &operators,
                profile,
            )
            .await
            .unwrap();
        if finished.spool.is_some() {
            spool = finished.spool;
        }
    }
    let spool = spool.expect("the final target returns the candidate spool");
    let mut candidates = Vec::new();
    let mut offset = 0;
    while let Some((candidate, next_offset)) = CandidateStore::replay_one(&spool, offset).unwrap() {
        candidates.push(candidate);
        offset = next_offset;
    }
    candidates
}

#[tokio::test]
async fn in_memory_discovery_matches_runtime_candidate_descriptors() {
    let (_directory, root, targets, operators) = two_target_fixture();

    let planned = discover_targets(&root, &targets, &operators, MutationProfile::Focused, 100)
        .await
        .unwrap();
    let runtime =
        replay_runtime_candidates(root, targets, operators, MutationProfile::Focused).await;

    assert_eq!(planned.candidates, runtime);
    assert!(!planned.truncated);
}

#[tokio::test]
async fn analyzer_rejects_replaced_source_parent() {
    let project = tempfile::tempdir().unwrap();
    let source_parent = project.path().join("src");
    fs::create_dir(&source_parent).unwrap();
    fs::write(
        source_parent.join("calc.py"),
        "def selected():\n    return 1 + 2\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap();
    let mut analyzer = handler(root);
    let original_parent = project.path().join("original-src");
    fs::rename(&source_parent, &original_parent).unwrap();

    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    fs::write(&sentinel, b"outside sentinel").unwrap();
    fs::write(
        outside.path().join("calc.py"),
        "def outside_secret():\n    return left == right\n",
    )
    .unwrap();
    create_dir_link(outside.path(), &source_parent).unwrap();

    let result = analyzer
        .handle(
            analysis_request(82, "src/calc.py", true, 10),
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await;

    assert!(matches!(
        result,
        Err(error)
            if error.id == EffectId(82) && error.failure.code() == "analyzer.source.read"
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"outside sentinel");
}

#[tokio::test]
async fn discover_targets_rejects_linked_source_parent() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    fs::write(&sentinel, b"outside sentinel").unwrap();
    fs::write(
        outside.path().join("calc.py"),
        "def outside_secret():\n    return left == right\n",
    )
    .unwrap();
    let link = project.path().join("src");
    create_dir_link(outside.path(), &link).unwrap();
    let root = Utf8PathBuf::from_path_buf(project.path().to_owned()).unwrap();
    let targets = vec![TargetSlice {
        path: "src/calc.py".into(),
        lines: Vec::new(),
        symbols: Vec::new(),
    }];

    let result = discover_targets(
        &root,
        &targets,
        &MutationOperatorSelection::default(),
        MutationProfile::Full,
        10,
    )
    .await;

    assert!(matches!(
        result,
        Err(error)
            if error.id == EffectId(0) && error.failure.code() == "analyzer.source.read"
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"outside sentinel");
}

#[tokio::test]
async fn in_memory_discovery_stops_when_a_later_target_exceeds_the_global_limit() {
    let (_directory, root, targets, operators) = two_target_fixture();

    let discovery = discover_targets(&root, &targets, &operators, MutationProfile::Focused, 1)
        .await
        .unwrap();

    assert_eq!(
        discovery
            .candidates
            .iter()
            .map(|candidate| candidate.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/first.py"]
    );
    assert!(discovery.truncated);
    assert_eq!(
        discovery.diagnostics,
        vec![AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::CandidateLimitExceeded,
            path: Some("src/second.py".into()),
            line: None,
            column: None,
            message: None,
        }]
    );
}

#[test]
fn candidate_store_enforces_limit_without_retaining_records() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    assert!(matches!(
        store.push(&candidate(3)),
        Err(StoreError::LimitExceeded { limit: 2 })
    ));
    assert_eq!(store.count(), 2);
}

#[test]
fn replays_in_stable_offset_order() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let (first, second_offset) = CandidateStore::replay_one(&reference, 0).unwrap().unwrap();
    let (second, end_offset) = CandidateStore::replay_one(&reference, second_offset)
        .unwrap()
        .unwrap();
    assert_eq!((first, second), (candidate(1), candidate(2)));
    assert_eq!(
        CandidateStore::replay_one(&reference, end_offset).unwrap(),
        None
    );
}

#[test]
fn rejects_malformed_jsonl() {
    assert!(matches!(
        protocol().receive_line(br#"{"kind":"candidate"}"#),
        Err(ProtocolError::MalformedJson(_))
    ));
}

#[test]
fn preserves_helper_diagnostic() {
    let mut protocol = protocol();
    let record = protocol
        .receive_line(
            br#"{"kind":"diagnostic","effect_id":7,"code":"invalid_syntax","path":"pkg/calc.py"}"#,
        )
        .unwrap();
    assert_eq!(
        record,
        Some(AnalyzerRecord::Diagnostic(AnalyzerDiagnostic {
            code: AnalyzerDiagnosticCode::InvalidSyntax,
            path: Some(Utf8PathBuf::from("pkg/calc.py")),
            line: None,
            column: None,
            message: None
        }))
    );
}

#[test]
fn rejects_effect_id_mismatch() {
    let mut protocol = protocol();
    assert_eq!(protocol.receive_line(br#"{"kind":"summary","effect_id":8,"candidate_count":0,"diagnostic_count":0,"truncated":false}"#), Err(ProtocolError::EffectIdMismatch { expected: EffectId(7), actual: EffectId(8) }));
}

#[test]
fn rejects_record_after_summary() {
    let mut protocol = protocol();
    let line = br#"{"kind":"summary","effect_id":7,"candidate_count":0,"diagnostic_count":0,"truncated":false}"#;
    protocol.receive_line(line).unwrap();
    assert_eq!(
        protocol.receive_line(line),
        Err(ProtocolError::RecordAfterSummary)
    );
}

#[test]
fn rejects_unparseable_replacement() {
    let mut protocol = protocol();
    assert!(matches!(protocol.receive_line(br#"{"kind":"candidate","effect_id":7,"path":"pkg/calc.py","span":{"start":1,"length":1},"original":"+","replacement":{"bad":true},"operator":"binary_add_sub","line":1,"column":1,"symbol":null}"#), Err(ProtocolError::MalformedJson(_))));
}

#[test]
fn rejects_count_mismatch_and_missing_summary() {
    let mut mismatch = protocol();
    assert_eq!(mismatch.receive_line(br#"{"kind":"summary","effect_id":7,"candidate_count":1,"diagnostic_count":0,"truncated":false}"#), Err(ProtocolError::CountMismatch { expected_candidates: 0, actual_candidates: 1, expected_diagnostics: 0, actual_diagnostics: 0 }));
    assert_eq!(protocol().finish(), Err(ProtocolError::MissingSummary));
}

#[test]
fn rejects_oversized_line_before_parsing() {
    let mut protocol = AnalyzerProtocol::with_limits(EffectId(7), 8, 16);
    assert_eq!(
        protocol.receive_line(b"123456789"),
        Err(ProtocolError::LineTooLarge { limit: 8 })
    );
}

#[test]
fn finished_summary_is_returned_once() {
    let mut protocol = protocol();
    let record = protocol.receive_line(br#"{"kind":"summary","effect_id":7,"candidate_count":0,"diagnostic_count":0,"truncated":false}"#).unwrap();
    let summary = match record {
        Some(AnalyzerRecord::Summary(summary)) => summary,
        other => panic!("unexpected record: {other:?}"),
    };
    assert_eq!(protocol.finish().unwrap(), summary);
}

#[test]
fn rejects_candidate_missing_required_nullable_symbol() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"candidate","effect_id":7,"path":"pkg/calc.py","span":{"start":1,"length":1},"original":"+","replacement":"-","operator":"binary_add_sub","line":1,"column":1}"#),
        Err(ProtocolError::InvalidRecord("candidate symbol is required"))
    ));
}

#[test]
fn rejects_diagnostic_with_wrong_field_shape() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"diagnostic","effect_id":7,"code":"invalid_syntax"}"#),
        Err(ProtocolError::InvalidRecord("invalid diagnostic fields"))
    ));
}

#[test]
fn rejects_unknown_nested_span_field() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"candidate","effect_id":7,"path":"pkg/calc.py","span":{"start":1,"length":1,"end":2},"original":"+","replacement":"-","operator":"binary_add_sub","line":1,"column":1,"symbol":null}"#),
        Err(ProtocolError::MalformedJson(_))
    ));
}

#[test]
fn candidate_limit_is_a_typed_expected_completion() {
    use hoimin_cli::analyzer::AnalysisStatus;

    let mut protocol = protocol();
    protocol.receive_line(br#"{"kind":"diagnostic","effect_id":7,"code":"candidate_limit_exceeded","path":"pkg/calc.py"}"#).unwrap();
    let summary = protocol.receive_line(br#"{"kind":"summary","effect_id":7,"candidate_count":0,"diagnostic_count":1,"truncated":true}"#).unwrap();
    assert!(matches!(
        summary,
        Some(AnalyzerRecord::Summary(summary)) if summary.status() == AnalysisStatus::AnalysisLimitReached
    ));
}

#[test]
fn protocol_accepts_nested_colon_path_like_task4() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"candidate","effect_id":7,"path":"pkg/a:b.py","span":{"start":1,"length":1},"original":"+","replacement":"-","operator":"binary_add_sub","line":1,"column":1,"symbol":null}"#),
        Ok(Some(AnalyzerRecord::Candidate(_)))
    ));
}

#[test]
fn protocol_accepts_type_nullable_remove_candidate_from_json_output() {
    let mut protocol = protocol();
    let output = serde_json::json!({
        "kind": "candidate",
        "effect_id": 7,
        "path": "pkg/calc.py",
        "span": { "start": 1, "length": 1 },
        "original": "?",
        "replacement": "",
        "operator": "type_nullable_remove",
        "line": 1,
        "column": 1,
        "symbol": null
    });
    assert!(matches!(
        protocol.receive_line(&serde_json::to_vec(&output).unwrap()),
        Ok(Some(AnalyzerRecord::Candidate(_)))
    ));
}
#[test]
fn protocol_rejects_type_mapping_selector_alias() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"candidate","effect_id":7,"path":"pkg/calc.py","span":{"start":1,"length":1},"original":"?","replacement":"","operator":"type_mapping","line":1,"column":1,"symbol":null}"#),
        Err(ProtocolError::InvalidRecord("invalid candidate fields"))
    ));
}

#[test]
fn protocol_accepts_type_dict_mapping_candidate() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"candidate","effect_id":7,"path":"pkg/calc.py","span":{"start":1,"length":1},"original":"?","replacement":"","operator":"type_dict_mapping","line":1,"column":1,"symbol":null}"#),
        Ok(Some(AnalyzerRecord::Candidate(_)))
    ));
}
#[test]
fn candidate_rejects_null_field_from_another_kind() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"candidate","effect_id":7,"path":"pkg/calc.py","span":{"start":1,"length":1},"original":"+","replacement":"-","operator":"binary_add_sub","line":1,"column":1,"symbol":null,"code":null}"#),
        Err(ProtocolError::InvalidRecord(
            "candidate contains fields for another record kind"
        ))
    ));
}

#[test]
fn diagnostic_rejects_null_field_from_another_kind() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"diagnostic","effect_id":7,"code":"invalid_syntax","path":"pkg/calc.py","replacement":null}"#),
        Err(ProtocolError::InvalidRecord(
            "diagnostic contains fields for another record kind"
        ))
    ));
}

#[test]
fn summary_rejects_null_field_from_another_kind() {
    let mut protocol = protocol();
    assert!(matches!(
        protocol.receive_line(br#"{"kind":"summary","effect_id":7,"candidate_count":0,"diagnostic_count":0,"truncated":false,"path":null}"#),
        Err(ProtocolError::InvalidRecord(
            "summary contains fields for another record kind"
        ))
    ));
}

fn spool_lines(reference: &hoimin_core::CandidateSpoolRef) -> Vec<String> {
    fs::read_to_string(&reference.token)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn replay_rejects_duplicate_or_reversed_sequence() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let lines = spool_lines(&reference);

    fs::write(&reference.token, format!("{}\n{}\n", lines[0], lines[0])).unwrap();
    let (_, next) = CandidateStore::replay_one(&reference, 0).unwrap().unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, next),
        Err(StoreError::InvalidSequence {
            expected: 2,
            actual: 1
        })
    ));

    fs::write(&reference.token, format!("{}\n{}\n", lines[1], lines[0])).unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, 0),
        Err(StoreError::InvalidSequence {
            expected: 1,
            actual: 2
        })
    ));
}

#[test]
fn replay_rejects_sequence_gap() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let first = spool_lines(&reference).remove(0);
    let third = serde_json::to_string(&candidate(3)).unwrap();
    fs::write(&reference.token, format!("{first}\n{third}\n")).unwrap();

    let (_, next) = CandidateStore::replay_one(&reference, 0).unwrap().unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, next),
        Err(StoreError::InvalidSequence {
            expected: 2,
            actual: 3
        })
    ));
}

#[test]
fn replay_rejects_early_eof_before_reference_record_count() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let first = spool_lines(&reference).remove(0);
    fs::write(&reference.token, format!("{first}\n")).unwrap();

    let (_, end) = CandidateStore::replay_one(&reference, 0).unwrap().unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, end),
        Err(StoreError::UnexpectedEof {
            expected_records: 2,
            actual_records: 1
        })
    ));
}

#[test]
fn replay_rejects_truncated_record() {
    let mut store = CandidateStore::new(2).unwrap();
    store.push(&candidate(1)).unwrap();
    store.push(&candidate(2)).unwrap();
    let reference = store.finish().unwrap();
    let lines = spool_lines(&reference);
    fs::write(&reference.token, format!("{}\n{{", lines[0])).unwrap();

    let (_, next) = CandidateStore::replay_one(&reference, 0).unwrap().unwrap();
    assert!(matches!(
        CandidateStore::replay_one(&reference, next),
        Err(StoreError::UnexpectedEof {
            expected_records: 2,
            actual_records: 1
        })
    ));
}

#[tokio::test]
async fn concrete_handler_does_not_spawn_python_for_analysis() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/calc.py"),
        "def equal(left, right):\n    return left == right\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let mut handler = AnalyzerHandler::new(root).unwrap();
    let finished = handler
        .handle(
            AnalyzeFile {
                id: EffectId(77),
                target: TargetSlice {
                    path: "src/calc.py".into(),
                    lines: Vec::new(),
                    symbols: Vec::new(),
                },
                final_target: true,
                max_candidates: 10,
            },
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await
        .unwrap();
    let spool = finished.spool.unwrap();
    let (first_candidate, _) = CandidateStore::replay_one(&spool, 0).unwrap().unwrap();

    assert_eq!(spool.records, 1);
    assert_eq!(first_candidate.original, "==");
    assert_eq!(first_candidate.replacement, "!=");
}

#[tokio::test]
async fn concrete_handler_reports_invalid_syntax_with_the_source_path() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(directory.path().join("src/broken.py"), "def broken(:\n").unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let finished = handler(root)
        .handle(
            analysis_request(78, "src/broken.py", true, 10),
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await
        .unwrap();

    assert_eq!(finished.diagnostics.len(), 1);
    assert_eq!(finished.diagnostics[0].code, "analyzer.invalid_syntax");
    assert!(
        finished.diagnostics[0].message.contains("src/broken.py"),
        "{}",
        finished.diagnostics[0].message
    );
    assert!(
        finished.diagnostics[0]
            .message
            .contains("source could not be parsed"),
        "{}",
        finished.diagnostics[0].message
    );
}

#[tokio::test]
async fn concrete_handler_reports_source_read_failure() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let result = handler(root)
        .handle(
            analysis_request(78, "src/missing.py", true, 10),
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await;

    assert!(matches!(
        result,
        Err(error) if error.id == EffectId(78) && error.failure.code() == "analyzer.source.read"
    ));
}

#[tokio::test]
async fn concrete_handler_truncates_at_candidate_limit() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/calc.py"),
        "first = left == right\nsecond = top == bottom\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let finished = handler(root)
        .handle(
            analysis_request(79, "src/calc.py", true, 1),
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await
        .unwrap();
    assert_eq!(finished.diagnostics.len(), 1);
    assert_eq!(finished.diagnostics[0].code, "analyzer.candidate_limit");
    assert!(
        finished.diagnostics[0].message.contains("src/calc.py"),
        "{}",
        finished.diagnostics[0].message
    );
    let spool = finished.spool.unwrap();
    let (candidate, offset) = CandidateStore::replay_one(&spool, 0).unwrap().unwrap();

    assert!(finished.truncated);
    assert_eq!(spool.records, 1);
    assert_eq!(candidate.sequence, 1);
    assert_eq!(CandidateStore::replay_one(&spool, offset).unwrap(), None);
}

#[tokio::test]
async fn concrete_handler_finishes_spool_when_a_non_final_target_is_truncated() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/calc.py"),
        "first = left == right\nsecond = top == bottom\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let finished = handler(root)
        .handle(
            analysis_request(80, "src/calc.py", false, 1),
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await
        .unwrap();
    let spool = finished.spool.unwrap();

    assert!(finished.truncated);
    assert_eq!(spool.records, 1);
}

#[tokio::test]
async fn concrete_handler_spools_multiple_requests_on_final_target() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/first.py"),
        "first = left == right\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/second.py"),
        "second = top == bottom\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let mut handler = handler(root);

    let first = handler
        .handle(
            analysis_request(80, "src/first.py", false, 10),
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await
        .unwrap();
    let second = handler
        .handle(
            analysis_request(81, "src/second.py", true, 10),
            &MutationOperatorSelection::default(),
            MutationProfile::Full,
        )
        .await
        .unwrap();
    let spool = second.spool.unwrap();
    let (first_candidate, second_offset) = CandidateStore::replay_one(&spool, 0).unwrap().unwrap();
    let (second_candidate, end_offset) = CandidateStore::replay_one(&spool, second_offset)
        .unwrap()
        .unwrap();

    assert_eq!(first.spool, None);
    assert_eq!(spool.records, 2);
    assert_eq!(
        (first_candidate.sequence, first_candidate.path.as_str()),
        (1, "src/first.py")
    );
    assert_eq!(
        (second_candidate.sequence, second_candidate.path.as_str()),
        (2, "src/second.py")
    );
    assert_eq!(
        CandidateStore::replay_one(&spool, end_offset).unwrap(),
        None
    );
}
