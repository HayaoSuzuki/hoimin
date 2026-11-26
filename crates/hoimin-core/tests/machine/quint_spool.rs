//! Compare the spool handoff for ordinary and explicitly selected runs.
use super::{
    complete_baseline_restoration, explicit_selection, fixture_config, process_finished, quint,
    test_resource_control, waiting_for_baseline_with_targets,
};
use hoimin_core::{
    AnalysisFinished, CandidateReadTarget, CandidateSpoolRef, EffectId, IntegrityCheckpoint,
    MachineError, OutputEmitted, OutputEvent, ProcessTermination, RunEffect, RunEvent, RunPhase,
    RunSelection, RunState, TargetSlice, transition,
};
use serde_json::Value;

const PATHS: [&str; 2] = ["src/a.py", "src/b.py"];
const SELECTED: &str = "m1_selected";

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one analysis or handoff request");
    effect
}

fn bootstrap(mode: &str) -> (RunState, RunEffect) {
    let selection = match mode {
        "Ordinary" => RunSelection::all(),
        "Explicit" => explicit_selection([SELECTED.to_owned()]),
        other => panic!("unknown spool test mode {other}"),
    };
    let initial = RunState::with_selection(
        "run-1",
        fixture_config(),
        selection,
        test_resource_control(),
    );
    let targets = PATHS.map(|path| TargetSlice {
        path: path.into(),
        lines: Vec::new(),
        symbols: Vec::new(),
    });
    let (state, effects) = waiting_for_baseline_with_targets(initial, targets.into());
    let baseline = one(effects);
    assert!(matches!(baseline, RunEffect::RunBaseline(_)));
    let (state, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(baseline.id(), ProcessTermination::Exit(0))),
    )
    .unwrap();
    let [RunEffect::EmitOutput(report), RunEffect::ResetWorker(_)] = effects.as_slice() else {
        panic!("expected baseline report and restoration: {effects:?}");
    };
    assert!(matches!(report.event, OutputEvent::BaselineFinished(_)));
    let (state, emitted) = transition(
        state,
        RunEvent::OutputEmitted(OutputEmitted { id: report.id }),
    )
    .unwrap();
    assert_eq!(emitted, Vec::new());
    let (state, effects) = complete_baseline_restoration(state, &effects);
    (state, one(effects))
}

fn index(target: &Value) -> usize {
    usize::try_from(quint::number(target) - 1).unwrap()
}

fn spool(target: usize, records: u64) -> CandidateSpoolRef {
    CandidateSpoolRef {
        token: format!("fixture-spool-{}", target + 1),
        records,
    }
}

fn check(
    state: &RunState,
    pending: &RunEffect,
    requests: &[Option<EffectId>; 2],
    expected: &Value,
) {
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let current = index(&expected["target"]);
    let rust_phase = match (phase, pending) {
        ("Analyzing", RunEffect::AnalyzeFile(request)) => {
            assert_eq!(request.target.path.as_str(), PATHS[current]);
            assert_eq!(request.final_target, current == 1);
            RunPhase::Analyze
        }
        ("Reading", RunEffect::ReadCandidate(request)) => {
            assert_eq!(request.target, CandidateReadTarget::Next);
            assert_eq!(request.spool, spool(current, 2));
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("Verifying" | "Stopped", RunEffect::VerifyOriginals(request)) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        _ => panic!("expected {phase}, got {pending:?}"),
    };
    assert_eq!(state.phase(), rust_phase);
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    assert_eq!(
        u64::try_from(state.exit_code()).unwrap(),
        quint::number(&expected["exit"])
    );
    let completed = quint::set(&expected["completed"]);
    for (target, request) in requests.iter().enumerate() {
        assert_eq!(request.is_some(), target <= current);
        if let Some(id) = request {
            let completed = completed.iter().any(|entry| index(entry) == target);
            assert_eq!(
                state.is_effect_pending(*id),
                !completed && phase == "Analyzing"
            );
            assert_eq!(
                state.is_effect_retired(*id),
                !completed && phase == "Stopped"
            );
        }
    }
}

fn analysis_finished(input: &Value, requests: &[Option<EffectId>; 2]) -> AnalysisFinished {
    let target = index(&input["target"]);
    let spool = match input["spool"]["tag"].as_str().unwrap() {
        "Absent" => None,
        "Empty" => Some(spool(target, 0)),
        "Populated" => Some(spool(target, 2)),
        other => panic!("unknown spool shape {other}"),
    };
    AnalysisFinished {
        id: requests[target].expect("issued analysis request"),
        spool,
        truncated: input["truncated"].as_bool().unwrap(),
        diagnostics: Vec::new(),
    }
}

fn replay(history: &[Value]) {
    let [_, initial, commands @ ..] = history else {
        panic!("expected a configured run");
    };
    let start = &initial["dispatch"];
    assert_eq!(start["tag"], "Start");
    let (mut state, mut pending) = bootstrap(start["value"]["tag"].as_str().unwrap());
    let mut requests = [Some(pending.id()), None];
    check(&state, &pending, &requests, &initial["s"]);

    for row in commands {
        let dispatch = &row["dispatch"];
        let (event, callback) = match dispatch["tag"].as_str().unwrap() {
            "Finish" => {
                let completion = analysis_finished(&dispatch["value"], &requests);
                let id = completion.id;
                (RunEvent::AnalysisFinished(completion), Some(id))
            }
            "Deadline" => (RunEvent::DeadlineReached, None),
            other => panic!("unknown spool input {other}"),
        };
        let expected = &row["s"];
        let outcome = transition(state.clone(), event);
        match expected["result"]["tag"].as_str().unwrap() {
            "Accepted" => {
                let (updated, effects) = outcome.unwrap();
                pending = one(effects);
                if expected["phase"]["tag"] == "Analyzing" {
                    assert!(requests[1].replace(pending.id()).is_none());
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
            "MissingSpool" => assert_eq!(outcome.unwrap_err(), MachineError::MissingCandidateSpool),
            "MissingCandidate" => assert_eq!(
                outcome.unwrap_err(),
                MachineError::SelectedCandidateMissing(SELECTED.to_owned()),
            ),
            other => panic!("unknown spool result {other}"),
        }
        check(&state, &pending, &requests, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/analysis_spool_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/analysis_spool_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/analysis_spool_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/analysis_spool_3.itf.json"))]
#[case::ordinary_truncated(include_str!("../../../../formal/quint/traces/analysis_spool_ordinaryTruncatedStep_0.itf.json"))]
#[case::explicit_truncated(include_str!("../../../../formal/quint/traces/analysis_spool_explicitTruncatedStep_0.itf.json"))]
#[case::ordinary_empty(include_str!("../../../../formal/quint/traces/analysis_spool_ordinaryEmptyStep_0.itf.json"))]
#[case::ordinary_no_spool(include_str!("../../../../formal/quint/traces/analysis_spool_ordinaryNoSpoolStep_0.itf.json"))]
#[case::explicit_retry(include_str!("../../../../formal/quint/traces/analysis_spool_explicitRetryStep_0.itf.json"))]
#[case::deadline(include_str!("../../../../formal/quint/traces/analysis_spool_deadlineStep_0.itf.json"))]
fn spool_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 2);
    replay(&history);
}
