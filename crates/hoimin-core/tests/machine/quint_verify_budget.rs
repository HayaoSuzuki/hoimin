//! Replay the ordered verification budget observation and warning barrier.
use super::{quint, waiting_for_top_budget_observation};
use hoimin_core::{
    EffectFailed, EffectId, IntegrityCheckpoint, MachineError, OutputEmitted, OutputEvent,
    RemainingBudgetObserved, RunEffect, RunEvent, RunPhase, RunState, transition,
};
use serde_json::Value;
use std::time::Duration;

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one verification request");
    effect
}

fn check(
    state: &RunState,
    pending: &RunEffect,
    observation: EffectId,
    warning: Option<EffectId>,
    expected: &Value,
) {
    let phase = match (expected["phase"]["tag"].as_str().unwrap(), pending) {
        ("Observing", RunEffect::ObserveRemainingBudget(_)) => RunPhase::BudgetCheck,
        ("Warning", RunEffect::EmitOutput(output)) => {
            let OutputEvent::Diagnostic(diagnostic) = &output.event else {
                panic!("expected budget diagnostic: {output:?}");
            };
            assert_eq!(diagnostic.level, "warning");
            assert_eq!(diagnostic.code, "budget.projected_shortfall");
            assert_eq!(diagnostic.run_id, "run-1");
            for field in [
                "selected=30".to_owned(),
                "jobs=1".to_owned(),
                "planned_total_timeout=300s".to_owned(),
                "baseline=17s".to_owned(),
                "effective_mutant_timeout=35s".to_owned(),
                format!(
                    "remaining={}s",
                    quint::number(&expected["remaining"]) / 1000
                ),
                "projected_capacity=1050s".to_owned(),
            ] {
                assert!(diagnostic.message.contains(&field), "{diagnostic:?}");
            }
            RunPhase::BudgetCheck
        }
        ("Analyzing", RunEffect::AnalyzeFile(request)) => {
            assert!(request.final_target);
            assert_eq!(request.target.path.as_str(), "src/calc.py");
            RunPhase::Analyze
        }
        ("Verifying", RunEffect::VerifyOriginals(request)) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        ("ReportingFailure", RunEffect::EmitOutput(output)) => {
            let OutputEvent::Diagnostic(diagnostic) = &output.event else {
                panic!("expected failure diagnostic: {output:?}");
            };
            assert_eq!(diagnostic.level, "error");
            assert_eq!(diagnostic.code, "fixture.verify-budget.failed");
            assert_eq!(diagnostic.message, "fixture budget failure");
            RunPhase::Finalize
        }
        (phase, effect) => panic!("unexpected {phase} effect: {effect:?}"),
    };
    assert_eq!(state.phase(), phase);
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    assert_eq!(
        u64::try_from(state.exit_code()).unwrap(),
        quint::number(&expected["exit"])
    );
    for (id, slot) in [
        (Some(observation), &expected["observation"]),
        (warning, &expected["warning"]),
    ] {
        assert_eq!(id.is_none(), slot["tag"] == "Absent");
        if let Some(id) = id {
            assert_eq!(state.is_effect_pending(id), slot["tag"] == "Pending");
            assert_eq!(state.is_effect_retired(id), slot["tag"] == "Retired");
        }
    }
}

fn replay(history: &[Value]) {
    let (mut state, effects) = waiting_for_top_budget_observation();
    let mut pending = one(effects);
    let observation_id = pending.id();
    let mut warning_id = None;
    check(
        &state,
        &pending,
        observation_id,
        warning_id,
        &history[0]["s"],
    );
    for row in &history[1..] {
        let input = &row["dispatch"];
        let expected = &row["s"];
        let (event, callback) = match input["tag"].as_str().unwrap() {
            "Observe" => (
                RunEvent::RemainingBudgetObserved(RemainingBudgetObserved {
                    id: observation_id,
                    remaining: Duration::from_millis(quint::number(&input["value"])),
                }),
                Some(observation_id),
            ),
            "Ack" => {
                let id = warning_id.expect("one issued budget warning");
                (RunEvent::OutputEmitted(OutputEmitted { id }), Some(id))
            }
            "Cancel" => (RunEvent::CancellationRequested, None),
            "Deadline" => (RunEvent::DeadlineReached, None),
            "FailRequest" => (
                RunEvent::EffectFailed(EffectFailed::other(
                    pending.id(),
                    "fixture.verify-budget.failed",
                    "fixture budget failure",
                )),
                None,
            ),
            other => panic!("unknown budget input {other}"),
        };
        let outcome = transition(state.clone(), event);
        match expected["result"]["tag"].as_str().unwrap() {
            "Accepted" => {
                let (updated, effects) = outcome.unwrap();
                pending = one(effects);
                if expected["phase"]["tag"] == "Warning" {
                    assert!(warning_id.replace(pending.id()).is_none());
                }
                state = updated;
            }
            "Duplicate" => assert_eq!(
                outcome.unwrap_err(),
                MachineError::DuplicateEffect(callback.unwrap())
            ),
            "Late" => assert_eq!(
                outcome.unwrap_err(),
                MachineError::RetiredEffect(callback.unwrap())
            ),
            other => panic!("unknown budget result {other}"),
        }
        check(&state, &pending, observation_id, warning_id, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/verify_budget_gate_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/verify_budget_gate_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/verify_budget_gate_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/verify_budget_gate_3.itf.json"))]
#[case::below(include_str!("../../../../formal/quint/traces/verify_budget_gate_belowStep_0.itf.json"))]
#[case::equal(include_str!("../../../../formal/quint/traces/verify_budget_gate_equalStep_0.itf.json"))]
#[case::above(include_str!("../../../../formal/quint/traces/verify_budget_gate_aboveStep_0.itf.json"))]
#[case::cancel_warning(include_str!("../../../../formal/quint/traces/verify_budget_gate_cancelWarningStep_0.itf.json"))]
#[case::deadline_observation(include_str!("../../../../formal/quint/traces/verify_budget_gate_deadlineObservationStep_0.itf.json"))]
#[case::fail_warning(include_str!("../../../../formal/quint/traces/verify_budget_gate_failWarningStep_0.itf.json"))]
fn budget_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
