use hoimin_core::{
    EffectId, ResumeFreshReason, ResumeOutcome, SessionLoadOutcome, SessionLoaded, SessionResumeRef,
};
use serde_json::json;

#[test]
fn session_loaded_rejects_legacy_or_missing_outcomes() {
    for value in [
        json!({"id":1,"resume":null}),
        json!({"id":1,"resume":{"run_id":"saved"}}),
        json!({"id":1,"resume":null,"fresh_reason":"no_prior_run"}),
        json!({"id":1}),
    ] {
        assert!(
            serde_json::from_value::<SessionLoaded>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn resume_metadata_has_stable_tagged_encoding() {
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

#[test]
fn session_loaded_round_trips_every_outcome_and_preserves_effect_and_run_ids() {
    let resumed = SessionLoaded {
        id: EffectId(17),
        outcome: SessionLoadOutcome::Resumed(SessionResumeRef {
            run_id: "saved-run".into(),
        }),
    };
    let value = serde_json::to_value(&resumed).unwrap();
    assert_eq!(
        value,
        json!({"id":17,"outcome":{"Resumed":{"run_id":"saved-run"}}})
    );
    assert_eq!(
        serde_json::from_value::<SessionLoaded>(value).unwrap(),
        resumed
    );
    for reason in [
        ResumeFreshReason::NoPriorRun,
        ResumeFreshReason::MatchingRunComplete,
        ResumeFreshReason::BudgetDecreased,
        ResumeFreshReason::FingerprintMismatch,
        ResumeFreshReason::NoIncompleteRun,
        ResumeFreshReason::CandidateChanged,
    ] {
        let loaded = SessionLoaded {
            id: EffectId(18),
            outcome: SessionLoadOutcome::Fresh(reason),
        };
        let value = serde_json::to_value(&loaded).unwrap();
        assert_eq!(value, json!({"id":18,"outcome":{"Fresh":reason.code()}}));
        assert_eq!(
            serde_json::from_value::<SessionLoaded>(value).unwrap(),
            loaded
        );
    }
}

#[test]
fn session_loaded_rejects_ambiguous_or_incomplete_payloads() {
    for value in [
        json!({"id":1,"outcome":null}),
        json!({"id":1,"outcome":{"Fresh":null}}),
        json!({"id":1,"outcome":{"Fresh":"no_compatible_run"}}),
        json!({"id":1,"outcome":{"Resumed":{}}}),
        json!({"id":1,"outcome":{"Fresh":"no_prior_run","Resumed":{"run_id":"saved"}}}),
        json!({"id":1,"outcome":{"Fresh":"no_prior_run"},"resume":null}),
        json!({"id":1,"outcome":{"Resumed":{"run_id":"saved"}},"fresh_reason":"no_prior_run"}),
    ] {
        assert!(
            serde_json::from_value::<SessionLoaded>(value.clone()).is_err(),
            "{value}"
        );
    }
    assert!(
        serde_json::from_value::<ResumeOutcome>(
            json!({"status":"fresh","reason":"no_compatible_run"})
        )
        .is_err()
    );
}
