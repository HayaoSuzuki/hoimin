//! The public lookup and synthetic output transitions of a resumed candidate.
use super::{fixture_candidate, quint, waiting_for_session_candidate};
use hoimin_core::{
    CandidateLoaded, CandidateReadTarget, CleanupFinished, EffectFailed, EffectFailure,
    MachineError, MutationOutcome, MutationStatus, OutputEmitted, OutputEvent,
    ReusedMutationStatus, RunEffect, RunEvent, RunPhase, RunState, SessionFinished, StoredResult,
    StoredResultLoaded, transition,
};
use serde_json::Value;

fn one(pending: Vec<RunEffect>) -> RunEffect {
    let [effect]: [RunEffect; 1] = pending.try_into().expect("one pending candidate effect");
    effect
}

fn bootstrap() -> (RunState, RunEffect) {
    let (state, pending) = waiting_for_session_candidate(true);
    let RunEffect::ReadCandidate(read) = one(pending) else {
        panic!("expected a sequential candidate read");
    };
    let (state, pending) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read.id,
            worker: read.worker,
            candidate: Some(fixture_candidate(1)),
        }),
    )
    .unwrap();
    (state, one(pending))
}

fn lookup_reply(effect: &RunEffect, status: u64, identity_ok: bool) -> RunEvent {
    let RunEffect::LookupStoredResult(lookup) = effect else {
        panic!("expected a pending lookup: {effect:?}");
    };
    let status = match status {
        0 => None,
        1 => Some(MutationStatus::Killed),
        2 => Some(MutationStatus::Survived),
        3 => Some(MutationStatus::Timeout),
        4 => Some(MutationStatus::OutOfMemory),
        5 => Some(MutationStatus::ProcessLimit),
        6 => Some(MutationStatus::Error),
        7 => Some(MutationStatus::NotRun),
        other => panic!("unknown model status {other}"),
    };
    RunEvent::StoredResultLoaded(StoredResultLoaded {
        id: lookup.id,
        worker: lookup.worker,
        result: status.map(|status| StoredResult {
            mutant_id: if identity_ok { "m1" } else { "m2" }.into(),
            status,
        }),
    })
}

fn acknowledgement(effect: &RunEffect) -> RunEvent {
    match effect {
        RunEffect::EmitOutput(output) => RunEvent::OutputEmitted(OutputEmitted { id: output.id }),
        RunEffect::Cleanup(request) => RunEvent::CleanupFinished(CleanupFinished {
            id: request.id,
            released_reservations: request.reservations.clone(),
        }),
        RunEffect::FinishSession(request) => RunEvent::SessionFinished(SessionFinished {
            id: request.id,
            run_id: request.run_id.clone(),
            complete: request.complete,
        }),
        other => panic!("unexpected acknowledgement: {other:?}"),
    }
}

fn check_output(
    state: &RunState,
    event: &OutputEvent,
    outcome: &str,
    failure: &str,
) -> (&'static str, RunPhase) {
    assert_eq!(event.run_id(), state.run_id());
    match event {
        OutputEvent::MutantStarted(started) => {
            assert_eq!(started.mutant_id, "m1");
            ("StartPending", RunPhase::Mutants)
        }
        OutputEvent::MutantFinished(finished) => {
            assert_eq!(finished.candidate.id, "m1");
            let expected_outcome = match outcome {
                "CachedKilled" => MutationOutcome::Reused {
                    status: ReusedMutationStatus::Killed,
                },
                "CachedSurvived" => MutationOutcome::Reused {
                    status: ReusedMutationStatus::Survived,
                },
                "NotRun" => MutationOutcome::NotRun,
                other => panic!("unknown synthetic outcome {other}"),
            };
            assert_eq!(finished.outcome, expected_outcome);
            ("FinishPending", RunPhase::Mutants)
        }
        OutputEvent::Diagnostic(diagnostic) => {
            let (code, message) = match failure {
                "EffectFailure" => ("synthetic.fixture", "synthetic output failed"),
                "DeliveryFailure" => ("report.io", "write: synthetic output failed"),
                other => panic!("diagnostic without a modeled failure: {other}"),
            };
            assert_eq!(diagnostic.code, code);
            assert_eq!(diagnostic.message, message);
            assert_eq!(diagnostic.level, "error");
            ("DiagnosticPending", RunPhase::Finalize)
        }
        OutputEvent::RunFinished(report) => {
            assert_eq!(&report.counts, state.summary());
            assert_eq!(report.exit_code, 2);
            assert!(!report.complete);
            ("ReportPending", RunPhase::Finalize)
        }
        other => panic!("unexpected candidate output: {other:?}"),
    }
}

fn check_state(state: &RunState, pending: Option<&RunEffect>, expected: &Value) {
    assert_eq!(state.pending_count(), usize::from(pending.is_some()));
    if let Some(effect) = pending {
        assert!(state.is_effect_pending(effect.id()));
    }
    let outcome = expected["outcome"]["tag"].as_str().unwrap();
    let failure = expected["failure"]["tag"].as_str().unwrap();
    let (pending_kind, machine_phase) = match pending {
        Some(RunEffect::LookupStoredResult(lookup)) => {
            assert_eq!(lookup.mutant_id, "m1");
            assert_eq!(lookup.run_id, state.run_id());
            ("LookupPending", RunPhase::Mutants)
        }
        Some(RunEffect::ApplyMutation(apply)) => {
            assert_eq!(apply.candidate.id, "m1");
            ("ApplyPending", RunPhase::Mutants)
        }
        Some(RunEffect::EmitOutput(output)) => check_output(state, &output.event, outcome, failure),
        Some(RunEffect::ReadCandidate(read)) => {
            assert_eq!(read.target, CandidateReadTarget::Next);
            ("NextCandidate", RunPhase::Mutants)
        }
        Some(RunEffect::Cleanup(_)) => ("CleanupPending", RunPhase::Cleaning),
        Some(RunEffect::FinishSession(request)) => {
            assert_eq!(request.run_id, state.run_id());
            assert!(!request.complete);
            ("SessionPending", RunPhase::Finalize)
        }
        None => ("Finished", RunPhase::Finished),
        other => panic!("unexpected candidate effect: {other:?}"),
    };
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(pending_kind, expected["stage"]["tag"].as_str().unwrap());
    let count = quint::number(&expected["count"]);
    let counts = ["CachedKilled", "CachedSurvived", "NotRun"]
        .map(|wanted| if outcome == wanted { count } else { 0 });
    let summary = state.summary();
    assert_eq!([summary.killed, summary.survived, summary.not_run], counts);
    assert_eq!(summary.inconclusive, counts[2]);
    assert_eq!(
        state.exit_code(),
        if expected["cancelled"] == true {
            130
        } else if failure != "NoFailure" {
            2
        } else {
            0
        }
    );
}

fn replay(history: &[Value]) {
    let (mut state, initial) = bootstrap();
    let mut pending = Some(initial);
    let mut obsolete = None;
    for row in history {
        let expected = &row["s"];
        let dispatch = &row["dispatch"];
        let value = &dispatch["value"];
        let input = match dispatch["tag"].as_str().unwrap() {
            "Init" => None,
            "Reply" => Some(lookup_reply(
                pending.as_ref().unwrap(),
                quint::number(&value["status"]),
                value["identityOk"].as_bool().unwrap(),
            )),
            "Ack" => Some(acknowledgement(pending.as_ref().unwrap())),
            "Cancel" => {
                obsolete.clone_from(&pending);
                Some(RunEvent::CancellationRequested)
            }
            "Fail" => {
                obsolete.clone_from(&pending);
                let id = pending.as_ref().unwrap().id();
                let failure = if value.as_bool().unwrap() {
                    EffectFailure::ReportIo {
                        operation: "write".into(),
                        message: "synthetic output failed".into(),
                    }
                } else {
                    EffectFailure::Other {
                        code: "synthetic.fixture".into(),
                        message: "synthetic output failed".into(),
                    }
                };
                Some(RunEvent::EffectFailed(EffectFailed { id, failure }))
            }
            "Late" => {
                let previous = obsolete.as_ref().expect("cancelled or failed request");
                let callback = match previous {
                    RunEffect::LookupStoredResult(_) => lookup_reply(previous, 1, true),
                    _ => acknowledgement(previous),
                };
                Some(callback)
            }
            other => panic!("unknown resume command {other}"),
        };
        if let Some(event) = input {
            match expected["result"].as_str().unwrap() {
                "accepted" => {
                    let (next, mut effects) = transition(state, event).unwrap();
                    state = next;
                    assert!(effects.len() <= 1, "one candidate continuation");
                    pending = effects.pop();
                }
                "identity" => assert_eq!(
                    transition(state.clone(), event).unwrap_err(),
                    MachineError::SessionIdentityMismatch {
                        worker: 0,
                        field: "mutant_id",
                        expected: "m1".into(),
                        received: "m2".into(),
                    },
                ),
                "retired" => assert_eq!(
                    transition(state.clone(), event).unwrap_err(),
                    MachineError::RetiredEffect(obsolete.as_ref().unwrap().id()),
                ),
                "duplicate" => assert_eq!(
                    transition(state.clone(), event).unwrap_err(),
                    MachineError::DuplicateEffect(obsolete.as_ref().unwrap().id()),
                ),
                other => panic!("unknown resume verdict {other}"),
            }
        }
        if let Some(previous) = &obsolete {
            assert!(!state.is_effect_pending(previous.id()));
            assert_eq!(
                state.is_effect_retired(previous.id()),
                expected["cancelled"] == true
            );
        }
        check_state(&state, pending.as_ref(), expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/resume_candidate_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/resume_candidate_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/resume_candidate_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/resume_candidate_3.itf.json"))]
#[case::reused(include_str!("../../../../formal/quint/traces/resume_candidate_reuseStep_0.itf.json"))]
#[case::rerun(include_str!("../../../../formal/quint/traces/resume_candidate_rerunStep_0.itf.json"))]
#[case::cancelled_lookup(include_str!("../../../../formal/quint/traces/resume_candidate_cancelledLookupStep_0.itf.json"))]
#[case::cancelled_start(include_str!("../../../../formal/quint/traces/resume_candidate_cancelledStartStep_0.itf.json"))]
#[case::cancelled_finish(include_str!("../../../../formal/quint/traces/resume_candidate_cancelledFinishStep_0.itf.json"))]
#[case::failed_start(include_str!("../../../../formal/quint/traces/resume_candidate_failedStartStep_0.itf.json"))]
#[case::failed_finish(include_str!("../../../../formal/quint/traces/resume_candidate_failedFinishStep_0.itf.json"))]
#[case::report_io(include_str!("../../../../formal/quint/traces/resume_candidate_reportIoStep_0.itf.json"))]
fn resume_history_matches_public_candidate_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
