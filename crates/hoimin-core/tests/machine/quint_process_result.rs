//! Replay process evidence across session persistence and reporting.
use super::{
    complete_mutant_started, fixture_candidate, process_finished, quint,
    waiting_for_session_candidate,
};
use hoimin_core::{
    CandidateLoaded, CandidateReadTarget, EffectId, IntegrityCheckpoint, MachineError,
    MutationApplied, MutationOutcome, MutationStatus, OutputEmitted, OutputEvent, OutputSpoolRef,
    PersistResult, ProcessFinished, ProcessOutputState, ProcessTermination, ResourceMode,
    ResultPersisted, RunEffect, RunEvent, RunPhase, RunState, SessionDiagnostic,
    StoredResultLoaded, WorkerReset, transition,
};
use serde_json::Value;
use std::time::Duration;

fn one(effects: Vec<RunEffect>) -> RunEffect {
    let [effect] = <[_; 1]>::try_from(effects).expect("one result protocol effect");
    effect
}

fn bootstrap() -> (RunState, RunEffect) {
    let (state, effects) = waiting_for_session_candidate(false);
    let RunEffect::ReadCandidate(read) = one(effects) else {
        panic!("expected a session candidate read");
    };
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(CandidateLoaded {
            id: read.id,
            worker: read.worker,
            candidate: Some(fixture_candidate(1)),
        }),
    )
    .unwrap();
    let RunEffect::LookupStoredResult(lookup) = one(effects) else {
        panic!("expected lookup in a fresh session");
    };
    let (state, effects) = transition(
        state,
        RunEvent::StoredResultLoaded(StoredResultLoaded {
            id: lookup.id,
            worker: lookup.worker,
            result: None,
        }),
    )
    .unwrap();
    let RunEffect::ApplyMutation(apply) = one(effects) else {
        panic!("expected real apply");
    };
    let (state, effects) = transition(
        state,
        RunEvent::MutationApplied(MutationApplied {
            id: apply.id,
            worker: apply.worker,
        }),
    )
    .unwrap();
    let (state, effects) = complete_mutant_started(state, &effects);
    (state, one(effects))
}

fn completion(id: EffectId, evidence: &Value) -> ProcessFinished {
    let termination = match evidence["termination"]["tag"].as_str().unwrap() {
        "ExitZero" => ProcessTermination::Exit(0),
        "ExitOne" => ProcessTermination::Exit(1),
        "TimedOut" => ProcessTermination::Timeout,
        "MemoryExhausted" => ProcessTermination::OutOfMemory,
        other => panic!("unknown process termination {other}"),
    };
    let mut finished = process_finished(id, termination);
    finished.output_state = if evidence["closeTimedOut"] == true {
        ProcessOutputState::CloseTimedOut
    } else {
        ProcessOutputState::Complete
    };
    finished.output = OutputSpoolRef {
        token: "fixture-captured-output".to_owned(),
        retained: 9,
        observed: 17,
    };
    finished.elapsed = Duration::from_millis(123);
    finished.resource_mode = ResourceMode::BestEffort;
    finished
}

fn check_evidence(
    request: EffectId,
    outcome: &MutationOutcome,
    diagnostics: &[SessionDiagnostic],
    resource_mode: ResourceMode,
    expected: &Value,
) {
    let captured = completion(request, &expected["evidence"]);
    assert_eq!(
        *outcome,
        MutationOutcome::Executed {
            termination: captured.termination,
            output_state: captured.output_state,
            output: captured.output,
            elapsed: captured.elapsed,
        },
    );
    assert_eq!(resource_mode, captured.resource_mode);
    let status = match expected["status"]["tag"].as_str().unwrap() {
        "Survived" => MutationStatus::Survived,
        "Killed" => MutationStatus::Killed,
        "Timeout" => MutationStatus::Timeout,
        "OutOfMemory" => MutationStatus::OutOfMemory,
        "Error" => MutationStatus::Error,
        other => panic!("unknown process status {other}"),
    };
    assert_eq!(outcome.status(), status);
    if captured.output_state == ProcessOutputState::CloseTimedOut {
        let [diagnostic] = diagnostics else {
            panic!("expected a capture diagnostic: {diagnostics:?}");
        };
        assert_eq!(diagnostic.code, "process.output.close.timeout");
        assert_eq!(diagnostic.level, "error");
        assert_eq!(diagnostic.mutant_id, "m1");
        assert_eq!(
            diagnostic.message,
            "process termination was classified, but output collection timed out; captured output may be incomplete"
        );
    } else {
        assert_eq!(diagnostics, []);
    }
}

fn check(
    state: &RunState,
    pending: &RunEffect,
    persistence: Option<&PersistResult>,
    expected: &Value,
) {
    assert_eq!(state.pending_count(), 1);
    assert!(state.is_effect_pending(pending.id()));
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let machine_phase = match (phase, pending) {
        ("Running", RunEffect::RunMutant(request)) => {
            assert_eq!(request.context.mutant_id, "m1");
            assert_eq!(request.context.worker, 0);
            RunPhase::Mutants
        }
        ("Persisting", RunEffect::PersistResult(request)) => {
            assert_eq!(request.worker, 0);
            assert_eq!(request.result.run_id, state.run_id());
            assert_eq!(request.result.candidate, fixture_candidate(1));
            check_evidence(
                request.id,
                &request.result.outcome,
                &request.result.diagnostics,
                request.result.resource_mode,
                expected,
            );
            RunPhase::Mutants
        }
        ("Finishing", RunEffect::EmitOutput(output)) => {
            let OutputEvent::MutantFinished(finished) = &output.event else {
                panic!("expected completed process report: {output:?}");
            };
            assert_eq!(finished.run_id, state.run_id());
            assert_eq!(finished.candidate, fixture_candidate(1));
            check_evidence(
                output.id,
                &finished.outcome,
                &finished.diagnostics,
                finished.resource_mode,
                expected,
            );
            RunPhase::Mutants
        }
        ("Reset", RunEffect::ResetWorker(request)) => {
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("Reading", RunEffect::ReadCandidate(request)) => {
            assert_eq!(request.target, CandidateReadTarget::Next);
            assert_eq!(request.worker, 0);
            RunPhase::Mutants
        }
        ("Verifying", RunEffect::VerifyOriginals(request)) => {
            assert_eq!(request.checkpoint, IntegrityCheckpoint::PreFinalReport);
            RunPhase::Finalize
        }
        _ => panic!("unexpected {phase} effect {pending:?}"),
    };
    let slot = &expected["persistence"]["tag"];
    assert_eq!(persistence.is_none(), *slot == "Absent");
    if let Some(request) = persistence {
        assert_eq!(state.is_effect_pending(request.id), *slot == "Pending");
        assert_eq!(state.is_effect_retired(request.id), *slot == "Retired");
    }
    assert_eq!(state.phase(), machine_phase);
    check_summary(state, expected);
}

fn check_summary(state: &RunState, expected: &Value) {
    let summary = state.summary();
    let counted = quint::number(&expected["counted"]);
    let [killed, survived, timed_out, out_of_memory, error] =
        ["Killed", "Survived", "Timeout", "OutOfMemory", "Error"].map(|wanted| {
            if expected["status"]["tag"] == wanted {
                counted
            } else {
                0
            }
        });
    assert_eq!(
        [
            summary.killed,
            summary.survived,
            summary.timeout,
            summary.out_of_memory,
            summary.error,
            summary.not_run,
            summary.process_limit,
            summary.inconclusive
        ],
        [
            killed,
            survived,
            timed_out,
            out_of_memory,
            error,
            0,
            0,
            timed_out + out_of_memory + error
        ]
    );
    assert_eq!(
        u64::try_from(state.exit_code()).unwrap(),
        quint::number(&expected["exit"])
    );
}

fn event(command: &Value, pending: &RunEffect, persistence: Option<&PersistResult>) -> RunEvent {
    let value = &command["value"];
    match (command["tag"].as_str().unwrap(), pending) {
        ("Finish", RunEffect::RunMutant(request)) => {
            RunEvent::MutantFinished(completion(request.id, value))
        }
        ("Persist", _) => {
            let request = persistence.expect("issued persistence request");
            let identity = value["tag"].as_str().unwrap();
            RunEvent::ResultPersisted(ResultPersisted {
                id: request.id,
                worker: request.worker,
                run_id: if identity == "WrongRunId" {
                    "another-run".to_owned()
                } else {
                    request.result.run_id.clone()
                },
                mutant_id: if identity == "WrongMutantId" {
                    "m2".to_owned()
                } else {
                    request.result.candidate.id.clone()
                },
            })
        }
        ("Cancel", _) => RunEvent::CancellationRequested,
        ("Ack", RunEffect::EmitOutput(output)) => {
            RunEvent::OutputEmitted(OutputEmitted { id: output.id })
        }
        ("Ack", RunEffect::ResetWorker(request)) => RunEvent::WorkerReset(WorkerReset {
            id: request.id,
            worker: request.worker,
        }),
        ("EndOfSpool", RunEffect::ReadCandidate(request)) => {
            RunEvent::CandidateLoaded(CandidateLoaded {
                id: request.id,
                worker: request.worker,
                candidate: None,
            })
        }
        _ => panic!("unexpected {command:?} for {pending:?}"),
    }
}

fn rejection(verdict: &str, request: &PersistResult) -> MachineError {
    match verdict {
        "Duplicate" => MachineError::DuplicateEffect(request.id),
        "Late" => MachineError::RetiredEffect(request.id),
        "RunMismatch" => MachineError::SessionIdentityMismatch {
            worker: request.worker,
            field: "run_id",
            expected: request.result.run_id.clone(),
            received: "another-run".into(),
        },
        "MutantMismatch" => MachineError::SessionIdentityMismatch {
            worker: request.worker,
            field: "mutant_id",
            expected: request.result.candidate.id.clone(),
            received: "m2".into(),
        },
        other => panic!("unexpected persistence verdict {other}"),
    }
}

fn replay(history: &[Value]) {
    let (mut state, mut pending) = bootstrap();
    let mut persistence = None;
    check(&state, &pending, persistence.as_ref(), &history[0]["s"]);
    for row in &history[1..] {
        let command = &row["dispatch"];
        let expected = &row["s"];
        if command["tag"] != "Idle" {
            let input = event(command, &pending, persistence.as_ref());
            let verdict = expected["result"]["tag"].as_str().unwrap();
            if verdict == "Accepted" {
                let (updated, effects) = transition(state, input).unwrap();
                pending = one(effects);
                if let RunEffect::PersistResult(request) = &pending {
                    assert!(persistence.replace(request.clone()).is_none());
                }
                state = updated;
            } else {
                assert_eq!(
                    transition(state.clone(), input).unwrap_err(),
                    rejection(verdict, persistence.as_ref().expect("issued persistence")),
                );
            }
        }
        check(&state, &pending, persistence.as_ref(), expected);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/process_result_0.itf.json"))]
#[case::random_1(include_str!("../../../../formal/quint/traces/process_result_1.itf.json"))]
#[case::random_2(include_str!("../../../../formal/quint/traces/process_result_2.itf.json"))]
#[case::random_3(include_str!("../../../../formal/quint/traces/process_result_3.itf.json"))]
#[case::output_timeout(include_str!("../../../../formal/quint/traces/process_result_outputTimeoutStep_0.itf.json"))]
#[case::process_timeout(include_str!("../../../../formal/quint/traces/process_result_processTimeoutStep_0.itf.json"))]
#[case::memory(include_str!("../../../../formal/quint/traces/process_result_memoryStep_0.itf.json"))]
#[case::identities(include_str!("../../../../formal/quint/traces/process_result_identitiesStep_0.itf.json"))]
#[case::cancel_persist(include_str!("../../../../formal/quint/traces/process_result_cancelPersistStep_0.itf.json"))]
#[case::cancel_finish(include_str!("../../../../formal/quint/traces/process_result_cancelFinishStep_0.itf.json"))]
fn result_history_matches_public_transition(#[case] trace: &str) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
}
