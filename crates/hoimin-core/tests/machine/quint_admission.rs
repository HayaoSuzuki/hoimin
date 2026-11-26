//! Replay two workers competing for a single real mutant admission.
use super::{
    fixture_candidate, fixture_raw_config, process_finished, quint, test_resource_control,
    waiting_for_selected_analysis,
};
use hoimin_core::{
    AnalysisFinished, CandidateLoaded, CandidateReadTarget, CandidateSpoolRef, EnvironmentSnapshot,
    IntegrityCheckpoint, MutationApplied, MutationOutcome, MutationStatus, OutputEmitted,
    OutputEvent, ProcessTermination, RunConfig, RunEffect, RunEvent, RunPhase, RunState,
    WorkerReset, transition,
};
use serde_json::Value;

fn spool(records: u64) -> CandidateSpoolRef {
    CandidateSpoolRef {
        token: "fixture-admission".to_owned(),
        records,
    }
}

fn bootstrap(records: u64) -> (RunState, Vec<RunEffect>) {
    let mut raw = fixture_raw_config();
    raw.limits.jobs = 2;
    raw.limits.max_mutants = 1;
    let config = RunConfig::try_from(raw)
        .unwrap()
        .with_environment(EnvironmentSnapshot::Untracked)
        .unwrap();
    let initial = RunState::new("run-1", config, test_resource_control());
    let (state, effects) = waiting_for_selected_analysis(initial, 2);
    let [RunEffect::AnalyzeFile(request)] = effects.as_slice() else {
        panic!("expected analysis, got {effects:?}");
    };
    transition(
        state,
        RunEvent::AnalysisFinished(AnalysisFinished {
            id: request.id,
            spool: Some(spool(records)),
            truncated: false,
            diagnostics: Vec::new(),
        }),
    )
    .unwrap()
}

fn worker(effect: &RunEffect) -> Option<u32> {
    let id = match effect {
        RunEffect::ReadCandidate(request) => return Some(request.worker),
        RunEffect::ApplyMutation(request) => return Some(request.worker),
        RunEffect::RunMutant(request) => return Some(request.context.worker),
        RunEffect::ResetWorker(request) => return Some(request.worker),
        RunEffect::VerifyOriginals(_) => return None,
        RunEffect::EmitOutput(output) => match &output.event {
            OutputEvent::MutantStarted(value) => value.mutant_id.as_str(),
            OutputEvent::MutantFinished(value) => value.candidate.id.as_str(),
            other => panic!("unexpected admission output {other:?}"),
        },
        other => panic!("unexpected admission effect {other:?}"),
    };
    match id {
        "m1" => Some(0),
        "m2" => Some(1),
        other => panic!("unexpected admission candidate {other}"),
    }
}

fn check_worker(slot: &Value, effect: &RunEffect, records: u64) {
    let candidate = quint::number(&slot["candidate"]);
    match (slot["stage"]["tag"].as_str().unwrap(), effect) {
        ("Read", RunEffect::ReadCandidate(request)) => {
            assert_eq!(request.target, CandidateReadTarget::Next);
            assert_eq!(request.spool, spool(records));
        }
        ("Apply", RunEffect::ApplyMutation(request)) => {
            assert_eq!(request.candidate, fixture_candidate(candidate));
        }
        ("RunReal", RunEffect::RunMutant(request)) => {
            assert_eq!(request.context.mutant_id, format!("m{candidate}"));
        }
        ("Reset", RunEffect::ResetWorker(_)) => {}
        (
            stage @ ("StartReal" | "StartSynthetic" | "FinishReal" | "FinishSynthetic"),
            RunEffect::EmitOutput(output),
        ) => match (stage, &output.event) {
            ("StartReal" | "StartSynthetic", OutputEvent::MutantStarted(value)) => {
                assert_eq!(value.mutant_id, format!("m{candidate}"));
                assert_eq!(value.mutant_sequence, candidate);
            }
            ("FinishReal" | "FinishSynthetic", OutputEvent::MutantFinished(value)) => {
                assert_eq!(value.candidate, fixture_candidate(candidate));
                if stage == "FinishReal" {
                    assert_eq!(value.outcome.status(), MutationStatus::Killed);
                } else {
                    assert_eq!(value.outcome, MutationOutcome::NotRun);
                }
            }
            _ => panic!("unexpected {stage} output: {output:?}"),
        },
        (stage, effect) => panic!("expected {stage}, got {effect:?}"),
    }
}

struct Counts {
    admitted: usize,
    started: usize,
}

fn check(state: &RunState, pending: &[RunEffect], counts: &Counts, records: u64, expected: &Value) {
    assert_eq!(state.pending_count(), pending.len());
    assert!(
        pending
            .iter()
            .all(|effect| state.is_effect_pending(effect.id()))
    );
    if expected["verifying"] == true {
        let [RunEffect::VerifyOriginals(request)] = pending else {
            panic!("expected final verification: {pending:?}");
        };
        assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
        assert_eq!(state.phase(), RunPhase::Finalize);
    } else {
        assert_eq!(state.phase(), RunPhase::Mutants);
        let mut total = 0;
        for id in 0..2_u32 {
            let slot = &expected["workers"][usize::try_from(id).unwrap()];
            let mut matching = pending.iter().filter(|effect| worker(effect) == Some(id));
            if slot["stage"]["tag"] != "Idle" {
                check_worker(
                    slot,
                    matching.next().expect("pending worker request"),
                    records,
                );
                total += 1;
            }
            assert!(matching.next().is_none());
        }
        assert_eq!(pending.len(), total);
    }
    assert_eq!(
        u64::try_from(counts.admitted).unwrap(),
        quint::number(&expected["admitted"])
    );
    assert_eq!(
        u64::try_from(counts.started).unwrap(),
        quint::number(&expected["started"])
    );
    let summary = state.summary();
    let killed = quint::number(&expected["killed"]);
    let not_run = quint::number(&expected["notRun"]);
    assert_eq!(
        [
            summary.killed,
            summary.survived,
            summary.not_run,
            summary.inconclusive
        ],
        [killed, 0, not_run, not_run]
    );
    assert_eq!(
        state.exit_code(),
        if expected["incomplete"] == true { 4 } else { 0 }
    );
}

fn callback(command: &str, effect: &RunEffect, supplied: u64) -> RunEvent {
    match (command, effect) {
        ("Load" | "EndOfSpool", RunEffect::ReadCandidate(request)) => {
            RunEvent::CandidateLoaded(CandidateLoaded {
                id: request.id,
                worker: request.worker,
                candidate: (command == "Load").then(|| fixture_candidate(supplied)),
            })
        }
        ("Advance", RunEffect::ApplyMutation(request)) => {
            RunEvent::MutationApplied(MutationApplied {
                id: request.id,
                worker: request.worker,
            })
        }
        ("Advance", RunEffect::EmitOutput(output)) => {
            RunEvent::OutputEmitted(OutputEmitted { id: output.id })
        }
        ("Advance", RunEffect::RunMutant(request)) => {
            let mut finished = process_finished(request.id, ProcessTermination::Exit(1));
            finished.worker = Some(request.context.worker);
            RunEvent::MutantFinished(finished)
        }
        ("Advance", RunEffect::ResetWorker(request)) => RunEvent::WorkerReset(WorkerReset {
            id: request.id,
            worker: request.worker,
        }),
        _ => panic!("unexpected callback for {command}: {effect:?}"),
    }
}

fn replay(history: &[Value]) {
    let records = u64::try_from(
        history
            .iter()
            .filter(|row| row["dispatch"]["tag"] == "Load")
            .count(),
    )
    .unwrap();
    let (mut state, mut pending) = bootstrap(records);
    let mut counts = Counts {
        admitted: 0,
        started: 0,
    };
    check(&state, &pending, &counts, records, &history[0]["s"]);
    for row in &history[1..] {
        let expected = &row["s"];
        let dispatch = &row["dispatch"];
        let command = dispatch["tag"].as_str().unwrap();
        if command != "Wait" {
            let id = u32::try_from(quint::number(&dispatch["value"])).unwrap();
            let index = pending
                .iter()
                .position(|effect| worker(effect) == Some(id))
                .unwrap();
            let effect = pending.swap_remove(index);
            let (updated, emitted) = transition(
                state,
                callback(command, &effect, quint::number(&expected["supplied"])),
            )
            .unwrap();
            assert!(!updated.is_effect_pending(effect.id()));
            counts.admitted += emitted
                .iter()
                .filter(|effect| matches!(effect, RunEffect::ApplyMutation(_)))
                .count();
            counts.started += emitted
                .iter()
                .filter(|effect| matches!(effect, RunEffect::RunMutant(_)))
                .count();
            state = updated;
            pending.extend(emitted);
        }
        check(&state, &pending, &counts, records, expected);
    }
    assert_eq!(state.phase(), RunPhase::Finalize);
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/mutant_admission_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/mutant_admission_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/mutant_admission_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/mutant_admission_3.itf.json"))]
#[case::early_overflow(include_str!("../../../../formal/quint/traces/mutant_admission_earlyOverflowStep_0.itf.json"))]
#[case::exact(include_str!("../../../../formal/quint/traces/mutant_admission_exactStep_0.itf.json"))]
#[case::real_first(include_str!("../../../../formal/quint/traces/mutant_admission_realFirstStep_0.itf.json"))]
fn admission_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
