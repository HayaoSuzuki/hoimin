use super::*;
use crate::{
    CommandArg, RawRunConfig, RawRunLimits, ResourceControl, ResourceMode, StartRequested,
};

fn state() -> RunState {
    let config = RunConfig::try_from(RawRunConfig {
        root: ".".into(),
        sources: vec!["src".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        limits: RawRunLimits {
            baseline_timeout: std::time::Duration::from_secs(1),
            ..RawRunLimits::default()
        },
        ..RawRunConfig::default()
    })
    .unwrap()
    .with_environment(crate::EnvironmentSnapshot::Untracked)
    .unwrap();
    RunState::new(
        "coverage",
        config,
        ResourceControl {
            mode: ResourceMode::Hard,
            mechanism: "test".into(),
        },
    )
}

#[test]
fn public_error_codes_remain_specific_to_each_failure() {
    let errors = [
        (
            MachineError::EffectNotPending(EffectId(1)),
            "machine.effect.not_pending",
        ),
        (
            MachineError::WrongPhase {
                phase: RunPhase::Validate,
            },
            "machine.phase.invalid",
        ),
        (MachineError::EffectIdOverflow, "machine.effect_id.overflow"),
        (MachineError::Budget("test".into()), "machine.budget"),
        (
            MachineError::MissingFingerprint,
            "machine.fingerprint.missing",
        ),
        (
            MachineError::MissingCandidateSpool,
            "machine.candidate_spool.missing",
        ),
        (
            MachineError::SelectedCandidateMissing("m1".into()),
            "machine.candidate.missing",
        ),
        (
            MachineError::WorkerCountOverflow,
            "machine.worker.count_overflow",
        ),
        (
            MachineError::ProcessCountOverflow,
            "machine.process.count_overflow",
        ),
        (
            MachineError::WorkerCountMismatch {
                configured: 1,
                preflight: 2,
            },
            "machine.worker.count_mismatch",
        ),
        (MachineError::UnknownWorker(1), "machine.worker.unknown"),
        (
            MachineError::ProcessWorkerMismatch {
                expected: Some(0),
                received: Some(1),
            },
            "machine.worker.process_mismatch",
        ),
    ];
    for (error, expected) in errors {
        assert_eq!(error.code(), expected);
    }
}

#[test]
fn allocated_but_unregistered_completion_is_rejected() {
    let mut state = state();
    let id = state.allocate_id().unwrap();
    let error = state.accept_completion(&RunEvent::OutputEmitted(crate::OutputEmitted { id }));
    assert!(matches!(error, Err(MachineError::EffectNotPending(found)) if found == id));
}

#[test]
fn duplicate_registration_keeps_original_pending_effect() {
    let (mut state, effects) =
        transition(state(), RunEvent::StartRequested(StartRequested)).unwrap();
    let original = state.pending.clone();
    assert!(matches!(
        state.register(&effects),
        Err(MachineError::DuplicateEffect(_))
    ));
    assert_eq!(state.pending, original);
}

#[test]
fn idle_and_resetting_workers_cannot_supply_candidates() {
    let mut state = state();
    for phase in [WorkerState::Idle, WorkerState::Resetting] {
        state.workers.insert(0, phase);
        assert!(matches!(
            state.candidate(0),
            Err(MachineError::WrongPhase { .. })
        ));
        assert!(matches!(
            state.take_candidate(0),
            Err(MachineError::WrongPhase { .. })
        ));
        assert!(state.workers[&0].candidate().is_none());
    }
}

#[test]
fn stopping_after_report_start_without_copy_goes_directly_to_cleanup() {
    let mut state = state();
    state.flags.report.report_started = true;
    let effects = state.begin_stop().unwrap();
    assert!(matches!(effects.as_slice(), [RunEffect::Cleanup(_)]));
    assert!(state.flags.scheduling.stop_requested);
}

fn pending_state(phase: RunPhase, kind: CompletionKind) -> (RunState, EffectId) {
    let mut value = state();
    value.phase = phase;
    let id = value.allocate_id().unwrap();
    value.pending.insert(
        id,
        PendingEffect {
            completion: PendingCompletion::Effect(kind),
            worker: Some(0),
            cleanup_reservations: None,
        },
    );
    (value, id)
}

#[test]
fn completion_cannot_be_transferred_to_another_worker() {
    let id = EffectId(1);
    let cases = [
        (
            RunPhase::Copy,
            CompletionKind::WorkerCreated,
            RunEvent::WorkerCreated(crate::WorkerCreated {
                id,
                worker: 1,
                reservation_id: ReservationId(1),
            }),
        ),
        (
            RunPhase::Mutants,
            CompletionKind::CandidateLoaded,
            RunEvent::CandidateLoaded(crate::CandidateLoaded {
                id,
                worker: 1,
                candidate: None,
            }),
        ),
        (
            RunPhase::Mutants,
            CompletionKind::StoredResultLoaded,
            RunEvent::StoredResultLoaded(crate::StoredResultLoaded {
                id,
                worker: 1,
                result: None,
            }),
        ),
        (
            RunPhase::Mutants,
            CompletionKind::MutationApplied,
            RunEvent::MutationApplied(crate::MutationApplied { id, worker: 1 }),
        ),
        (
            RunPhase::Mutants,
            CompletionKind::ResultPersisted,
            RunEvent::ResultPersisted(crate::ResultPersisted {
                id,
                worker: 1,
                run_id: "coverage".into(),
                mutant_id: "m1".into(),
            }),
        ),
    ];
    for (phase, kind, event) in cases {
        let (state, _) = pending_state(phase, kind);
        assert!(matches!(
            transition(state, event),
            Err(MachineError::UnknownWorker(1))
        ));
    }
}

fn process_finished(id: EffectId, worker: Option<u32>) -> ProcessFinished {
    ProcessFinished {
        id,
        worker,
        termination: ProcessTermination::Exit(0),
        output_state: ProcessOutputState::Complete,
        output: crate::OutputSpoolRef {
            token: "output".into(),
            retained: 0,
            observed: 0,
        },
        elapsed: std::time::Duration::ZERO,
        resource_mode: ResourceMode::Hard,
    }
}

#[test]
fn process_completion_worker_must_match_its_request() {
    for (phase, kind) in [
        (RunPhase::Baseline, CompletionKind::BaselineFinished),
        (RunPhase::Mutants, CompletionKind::MutantFinished),
    ] {
        let (state, id) = pending_state(phase, kind);
        let value = process_finished(id, Some(1));
        let event = if phase == RunPhase::Baseline {
            RunEvent::BaselineFinished(value)
        } else {
            RunEvent::MutantFinished(value)
        };
        assert!(matches!(
            transition(state, event),
            Err(MachineError::ProcessWorkerMismatch {
                expected: Some(0),
                received: Some(1)
            })
        ));
    }
}

fn candidate() -> MutationCandidate {
    MutationCandidate {
        id: "m1".into(),
        sequence: 1,
        path: "src/calc.py".into(),
        span: crate::ByteSpan {
            start: 1,
            length: 1,
        },
        original: "+".into(),
        replacement: "-".into(),
        operator: "binary_add_sub".into(),
        line: 1,
        column: 1,
        symbol: None,
        file_hash: "hash".into(),
    }
}

#[test]
fn persistence_and_output_keep_candidate_ownership_until_acknowledgement() {
    for finishing in [false, true] {
        let mut state = state();
        let result = MutantResult {
            run_id: "coverage".into(),
            candidate: candidate(),
            outcome: MutationOutcome::NotRun,
            resource_mode: ResourceMode::Hard,
            diagnostics: Vec::new(),
        };
        state.workers.insert(
            0,
            if finishing {
                WorkerState::Finishing(result)
            } else {
                WorkerState::Persisting(result)
            },
        );
        assert_eq!(state.candidate(0).unwrap().id, "m1");
        assert_eq!(state.take_candidate(0).unwrap().id, "m1");
        assert!(matches!(state.workers[&0], WorkerState::Idle));
    }
}

#[test]
fn preflight_grant_must_match_configured_worker_count() {
    let mut state = state();
    let preflight = crate::PreflightCompleted {
        id: EffectId(1),
        requested_workers: 2,
        per_worker_logical_bytes: 1,
        aggregate_logical_bytes: 2,
        fingerprint: None,
    };
    state.copy_grant = Some(reserve_workspace_copy(&mut state.budgets, &preflight).unwrap());
    assert!(matches!(
        state.create_worker_effects(),
        Err(MachineError::WorkerCountMismatch {
            configured: 1,
            preflight: 2
        })
    ));
}

#[test]
fn selected_execution_requires_a_candidate_spool() {
    for truncated in [false, true] {
        let mut state = state();
        state.selection = RunSelection::explicit(
            ["m1".to_owned()],
            crate::VerificationSelection::candidate_ids(1, 1, false).unwrap(),
        )
        .unwrap();
        let result = state.apply_analysis_finished(AnalysisFinished {
            id: EffectId(1),
            spool: None,
            truncated,
            diagnostics: Vec::new(),
        });
        assert!(matches!(result, Err(MachineError::MissingCandidateSpool)));
    }
}

#[test]
fn persistence_acknowledgement_must_match_owned_result_identity() {
    for (run_id, mutant_id, expected_field) in [
        ("other", "m1", "run_id"),
        ("coverage", "other", "mutant_id"),
    ] {
        let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::ResultPersisted);
        state.workers.insert(
            0,
            WorkerState::Persisting(MutantResult {
                run_id: "coverage".into(),
                candidate: candidate(),
                outcome: MutationOutcome::NotRun,
                resource_mode: ResourceMode::Hard,
                diagnostics: Vec::new(),
            }),
        );
        let result = transition(
            state,
            RunEvent::ResultPersisted(crate::ResultPersisted {
                id,
                worker: 0,
                run_id: run_id.into(),
                mutant_id: mutant_id.into(),
            }),
        );
        assert!(matches!(result,
            Err(MachineError::SessionIdentityMismatch { field, .. }) if field == expected_field));
    }
}

#[test]
fn persistence_acknowledgement_rejects_a_worker_without_owned_result() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::ResultPersisted);
    state.workers.insert(0, WorkerState::Idle);
    let result = transition(
        state,
        RunEvent::ResultPersisted(crate::ResultPersisted {
            id,
            worker: 0,
            run_id: "coverage".into(),
            mutant_id: "m1".into(),
        }),
    );
    assert!(matches!(
        result,
        Err(MachineError::WrongPhase {
            phase: RunPhase::Mutants
        })
    ));
}

#[test]
fn synthetic_output_acknowledgements_require_the_owned_synthetic_phase() {
    for continuation in [
        OutputContinuation::StartSynthetic(0),
        OutputContinuation::FinishSynthetic(0),
    ] {
        let mut state = state();
        state.phase = RunPhase::Mutants;
        state.workers.insert(0, WorkerState::Idle);
        let id = state.allocate_id().unwrap();
        state.register_output(
            EmitOutput {
                id,
                event: OutputEvent::Diagnostic(Diagnostic::new(
                    "coverage", 1, "warning", "test", "test",
                )),
            },
            continuation,
        );
        assert!(matches!(
            transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
            Err(MachineError::WrongPhase {
                phase: RunPhase::Mutants
            })
        ));
    }
}

#[test]
fn ordered_collection_waits_while_a_candidate_is_owned() {
    let mut state = state();
    state.selection = RunSelection::ordered(
        vec!["m1".into()],
        crate::VerificationSelection::top(1, 1, crate::VerificationTopPolicy::Strict, false)
            .unwrap(),
    )
    .unwrap();
    state
        .workers
        .insert(0, WorkerState::Reading(CandidateReadTarget::Next));
    assert_eq!(state.schedule_read_or_finalize().unwrap(), []);
    assert!(matches!(
        state.workers[&0],
        WorkerState::Reading(CandidateReadTarget::Next)
    ));
}

#[test]
fn process_elapsed_milliseconds_saturate_without_overflow() {
    assert_eq!(elapsed_millis(std::time::Duration::MAX), u64::MAX);
    assert_eq!(elapsed_millis(std::time::Duration::from_nanos(1)), 0);
}

#[test]
fn output_effect_kind_is_an_output_acknowledgement() {
    let effect = RunEffect::EmitOutput(EmitOutput {
        id: EffectId(1),
        event: OutputEvent::Diagnostic(Diagnostic::new("coverage", 1, "warning", "test", "test")),
    });
    assert_eq!(expected_completion(&effect), CompletionKind::OutputEmitted);
}

#[cfg(feature = "contracts")]
fn assert_contract_panics(state: RunState, event: RunEvent, expected: &str) {
    let failure =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| transition(state, event)))
            .unwrap_err();
    let message = failure.downcast_ref::<String>().unwrap();
    assert!(message.contains(expected), "{message}");
}

#[cfg(feature = "contracts")]
#[test]
fn contract_rejects_worker_ids_outside_the_configured_pool() {
    let mut state = state();
    state.workers.insert(1, WorkerState::Idle);
    assert_contract_panics(
        state,
        RunEvent::StartRequested(StartRequested),
        "machine.worker.invariant",
    );
}

#[cfg(feature = "contracts")]
#[test]
fn contract_counts_baseline_processes_against_the_process_budget() {
    let mut state = state();
    for _ in 0..2 {
        let id = state.allocate_id().unwrap();
        state.pending.insert(
            id,
            PendingEffect {
                completion: PendingCompletion::Effect(CompletionKind::BaselineFinished),
                worker: Some(0),
                cleanup_reservations: None,
            },
        );
    }
    assert_contract_panics(
        state,
        RunEvent::StartRequested(StartRequested),
        "machine.budget.invariant",
    );
}

#[cfg(feature = "contracts")]
#[test]
fn contract_rejects_mutation_dispatch_from_a_fatal_state() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::StoredResultLoaded);
    state.flags.outcome.infrastructure_error = true;
    state.workers.insert(0, WorkerState::Lookup(candidate()));
    state.copy_grant = Some(
        reserve_workspace_copy(
            &mut state.budgets,
            &crate::PreflightCompleted {
                id: EffectId(0),
                requested_workers: 1,
                per_worker_logical_bytes: 1,
                aggregate_logical_bytes: 1,
                fingerprint: None,
            },
        )
        .unwrap(),
    );
    assert_contract_panics(
        state,
        RunEvent::StoredResultLoaded(crate::StoredResultLoaded {
            id,
            worker: 0,
            result: None,
        }),
        "machine.cancel.post",
    );
}

#[test]
fn effect_id_exhaustion_does_not_wrap_or_advance_the_allocator() {
    let mut state = state();
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        state.allocate_id(),
        Err(MachineError::EffectIdOverflow)
    ));
    assert_eq!(state.next_effect_id, u64::MAX);
}

#[test]
fn worker_creation_requires_a_preflight_grant() {
    let mut state = state();
    assert!(matches!(
        state.create_worker_effects(),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Validate
        })
    ));
}
