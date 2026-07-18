use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use crate::{
    AnalyzeFile, ApplyMutation, BaselineFinished as BaselineOutput, BeginSession, BudgetLedger,
    CandidateSpoolRef, Cleanup, Diagnostic, EffectFailed, EffectId, EmitOutput, ExitPolicy,
    FinishSession, IntegrityCheckpoint, LoadSession, LookupStoredResult,
    MutantFinished as MutantOutput, MutantResult, MutantStarted, MutantTimeout, MutationCandidate,
    MutationStatus, MutationSummary, OutputEvent, PersistResult, Preflight, ProcessFinished,
    ProcessLimits, ProcessTermination, ReadCandidate, ResetWorker, ResolveTargets, ResumeDecision,
    RunBudgets, RunConfig, RunEffect, RunEvent, RunFingerprint, RunProcess, RunStarted, RunSummary,
    TargetSlice, VerifyOriginals, WorkspaceCopyGrant, auto_mutant_timeout, classify_mutant,
    contract_ensure, exit_code_for, release_workspace_copy, reserve_workspace_copy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunPhase {
    Validate,
    Preflight,
    Copy,
    Baseline,
    Analyze,
    Mutants,
    Finalize,
    Cleaning,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionKind {
    TargetsResolved,
    PreflightCompleted,
    WorkerCreated,
    BaselineFinished,
    AnalysisFinished,
    CandidateLoaded,
    MutationApplied,
    MutantFinished,
    WorkerReset,
    OriginalsVerified,
    SessionLoaded,
    StoredResultLoaded,
    SessionStarted,
    ResultPersisted,
    SessionFinished,
    OutputEmitted,
    CleanupFinished,
}

#[derive(Clone, Debug)]
pub struct RunState {
    run_id: String,
    config: RunConfig,
    phase: RunPhase,
    next_effect_id: u64,
    pending: BTreeMap<EffectId, CompletionKind>,
    completed_floor: u64,
    completed_gaps: BTreeSet<EffectId>,
    targets: VecDeque<TargetSlice>,
    budgets: BudgetLedger,
    copy_grant: Option<WorkspaceCopyGrant>,
    candidate_spool: Option<CandidateSpoolRef>,
    candidate_offset: u64,
    active_candidate: Option<MutationCandidate>,
    finished_output_id: Option<EffectId>,
    not_run_output_id: Option<EffectId>,
    run_finished_output_id: Option<EffectId>,
    mutant_limit_reached: bool,
    baseline_elapsed: std::time::Duration,
    summary: MutationSummary,
    output_sequence: u64,
    baseline_failed: bool,
    infrastructure_error: bool,
    incomplete: bool,
    interrupted: bool,
    fingerprint: Option<RunFingerprint>,
    session_run_id: Option<String>,
    active_result: Option<MutantResult>,
    reuse_output_id: Option<EffectId>,
    diagnostic_output_id: Option<EffectId>,
    run_started_output_id: Option<EffectId>,
    report_started: bool,
    pending_failure: Option<EffectFailed>,
    stop_after_run_started: bool,
    cleanup_done: bool,
    session_finish_attempted: bool,
}

impl RunState {
    pub fn new(run_id: impl Into<String>, config: RunConfig) -> Self {
        let budgets = RunBudgets {
            memory: config.limits.max_memory.get(),
            copy: config.limits.max_copy_size.get(),
            processes: config.limits.max_processes.get() as u64,
        };
        Self {
            run_id: run_id.into(),
            config,
            phase: RunPhase::Validate,
            next_effect_id: 1,
            pending: BTreeMap::new(),
            completed_floor: 0,
            completed_gaps: BTreeSet::new(),
            targets: VecDeque::new(),
            budgets: BudgetLedger::new(budgets),
            copy_grant: None,
            candidate_spool: None,
            candidate_offset: 0,
            active_candidate: None,
            finished_output_id: None,
            not_run_output_id: None,
            run_finished_output_id: None,
            mutant_limit_reached: false,
            baseline_elapsed: std::time::Duration::ZERO,
            summary: MutationSummary::default(),
            output_sequence: 0,
            baseline_failed: false,
            infrastructure_error: false,
            incomplete: false,
            interrupted: false,
            fingerprint: None,
            session_run_id: None,
            active_result: None,
            reuse_output_id: None,
            diagnostic_output_id: None,
            run_started_output_id: None,
            report_started: false,
            pending_failure: None,
            stop_after_run_started: false,
            cleanup_done: false,
            session_finish_attempted: false,
        }
    }

    pub fn with_fingerprint(
        run_id: impl Into<String>,
        config: RunConfig,
        fingerprint: RunFingerprint,
    ) -> Self {
        let mut state = Self::new(run_id, config);
        state.fingerprint = Some(fingerprint);
        state
    }

    pub fn phase(&self) -> RunPhase {
        self.phase
    }

    pub fn summary(&self) -> &MutationSummary {
        &self.summary
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn candidate_offset(&self) -> u64 {
        self.candidate_offset
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn is_effect_pending(&self, id: EffectId) -> bool {
        self.pending.contains_key(&id)
    }

    pub fn completion_ledger_entries(&self) -> usize {
        self.completed_gaps.len()
    }

    pub fn exit_code(&self) -> i32 {
        let summary_policy = ExitPolicy::from_summary(&self.summary);
        exit_code_for(ExitPolicy {
            infrastructure_error: self.infrastructure_error || summary_policy.infrastructure_error,
            baseline_failed: self.baseline_failed,
            incomplete: self.incomplete || summary_policy.incomplete,
            survivors: summary_policy.survivors,
            interrupted: self.interrupted,
        })
    }

    fn allocate_id(&mut self) -> Result<EffectId, MachineError> {
        let id = EffectId(self.next_effect_id);
        self.next_effect_id = self
            .next_effect_id
            .checked_add(1)
            .ok_or(MachineError::EffectIdOverflow)?;
        Ok(id)
    }

    fn output_sequence(&mut self) -> u64 {
        self.output_sequence = self.output_sequence.saturating_add(1);
        self.output_sequence
    }

    fn accept_completion(&mut self, event: &RunEvent) -> Result<(), MachineError> {
        let Some((id, received)) = completion(event) else {
            return Ok(());
        };
        if id.0 <= self.completed_floor || self.completed_gaps.contains(&id) {
            return Err(MachineError::DuplicateEffect(id));
        }
        let expected = match self.pending.get(&id).copied() {
            Some(expected) => expected,
            None if id.0 >= self.next_effect_id => return Err(MachineError::UnknownEffect(id)),
            None => return Err(MachineError::EffectNotPending(id)),
        };
        if !matches!(event, RunEvent::EffectFailed(_)) && expected != received {
            return Err(MachineError::WrongCompletion {
                id,
                expected,
                received,
            });
        }
        self.pending.remove(&id);
        if id.0 == self.completed_floor.saturating_add(1) {
            self.completed_floor = id.0;
            while self
                .completed_gaps
                .remove(&EffectId(self.completed_floor.saturating_add(1)))
            {
                self.completed_floor = self.completed_floor.saturating_add(1);
            }
        } else {
            self.completed_gaps.insert(id);
        }
        Ok(())
    }

    fn register(&mut self, effects: &[RunEffect]) -> Result<(), MachineError> {
        for effect in effects {
            let id = effect.id();
            if self.pending.contains_key(&id)
                || id.0 <= self.completed_floor
                || self.completed_gaps.contains(&id)
            {
                return Err(MachineError::DuplicateEffect(id));
            }
            self.pending.insert(id, expected_completion(effect));
        }
        contract_ensure!(
            "machine.pending.invariant",
            self.pending.len() == self.pending.keys().collect::<BTreeSet<_>>().len(),
            (&self.pending, self.completed_floor, &self.completed_gaps)
        );
        Ok(())
    }

    fn retire_pending(&mut self) {
        self.pending.clear();
        self.completed_floor = self.next_effect_id.saturating_sub(1);
        self.completed_gaps.clear();
    }

    fn diagnostic_effect(&mut self, failed: &EffectFailed) -> Result<Vec<RunEffect>, MachineError> {
        self.phase = RunPhase::Finalize;
        let id = self.allocate_id()?;
        self.diagnostic_output_id = Some(id);
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::Diagnostic(Diagnostic::new(
                self.run_id.clone(),
                self.output_sequence(),
                "error",
                failed.failure.code(),
                failed.failure.message(),
            )),
        })])
    }

    fn analyze_next(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let Some(target) = self.targets.pop_front() else {
            self.phase = RunPhase::Finalize;
            return self.finalize_effects();
        };
        let final_target = self.targets.is_empty();
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::AnalyzeFile(AnalyzeFile {
            id,
            target,
            final_target,
            max_candidates: self.config.limits.max_candidates.get() as u64,
        })])
    }

    fn preflight_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::Preflight(Preflight { id })])
    }

    fn create_worker_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        let create = self
            .copy_grant
            .ok_or(MachineError::WrongPhase { phase: self.phase })?
            .create_worker(id, 0)
            .map_err(|error| MachineError::Budget(error.to_string()))?;
        self.phase = RunPhase::Copy;
        Ok(vec![RunEffect::CreateWorker(create)])
    }

    fn begin_session_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let fingerprint = self.fingerprint.ok_or(MachineError::MissingFingerprint)?;
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::BeginSession(BeginSession {
            id,
            run_id: self.run_id.clone(),
            fingerprint,
        })])
    }

    fn start_run_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        self.run_started_output_id = Some(id);
        let sequence = self.output_sequence();
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::RunStarted(RunStarted::minimal(self.run_id.clone(), sequence)),
        })])
    }

    fn apply_active_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::ApplyMutation(ApplyMutation {
            id,
            worker: 0,
        })])
    }

    fn candidate_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        if let Some(run_id) = self.session_run_id.clone() {
            let mutant_id = self
                .active_candidate
                .as_ref()
                .ok_or(MachineError::WrongPhase { phase: self.phase })?
                .id
                .clone();
            let id = self.allocate_id()?;
            Ok(vec![RunEffect::LookupStoredResult(LookupStoredResult {
                id,
                run_id,
                mutant_id,
            })])
        } else {
            self.apply_active_effects()
        }
    }

    fn finalize_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::VerifyOriginals(VerifyOriginals {
            id,
            checkpoint: IntegrityCheckpoint::PreFinalReport,
        })])
    }

    fn read_next_candidate(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let spool = self
            .candidate_spool
            .clone()
            .ok_or(MachineError::WrongPhase { phase: self.phase })?;
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::ReadCandidate(ReadCandidate {
            id,
            spool,
            offset: self.candidate_offset,
        })])
    }

    fn cleanup_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        self.phase = RunPhase::Cleaning;
        let id = self.allocate_id()?;
        let reservations = self
            .copy_grant
            .as_ref()
            .map(|grant| vec![grant.reservation_id()])
            .unwrap_or_default();
        Ok(vec![RunEffect::Cleanup(Cleanup { id, reservations })])
    }

    fn final_report_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        self.phase = RunPhase::Finalize;
        let id = self.allocate_id()?;
        self.run_finished_output_id = Some(id);
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::RunFinished(RunSummary {
                schema_version: crate::REPORT_SCHEMA_VERSION,
                sequence: self.output_sequence(),
                run_id: self.run_id.clone(),
                counts: self.summary.clone(),
                complete: !self.incomplete && !self.infrastructure_error && !self.interrupted,
                exit_code: self.exit_code(),
            }),
        })])
    }

    fn post_cleanup_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        self.phase = RunPhase::Finalize;
        if let Some(run_id) = self.session_run_id.clone()
            && !self.session_finish_attempted
        {
            self.session_finish_attempted = true;
            let id = self.allocate_id()?;
            Ok(vec![RunEffect::FinishSession(FinishSession {
                id,
                run_id,
                complete: !self.incomplete && !self.infrastructure_error && !self.interrupted,
            })])
        } else {
            self.final_report_effects()
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MachineError {
    #[error("unknown effect completion {0:?}")]
    UnknownEffect(EffectId),
    #[error("duplicate effect completion {0:?}")]
    DuplicateEffect(EffectId),
    #[error("effect completion {0:?} is allocated but no longer pending")]
    EffectNotPending(EffectId),
    #[error("effect {id:?} completed as {received:?}, expected {expected:?}")]
    WrongCompletion {
        id: EffectId,
        expected: CompletionKind,
        received: CompletionKind,
    },
    #[error("event is invalid in phase {phase:?}")]
    WrongPhase { phase: RunPhase },
    #[error("effect ID overflow")]
    EffectIdOverflow,
    #[error("workspace budget rejected preflight: {0}")]
    Budget(String),
    #[error("session configuration requires a run fingerprint")]
    MissingFingerprint,
}

impl MachineError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownEffect(_) => "machine.effect.unknown",
            Self::DuplicateEffect(_) => "machine.effect.duplicate",
            Self::EffectNotPending(_) => "machine.effect.not_pending",
            Self::WrongCompletion { .. } => "machine.effect.wrong_completion",
            Self::WrongPhase { .. } => "machine.phase.invalid",
            Self::EffectIdOverflow => "machine.effect_id.overflow",
            Self::Budget(_) => "machine.budget",
            Self::MissingFingerprint => "machine.fingerprint.missing",
        }
    }
}

pub fn transition(
    mut state: RunState,
    event: RunEvent,
) -> Result<(RunState, Vec<RunEffect>), MachineError> {
    let _was_fatal = state.infrastructure_error;
    state.accept_completion(&event)?;
    let effects = match event {
        RunEvent::StartRequested(_) if state.phase == RunPhase::Validate => {
            state.phase = RunPhase::Preflight;
            let resolve_id = state.allocate_id()?;
            vec![RunEffect::ResolveTargets(ResolveTargets {
                id: resolve_id,
                selection: state.config.selection.clone(),
            })]
        }
        RunEvent::TargetsResolved(value) if state.phase == RunPhase::Preflight => {
            state.targets = value.targets.into();
            state.preflight_effects()?
        }
        RunEvent::SessionLoaded(value) if state.phase == RunPhase::Preflight => {
            if let Some(resume) = value.resume {
                state.run_id.clone_from(&resume.run_id);
                state.session_run_id = Some(resume.run_id);
                state.start_run_effects()?
            } else {
                state.begin_session_effects()?
            }
        }
        RunEvent::SessionStarted(value) if state.phase == RunPhase::Preflight => {
            state.run_id.clone_from(&value.run_id);
            state.session_run_id = Some(value.run_id);
            state.start_run_effects()?
        }
        RunEvent::PreflightCompleted(value) if state.phase == RunPhase::Preflight => {
            state.fingerprint = value.fingerprint;
            let grant = reserve_workspace_copy(&mut state.budgets, &value)
                .map_err(|error| MachineError::Budget(error.to_string()))?;
            state.copy_grant = Some(grant);
            if state.config.session.is_some() {
                let fingerprint = state.fingerprint.ok_or(MachineError::MissingFingerprint)?;
                if state.config.resume {
                    let id = state.allocate_id()?;
                    vec![RunEffect::LoadSession(LoadSession { id, fingerprint })]
                } else {
                    state.begin_session_effects()?
                }
            } else {
                state.start_run_effects()?
            }
        }
        RunEvent::OutputEmitted(value) if state.run_started_output_id == Some(value.id) => {
            state.run_started_output_id = None;
            state.report_started = true;
            if let Some(failed) = state.pending_failure.take() {
                state.diagnostic_effect(&failed)?
            } else if state.stop_after_run_started {
                state.stop_after_run_started = false;
                state.phase = RunPhase::Finalize;
                if state.copy_grant.is_some() {
                    state.finalize_effects()?
                } else {
                    state.cleanup_effects()?
                }
            } else {
                state.create_worker_effects()?
            }
        }
        RunEvent::WorkerCreated(_) if state.phase == RunPhase::Copy => {
            state.phase = RunPhase::Baseline;
            let id = state.allocate_id()?;
            vec![RunEffect::RunBaseline(RunProcess {
                id,
                argv: state.config.test_argv.clone(),
                cwd: state.config.root.clone(),
                limits: ProcessLimits {
                    timeout: state.config.limits.baseline_timeout.get(),
                    max_output_bytes: state.config.limits.max_output.get(),
                    max_memory_bytes: state.config.limits.max_memory.get(),
                    max_processes: state.config.limits.max_processes.get() as u32,
                },
            })]
        }
        RunEvent::BaselineFinished(value) if state.phase == RunPhase::Baseline => {
            let success = value.termination == ProcessTermination::Exit(0);
            state.baseline_elapsed = value.elapsed;
            let output = baseline_output(&mut state, &value);
            let output_id = state.allocate_id()?;
            let mut effects = vec![RunEffect::EmitOutput(EmitOutput {
                id: output_id,
                event: output,
            })];
            if success {
                state.phase = RunPhase::Analyze;
                effects.extend(state.analyze_next()?);
            } else {
                state.baseline_failed = true;
                state.phase = RunPhase::Finalize;
                effects.extend(state.finalize_effects()?);
            }
            effects
        }
        RunEvent::AnalysisFinished(value) if state.phase == RunPhase::Analyze => {
            state.incomplete |= value.truncated;
            if value.truncated {
                state.phase = RunPhase::Finalize;
                state.finalize_effects()?
            } else {
                match value.spool {
                    None => state.analyze_next()?,
                    Some(spool) if spool.records == 0 => {
                        state.candidate_spool = Some(spool);
                        state.phase = RunPhase::Finalize;
                        state.finalize_effects()?
                    }
                    Some(spool) => {
                        state.phase = RunPhase::Mutants;
                        state.candidate_spool = Some(spool.clone());
                        let id = state.allocate_id()?;
                        vec![RunEffect::ReadCandidate(ReadCandidate {
                            id,
                            spool,
                            offset: state.candidate_offset,
                        })]
                    }
                }
            }
        }
        RunEvent::CandidateLoaded(value) if state.phase == RunPhase::Mutants => {
            state.candidate_offset = value.next_offset;
            match value.candidate {
                Some(candidate) => {
                    contract_ensure!(
                        "machine.active_candidate.pre",
                        state.active_candidate.is_none(),
                        (&state.active_candidate, &candidate)
                    );
                    state.active_candidate = Some(candidate);
                    if state.mutant_limit_reached {
                        state.incomplete = true;
                        state.summary.record(MutationStatus::NotRun);
                        let started_id = state.allocate_id()?;
                        let finished_id = state.allocate_id()?;
                        state.not_run_output_id = Some(finished_id);
                        let candidate = state.active_candidate.as_ref().expect("set above").clone();
                        vec![
                            RunEffect::EmitOutput(EmitOutput {
                                id: started_id,
                                event: OutputEvent::MutantStarted(MutantStarted::new(
                                    state.run_id.clone(),
                                    state.output_sequence(),
                                    candidate.id.clone(),
                                    candidate.sequence,
                                )),
                            }),
                            RunEffect::EmitOutput(EmitOutput {
                                id: finished_id,
                                event: OutputEvent::MutantFinished(MutantOutput {
                                    schema_version: crate::REPORT_SCHEMA_VERSION,
                                    sequence: state.output_sequence(),
                                    run_id: state.run_id.clone(),
                                    candidate,
                                    status: MutationStatus::NotRun,
                                    termination: None,
                                    elapsed_ms: 0,
                                    resource_mode: crate::ResourceMode::Hard,
                                    output: None,
                                }),
                            }),
                        ]
                    } else {
                        state.candidate_effects()?
                    }
                }
                None => {
                    state.phase = RunPhase::Finalize;
                    state.finalize_effects()?
                }
            }
        }
        RunEvent::StoredResultLoaded(value) if state.phase == RunPhase::Mutants => {
            match crate::resume_policy(value.result.as_ref()) {
                ResumeDecision::Rerun => state.apply_active_effects()?,
                ResumeDecision::Reuse => {
                    let candidate = state
                        .active_candidate
                        .as_ref()
                        .ok_or(MachineError::WrongPhase { phase: state.phase })?
                        .clone();
                    let status = value.result.expect("reuse requires a stored result").status;
                    state.summary.record(status);
                    let started_id = state.allocate_id()?;
                    let finished_id = state.allocate_id()?;
                    state.reuse_output_id = Some(finished_id);
                    vec![
                        RunEffect::EmitOutput(EmitOutput {
                            id: started_id,
                            event: OutputEvent::MutantStarted(MutantStarted::new(
                                state.run_id.clone(),
                                state.output_sequence(),
                                candidate.id.clone(),
                                candidate.sequence,
                            )),
                        }),
                        RunEffect::EmitOutput(EmitOutput {
                            id: finished_id,
                            event: OutputEvent::MutantFinished(MutantOutput {
                                schema_version: crate::REPORT_SCHEMA_VERSION,
                                sequence: state.output_sequence(),
                                run_id: state.run_id.clone(),
                                candidate,
                                status,
                                termination: None,
                                elapsed_ms: 0,
                                resource_mode: crate::ResourceMode::Hard,
                                output: None,
                            }),
                        }),
                    ]
                }
            }
        }
        RunEvent::MutationApplied(_) if state.phase == RunPhase::Mutants => {
            let candidate = state
                .active_candidate
                .as_ref()
                .ok_or(MachineError::WrongPhase { phase: state.phase })?
                .clone();
            let output_id = state.allocate_id()?;
            let run_id = state.allocate_id()?;
            let timeout = match state.config.limits.mutant_timeout {
                MutantTimeout::Auto => auto_mutant_timeout(state.baseline_elapsed),
                MutantTimeout::Fixed(value) => value.get(),
            };
            vec![
                RunEffect::EmitOutput(EmitOutput {
                    id: output_id,
                    event: OutputEvent::MutantStarted(MutantStarted::new(
                        state.run_id.clone(),
                        state.output_sequence(),
                        candidate.id.clone(),
                        candidate.sequence,
                    )),
                }),
                RunEffect::RunMutant(RunProcess {
                    id: run_id,
                    argv: state.config.test_argv.clone(),
                    cwd: state.config.root.clone(),
                    limits: ProcessLimits {
                        timeout,
                        max_output_bytes: state.config.limits.max_output.get(),
                        max_memory_bytes: state.config.limits.max_memory.get(),
                        max_processes: state.config.limits.max_processes.get() as u32,
                    },
                }),
            ]
        }
        RunEvent::MutantFinished(value) if state.phase == RunPhase::Mutants => {
            let candidate = state
                .active_candidate
                .as_ref()
                .ok_or(MachineError::WrongPhase { phase: state.phase })?
                .clone();
            let status = classify_mutant(value.termination);
            let result = MutantResult {
                run_id: state
                    .session_run_id
                    .clone()
                    .unwrap_or_else(|| state.run_id.clone()),
                candidate,
                status,
                elapsed: value.elapsed,
                resource_mode: value.resource_mode,
                output: Some(value.output),
                diagnostics: Vec::new(),
            };
            if state.session_run_id.is_some() {
                let id = state.allocate_id()?;
                state.active_result = Some(result.clone());
                vec![RunEffect::PersistResult(PersistResult { id, result })]
            } else {
                state.summary.record(status);
                let id = state.allocate_id()?;
                state.finished_output_id = Some(id);
                vec![RunEffect::EmitOutput(EmitOutput {
                    id,
                    event: OutputEvent::MutantFinished(MutantOutput {
                        schema_version: crate::REPORT_SCHEMA_VERSION,
                        sequence: state.output_sequence(),
                        run_id: state.run_id.clone(),
                        candidate: result.candidate,
                        status: result.status,
                        termination: Some(value.termination),
                        elapsed_ms: result.elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
                        resource_mode: result.resource_mode,
                        output: result.output,
                    }),
                })]
            }
        }
        RunEvent::ResultPersisted(_) if state.phase == RunPhase::Mutants => {
            let result = state
                .active_result
                .take()
                .ok_or(MachineError::WrongPhase { phase: state.phase })?;
            state.summary.record(result.status);
            let id = state.allocate_id()?;
            state.finished_output_id = Some(id);
            vec![RunEffect::EmitOutput(EmitOutput {
                id,
                event: OutputEvent::MutantFinished(MutantOutput {
                    schema_version: crate::REPORT_SCHEMA_VERSION,
                    sequence: state.output_sequence(),
                    run_id: state.run_id.clone(),
                    candidate: result.candidate,
                    status: result.status,
                    termination: None,
                    elapsed_ms: result.elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
                    resource_mode: result.resource_mode,
                    output: result.output,
                }),
            })]
        }
        RunEvent::OutputEmitted(value) if state.finished_output_id == Some(value.id) => {
            state.finished_output_id = None;
            let id = state.allocate_id()?;
            vec![RunEffect::ResetWorker(ResetWorker { id, worker: 0 })]
        }
        RunEvent::OutputEmitted(value) if state.not_run_output_id == Some(value.id) => {
            state.not_run_output_id = None;
            state.active_candidate = None;
            state.read_next_candidate()?
        }
        RunEvent::OutputEmitted(value) if state.reuse_output_id == Some(value.id) => {
            state.reuse_output_id = None;
            state.active_candidate = None;
            state.read_next_candidate()?
        }
        RunEvent::WorkerReset(_) if state.phase == RunPhase::Mutants => {
            state.active_candidate = None;
            if state.summary.killed
                + state.summary.survived
                + state.summary.timeout
                + state.summary.out_of_memory
                + state.summary.process_limit
                + state.summary.error
                >= state.config.limits.max_mutants.get() as u64
            {
                state.mutant_limit_reached = true;
            }
            state.read_next_candidate()?
        }
        RunEvent::OriginalsVerified(_) if state.phase == RunPhase::Finalize => {
            state.cleanup_effects()?
        }
        RunEvent::SessionFinished(_) if state.phase == RunPhase::Finalize => {
            state.final_report_effects()?
        }
        RunEvent::OutputEmitted(value) if state.run_finished_output_id == Some(value.id) => {
            state.run_finished_output_id = None;
            state.phase = RunPhase::Finished;
            Vec::new()
        }
        RunEvent::OutputEmitted(value) if state.diagnostic_output_id == Some(value.id) => {
            state.diagnostic_output_id = None;
            if state.cleanup_done {
                state.post_cleanup_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        RunEvent::CleanupFinished(value) if state.phase == RunPhase::Cleaning => {
            release_workspace_copy(&mut state.budgets, &value)
                .map_err(|error| MachineError::Budget(error.to_string()))?;
            state.copy_grant = None;
            state.cleanup_done = true;
            state.post_cleanup_effects()?
        }
        RunEvent::OutputEmitted(_) => Vec::new(),
        RunEvent::EffectFailed(failed) if state.phase == RunPhase::Cleaning => {
            state.infrastructure_error = true;
            state.retire_pending();
            state.copy_grant = None;
            state.cleanup_done = true;
            state.diagnostic_effect(&failed)?
        }
        RunEvent::EffectFailed(failed) if state.diagnostic_output_id == Some(failed.id) => {
            state.infrastructure_error = true;
            state.retire_pending();
            state.diagnostic_output_id = None;
            if state.cleanup_done {
                state.post_cleanup_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        RunEvent::EffectFailed(failed) => {
            state.infrastructure_error = true;
            state.retire_pending();
            if !state.report_started {
                if state.run_started_output_id == Some(failed.id) {
                    state.run_started_output_id = None;
                    state.cleanup_effects()?
                } else {
                    state.pending_failure = Some(failed);
                    state.start_run_effects()?
                }
            } else if state.run_finished_output_id.is_some() {
                state.run_finished_output_id = None;
                state.phase = RunPhase::Finished;
                Vec::new()
            } else {
                state.diagnostic_effect(&failed)?
            }
        }
        RunEvent::DeadlineReached => {
            state.incomplete = true;
            state.retire_pending();
            if !state.report_started {
                state.stop_after_run_started = true;
                state.start_run_effects()?
            } else if state.copy_grant.is_some() {
                state.phase = RunPhase::Finalize;
                state.finalize_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        RunEvent::CancellationRequested => {
            state.interrupted = true;
            state.retire_pending();
            if !state.report_started {
                state.stop_after_run_started = true;
                state.start_run_effects()?
            } else if state.copy_grant.is_some() {
                state.phase = RunPhase::Finalize;
                state.finalize_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        _ => return Err(MachineError::WrongPhase { phase: state.phase }),
    };
    state.register(&effects)?;
    contract_ensure!(
        "machine.transition.post",
        effects
            .iter()
            .all(|effect| state.pending.contains_key(&effect.id())),
        (&state.phase, &effects, &state.pending)
    );
    contract_ensure!(
        "machine.worker.invariant",
        !matches!(
            state.phase,
            RunPhase::Baseline | RunPhase::Analyze | RunPhase::Mutants
        ) || state.copy_grant.is_some(),
        (&state.phase, &state.copy_grant)
    );
    contract_ensure!(
        "machine.fatal.post",
        !(_was_fatal || state.infrastructure_error)
            || !effects
                .iter()
                .any(|effect| matches!(effect, RunEffect::RunMutant(_))),
        (&state.phase, &effects)
    );
    Ok((state, effects))
}

fn baseline_output(state: &mut RunState, value: &ProcessFinished) -> OutputEvent {
    OutputEvent::BaselineFinished(BaselineOutput {
        schema_version: crate::REPORT_SCHEMA_VERSION,
        sequence: state.output_sequence(),
        run_id: state.run_id.clone(),
        termination: value.termination,
        elapsed_ms: value.elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        resource_mode: value.resource_mode,
        output: value.output.clone(),
    })
}

fn expected_completion(effect: &RunEffect) -> CompletionKind {
    match effect {
        RunEffect::ResolveTargets(_) => CompletionKind::TargetsResolved,
        RunEffect::Preflight(_) => CompletionKind::PreflightCompleted,
        RunEffect::CreateWorker(_) => CompletionKind::WorkerCreated,
        RunEffect::RunBaseline(_) => CompletionKind::BaselineFinished,
        RunEffect::AnalyzeFile(_) => CompletionKind::AnalysisFinished,
        RunEffect::ReadCandidate(_) => CompletionKind::CandidateLoaded,
        RunEffect::ApplyMutation(_) => CompletionKind::MutationApplied,
        RunEffect::RunMutant(_) => CompletionKind::MutantFinished,
        RunEffect::ResetWorker(_) => CompletionKind::WorkerReset,
        RunEffect::VerifyOriginals(_) => CompletionKind::OriginalsVerified,
        RunEffect::LoadSession(_) => CompletionKind::SessionLoaded,
        RunEffect::LookupStoredResult(_) => CompletionKind::StoredResultLoaded,
        RunEffect::BeginSession(_) => CompletionKind::SessionStarted,
        RunEffect::PersistResult(_) => CompletionKind::ResultPersisted,
        RunEffect::FinishSession(_) => CompletionKind::SessionFinished,
        RunEffect::EmitOutput(_) => CompletionKind::OutputEmitted,
        RunEffect::Cleanup(_) => CompletionKind::CleanupFinished,
    }
}

fn completion(event: &RunEvent) -> Option<(EffectId, CompletionKind)> {
    Some(match event {
        RunEvent::StartRequested(_)
        | RunEvent::DeadlineReached
        | RunEvent::CancellationRequested => return None,
        RunEvent::TargetsResolved(value) => (value.id, CompletionKind::TargetsResolved),
        RunEvent::PreflightCompleted(value) => (value.id, CompletionKind::PreflightCompleted),
        RunEvent::WorkerCreated(value) => (value.id, CompletionKind::WorkerCreated),
        RunEvent::BaselineFinished(value) => (value.id, CompletionKind::BaselineFinished),
        RunEvent::AnalysisFinished(value) => (value.id, CompletionKind::AnalysisFinished),
        RunEvent::CandidateLoaded(value) => (value.id, CompletionKind::CandidateLoaded),
        RunEvent::MutationApplied(value) => (value.id, CompletionKind::MutationApplied),
        RunEvent::MutantFinished(value) => (value.id, CompletionKind::MutantFinished),
        RunEvent::WorkerReset(value) => (value.id, CompletionKind::WorkerReset),
        RunEvent::OriginalsVerified(value) => (value.id, CompletionKind::OriginalsVerified),
        RunEvent::SessionLoaded(value) => (value.id, CompletionKind::SessionLoaded),
        RunEvent::StoredResultLoaded(value) => (value.id, CompletionKind::StoredResultLoaded),
        RunEvent::SessionStarted(value) => (value.id, CompletionKind::SessionStarted),
        RunEvent::ResultPersisted(value) => (value.id, CompletionKind::ResultPersisted),
        RunEvent::SessionFinished(value) => (value.id, CompletionKind::SessionFinished),
        RunEvent::OutputEmitted(value) => (value.id, CompletionKind::OutputEmitted),
        RunEvent::CleanupFinished(value) => (value.id, CompletionKind::CleanupFinished),
        RunEvent::EffectFailed(value) => {
            let expected = CompletionKind::OutputEmitted;
            (value.id, expected)
        }
    })
}
