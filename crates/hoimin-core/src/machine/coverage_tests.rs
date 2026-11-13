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

#[rstest::rstest]
#[case(
    &MachineError::EffectNotPending(EffectId(1)),
    "machine.effect.not_pending"
)]
#[case(
            &MachineError::WrongPhase {
                phase: RunPhase::Validate,
            },
            "machine.phase.invalid",
        )]
#[case(&MachineError::EffectIdOverflow, "machine.effect_id.overflow")]
#[case(&MachineError::Budget("test".into()), "machine.budget")]
#[case(&MachineError::MissingFingerprint, "machine.fingerprint.missing")]
#[case(&MachineError::MissingCandidateSpool, "machine.candidate_spool.missing")]
#[case(
            &MachineError::SelectedCandidateMissing("m1".into()),
            "machine.candidate.missing",
        )]
#[case(&MachineError::WorkerCountOverflow, "machine.worker.count_overflow")]
#[case(&MachineError::ProcessCountOverflow, "machine.process.count_overflow")]
#[case(
            &MachineError::WorkerCountMismatch {
                configured: 1,
                preflight: 2,
            },
            "machine.worker.count_mismatch",
        )]
#[case(&MachineError::UnknownWorker(1), "machine.worker.unknown")]
#[case(
            &MachineError::ProcessWorkerMismatch {
                expected: Some(0),
                received: Some(1),
            },
            "machine.worker.process_mismatch",
        )]
fn public_error_codes_remain_specific_to_each_failure(
    #[case] error: &MachineError,
    #[case] expected: &str,
) {
    assert_eq!(error.code(), expected);
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

#[rstest::rstest]
#[case(WorkerState::Idle)]
#[case(WorkerState::Resetting)]
fn idle_and_resetting_workers_cannot_supply_candidates(#[case] phase: WorkerState) {
    let mut state = state();

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

#[rstest::rstest]
#[case(
            RunPhase::Copy,
            CompletionKind::WorkerCreated,
            RunEvent::WorkerCreated(crate::WorkerCreated {
                id: EffectId(1),
                worker: 1,
                reservation_id: ReservationId(1),
            }),
        )]
#[case(
            RunPhase::Mutants,
            CompletionKind::CandidateLoaded,
            RunEvent::CandidateLoaded(crate::CandidateLoaded {
                id: EffectId(1),
                worker: 1,
                candidate: None,
            }),
        )]
#[case(
            RunPhase::Mutants,
            CompletionKind::StoredResultLoaded,
            RunEvent::StoredResultLoaded(crate::StoredResultLoaded {
                id: EffectId(1),
                worker: 1,
                result: None,
            }),
        )]
#[case(
            RunPhase::Mutants,
            CompletionKind::MutationApplied,
            RunEvent::MutationApplied(crate::MutationApplied { id: EffectId(1), worker: 1 }),
        )]
#[case(
            RunPhase::Mutants,
            CompletionKind::ResultPersisted,
            RunEvent::ResultPersisted(crate::ResultPersisted {
                id: EffectId(1),
                worker: 1,
                run_id: "coverage".into(),
                mutant_id: "m1".into(),
            }),
        )]
fn completion_cannot_be_transferred_to_another_worker(
    #[case] phase: RunPhase,
    #[case] kind: CompletionKind,
    #[case] event: RunEvent,
) {
    let (state, _) = pending_state(phase, kind);
    assert!(matches!(
        transition(state, event),
        Err(MachineError::UnknownWorker(1))
    ));
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

#[rstest::rstest]
#[case(
    RunPhase::Baseline,
    CompletionKind::BaselineFinished,
    RunEvent::BaselineFinished
)]
#[case(
    RunPhase::Mutants,
    CompletionKind::MutantFinished,
    RunEvent::MutantFinished
)]
fn process_completion_worker_must_match_its_request(
    #[case] phase: RunPhase,
    #[case] kind: CompletionKind,
    #[case] wrap: fn(ProcessFinished) -> RunEvent,
) {
    let (state, id) = pending_state(phase, kind);
    assert!(matches!(
        transition(state, wrap(process_finished(id, Some(1)))),
        Err(MachineError::ProcessWorkerMismatch {
            expected: Some(0),
            received: Some(1)
        })
    ));
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

#[rstest::rstest]
#[case(WorkerState::Finishing)]
#[case(WorkerState::Persisting)]
fn persistence_and_output_keep_candidate_ownership_until_acknowledgement(
    #[case] own: fn(MutantResult) -> WorkerState,
) {
    let mut state = state();
    let result = MutantResult {
        run_id: "coverage".into(),
        candidate: candidate(),
        outcome: MutationOutcome::NotRun,
        resource_mode: ResourceMode::Hard,
        diagnostics: Vec::new(),
    };
    state.workers.insert(0, own(result));
    assert_eq!(state.candidate(0).unwrap().id, "m1");
    assert_eq!(state.take_candidate(0).unwrap().id, "m1");
    assert!(matches!(state.workers[&0], WorkerState::Idle));
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

#[rstest::rstest]
#[case(false)]
#[case(true)]
fn selected_execution_requires_a_candidate_spool(#[case] truncated: bool) {
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

#[rstest::rstest]
#[case("other", "m1", "run_id")]
#[case("coverage", "other", "mutant_id")]
fn persistence_acknowledgement_must_match_owned_result_identity(
    #[case] run_id: &str,
    #[case] mutant_id: &str,
    #[case] expected_field: &str,
) {
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

#[rstest::rstest]
#[case(OutputContinuation::StartSynthetic(0))]
#[case(OutputContinuation::FinishSynthetic(0))]
fn synthetic_output_acknowledgements_require_the_owned_synthetic_phase(
    #[case] continuation: OutputContinuation,
) {
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
    state.next_effect_id = 3;
    state.pending = [1, 2]
        .map(|id| {
            (
                EffectId(id),
                PendingEffect {
                    completion: PendingCompletion::Effect(CompletionKind::BaselineFinished),
                    worker: Some(0),
                    cleanup_reservations: None,
                },
            )
        })
        .into_iter()
        .collect();
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

#[rstest::rstest]
#[case(RunState::preflight_effects)]
#[case(RunState::materialization_verification_effects)]
#[case(RunState::baseline_effects)]
#[case(RunState::start_run_effects)]
#[case(RunState::finalize_effects)]
#[case(RunState::cleanup_effects)]
#[case(RunState::final_report_effects)]
fn effect_builders_propagate_identifier_exhaustion(
    #[case] build: fn(&mut RunState) -> Result<Vec<RunEffect>, MachineError>,
) {
    let mut state = state();
    state.next_effect_id = u64::MAX;
    assert_eq!(build(&mut state), Err(MachineError::EffectIdOverflow));
    assert_eq!(state.next_effect_id, u64::MAX);
}

#[rstest::rstest]
#[case(RunState::apply_worker_effects)]
#[case(RunState::started_output)]
#[case(RunState::mutant_process)]
fn candidate_effect_builders_propagate_identifier_exhaustion(
    #[case] build: fn(&mut RunState, u32) -> Result<Vec<RunEffect>, MachineError>,
) {
    let mut state = state();
    state.workers.insert(0, WorkerState::Ready(candidate()));
    state.next_effect_id = u64::MAX;
    assert_eq!(build(&mut state, 0), Err(MachineError::EffectIdOverflow));
    assert_eq!(state.next_effect_id, u64::MAX);
}

#[rstest::rstest]
#[case(RunState::apply_worker_effects)]
#[case(RunState::started_output)]
#[case(RunState::mutant_process)]
fn candidate_effect_builders_require_an_owned_candidate(
    #[case] build: fn(&mut RunState, u32) -> Result<Vec<RunEffect>, MachineError>,
) {
    let mut state = state();
    assert_eq!(
        build(&mut state, 1),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Validate
        })
    );
}

#[test]
fn analysis_diagnostic_requires_an_owned_analysis() {
    let mut state = state();
    assert_eq!(
        state.analysis_diagnostic_effects(),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Validate
        })
    );
}

#[rstest::rstest]
#[case(RunPhase::Preflight, CompletionKind::TargetsResolved, |id| RunEvent::TargetsResolved(crate::TargetsResolved { id, targets: vec![] }))]
#[case(RunPhase::Preflight, CompletionKind::PreflightCompleted, |id| RunEvent::PreflightCompleted(crate::PreflightCompleted { id, requested_workers: 1, per_worker_logical_bytes: 1, aggregate_logical_bytes: 1, fingerprint: None }))]
#[case(RunPhase::Preflight, CompletionKind::SessionStarted, |id| RunEvent::SessionStarted(crate::SessionStarted { id, run_id: "session".into() }))]
#[case(RunPhase::Copy, CompletionKind::WorkerCreated, |id| RunEvent::WorkerCreated(crate::WorkerCreated { id, worker: 0, reservation_id: crate::ReservationId(0) }))]
#[case(RunPhase::MaterializationVerification, CompletionKind::OriginalsVerified, |id| RunEvent::OriginalsVerified(crate::OriginalsVerified { id, checkpoint: IntegrityCheckpoint::PostMaterialization }))]
#[case(RunPhase::Baseline, CompletionKind::BaselineFinished, |id| RunEvent::BaselineFinished(process_finished(id, Some(0))))]
#[case(RunPhase::Analyze, CompletionKind::AnalysisFinished, |id| RunEvent::AnalysisFinished(AnalysisFinished { id, spool: None, truncated: false, diagnostics: vec![] }))]
#[case(RunPhase::Analyze, CompletionKind::AnalysisFinished, |id| RunEvent::AnalysisFinished(AnalysisFinished { id, spool: None, truncated: false, diagnostics: vec![crate::AnalysisDiagnostic { code: "warning".into(), message: "warning".into() }] }))]
#[case(RunPhase::Mutants, CompletionKind::MutationApplied, |id| RunEvent::MutationApplied(crate::MutationApplied { id, worker: 0 }))]
#[case(RunPhase::Mutants, CompletionKind::MutantFinished, |id| RunEvent::MutantFinished(process_finished(id, Some(0))))]
#[case(RunPhase::Mutants, CompletionKind::WorkerReset, |id| RunEvent::WorkerReset(crate::WorkerReset { id, worker: 0 }))]
#[case(RunPhase::BaselineRestoration, CompletionKind::WorkerReset, |id| RunEvent::WorkerReset(crate::WorkerReset { id, worker: 0 }))]
#[case(RunPhase::Finalize, CompletionKind::OriginalsVerified, |id| RunEvent::OriginalsVerified(crate::OriginalsVerified { id, checkpoint: IntegrityCheckpoint::PreFinalReport }))]
#[case(RunPhase::Preflight, CompletionKind::SessionLoaded, |id| RunEvent::SessionLoaded(crate::SessionLoaded { id, outcome: crate::SessionLoadOutcome::Resumed(crate::SessionResumeRef { run_id: "session".into() }) }))]
#[case(RunPhase::Preflight, CompletionKind::SessionLoaded, |id| RunEvent::SessionLoaded(crate::SessionLoaded { id, outcome: crate::SessionLoadOutcome::Fresh(crate::ResumeFreshReason::NoPriorRun) }))]
fn completion_propagates_exhaustion_from_its_next_effect(
    #[case] phase: RunPhase,
    #[case] kind: CompletionKind,
    #[case] event: fn(EffectId) -> RunEvent,
) {
    let (mut state, id) = pending_state(phase, kind);
    state.fingerprint = Some(crate::RunFingerprint::from_bytes([0; 32]));
    state.workers.insert(0, WorkerState::Ready(candidate()));
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, event(id)),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn start_request_propagates_identifier_exhaustion() {
    let mut state = state();
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, RunEvent::StartRequested(StartRequested)),
        Err(MachineError::EffectIdOverflow)
    ));
}

fn exhausted_effect_fixture() -> RunState {
    let mut state = state();
    state.identity = RunIdentity::Session("coverage".into());
    state.fingerprint = Some(crate::RunFingerprint::from_bytes([0; 32]));
    state.copy_grant = Some(
        reserve_workspace_copy(
            &mut state.budgets,
            &crate::PreflightCompleted {
                id: EffectId(1),
                requested_workers: 1,
                per_worker_logical_bytes: 1,
                aggregate_logical_bytes: 1,
                fingerprint: None,
            },
        )
        .unwrap(),
    );
    state.targets.push_back(crate::TargetSlice {
        path: "calc.py".into(),
        lines: vec![],
        symbols: vec![],
    });
    state
        .analysis_diagnostics
        .push_back(crate::AnalysisDiagnostic {
            code: "warning".into(),
            message: "warning".into(),
        });
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    state.workers.insert(0, WorkerState::Ready(candidate()));
    state.next_effect_id = u64::MAX;
    state
}

fn result_fixture() -> MutantResult {
    MutantResult {
        run_id: "coverage".into(),
        candidate: candidate(),
        outcome: MutationOutcome::NotRun,
        resource_mode: ResourceMode::Hard,
        diagnostics: vec![],
    }
}

#[rstest::rstest]
#[case(|state: &mut RunState| state.analyze_next())]
#[case(|state: &mut RunState| state.analysis_diagnostic_effects())]
#[case(|state: &mut RunState| state.begin_session_effects())]
#[case(|state: &mut RunState| state.create_worker_effects())]
#[case(|state: &mut RunState| state.candidate_effects(0))]
#[case(|state: &mut RunState| state.read_candidate(0, CandidateReadTarget::Next))]
#[case(|state: &mut RunState| state.synthetic_started_output(0, SyntheticOutcome::NotRun))]
#[case(|state: &mut RunState| state.synthetic_finished_output(0, SyntheticOutcome::NotRun))]
#[case(|state: &mut RunState| state.finished_output(0, result_fixture()))]
#[case(|state: &mut RunState| state.post_report_effects())]
#[case(|state: &mut RunState| state.diagnostic_effect(&EffectFailed::other(EffectId(1), "test", "failure")))]
fn owned_effect_builders_propagate_identifier_exhaustion(
    #[case] build: fn(&mut RunState) -> Result<Vec<RunEffect>, MachineError>,
) {
    let mut state = exhausted_effect_fixture();
    assert_eq!(build(&mut state), Err(MachineError::EffectIdOverflow));
}

#[rstest::rstest]
#[case(RunPhase::Mutants, CompletionKind::CandidateLoaded, |id| RunEvent::CandidateLoaded(crate::CandidateLoaded { id, worker: 0, candidate: None }))]
#[case(RunPhase::Mutants, CompletionKind::WorkerReset, |id| RunEvent::WorkerReset(crate::WorkerReset { id, worker: 0 }))]
#[case(RunPhase::BaselineRestoration, CompletionKind::WorkerReset, |id| RunEvent::WorkerReset(crate::WorkerReset { id, worker: 0 }))]
fn worker_completion_rejects_missing_worker_state(
    #[case] phase: RunPhase,
    #[case] kind: CompletionKind,
    #[case] event: fn(EffectId) -> RunEvent,
) {
    let (state, id) = pending_state(phase, kind);
    assert!(matches!(
        transition(state, event(id)),
        Err(MachineError::UnknownWorker(0))
    ));
}

#[rstest::rstest]
#[case(OutputContinuation::StartSynthetic(0))]
#[case(OutputContinuation::FinishSynthetic(0))]
#[case(OutputContinuation::FinishMutant(0))]
fn output_acknowledgement_rejects_missing_worker_state(#[case] continuation: OutputContinuation) {
    let mut state = state();
    state.phase = RunPhase::Mutants;
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
        Err(MachineError::UnknownWorker(0))
    ));
}

#[rstest::rstest]
#[case(|state: &mut RunState| state.apply_worker_effects(0))]
#[case(|state: &mut RunState| state.candidate_effects(0))]
#[case(|state: &mut RunState| state.started_output(0))]
#[case(|state: &mut RunState| state.synthetic_started_output(0, SyntheticOutcome::NotRun))]
#[case(|state: &mut RunState| state.mutant_process(0))]
#[case(|state: &mut RunState| state.synthetic_finished_output(0, SyntheticOutcome::NotRun))]
fn ownership_transfer_still_rejects_a_missing_candidate_before_mutating_state(
    #[case] build: fn(&mut RunState) -> Result<Vec<RunEffect>, MachineError>,
) {
    let mut state = state();
    state.identity = RunIdentity::Session("coverage".into());
    assert_eq!(
        build(&mut state),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Validate
        })
    );
    assert!(state.workers.is_empty());
    assert_eq!(state.next_effect_id, 1);
}

#[rstest::rstest]
#[case(OutputContinuation::RunStarted)]
#[case(OutputContinuation::RunFinished)]
#[case(OutputContinuation::Diagnostic)]
#[case(OutputContinuation::ContinueAfterBudgetWarning)]
#[case(OutputContinuation::ContinueAfterAnalysisDiagnostic)]
#[case(OutputContinuation::StartMutant(0))]
#[case(OutputContinuation::FinishMutant(0))]
fn output_acknowledgement_propagates_next_effect_identifier_exhaustion(
    #[case] continuation: OutputContinuation,
) {
    let mut state = exhausted_effect_fixture();
    state.phase = RunPhase::Mutants;
    state.next_effect_id = 1;
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
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(ProcessTermination::Exit(0))]
#[case(ProcessTermination::Exit(1))]
fn baseline_completion_propagates_exhaustion_after_emitting_baseline_output(
    #[case] termination: ProcessTermination,
) {
    let (mut state, id) = pending_state(RunPhase::Baseline, CompletionKind::BaselineFinished);
    state.workers.insert(0, WorkerState::Idle);
    state.next_effect_id = u64::MAX - 1;
    let mut finished = process_finished(id, Some(0));
    finished.termination = termination;
    assert!(matches!(
        transition(state, RunEvent::BaselineFinished(finished)),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(|state: &mut RunState| state.apply_worker_effects(0))]
#[case(|state: &mut RunState| state.candidate_effects(0))]
#[case(|state: &mut RunState| state.started_output(0))]
#[case(|state: &mut RunState| state.synthetic_started_output(0, SyntheticOutcome::NotRun))]
#[case(|state: &mut RunState| state.mutant_process(0))]
#[case(|state: &mut RunState| state.synthetic_finished_output(0, SyntheticOutcome::NotRun))]
fn ownership_transfer_keeps_the_candidate_when_identifier_allocation_fails(
    #[case] build: fn(&mut RunState) -> Result<Vec<RunEffect>, MachineError>,
) {
    let mut state = exhausted_effect_fixture();
    assert_eq!(build(&mut state), Err(MachineError::EffectIdOverflow));
    assert_eq!(state.candidate(0).unwrap(), &candidate());
    assert_eq!(state.next_effect_id, u64::MAX);
}

#[test]
fn failed_report_delivery_finishes_without_requesting_another_output() {
    let mut state = state();
    state.flags.report.delivery = ReportDelivery::Failed;
    assert_eq!(state.final_report_effects().unwrap(), []);
    assert_eq!(state.phase, RunPhase::Finished);
}

#[test]
fn candidate_read_requires_an_owned_spool() {
    let mut state = state();
    assert_eq!(
        state.read_candidate(0, CandidateReadTarget::Next),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Validate
        })
    );
}

#[test]
fn candidate_read_rejects_unknown_worker_before_allocating_an_effect() {
    let mut state = state();
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    assert_eq!(
        state.read_candidate(0, CandidateReadTarget::Next),
        Err(MachineError::UnknownWorker(0))
    );
    assert_eq!(state.next_effect_id, 1);
}

#[test]
fn finished_output_requires_a_live_worker_entry() {
    let mut state = state();
    assert_eq!(
        state.finished_output(0, result_fixture()),
        Err(MachineError::UnknownWorker(0))
    );
}

#[test]
fn stopped_candidate_drain_requires_an_owned_worker() {
    let mut state = state();
    assert_eq!(
        state.next_stopped_candidate_effects(),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Validate
        })
    );
}

#[test]
fn transition_propagates_an_effect_identifier_collision() {
    let mut state = state();
    state.pending.insert(
        EffectId(1),
        PendingEffect {
            completion: PendingCompletion::Effect(CompletionKind::TargetsResolved),
            worker: None,
            cleanup_reservations: None,
        },
    );
    assert!(matches!(
        transition(state, RunEvent::StartRequested(StartRequested)),
        Err(MachineError::DuplicateEffect(EffectId(1)))
    ));
}

#[test]
fn output_acknowledgement_requires_output_continuation_metadata() {
    let (state, id) = pending_state(RunPhase::Finalize, CompletionKind::OutputEmitted);
    assert!(matches!(
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Finalize
        })
    ));
}

#[test]
fn mutant_completion_requires_a_worker_even_when_the_pending_metadata_omits_one() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::MutantFinished);
    state.pending.get_mut(&id).unwrap().worker = None;
    assert!(matches!(
        transition(state, RunEvent::MutantFinished(process_finished(id, None))),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Mutants
        })
    ));
}

#[test]
fn budget_observation_requires_verification_selection_metadata() {
    let (state, id) = pending_state(
        RunPhase::BudgetCheck,
        CompletionKind::RemainingBudgetObserved,
    );
    assert!(matches!(
        transition(
            state,
            RunEvent::RemainingBudgetObserved(crate::RemainingBudgetObserved {
                id,
                remaining: std::time::Duration::ZERO
            })
        ),
        Err(MachineError::WrongPhase {
            phase: RunPhase::BudgetCheck
        })
    ));
}

#[rstest::rstest]
#[case(crate::SessionMode::Fresh(crate::SessionConfig { path: "session.db".into() }))]
#[case(crate::SessionMode::Resume(crate::SessionConfig { path: "session.db".into() }))]
fn preflight_session_dispatch_propagates_identifier_exhaustion(#[case] mode: crate::SessionMode) {
    let (mut state, id) = pending_state(RunPhase::Preflight, CompletionKind::PreflightCompleted);
    let mut data = state.config.clone().into_data();
    data.session = mode;
    state.config = RunConfig::try_from(data).unwrap();
    state.next_effect_id = u64::MAX;
    let event = RunEvent::PreflightCompleted(crate::PreflightCompleted {
        id,
        requested_workers: 1,
        per_worker_logical_bytes: 1,
        aggregate_logical_bytes: 1,
        fingerprint: Some(crate::RunFingerprint::from_bytes([0; 32])),
    });
    assert!(matches!(
        transition(state, event),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn preflight_rejects_inconsistent_aggregate_copy_evidence() {
    let (state, id) = pending_state(RunPhase::Preflight, CompletionKind::PreflightCompleted);
    let event = RunEvent::PreflightCompleted(crate::PreflightCompleted {
        id,
        requested_workers: 1,
        per_worker_logical_bytes: 1,
        aggregate_logical_bytes: 2,
        fingerprint: None,
    });
    assert!(
        matches!(transition(state, event), Err(MachineError::Budget(message)) if message.contains("aggregate mismatch"))
    );
}

#[test]
fn cleanup_reports_a_reservation_missing_from_the_ledger() {
    let (mut state, id) = pending_state(RunPhase::Cleaning, CompletionKind::CleanupFinished);
    state.pending.get_mut(&id).unwrap().cleanup_reservations = Some(vec![crate::ReservationId(99)]);
    let event = RunEvent::CleanupFinished(crate::CleanupFinished {
        id,
        released_reservations: vec![crate::ReservationId(99)],
    });
    assert!(
        matches!(transition(state, event), Err(MachineError::Budget(message)) if message.contains("unknown"))
    );
}

#[test]
fn persistence_acknowledgement_requires_an_existing_worker() {
    let (state, id) = pending_state(RunPhase::Mutants, CompletionKind::ResultPersisted);
    let event = RunEvent::ResultPersisted(crate::ResultPersisted {
        id,
        worker: 0,
        run_id: "coverage".into(),
        mutant_id: "m1".into(),
    });
    assert!(matches!(
        transition(state, event),
        Err(MachineError::UnknownWorker(0))
    ));
}

#[test]
fn reused_result_requires_a_candidate_owned_by_its_worker() {
    let (state, id) = pending_state(RunPhase::Mutants, CompletionKind::StoredResultLoaded);
    let event = RunEvent::StoredResultLoaded(crate::StoredResultLoaded {
        id,
        worker: 0,
        result: Some(crate::StoredResult {
            mutant_id: "m1".into(),
            status: crate::MutationStatus::Killed,
        }),
    });
    assert!(matches!(
        transition(state, event),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Mutants
        })
    ));
}

#[rstest::rstest]
#[case(RunPhase::Mutants)]
#[case(RunPhase::BaselineRestoration)]
fn worker_reset_completion_cannot_be_transferred_to_another_worker(#[case] phase: RunPhase) {
    let (state, id) = pending_state(phase, CompletionKind::WorkerReset);
    assert!(matches!(
        transition(
            state,
            RunEvent::WorkerReset(crate::WorkerReset { id, worker: 1 })
        ),
        Err(MachineError::UnknownWorker(1))
    ));
}

#[rstest::rstest]
#[case(RunEvent::DeadlineReached)]
#[case(RunEvent::CancellationRequested)]
#[case(RunEvent::DiskStopRequested(crate::DiskStopRequested { failure: crate::DiskFailure { reason: crate::DiskFailureReason::MeasurementFailed("unavailable".into()), secondary: vec![] } }))]
fn stop_events_propagate_identifier_exhaustion(#[case] event: RunEvent) {
    let mut state = state();
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, event),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(RunPhase::Cleaning, CompletionKind::CleanupFinished, true)]
#[case(RunPhase::Preflight, CompletionKind::TargetsResolved, false)]
#[case(RunPhase::Finalize, CompletionKind::OriginalsVerified, true)]
#[case(RunPhase::Mutants, CompletionKind::CandidateLoaded, true)]
fn effect_failure_propagates_identifier_exhaustion_from_its_recovery_effect(
    #[case] phase: RunPhase,
    #[case] kind: CompletionKind,
    #[case] report_started: bool,
) {
    let (mut state, id) = pending_state(phase, kind);
    state.workers.insert(0, WorkerState::Idle);
    state.flags.report.report_started = report_started;
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::EffectFailed(EffectFailed::other(id, "test.io", "denied"))
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn failed_candidate_read_requires_its_worker_entry_for_recovery() {
    let (state, id) = pending_state(RunPhase::Mutants, CompletionKind::CandidateLoaded);
    assert!(matches!(
        transition(
            state,
            RunEvent::EffectFailed(EffectFailed::other(id, "test.io", "denied"))
        ),
        Err(MachineError::UnknownWorker(0))
    ));
}

#[rstest::rstest]
#[case(OutputContinuation::RunStarted, false)]
#[case(OutputContinuation::Diagnostic, false)]
#[case(OutputContinuation::Diagnostic, true)]
fn output_failure_propagates_identifier_exhaustion_from_its_recovery_effect(
    #[case] continuation: OutputContinuation,
    #[case] cleanup_done: bool,
) {
    let mut state = state();
    state.phase = RunPhase::Preflight;
    state.flags.cleanup.cleanup_done = cleanup_done;
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
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::EffectFailed(EffectFailed::other(id, "test.io", "denied"))
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(None, |state: &mut RunState| state.flags.scheduling.stop_requested = true)]
#[case(None, |_: &mut RunState| {})]
#[case(Some(candidate()), |state: &mut RunState| state.flags.scheduling.stop_requested = true)]
#[case(Some(candidate()), |state: &mut RunState| state.scheduled_mutants = u64::try_from(state.config.limits.max_mutants.get()).unwrap())]
#[case(Some(candidate()), |state: &mut RunState| state.selection = RunSelection::explicit(["other".into()], crate::VerificationSelection::candidate_ids(1, 1, false).unwrap()).unwrap())]
#[case(Some(candidate()), |_: &mut RunState| {})]
fn candidate_completion_propagates_identifier_exhaustion_for_its_dispatch_path(
    #[case] loaded: Option<MutationCandidate>,
    #[case] configure: fn(&mut RunState),
) {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::CandidateLoaded);
    state
        .workers
        .insert(0, WorkerState::Reading(CandidateReadTarget::Next));
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    configure(&mut state);
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::CandidateLoaded(crate::CandidateLoaded {
                id,
                worker: 0,
                candidate: loaded
            })
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(None)]
#[case(Some(crate::StoredResult { mutant_id: "m1".into(), status: crate::MutationStatus::Killed }))]
fn stored_result_completion_propagates_identifier_exhaustion(
    #[case] result: Option<crate::StoredResult>,
) {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::StoredResultLoaded);
    state.workers.insert(0, WorkerState::Lookup(candidate()));
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::StoredResultLoaded(crate::StoredResultLoaded {
                id,
                worker: 0,
                result
            })
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(
    OutputContinuation::StartSynthetic(0),
    WorkerState::SyntheticStarting(candidate(), SyntheticOutcome::NotRun),
    false
)]
#[case(
    OutputContinuation::FinishSynthetic(0),
    WorkerState::SyntheticFinishing(candidate(), SyntheticOutcome::NotRun),
    false
)]
#[case(
    OutputContinuation::FinishSynthetic(0),
    WorkerState::SyntheticFinishing(candidate(), SyntheticOutcome::NotRun),
    true
)]
#[case(
    OutputContinuation::FinishMutant(0),
    WorkerState::Finishing(result_fixture()),
    true
)]
fn synthetic_and_stopped_output_completion_propagates_identifier_exhaustion(
    #[case] continuation: OutputContinuation,
    #[case] worker: WorkerState,
    #[case] stop_requested: bool,
) {
    let mut state = state();
    state.phase = RunPhase::Mutants;
    state.workers.insert(0, worker);
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    state.flags.scheduling.stop_requested = stop_requested;
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
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn baseline_completion_requires_the_created_worker_entry() {
    let (state, id) = pending_state(RunPhase::Baseline, CompletionKind::BaselineFinished);
    assert!(matches!(
        transition(
            state,
            RunEvent::BaselineFinished(process_finished(id, Some(0)))
        ),
        Err(MachineError::UnknownWorker(0))
    ));
}

#[test]
fn persisted_result_output_propagates_identifier_exhaustion() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::ResultPersisted);
    state
        .workers
        .insert(0, WorkerState::Persisting(result_fixture()));
    state.next_effect_id = u64::MAX;
    let event = RunEvent::ResultPersisted(crate::ResultPersisted {
        id,
        worker: 0,
        run_id: "coverage".into(),
        mutant_id: "m1".into(),
    });
    assert!(matches!(
        transition(state, event),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case("test.io", "primary", &[crate::DiskSecondary::Error { code: "test.io".into(), message: "primary".into() }])]
#[case(crate::DISK_MEASUREMENT_FAILED, "primary", &[])]
#[case(crate::DISK_MEASUREMENT_FAILED, "different", &[crate::DiskSecondary::Error { code: crate::DISK_MEASUREMENT_FAILED.into(), message: "different".into() }])]
fn effect_failure_retains_distinct_disk_evidence_without_repeating_the_primary(
    #[case] code: &str,
    #[case] message: &str,
    #[case] expected: &[crate::DiskSecondary],
) {
    let (mut state, id) = pending_state(RunPhase::Finalize, CompletionKind::OriginalsVerified);
    state.flags.report.report_started = true;
    state.disk_summary.stop = Some(
        crate::DiskFailure {
            reason: crate::DiskFailureReason::MeasurementFailed("primary".into()),
            secondary: vec![],
        }
        .into(),
    );
    let (state, _) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(id, code, message)),
    )
    .unwrap();
    let stop = state.disk_summary.stop.as_ref().unwrap();
    assert_eq!(stop.code, crate::DISK_MEASUREMENT_FAILED);
    assert_eq!(stop.message.as_deref(), Some("primary"));
    assert_eq!(stop.secondary.as_slice(), expected);
}

#[test]
fn repeated_effect_failure_does_not_repeat_the_secondary_disk_error() {
    let (mut state, id) = pending_state(RunPhase::Finalize, CompletionKind::OriginalsVerified);
    state.flags.report.report_started = true;
    state.disk_summary.stop = Some(
        crate::DiskFailure {
            reason: crate::DiskFailureReason::MeasurementFailed("primary".into()),
            secondary: vec![],
        }
        .into(),
    );
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(id, "test.io", "secondary")),
    )
    .unwrap();
    let next_id = effects.first().unwrap().id();
    let (state, _) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(next_id, "test.io", "secondary")),
    )
    .unwrap();
    assert_eq!(
        state.disk_summary.stop.as_ref().unwrap().secondary,
        [crate::DiskSecondary::Error {
            code: "test.io".into(),
            message: "secondary".into()
        }]
    );
}

#[test]
fn baseline_success_emits_output_and_requires_worker_restoration() {
    let (mut state, id) = pending_state(RunPhase::Baseline, CompletionKind::BaselineFinished);
    reserve_single_worker_fixture_workspace(&mut state);
    state.workers.insert(0, WorkerState::Idle);
    state.flags.report.report_started = true;
    state.output_sequence = 1;
    let (state, effects) = transition(
        state,
        RunEvent::BaselineFinished(process_finished(id, Some(0))),
    )
    .unwrap();
    assert_eq!(state.phase, RunPhase::BaselineRestoration);
    assert!(matches!(state.workers[&0], WorkerState::Resetting));
    assert_eq!(
        effects.iter().map(expected_completion).collect::<Vec<_>>(),
        [CompletionKind::OutputEmitted, CompletionKind::WorkerReset]
    );
    assert_eq!(state.output_sequence, 2);
}

#[test]
fn creating_one_of_two_workers_keeps_the_materialization_barrier_closed() {
    let mut state = state();
    let mut data = state.config.clone().into_data();
    data.limits.jobs = std::num::NonZeroUsize::new(2).unwrap();
    state.config = RunConfig::try_from(data).unwrap();
    let grant = reserve_workspace_copy(
        &mut state.budgets,
        &crate::PreflightCompleted {
            id: EffectId(0),
            requested_workers: 2,
            per_worker_logical_bytes: 1,
            aggregate_logical_bytes: 2,
            fingerprint: None,
        },
    )
    .unwrap();
    state.copy_grant = Some(grant);
    let requests = state.create_worker_effects().unwrap();
    state.register(&requests).unwrap();
    let (state, effects) = transition(
        state,
        RunEvent::WorkerCreated(crate::WorkerCreated {
            id: requests[0].id(),
            worker: 0,
            reservation_id: grant.reservation_id(),
        }),
    )
    .unwrap();
    assert_eq!(effects, []);
    assert_eq!(state.phase, RunPhase::Copy);
    assert_eq!(state.workers.len(), 1);
    assert!(state.is_effect_pending(requests[1].id()));
}

#[rstest::rstest]
#[case(crate::SessionMode::Fresh(crate::SessionConfig { path: "session.db".into() }), CompletionKind::SessionStarted)]
#[case(crate::SessionMode::Resume(crate::SessionConfig { path: "session.db".into() }), CompletionKind::SessionLoaded)]
fn preflight_dispatches_the_requested_session_mode(
    #[case] mode: crate::SessionMode,
    #[case] expected: CompletionKind,
) {
    let (mut state, id) = pending_state(RunPhase::Preflight, CompletionKind::PreflightCompleted);
    let mut data = state.config.clone().into_data();
    data.session = mode;
    state.config = RunConfig::try_from(data).unwrap();
    let fingerprint = crate::RunFingerprint::from_bytes([7; 32]);
    let event = RunEvent::PreflightCompleted(crate::PreflightCompleted {
        id,
        requested_workers: 1,
        per_worker_logical_bytes: 1,
        aggregate_logical_bytes: 1,
        fingerprint: Some(fingerprint),
    });
    let (state, effects) = transition(state, event).unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(expected_completion(&effects[0]), expected);
    assert_eq!(state.phase, RunPhase::Preflight);
    assert_eq!(state.fingerprint, Some(fingerprint));
    assert!(state.copy_grant.is_some());
}

fn budget_observation_fixture() -> (RunState, EffectId) {
    let (mut state, id) = pending_state(
        RunPhase::BudgetCheck,
        CompletionKind::RemainingBudgetObserved,
    );
    reserve_single_worker_fixture_workspace(&mut state);
    state.pending.get_mut(&id).unwrap().worker = None;
    state.workers.insert(0, WorkerState::Idle);
    state.flags.report.report_started = true;
    state.output_sequence = 2;
    state.baseline_elapsed = std::time::Duration::from_secs(5);
    state.targets.push_back(crate::TargetSlice {
        path: "src/calc.py".into(),
        lines: vec![],
        symbols: vec![],
    });
    state.selection = RunSelection::ordered(
        vec!["m1".into()],
        crate::VerificationSelection::top(1, 1, crate::VerificationTopPolicy::Strict, false)
            .unwrap(),
    )
    .unwrap();
    (state, id)
}

#[test]
fn short_budget_observation_emits_a_warning_before_analysis() {
    let (state, id) = budget_observation_fixture();
    let (state, effects) = transition(
        state,
        RunEvent::RemainingBudgetObserved(crate::RemainingBudgetObserved {
            id,
            remaining: std::time::Duration::ZERO,
        }),
    )
    .unwrap();
    assert_eq!(state.phase, RunPhase::BudgetCheck);
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::EmitOutput(output) if matches!(&output.event, OutputEvent::Diagnostic(diagnostic) if diagnostic.code == "budget.projected_shortfall"))
    );
    assert_eq!(state.output_sequence, 3);
}

#[test]
fn sufficient_budget_observation_starts_analysis_without_an_output() {
    let (state, id) = budget_observation_fixture();
    let (state, effects) = transition(
        state,
        RunEvent::RemainingBudgetObserved(crate::RemainingBudgetObserved {
            id,
            remaining: std::time::Duration::MAX,
        }),
    )
    .unwrap();
    assert_eq!(state.phase, RunPhase::Analyze);
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::AnalyzeFile(analyze) if analyze.target.path == "src/calc.py")
    );
    assert_eq!(state.output_sequence, 2);
}

#[rstest::rstest]
#[case(|id| RunEvent::SessionFinished(crate::SessionFinished { id, run_id: "session".into(), complete: true }), 0)]
#[case(|id| RunEvent::EffectFailed(EffectFailed::other(id, "session.finish", "denied")), 2)]
fn terminal_session_completion_finishes_without_reissuing_effects(
    #[case] event: fn(EffectId) -> RunEvent,
    #[case] exit: i32,
) {
    let (mut state, id) = pending_state(RunPhase::Finalize, CompletionKind::SessionFinished);
    state.pending.get_mut(&id).unwrap().worker = None;
    state.identity = RunIdentity::Session("session".into());
    state.flags.cleanup.cleanup_done = true;
    state.flags.cleanup.session_finish_attempted = true;
    state.flags.report.report_started = true;
    let (state, effects) = transition(state, event(id)).unwrap();
    assert_eq!(effects, []);
    assert_eq!(state.phase, RunPhase::Finished);
    assert_eq!(state.exit_code(), exit);
    assert!(!state.is_effect_pending(id));
}

fn reserve_single_worker_fixture_workspace(state: &mut RunState) {
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
}

#[test]
fn candidate_spool_eof_rejects_missing_explicit_selection() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::CandidateLoaded);
    state
        .workers
        .insert(0, WorkerState::Reading(CandidateReadTarget::Next));
    state.selection = RunSelection::explicit(
        ["m1".into()],
        crate::VerificationSelection::candidate_ids(1, 1, false).unwrap(),
    )
    .unwrap();
    assert!(
        matches!(transition(state, RunEvent::CandidateLoaded(crate::CandidateLoaded { id, worker: 0, candidate: None })), Err(MachineError::SelectedCandidateMissing(found)) if found == "m1")
    );
}

#[rstest::rstest]
#[case(None, None)]
#[case(Some(candidate()), Some("m1"))]
fn indexed_candidate_completion_rejects_the_wrong_identity(
    #[case] loaded: Option<MutationCandidate>,
    #[case] received: Option<&str>,
) {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::CandidateLoaded);
    state.workers.insert(
        0,
        WorkerState::Reading(CandidateReadTarget::Id("expected".into())),
    );
    assert!(
        matches!(transition(state, RunEvent::CandidateLoaded(crate::CandidateLoaded { id, worker: 0, candidate: loaded })), Err(MachineError::CandidateIdentityMismatch { worker: 0, expected, received: actual }) if expected == "expected" && actual.as_deref() == received)
    );
}

#[test]
fn stored_result_completion_rejects_a_different_mutant_identity() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::StoredResultLoaded);
    state.workers.insert(0, WorkerState::Lookup(candidate()));
    let event = RunEvent::StoredResultLoaded(crate::StoredResultLoaded {
        id,
        worker: 0,
        result: Some(crate::StoredResult {
            mutant_id: "different".into(),
            status: crate::MutationStatus::Killed,
        }),
    });
    assert!(
        matches!(transition(state, event), Err(MachineError::SessionIdentityMismatch { worker: 0, field: "mutant_id", expected, received }) if expected == "m1" && received == "different")
    );
}

#[test]
fn incomplete_mutant_output_is_persisted_before_finished_output_and_reset() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::MutantFinished);
    reserve_single_worker_fixture_workspace(&mut state);
    state.identity = RunIdentity::Session("coverage".into());
    state.workers.insert(0, WorkerState::Running(candidate()));
    state.flags.report.report_started = true;
    state.scheduled_mutants = 1;
    state.output_sequence = 3;
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    let mut finished = process_finished(id, Some(0));
    finished.output_state = ProcessOutputState::CloseTimedOut;
    let (state, effects) = transition(state, RunEvent::MutantFinished(finished)).unwrap();
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::PersistResult(persist) if persist.result.outcome.status() == crate::MutationStatus::Error && persist.result.candidate == candidate() && persist.result.diagnostics[0].code == PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE)
    );
    assert_eq!(state.summary.error, 0);
    let (state, effects) = transition(
        state,
        RunEvent::ResultPersisted(crate::ResultPersisted {
            id: effects[0].id(),
            worker: 0,
            run_id: "coverage".into(),
            mutant_id: "m1".into(),
        }),
    )
    .unwrap();
    assert_eq!(state.summary.error, 1);
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::EmitOutput(output) if matches!(&output.event, OutputEvent::MutantFinished(result) if result.candidate == candidate() && result.outcome.status() == crate::MutationStatus::Error && result.sequence == 4))
    );
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(crate::OutputEmitted {
            id: effects[0].id(),
        }),
    )
    .unwrap();
    assert_eq!(effects.len(), 1);
    assert!(matches!(&effects[0], RunEffect::ResetWorker(reset) if reset.worker == 0));
    assert!(matches!(state.workers[&0], WorkerState::Resetting));
}

fn ordered_restoration_fixture() -> (RunState, EffectId) {
    let (mut state, id) = pending_state(RunPhase::BaselineRestoration, CompletionKind::WorkerReset);
    reserve_single_worker_fixture_workspace(&mut state);
    state.workers.insert(0, WorkerState::Resetting);
    state.flags.report.report_started = true;
    state.selection = RunSelection::ordered(
        vec!["m1".into()],
        crate::VerificationSelection::top(1, 1, crate::VerificationTopPolicy::Strict, false)
            .unwrap(),
    )
    .unwrap();
    (state, id)
}

#[test]
fn ordered_baseline_restoration_observes_remaining_budget_before_analysis() {
    let (state, id) = ordered_restoration_fixture();
    let (state, effects) = transition(
        state,
        RunEvent::WorkerReset(crate::WorkerReset { id, worker: 0 }),
    )
    .unwrap();
    assert_eq!(state.phase, RunPhase::BudgetCheck);
    assert_eq!(effects.len(), 1);
    assert_eq!(
        expected_completion(&effects[0]),
        CompletionKind::RemainingBudgetObserved
    );
    assert!(matches!(state.workers[&0], WorkerState::Idle));
}

#[test]
fn ordered_baseline_restoration_propagates_identifier_exhaustion() {
    let (mut state, id) = ordered_restoration_fixture();
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::WorkerReset(crate::WorkerReset { id, worker: 0 })
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn cancellation_during_cleanup_preserves_the_cleanup_request() {
    let (mut state, id) = pending_state(RunPhase::Cleaning, CompletionKind::CleanupFinished);
    state.pending.get_mut(&id).unwrap().worker = None;
    let (state, effects) = transition(state, RunEvent::CancellationRequested).unwrap();
    assert_eq!(effects, []);
    assert_eq!(state.phase, RunPhase::Cleaning);
    assert!(state.is_effect_pending(id));
    assert!(!state.flags.report.interrupted);
}

#[test]
fn cancellation_after_finish_does_not_allocate_or_change_outcome_flags() {
    let mut state = state();
    state.phase = RunPhase::Finished;
    let next = state.next_effect_id;
    let (state, effects) = transition(state, RunEvent::CancellationRequested).unwrap();
    assert_eq!(effects, []);
    assert_eq!(state.phase, RunPhase::Finished);
    assert_eq!(state.next_effect_id, next);
    assert!(!state.flags.report.interrupted);
}

#[test]
fn cancellation_preserves_a_terminal_session_finish_request() {
    let (mut state, id) = pending_state(RunPhase::Finalize, CompletionKind::SessionFinished);
    state.pending.get_mut(&id).unwrap().worker = None;
    state.identity = RunIdentity::Session("coverage".into());
    state.flags.report.report_started = true;
    state.flags.cleanup.cleanup_done = true;
    state.flags.cleanup.session_finish_attempted = true;
    let (state, effects) = transition(state, RunEvent::CancellationRequested).unwrap();
    assert_eq!(effects, []);
    assert_eq!(state.phase, RunPhase::Finalize);
    assert!(state.is_effect_pending(id));
    assert!(!state.flags.report.interrupted);
}

#[test]
fn cancellation_preserves_a_pending_terminal_report() {
    let mut state = state();
    state.flags.report.report_started = true;
    state.flags.cleanup.cleanup_done = true;
    state.output_sequence = 2;
    let terminal = state.final_report_effects().unwrap();
    let id = terminal[0].id();
    let (state, effects) = transition(state, RunEvent::CancellationRequested).unwrap();
    assert_eq!(effects, []);
    assert!(state.is_effect_pending(id));
    assert_eq!(state.output_sequence, 3);
    assert!(!state.flags.report.interrupted);
}

#[test]
fn resume_preflight_requires_a_fingerprint() {
    let (mut state, id) = pending_state(RunPhase::Preflight, CompletionKind::PreflightCompleted);
    let mut data = state.config.clone().into_data();
    data.session = crate::SessionMode::Resume(crate::SessionConfig {
        path: "session.db".into(),
    });
    state.config = RunConfig::try_from(data).unwrap();
    let event = RunEvent::PreflightCompleted(crate::PreflightCompleted {
        id,
        requested_workers: 1,
        per_worker_logical_bytes: 1,
        aggregate_logical_bytes: 1,
        fingerprint: None,
    });
    assert!(matches!(
        transition(state, event),
        Err(MachineError::MissingFingerprint)
    ));
}

#[rstest::rstest]
#[case(std::time::Duration::ZERO)]
#[case(std::time::Duration::MAX)]
fn budget_projection_continuation_propagates_identifier_exhaustion(
    #[case] remaining: std::time::Duration,
) {
    let (mut state, id) = budget_observation_fixture();
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::RemainingBudgetObserved(crate::RemainingBudgetObserved { id, remaining })
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

fn ordered_collection_fixture() -> (RunState, EffectId) {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::CandidateLoaded);
    reserve_single_worker_fixture_workspace(&mut state);
    state.flags.report.report_started = true;
    state
        .workers
        .insert(0, WorkerState::Reading(CandidateReadTarget::Next));
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    state.selection = RunSelection::ordered(
        vec!["m1".into()],
        crate::VerificationSelection::top(1, 1, crate::VerificationTopPolicy::Strict, false)
            .unwrap(),
    )
    .unwrap();
    (state, id)
}

#[test]
fn ordered_collection_requires_eof_before_requesting_an_indexed_candidate() {
    let (state, id) = ordered_collection_fixture();
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(crate::CandidateLoaded {
            id,
            worker: 0,
            candidate: Some(candidate()),
        }),
    )
    .unwrap();
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::ReadCandidate(read) if read.target == CandidateReadTarget::Next)
    );
    assert_eq!(state.scheduled_mutants, 0);
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(crate::CandidateLoaded {
            id: effects[0].id(),
            worker: 0,
            candidate: None,
        }),
    )
    .unwrap();
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::ReadCandidate(read) if read.target == CandidateReadTarget::Id("m1".into()))
    );
    assert_eq!(state.scheduled_mutants, 0);
}

#[test]
fn ordered_collection_propagates_identifier_exhaustion() {
    let (mut state, id) = ordered_collection_fixture();
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::CandidateLoaded(crate::CandidateLoaded {
                id,
                worker: 0,
                candidate: Some(candidate())
            })
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn collecting_an_ordered_candidate_requires_the_worker_entry() {
    let mut state = state();
    state.selection = RunSelection::ordered(
        vec!["m1".into()],
        crate::VerificationSelection::top(1, 1, crate::VerificationTopPolicy::Strict, false)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        state.collect_ordered_candidate(0, Some(candidate())),
        Err(MachineError::UnknownWorker(0))
    );
}

#[test]
fn an_ordinary_candidate_completion_dispatches_its_mutation() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::CandidateLoaded);
    reserve_single_worker_fixture_workspace(&mut state);
    state.flags.report.report_started = true;
    state
        .workers
        .insert(0, WorkerState::Reading(CandidateReadTarget::Next));
    state.candidate_spool = Some(crate::CandidateSpoolRef {
        token: "candidates".into(),
        records: 1,
    });
    let (state, effects) = transition(
        state,
        RunEvent::CandidateLoaded(crate::CandidateLoaded {
            id,
            worker: 0,
            candidate: Some(candidate()),
        }),
    )
    .unwrap();
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::ApplyMutation(apply) if apply.worker == 0 && apply.candidate == candidate())
    );
    assert_eq!(state.scheduled_mutants, 1);
    assert!(matches!(&state.workers[&0], WorkerState::Applying(owned) if owned == &candidate()));
}

#[test]
fn mutant_completion_requires_an_owned_candidate() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::MutantFinished);
    state.workers.insert(0, WorkerState::Idle);
    assert!(matches!(
        transition(
            state,
            RunEvent::MutantFinished(process_finished(id, Some(0)))
        ),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Mutants
        })
    ));
}

#[test]
fn mutant_persistence_propagates_identifier_exhaustion() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::MutantFinished);
    state.identity = RunIdentity::Session("coverage".into());
    state.workers.insert(0, WorkerState::Running(candidate()));
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::MutantFinished(process_finished(id, Some(0)))
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn baseline_output_acknowledgement_keeps_restoration_pending() {
    let mut state = state();
    reserve_single_worker_fixture_workspace(&mut state);
    state.phase = RunPhase::BaselineRestoration;
    state.flags.report.report_started = true;
    state.workers.insert(0, WorkerState::Resetting);
    let output_id = state.allocate_id().unwrap();
    let event = baseline_output(&mut state, &process_finished(EffectId(0), Some(0)));
    state.register_output(
        EmitOutput {
            id: output_id,
            event,
        },
        OutputContinuation::Baseline,
    );
    let reset_id = state.allocate_id().unwrap();
    state
        .register(&[RunEffect::ResetWorker(ResetWorker {
            id: reset_id,
            worker: 0,
        })])
        .unwrap();
    let (state, effects) = transition(
        state,
        RunEvent::OutputEmitted(crate::OutputEmitted { id: output_id }),
    )
    .unwrap();
    assert_eq!(effects, []);
    assert_eq!(state.phase, RunPhase::BaselineRestoration);
    assert!(state.is_effect_pending(reset_id));
    assert!(!state.is_effect_pending(output_id));
}

fn run_started_output_fixture() -> (RunState, EffectId) {
    let mut state = state();
    state.phase = RunPhase::Preflight;
    let effects = state.start_run_effects().unwrap();
    (state, effects[0].id())
}

#[test]
fn run_started_acknowledgement_delivers_its_saved_failure_once() {
    let (mut state, id) = run_started_output_fixture();
    state.flags.outcome.infrastructure_error = true;
    state.pending_failure = Some(EffectFailed::other(EffectId(0), "test.io", "denied"));
    let (state, effects) =
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })).unwrap();
    assert_eq!(effects.len(), 1);
    assert!(
        matches!(&effects[0], RunEffect::EmitOutput(output) if matches!(&output.event, OutputEvent::Diagnostic(diagnostic) if diagnostic.code == "test.io" && diagnostic.message == "denied"))
    );
    assert!(state.pending_failure.is_none());
    assert!(state.flags.report.report_started);
}

#[test]
fn run_started_failure_diagnostic_propagates_identifier_exhaustion() {
    let (mut state, id) = run_started_output_fixture();
    state.pending_failure = Some(EffectFailed::other(EffectId(0), "test.io", "denied"));
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(|_: &mut RunState| {}, RunPhase::Cleaning, CompletionKind::CleanupFinished)]
#[case(
    reserve_single_worker_fixture_workspace,
    RunPhase::Finalize,
    CompletionKind::OriginalsVerified
)]
fn stopping_before_run_started_acknowledgement_preserves_cleanup_order(
    #[case] configure: fn(&mut RunState),
    #[case] phase: RunPhase,
    #[case] completion: CompletionKind,
) {
    let (mut state, id) = run_started_output_fixture();
    configure(&mut state);
    state.flags.report.stop_after_run_started = true;
    state.flags.scheduling.stop_requested = true;
    let (state, effects) =
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })).unwrap();
    assert_eq!(state.phase, phase);
    assert_eq!(effects.len(), 1);
    assert_eq!(expected_completion(&effects[0]), completion);
    assert!(!state.flags.report.stop_after_run_started);
    assert!(state.flags.report.report_started);
}

#[test]
fn duplicate_start_request_is_rejected_in_preflight() {
    let (state, _) = transition(state(), RunEvent::StartRequested(StartRequested)).unwrap();
    assert!(matches!(
        transition(state, RunEvent::StartRequested(StartRequested)),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Preflight
        })
    ));
}

#[rstest::rstest]
#[case(|_: &mut RunState| {})]
#[case(reserve_single_worker_fixture_workspace)]
fn stopping_before_header_acknowledgement_preserves_identifier_errors(
    #[case] configure: fn(&mut RunState),
) {
    let (mut state, id) = run_started_output_fixture();
    configure(&mut state);
    state.flags.report.stop_after_run_started = true;
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::EffectIdOverflow)
    ));
}

fn diagnostic_after_cleanup_fixture() -> (RunState, EffectId) {
    let mut state = state();
    state.phase = RunPhase::Cleaning;
    state.flags.report.report_started = true;
    state.flags.cleanup.cleanup_done = true;
    let effects = state
        .diagnostic_effect(&EffectFailed::other(EffectId(0), "test.io", "denied"))
        .unwrap();
    (state, effects[0].id())
}

#[test]
fn diagnostic_acknowledgement_after_cleanup_emits_the_final_report() {
    let (state, id) = diagnostic_after_cleanup_fixture();
    let (state, effects) =
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })).unwrap();
    assert!(
        matches!(&effects[..], [RunEffect::EmitOutput(output)] if matches!(output.event, OutputEvent::RunFinished(_)))
    );
    assert!(state.flags.cleanup.cleanup_done);
}

#[test]
fn diagnostic_acknowledgement_after_cleanup_preserves_identifier_errors() {
    let (mut state, id) = diagnostic_after_cleanup_fixture();
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[test]
fn successful_cleanup_preserves_final_report_identifier_errors() {
    let (mut state, id) = pending_state(RunPhase::Cleaning, CompletionKind::CleanupFinished);
    state.flags.report.report_started = true;
    state.next_effect_id = u64::MAX;
    assert!(matches!(
        transition(
            state,
            RunEvent::CleanupFinished(crate::CleanupFinished {
                id,
                released_reservations: Vec::new(),
            })
        ),
        Err(MachineError::EffectIdOverflow)
    ));
}

#[rstest::rstest]
#[case(OutputContinuation::RunStarted)]
#[case(OutputContinuation::Diagnostic)]
fn failed_report_delivery_requests_cleanup_without_another_output(
    #[case] continuation: OutputContinuation,
) {
    let (mut state, id) = run_started_output_fixture();
    state.pending.get_mut(&id).unwrap().completion = PendingCompletion::Output(continuation);
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed {
            id,
            failure: EffectFailure::ReportIo {
                operation: "write".into(),
                message: "closed".into(),
            },
        }),
    )
    .unwrap();
    assert!(matches!(&effects[..], [RunEffect::Cleanup(_)]));
    assert_eq!(state.flags.report.delivery, ReportDelivery::Failed);
    assert!(state.flags.outcome.infrastructure_error);
}

#[test]
fn failed_terminal_report_finishes_without_retrying_output() {
    let (mut state, id) = diagnostic_after_cleanup_fixture();
    state.pending.get_mut(&id).unwrap().completion =
        PendingCompletion::Output(OutputContinuation::RunFinished);
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed {
            id,
            failure: EffectFailure::ReportIo {
                operation: "write".into(),
                message: "closed".into(),
            },
        }),
    )
    .unwrap();
    assert_eq!(effects, []);
    assert_eq!(state.phase, RunPhase::Finished);
    assert_eq!(state.flags.report.delivery, ReportDelivery::Failed);
}

#[test]
fn stopped_mutant_acknowledgement_requires_the_worker_entry() {
    let (mut state, id) = run_started_output_fixture();
    state.pending.get_mut(&id).unwrap().completion =
        PendingCompletion::Output(OutputContinuation::FinishMutant(0));
    state.flags.scheduling.stop_requested = true;
    assert!(matches!(
        transition(state, RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::UnknownWorker(0))
    ));
}

#[test]
fn deferred_cleanup_is_reported_as_deferred() {
    let (mut state, id) = pending_state(RunPhase::Cleaning, CompletionKind::CleanupFinished);
    state.flags.report.report_started = true;
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(
            id,
            crate::WORKSPACE_CLEANUP_DEFERRED,
            "cleanup lock held",
        )),
    )
    .unwrap();
    assert_eq!(
        state.disk_summary.cleanup[0].status,
        crate::DiskCleanupStatus::Deferred
    );
    assert!(
        matches!(&effects[..], [RunEffect::EmitOutput(output)] if matches!(&output.event, OutputEvent::Diagnostic(diagnostic) if diagnostic.code == crate::WORKSPACE_CLEANUP_DEFERRED))
    );
    assert!(state.flags.cleanup.cleanup_done);
}

#[test]
fn failed_mutant_process_drains_its_owned_candidate_as_not_run() {
    let (mut state, id) = pending_state(RunPhase::Mutants, CompletionKind::MutantFinished);
    reserve_single_worker_fixture_workspace(&mut state);
    state.flags.report.report_started = true;
    state.workers.insert(0, WorkerState::Running(candidate()));
    let (state, effects) = transition(
        state,
        RunEvent::EffectFailed(EffectFailed::other(id, "process.io", "denied")),
    )
    .unwrap();
    assert!(
        matches!(&effects[..], [RunEffect::EmitOutput(output)] if matches!(&output.event, OutputEvent::MutantFinished(finished) if finished.candidate.id == "m1" && finished.outcome == crate::MutationOutcome::NotRun))
    );
    assert!(
        matches!(&state.workers[&0], WorkerState::SyntheticFinishing(owned, SyntheticOutcome::NotRun) if owned == &candidate())
    );
    assert_eq!(state.pending_failure.unwrap().failure.code(), "process.io");
}

#[rstest::rstest]
#[case(|state: &mut RunState, _| state.materialization_verification_effects(), CompletionKind::OriginalsVerified)]
#[case(|state: &mut RunState, _| state.baseline_effects(), CompletionKind::BaselineFinished)]
#[case(RunState::candidate_effects, CompletionKind::StoredResultLoaded)]
#[case(RunState::mutant_process, CompletionKind::MutantFinished)]
#[case(RunState::started_output, CompletionKind::OutputEmitted)]
fn owned_worker_effects_emit_their_expected_completion(
    #[case] build: fn(&mut RunState, u32) -> Result<Vec<RunEffect>, MachineError>,
    #[case] expected: CompletionKind,
) {
    let mut state = exhausted_effect_fixture();
    state.next_effect_id = 1;
    state.workers.insert(0, WorkerState::Ready(candidate()));
    let effects = build(&mut state, 0).unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(expected_completion(&effects[0]), expected);
}

#[test]
fn preflight_effects_preserve_identifier_exhaustion() {
    let mut state = state();
    state.next_effect_id = u64::MAX;
    assert_eq!(
        state.preflight_effects(),
        Err(MachineError::EffectIdOverflow)
    );
}

#[test]
fn beginning_a_session_requires_its_fingerprint() {
    let mut state = state();
    assert_eq!(
        state.begin_session_effects(),
        Err(MachineError::MissingFingerprint)
    );
}

#[test]
fn stopped_run_without_a_copy_grant_requests_cleanup() {
    let mut state = state();
    state.flags.report.report_started = true;
    let effects = state.begin_stop().unwrap();
    assert!(matches!(&effects[..], [RunEffect::Cleanup(_)]));
    assert_eq!(state.phase, RunPhase::Cleaning);
}

#[test]
fn finishing_a_session_emits_its_persistence_effect_once() {
    let mut state = state();
    state.identity = RunIdentity::Session("saved".into());
    let effects = state.post_report_effects().unwrap();
    state.register(&effects).unwrap();
    assert!(matches!(&effects[..], [RunEffect::FinishSession(finish)] if finish.run_id == "saved"));
    assert!(state.flags.cleanup.session_finish_attempted);
}

#[test]
fn accepting_a_missing_previously_allocated_effect_reports_not_pending() {
    let mut state = state();
    state.next_effect_id = 3;
    assert_eq!(
        state.accept_completion(&RunEvent::OutputEmitted(crate::OutputEmitted {
            id: EffectId(2)
        })),
        Err(MachineError::EffectNotPending(EffectId(2)))
    );
}

#[test]
fn stopped_candidate_dispatch_requires_an_available_worker() {
    let mut state = state();
    assert_eq!(
        state.next_stopped_candidate_effects(),
        Err(MachineError::WrongPhase {
            phase: RunPhase::Validate
        })
    );
}

#[rstest::rstest]
#[case(&MachineError::UnknownEffect(EffectId(1)), "machine.effect.unknown")]
#[case(&MachineError::DuplicateEffect(EffectId(1)), "machine.effect.duplicate")]
#[case(&MachineError::RetiredEffect(EffectId(1)), "machine.effect.retired")]
#[case(&MachineError::WrongCompletion { id: EffectId(1), expected: CompletionKind::OutputEmitted, received: CompletionKind::WorkerReset }, "machine.effect.wrong_completion")]
#[case(&MachineError::CleanupReservationMismatch { id: EffectId(1), expected: vec![], received: vec![] }, "machine.cleanup.reservation_mismatch")]
#[case(&MachineError::SelectedCandidateMissing("missing".into()), "machine.candidate.missing")]
#[case(&MachineError::ProcessWorkerMismatch { expected: Some(0), received: None }, "machine.worker.process_mismatch")]
fn machine_error_codes_identify_their_precise_failure(
    #[case] error: &MachineError,
    #[case] code: &str,
) {
    assert_eq!(error.code(), code);
}

#[test]
fn selected_analysis_without_a_spool_advances_to_the_next_target() {
    let mut state = exhausted_effect_fixture();
    state.next_effect_id = 1;
    state.selection = RunSelection::explicit(
        ["m1".into()],
        crate::VerificationSelection::candidate_ids(1, 1, false).unwrap(),
    )
    .unwrap();
    let effects = state
        .apply_analysis_finished(AnalysisFinished {
            id: EffectId(0),
            spool: None,
            truncated: false,
            diagnostics: vec![],
        })
        .unwrap();
    assert!(
        matches!(&effects[..], [RunEffect::AnalyzeFile(analyze)] if analyze.target.path == "calc.py")
    );
}

#[test]
fn ordered_collection_ignores_an_unselected_candidate() {
    let (mut state, _) = ordered_collection_fixture();
    state.pending.clear();
    let mut ignored = candidate();
    ignored.id = "unselected".into();
    let effects = state.collect_ordered_candidate(0, Some(ignored)).unwrap();
    assert!(matches!(&effects[..], [RunEffect::ReadCandidate(_)]));
    assert_eq!(state.selection.missing_id().map(String::as_str), Some("m1"));
}

#[test]
fn ordered_collection_waits_when_every_worker_is_busy() {
    let (mut state, _) = ordered_collection_fixture();
    state.workers.insert(0, WorkerState::Resetting);
    assert_eq!(state.schedule_read_or_finalize().unwrap(), []);
}

#[test]
fn analysis_diagnostics_emit_their_original_code_and_message() {
    let mut state = state();
    state
        .analysis_diagnostics
        .push_back(crate::AnalysisDiagnostic {
            code: "analysis.warning".into(),
            message: "specific warning".into(),
        });
    let effects = state.analysis_diagnostic_effects().unwrap();
    assert!(
        matches!(&effects[..], [RunEffect::EmitOutput(output)] if matches!(&output.event, OutputEvent::Diagnostic(diagnostic) if diagnostic.code == "analysis.warning" && diagnostic.message == "specific warning"))
    );
}

#[test]
fn pending_analysis_resumes_after_its_last_diagnostic() {
    let mut state = exhausted_effect_fixture();
    state.next_effect_id = 1;
    state.analysis_diagnostics.clear();
    state.pending_analysis = Some(AnalysisFinished {
        id: EffectId(0),
        spool: None,
        truncated: false,
        diagnostics: vec![],
    });
    let effects = state.analysis_diagnostic_effects().unwrap();
    assert!(matches!(&effects[..], [RunEffect::AnalyzeFile(_)]));
    assert!(state.pending_analysis.is_none());
}

#[test]
fn stopping_with_a_copy_grant_verifies_originals_before_cleanup() {
    let mut state = state();
    reserve_single_worker_fixture_workspace(&mut state);
    state.flags.report.report_started = true;
    let effects = state.begin_stop().unwrap();
    assert!(
        matches!(&effects[..], [RunEffect::VerifyOriginals(verify)] if verify.checkpoint == IntegrityCheckpoint::PreFinalReport)
    );
    assert_eq!(state.phase, RunPhase::Finalize);
}

#[test]
fn draining_finished_results_preserves_candidate_sequence_order() {
    let mut state = state();
    let first = result_fixture();
    let mut second = result_fixture();
    second.candidate.id = "m2".into();
    second.candidate.sequence = 2;
    state.workers.insert(0, WorkerState::Finishing(second));
    state.workers.insert(1, WorkerState::Persisting(first));
    let effects = state.begin_stopped_mutant_drain().unwrap();
    assert!(
        matches!(&effects[..], [RunEffect::EmitOutput(output)] if matches!(&output.event, OutputEvent::MutantFinished(result) if result.candidate.id == "m1"))
    );
}

#[test]
fn preflight_builds_a_request_with_its_allocated_identifier() {
    let mut state = state();
    let effects = state.preflight_effects().unwrap();
    state.register(&effects).unwrap();
    assert_eq!(
        effects,
        [RunEffect::Preflight(Preflight { id: EffectId(1) })]
    );
    assert!(state.is_effect_pending(EffectId(1)));
}

#[test]
fn ordered_end_of_stream_requires_every_selected_candidate() {
    let (mut state, _) = ordered_collection_fixture();
    assert_eq!(
        state.collect_ordered_candidate(0, None),
        Err(MachineError::SelectedCandidateMissing("m1".into()))
    );
}

#[test]
fn stopped_synthetic_finished_candidate_retains_its_outcome() {
    let mut state = state();
    state.workers.insert(0, WorkerState::Idle);
    state
        .stopped_candidates
        .push_back(StoppedCandidate::SyntheticFinished(
            candidate(),
            SyntheticOutcome::NotRun,
        ));
    let effects = state.next_stopped_candidate_effects().unwrap();
    assert!(
        matches!(&effects[..], [RunEffect::EmitOutput(output)] if matches!(&output.event, OutputEvent::MutantFinished(result) if result.outcome == MutationOutcome::NotRun))
    );
}

#[rstest::rstest]
#[case(&MachineError::CandidateIdentityMismatch { worker: 0, expected: "m1".into(), received: None }, "machine.candidate.identity_mismatch")]
#[case(&MachineError::SessionIdentityMismatch { worker: 0, field: "run_id", expected: "saved".into(), received: "other".into() }, "machine.session.identity_mismatch")]
fn identity_error_codes_identify_the_mismatched_owner(
    #[case] error: &MachineError,
    #[case] code: &str,
) {
    assert_eq!(error.code(), code);
}

#[rstest::rstest]
#[case(1, WorkerState::Idle, Some(CompletionKind::CandidateLoaded))]
#[case(0, WorkerState::Idle, Some(CompletionKind::OriginalsVerified))]
#[case(0, WorkerState::Resetting, None)]
fn ready_ordered_scheduler_observes_remaining_work_and_worker_availability(
    #[case] remaining: usize,
    #[case] worker: WorkerState,
    #[case] expected: Option<CompletionKind>,
) {
    let (mut state, _) = ordered_collection_fixture();
    state.pending.clear();
    state.workers.insert(0, worker);
    state.selection.ordered_state_mut().unwrap().phase = OrderedCandidatePhase::Ready {
        remaining: std::iter::repeat_n("m1".into(), remaining).collect(),
    };
    let effects = state.schedule_read_or_finalize().unwrap();
    assert_eq!(effects.first().map(expected_completion), expected);
    assert_eq!(effects.len(), usize::from(expected.is_some()));
}

#[rstest::rstest]
#[case(false, false, 0, CompletionKind::OriginalsVerified)]
#[case(false, false, 1, CompletionKind::CandidateLoaded)]
#[case(false, true, 1, CompletionKind::OriginalsVerified)]
#[case(true, false, 0, CompletionKind::OriginalsVerified)]
#[case(true, false, 1, CompletionKind::CandidateLoaded)]
fn analysis_spool_completion_dispatches_or_finalizes_its_selected_work(
    #[case] selected: bool,
    #[case] truncated: bool,
    #[case] records: u64,
    #[case] expected: CompletionKind,
) {
    let mut state = exhausted_effect_fixture();
    state.next_effect_id = 1;
    state.targets.clear();
    state.workers.insert(0, WorkerState::Idle);
    let selection = RunSelection::explicit(
        ["m1".into()],
        crate::VerificationSelection::candidate_ids(1, 1, false).unwrap(),
    )
    .unwrap();
    state.selection = [RunSelection::all(), selection][usize::from(selected)].clone();
    state.selection.select_streamed("m1");
    let effects = state
        .apply_analysis_finished(AnalysisFinished {
            id: EffectId(0),
            spool: Some(crate::CandidateSpoolRef {
                token: "ready".into(),
                records,
            }),
            truncated,
            diagnostics: vec![],
        })
        .unwrap();
    assert_eq!(expected_completion(&effects[0]), expected);
    assert_eq!(state.candidate_spool.as_ref().unwrap().records, records);
}

#[test]
fn accepting_an_unallocated_effect_reports_unknown_identifier() {
    let mut state = state();
    assert_eq!(
        state.accept_completion(&RunEvent::OutputEmitted(crate::OutputEmitted {
            id: EffectId(1)
        })),
        Err(MachineError::UnknownEffect(EffectId(1)))
    );
}

#[test]
fn accepting_a_wrong_completion_preserves_the_pending_request() {
    let (mut state, id) = pending_state(RunPhase::Preflight, CompletionKind::TargetsResolved);
    assert_eq!(
        state.accept_completion(&RunEvent::OutputEmitted(crate::OutputEmitted { id })),
        Err(MachineError::WrongCompletion {
            id,
            expected: CompletionKind::TargetsResolved,
            received: CompletionKind::OutputEmitted
        })
    );
    assert!(state.is_effect_pending(id));
}

#[test]
fn accepting_cleanup_requires_the_requested_reservations() {
    let (mut state, id) = pending_state(RunPhase::Cleaning, CompletionKind::CleanupFinished);
    state.pending.get_mut(&id).unwrap().cleanup_reservations = Some(vec![crate::ReservationId(1)]);
    assert_eq!(
        state.accept_completion(&RunEvent::CleanupFinished(crate::CleanupFinished {
            id,
            released_reservations: vec![]
        })),
        Err(MachineError::CleanupReservationMismatch {
            id,
            expected: vec![crate::ReservationId(1)],
            received: vec![]
        })
    );
    assert!(state.is_effect_pending(id));
}

#[test]
fn out_of_order_completion_advances_the_floor_when_the_gap_closes() {
    let mut state = state();
    let effects = [
        RunEffect::Preflight(Preflight {
            id: state.allocate_id().unwrap(),
        }),
        RunEffect::Preflight(Preflight {
            id: state.allocate_id().unwrap(),
        }),
    ];
    state.register(&effects).unwrap();
    state
        .accept_completion(&RunEvent::EffectFailed(EffectFailed::other(
            EffectId(2),
            "test.io",
            "second",
        )))
        .unwrap();
    assert_eq!(state.completed_gaps, [EffectId(2)].into());
    state
        .accept_completion(&RunEvent::EffectFailed(EffectFailed::other(
            EffectId(1),
            "test.io",
            "first",
        )))
        .unwrap();
    assert_eq!(state.completed_floor, 2);
    assert!(state.completed_gaps.is_empty());
}

#[test]
fn empty_selected_spool_requires_discovery_of_the_requested_candidate() {
    let mut state = state();
    state.selection = RunSelection::explicit(
        ["missing".into()],
        crate::VerificationSelection::candidate_ids(1, 1, false).unwrap(),
    )
    .unwrap();
    assert_eq!(
        state.apply_analysis_finished(AnalysisFinished {
            id: EffectId(0),
            spool: Some(crate::CandidateSpoolRef {
                token: "empty".into(),
                records: 0
            }),
            truncated: false,
            diagnostics: vec![]
        }),
        Err(MachineError::SelectedCandidateMissing("missing".into()))
    );
}
