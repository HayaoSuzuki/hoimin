use camino::Utf8PathBuf;
use hoimin_core::{
    EffectFailed, EffectFailure, EffectId, IntegrityCheckpoint, OriginalsVerified, RunEffect,
    RunEvent, VerifyOriginals,
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
