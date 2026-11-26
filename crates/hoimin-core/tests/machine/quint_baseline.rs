//! Replay independent baseline output and restoration acknowledgements.
use super::{quint, waiting_for_baseline_restoration_with_jobs};
use hoimin_core::{
    CompletionKind, EffectFailed, EffectId, IntegrityCheckpoint, MachineError, OutputEmitted,
    OutputEvent, RunEffect, RunEvent, RunPhase, RunState, WorkerReset, transition,
};
use serde_json::Value;

fn check(state: &RunState, reset: EffectId, output: EffectId, expected: &Value) {
    let mut pending = 0;
    for (id, slot) in [(reset, &expected["reset"]), (output, &expected["output"])] {
        assert_eq!(state.is_effect_pending(id), slot["tag"] == "Pending");
        assert_eq!(state.is_effect_retired(id), slot["tag"] == "Retired");
        pending += usize::from(state.is_effect_pending(id));
    }
    let phase = match expected["phase"]["tag"].as_str().unwrap() {
        "Restoring" => RunPhase::BaselineRestoration,
        "Analyzing" => RunPhase::Analyze,
        "Verifying" | "ReportingFailure" => RunPhase::Finalize,
        other => panic!("unknown baseline phase {other}"),
    };
    pending += usize::from(phase != RunPhase::BaselineRestoration);
    assert_eq!(state.pending_count(), pending);
    assert_eq!(state.phase(), phase);
    assert_eq!(
        u64::try_from(state.exit_code()).unwrap(),
        quint::number(&expected["exit"])
    );
}

fn failure(id: EffectId) -> RunEvent {
    RunEvent::EffectFailed(EffectFailed::other(
        id,
        "fixture.baseline.failed",
        "fixture baseline failure",
    ))
}

fn check_followup(command: &str, effects: &[RunEffect]) {
    match (command, effects) {
        ("Reset", [RunEffect::AnalyzeFile(request)]) => assert!(request.final_target),
        ("Output", []) => {}
        ("Cancel", [RunEffect::VerifyOriginals(request)]) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
        }
        ("FailReset" | "FailOutput", [RunEffect::EmitOutput(request)]) => {
            let OutputEvent::Diagnostic(diagnostic) = &request.event else {
                panic!("expected baseline failure diagnostic: {request:?}");
            };
            assert_eq!(diagnostic.level, "error");
            assert_eq!(diagnostic.code, "fixture.baseline.failed");
            assert_eq!(diagnostic.message, "fixture baseline failure");
        }
        _ => panic!("unexpected {command} effects: {effects:?}"),
    }
}

fn replay(history: &[Value]) {
    let (mut state, requests) = waiting_for_baseline_restoration_with_jobs(2);
    let [
        RunEffect::EmitOutput(report),
        RunEffect::ResetWorker(restoration),
    ] = requests.as_slice()
    else {
        panic!("expected baseline output and reset: {requests:?}");
    };
    assert!(matches!(report.event, OutputEvent::BaselineFinished(_)));
    assert_eq!(restoration.worker, 0);
    let (reset, output) = (restoration.id, report.id);
    check(&state, reset, output, &history[0]["s"]);

    for row in &history[1..] {
        let command = row["dispatch"]["tag"].as_str().unwrap();
        let event = match command {
            "Reset" | "WrongWorker" => RunEvent::WorkerReset(WorkerReset {
                id: reset,
                worker: u32::from(command == "WrongWorker"),
            }),
            "Output" => RunEvent::OutputEmitted(OutputEmitted { id: output }),
            "WrongKind" => RunEvent::OutputEmitted(OutputEmitted { id: reset }),
            "Cancel" => RunEvent::CancellationRequested,
            "FailReset" => failure(reset),
            "FailOutput" => failure(output),
            other => panic!("unknown baseline input {other}"),
        };
        let expected = &row["s"];
        let id = if command == "Output" { output } else { reset };
        let outcome = transition(state.clone(), event);
        match expected["result"]["tag"].as_str().unwrap() {
            "Accepted" => {
                let (updated, effects) = outcome.unwrap();
                check_followup(command, &effects);
                state = updated;
            }
            "Duplicate" => assert_eq!(outcome.unwrap_err(), MachineError::DuplicateEffect(id)),
            "Late" => assert_eq!(outcome.unwrap_err(), MachineError::RetiredEffect(id)),
            "WorkerMismatch" => assert_eq!(outcome.unwrap_err(), MachineError::UnknownWorker(1)),
            "KindMismatch" => assert_eq!(
                outcome.unwrap_err(),
                MachineError::WrongCompletion {
                    id: reset,
                    expected: CompletionKind::WorkerReset,
                    received: CompletionKind::OutputEmitted,
                }
            ),
            other => panic!("unknown baseline result {other}"),
        }
        check(&state, reset, output, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/baseline_restoration_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/baseline_restoration_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/baseline_restoration_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/baseline_restoration_3.itf.json"))]
#[case::output_first(include_str!("../../../../formal/quint/traces/baseline_restoration_outputFirstStep_0.itf.json"))]
#[case::reset_first(include_str!("../../../../formal/quint/traces/baseline_restoration_resetFirstStep_0.itf.json"))]
#[case::rejections(include_str!("../../../../formal/quint/traces/baseline_restoration_rejectionsStep_0.itf.json"))]
#[case::failure(include_str!("../../../../formal/quint/traces/baseline_restoration_failureStep_0.itf.json"))]
#[case::stop(include_str!("../../../../formal/quint/traces/baseline_restoration_stopStep_0.itf.json"))]
fn restoration_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
