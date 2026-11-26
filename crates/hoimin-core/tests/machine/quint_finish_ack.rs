//! Replay final report delivery and session finish callbacks.
use super::{fixture_session_config, quint, waiting_for_analysis_with};
use hoimin_core::{
    AnalysisFinished, CleanupFinished, EffectFailed, EffectFailure, FinishSession,
    IntegrityCheckpoint, MachineError, OriginalsVerified, OutputEmitted, OutputEvent, RunEffect,
    RunEvent, RunPhase, RunState, SessionFinished, transition,
};
use serde_json::Value;

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one session finalization effect");
    effect
}

fn bootstrap(complete: bool) -> (RunState, RunEffect) {
    let (state, effects) = waiting_for_analysis_with(fixture_session_config(false));
    let RunEffect::AnalyzeFile(analysis) = one(effects) else {
        panic!("expected a session analysis request");
    };
    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: analysis.id,
            spool: None,
            truncated: !complete,
            diagnostics: Vec::new(),
        }),
    )
    .unwrap();
    let RunEffect::VerifyOriginals(verify) = one(effects) else {
        panic!("expected final verification");
    };
    assert_eq!(verify.checkpoint, IntegrityCheckpoint::PreFinalReport);
    let (state, effects) = transition(
        state,
        RunEvent::OriginalsVerified(OriginalsVerified {
            id: verify.id,
            checkpoint: verify.checkpoint,
        }),
    )
    .unwrap();
    let RunEffect::Cleanup(cleanup) = one(effects) else {
        panic!("expected cleanup before publishing the report");
    };
    let (state, effects) = transition(
        state,
        RunEvent::CleanupFinished(CleanupFinished {
            id: cleanup.id,
            released_reservations: cleanup.reservations,
        }),
    )
    .unwrap();
    (state, one(effects))
}

fn input(command: &Value, pending: Option<&RunEffect>, finish: Option<&FinishSession>) -> RunEvent {
    match (command["tag"].as_str().unwrap(), pending) {
        ("AckReport", Some(RunEffect::EmitOutput(output))) => {
            RunEvent::OutputEmitted(OutputEmitted { id: output.id })
        }
        ("Finish", _) => {
            let finish = finish.expect("issued finish request");
            RunEvent::SessionFinished(SessionFinished {
                id: finish.id,
                run_id: if command["value"]["runMatches"] == true {
                    finish.run_id.clone()
                } else {
                    "wrong-session".to_owned()
                },
                complete: command["value"]["complete"].as_bool().unwrap(),
            })
        }
        ("Fail", Some(RunEffect::EmitOutput(output))) => RunEvent::EffectFailed(EffectFailed {
            id: output.id,
            failure: EffectFailure::ReportIo {
                operation: "write".into(),
                message: "fixture failure".into(),
            },
        }),
        ("Fail", Some(RunEffect::FinishSession(request))) => RunEvent::EffectFailed(
            EffectFailed::other(request.id, "fixture.session.finish", "fixture failure"),
        ),
        ("Deadline", _) => RunEvent::DeadlineReached,
        _ => panic!("unexpected finalization command {command:?} for {pending:?}"),
    }
}

fn check(
    state: &RunState,
    pending: Option<&RunEffect>,
    finish: Option<&FinishSession>,
    expected: &Value,
) {
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let complete = expected["complete"].as_bool().unwrap();
    let exit = quint::number(&expected["exit"]);
    let machine_phase = match (phase, pending) {
        ("Report", Some(RunEffect::EmitOutput(output))) => {
            let OutputEvent::RunFinished(report) = &output.event else {
                panic!("expected final report: {output:?}");
            };
            assert_eq!(report.complete, complete);
            assert_eq!(report.run_id, state.run_id());
            assert_eq!(u64::try_from(report.exit_code).unwrap(), exit);
            assert_eq!(report.disk.cleanup.len(), 1);
            RunPhase::Finalize
        }
        ("Closing", Some(RunEffect::FinishSession(request))) => {
            assert_eq!(request.complete, complete);
            assert_eq!(request.run_id, state.run_id());
            RunPhase::Finalize
        }
        ("Finished", None) => RunPhase::Finished,
        _ => panic!("unexpected {phase} pending effect {pending:?}"),
    };
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(u64::try_from(state.exit_code()).unwrap(), exit);
    assert_eq!(state.pending_count(), usize::from(pending.is_some()));
    if let Some(effect) = pending {
        assert!(state.is_effect_pending(effect.id()));
    }
    assert_eq!(finish.is_some(), expected["reportAck"] == true);
    if let Some(request) = finish {
        assert_eq!(state.is_effect_pending(request.id), phase == "Closing");
        assert!(!state.is_effect_retired(request.id));
    }
}

fn replay(history: &[Value]) {
    let [_, initial, commands @ ..] = history else {
        panic!("expected a published report");
    };
    let publish = &initial["dispatch"];
    assert_eq!(publish["tag"], "Publish");
    let (mut state, report) = bootstrap(publish["value"].as_bool().unwrap());
    let mut pending = Some(report);
    let mut finish = None;
    check(&state, pending.as_ref(), finish.as_ref(), &initial["s"]);
    for row in commands {
        let command = &row["dispatch"];
        let expected = &row["s"];
        if command["tag"] != "Idle" {
            let outcome = transition(
                state.clone(),
                input(command, pending.as_ref(), finish.as_ref()),
            );
            match (
                command["tag"].as_str().unwrap(),
                expected["result"]["tag"].as_str().unwrap(),
            ) {
                ("Deadline", _) => {
                    let (updated, effects) = outcome.unwrap();
                    assert_eq!(effects, Vec::new());
                    state = updated;
                }
                (_, "Accepted") => {
                    let (updated, mut effects) = outcome.unwrap();
                    assert!(effects.len() <= 1);
                    pending = effects.pop();
                    if let Some(RunEffect::FinishSession(request)) = &pending {
                        assert!(finish.replace(request.clone()).is_none());
                    }
                    state = updated;
                }
                (_, "Mismatch") => {
                    let request = finish.as_ref().expect("requested session finish");
                    let RunEvent::SessionFinished(received) = input(command, None, Some(request))
                    else {
                        panic!("expected a session finish reply");
                    };
                    let error = outcome.unwrap_err();
                    assert_eq!(error.code(), "machine.session.finish_mismatch");
                    assert_eq!(
                        error,
                        MachineError::SessionFinishMismatch {
                            id: request.id,
                            expected_run_id: request.run_id.clone(),
                            received_run_id: received.run_id,
                            expected_complete: request.complete,
                            received_complete: received.complete,
                        },
                    );
                }
                (_, "Duplicate") => assert_eq!(
                    outcome.unwrap_err(),
                    MachineError::DuplicateEffect(finish.as_ref().unwrap().id)
                ),
                (_, other) => panic!("unknown session finish verdict {other}"),
            }
        }
        check(&state, pending.as_ref(), finish.as_ref(), expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/session_finish_ack_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/session_finish_ack_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/session_finish_ack_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/session_finish_ack_3.itf.json"))]
#[case::clean(include_str!("../../../../formal/quint/traces/session_finish_ack_cleanStep_0.itf.json"))]
#[case::wrong_flag(include_str!("../../../../formal/quint/traces/session_finish_ack_wrongFlagStep_0.itf.json"))]
#[case::wrong_run(include_str!("../../../../formal/quint/traces/session_finish_ack_wrongRunStep_0.itf.json"))]
#[case::session_failure(include_str!("../../../../formal/quint/traces/session_finish_ack_sessionFailureStep_0.itf.json"))]
#[case::report_failure(include_str!("../../../../formal/quint/traces/session_finish_ack_reportFailureStep_0.itf.json"))]
#[case::freeze(include_str!("../../../../formal/quint/traces/session_finish_ack_freezeStep_0.itf.json"))]
fn session_finish_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 2);
    replay(&history);
}
