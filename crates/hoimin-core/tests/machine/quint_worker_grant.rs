//! Replay the shared reservation and both materialization acknowledgements.
use super::{complete_run_started, fixture_raw_config, pending_preflight_with, quint};
use hoimin_core::{
    CreateWorker, EffectFailed, EnvironmentSnapshot, IntegrityCheckpoint, MachineError,
    OutputEvent, ReservationId, RunConfig, RunEffect, RunEvent, RunPhase, RunState, WorkerCreated,
    transition,
};
use serde_json::Value;

const OTHER_RESERVATION: ReservationId = ReservationId(99);

fn bootstrap() -> (RunState, [CreateWorker; 2]) {
    let mut raw = fixture_raw_config();
    raw.limits.jobs = 2;
    let config = RunConfig::try_from(raw)
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap();
    let (state, preflight) = pending_preflight_with(config);
    let (preflight_id, allowance) = (preflight.id, preflight.aggregate_logical_bytes);
    let (state, report) = transition(state, RunEvent::PreflightCompleted(preflight)).unwrap();
    let (state, effects) = complete_run_started(state, &report);
    let Ok(
        [
            RunEffect::CreateWorker(first),
            RunEffect::CreateWorker(second),
        ],
    ) = <[_; 2]>::try_from(effects)
    else {
        panic!("expected both worker creation requests");
    };
    let requests = [first, second];
    assert_eq!(requests[0].reservation_id(), requests[1].reservation_id());
    assert_ne!(requests[0].reservation_id(), OTHER_RESERVATION);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request.worker(), u32::try_from(index).unwrap());
        assert_eq!(request.preflight_id(), preflight_id);
        assert_eq!(request.granted_allowance(), allowance);
    }
    (state, requests)
}

fn check(
    state: &RunState,
    next: Option<&RunEffect>,
    requests: &[CreateWorker; 2],
    expected: &Value,
) {
    let machine_phase = match (expected["phase"]["tag"].as_str().unwrap(), next) {
        ("Copying", None) => RunPhase::Copy,
        ("Materialized", Some(RunEffect::VerifyOriginals(request))) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PostMaterialization);
            RunPhase::MaterializationVerification
        }
        ("VerifyingStop", Some(RunEffect::VerifyOriginals(request))) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        ("Diagnostic", Some(RunEffect::EmitOutput(output))) => {
            let OutputEvent::Diagnostic(diagnostic) = &output.event else {
                panic!("expected copy diagnostic: {output:?}");
            };
            assert_eq!(diagnostic.level, "error");
            assert_eq!(diagnostic.code, "fixture.copy.failed");
            assert_eq!(diagnostic.message, "fixture copy failure");
            RunPhase::Finalize
        }
        (phase, effect) => panic!("unexpected {phase} effect: {effect:?}"),
    };
    let mut pending = 0;
    for (index, request) in requests.iter().enumerate() {
        let slot = &expected["slots"][index]["tag"];
        let live = state.is_effect_pending(request.id());
        assert_eq!(live, *slot == "Pending");
        assert_eq!(state.is_effect_retired(request.id()), *slot == "Retired");
        pending += usize::from(live);
    }
    if let Some(effect) = next {
        pending += 1;
        assert!(state.is_effect_pending(effect.id()));
    }
    assert_eq!(state.pending_count(), pending);
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(
        u64::try_from(state.exit_code()).unwrap(),
        quint::number(&expected["exit"])
    );
}

fn worker_reply(value: &Value, requests: &[CreateWorker; 2]) -> WorkerCreated {
    let request = &requests[usize::try_from(quint::number(&value["request"])).unwrap()];
    WorkerCreated {
        id: request.id(),
        worker: u32::try_from(quint::number(&value["worker"])).unwrap(),
        reservation_id: if value["reservationMatches"] == true {
            request.reservation_id()
        } else {
            OTHER_RESERVATION
        },
    }
}

fn replay(history: &[Value]) {
    let (mut state, requests) = bootstrap();
    let mut next = None;
    check(&state, next.as_ref(), &requests, &history[0]["s"]);
    for row in &history[1..] {
        let dispatch = &row["dispatch"];
        let expected = &row["s"];
        let input = &dispatch["value"];
        let tag = dispatch["tag"].as_str().unwrap();
        if tag == "Idle" {
            check(&state, next.as_ref(), &requests, expected);
            continue;
        }
        let event = match tag {
            "Created" => RunEvent::WorkerCreated(worker_reply(input, &requests)),
            "Failed" => {
                let request = &requests[usize::try_from(quint::number(input)).unwrap()];
                RunEvent::EffectFailed(EffectFailed::other(
                    request.id(),
                    "fixture.copy.failed",
                    "fixture copy failure",
                ))
            }
            "Deadline" => RunEvent::DeadlineReached,
            other => panic!("unknown copy command {other}"),
        };
        let result = transition(state.clone(), event);
        let verdict = expected["result"]["tag"].as_str().unwrap();
        if verdict == "Accepted" {
            let (updated, mut effects) = result.unwrap();
            assert!(effects.len() <= 1);
            state = updated;
            next = effects.pop();
        } else {
            let reply = worker_reply(input, &requests);
            let error = result.unwrap_err();
            match verdict {
                "WrongReservation" => {
                    let request =
                        &requests[usize::try_from(quint::number(&input["request"])).unwrap()];
                    assert_eq!(error.code(), "machine.worker.reservation_mismatch");
                    assert_eq!(
                        error,
                        MachineError::WorkerReservationMismatch {
                            id: reply.id,
                            expected: request.reservation_id(),
                            received: reply.reservation_id,
                        },
                    );
                }
                "WrongWorker" => assert_eq!(error, MachineError::UnknownWorker(reply.worker)),
                "Duplicate" => assert_eq!(error, MachineError::DuplicateEffect(reply.id)),
                "Late" => assert_eq!(error, MachineError::RetiredEffect(reply.id)),
                other => panic!("unknown copy verdict {other}"),
            }
        }
        check(&state, next.as_ref(), &requests, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/worker_grant_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/worker_grant_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/worker_grant_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/worker_grant_3.itf.json"))]
#[case::reverse(include_str!("../../../../formal/quint/traces/worker_grant_reverseStep_0.itf.json"))]
#[case::wrong_reservation(include_str!("../../../../formal/quint/traces/worker_grant_wrongReservationStep_0.itf.json"))]
#[case::wrong_worker(include_str!("../../../../formal/quint/traces/worker_grant_wrongWorkerStep_0.itf.json"))]
#[case::failure(include_str!("../../../../formal/quint/traces/worker_grant_failureStep_0.itf.json"))]
#[case::deadline(include_str!("../../../../formal/quint/traces/worker_grant_deadlineStep_0.itf.json"))]
fn grant_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
