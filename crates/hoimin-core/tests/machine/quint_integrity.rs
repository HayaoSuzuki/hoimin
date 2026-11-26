//! Replay the two original-source verification barriers through the public machine.
use super::{
    complete_baseline_restoration, fixture_config, process_finished, quint,
    waiting_for_materialization_verification_with,
};
use hoimin_core::{
    AnalysisFinished, EffectFailed, EffectFailure, EffectId, IntegrityCheckpoint, MachineError,
    OriginalsVerified, OutputEmitted, OutputEvent, ProcessTermination, RunEffect, RunEvent,
    RunPhase, RunState, transition,
};
use serde_json::Value;

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one integrity protocol request");
    effect
}

fn run_to_final(state: RunState, effect: &RunEffect) -> (RunState, Vec<RunEffect>) {
    assert!(matches!(effect, RunEffect::RunBaseline(_)));
    let (state, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(effect.id(), ProcessTermination::Exit(0))),
    )
    .unwrap();
    let [RunEffect::EmitOutput(report), RunEffect::ResetWorker(_)] = effects.as_slice() else {
        panic!("expected baseline report and restoration: {effects:?}");
    };
    let (state, emitted) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: report.id }),
    )
    .unwrap();
    assert_eq!(emitted, Vec::new());
    let (state, effects) = complete_baseline_restoration(state, &effects);
    let RunEffect::AnalyzeFile(analysis) = one(effects) else {
        panic!("expected analysis");
    };
    transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis.id,
            spool: None,
            truncated: false,
            diagnostics: Vec::new(),
        }),
    )
    .unwrap()
}

fn request_index(request: &Value) -> usize {
    match request["tag"].as_str().unwrap() {
        "AfterCopy" => 0,
        "BeforeReport" => 1,
        "Replacement" => 2,
        other => panic!("unknown verification request {other}"),
    }
}

fn checkpoint(value: &Value) -> IntegrityCheckpoint {
    match value["tag"].as_str().unwrap() {
        "PostMaterialization" => IntegrityCheckpoint::PostMaterialization,
        "PreFinalReport" => IntegrityCheckpoint::PreFinalReport,
        "PreAnalysis" => IntegrityCheckpoint::PreAnalysis,
        "Periodic" => IntegrityCheckpoint::Periodic,
        "CleanupCheck" => IntegrityCheckpoint::Cleanup,
        other => panic!("unknown integrity checkpoint {other}"),
    }
}

fn check(
    state: &RunState,
    pending: &RunEffect,
    requests: &[Option<EffectId>; 3],
    expected: &Value,
) {
    let phase = &expected["phase"];
    let machine_phase = match (phase["tag"].as_str().unwrap(), pending) {
        ("Pending", RunEffect::VerifyOriginals(request)) => {
            let index = request_index(&phase["value"]);
            assert_eq!(requests[index], Some(request.id));
            if index == 0 {
                assert_eq!(request.checkpoint, IntegrityCheckpoint::PostMaterialization);
                RunPhase::MaterializationVerification
            } else {
                assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
                RunPhase::Finalize
            }
        }
        ("Running", RunEffect::RunBaseline(_)) => RunPhase::Baseline,
        ("Cleaning", RunEffect::Cleanup(request)) => {
            assert_eq!(request.reservations.len(), 1);
            RunPhase::Cleaning
        }
        ("Diagnostic", RunEffect::EmitOutput(output)) => {
            let OutputEvent::Diagnostic(diagnostic) = &output.event else {
                panic!("expected verification diagnostic: {output:?}");
            };
            assert_eq!(diagnostic.level, "error");
            assert_eq!(diagnostic.code, "workspace.original.changed");
            assert_eq!(diagnostic.message, "original changed: src/calc.py");
            RunPhase::Finalize
        }
        _ => panic!("expected {phase}, got {pending:?}"),
    };
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    assert_eq!(
        u64::try_from(state.exit_code()).unwrap(),
        quint::number(&expected["exit"])
    );
    let retired = quint::set(&expected["retired"]);
    let completed = quint::set(&expected["completed"]);
    for (index, id) in requests.iter().enumerate() {
        if let Some(id) = id {
            let late = retired
                .iter()
                .any(|request| request_index(request) == index);
            let done = completed
                .iter()
                .any(|request| request_index(request) == index);
            assert_eq!(state.is_effect_retired(*id), late);
            assert_eq!(state.is_effect_pending(*id), !late && !done);
        }
    }
}

fn reply(
    state: RunState,
    requests: &[Option<EffectId>; 3],
    input: &Value,
    expected: &Value,
) -> (RunState, Option<Vec<RunEffect>>) {
    let id = requests[request_index(&input["request"])].expect("issued verification request");
    let outcome = transition(
        state.clone(),
        RunEvent::OriginalsVerified(OriginalsVerified {
            id,
            checkpoint: checkpoint(&input["checkpoint"]),
        }),
    );
    let error = match expected["result"]["tag"].as_str().unwrap() {
        "Accepted" => {
            let (updated, effects) = outcome.unwrap();
            return (updated, Some(effects));
        }
        "Mismatch" => MachineError::WrongPhase {
            phase: state.phase(),
        },
        "Duplicate" => MachineError::DuplicateEffect(id),
        "Late" => MachineError::RetiredEffect(id),
        other => panic!("unknown verification verdict {other}"),
    };
    assert_eq!(outcome.unwrap_err(), error);
    (state, None)
}

fn replay(history: &[Value]) {
    let (mut state, effects) = waiting_for_materialization_verification_with(fixture_config());
    let mut pending = one(effects);
    let mut requests = [Some(pending.id()), None, None];
    check(&state, &pending, &requests, &history[0]["s"]);
    for row in &history[1..] {
        let expected = &row["s"];
        let dispatch = &row["dispatch"];
        let (updated, emitted) = match dispatch["tag"].as_str().unwrap() {
            "ReplyTo" => reply(state, &requests, &dispatch["value"], expected),
            "RunToFinal" => {
                let (updated, effects) = run_to_final(state, &pending);
                (updated, Some(effects))
            }
            "Deadline" | "FailVerification" => {
                let event = if dispatch["tag"] == "Deadline" {
                    RunEvent::DeadlineReached
                } else {
                    RunEvent::EffectFailed(EffectFailed {
                        id: pending.id(),
                        failure: EffectFailure::OriginalChanged {
                            path: "src/calc.py".into(),
                        },
                    })
                };
                let (updated, effects) = transition(state, event).unwrap();
                (updated, Some(effects))
            }
            "Idle" => (state, None),
            other => panic!("unknown integrity input {other}"),
        };
        state = updated;
        if let Some(effects) = emitted {
            pending = one(effects);
            if expected["phase"]["tag"] == "Pending" {
                let index = request_index(&expected["phase"]["value"]);
                assert!(requests[index].replace(pending.id()).is_none());
            }
        }
        check(&state, &pending, &requests, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/integrity_checkpoint_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/integrity_checkpoint_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/integrity_checkpoint_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/integrity_checkpoint_3.itf.json"))]
#[case::both_mismatch(include_str!("../../../../formal/quint/traces/integrity_checkpoint_bothMismatchStep_0.itf.json"))]
#[case::deadline(include_str!("../../../../formal/quint/traces/integrity_checkpoint_deadlineStep_0.itf.json"))]
#[case::invalid_final(include_str!("../../../../formal/quint/traces/integrity_checkpoint_invalidFinalStep_0.itf.json"))]
fn checkpoint_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
