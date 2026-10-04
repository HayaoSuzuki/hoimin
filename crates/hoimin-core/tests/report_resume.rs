use hoimin_core::{ResumeFreshReason, ResumeOutcome, SessionLoaded};
use serde_json::json;

#[test]
fn resume_metadata_has_stable_tagged_encoding_and_legacy_events_default() {
    let legacy: SessionLoaded = serde_json::from_value(json!({"id":1,"resume":null})).unwrap();
    assert!(legacy.fresh_reason.is_none());
    assert_eq!(
        serde_json::to_value(ResumeOutcome::Resumed).unwrap(),
        json!({"status":"resumed"})
    );
    for reason in [
        ResumeFreshReason::NoPriorRun,
        ResumeFreshReason::MatchingRunComplete,
        ResumeFreshReason::BudgetDecreased,
        ResumeFreshReason::FingerprintMismatch,
        ResumeFreshReason::NoIncompleteRun,
        ResumeFreshReason::CandidateChanged,
        ResumeFreshReason::NoCompatibleRun,
    ] {
        let outcome = ResumeOutcome::Fresh { reason };
        let value = serde_json::to_value(outcome).unwrap();
        assert_eq!(value, json!({"status":"fresh","reason":reason.code()}));
        assert_eq!(
            serde_json::from_value::<ResumeOutcome>(value).unwrap(),
            outcome
        );
        assert_ne!(reason.explanation(), "");
    }
    assert!(
        serde_json::from_value::<ResumeOutcome>(json!({"status":"fresh","reason":"unknown"}))
            .is_err()
    );
}
