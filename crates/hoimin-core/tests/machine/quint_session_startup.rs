//! Replay session selection through the run-start output barrier.
use super::{fixture_config, fixture_session_config, pending_preflight_with, quint};
use hoimin_core::{
    EffectFailed, IntegrityCheckpoint, MachineError, OutputEmitted, OutputEvent,
    PreflightCompleted, ResumeFreshReason, ResumeOutcome, RunEffect, RunEvent, RunFingerprint,
    RunPhase, RunState, SessionLoadOutcome, SessionLoaded, SessionResumeRef, SessionStarted,
    transition,
};
use serde_json::Value;

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one startup request");
    effect
}

fn reason(value: &Value) -> ResumeFreshReason {
    match value["tag"].as_str().unwrap() {
        "NoPriorRun" => ResumeFreshReason::NoPriorRun,
        "MatchingRunComplete" => ResumeFreshReason::MatchingRunComplete,
        "BudgetDecreased" => ResumeFreshReason::BudgetDecreased,
        "FingerprintMismatch" => ResumeFreshReason::FingerprintMismatch,
        "NoIncompleteRun" => ResumeFreshReason::NoIncompleteRun,
        "CandidateChanged" => ResumeFreshReason::CandidateChanged,
        other => panic!("unknown fallback reason {other}"),
    }
}

fn evidence(value: &Value) -> Option<RunFingerprint> {
    let byte = u8::try_from(quint::number(value)).unwrap();
    (byte != 0).then(|| RunFingerprint::from_bytes([byte; 32]))
}

fn load(request: &RunEffect, outcome: SessionLoadOutcome) -> RunEvent {
    RunEvent::SessionLoaded(SessionLoaded {
        id: request.id(),
        outcome,
    })
}

fn resumed(request: &RunEffect) -> RunEvent {
    load(
        request,
        SessionLoadOutcome::Resumed(SessionResumeRef {
            run_id: "saved-run".into(),
        }),
    )
}

fn begun(request: &RunEffect) -> RunEvent {
    let RunEffect::BeginSession(begin) = request else {
        panic!("expected session begin request");
    };
    RunEvent::SessionStarted(SessionStarted {
        id: begin.id,
        run_id: begin.run_id.clone(),
    })
}

fn check(state: &RunState, pending: &RunEffect, preflight: &PreflightCompleted, expected: &Value) {
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let id = if quint::number(&expected["runNumber"]) == 2 {
        "saved-run"
    } else {
        "run-1"
    };
    assert_eq!(state.run_id(), id);
    let machine_phase = match (phase, pending) {
        ("Preflight", RunEffect::Preflight(request)) => {
            assert_eq!(request.id, preflight.id);
            RunPhase::Preflight
        }
        ("Loading", RunEffect::LoadSession(request)) => {
            assert_eq!(
                request.fingerprint,
                evidence(&expected["fingerprint"]).unwrap()
            );
            assert_eq!(request.max_mutants, fixture_config().limits.max_mutants);
            RunPhase::Preflight
        }
        ("Beginning", RunEffect::BeginSession(request)) => {
            assert_eq!(
                request.fingerprint,
                evidence(&expected["fingerprint"]).unwrap()
            );
            assert_eq!(request.max_mutants, fixture_config().limits.max_mutants);
            assert_eq!(request.run_id, id);
            RunPhase::Preflight
        }
        ("Reporting", RunEffect::EmitOutput(output)) => {
            let OutputEvent::RunStarted(started) = &output.event else {
                panic!("expected run-start header: {output:?}");
            };
            let provenance = &expected["provenance"];
            let resume = match provenance["tag"].as_str().unwrap() {
                "NoProvenance" => None,
                "Resumed" => Some(ResumeOutcome::Resumed),
                "Fresh" => Some(ResumeOutcome::Fresh {
                    reason: reason(&provenance["value"]),
                }),
                other => panic!("unexpected provenance {other}"),
            };
            assert_eq!(started.run_id, id);
            assert_eq!(started.resume, resume);
            RunPhase::Preflight
        }
        ("Copying", RunEffect::CreateWorker(request)) => {
            assert_eq!(request.preflight_id(), preflight.id);
            assert_eq!(
                request.granted_allowance(),
                preflight.aggregate_logical_bytes
            );
            assert_eq!(request.worker(), 0);
            RunPhase::Copy
        }
        ("Verifying", RunEffect::VerifyOriginals(request)) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        ("Diagnostic", RunEffect::EmitOutput(output)) => {
            let OutputEvent::Diagnostic(diagnostic) = &output.event else {
                panic!("expected session I/O diagnostic: {output:?}");
            };
            assert_eq!(diagnostic.level, "error");
            assert_eq!(diagnostic.code, "fixture.session.start");
            assert_eq!(diagnostic.message, "fixture session failure");
            RunPhase::Finalize
        }
        _ => panic!("unexpected {phase} effect: {pending:?}"),
    };
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    let exit = match expected["cause"]["tag"].as_str().unwrap() {
        "Healthy" => 0,
        "DeadlineStop" => 4,
        "IoFailure" => 2,
        other => panic!("unexpected startup cause {other}"),
    };
    assert_eq!(state.exit_code(), exit);
}

fn input(dispatch: &Value, pending: &RunEffect, preflight: &PreflightCompleted) -> RunEvent {
    let value = &dispatch["value"];
    match dispatch["tag"].as_str().unwrap() {
        "Measure" => {
            let mut finished = preflight.clone();
            finished.fingerprint = evidence(value);
            RunEvent::PreflightCompleted(finished)
        }
        "Load" if value["tag"] == "Reuse" => resumed(pending),
        "Load" => load(pending, SessionLoadOutcome::Fresh(reason(&value["value"]))),
        "Begin" => begun(pending),
        "AckReport" => RunEvent::OutputEmitted(OutputEmitted { id: pending.id() }),
        "Deadline" => RunEvent::DeadlineReached,
        "Fail" => RunEvent::EffectFailed(EffectFailed::other(
            pending.id(),
            "fixture.session.start",
            "fixture session failure",
        )),
        other => panic!("unknown session startup input {other}"),
    }
}

fn replay(history: &[Value]) {
    let [_, initial, commands @ ..] = history else {
        panic!("expected a configured session startup");
    };
    let configuration = &initial["dispatch"];
    assert_eq!(configuration["tag"], "Configure");
    let config = match configuration["value"]["tag"].as_str().unwrap() {
        "Disabled" => fixture_config(),
        "FreshMode" => fixture_session_config(false),
        "ResumeMode" => fixture_session_config(true),
        other => panic!("unknown session mode {other}"),
    };
    let (mut state, preflight) = pending_preflight_with(config);
    let mut pending = RunEffect::Preflight(hoimin_core::Preflight { id: preflight.id });
    let mut superseded = None;
    check(&state, &pending, &preflight, &initial["s"]);
    for row in commands {
        let dispatch = &row["dispatch"];
        let expected = &row["s"];
        let tag = dispatch["tag"].as_str().unwrap();
        if tag == "Probe" {
            let previous: &RunEffect = superseded.as_ref().expect("interrupted session request");
            let reply = match previous {
                RunEffect::LoadSession(_) => resumed(previous),
                RunEffect::BeginSession(_) => begun(previous),
                other => panic!("unexpected prior request: {other:?}"),
            };
            let error = if expected["result"]["tag"] == "Late" {
                MachineError::RetiredEffect(previous.id())
            } else {
                assert_eq!(expected["result"]["tag"], "Duplicate");
                MachineError::DuplicateEffect(previous.id())
            };
            assert_eq!(transition(state.clone(), reply).unwrap_err(), error);
        } else if tag != "Idle" {
            let outcome = transition(state.clone(), input(dispatch, &pending, &preflight));
            if expected["result"]["tag"] == "MissingFingerprint" {
                assert_eq!(outcome.unwrap_err(), MachineError::MissingFingerprint);
            } else {
                let (updated, effects) = outcome.unwrap();
                if matches!(tag, "Deadline" | "Fail") {
                    superseded = Some(pending);
                }
                pending = one(effects);
                state = updated;
            }
        }
        check(&state, &pending, &preflight, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/session_startup_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/session_startup_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/session_startup_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/session_startup_3.itf.json"))]
#[case::disabled(include_str!("../../../../formal/quint/traces/session_startup_disabledStep_0.itf.json"))]
#[case::resumed(include_str!("../../../../formal/quint/traces/session_startup_resumedStep_0.itf.json"))]
#[case::fallback(include_str!("../../../../formal/quint/traces/session_startup_fallbackStep_0.itf.json"))]
#[case::missing_fingerprint(include_str!("../../../../formal/quint/traces/session_startup_missingFingerprintStep_0.itf.json"))]
#[case::deadline_load(include_str!("../../../../formal/quint/traces/session_startup_deadlineLoadStep_0.itf.json"))]
#[case::failed_begin(include_str!("../../../../formal/quint/traces/session_startup_failedBeginStep_0.itf.json"))]
fn startup_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 2);
    replay(&history);
}
