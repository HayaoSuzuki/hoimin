use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, CandidateDescriptor, CandidateIdentity, stable_mutant_id};

use super::model::*;

fn key() -> ReviewKey {
    let descriptor = CandidateDescriptor {
        schema_version: 1,
        path: Utf8PathBuf::from("sample.py"),
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

fn draft() -> ReviewDraft {
    ReviewDraft {
        assessment: Assessment::Actionable,
        reason: "required field missing".into(),
        evidence: vec![],
        timestamp_ms: 0,
        plan: ArtifactReference {
            path: "plan.json".into(),
            content_hash: "a".repeat(64),
            run_id: None,
        },
        report: None,
    }
}

#[test]
fn identity_and_update_contract() {
    let original = key();
    assert_eq!(
        applicability(&original, &original, SourceState::Fresh),
        Applicability::Current
    );
    assert_eq!(
        applicability(&original, &original, SourceState::Stale),
        Applicability::Stale
    );
    assert_eq!(
        applicability(&original, &original, SourceState::Unavailable),
        Applicability::SourceUnavailable
    );
    let mut other = original.clone();
    other.project = "/another".into();
    assert_eq!(
        applicability(&original, &other, SourceState::Fresh),
        Applicability::IdentityConflict
    );
    other = original.clone();
    other.descriptor.original = "9".into();
    assert_eq!(
        applicability(&original, &other, SourceState::Fresh),
        Applicability::IdentityConflict
    );
    let empty = ReviewState::default();
    let first = apply_update(&empty, Applicability::Current, 0, draft()).unwrap();
    assert_eq!(first.history[0].revision, 1);
    assert!(empty.history.is_empty());
    assert_eq!(
        apply_update(&first, Applicability::Current, 0, draft()),
        Err(UpdateRejection::RevisionConflict)
    );
    assert_eq!(
        apply_update(&first, Applicability::Stale, 1, draft()),
        Err(UpdateRejection::NotCurrent)
    );
    let second = apply_update(&first, Applicability::Current, 1, draft()).unwrap();
    assert_eq!(&second.history[..1], first.history.as_slice());
    assert_eq!(second.history[1].revision, 2);
    let overflow = ReviewState {
        history: vec![ReviewEvent {
            revision: u64::MAX,
            draft: draft(),
        }],
    };
    assert_eq!(
        apply_update(&overflow, Applicability::Current, u64::MAX, draft()),
        Err(UpdateRejection::RevisionOverflow)
    );
}

#[test]
fn draft_limits() {
    let mut value = draft();
    value.reason = "x".repeat(4096);
    value.evidence = vec!["x".repeat(2048); 8];
    assert!(validate_draft(&value).is_ok());
    value.reason.push('x');
    assert!(validate_draft(&value).is_err());
    for invalid in [
        " \n\t".to_string(),
        "\x1b[31m".to_string(),
        "\0".to_string(),
        "あ".repeat(1366),
    ] {
        value.reason = invalid;
        assert!(validate_draft(&value).is_err());
    }
    value.reason = "reason\n\tmore".into();
    value.evidence.push("extra".into());
    assert!(validate_draft(&value).is_err());
    value.evidence = vec!["x".repeat(2049)];
    assert!(validate_draft(&value).is_err());
    value.evidence.clear();
    assert!(validate_draft(&value).is_ok());
}
