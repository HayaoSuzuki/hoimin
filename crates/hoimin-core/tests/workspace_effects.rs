use camino::Utf8PathBuf;
use hoimin_core::{
    BudgetLedger, EffectFailed, EffectFailure, EffectId, IntegrityCheckpoint,
    ObserveRemainingBudget, OriginalsVerified, Preflight, PreflightCompleted,
    RemainingBudgetObserved, RunBudgets, RunEffect, RunEvent, VerifyOriginals,
    reserve_workspace_copy,
};
use std::time::Duration;

#[test]
fn workspace_failures_are_machine_readable_and_keep_the_effect_id() {
    let failed = EffectFailed {
        id: EffectId(40),
        failure: EffectFailure::WorkspaceRestore {
            path: Utf8PathBuf::from("pkg/a.py"),
            message: "access denied".into(),
        },
    };

    assert_eq!(failed.id, EffectId(40));
    assert_eq!(failed.failure.code(), "workspace.restore");
    assert!(matches!(
        failed.failure,
        EffectFailure::WorkspaceRestore { .. }
    ));
}

#[test]
fn run_effect_deserialization_preserves_messages_but_rejects_worker_capabilities() {
    let ordinary = RunEffect::Preflight(Preflight { id: EffectId(50) });
    let ordinary_json = serde_json::to_string(&ordinary).unwrap();

    assert_eq!(
        serde_json::from_str::<RunEffect>(&ordinary_json).unwrap(),
        ordinary
    );

    let preflight = PreflightCompleted {
        id: EffectId(51),
        per_worker_logical_bytes: 5,
        requested_workers: 1,
        aggregate_logical_bytes: 5,
        fingerprint: None,
    };
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 1,
        copy: 5,
        processes: 1,
    });
    let capability = reserve_workspace_copy(&mut ledger, &preflight)
        .unwrap()
        .create_worker(EffectId(52), 0)
        .unwrap();
    let capability_json = serde_json::to_string(&RunEffect::CreateWorker(capability)).unwrap();

    assert!(serde_json::from_str::<RunEffect>(&capability_json).is_err());
}

#[rstest::rstest]
#[case::pre_analysis(0, IntegrityCheckpoint::PreAnalysis)]
#[case::post_materialization(1, IntegrityCheckpoint::PostMaterialization)]
#[case::periodic(2, IntegrityCheckpoint::Periodic)]
#[case::pre_final_report(3, IntegrityCheckpoint::PreFinalReport)]
#[case::cleanup(4, IntegrityCheckpoint::Cleanup)]
fn original_integrity_checkpoints_have_typed_effect_and_completion_events(
    #[case] sequence: u64,
    #[case] checkpoint: IntegrityCheckpoint,
) {
    let id = EffectId(sequence);

    let effect = RunEffect::VerifyOriginals(VerifyOriginals { id, checkpoint });
    let event = RunEvent::OriginalsVerified(OriginalsVerified { id, checkpoint });

    assert!(matches!(effect, RunEffect::VerifyOriginals(_)));
    assert!(matches!(event, RunEvent::OriginalsVerified(_)));
}

#[test]
fn remaining_budget_observation_round_trips_with_effect_identity() {
    let id = EffectId(61);
    let effect = RunEffect::ObserveRemainingBudget(ObserveRemainingBudget { id });
    let event = RunEvent::RemainingBudgetObserved(RemainingBudgetObserved {
        id,
        remaining: Duration::from_secs(281),
    });

    let effect_json = serde_json::to_string(&effect).unwrap();
    let event_json = serde_json::to_string(&event).unwrap();

    assert_eq!(
        serde_json::from_str::<RunEffect>(&effect_json).unwrap(),
        effect
    );
    assert_eq!(effect.id(), id);
    assert_eq!(
        serde_json::from_str::<RunEvent>(&event_json).unwrap(),
        event
    );
}

#[test]
fn analysis_effect_rejects_obsolete_per_request_candidate_limit() {
    // Arrange: an otherwise valid file request carries the removed session setting.
    let json = serde_json::json!({
        "id": 62,
        "target": {"path": "src/calc.py", "lines": [], "symbols": []},
        "final_target": true,
        "max_candidates": 10,
    });

    // Act.
    let decoded = serde_json::from_value::<hoimin_core::AnalyzeFile>(json);

    // Assert: accepting it would permit callers to believe per-file overrides apply.
    assert!(decoded.is_err());
}
