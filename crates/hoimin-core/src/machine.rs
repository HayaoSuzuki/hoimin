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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingEffect {
    kind: CompletionKind,
    worker: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerPhase {
    Idle,
    Reading,
    Lookup,
    Applying,
    Starting,
    Running,
    Persisting,
    Finishing,
    Resetting,
    SyntheticStarting(MutationStatus),
    SyntheticFinishing(MutationStatus),
}

#[derive(Clone, Debug)]
struct WorkerState {
    phase: WorkerPhase,
    candidate: Option<MutationCandidate>,
    result: Option<MutantResult>,
}

impl Default for WorkerState {
    fn default() -> Self {
        Self {
            phase: WorkerPhase::Idle,
            candidate: None,
            result: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputAction {
    StartMutant(u32),
    StartSynthetic(u32, MutationStatus),
    FinishMutant(u32),
    FinishSynthetic(u32, MutationStatus),
}

#[derive(Clone, Debug)]
enum StoppedCandidate {
    NotStarted(MutationCandidate),
    SyntheticNotStarted(MutationCandidate, MutationStatus),
    StartedNotRun(MutationCandidate),
    Finished(MutantResult),
    SyntheticFinished(MutationCandidate, MutationStatus),
}

#[derive(Clone, Debug)]
struct RunFlags {
    scheduling: SchedulingFlags,
    outcome: OutcomeFlags,
    report: ReportFlags,
    cleanup: CleanupFlags,
}

#[derive(Clone, Debug)]
struct SchedulingFlags {
    candidate_exhausted: bool,
    mutant_limit_reached: bool,
    stop_requested: bool,
}

#[derive(Clone, Debug)]
struct OutcomeFlags {
    baseline_failed: bool,
    infrastructure_error: bool,
    incomplete: bool,
}

#[derive(Clone, Debug)]
struct ReportFlags {
    interrupted: bool,
    report_started: bool,
    stop_after_run_started: bool,
}

#[derive(Clone, Debug)]
struct CleanupFlags {
    cleanup_done: bool,
    session_finish_attempted: bool,
}

#[derive(Clone, Debug)]
pub struct RunState {
    run_id: String,
    config: RunConfig,
    phase: RunPhase,
    next_effect_id: u64,
    pending: BTreeMap<EffectId, PendingEffect>,
    completed_floor: u64,
    completed_gaps: BTreeSet<EffectId>,
    retired: BTreeSet<EffectId>,
    targets: VecDeque<TargetSlice>,
    budgets: BudgetLedger,
    copy_grant: Option<WorkspaceCopyGrant>,
    candidate_spool: Option<CandidateSpoolRef>,
    candidate_offset: u64,
    workers: BTreeMap<u32, WorkerState>,
    output_actions: BTreeMap<EffectId, OutputAction>,
    created_workers: BTreeSet<u32>,
    scheduled_mutants: u64,
    run_finished_output_id: Option<EffectId>,
    baseline_elapsed: std::time::Duration,
    summary: MutationSummary,
    output_sequence: u64,
    fingerprint: Option<RunFingerprint>,
    session_run_id: Option<String>,
    diagnostic_output_id: Option<EffectId>,
    run_started_output_id: Option<EffectId>,
    pending_failure: Option<EffectFailed>,
    flags: RunFlags,
    stopped_candidates: VecDeque<StoppedCandidate>,
    candidate_filter: Option<BTreeSet<String>>,
    ordered_candidate_ids: Option<Vec<String>>,
    ordered_candidates: BTreeMap<String, MutationCandidate>,
    ordered_candidates_ready: VecDeque<MutationCandidate>,
    ordered_collection_complete: bool,
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
            retired: BTreeSet::new(),
            targets: VecDeque::new(),
            budgets: BudgetLedger::new(budgets),
            copy_grant: None,
            candidate_spool: None,
            candidate_offset: 0,
            workers: BTreeMap::new(),
            output_actions: BTreeMap::new(),
            created_workers: BTreeSet::new(),
            scheduled_mutants: 0,
            run_finished_output_id: None,
            baseline_elapsed: std::time::Duration::ZERO,
            summary: MutationSummary::default(),
            output_sequence: 0,
            fingerprint: None,
            session_run_id: None,
            diagnostic_output_id: None,
            run_started_output_id: None,
            pending_failure: None,
            flags: RunFlags {
                scheduling: SchedulingFlags {
                    candidate_exhausted: false,
                    mutant_limit_reached: false,
                    stop_requested: false,
                },
                outcome: OutcomeFlags {
                    baseline_failed: false,
                    infrastructure_error: false,
                    incomplete: false,
                },
                report: ReportFlags {
                    interrupted: false,
                    report_started: false,
                    stop_after_run_started: false,
                },
                cleanup: CleanupFlags {
                    cleanup_done: false,
                    session_finish_attempted: false,
                },
            },
            stopped_candidates: VecDeque::new(),
            candidate_filter: None,
            ordered_candidate_ids: None,
            ordered_candidates: BTreeMap::new(),
            ordered_candidates_ready: VecDeque::new(),
            ordered_collection_complete: false,
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

    /// Creates a state that executes only candidates whose IDs are explicitly selected.
    #[must_use]
    pub fn with_candidate_filter(
        run_id: impl Into<String>,
        config: RunConfig,
        candidate_ids: BTreeSet<String>,
    ) -> Self {
        let mut state = Self::new(run_id, config);
        state.candidate_filter = Some(candidate_ids);
        state
    }

    /// Creates a state that executes selected candidates in the supplied order.
    #[must_use]
    pub fn with_ordered_candidate_filter(
        run_id: impl Into<String>,
        config: RunConfig,
        ordered_candidate_ids: Vec<String>,
    ) -> Self {
        let mut state = Self::new(run_id, config);
        state.candidate_filter = Some(ordered_candidate_ids.iter().cloned().collect());
        state.ordered_candidate_ids = Some(ordered_candidate_ids);
        state
    }

    #[must_use]
    pub fn phase(&self) -> RunPhase {
        self.phase
    }

    #[must_use]
    pub fn summary(&self) -> &MutationSummary {
        &self.summary
    }

    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    #[must_use]
    pub fn candidate_offset(&self) -> u64 {
        self.candidate_offset
    }

    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_effect_pending(&self, id: EffectId) -> bool {
        self.pending.contains_key(&id)
    }

    #[must_use]
    pub fn is_effect_retired(&self, id: EffectId) -> bool {
        self.retired.contains(&id)
    }

    #[must_use]
    pub fn completion_ledger_entries(&self) -> usize {
        self.completed_gaps.len()
    }

    fn exit_policy(&self) -> ExitPolicy {
        let summary_policy = ExitPolicy::from_summary(&self.summary);
        ExitPolicy {
            infrastructure_error: self.flags.outcome.infrastructure_error
                || summary_policy.infrastructure_error,
            baseline_failed: self.flags.outcome.baseline_failed,
            incomplete: self.flags.outcome.incomplete || summary_policy.incomplete,
            survivors: summary_policy.survivors,
            interrupted: self.flags.report.interrupted,
        }
    }

    fn complete(&self) -> bool {
        let policy = self.exit_policy();
        !policy.infrastructure_error
            && !policy.baseline_failed
            && !policy.incomplete
            && !policy.interrupted
    }

    #[must_use]
    pub fn exit_code(&self) -> i32 {
        exit_code_for(self.exit_policy())
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

    fn accept_completion(
        &mut self,
        event: &RunEvent,
    ) -> Result<Option<PendingEffect>, MachineError> {
        let Some((id, received)) = completion(event) else {
            return Ok(None);
        };
        if self.retired.contains(&id) {
            return Err(MachineError::RetiredEffect(id));
        }
        if id.0 <= self.completed_floor || self.completed_gaps.contains(&id) {
            return Err(MachineError::DuplicateEffect(id));
        }
        let expected = match self.pending.get(&id).copied() {
            Some(expected) => expected,
            None if id.0 >= self.next_effect_id => return Err(MachineError::UnknownEffect(id)),
            None => return Err(MachineError::EffectNotPending(id)),
        };
        if !matches!(event, RunEvent::EffectFailed(_)) && expected.kind != received {
            return Err(MachineError::WrongCompletion {
                id,
                expected: expected.kind,
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
        Ok(Some(expected))
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
            self.pending.insert(
                id,
                PendingEffect {
                    kind: expected_completion(effect),
                    worker: effect_worker(effect),
                },
            );
        }
        contract_ensure!(
            "machine.effect.once",
            self.pending.len() == self.pending.keys().collect::<BTreeSet<_>>().len(),
            (&self.pending, self.completed_floor, &self.completed_gaps)
        );
        Ok(())
    }

    fn retire_pending(&mut self) {
        self.retired.extend(self.pending.keys().copied());
        self.pending.clear();
        self.completed_floor = self.next_effect_id.saturating_sub(1);
        self.completed_gaps.clear();
        self.output_actions.clear();
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
        let count = u32::try_from(self.config.limits.jobs.get())
            .map_err(|_| MachineError::WorkerCountOverflow)?;
        let grant = self
            .copy_grant
            .ok_or(MachineError::WrongPhase { phase: self.phase })?;
        if grant.requested_workers() != count {
            return Err(MachineError::WorkerCountMismatch {
                configured: count,
                preflight: grant.requested_workers(),
            });
        }
        let mut effects = Vec::with_capacity(count as usize);
        for worker in 0..count {
            let id = self.allocate_id()?;
            effects.push(RunEffect::CreateWorker(
                grant
                    .create_worker(id, worker)
                    .map_err(|error| MachineError::Budget(error.to_string()))?,
            ));
        }
        self.phase = RunPhase::Copy;
        Ok(effects)
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
        let mut run_started = RunStarted::minimal(self.run_id.clone(), sequence);
        run_started.normalized_config = Some(self.config.clone());
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::RunStarted(run_started),
        })])
    }

    fn apply_worker_effects(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        let candidate = self.candidate(worker)?;
        self.worker_mut(worker)?.phase = WorkerPhase::Applying;
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::ApplyMutation(ApplyMutation {
            id,
            worker,
            candidate,
        })])
    }

    fn candidate_effects(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        if let Some(run_id) = self.session_run_id.clone() {
            let mutant_id = self
                .workers
                .get(&worker)
                .and_then(|state| state.candidate.as_ref())
                .as_ref()
                .ok_or(MachineError::WrongPhase { phase: self.phase })?
                .id
                .clone();
            self.worker_mut(worker)?.phase = WorkerPhase::Lookup;
            let id = self.allocate_id()?;
            Ok(vec![RunEffect::LookupStoredResult(LookupStoredResult {
                id,
                worker,
                run_id,
                mutant_id,
            })])
        } else {
            self.apply_worker_effects(worker)
        }
    }

    fn finalize_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::VerifyOriginals(VerifyOriginals {
            id,
            checkpoint: IntegrityCheckpoint::PreFinalReport,
        })])
    }

    fn read_next_candidate(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        let spool = self
            .candidate_spool
            .clone()
            .ok_or(MachineError::WrongPhase { phase: self.phase })?;
        self.worker_mut(worker)?.phase = WorkerPhase::Reading;
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::ReadCandidate(ReadCandidate {
            id,
            worker,
            spool,
            offset: self.candidate_offset,
        })])
    }

    fn worker_mut(&mut self, worker: u32) -> Result<&mut WorkerState, MachineError> {
        self.workers
            .get_mut(&worker)
            .ok_or(MachineError::UnknownWorker(worker))
    }

    fn idle_worker(&self) -> Option<u32> {
        self.workers
            .iter()
            .find_map(|(worker, state)| (state.phase == WorkerPhase::Idle).then_some(*worker))
    }

    fn candidate_read_pending(&self) -> bool {
        self.pending
            .values()
            .any(|pending| pending.kind == CompletionKind::CandidateLoaded)
    }

    #[cfg(feature = "contracts")]
    fn worker_invariant(&self) -> bool {
        let jobs = self.config.limits.jobs.get();
        let worker_ids_are_bounded = self
            .workers
            .keys()
            .all(|worker| usize::try_from(*worker).is_ok_and(|worker| worker < jobs));
        let created_ids_are_bounded = self
            .created_workers
            .iter()
            .all(|worker| usize::try_from(*worker).is_ok_and(|worker| worker < jobs));
        let pending_workers_are_bounded = self.pending.values().all(|pending| {
            pending
                .worker
                .is_none_or(|worker| usize::try_from(worker).is_ok_and(|worker| worker < jobs))
        });
        let candidate_reads = self
            .pending
            .values()
            .filter(|pending| pending.kind == CompletionKind::CandidateLoaded)
            .count();
        let running_processes = self
            .pending
            .values()
            .filter(|pending| pending.kind == CompletionKind::MutantFinished)
            .count();
        let worker_state_is_consistent = self.workers.values().all(|worker| match worker.phase {
            WorkerPhase::Idle | WorkerPhase::Reading => {
                worker.candidate.is_none() && worker.result.is_none()
            }
            WorkerPhase::Lookup
            | WorkerPhase::Applying
            | WorkerPhase::Starting
            | WorkerPhase::Running
            | WorkerPhase::SyntheticStarting(_)
            | WorkerPhase::SyntheticFinishing(_) => {
                worker.candidate.is_some() && worker.result.is_none()
            }
            WorkerPhase::Persisting | WorkerPhase::Finishing | WorkerPhase::Resetting => {
                worker.candidate.is_some() && worker.result.is_some()
            }
        });

        self.workers.len() <= jobs
            && self.created_workers.len() <= jobs
            && self.workers.keys().eq(self.created_workers.iter())
            && worker_ids_are_bounded
            && created_ids_are_bounded
            && pending_workers_are_bounded
            && candidate_reads <= 1
            && running_processes <= jobs
            && worker_state_is_consistent
    }

    fn schedule_read_or_finalize(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        if self.ordered_candidate_ids.is_some() {
            if !self.ordered_collection_complete {
                if !self.candidate_read_pending()
                    && let Some(worker) = self.idle_worker()
                {
                    return self.read_next_candidate(worker);
                }
                return Ok(Vec::new());
            }

            let mut effects = Vec::new();
            while let Some(worker) = self.idle_worker() {
                let Some(candidate) = self.ordered_candidates_ready.pop_front() else {
                    break;
                };
                let worker_state = self.worker_mut(worker)?;
                worker_state.candidate = Some(candidate);
                worker_state.phase = WorkerPhase::Idle;
                if self.flags.scheduling.stop_requested {
                    effects.extend(self.synthetic_started_output(worker, MutationStatus::NotRun)?);
                } else {
                    self.scheduled_mutants += 1;
                    effects.extend(self.candidate_effects(worker)?);
                }
            }
            if self.ordered_candidates_ready.is_empty()
                && self
                    .workers
                    .values()
                    .all(|worker| worker.phase == WorkerPhase::Idle)
            {
                self.flags.scheduling.candidate_exhausted = true;
                self.phase = RunPhase::Finalize;
                effects.extend(self.finalize_effects()?);
            }
            return Ok(effects);
        }
        if !self.flags.scheduling.candidate_exhausted
            && !self.candidate_read_pending()
            && let Some(worker) = self.idle_worker()
        {
            return self.read_next_candidate(worker);
        }
        if self.flags.scheduling.candidate_exhausted
            && self
                .workers
                .values()
                .all(|worker| worker.phase == WorkerPhase::Idle)
        {
            self.phase = RunPhase::Finalize;
            return self.finalize_effects();
        }
        Ok(Vec::new())
    }

    fn collect_ordered_candidate(
        &mut self,
        worker: u32,
        candidate: Option<MutationCandidate>,
    ) -> Result<Vec<RunEffect>, MachineError> {
        *self.worker_mut(worker)? = WorkerState::default();
        if let Some(candidate) = candidate {
            if self
                .candidate_filter
                .as_ref()
                .is_some_and(|filter| filter.contains(&candidate.id))
            {
                self.ordered_candidates
                    .insert(candidate.id.clone(), candidate);
            }
            return self.schedule_read_or_finalize();
        }

        let ordered_ids = self
            .ordered_candidate_ids
            .as_ref()
            .expect("ordered collection requires ordered ids");
        for candidate_id in ordered_ids {
            if !self.ordered_candidates.contains_key(candidate_id) {
                return Err(MachineError::SelectedCandidateMissing(candidate_id.clone()));
            }
        }
        self.ordered_candidates_ready = ordered_ids
            .iter()
            .map(|candidate_id| {
                self.ordered_candidates
                    .remove(candidate_id)
                    .expect("ordered candidate presence was validated")
            })
            .collect();
        self.ordered_collection_complete = true;
        self.schedule_read_or_finalize()
    }

    fn candidate(&self, worker: u32) -> Result<MutationCandidate, MachineError> {
        self.workers
            .get(&worker)
            .and_then(|state| state.candidate.clone())
            .ok_or(MachineError::WrongPhase { phase: self.phase })
    }

    fn started_output(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        self.worker_mut(worker)?.phase = WorkerPhase::Starting;
        let candidate = self.candidate(worker)?;
        let id = self.allocate_id()?;
        self.output_actions
            .insert(id, OutputAction::StartMutant(worker));
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::MutantStarted(MutantStarted::new(
                self.run_id.clone(),
                self.output_sequence(),
                candidate.id,
                candidate.sequence,
            )),
        })])
    }

    fn synthetic_started_output(
        &mut self,
        worker: u32,
        status: MutationStatus,
    ) -> Result<Vec<RunEffect>, MachineError> {
        self.worker_mut(worker)?.phase = WorkerPhase::SyntheticStarting(status);
        let candidate = self.candidate(worker)?;
        let id = self.allocate_id()?;
        self.output_actions
            .insert(id, OutputAction::StartSynthetic(worker, status));
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::MutantStarted(MutantStarted::new(
                self.run_id.clone(),
                self.output_sequence(),
                candidate.id,
                candidate.sequence,
            )),
        })])
    }

    fn max_processes(&self) -> Result<u32, MachineError> {
        u32::try_from(self.config.limits.max_processes.get())
            .map_err(|_| MachineError::ProcessCountOverflow)
    }
    fn mutant_process(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        self.worker_mut(worker)?.phase = WorkerPhase::Running;
        let id = self.allocate_id()?;
        let timeout = match self.config.limits.mutant_timeout {
            MutantTimeout::Auto => auto_mutant_timeout(self.baseline_elapsed),
            MutantTimeout::Fixed(value) => value.get(),
        };
        Ok(vec![RunEffect::RunMutant(RunProcess {
            id,
            worker: Some(worker),
            run_id: Some(self.run_id.clone()),
            mutant_id: Some(self.candidate(worker)?.id),
            argv: self.config.test_argv.clone(),
            cwd: self.config.root.clone(),
            limits: ProcessLimits {
                timeout,
                max_output_bytes: self.config.limits.max_output.get(),
                max_memory_bytes: self.config.limits.max_memory.get(),
                max_processes: self.max_processes()?,
            },
        })])
    }

    fn finished_output(
        &mut self,
        worker: u32,
        termination: Option<ProcessTermination>,
    ) -> Result<Vec<RunEffect>, MachineError> {
        let result = self
            .workers
            .get(&worker)
            .and_then(|state| state.result.clone())
            .ok_or(MachineError::WrongPhase { phase: self.phase })?;
        self.worker_mut(worker)?.phase = WorkerPhase::Finishing;
        let id = self.allocate_id()?;
        self.output_actions
            .insert(id, OutputAction::FinishMutant(worker));
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::MutantFinished(MutantOutput {
                schema_version: crate::REPORT_SCHEMA_VERSION,
                sequence: self.output_sequence(),
                run_id: self.run_id.clone(),
                candidate: result.candidate,
                status: result.status,
                termination,
                elapsed_ms: elapsed_millis(result.elapsed),
                resource_mode: result.resource_mode,
                output: result.output,
            }),
        })])
    }

    fn synthetic_finished_output(
        &mut self,
        worker: u32,
        status: MutationStatus,
    ) -> Result<Vec<RunEffect>, MachineError> {
        self.worker_mut(worker)?.phase = WorkerPhase::SyntheticFinishing(status);
        let candidate = self.candidate(worker)?;
        let id = self.allocate_id()?;
        self.output_actions
            .insert(id, OutputAction::FinishSynthetic(worker, status));
        Ok(vec![RunEffect::EmitOutput(EmitOutput {
            id,
            event: OutputEvent::MutantFinished(MutantOutput {
                schema_version: crate::REPORT_SCHEMA_VERSION,
                sequence: self.output_sequence(),
                run_id: self.run_id.clone(),
                candidate,
                status,
                termination: None,
                elapsed_ms: 0,
                resource_mode: crate::ResourceMode::Hard,
                output: None,
            }),
        })])
    }

    fn begin_stopped_mutant_drain(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let mut stopped = Vec::new();
        for worker in self.workers.values() {
            let candidate = worker.candidate.clone();
            let stopped_candidate = match worker.phase {
                WorkerPhase::Lookup | WorkerPhase::Applying | WorkerPhase::Starting => {
                    candidate.map(StoppedCandidate::NotStarted)
                }
                WorkerPhase::SyntheticStarting(status) => candidate
                    .map(|candidate| StoppedCandidate::SyntheticNotStarted(candidate, status)),
                WorkerPhase::Running | WorkerPhase::Persisting => {
                    candidate.map(StoppedCandidate::StartedNotRun)
                }
                WorkerPhase::Finishing => worker.result.clone().map(StoppedCandidate::Finished),
                WorkerPhase::SyntheticFinishing(status) => candidate
                    .map(|candidate| StoppedCandidate::SyntheticFinished(candidate, status)),
                WorkerPhase::Idle | WorkerPhase::Reading | WorkerPhase::Resetting => None,
            };
            if let Some(candidate) = stopped_candidate {
                stopped.push(candidate);
            }
        }
        stopped.sort_by_key(|candidate| match candidate {
            StoppedCandidate::NotStarted(candidate)
            | StoppedCandidate::SyntheticNotStarted(candidate, _)
            | StoppedCandidate::StartedNotRun(candidate)
            | StoppedCandidate::SyntheticFinished(candidate, _) => candidate.sequence,
            StoppedCandidate::Finished(result) => result.candidate.sequence,
        });
        self.stopped_candidates = stopped.into();
        for worker in self.workers.values_mut() {
            *worker = WorkerState::default();
        }
        self.phase = RunPhase::Mutants;
        self.next_stopped_candidate_effects()
    }

    fn next_stopped_candidate_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let worker = *self
            .workers
            .keys()
            .next()
            .ok_or(MachineError::WrongPhase { phase: self.phase })?;
        if let Some(candidate) = self.stopped_candidates.pop_front() {
            match candidate {
                StoppedCandidate::NotStarted(candidate) => {
                    self.worker_mut(worker)?.candidate = Some(candidate);
                    self.synthetic_started_output(worker, MutationStatus::NotRun)
                }
                StoppedCandidate::SyntheticNotStarted(candidate, status) => {
                    self.worker_mut(worker)?.candidate = Some(candidate);
                    self.synthetic_started_output(worker, status)
                }
                StoppedCandidate::StartedNotRun(candidate) => {
                    self.worker_mut(worker)?.candidate = Some(candidate);
                    self.synthetic_finished_output(worker, MutationStatus::NotRun)
                }
                StoppedCandidate::Finished(result) => {
                    let worker_state = self.worker_mut(worker)?;
                    worker_state.candidate = Some(result.candidate.clone());
                    worker_state.result = Some(result);
                    self.finished_output(worker, None)
                }
                StoppedCandidate::SyntheticFinished(candidate, status) => {
                    self.worker_mut(worker)?.candidate = Some(candidate);
                    self.synthetic_finished_output(worker, status)
                }
            }
        } else if !self.flags.scheduling.candidate_exhausted {
            self.read_next_candidate(worker)
        } else {
            self.phase = RunPhase::Finalize;
            self.finalize_effects()
        }
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
                complete: self.complete(),
                exit_code: self.exit_code(),
            }),
        })])
    }

    fn post_cleanup_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        self.phase = RunPhase::Finalize;
        let complete = self.complete();
        if let Some(run_id) = self.session_run_id.clone()
            && !self.flags.cleanup.session_finish_attempted
        {
            self.flags.cleanup.session_finish_attempted = true;
            let id = self.allocate_id()?;
            Ok(vec![RunEffect::FinishSession(FinishSession {
                id,
                run_id,
                complete,
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
    #[error("effect completion {0:?} was retired by a stop transition")]
    RetiredEffect(EffectId),
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
    #[error("selected candidate execution requires an analyzer candidate spool")]
    MissingCandidateSpool,
    #[error("selected candidate was not discovered: {0}")]
    SelectedCandidateMissing(String),
    #[error("configured worker count does not fit in u32")]
    WorkerCountOverflow,
    #[error("configured maximum process count does not fit in u32")]
    ProcessCountOverflow,
    #[error("preflight reserved {preflight} workers, configured jobs is {configured}")]
    WorkerCountMismatch { configured: u32, preflight: u32 },
    #[error("unknown worker {0}")]
    UnknownWorker(u32),
    #[error("process worker mismatch: expected {expected:?}, received {received:?}")]
    ProcessWorkerMismatch {
        expected: Option<u32>,
        received: Option<u32>,
    },
    #[error(
        "session {field} mismatch for worker {worker}: expected {expected}, received {received}"
    )]
    SessionIdentityMismatch {
        worker: u32,
        field: &'static str,
        expected: String,
        received: String,
    },
}

impl MachineError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownEffect(_) => "machine.effect.unknown",
            Self::DuplicateEffect(_) => "machine.effect.duplicate",
            Self::EffectNotPending(_) => "machine.effect.not_pending",
            Self::RetiredEffect(_) => "machine.effect.retired",
            Self::WrongCompletion { .. } => "machine.effect.wrong_completion",
            Self::WrongPhase { .. } => "machine.phase.invalid",
            Self::EffectIdOverflow => "machine.effect_id.overflow",
            Self::Budget(_) => "machine.budget",
            Self::MissingFingerprint => "machine.fingerprint.missing",
            Self::MissingCandidateSpool => "machine.candidate_spool.missing",
            Self::SelectedCandidateMissing(_) => "machine.candidate.missing",
            Self::WorkerCountOverflow => "machine.worker.count_overflow",
            Self::ProcessCountOverflow => "machine.process.count_overflow",
            Self::WorkerCountMismatch { .. } => "machine.worker.count_mismatch",
            Self::UnknownWorker(_) => "machine.worker.unknown",
            Self::ProcessWorkerMismatch { .. } => "machine.worker.process_mismatch",
            Self::SessionIdentityMismatch { .. } => "machine.session.identity_mismatch",
        }
    }
}

/// Applies an event and produces its resulting state and requested effects.
///
/// # Errors
///
/// Returns [`MachineError`] when the event does not complete a pending effect or is invalid for the current state.
#[allow(
    clippy::too_many_lines,
    reason = "the transition table is kept together to make state-machine cases auditable"
)]
pub fn transition(
    mut state: RunState,
    event: RunEvent,
) -> Result<(RunState, Vec<RunEffect>), MachineError> {
    #[cfg(feature = "contracts")]
    let was_fatal = state.flags.outcome.infrastructure_error;
    let completed = state.accept_completion(&event)?;
    let completed_worker = completed.and_then(|pending| pending.worker);
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
            state.flags.report.report_started = true;
            if let Some(failed) = state.pending_failure.take() {
                state.diagnostic_effect(&failed)?
            } else if state.flags.report.stop_after_run_started {
                state.flags.report.stop_after_run_started = false;
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
        RunEvent::WorkerCreated(value) if state.phase == RunPhase::Copy => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            state.created_workers.insert(value.worker);
            state.workers.entry(value.worker).or_default();
            if state.created_workers.len() == state.config.limits.jobs.get() {
                state.phase = RunPhase::Baseline;
                let id = state.allocate_id()?;
                vec![RunEffect::RunBaseline(RunProcess {
                    id,
                    worker: Some(0),
                    run_id: Some(state.run_id.clone()),
                    mutant_id: None,
                    argv: state.config.test_argv.clone(),
                    cwd: state.config.root.clone(),
                    limits: ProcessLimits {
                        timeout: state.config.limits.baseline_timeout.get(),
                        max_output_bytes: state.config.limits.max_output.get(),
                        max_memory_bytes: state.config.limits.max_memory.get(),
                        max_processes: state.max_processes()?,
                    },
                })]
            } else {
                Vec::new()
            }
        }
        RunEvent::BaselineFinished(value) if state.phase == RunPhase::Baseline => {
            if completed_worker != value.worker {
                return Err(MachineError::ProcessWorkerMismatch {
                    expected: completed_worker,
                    received: value.worker,
                });
            }
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
                state.flags.outcome.baseline_failed = true;
                state.phase = RunPhase::Finalize;
                effects.extend(state.finalize_effects()?);
            }
            effects
        }
        RunEvent::AnalysisFinished(value) if state.phase == RunPhase::Analyze => {
            if state.candidate_filter.is_some() {
                match value.spool {
                    None if !value.truncated && !state.targets.is_empty() => {
                        state.analyze_next()?
                    }
                    None => return Err(MachineError::MissingCandidateSpool),
                    Some(spool) if spool.records == 0 => {
                        state.candidate_spool = Some(spool);
                        state.phase = RunPhase::Finalize;
                        state.finalize_effects()?
                    }
                    Some(spool) => {
                        state.phase = RunPhase::Mutants;
                        state.candidate_spool = Some(spool);
                        state.schedule_read_or_finalize()?
                    }
                }
            } else {
                state.flags.outcome.incomplete |= value.truncated;
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
                            state.schedule_read_or_finalize()?
                        }
                    }
                }
            }
        }
        RunEvent::CandidateLoaded(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            let worker = value.worker;
            state.candidate_offset = value.next_offset;
            if state.ordered_candidate_ids.is_some() && !state.ordered_collection_complete {
                state.collect_ordered_candidate(worker, value.candidate)?
            } else {
                let selected = value.candidate.as_ref().is_none_or(|candidate| {
                    state
                        .candidate_filter
                        .as_ref()
                        .is_none_or(|filter| filter.contains(&candidate.id))
                });
                if !selected {
                    *state.worker_mut(worker)? = WorkerState::default();
                    state.schedule_read_or_finalize()?
                } else if state.flags.scheduling.stop_requested {
                    if let Some(candidate) = value.candidate {
                        let worker_state = state.worker_mut(worker)?;
                        worker_state.candidate = Some(candidate);
                        worker_state.result = None;
                        worker_state.phase = WorkerPhase::Idle;
                        state.synthetic_started_output(worker, MutationStatus::NotRun)?
                    } else {
                        let worker_state = state.worker_mut(worker)?;
                        *worker_state = WorkerState::default();
                        state.flags.scheduling.candidate_exhausted = true;
                        state.next_stopped_candidate_effects()?
                    }
                } else {
                    let mut effects = if let Some(candidate) = value.candidate {
                        contract_ensure!(
                            "machine.worker.candidate.pre",
                            state.worker_mut(worker)?.candidate.is_none(),
                            (worker, &candidate)
                        );
                        let worker_state = state.worker_mut(worker)?;
                        worker_state.candidate = Some(candidate);
                        worker_state.phase = WorkerPhase::Idle;
                        if state.scheduled_mutants >= state.config.limits.max_mutants.get() as u64 {
                            state.flags.scheduling.mutant_limit_reached = true;
                            state.flags.outcome.incomplete = true;
                            state.synthetic_started_output(worker, MutationStatus::NotRun)?
                        } else {
                            state.scheduled_mutants += 1;
                            state.candidate_effects(worker)?
                        }
                    } else {
                        let worker_state = state.worker_mut(worker)?;
                        worker_state.phase = WorkerPhase::Idle;
                        worker_state.candidate = None;
                        state.flags.scheduling.candidate_exhausted = true;
                        Vec::new()
                    };
                    effects.extend(state.schedule_read_or_finalize()?);
                    effects
                }
            }
        }
        RunEvent::StoredResultLoaded(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            let worker = value.worker;
            if let Some(result) = value.result.as_ref() {
                let expected = state.candidate(worker)?.id;
                if result.mutant_id != expected {
                    return Err(MachineError::SessionIdentityMismatch {
                        worker,
                        field: "mutant_id",
                        expected,
                        received: result.mutant_id.clone(),
                    });
                }
            }
            match crate::resume_policy(value.result.as_ref()) {
                ResumeDecision::Rerun => state.apply_worker_effects(worker)?,
                ResumeDecision::Reuse => {
                    let result = value
                        .result
                        .ok_or(MachineError::WrongPhase { phase: state.phase })?;
                    state.synthetic_started_output(worker, result.status)?
                }
            }
        }
        RunEvent::MutationApplied(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            state.started_output(value.worker)?
        }
        RunEvent::MutantFinished(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != value.worker {
                return Err(MachineError::ProcessWorkerMismatch {
                    expected: completed_worker,
                    received: value.worker,
                });
            }
            let worker = value
                .worker
                .ok_or(MachineError::WrongPhase { phase: state.phase })?;
            let candidate = state.candidate(worker)?;
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
            state.worker_mut(worker)?.result = Some(result.clone());
            if state.session_run_id.is_some() {
                state.worker_mut(worker)?.phase = WorkerPhase::Persisting;
                let id = state.allocate_id()?;
                vec![RunEffect::PersistResult(PersistResult {
                    id,
                    worker,
                    result,
                })]
            } else {
                state.summary.record(status);
                state.finished_output(worker, Some(value.termination))?
            }
        }
        RunEvent::ResultPersisted(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            let worker = value.worker;
            let result = state
                .workers
                .get(&worker)
                .and_then(|state| state.result.as_ref())
                .ok_or(MachineError::WrongPhase { phase: state.phase })?;
            if value.run_id != result.run_id {
                return Err(MachineError::SessionIdentityMismatch {
                    worker,
                    field: "run_id",
                    expected: result.run_id.clone(),
                    received: value.run_id,
                });
            }
            if value.mutant_id != result.candidate.id {
                return Err(MachineError::SessionIdentityMismatch {
                    worker,
                    field: "mutant_id",
                    expected: result.candidate.id.clone(),
                    received: value.mutant_id,
                });
            }
            state.summary.record(result.status);
            state.finished_output(worker, None)?
        }
        RunEvent::OutputEmitted(value) if state.output_actions.contains_key(&value.id) => {
            match state
                .output_actions
                .remove(&value.id)
                .ok_or(MachineError::WrongPhase { phase: state.phase })?
            {
                OutputAction::StartMutant(worker) => state.mutant_process(worker)?,
                OutputAction::StartSynthetic(worker, status) => {
                    state.synthetic_finished_output(worker, status)?
                }
                OutputAction::FinishMutant(worker) => {
                    if state.flags.scheduling.stop_requested {
                        *state.worker_mut(worker)? = WorkerState::default();
                        state.next_stopped_candidate_effects()?
                    } else {
                        state.worker_mut(worker)?.phase = WorkerPhase::Resetting;
                        let id = state.allocate_id()?;
                        vec![RunEffect::ResetWorker(ResetWorker { id, worker })]
                    }
                }
                OutputAction::FinishSynthetic(worker, status) => {
                    state.summary.record(status);
                    let worker_state = state.worker_mut(worker)?;
                    worker_state.phase = WorkerPhase::Idle;
                    worker_state.candidate = None;
                    worker_state.result = None;
                    if state.flags.scheduling.stop_requested {
                        state.next_stopped_candidate_effects()?
                    } else {
                        state.schedule_read_or_finalize()?
                    }
                }
            }
        }
        RunEvent::WorkerReset(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            let worker_state = state.worker_mut(value.worker)?;
            worker_state.phase = WorkerPhase::Idle;
            worker_state.candidate = None;
            worker_state.result = None;
            state.schedule_read_or_finalize()?
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
            if state.flags.cleanup.cleanup_done {
                state.post_cleanup_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        RunEvent::CleanupFinished(value) if state.phase == RunPhase::Cleaning => {
            release_workspace_copy(&mut state.budgets, &value)
                .map_err(|error| MachineError::Budget(error.to_string()))?;
            state.copy_grant = None;
            state.flags.cleanup.cleanup_done = true;
            state.post_cleanup_effects()?
        }
        RunEvent::OutputEmitted(_) => Vec::new(),
        RunEvent::EffectFailed(failed) if state.phase == RunPhase::Cleaning => {
            state.flags.outcome.infrastructure_error = true;
            state.flags.scheduling.stop_requested = true;
            state.retire_pending();
            state.copy_grant = None;
            state.flags.cleanup.cleanup_done = true;
            state.diagnostic_effect(&failed)?
        }
        RunEvent::EffectFailed(failed) if state.diagnostic_output_id == Some(failed.id) => {
            state.flags.outcome.infrastructure_error = true;
            state.flags.scheduling.stop_requested = true;
            state.retire_pending();
            state.diagnostic_output_id = None;
            if state.flags.cleanup.cleanup_done {
                state.post_cleanup_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        RunEvent::EffectFailed(failed) => {
            state.flags.outcome.infrastructure_error = true;
            state.flags.scheduling.stop_requested = true;
            state.retire_pending();
            if !state.flags.report.report_started {
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
            state.flags.outcome.incomplete = true;
            state.flags.scheduling.stop_requested = true;
            let was_mutating = state.phase == RunPhase::Mutants;
            state.retire_pending();
            if was_mutating {
                state.begin_stopped_mutant_drain()?
            } else if !state.flags.report.report_started {
                state.flags.report.stop_after_run_started = true;
                state.start_run_effects()?
            } else if state.copy_grant.is_some() {
                state.phase = RunPhase::Finalize;
                state.finalize_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        RunEvent::CancellationRequested => {
            state.flags.report.interrupted = true;
            state.flags.scheduling.stop_requested = true;
            let was_mutating = state.phase == RunPhase::Mutants;
            state.retire_pending();
            if was_mutating {
                state.begin_stopped_mutant_drain()?
            } else if !state.flags.report.report_started {
                state.flags.report.stop_after_run_started = true;
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
        (!matches!(
            state.phase,
            RunPhase::Baseline | RunPhase::Analyze | RunPhase::Mutants
        ) || state.copy_grant.is_some())
            && state.worker_invariant(),
        (
            &state.phase,
            &state.copy_grant,
            &state.workers,
            &state.created_workers,
            &state.pending,
        )
    );
    contract_ensure!(
        "machine.budget.invariant",
        state.config.limits.jobs.get() <= state.config.limits.max_processes.get()
            && state
                .pending
                .values()
                .filter(|pending| matches!(
                    pending.kind,
                    CompletionKind::BaselineFinished | CompletionKind::MutantFinished
                ))
                .count()
                <= state.config.limits.jobs.get(),
        (
            state.config.limits.jobs,
            state.config.limits.max_processes,
            &state.pending,
        )
    );
    contract_ensure!(
        "machine.cancel.post",
        !(was_fatal
            || state.flags.outcome.infrastructure_error
            || state.flags.scheduling.stop_requested)
            || !effects.iter().any(|effect| matches!(
                effect,
                RunEffect::ApplyMutation(_) | RunEffect::RunMutant(_)
            )),
        (&state.phase, &effects)
    );
    Ok((state, effects))
}

fn elapsed_millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn baseline_output(state: &mut RunState, value: &ProcessFinished) -> OutputEvent {
    OutputEvent::BaselineFinished(BaselineOutput {
        schema_version: crate::REPORT_SCHEMA_VERSION,
        sequence: state.output_sequence(),
        run_id: state.run_id.clone(),
        termination: value.termination,
        elapsed_ms: elapsed_millis(value.elapsed),
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

fn effect_worker(effect: &RunEffect) -> Option<u32> {
    match effect {
        RunEffect::CreateWorker(value) => Some(value.worker()),
        RunEffect::RunBaseline(value) | RunEffect::RunMutant(value) => value.worker,
        RunEffect::ReadCandidate(value) => Some(value.worker),
        RunEffect::ApplyMutation(value) => Some(value.worker),
        RunEffect::ResetWorker(value) => Some(value.worker),
        RunEffect::LookupStoredResult(value) => Some(value.worker),
        RunEffect::PersistResult(value) => Some(value.worker),
        _ => None,
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
