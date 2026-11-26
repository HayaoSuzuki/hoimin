//! Ordered discovery and indexed reads interrupted through public transitions.
use super::{fixture_candidate, quint, quint_ordered::bootstrap};
use hoimin_core::{
    CandidateLoaded, CandidateReadTarget, EffectFailed, IntegrityCheckpoint, MachineError,
    MutationOutcome, OutputEmitted, OutputEvent, ReadCandidate, RunEffect, RunEvent, RunPhase,
    RunState, transition,
};
use serde_json::Value;

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect]: [_; 1] = effects
        .try_into()
        .expect("one pending ordered-drain effect");
    effect
}

fn read(effect: &RunEffect) -> &ReadCandidate {
    let RunEffect::ReadCandidate(request) = effect else {
        panic!("expected a candidate read: {effect:?}");
    };
    request
}

fn loaded(request: &ReadCandidate, id: Option<u64>) -> RunEvent {
    RunEvent::CandidateLoaded(CandidateLoaded {
        id: request.id,
        worker: request.worker,
        candidate: id.map(fixture_candidate),
    })
}

fn head_id(expected: &Value) -> u64 {
    quint::number(&expected["pending"][0])
}

fn check(state: &RunState, effect: &RunEffect, reported: &[String], expected: &Value) {
    assert!(state.is_effect_pending(effect.id()));
    assert_eq!(state.pending_count(), 1);
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let machine_phase = match (phase, effect) {
        ("Scanning", RunEffect::ReadCandidate(request)) => {
            assert_eq!(request.target, CandidateReadTarget::Next);
            RunPhase::Mutants
        }
        ("Indexed" | "Loading", RunEffect::ReadCandidate(request)) => {
            assert_eq!(
                request.target,
                CandidateReadTarget::Id(format!("m{}", head_id(expected)))
            );
            RunPhase::Mutants
        }
        ("Starting", RunEffect::EmitOutput(output)) => {
            let OutputEvent::MutantStarted(started) = &output.event else {
                panic!("expected synthetic start: {output:?}");
            };
            assert_eq!(started.mutant_id, format!("m{}", head_id(expected)));
            RunPhase::Mutants
        }
        ("Finishing", RunEffect::EmitOutput(output)) => {
            let OutputEvent::MutantFinished(finished) = &output.event else {
                panic!("expected synthetic finish: {output:?}");
            };
            assert_eq!(finished.candidate.id, format!("m{}", head_id(expected)));
            assert_eq!(finished.outcome, MutationOutcome::NotRun);
            RunPhase::Mutants
        }
        ("VerifyPending", RunEffect::VerifyOriginals(request)) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        ("FailureDiagnostic", RunEffect::EmitOutput(output)) => {
            let OutputEvent::Diagnostic(diagnostic) = &output.event else {
                panic!("expected read failure diagnostic: {output:?}");
            };
            assert_eq!(diagnostic.code, "fixture.read.failed");
            assert_eq!(diagnostic.message, "fixture read failed");
            assert_eq!(diagnostic.level, "error");
            RunPhase::Finalize
        }
        _ => panic!("expected {phase}, got {effect:?}"),
    };
    assert_eq!(state.phase(), machine_phase);
    let order: Vec<_> = expected["reported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| format!("m{}", quint::number(id)))
        .collect();
    assert_eq!(reported, order);
    let summary = state.summary();
    let count = u64::try_from(order.len()).unwrap();
    assert_eq!(
        [
            summary.killed,
            summary.survived,
            summary.not_run,
            summary.inconclusive
        ],
        [0, 0, count, count]
    );
    let exit_code = if expected["interrupted"] == true {
        130
    } else if expected["failed"] == true {
        2
    } else if quint::number(&expected["stops"]) > 0 {
        4
    } else {
        0
    };
    assert_eq!(state.exit_code(), exit_code);
}

fn replay(history: &[Value]) {
    let (mut state, effects) = bootstrap();
    let mut pending = one(effects);
    let mut reported = Vec::new();
    let mut retired = Vec::new();
    check(&state, &pending, &reported, &history[0]["s"]);
    for (index, row) in history.iter().enumerate().skip(1) {
        let dispatch = &row["dispatch"];
        let value = &dispatch["value"];
        let event = match dispatch["tag"].as_str().unwrap() {
            "Discover" => loaded(read(&pending), Some(quint::number(value))),
            "EndOfSpool" => loaded(read(&pending), None),
            "LoadBody" => loaded(read(&pending), Some(head_id(&history[index - 1]["s"]))),
            "Stop" => {
                retired.push(read(&pending).clone());
                if value.as_bool().unwrap() {
                    RunEvent::CancellationRequested
                } else {
                    RunEvent::DeadlineReached
                }
            }
            "FailRead" => RunEvent::EffectFailed(EffectFailed::other(
                read(&pending).id,
                "fixture.read.failed",
                "fixture read failed",
            )),
            "Ack" => {
                let RunEffect::EmitOutput(output) = &pending else {
                    panic!("expected synthetic output: {pending:?}");
                };
                if let OutputEvent::MutantFinished(finished) = &output.event {
                    reported.push(finished.candidate.id.clone());
                }
                RunEvent::OutputEmitted(OutputEmitted { id: output.id })
            }
            "Late" => {
                let request = &retired[usize::try_from(quint::number(value)).unwrap()];
                let body = match &request.target {
                    CandidateReadTarget::Next => Some(3),
                    CandidateReadTarget::Id(id) => {
                        Some(id.strip_prefix('m').unwrap().parse().unwrap())
                    }
                };
                assert_eq!(
                    transition(state.clone(), loaded(request, body)).unwrap_err(),
                    MachineError::RetiredEffect(request.id)
                );
                check(&state, &pending, &reported, &row["s"]);
                continue;
            }
            other => panic!("unknown drain input {other}"),
        };
        let (next, emitted) = transition(state, event).unwrap();
        state = next;
        pending = one(emitted);
        for previous in &retired {
            assert!(state.is_effect_retired(previous.id));
        }
        check(&state, &pending, &reported, &row["s"]);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/ordered_drain_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/ordered_drain_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/ordered_drain_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/ordered_drain_3.itf.json"))]
#[case::partial_stop(include_str!("../../../../formal/quint/traces/ordered_drain_partialStopStep_0.itf.json"))]
#[case::repeated_indexed_stop(include_str!("../../../../formal/quint/traces/ordered_drain_repeatedIndexedStopStep_0.itf.json"))]
#[case::indexed_failure(include_str!("../../../../formal/quint/traces/ordered_drain_indexedFailureStep_0.itf.json"))]
#[case::trailing_failure(include_str!("../../../../formal/quint/traces/ordered_drain_trailingFailureStep_0.itf.json"))]
fn ordered_drain_history_matches_public_requests(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
