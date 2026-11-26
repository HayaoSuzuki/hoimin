//! Replay the warning queue through the first post-analysis request.
use super::{quint, waiting_for_analysis};
use hoimin_core::{
    AnalysisDiagnostic, AnalysisFinished, CandidateReadTarget, CandidateSpoolRef, EffectFailed,
    EffectId, IntegrityCheckpoint, MachineError, OutputEmitted, OutputEvent, RunEffect, RunEvent,
    RunPhase, RunState, transition,
};
use serde_json::Value;

fn warning(number: u64) -> AnalysisDiagnostic {
    AnalysisDiagnostic {
        code: format!("fixture.analysis.warning{number}"),
        message: format!("fixture analyzer warning {number}"),
    }
}

fn spool(records: u64) -> CandidateSpoolRef {
    CandidateSpoolRef {
        token: "fixture-analysis".to_owned(),
        records,
    }
}

fn finish(id: EffectId, input: &Value) -> RunEvent {
    let (spool, truncated) = match input["spool"]["tag"].as_str().unwrap() {
        "Absent" => (None, false),
        "Empty" => (Some(spool(0)), false),
        "Populated" => (Some(spool(2)), false),
        "Truncated" => (Some(spool(2)), true),
        other => panic!("unknown analysis spool {other}"),
    };
    let diagnostics = input["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|number| warning(quint::number(number)))
        .collect();
    RunEvent::AnalysisFinished(AnalysisFinished {
        id,
        spool,
        truncated,
        diagnostics,
    })
}

#[derive(Default)]
struct Warnings {
    issued: Vec<EffectId>,
    sequence: Option<u64>,
}

impl Warnings {
    fn check_output(&mut self, effect: &RunEffect, expected: &Value) {
        let RunEffect::EmitOutput(output) = effect else {
            return;
        };
        let OutputEvent::Diagnostic(diagnostic) = &output.event else {
            panic!("expected an analysis diagnostic: {output:?}");
        };
        assert_eq!(diagnostic.run_id, "run-1");
        if let Some(previous) = self.sequence {
            assert_eq!(diagnostic.sequence, previous + 1);
        }
        self.sequence = Some(diagnostic.sequence);

        if expected["phase"]["tag"] == "Warning" {
            let number = quint::number(&expected["issued"][self.issued.len()]);
            let wanted = warning(number);
            assert_eq!(diagnostic.level, "warning");
            assert_eq!(diagnostic.code, wanted.code);
            assert_eq!(diagnostic.message, wanted.message);
            self.issued.push(output.id);
        } else {
            assert_eq!(expected["phase"]["tag"], "ReportingFailure");
            assert_eq!(diagnostic.level, "error");
            assert_eq!(diagnostic.code, "fixture.analysis.failed");
            assert_eq!(diagnostic.message, "fixture diagnostic failure");
        }
    }
}

fn check(state: &RunState, pending: &RunEffect, warnings: &Warnings, expected: &Value) {
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let machine_phase = match (phase, pending) {
        ("Analyzing", RunEffect::AnalyzeFile(request)) => {
            assert!(request.final_target);
            RunPhase::Analyze
        }
        ("Warning", RunEffect::EmitOutput(_)) => RunPhase::Analyze,
        ("Reading", RunEffect::ReadCandidate(request)) => {
            assert_eq!(request.spool, spool(2));
            assert_eq!(request.target, CandidateReadTarget::Next);
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("Verifying" | "Cancelled", RunEffect::VerifyOriginals(request)) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        ("ReportingFailure", RunEffect::EmitOutput(_)) => RunPhase::Finalize,
        _ => panic!("expected {phase}, got {pending:?}"),
    };
    assert_eq!(state.phase(), machine_phase);
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    assert_eq!(
        u64::try_from(state.exit_code()).unwrap(),
        quint::number(&expected["exit"])
    );
    let acknowledged = usize::try_from(quint::number(&expected["acknowledged"])).unwrap();
    assert_eq!(
        warnings.issued.len(),
        expected["issued"].as_array().unwrap().len()
    );
    for (index, &id) in warnings.issued.iter().enumerate() {
        assert_eq!(
            state.is_effect_pending(id),
            index == acknowledged && phase == "Warning"
        );
        assert_eq!(
            state.is_effect_retired(id),
            index == acknowledged && phase == "Cancelled"
        );
    }
}

fn replay(history: &[Value]) {
    let (mut state, effects) = waiting_for_analysis();
    let [mut pending] = <[_; 1]>::try_from(effects).expect("one pending analysis request");
    let mut warnings = Warnings::default();
    check(&state, &pending, &warnings, &history[0]["s"]);

    for row in &history[1..] {
        let dispatch = &row["dispatch"];
        let expected = &row["s"];
        let payload = &dispatch["value"];
        let event = match dispatch["tag"].as_str().unwrap() {
            "Finish" => finish(pending.id(), payload),
            "Ack" => RunEvent::OutputEmitted(OutputEmitted {
                id: warnings.issued[usize::try_from(quint::number(payload)).unwrap()],
            }),
            "Cancel" => RunEvent::CancellationRequested,
            "FailOutput" => RunEvent::EffectFailed(EffectFailed::other(
                pending.id(),
                "fixture.analysis.failed",
                "fixture diagnostic failure",
            )),
            "Idle" => {
                check(&state, &pending, &warnings, expected);
                continue;
            }
            other => panic!("unknown analysis input {other}"),
        };
        let id = match &event {
            RunEvent::OutputEmitted(output) => output.id,
            _ => pending.id(),
        };
        let outcome = transition(state.clone(), event);
        match expected["result"]["tag"].as_str().unwrap() {
            "Accepted" => {
                let (updated, effects) = outcome.unwrap();
                [pending] = <[_; 1]>::try_from(effects).expect("one post-analysis request");
                warnings.check_output(&pending, expected);
                state = updated;
            }
            "Duplicate" => assert_eq!(outcome.unwrap_err(), MachineError::DuplicateEffect(id)),
            "Late" => assert_eq!(outcome.unwrap_err(), MachineError::RetiredEffect(id)),
            other => panic!("unknown analysis result {other}"),
        }
        check(&state, &pending, &warnings, expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/analysis_diagnostics_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/analysis_diagnostics_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/analysis_diagnostics_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/analysis_diagnostics_3.itf.json"))]
#[case::fifo(include_str!("../../../../formal/quint/traces/analysis_diagnostics_fifoStep_0.itf.json"))]
#[case::repeated(include_str!("../../../../formal/quint/traces/analysis_diagnostics_repeatedStep_0.itf.json"))]
#[case::truncated(include_str!("../../../../formal/quint/traces/analysis_diagnostics_truncatedStep_0.itf.json"))]
#[case::no_spool(include_str!("../../../../formal/quint/traces/analysis_diagnostics_noSpoolStep_0.itf.json"))]
#[case::cancel(include_str!("../../../../formal/quint/traces/analysis_diagnostics_cancelStep_0.itf.json"))]
#[case::failure(include_str!("../../../../formal/quint/traces/analysis_diagnostics_failureStep_0.itf.json"))]
fn analysis_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
