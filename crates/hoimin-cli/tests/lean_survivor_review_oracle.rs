use camino::Utf8PathBuf;
use hoimin_cli::review::model::*;
use hoimin_core::{ByteSpan, CandidateDescriptor, CandidateIdentity, stable_mutant_id};
use serde::Deserialize;

const CORPUS: &str = include_str!("../../../formal/HoiminOracle/corpus/survivor-review.jsonl");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u32,
    difference: String,
    source: u8,
    current: u64,
    expected: u64,
    applicability: String,
    result: String,
    next_revision: u64,
    history: Vec<u64>,
}

#[test]
fn pure_review_matches_lean_identity_and_history_cases() {
    let key = fixture_key();
    let draft = ReviewDraft {
        assessment: Assessment::Actionable,
        reason: "missing field".into(),
        evidence: vec![],
        timestamp_ms: 0,
        plan: ArtifactReference {
            path: "plan.json".into(),
            content_hash: "a".repeat(64),
            run_id: None,
        },
        report: None,
    };
    assert!(CORPUS.lines().count() >= 100);
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        let mut stored = key.clone();
        match case.difference.as_str() {
            "same" => (),
            "project" => stored.project = "/other".into(),
            "id" => stored.candidate_id = "0".repeat(64),
            "schema" => stored.descriptor.schema_version = 2,
            "path" => stored.descriptor.path = "other.py".into(),
            "span" => stored.descriptor.span.start = 1,
            "length" => stored.descriptor.span.length = 2,
            "original" => stored.descriptor.original = "9".into(),
            "replacement" => stored.descriptor.replacement = "3".into(),
            "operator" => stored.descriptor.operator = "other".into(),
            "line" => stored.descriptor.line = 2,
            "column" => stored.descriptor.column = 1,
            "symbol" => stored.descriptor.symbol = Some("f".into()),
            "hash" => stored.descriptor.file_hash = "b".repeat(64),
            _ => panic!("unknown difference"),
        }
        let source = match case.source {
            0 => SourceState::Fresh,
            1 => SourceState::Stale,
            2 => SourceState::Unavailable,
            _ => panic!("unknown source"),
        };
        let applicable = applicability(&key, &stored, source);
        assert_eq!(
            serde_json::to_value(applicable).unwrap(),
            case.applicability,
            "{line}"
        );
        let state = ReviewState {
            history: if case.current == 0 {
                vec![]
            } else {
                vec![ReviewEvent {
                    revision: case.current,
                    draft: draft.clone(),
                }]
            },
        };
        let (result, next) = match apply_update(&state, applicable, case.expected, draft.clone()) {
            Ok(next) => ("accepted", next),
            Err(reason) => (
                match reason {
                    UpdateRejection::NotCurrent => "not_current",
                    UpdateRejection::RevisionConflict => "revision_conflict",
                    UpdateRejection::RevisionOverflow => "revision_overflow",
                },
                state.clone(),
            ),
        };
        assert_eq!(result, case.result, "{line}");
        assert_eq!(
            next.history.last().map_or(0, |event| event.revision),
            case.next_revision,
            "{line}"
        );
        assert_eq!(
            next.history
                .iter()
                .map(|event| event.revision)
                .collect::<Vec<_>>(),
            case.history,
            "{line}"
        );
        if result == "accepted" {
            assert_eq!(
                &next.history[..state.history.len()],
                state.history.as_slice()
            );
        } else {
            assert_eq!(next, state);
        }
    }
}

fn fixture_key() -> ReviewKey {
    let descriptor = CandidateDescriptor {
        schema_version: 1,
        path: "sample.py".into(),
        span: ByteSpan {
            start: 0,
            length: 1,
        },
        original: "1".into(),
        replacement: "2".into(),
        operator: "integer_literal".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: blake3::hash(b"1\n").to_hex().to_string(),
    };
    ReviewKey {
        project: Utf8PathBuf::from("/project"),
        candidate_id: stable_mutant_id(&CandidateIdentity::from(&descriptor)).to_string(),
        descriptor,
    }
}
