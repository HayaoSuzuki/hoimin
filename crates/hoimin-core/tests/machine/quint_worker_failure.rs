//! Replay generic and report delivery failures for one admitted candidate.
use super::{fixture_candidate, process_finished, quint, waiting_for_candidate};
use hoimin_core::{
    CandidateLoaded, CandidateReadTarget, CleanupFinished, EffectFailed, EffectFailure,
    MachineError, MutationApplied, MutationOutcome, OutputEmitted, OutputEvent, ProcessTermination,
    RunEffect, RunEvent, RunPhase, RunState, WorkerReset, transition,
};
use serde_json::Value;

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one worker effect");
    effect
}

fn bootstrap() -> (RunState, RunEffect) {
    let (state, effects) = waiting_for_candidate();
    let RunEffect::ReadCandidate(request) = one(effects) else {
        panic!("expected a sequential candidate read");
    };
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: request.id,
            worker: request.worker,
            candidate: Some(fixture_candidate(1)),
        }),
    )
    .unwrap();
    (state, one(effects))
}

fn acknowledge(effect: &RunEffect) -> RunEvent {
    match effect {
        RunEffect::ApplyMutation(request) => RunEvent::MutationApplied(MutationApplied {
            id: request.id,
            worker: request.worker,
        }),
        RunEffect::RunMutant(request) => {
            RunEvent::MutantFinished(process_finished(request.id, ProcessTermination::Exit(1)))
        }
        RunEffect::EmitOutput(output) => RunEvent::OutputEmitted(OutputEmitted { id: output.id }),
        RunEffect::ResetWorker(request) => RunEvent::WorkerReset(WorkerReset {
            id: request.id,
            worker: request.worker,
        }),
        RunEffect::Cleanup(request) => RunEvent::CleanupFinished(CleanupFinished {
            id: request.id,
            released_reservations: request.reservations.clone(),
        }),
        other => panic!("unexpected acknowledgement for {other:?}"),
    }
}

fn effect_failure(effect: &RunEffect, kind: &str) -> RunEvent {
    let failure = match kind {
        "EffectFailure" => EffectFailure::Other {
            code: "fixture.worker.failure".into(),
            message: "fixture worker failure".into(),
        },
        "DeliveryFailure" => EffectFailure::ReportIo {
            operation: "write".into(),
            message: "fixture worker failure".into(),
        },
        other => panic!("unexpected failure kind {other}"),
    };
    RunEvent::EffectFailed(EffectFailed {
        id: effect.id(),
        failure,
    })
}

fn check_output(state: &RunState, phase: &str, event: &OutputEvent, expected: &Value) -> RunPhase {
    assert_eq!(event.run_id(), "run-1");
    match (phase, event) {
        ("Start" | "ReplacementStart", OutputEvent::MutantStarted(start)) => {
            assert_eq!(start.mutant_id, "m1");
            assert_eq!(start.mutant_sequence, 1);
            RunPhase::Mutants
        }
        ("Finish" | "ReplacementFinish", OutputEvent::MutantFinished(finished)) => {
            assert_eq!(finished.candidate, fixture_candidate(1));
            match expected["outcome"]["tag"].as_str().unwrap() {
                "Killed" => assert!(matches!(
                    finished.outcome,
                    MutationOutcome::Executed {
                        termination: ProcessTermination::Exit(1),
                        ..
                    }
                )),
                "NotRun" => assert_eq!(finished.outcome, MutationOutcome::NotRun),
                other => panic!("unexpected candidate outcome {other}"),
            }
            RunPhase::Mutants
        }
        ("Diagnostic", OutputEvent::Diagnostic(diagnostic)) => {
            let (code, message) = match expected["failure"]["tag"].as_str().unwrap() {
                "EffectFailure" => ("fixture.worker.failure", "fixture worker failure"),
                "DeliveryFailure" => ("report.io", "write: fixture worker failure"),
                other => panic!("unexpected diagnostic kind {other}"),
            };
            assert_eq!(diagnostic.level, "error");
            assert_eq!(diagnostic.code, code);
            assert_eq!(diagnostic.message, message);
            RunPhase::Finalize
        }
        ("Report", OutputEvent::RunFinished(report)) => {
            assert_eq!(&report.counts, state.summary());
            assert_eq!(report.exit_code, 2);
            assert!(!report.complete);
            assert_eq!(report.disk.cleanup.len(), 1);
            RunPhase::Finalize
        }
        _ => panic!("unexpected {phase} output {event:?}"),
    }
}

fn check(state: &RunState, pending: Option<&RunEffect>, expected: &Value) {
    assert_eq!(state.pending_count(), usize::from(pending.is_some()));
    if let Some(effect) = pending {
        assert!(state.is_effect_pending(effect.id()));
    }
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let machine_phase = match (phase, pending) {
        ("Apply", Some(RunEffect::ApplyMutation(request))) => {
            assert_eq!(request.candidate, fixture_candidate(1));
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("Running", Some(RunEffect::RunMutant(request))) => {
            assert_eq!(request.context.mutant_id, "m1");
            assert_eq!(request.context.worker, 0);
            RunPhase::Mutants
        }
        ("Reset", Some(RunEffect::ResetWorker(request))) => {
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("NextRead", Some(RunEffect::ReadCandidate(request))) => {
            assert_eq!(request.target, CandidateReadTarget::Next);
            RunPhase::Mutants
        }
        ("Cleanup", Some(RunEffect::Cleanup(request))) => {
            assert_eq!(request.reservations.len(), 1);
            RunPhase::Cleaning
        }
        ("Done", None) => RunPhase::Finished,
        (_, Some(RunEffect::EmitOutput(output))) => {
            check_output(state, phase, &output.event, expected)
        }
        _ => panic!("unexpected {phase} effect {pending:?}"),
    };
    let counts = state.summary();
    let killed = quint::number(&expected["killed"]);
    let not_run = quint::number(&expected["notRun"]);
    assert_eq!(
        [
            counts.killed,
            counts.survived,
            counts.not_run,
            counts.inconclusive
        ],
        [killed, 0, not_run, not_run]
    );
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(
        state.exit_code(),
        if expected["failure"]["tag"] == "NoFailure" {
            0
        } else {
            2
        }
    );
}

fn replay(history: &[Value]) {
    let (mut state, pending) = bootstrap();
    let mut failed = None;
    let mut active = Some(pending);
    check(&state, active.as_ref(), &history[0]["s"]);
    for row in &history[1..] {
        let command = &row["dispatch"];
        let expected = &row["s"];
        match command["tag"].as_str().unwrap() {
            "RetryFailed" => {
                assert_eq!(expected["result"]["tag"], "Duplicate");
                let previous: &RunEffect = failed.as_ref().expect("one failed request");
                assert_eq!(
                    transition(state.clone(), acknowledge(previous)).unwrap_err(),
                    MachineError::DuplicateEffect(previous.id()),
                );
            }
            "Idle" => {}
            action @ ("Ack" | "Fail") => {
                let pending = active.take().expect("active worker request");
                let event = if action == "Fail" {
                    let event = effect_failure(&pending, command["value"]["tag"].as_str().unwrap());
                    failed = Some(pending);
                    event
                } else {
                    acknowledge(&pending)
                };
                let (updated, mut effects) = transition(state, event).unwrap();
                assert!(effects.len() <= 1, "one worker continuation");
                active = effects.pop();
                state = updated;
                if let Some(previous) = &failed {
                    assert!(!state.is_effect_pending(previous.id()));
                    assert!(!state.is_effect_retired(previous.id()));
                }
            }
            other => panic!("unexpected worker failure input {other}"),
        }
        check(&state, active.as_ref(), expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/worker_failure_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/worker_failure_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/worker_failure_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/worker_failure_3.itf.json"))]
#[case::apply(include_str!("../../../../formal/quint/traces/worker_failure_applyFailureStep_0.itf.json"))]
#[case::start(include_str!("../../../../formal/quint/traces/worker_failure_startFailureStep_0.itf.json"))]
#[case::run(include_str!("../../../../formal/quint/traces/worker_failure_runFailureStep_0.itf.json"))]
#[case::finish(include_str!("../../../../formal/quint/traces/worker_failure_finishFailureStep_0.itf.json"))]
#[case::delivery_start(include_str!("../../../../formal/quint/traces/worker_failure_reportStartFailureStep_0.itf.json"))]
#[case::delivery_finish(include_str!("../../../../formal/quint/traces/worker_failure_reportFinishFailureStep_0.itf.json"))]
fn failure_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
