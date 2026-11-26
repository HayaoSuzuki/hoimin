//! The public lookup and synthetic output transitions of a resumed candidate.
use super::{fixture_candidate, quint, waiting_for_session_candidate};
use hoimin_core::{
    CandidateLoaded, CandidateReadTarget, MachineError, MutationOutcome, MutationStatus,
    OutputEmitted, OutputEvent, ReusedMutationStatus, RunEffect, RunEvent, RunPhase, RunState,
    StoredResult, StoredResultLoaded, transition,
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
    let RunEffect::EmitOutput(output) = effect else {
        panic!("expected a pending output: {effect:?}");
    };
    RunEvent::OutputEmitted(OutputEmitted { id: output.id })
}

fn check_state(state: &RunState, pending: &RunEffect, expected: &Value) {
    assert_eq!(state.phase(), RunPhase::Mutants);
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    let outcome = expected["outcome"]["tag"].as_str().unwrap();
    let pending_kind = match pending {
        RunEffect::LookupStoredResult(lookup) => {
            assert_eq!(lookup.mutant_id, "m1");
            assert_eq!(lookup.run_id, state.run_id());
            "LookupPending"
        }
        RunEffect::ApplyMutation(apply) => {
            assert_eq!(apply.candidate.id, "m1");
            "ApplyPending"
        }
        RunEffect::EmitOutput(output) => {
            assert_eq!(output.event.run_id(), state.run_id());
            match &output.event {
                OutputEvent::MutantStarted(started) => {
                    assert_eq!(started.mutant_id, "m1");
                    "StartPending"
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
                    "FinishPending"
                }
                other => panic!("unexpected candidate output: {other:?}"),
            }
        }
        RunEffect::ReadCandidate(read) => {
            assert_eq!(read.target, CandidateReadTarget::Next);
            "NextCandidate"
        }
        other => panic!("unexpected candidate effect: {other:?}"),
    };
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
        } else {
            0
        }
    );
}

fn replay(history: &[Value]) {
    let (mut state, mut pending) = bootstrap();
    let mut retired = None;
    for row in history {
        let expected = &row["s"];
        let dispatch = &row["dispatch"];
        let value = &dispatch["value"];
        let input = match dispatch["tag"].as_str().unwrap() {
            "Init" => None,
            "Reply" => Some(lookup_reply(
                &pending,
                quint::number(&value["status"]),
                value["identityOk"].as_bool().unwrap(),
            )),
            "Ack" => Some(acknowledgement(&pending)),
            "Cancel" => {
                retired = Some(pending.clone());
                Some(RunEvent::CancellationRequested)
            }
            "Late" => {
                let retired_request = retired.as_ref().expect("cancelled request");
                let callback = match retired_request {
                    RunEffect::LookupStoredResult(_) => lookup_reply(retired_request, 1, true),
                    _ => acknowledgement(retired_request),
                };
                Some(callback)
            }
            other => panic!("unknown resume command {other}"),
        };
        if let Some(event) = input {
            match expected["result"].as_str().unwrap() {
                "accepted" => {
                    let (next, effects) = transition(state, event).unwrap();
                    state = next;
                    pending = one(effects);
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
                    MachineError::RetiredEffect(retired.as_ref().unwrap().id()),
                ),
                other => panic!("unknown resume verdict {other}"),
            }
        }
        if let Some(retired) = &retired {
            assert!(state.is_effect_retired(retired.id()));
        }
        check_state(&state, &pending, expected);
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
fn resume_history_matches_public_candidate_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
