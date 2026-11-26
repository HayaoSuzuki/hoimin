//! Replay preflight accounting and worker count validation through public transitions.
use super::{fixture_raw_config, pending_preflight_with, quint};
use hoimin_core::{
    BudgetKind, EffectId, EnvironmentSnapshot, LimitReached, MachineError, OutputEmitted,
    OutputEvent, PreflightCompleted, ReserveError, RunConfig, RunEffect, RunEvent, RunFingerprint,
    RunPhase, RunState, SessionConfig, WorkspaceBudgetError, transition,
};
use serde_json::Value;

const WORKERS: u32 = 2;
const CAPACITY: u64 = 20;

fn configure(mode: &str) -> RunConfig {
    let mut raw = fixture_raw_config();
    raw.limits.jobs = usize::try_from(WORKERS).unwrap();
    raw.limits.max_copy_size = CAPACITY;
    if mode != "Disabled" {
        raw.session = Some(SessionConfig {
            path: "session.sqlite3".into(),
        });
        raw.resume = mode == "Resume";
    }
    RunConfig::try_from(raw)
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap()
}

fn measurement(preflight: &PreflightCompleted, value: &Value) -> PreflightCompleted {
    let per_worker = &value["perWorker"];
    let mut measured = preflight.clone();
    measured.per_worker_logical_bytes = match per_worker["tag"].as_str().unwrap() {
        "Bytes" => quint::number(&per_worker["value"]),
        "Overflow" => u64::MAX,
        other => panic!("unknown copy measurement {other}"),
    };
    measured.requested_workers = u32::try_from(quint::number(&value["workers"])).unwrap();
    measured.aggregate_logical_bytes = quint::number(&value["total"]);
    measured
}

fn rejection(verdict: &str, preflight: &PreflightCompleted) -> MachineError {
    let budget_error = match verdict {
        "CountMismatch" => {
            return MachineError::WorkerCountMismatch {
                configured: WORKERS,
                preflight: preflight.requested_workers,
            };
        }
        "Duplicate" => return MachineError::DuplicateEffect(preflight.id),
        "AggregateOverflow" => WorkspaceBudgetError::AggregateOverflow,
        "AggregateMismatch" => WorkspaceBudgetError::AggregateMismatch {
            per_worker: preflight.per_worker_logical_bytes,
            workers: preflight.requested_workers,
            reported: preflight.aggregate_logical_bytes,
        },
        "LimitExceeded" => {
            WorkspaceBudgetError::Reserve(ReserveError::LimitReached(LimitReached {
                kind: BudgetKind::Copy,
                requested: preflight.aggregate_logical_bytes,
                available: CAPACITY,
            }))
        }
        other => panic!("unknown preflight verdict {other}"),
    };
    MachineError::Budget(budget_error.to_string())
}

fn check_workers(pending: &[RunEffect], preflight: EffectId, allowance: u64) {
    let [
        RunEffect::CreateWorker(first),
        RunEffect::CreateWorker(second),
    ] = pending
    else {
        panic!("expected both worker requests: {pending:?}");
    };
    assert_eq!(first.reservation_id(), second.reservation_id());
    assert_ne!(first.id(), second.id());
    for (worker, request) in [(0, first), (1, second)] {
        assert_eq!(request.worker(), worker);
        assert_eq!(request.preflight_id(), preflight);
        assert_eq!(request.granted_allowance(), allowance);
    }
}

fn check(
    state: &RunState,
    pending: &[RunEffect],
    preflight: &PreflightCompleted,
    expected: &Value,
) {
    let expected_effect = expected["stage"]["tag"].as_str().unwrap();
    let phase = match (expected_effect, pending) {
        ("Awaiting", [RunEffect::Preflight(request)]) => {
            assert_eq!(request.id, preflight.id);
            RunPhase::Preflight
        }
        ("Report", [RunEffect::EmitOutput(output)]) => {
            assert!(matches!(output.event, OutputEvent::RunStarted(_)));
            RunPhase::Preflight
        }
        ("BeginSession", [RunEffect::BeginSession(request)]) => {
            assert_eq!(request.run_id, state.run_id());
            assert_eq!(request.fingerprint, preflight.fingerprint.unwrap());
            RunPhase::Preflight
        }
        ("LoadSession", [RunEffect::LoadSession(request)]) => {
            assert_eq!(request.fingerprint, preflight.fingerprint.unwrap());
            RunPhase::Preflight
        }
        ("Workers", _) => {
            check_workers(
                pending,
                preflight.id,
                quint::number(&expected["evidence"]["total"]),
            );
            RunPhase::Copy
        }
        _ => panic!("expected {expected_effect}, got {pending:?}"),
    };
    assert_eq!(state.phase(), phase);
    assert_eq!(state.exit_code(), 0);
    assert_eq!(state.pending_count(), pending.len());
    assert!(
        pending
            .iter()
            .all(|effect| state.is_effect_pending(effect.id()))
    );
}

fn replay(history: &[Value]) {
    let [_, configured, commands @ ..] = history else {
        panic!("expected a configured copy budget");
    };
    let configure_command = &configured["dispatch"];
    assert_eq!(configure_command["tag"], "Configure");
    let config = configure(configure_command["value"]["tag"].as_str().unwrap());
    let (mut state, preflight) = pending_preflight_with(config);
    if configure_command["value"]["tag"] != "Disabled" {
        assert_eq!(
            preflight.fingerprint,
            Some(RunFingerprint::from_bytes([7; 32]))
        );
    }
    let mut pending = vec![RunEffect::Preflight(hoimin_core::Preflight {
        id: preflight.id,
    })];
    check(&state, &pending, &preflight, &configured["s"]);
    for row in commands {
        let dispatch = &row["dispatch"];
        let expected = &row["s"];
        match dispatch["tag"].as_str().unwrap() {
            "Measure" => {
                let measured = measurement(&preflight, &dispatch["value"]);
                let outcome = transition(
                    state.clone(),
                    RunEvent::PreflightCompleted(measured.clone()),
                );
                let verdict = expected["result"]["tag"].as_str().unwrap();
                if verdict == "Accepted" {
                    (state, pending) = outcome.unwrap();
                } else {
                    assert_eq!(outcome.unwrap_err(), rejection(verdict, &measured));
                }
            }
            "AckReport" => {
                let [RunEffect::EmitOutput(output)] = pending.as_slice() else {
                    panic!("expected run-start header before worker requests");
                };
                (state, pending) = transition(
                    state,
                    RunEvent::OutputEmitted(OutputEmitted { id: output.id }),
                )
                .unwrap();
            }
            "Idle" => {}
            other => panic!("unknown copy-preflight input {other}"),
        }
        check(&state, &pending, &preflight, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/copy_preflight_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/copy_preflight_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/copy_preflight_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/copy_preflight_3.itf.json"))]
#[case::ordinary(include_str!("../../../../formal/quint/traces/copy_preflight_ordinaryStep_0.itf.json"))]
#[case::fresh_retry(include_str!("../../../../formal/quint/traces/copy_preflight_freshRetryStep_0.itf.json"))]
#[case::resume_retry(include_str!("../../../../formal/quint/traces/copy_preflight_resumeRetryStep_0.itf.json"))]
#[case::empty(include_str!("../../../../formal/quint/traces/copy_preflight_emptyStep_0.itf.json"))]
fn copy_preflight_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 2);
    replay(&history);
}
