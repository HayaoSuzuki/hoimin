use camino::Utf8PathBuf;
use hoimin_core::{
    BudgetLedger, EffectFailed, EffectFailure, EffectId, IntegrityCheckpoint, OriginalsVerified,
    Preflight, PreflightCompleted, RunBudgets, RunEffect, RunEvent, VerifyOriginals,
    reserve_workspace_copy,
};

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

#[test]
fn original_integrity_checkpoints_have_typed_effect_and_completion_events() {
    for (sequence, checkpoint) in [
        IntegrityCheckpoint::PreAnalysis,
        IntegrityCheckpoint::Periodic,
        IntegrityCheckpoint::PreFinalReport,
        IntegrityCheckpoint::Cleanup,
    ]
    .into_iter()
    .enumerate()
    {
        let id = EffectId(sequence as u64);
        let effect = RunEffect::VerifyOriginals(VerifyOriginals { id, checkpoint });
        let event = RunEvent::OriginalsVerified(OriginalsVerified { id, checkpoint });
        assert!(matches!(effect, RunEffect::VerifyOriginals(_)));
        assert!(matches!(event, RunEvent::OriginalsVerified(_)));
    }
}
