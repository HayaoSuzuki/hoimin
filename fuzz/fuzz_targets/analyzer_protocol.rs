#![no_main]

// Compile the production module directly so the harness needs no shipping API.
#[allow(dead_code)]
#[path = "../../crates/hoimin-cli/src/analyzer/protocol.rs"]
mod protocol;

use hoimin_core::EffectId;
use libfuzzer_sys::fuzz_target;
use protocol::{AnalyzerProtocol, AnalyzerRecord, ProtocolError};
use serde_json::json;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let mut raw = AnalyzerProtocol::with_limits(EffectId(7), 4096, 8192);
    let (mut candidates, mut diagnostics) = (0, 0);
    let mut summary = None;
    for line in data.split_inclusive(|byte| *byte == b'\n') {
        match raw.receive_line(line) {
            Ok(Some(AnalyzerRecord::Candidate(_))) => candidates += 1,
            Ok(Some(AnalyzerRecord::Diagnostic(_))) => diagnostics += 1,
            Ok(Some(AnalyzerRecord::Summary(record))) => {
                assert_eq!(record.candidate_count, candidates);
                assert_eq!(record.diagnostic_count, diagnostics);
                summary = Some(record);
            }
            Ok(None) => panic!("non-empty protocol records must produce an observation"),
            Err(_) => break,
        }
    }
    assert_eq!(raw.finish(), summary.ok_or(ProtocolError::MissingSummary));

    // Construct valid records from arbitrary payloads so the stateful checks
    // remain exercised even when raw fuzz bytes are not valid JSON.
    let original = String::from_utf8_lossy(data);
    let replacement = format!("{original}x");
    let candidate = serde_json::to_vec(&json!({
        "kind": "candidate", "effect_id": 7, "path": "input.py",
        "span": {"start": 1, "length": original.len()},
        "original": original, "replacement": replacement,
        "operator": "binary_add_sub", "line": 1, "column": 1, "symbol": null
    }))
    .unwrap();
    let count = u64::from(data.first().copied().unwrap_or(0) % 8);
    let summary = serde_json::to_vec(&json!({
        "kind": "summary", "effect_id": 7, "candidate_count": count,
        "diagnostic_count": 0, "truncated": false
    }))
    .unwrap();
    let mut valid = AnalyzerProtocol::new(EffectId(7));
    for _ in 0..count {
        assert!(matches!(
            valid.receive_line(&candidate),
            Ok(Some(AnalyzerRecord::Candidate(_)))
        ));
    }
    assert!(matches!(
        valid.receive_line(&summary),
        Ok(Some(AnalyzerRecord::Summary(_)))
    ));
    assert_eq!(
        valid.receive_line(&candidate),
        Err(ProtocolError::RecordAfterSummary)
    );
    assert_eq!(valid.finish().unwrap().candidate_count, count);

    let mut wrong_id = AnalyzerProtocol::new(EffectId(8));
    assert!(matches!(
        wrong_id.receive_line(&candidate),
        Err(ProtocolError::EffectIdMismatch { .. })
    ));
    let mut wrong_count = AnalyzerProtocol::new(EffectId(7));
    for _ in 0..=count {
        wrong_count.receive_line(&candidate).unwrap();
    }
    assert!(matches!(
        wrong_count.receive_line(&summary),
        Err(ProtocolError::CountMismatch { .. })
    ));
    let mut missing = AnalyzerProtocol::new(EffectId(7));
    missing.receive_line(&candidate).unwrap();
    assert_eq!(missing.finish(), Err(ProtocolError::MissingSummary));

    let mut reversed = AnalyzerProtocol::new(EffectId(7));
    reversed.receive_line(&candidate).unwrap();
    let mut earlier: serde_json::Value = serde_json::from_slice(&candidate).unwrap();
    earlier["span"]["start"] = json!(0);
    assert_eq!(
        reversed.receive_line(&serde_json::to_vec(&earlier).unwrap()),
        Err(ProtocolError::CandidateOrder)
    );

    let mut short_line =
        AnalyzerProtocol::with_limits(EffectId(7), candidate.len() - 1, usize::MAX);
    assert!(matches!(
        short_line.receive_line(&candidate),
        Err(ProtocolError::LineTooLarge { .. })
    ));
    let mut short_output =
        AnalyzerProtocol::with_limits(EffectId(7), candidate.len(), candidate.len());
    short_output.receive_line(&candidate).unwrap();
    assert!(matches!(
        short_output.receive_line(&candidate),
        Err(ProtocolError::OutputTooLarge { .. })
    ));
});
