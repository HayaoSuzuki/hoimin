use camino::Utf8PathBuf;
use hoimin_cli::review::model::*;
use hoimin_core::{ByteSpan, CandidateDescriptor, CandidateIdentity, stable_mutant_id};
use serde::Deserialize;

type KeyMutation = (&'static str, fn(&mut ReviewKey));

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
        let differences: [KeyMutation; 14] = [
            ("same", |_| ()),
            ("project", |key| key.project = "/other".into()),
            ("id", |key| key.candidate_id = "0".repeat(64)),
            ("schema", |key| key.descriptor.schema_version = 2),
            ("path", |key| key.descriptor.path = "other.py".into()),
            ("span", |key| key.descriptor.span.start = 1),
            ("length", |key| key.descriptor.span.length = 2),
            ("original", |key| key.descriptor.original = "9".into()),
            ("replacement", |key| key.descriptor.replacement = "3".into()),
            ("operator", |key| key.descriptor.operator = "other".into()),
            ("line", |key| key.descriptor.line = 2),
            ("column", |key| key.descriptor.column = 1),
            ("symbol", |key| key.descriptor.symbol = Some("f".into())),
            ("hash", |key| key.descriptor.file_hash = "b".repeat(64)),
        ];
        differences
            .iter()
            .find(|(name, _)| *name == case.difference)
            .expect("registered difference")
            .1(&mut stored);
        let source = [
            SourceState::Fresh,
            SourceState::Stale,
            SourceState::Unavailable,
        ][usize::from(case.source)];
        let applicable = applicability(&key, &stored, source);
        assert_eq!(
            serde_json::to_value(applicable).unwrap(),
            case.applicability,
            "{line}"
        );
        let state = ReviewState {
            history: (case.current != 0)
                .then_some(ReviewEvent {
                    revision: case.current,
                    draft: draft.clone(),
                })
                .into_iter()
                .collect(),
        };
        let mut expected_history = state.history.clone();
        expected_history.extend((case.result == "accepted").then_some(ReviewEvent {
            revision: case.next_revision,
            draft: draft.clone(),
        }));

        let (result, next) = apply_update(&state, applicable, case.expected, draft.clone())
            .map_or_else(
                |reason| (rejection_name(reason), state.clone()),
                |next| ("accepted", next),
            );
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
        assert_eq!(
            next,
            ReviewState {
                history: expected_history
            }
        );
    }
}

fn rejection_name(reason: UpdateRejection) -> &'static str {
    match reason {
        UpdateRejection::NotCurrent => "not_current",
        UpdateRejection::RevisionConflict => "revision_conflict",
        UpdateRejection::RevisionOverflow => "revision_overflow",
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
