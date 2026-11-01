use hoimin_core::{
    EffectId, ResumeFreshReason, ResumeOutcome, SessionLoadOutcome, SessionLoaded, SessionResumeRef,
};
use serde_json::json;

#[rstest::rstest]
#[case::json_id_1_resume_null(json!({"id":1,"resume":null}))]
#[case::json_id_1_resume_run_id_saved(json!({"id":1,"resume":{"run_id":"saved"}}))]
#[case::json_id_1_resume_null_fresh_reason_no_prior_run(json!({"id":1,"resume":null,"fresh_reason":"no_prior_run"}))]
#[case::json_id_1(json!({"id":1}))]
fn session_loaded_rejects_legacy_or_missing_outcomes(#[case] value: serde_json::Value) {
    assert!(
        serde_json::from_value::<SessionLoaded>(value.clone()).is_err(),
        "{value}"
    );
}

#[test]
fn resumed_metadata_has_stable_tagged_encoding() {
    let value = serde_json::to_value(ResumeOutcome::Resumed).unwrap();

    assert_eq!(value, json!({"status":"resumed"}));
}

#[rstest::rstest]
#[case::no_prior_run(ResumeFreshReason::NoPriorRun)]
#[case::matching_run_complete(ResumeFreshReason::MatchingRunComplete)]
#[case::budget_decreased(ResumeFreshReason::BudgetDecreased)]
#[case::fingerprint_mismatch(ResumeFreshReason::FingerprintMismatch)]
#[case::no_incomplete_run(ResumeFreshReason::NoIncompleteRun)]
#[case::candidate_changed(ResumeFreshReason::CandidateChanged)]
fn fresh_metadata_has_stable_tagged_encoding(#[case] reason: ResumeFreshReason) {
    let outcome = ResumeOutcome::Fresh { reason };

    let value = serde_json::to_value(outcome).unwrap();

    let restored = serde_json::from_value::<ResumeOutcome>(value.clone()).unwrap();

    assert_eq!(value, json!({"status":"fresh","reason":reason.code()}));
    assert_eq!(restored, outcome);
    assert_ne!(reason.explanation(), "");
}

#[test]
fn unknown_fresh_reason_is_rejected() {
    let wire = json!({"status":"fresh","reason":"unknown"});

    let result = serde_json::from_value::<ResumeOutcome>(wire);

    assert!(result.is_err());
}

#[test]
fn resumed_session_loaded_preserves_effect_and_run_ids() {
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
}

#[rstest::rstest]
#[case::resumefreshreason_nopriorrun(ResumeFreshReason::NoPriorRun)]
#[case::resumefreshreason_matchingruncomplete(ResumeFreshReason::MatchingRunComplete)]
#[case::resumefreshreason_budgetdecreased(ResumeFreshReason::BudgetDecreased)]
#[case::resumefreshreason_fingerprintmismatch(ResumeFreshReason::FingerprintMismatch)]
#[case::resumefreshreason_noincompleterun(ResumeFreshReason::NoIncompleteRun)]
#[case::resumefreshreason_candidatechanged(ResumeFreshReason::CandidateChanged)]
fn session_loaded_round_trips_every_outcome_and_preserves_effect_and_run_ids(
    #[case] reason: ResumeFreshReason,
) {
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

#[rstest::rstest]
#[case::json_id_1_outcome_null(json!({"id":1,"outcome":null}))]
#[case::json_id_1_outcome_fresh_null(json!({"id":1,"outcome":{"Fresh":null}}))]
#[case::json_id_1_outcome_fresh_no_compatible_run(json!({"id":1,"outcome":{"Fresh":"no_compatible_run"}}))]
#[case::json_id_1_outcome_resumed(json!({"id":1,"outcome":{"Resumed":{}}}))]
#[case::json_id_1_outcome_fresh_no_prior_run_resumed_run_id_saved(json!({"id":1,"outcome":{"Fresh":"no_prior_run","Resumed":{"run_id":"saved"}}}))]
#[case::json_id_1_outcome_fresh_no_prior_run_resume_null(json!({"id":1,"outcome":{"Fresh":"no_prior_run"},"resume":null}))]
#[case::json_id_1_outcome_resumed_run_id_saved_fresh_reason_no_prior_run(json!({"id":1,"outcome":{"Resumed":{"run_id":"saved"}},"fresh_reason":"no_prior_run"}))]
fn session_loaded_rejects_ambiguous_or_incomplete_payloads_table_1(
    #[case] value: serde_json::Value,
) {
    assert!(
        serde_json::from_value::<SessionLoaded>(value.clone()).is_err(),
        "{value}"
    );
}

#[test]
fn session_loaded_rejects_ambiguous_or_incomplete_payloads() {
    assert!(
        serde_json::from_value::<ResumeOutcome>(
            json!({"status":"fresh","reason":"no_compatible_run"})
        )
        .is_err()
    );
}
