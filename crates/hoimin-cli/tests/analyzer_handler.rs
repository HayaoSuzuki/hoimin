use camino::Utf8PathBuf;
use hoimin_cli::analyzer::{
    AnalyzerDiagnostic, AnalyzerDiagnosticCode, AnalyzerHandler, AnalyzerProtocol, AnalyzerRecord,
    CandidateStore, ProtocolError, StoreError,
};
use hoimin_core::{AnalyzeFile, ByteSpan, EffectId, MutationCandidate, TargetSlice};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
async fn concrete_handler_rejects_a_huge_stdout_line_without_unbounded_retention() {
    let (directory, mut handler) = fault_handler(
        "import json,sys\njson.loads(sys.stdin.readline())\nsys.stdout.write('x' * (2 * 1024 * 1024 + 1) + '\\n')\nsys.stdout.flush()\n",
        Duration::from_secs(5),
        4096,
    );
    let error = handler.handle(fault_request()).await.unwrap_err();
    assert_eq!(error.failure.code(), "analyzer.protocol");
    drop(directory);
}

#[tokio::test]
async fn concrete_handler_bounds_stderr_even_when_the_helper_exits() {
    let (directory, mut handler) = fault_handler(
        "import json,sys\njson.loads(sys.stdin.readline())\nsys.stderr.write('e' * 2048)\nsys.stderr.flush()\n",
        Duration::from_secs(5),
        1024,
    );
    let error = handler.handle(fault_request()).await.unwrap_err();
    assert_eq!(error.failure.code(), "analyzer.stderr.limit");
    drop(directory);
}

#[tokio::test]
async fn concrete_handler_times_out_and_reaps_a_helper_with_a_child_process() {
    let (directory, mut handler) = fault_handler(
        "import json,subprocess,sys,time\njson.loads(sys.stdin.readline())\nsubprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])\ntime.sleep(60)\n",
        Duration::from_millis(100),
        4096,
    );
    let started = Instant::now();
    let error = handler.handle(fault_request()).await.unwrap_err();
    assert_eq!(error.failure.code(), "analyzer.timeout");
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(directory);
}

#[tokio::test]
async fn concrete_handler_times_out_while_helper_refuses_large_stdin() {
    let (directory, mut handler) = fault_handler(
        "import time\ntime.sleep(60)\n",
        Duration::from_millis(100),
        4096,
    );
    fs::write(
        directory.path().join("src/calc.py"),
        format!("# {}\n", "x".repeat(4 * 1024 * 1024)),
    )
    .unwrap();
    let started = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(5), handler.handle(fault_request()))
        .await
        .expect("analyzer timeout must supervise stdin delivery");
    let error = result.unwrap_err();

    assert_eq!(error.failure.code(), "analyzer.timeout");
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(directory);
}

fn fault_handler(
    helper: &str,
    timeout: Duration,
    max_output: u64,
) -> (tempfile::TempDir, AnalyzerHandler) {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/calc.py"),
        "def add(a, b):\n    return a + b\n",
    )
    .unwrap();
    let root = Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap();
    let handler = AnalyzerHandler::with_helper_source_for_tests(
        root,
        Utf8PathBuf::from_path_buf(python_executable()).unwrap(),
        timeout,
        max_output,
        helper,
    )
    .unwrap();
    (directory, handler)
}

fn fault_request() -> AnalyzeFile {
    AnalyzeFile {
        id: EffectId(77),
        target: TargetSlice {
            path: "src/calc.py".into(),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
        final_target: true,
        max_candidates: 10,
    }
}

fn python_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("HOIMIN_TEST_PYTHON") {
        return path.into();
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    if cfg!(windows) {
        repository.join(".venv/Scripts/python.exe")
    } else {
        repository.join(".venv/bin/python")
    }
}
