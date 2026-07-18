use camino::Utf8PathBuf;
use hoimin_cli::analyzer::{
    AnalyzerDiagnostic, AnalyzerDiagnosticCode, AnalyzerProtocol, AnalyzerRecord, CandidateStore,
    ProtocolError, StoreError,
};
use hoimin_core::{ByteSpan, EffectId, MutationCandidate};

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
