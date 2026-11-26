//! Replay explicit membership selection in spool order through public effects.
use super::{complete_killed_candidate, fixture_candidate, quint, waiting_for_filtered_analysis};
use hoimin_core::{
    AnalysisFinished, CandidateLoaded, CandidateReadTarget, CandidateSpoolRef, IntegrityCheckpoint,
    MachineError, RunEffect, RunEvent, RunPhase, RunState, transition,
};
use serde_json::Value;

fn spool() -> CandidateSpoolRef {
    CandidateSpoolRef {
        token: "fixture-explicit-stream".to_owned(),
        records: 4,
    }
}

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one pending explicit-stream request");
    effect
}

fn bootstrap() -> (RunState, RunEffect) {
    let (state, effects) = waiting_for_filtered_analysis(["m1".to_owned(), "m3".to_owned()].into());
    let RunEffect::AnalyzeFile(request) = one(effects) else {
        panic!("expected analysis request");
    };
    let (state, effects) = transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: request.id,
            spool: Some(spool()),
            truncated: false,
            diagnostics: Vec::new(),
        }),
    )
    .unwrap();
    (state, one(effects))
}

fn check(state: &RunState, pending: &RunEffect, admitted: &[u64], expected: &Value) {
    let phase = &expected["phase"];
    let machine_phase = match (phase["tag"].as_str().unwrap(), pending) {
        ("Reading", RunEffect::ReadCandidate(request)) => {
            assert_eq!(request.spool, spool());
            assert_eq!(request.target, CandidateReadTarget::Next);
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("Applying", RunEffect::ApplyMutation(request)) => {
            assert_eq!(
                request.candidate,
                fixture_candidate(quint::number(&phase["value"]))
            );
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("Verifying", RunEffect::VerifyOriginals(request)) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        (tag, effect) => panic!("expected {tag}, got {effect:?}"),
    };
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    let order: Vec<_> = expected["admitted"]
        .as_array()
        .unwrap()
        .iter()
        .map(quint::number)
        .collect();
    assert_eq!(admitted, order);
    let summary = state.summary();
    let killed = quint::number(&expected["killed"]);
    assert_eq!(
        [
            summary.killed,
            summary.survived,
            summary.inconclusive,
            summary.not_run
        ],
        [killed, 0, 0, 0]
    );
    assert_eq!(state.exit_code(), 0);
}

fn replay(history: &[Value]) {
    let (mut state, mut pending) = bootstrap();
    let mut admitted = Vec::new();
    check(&state, &pending, &admitted, &history[0]["s"]);

    for row in &history[1..] {
        let dispatch = &row["dispatch"];
        let command = dispatch["tag"].as_str().unwrap();
        let expected = &row["s"];
        let completed = pending.id();
        let outcome = match command {
            "Idle" => {
                check(&state, &pending, &admitted, expected);
                continue;
            }
            "Complete" => {
                let candidate = fixture_candidate(*admitted.last().unwrap());
                Ok(complete_killed_candidate(
                    state.clone(),
                    std::slice::from_ref(&pending),
                    &candidate.id,
                ))
            }
            "Supply" | "EndOfSpool" | "WrongWorker" => {
                let RunEffect::ReadCandidate(request) = &pending else {
                    panic!("expected a read request: {pending:?}");
                };
                let candidate = (command != "EndOfSpool").then(|| {
                    fixture_candidate(if command == "WrongWorker" {
                        3
                    } else {
                        quint::number(&dispatch["value"])
                    })
                });
                transition(
                    state.clone(),
                    RunEvent::CandidateLoaded(CandidateLoaded {
                        id: request.id,
                        worker: if command == "WrongWorker" {
                            1
                        } else {
                            request.worker
                        },
                        candidate,
                    }),
                )
            }
            other => panic!("unknown explicit-stream input {other}"),
        };
        let result = &expected["result"];
        match result["tag"].as_str().unwrap() {
            "Accepted" => {
                let (updated, effects) = outcome.unwrap();
                pending = one(effects);
                if let RunEffect::ApplyMutation(request) = &pending {
                    admitted.push(request.candidate.sequence);
                }
                state = updated;
                assert!(!state.is_effect_pending(completed));
            }
            "Missing" => assert_eq!(
                outcome.unwrap_err(),
                MachineError::SelectedCandidateMissing(format!(
                    "m{}",
                    quint::number(&result["value"])
                ),)
            ),
            "WorkerMismatch" => assert_eq!(outcome.unwrap_err(), MachineError::UnknownWorker(1)),
            other => panic!("unknown explicit-stream result {other}"),
        }
        check(&state, &pending, &admitted, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/explicit_stream_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/explicit_stream_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/explicit_stream_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/explicit_stream_3.itf.json"))]
#[case::filtered(include_str!("../../../../formal/quint/traces/explicit_stream_filteredStep_0.itf.json"))]
#[case::partial(include_str!("../../../../formal/quint/traces/explicit_stream_partialStep_0.itf.json"))]
#[case::wrong_worker(include_str!("../../../../formal/quint/traces/explicit_stream_workerStep_0.itf.json"))]
fn explicit_stream_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
