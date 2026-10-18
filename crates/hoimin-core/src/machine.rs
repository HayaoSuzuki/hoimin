mod selection;
#[cfg(test)]
mod stop_tests;
use selection::OrderedCandidatePhase;
pub use selection::{RunSelection, RunSelectionError};

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use thiserror::Error;

use crate::{
    AnalysisDiagnostic, AnalysisFinished, AnalyzeFile, ApplyMutation,
    BaselineFinished as BaselineOutput, BeginSession, BudgetLedger, CandidateCursor,
    CandidateSpoolRef, Cleanup, Diagnostic, EffectFailed, EffectFailure, EffectId, EmitOutput,
    ExitPolicy, FinishSession, IntegrityCheckpoint, LoadSession, LookupStoredResult,
    MutantFinished as MutantOutput, MutantResult, MutantStarted, MutantTimeout, MutationCandidate,
    MutationOutcome, MutationSummary, ObserveRemainingBudget, OutputEvent,
    PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE, PersistResult, Preflight, ProcessFinished, ProcessLimits,
    ProcessOutputState, ProcessTermination, ReadCandidate, ReservationId, ResetWorker,
    ResolveTargets, ResumeDecision, ReusedMutationStatus, RunBudgets, RunConfig, RunEffect,
    RunEvent, RunFingerprint, RunProcess, RunStarted, RunSummary, SessionDiagnostic, TargetSlice,
    VerifyOriginals, WorkspaceCopyGrant, auto_mutant_timeout, contract_ensure, exit_code_for,
    project_top_budget, release_workspace_copy, reserve_workspace_copy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunPhase {
    Validate,
    Preflight,
    Copy,
    MaterializationVerification,
    Baseline,
    BaselineRestoration,
    BudgetCheck,
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
    RemainingBudgetObserved,
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingEffect {
    completion: PendingCompletion,
    worker: Option<u32>,
    cleanup_reservations: Option<Vec<ReservationId>>,
}

/// Output waits own their continuation; ordinary effects retain their completion kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingCompletion {
    Effect(CompletionKind),
    Output(OutputContinuation),
}

impl PendingEffect {
    fn kind(&self) -> CompletionKind {
        match self.completion {
            PendingCompletion::Effect(kind) => kind,
            PendingCompletion::Output(_) => CompletionKind::OutputEmitted,
        }
    }

    fn output_continuation(&self) -> Option<OutputContinuation> {
        match self.completion {
            PendingCompletion::Output(continuation) => Some(continuation),
            PendingCompletion::Effect(_) => None,
        }
    }
}

/// Synthetic results remain owned by the worker (or stopped drain) until output ACK.
#[derive(Clone, Copy, Debug)]
enum SyntheticOutcome {
    Reused(ReusedMutationStatus),
    NotRun,
}

impl SyntheticOutcome {
    fn report_outcome(self) -> MutationOutcome {
        match self {
            Self::Reused(status) => MutationOutcome::Reused { status },
            Self::NotRun => MutationOutcome::NotRun,
        }
    }
}

/// Each phase owns its required data. Acknowledged results are released before reset.
#[derive(Clone, Debug, Default)]
enum WorkerState {
    #[default]
    Idle,
    Reading,
    Ready(MutationCandidate),
    Lookup(MutationCandidate),
    Applying(MutationCandidate),
    Starting(MutationCandidate),
    Running(MutationCandidate),
    Persisting(MutantResult),
    Finishing(MutantResult),
    Resetting,
    SyntheticStarting(MutationCandidate, SyntheticOutcome),
    SyntheticFinishing(MutationCandidate, SyntheticOutcome),
}

impl WorkerState {
    fn candidate(&self) -> Option<&MutationCandidate> {
        match self {
            Self::Ready(candidate)
            | Self::Lookup(candidate)
            | Self::Applying(candidate)
            | Self::Starting(candidate)
            | Self::Running(candidate)
            | Self::SyntheticStarting(candidate, _)
            | Self::SyntheticFinishing(candidate, _) => Some(candidate),
            Self::Persisting(result) | Self::Finishing(result) => Some(&result.candidate),
            Self::Idle | Self::Reading | Self::Resetting => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputContinuation {
    Baseline,
    RunStarted,
    RunFinished,
    Diagnostic,
    ContinueAfterBudgetWarning,
    ContinueAfterAnalysisDiagnostic,
    StartMutant(u32),
    StartSynthetic(u32),
    FinishMutant(u32),
    FinishSynthetic(u32),
}

#[derive(Clone, Debug)]
enum StoppedCandidate {
    NotStarted(MutationCandidate),
    SyntheticNotStarted(MutationCandidate, SyntheticOutcome),
    StartedNotRun(MutationCandidate),
    Finished(MutantResult),
    SyntheticFinished(MutationCandidate, SyntheticOutcome),
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
    delivery: ReportDelivery,
    stop_after_run_started: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReportDelivery {
    Writable,
    Failed,
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
    resource_control: crate::ResourceControl,
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
    candidate_cursor: CandidateCursor,
    workers: BTreeMap<u32, WorkerState>,
    scheduled_mutants: u64,
    baseline_elapsed: std::time::Duration,
    summary: MutationSummary,
    disk_summary: crate::DiskRunSummary,
    output_sequence: u64,
    fingerprint: Option<RunFingerprint>,
    session_run_id: Option<String>,
    resume_outcome: Option<crate::ResumeOutcome>,
    pending_failure: Option<EffectFailed>,
    flags: RunFlags,
    stopped_candidates: VecDeque<StoppedCandidate>,
    selection: RunSelection,
    analysis_diagnostics: VecDeque<AnalysisDiagnostic>,
    pending_analysis: Option<AnalysisFinished>,
}

impl RunState {
    /// Creates a run with the description of its already-selected resource backend.
    pub fn new(
        run_id: impl Into<String>,
        config: RunConfig,
        resource_control: crate::ResourceControl,
    ) -> Self {
        Self::with_selection(run_id, config, RunSelection::all(), resource_control)
    }

    /// Initializes execution with checked membership, ordering and provenance.
    pub fn with_selection(
        run_id: impl Into<String>,
        config: RunConfig,
        selection: RunSelection,
        resource_control: crate::ResourceControl,
    ) -> Self {
        let budgets = RunBudgets {
            memory: config.limits.max_memory.get(),
            copy: config.limits.max_copy_size.get(),
            processes: config.limits.max_processes.get() as u64,
        };
        let disk_summary = crate::DiskRunSummary::unmeasured(
            config.limits.max_workspace_size.get(),
            config.limits.min_free_space.get(),
        );
        Self {
            run_id: run_id.into(),
            config,
            resource_control,
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
            candidate_cursor: CandidateCursor::START,
            workers: BTreeMap::new(),
            scheduled_mutants: 0,
            baseline_elapsed: std::time::Duration::ZERO,
            summary: MutationSummary::default(),
            disk_summary,
            output_sequence: 0,
            fingerprint: None,
            session_run_id: None,
            resume_outcome: None,
            pending_failure: None,
            flags: RunFlags {
                scheduling: SchedulingFlags {
                    candidate_exhausted: false,
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
                    delivery: ReportDelivery::Writable,
                    stop_after_run_started: false,
                },
                cleanup: CleanupFlags {
                    cleanup_done: false,
                    session_finish_attempted: false,
                },
            },
            stopped_candidates: VecDeque::new(),
            selection,
            analysis_diagnostics: VecDeque::new(),
            pending_analysis: None,
        }
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
    pub fn disk_summary(&self) -> &crate::DiskRunSummary {
        &self.disk_summary
    }

    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    #[must_use]
    pub fn candidate_offset(&self) -> u64 {
        self.candidate_cursor.offset
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
        let expected = match self.pending.get(&id).cloned() {
            Some(expected) => expected,
            None if id.0 >= self.next_effect_id => return Err(MachineError::UnknownEffect(id)),
            None => return Err(MachineError::EffectNotPending(id)),
        };
        if !matches!(event, RunEvent::EffectFailed(_)) && expected.kind() != received {
            return Err(MachineError::WrongCompletion {
                id,
                expected: expected.kind(),
                received,
            });
        }
        if let (Some(expected_reservations), RunEvent::CleanupFinished(cleanup)) =
            (&expected.cleanup_reservations, event)
            && !cleanup_reservations_match(expected_reservations, &cleanup.released_reservations)
        {
            return Err(MachineError::CleanupReservationMismatch {
                id,
                expected: expected_reservations.clone(),
                received: cleanup.released_reservations.clone(),
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

    /// Registers an output and its ACK/failure continuation together. IDs come from
    /// `allocate_id`; retirement and accepted completion remove this sole owner.
    fn register_output(
        &mut self,
        output: EmitOutput,
        continuation: OutputContinuation,
    ) -> RunEffect {
        let previous = self.pending.insert(
            output.id,
            PendingEffect {
                completion: PendingCompletion::Output(continuation),
                worker: None,
                cleanup_reservations: None,
            },
        );
        assert!(previous.is_none(), "output ID must be freshly allocated");
        RunEffect::EmitOutput(output)
    }

    fn register(&mut self, effects: &[RunEffect]) -> Result<(), MachineError> {
        for effect in effects {
            let id = effect.id();
            if matches!(effect, RunEffect::EmitOutput(_)) {
                assert!(
                    self.pending
                        .get(&id)
                        .and_then(PendingEffect::output_continuation)
                        .is_some(),
                    "outputs must be constructed through register_output"
                );
                continue;
            }
            if self.pending.contains_key(&id)
                || id.0 <= self.completed_floor
                || self.completed_gaps.contains(&id)
            {
                return Err(MachineError::DuplicateEffect(id));
            }
            self.pending.insert(
                id,
                PendingEffect {
                    completion: PendingCompletion::Effect(expected_completion(effect)),
                    worker: effect_worker(effect),
                    cleanup_reservations: match effect {
                        RunEffect::Cleanup(cleanup) => Some(cleanup.reservations.clone()),
                        _ => None,
                    },
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
        self.analysis_diagnostics.clear();
        self.pending_analysis = None;
    }

    fn diagnostic_effect(&mut self, failed: &EffectFailed) -> Result<Vec<RunEffect>, MachineError> {
        self.phase = RunPhase::Finalize;
        let id = self.allocate_id()?;
        let output = EmitOutput {
            id,
            event: OutputEvent::Diagnostic(Diagnostic::new(
                self.run_id.clone(),
                self.output_sequence(),
                "error",
                failed.failure.code(),
                failed.failure.message(),
            )),
        };
        Ok(vec![
            self.register_output(output, OutputContinuation::Diagnostic),
        ])
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
        })])
    }

    fn analysis_diagnostic_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        if let Some(diagnostic) = self.analysis_diagnostics.pop_front() {
            let id = self.allocate_id()?;
            let output = EmitOutput {
                id,
                event: OutputEvent::Diagnostic(Diagnostic::new(
                    self.run_id.clone(),
                    self.output_sequence(),
                    "warning",
                    diagnostic.code,
                    diagnostic.message,
                )),
            };
            Ok(vec![self.register_output(
                output,
                OutputContinuation::ContinueAfterAnalysisDiagnostic,
            )])
        } else {
            let analysis = self
                .pending_analysis
                .take()
                .ok_or(MachineError::WrongPhase { phase: self.phase })?;
            self.apply_analysis_finished(analysis)
        }
    }

    fn apply_analysis_finished(
        &mut self,
        value: AnalysisFinished,
    ) -> Result<Vec<RunEffect>, MachineError> {
        if self.selection.candidate_ids().is_some() {
            self.flags.outcome.incomplete |= value.truncated;
            match value.spool {
                None if !value.truncated && !self.targets.is_empty() => self.analyze_next(),
                None => Err(MachineError::MissingCandidateSpool),
                Some(spool) if spool.records == 0 => {
                    self.ensure_selected_candidates_discovered()?;
                    self.candidate_spool = Some(spool);
                    self.phase = RunPhase::Finalize;
                    self.finalize_effects()
                }
                Some(spool) => {
                    self.phase = RunPhase::Mutants;
                    self.candidate_spool = Some(spool);
                    self.schedule_read_or_finalize()
                }
            }
        } else {
            self.flags.outcome.incomplete |= value.truncated;
            if value.truncated {
                self.phase = RunPhase::Finalize;
                self.finalize_effects()
            } else {
                match value.spool {
                    None => self.analyze_next(),
                    Some(spool) if spool.records == 0 => {
                        self.candidate_spool = Some(spool);
                        self.phase = RunPhase::Finalize;
                        self.finalize_effects()
                    }
                    Some(spool) => {
                        self.phase = RunPhase::Mutants;
                        self.candidate_spool = Some(spool);
                        self.schedule_read_or_finalize()
                    }
                }
            }
        }
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

    fn materialization_verification_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::VerifyOriginals(VerifyOriginals {
            id,
            checkpoint: IntegrityCheckpoint::PostMaterialization,
        })])
    }

    fn baseline_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::RunBaseline(RunProcess {
            id,
            context: crate::BaselineContext {
                worker: 0,
                run_id: self.run_id.clone(),
            },
            argv: self.config.test_argv.clone(),
            cwd: self.config.selection.root.clone(),
            limits: ProcessLimits {
                timeout: self.config.limits.baseline_timeout.get(),
                max_output_bytes: self.config.limits.max_output.get(),
                max_memory_bytes: self.config.limits.max_memory.get(),
                max_processes: self.max_processes()?,
            },
        })])
    }

    fn begin_session_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let fingerprint = self.fingerprint.ok_or(MachineError::MissingFingerprint)?;
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::BeginSession(BeginSession {
            id,
            run_id: self.run_id.clone(),
            fingerprint,
            max_mutants: self.config.limits.max_mutants,
        })])
    }

    fn start_run_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let id = self.allocate_id()?;
        let sequence = self.output_sequence();
        let mut run_started =
            RunStarted::minimal(self.run_id.clone(), sequence, self.resource_control.clone());
        run_started.normalized_config = Some(self.config.clone());
        run_started.resume = self.resume_outcome;
        run_started.verification_selection = self.selection.verification_selection().cloned();
        let output = EmitOutput {
            id,
            event: OutputEvent::RunStarted(run_started),
        };
        Ok(vec![
            self.register_output(output, OutputContinuation::RunStarted),
        ])
    }

    fn apply_worker_effects(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        let candidate = self.candidate(worker)?;
        *self.worker_mut(worker)? = WorkerState::Applying(candidate.clone());
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::ApplyMutation(ApplyMutation {
            id,
            worker,
            candidate,
        })])
    }

    fn candidate_effects(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        if let Some(run_id) = self.session_run_id.clone() {
            let candidate = self.candidate(worker)?;
            let mutant_id = candidate.id.clone();
            *self.worker_mut(worker)? = WorkerState::Lookup(candidate);
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
        *self.worker_mut(worker)? = WorkerState::Reading;
        let id = self.allocate_id()?;
        Ok(vec![RunEffect::ReadCandidate(ReadCandidate {
            id,
            worker,
            spool,
            cursor: self.candidate_cursor,
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
            .find_map(|(worker, state)| matches!(state, WorkerState::Idle).then_some(*worker))
    }

    fn candidate_read_pending(&self) -> bool {
        self.pending
            .values()
            .any(|pending| pending.kind() == CompletionKind::CandidateLoaded)
    }

    #[cfg(feature = "contracts")]
    fn worker_invariant(&self) -> bool {
        let jobs = self.config.limits.jobs.get();
        let worker_ids_are_bounded = self
            .workers
            .keys()
            .all(|worker| usize::try_from(*worker).is_ok_and(|worker| worker < jobs));
        let pending_workers_are_bounded = self.pending.values().all(|pending| {
            pending
                .worker
                .is_none_or(|worker| usize::try_from(worker).is_ok_and(|worker| worker < jobs))
        });
        let candidate_reads = self
            .pending
            .values()
            .filter(|pending| pending.kind() == CompletionKind::CandidateLoaded)
            .count();
        let running_processes = self
            .pending
            .values()
            .filter(|pending| pending.kind() == CompletionKind::MutantFinished)
            .count();

        self.workers.len() <= jobs
            && worker_ids_are_bounded
            && pending_workers_are_bounded
            && candidate_reads <= 1
            && running_processes <= jobs
    }

    fn schedule_read_or_finalize(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        if self.selection.ordered_state().is_some() {
            if self.selection.ordered_state().is_some_and(|ordered| {
                matches!(ordered.phase, OrderedCandidatePhase::Collecting { .. })
            }) {
                if !self.candidate_read_pending()
                    && let Some(worker) = self.idle_worker()
                {
                    return self.read_next_candidate(worker);
                }
                return Ok(Vec::new());
            }

            let mut effects = Vec::new();
            while let Some(worker) = self.idle_worker() {
                let Some(candidate) = self.selection.ordered_state_mut().and_then(|ordered| {
                    match &mut ordered.phase {
                        OrderedCandidatePhase::Ready { remaining } => remaining.pop_front(),
                        OrderedCandidatePhase::Collecting { .. } => None,
                    }
                }) else {
                    break;
                };
                *self.worker_mut(worker)? = WorkerState::Ready(candidate);
                if self.flags.scheduling.stop_requested {
                    effects
                        .extend(self.synthetic_started_output(worker, SyntheticOutcome::NotRun)?);
                } else if self.scheduled_mutants >= self.config.limits.max_mutants.get() as u64 {
                    self.flags.outcome.incomplete = true;
                    effects
                        .extend(self.synthetic_started_output(worker, SyntheticOutcome::NotRun)?);
                } else {
                    self.scheduled_mutants += 1;
                    effects.extend(self.candidate_effects(worker)?);
                }
            }
            if self.selection.ordered_state().is_some_and(|ordered| {
                matches!(
                    &ordered.phase,
                    OrderedCandidatePhase::Ready { remaining } if remaining.is_empty()
                )
            }) && self
                .workers
                .values()
                .all(|worker| matches!(worker, WorkerState::Idle))
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
                .all(|worker| matches!(worker, WorkerState::Idle))
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
                .selection
                .candidate_ids()
                .is_some_and(|filter| filter.contains(&candidate.id))
            {
                let ordered = self
                    .selection
                    .ordered_state_mut()
                    .expect("ordered collection requires ordered state");
                let OrderedCandidatePhase::Collecting { discovered } = &mut ordered.phase else {
                    unreachable!("ordered collection requires the collecting phase");
                };
                discovered.insert(candidate.id.clone(), candidate);
            }
            return self.schedule_read_or_finalize();
        }

        self.ensure_selected_candidates_discovered()?;
        let ordered = self
            .selection
            .ordered_state_mut()
            .expect("ordered collection requires ordered state");
        ordered.phase = OrderedCandidatePhase::Ready {
            remaining: ordered.take_remaining(),
        };
        self.schedule_read_or_finalize()
    }

    fn ensure_selected_candidates_discovered(&self) -> Result<(), MachineError> {
        let missing = self.selection.missing_id();
        if let Some(candidate_id) = missing {
            return Err(MachineError::SelectedCandidateMissing(candidate_id.clone()));
        }
        Ok(())
    }

    fn candidate(&self, worker: u32) -> Result<MutationCandidate, MachineError> {
        self.workers
            .get(&worker)
            .and_then(WorkerState::candidate)
            .cloned()
            .ok_or(MachineError::WrongPhase { phase: self.phase })
    }

    fn started_output(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        let candidate = self.candidate(worker)?;
        *self.worker_mut(worker)? = WorkerState::Starting(candidate.clone());
        let id = self.allocate_id()?;
        let output = EmitOutput {
            id,
            event: OutputEvent::MutantStarted(MutantStarted::new(
                self.run_id.clone(),
                self.output_sequence(),
                candidate.id,
                candidate.sequence,
            )),
        };
        Ok(vec![self.register_output(
            output,
            OutputContinuation::StartMutant(worker),
        )])
    }

    fn synthetic_started_output(
        &mut self,
        worker: u32,
        outcome: SyntheticOutcome,
    ) -> Result<Vec<RunEffect>, MachineError> {
        let candidate = self.candidate(worker)?;
        *self.worker_mut(worker)? = WorkerState::SyntheticStarting(candidate.clone(), outcome);
        let id = self.allocate_id()?;
        let output = EmitOutput {
            id,
            event: OutputEvent::MutantStarted(MutantStarted::new(
                self.run_id.clone(),
                self.output_sequence(),
                candidate.id,
                candidate.sequence,
            )),
        };
        Ok(vec![self.register_output(
            output,
            OutputContinuation::StartSynthetic(worker),
        )])
    }

    fn max_processes(&self) -> Result<u32, MachineError> {
        u32::try_from(self.config.limits.max_processes.get())
            .map_err(|_| MachineError::ProcessCountOverflow)
    }
    fn mutant_process(&mut self, worker: u32) -> Result<Vec<RunEffect>, MachineError> {
        let candidate = self.candidate(worker)?;
        *self.worker_mut(worker)? = WorkerState::Running(candidate.clone());
        let id = self.allocate_id()?;
        let timeout = match self.config.limits.mutant_timeout {
            MutantTimeout::Auto => auto_mutant_timeout(self.baseline_elapsed),
            MutantTimeout::Fixed(value) => value.get(),
        };
        Ok(vec![RunEffect::RunMutant(RunProcess {
            id,
            context: crate::MutantContext {
                worker,
                run_id: self.run_id.clone(),
                mutant_id: candidate.id,
            },
            argv: self.config.test_argv.clone(),
            cwd: self.config.selection.root.clone(),
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
        result: MutantResult,
    ) -> Result<Vec<RunEffect>, MachineError> {
        *self.worker_mut(worker)? = WorkerState::Finishing(result.clone());
        let id = self.allocate_id()?;
        let output = EmitOutput {
            id,
            event: OutputEvent::MutantFinished(MutantOutput {
                schema_version: crate::REPORT_SCHEMA_VERSION,
                sequence: self.output_sequence(),
                run_id: self.run_id.clone(),
                candidate: result.candidate,
                outcome: result.outcome,
                resource_mode: result.resource_mode,
                diagnostics: result.diagnostics,
            }),
        };
        Ok(vec![self.register_output(
            output,
            OutputContinuation::FinishMutant(worker),
        )])
    }

    fn synthetic_finished_output(
        &mut self,
        worker: u32,
        outcome: SyntheticOutcome,
    ) -> Result<Vec<RunEffect>, MachineError> {
        let candidate = self.candidate(worker)?;
        *self.worker_mut(worker)? = WorkerState::SyntheticFinishing(candidate.clone(), outcome);
        let id = self.allocate_id()?;
        let output = EmitOutput {
            id,
            event: OutputEvent::MutantFinished(MutantOutput {
                schema_version: crate::REPORT_SCHEMA_VERSION,
                sequence: self.output_sequence(),
                run_id: self.run_id.clone(),
                candidate,
                outcome: outcome.report_outcome(),
                resource_mode: self.resource_control.mode,
                diagnostics: Vec::new(),
            }),
        };
        Ok(vec![self.register_output(
            output,
            OutputContinuation::FinishSynthetic(worker),
        )])
    }

    /// Stops dispatch and retires pending work before draining known candidates or
    /// finalizing. Callers record the stop cause; terminal stops are filtered by transition.
    fn begin_stop(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        self.flags.scheduling.stop_requested = true;
        let was_mutating = self.phase == RunPhase::Mutants;
        self.retire_pending();
        if was_mutating {
            self.begin_stopped_mutant_drain()
        } else if !self.flags.report.report_started {
            self.flags.report.stop_after_run_started = true;
            self.start_run_effects()
        } else if self.copy_grant.is_some() {
            self.phase = RunPhase::Finalize;
            self.finalize_effects()
        } else {
            self.cleanup_effects()
        }
    }

    fn begin_stopped_mutant_drain(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        let mut stopped: Vec<_> = self.stopped_candidates.drain(..).collect();
        for worker in self.workers.values_mut() {
            let stopped_candidate = match std::mem::take(worker) {
                WorkerState::Ready(candidate)
                | WorkerState::Lookup(candidate)
                | WorkerState::Applying(candidate)
                | WorkerState::Starting(candidate) => Some(StoppedCandidate::NotStarted(candidate)),
                WorkerState::SyntheticStarting(candidate, outcome) => {
                    Some(StoppedCandidate::SyntheticNotStarted(candidate, outcome))
                }
                WorkerState::Running(candidate) => Some(StoppedCandidate::StartedNotRun(candidate)),
                WorkerState::Persisting(result) => {
                    self.summary.record(result.outcome.status());
                    Some(StoppedCandidate::Finished(result))
                }
                WorkerState::Finishing(result) => Some(StoppedCandidate::Finished(result)),
                WorkerState::SyntheticFinishing(candidate, outcome) => {
                    Some(StoppedCandidate::SyntheticFinished(candidate, outcome))
                }
                WorkerState::Idle | WorkerState::Reading | WorkerState::Resetting => None,
            };
            if let Some(candidate) = stopped_candidate {
                stopped.push(candidate);
            }
        }
        if let Some(ordered) = self.selection.ordered_state_mut() {
            stopped.extend(
                ordered
                    .take_remaining()
                    .into_iter()
                    .map(StoppedCandidate::NotStarted),
            );
            self.flags.scheduling.candidate_exhausted = true;
        }
        // The selection owns request order; borrow one temporary index for this merge.
        let requested_ranks: Option<HashMap<&str, usize>> =
            self.selection.ordered_ids().map(|ids| {
                ids.iter()
                    .enumerate()
                    .map(|(rank, id)| {
                        #[cfg(test)]
                        stop_tests::record_requested_id_visit();
                        (id.as_str(), rank)
                    })
                    .collect()
            });
        stopped.sort_by_key(|candidate| {
            #[cfg(test)]
            stop_tests::record_rank_query();
            let candidate = match candidate {
                StoppedCandidate::NotStarted(candidate)
                | StoppedCandidate::SyntheticNotStarted(candidate, _)
                | StoppedCandidate::StartedNotRun(candidate)
                | StoppedCandidate::SyntheticFinished(candidate, _) => candidate,
                StoppedCandidate::Finished(result) => &result.candidate,
            };
            requested_ranks
                .as_ref()
                .and_then(|ranks| ranks.get(candidate.id.as_str()).copied())
                .unwrap_or_else(|| usize::try_from(candidate.sequence).unwrap_or(usize::MAX))
        });
        self.stopped_candidates = stopped.into();
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
                    *self.worker_mut(worker)? = WorkerState::Ready(candidate);
                    self.synthetic_started_output(worker, SyntheticOutcome::NotRun)
                }
                StoppedCandidate::SyntheticNotStarted(candidate, outcome) => {
                    *self.worker_mut(worker)? = WorkerState::Ready(candidate);
                    self.synthetic_started_output(worker, outcome)
                }
                StoppedCandidate::StartedNotRun(candidate) => {
                    *self.worker_mut(worker)? = WorkerState::Ready(candidate);
                    self.synthetic_finished_output(worker, SyntheticOutcome::NotRun)
                }
                StoppedCandidate::Finished(result) => self.finished_output(worker, result),
                StoppedCandidate::SyntheticFinished(candidate, outcome) => {
                    *self.worker_mut(worker)? = WorkerState::Ready(candidate);
                    self.synthetic_finished_output(worker, outcome)
                }
            }
        } else if !self.flags.scheduling.candidate_exhausted {
            self.read_next_candidate(worker)
        } else if let Some(failed) = self.pending_failure.take() {
            self.diagnostic_effect(&failed)
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
        if self.flags.report.delivery == ReportDelivery::Failed {
            self.phase = RunPhase::Finished;
            return Ok(Vec::new());
        }
        let id = self.allocate_id()?;
        let output = EmitOutput {
            id,
            event: OutputEvent::RunFinished(RunSummary {
                schema_version: crate::REPORT_SCHEMA_VERSION,
                sequence: self.output_sequence(),
                run_id: self.run_id.clone(),
                counts: self.summary.clone(),
                complete: self.complete(),
                exit_code: self.exit_code(),
                disk: self.disk_summary.clone(),
                verification_selection: self.selection.verification_selection().cloned(),
            }),
        };
        Ok(vec![
            self.register_output(output, OutputContinuation::RunFinished),
        ])
    }

    fn post_cleanup_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        self.phase = RunPhase::Finalize;
        self.final_report_effects()
    }

    fn post_report_effects(&mut self) -> Result<Vec<RunEffect>, MachineError> {
        if let Some(run_id) = self.session_run_id.clone()
            && !self.flags.cleanup.session_finish_attempted
        {
            self.flags.cleanup.session_finish_attempted = true;
            let id = self.allocate_id()?;
            return Ok(vec![RunEffect::FinishSession(FinishSession {
                id,
                run_id,
                complete: self.complete(),
            })]);
        }
        self.phase = RunPhase::Finished;
        Ok(Vec::new())
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
    #[error("cleanup {id:?} released {received:?}, expected exactly {expected:?}")]
    CleanupReservationMismatch {
        id: EffectId,
        expected: Vec<ReservationId>,
        received: Vec<ReservationId>,
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
            Self::CleanupReservationMismatch { .. } => "machine.cleanup.reservation_mismatch",
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
    let is_stop = matches!(
        event,
        RunEvent::DeadlineReached
            | RunEvent::CancellationRequested
            | RunEvent::DiskStopRequested(_)
    );
    let final_report_pending = state
        .pending
        .values()
        .any(|pending| pending.output_continuation() == Some(OutputContinuation::RunFinished));
    if is_stop
        && (final_report_pending
            || state.flags.cleanup.session_finish_attempted
            || state.phase == RunPhase::Finished
            || state.phase == RunPhase::Cleaning)
    {
        return Ok((state, Vec::new()));
    }
    #[cfg(feature = "contracts")]
    let was_fatal = state.flags.outcome.infrastructure_error;
    let completed = state.accept_completion(&event)?;
    let completed_kind = completed.as_ref().map(PendingEffect::kind);
    let completed_output = completed
        .as_ref()
        .and_then(PendingEffect::output_continuation);
    let completed_worker = completed.and_then(|pending| pending.worker);
    if let RunEvent::EffectFailed(failed) = &event
        && let Some(stop) = &mut state.disk_summary.stop
    {
        let secondary = crate::DiskSecondary::Error {
            code: failed.failure.code().to_owned(),
            message: failed.failure.message(),
        };
        let matches_primary = matches!(
            &secondary,
            crate::DiskSecondary::Error { code, message }
                if stop.code == *code && stop.message.as_ref() == Some(message)
        );
        if !matches_primary && !stop.secondary.contains(&secondary) {
            stop.secondary.push(secondary);
        }
    }
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
            match value.outcome {
                crate::SessionLoadOutcome::Resumed(resume) => {
                    state.resume_outcome = Some(crate::ResumeOutcome::Resumed);
                    state.run_id.clone_from(&resume.run_id);
                    state.session_run_id = Some(resume.run_id);
                    state.start_run_effects()?
                }
                crate::SessionLoadOutcome::Fresh(reason) => {
                    state.resume_outcome = Some(crate::ResumeOutcome::Fresh { reason });
                    state.begin_session_effects()?
                }
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
            match &state.config.session {
                crate::SessionMode::Disabled => state.start_run_effects()?,
                crate::SessionMode::Fresh(_) => state.begin_session_effects()?,
                crate::SessionMode::Resume(_) => {
                    let fingerprint = state.fingerprint.ok_or(MachineError::MissingFingerprint)?;
                    let id = state.allocate_id()?;
                    vec![RunEffect::LoadSession(LoadSession {
                        id,
                        fingerprint,
                        max_mutants: state.config.limits.max_mutants,
                    })]
                }
            }
        }
        RunEvent::WorkerCreated(value) if state.phase == RunPhase::Copy => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            state.workers.entry(value.worker).or_default();
            if state.workers.len() == state.config.limits.jobs.get() {
                state.phase = RunPhase::MaterializationVerification;
                state.materialization_verification_effects()?
            } else {
                Vec::new()
            }
        }
        RunEvent::OriginalsVerified(value)
            if state.phase == RunPhase::MaterializationVerification
                && value.checkpoint == IntegrityCheckpoint::PostMaterialization =>
        {
            state.phase = RunPhase::Baseline;
            state.baseline_effects()?
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
            let output = EmitOutput {
                id: output_id,
                event: output,
            };
            let mut effects = vec![state.register_output(output, OutputContinuation::Baseline)];
            if success {
                state.phase = RunPhase::BaselineRestoration;
                *state.worker_mut(0)? = WorkerState::Resetting;
                let id = state.allocate_id()?;
                effects.push(RunEffect::ResetWorker(ResetWorker { id, worker: 0 }));
            } else {
                state.flags.outcome.baseline_failed = true;
                state.phase = RunPhase::Finalize;
                effects.extend(state.finalize_effects()?);
            }
            effects
        }
        RunEvent::RemainingBudgetObserved(value) if state.phase == RunPhase::BudgetCheck => {
            let selection = state
                .selection
                .verification_selection()
                .ok_or(MachineError::WrongPhase { phase: state.phase })?;
            let projection = project_top_budget(
                selection.selected(),
                state.config.limits.jobs,
                state.config.limits.total_timeout.get(),
                state.baseline_elapsed,
                state.config.limits.mutant_timeout,
                value.remaining,
            );
            if projection.is_shortfall() {
                let id = state.allocate_id()?;
                let output = EmitOutput {
                    id,
                    event: OutputEvent::Diagnostic(Diagnostic::new(
                        state.run_id.clone(),
                        state.output_sequence(),
                        "warning",
                        "budget.projected_shortfall",
                        format!(
                            "verify budget projection: selected={}, jobs={}, \
                             planned_total_timeout={}s, baseline={}s, \
                             effective_mutant_timeout={}s, remaining={}s, \
                             projected_capacity={}s. This is not a guaranteed failure; \
                             create a new plan with increased --jobs and/or \
                             --total-timeout.",
                            projection.selected,
                            projection.jobs,
                            projection.planned_total_timeout.as_secs(),
                            projection.baseline.as_secs(),
                            projection.effective_mutant_timeout.as_secs(),
                            projection.remaining.as_secs(),
                            projection.projected_capacity.as_secs(),
                        ),
                    )),
                };
                vec![state.register_output(output, OutputContinuation::ContinueAfterBudgetWarning)]
            } else {
                state.phase = RunPhase::Analyze;
                state.analyze_next()?
            }
        }
        RunEvent::AnalysisFinished(mut value) if state.phase == RunPhase::Analyze => {
            if value.diagnostics.is_empty() {
                state.apply_analysis_finished(value)?
            } else {
                state.flags.outcome.incomplete = true;
                state.analysis_diagnostics = std::mem::take(&mut value.diagnostics).into();
                state.pending_analysis = Some(value);
                state.analysis_diagnostic_effects()?
            }
        }
        RunEvent::CandidateLoaded(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            let worker = value.worker;
            state.candidate_cursor = value.next_cursor;
            if state.selection.ordered_state().is_some_and(|ordered| {
                matches!(ordered.phase, OrderedCandidatePhase::Collecting { .. })
            }) {
                state.collect_ordered_candidate(worker, value.candidate)?
            } else {
                if value.candidate.is_none() {
                    state.ensure_selected_candidates_discovered()?;
                }
                let selected = value
                    .candidate
                    .as_ref()
                    .is_none_or(|candidate| state.selection.select_streamed(&candidate.id));
                if !selected {
                    *state.worker_mut(worker)? = WorkerState::default();
                    state.schedule_read_or_finalize()?
                } else if state.flags.scheduling.stop_requested {
                    if let Some(candidate) = value.candidate {
                        *state.worker_mut(worker)? = WorkerState::Ready(candidate);
                        state.synthetic_started_output(worker, SyntheticOutcome::NotRun)?
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
                            matches!(state.worker_mut(worker)?, WorkerState::Reading),
                            (worker, &candidate)
                        );
                        *state.worker_mut(worker)? = WorkerState::Ready(candidate);
                        if state.scheduled_mutants >= state.config.limits.max_mutants.get() as u64 {
                            state.flags.outcome.incomplete = true;
                            state.synthetic_started_output(worker, SyntheticOutcome::NotRun)?
                        } else {
                            state.scheduled_mutants += 1;
                            state.candidate_effects(worker)?
                        }
                    } else {
                        *state.worker_mut(worker)? = WorkerState::Idle;
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
                ResumeDecision::Reuse(status) => {
                    state.synthetic_started_output(worker, SyntheticOutcome::Reused(status))?
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
            let diagnostics = match value.output_state {
                ProcessOutputState::Complete => Vec::new(),
                ProcessOutputState::CloseTimedOut => vec![SessionDiagnostic {
                    mutant_id: candidate.id.clone(),
                    level: "error".to_owned(),
                    code: PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE.to_owned(),
                    message:
                        "process termination was classified, but output collection timed out; \
                              captured output may be incomplete"
                            .to_owned(),
                }],
            };
            let result = MutantResult {
                run_id: state
                    .session_run_id
                    .clone()
                    .unwrap_or_else(|| state.run_id.clone()),
                candidate,
                outcome: MutationOutcome::Executed {
                    termination: value.termination,
                    output_state: value.output_state,
                    output: value.output,
                    elapsed: value.elapsed,
                },
                resource_mode: value.resource_mode,
                diagnostics,
            };
            let status = result.outcome.status();
            if state.session_run_id.is_some() {
                *state.worker_mut(worker)? = WorkerState::Persisting(result.clone());
                let id = state.allocate_id()?;
                vec![RunEffect::PersistResult(PersistResult {
                    id,
                    worker,
                    result,
                })]
            } else {
                state.summary.record(status);
                state.finished_output(worker, result)?
            }
        }
        RunEvent::ResultPersisted(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            let worker = value.worker;
            let WorkerState::Persisting(result) = std::mem::take(state.worker_mut(worker)?) else {
                return Err(MachineError::WrongPhase { phase: state.phase });
            };
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
            state.summary.record(result.outcome.status());
            state.finished_output(worker, result)?
        }
        RunEvent::OutputEmitted(_) => {
            match completed_output.ok_or(MachineError::WrongPhase { phase: state.phase })? {
                OutputContinuation::Baseline => Vec::new(),
                OutputContinuation::RunStarted => {
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
                OutputContinuation::RunFinished => state.post_report_effects()?,
                OutputContinuation::Diagnostic => {
                    if state.flags.cleanup.cleanup_done {
                        state.post_cleanup_effects()?
                    } else {
                        state.cleanup_effects()?
                    }
                }
                OutputContinuation::ContinueAfterBudgetWarning => {
                    state.phase = RunPhase::Analyze;
                    state.analyze_next()?
                }
                OutputContinuation::ContinueAfterAnalysisDiagnostic => {
                    state.analysis_diagnostic_effects()?
                }
                OutputContinuation::StartMutant(worker) => state.mutant_process(worker)?,
                OutputContinuation::StartSynthetic(worker) => {
                    let WorkerState::SyntheticStarting(_, outcome) = state.worker_mut(worker)?
                    else {
                        return Err(MachineError::WrongPhase { phase: state.phase });
                    };
                    let outcome = *outcome;
                    state.synthetic_finished_output(worker, outcome)?
                }
                OutputContinuation::FinishMutant(worker) => {
                    if state.flags.scheduling.stop_requested {
                        *state.worker_mut(worker)? = WorkerState::default();
                        state.next_stopped_candidate_effects()?
                    } else {
                        *state.worker_mut(worker)? = WorkerState::Resetting;
                        let id = state.allocate_id()?;
                        vec![RunEffect::ResetWorker(ResetWorker { id, worker })]
                    }
                }
                OutputContinuation::FinishSynthetic(worker) => {
                    let WorkerState::SyntheticFinishing(_, outcome) = state.worker_mut(worker)?
                    else {
                        return Err(MachineError::WrongPhase { phase: state.phase });
                    };
                    let status = outcome.report_outcome().status();
                    state.summary.record(status);
                    *state.worker_mut(worker)? = WorkerState::Idle;
                    if state.flags.scheduling.stop_requested {
                        state.next_stopped_candidate_effects()?
                    } else {
                        state.schedule_read_or_finalize()?
                    }
                }
            }
        }
        RunEvent::WorkerReset(value) if state.phase == RunPhase::BaselineRestoration => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            *state.worker_mut(value.worker)? = WorkerState::Idle;
            if state.selection.ordered_state().is_some() {
                state.phase = RunPhase::BudgetCheck;
                let id = state.allocate_id()?;
                vec![RunEffect::ObserveRemainingBudget(ObserveRemainingBudget {
                    id,
                })]
            } else {
                state.phase = RunPhase::Analyze;
                state.analyze_next()?
            }
        }
        RunEvent::WorkerReset(value) if state.phase == RunPhase::Mutants => {
            if completed_worker != Some(value.worker) {
                return Err(MachineError::UnknownWorker(value.worker));
            }
            *state.worker_mut(value.worker)? = WorkerState::Idle;
            state.schedule_read_or_finalize()?
        }
        RunEvent::OriginalsVerified(_) if state.phase == RunPhase::Finalize => {
            state.cleanup_effects()?
        }
        RunEvent::SessionFinished(_) if state.phase == RunPhase::Finalize => {
            state.phase = RunPhase::Finished;
            Vec::new()
        }
        RunEvent::CleanupFinished(value) if state.phase == RunPhase::Cleaning => {
            release_workspace_copy(&mut state.budgets, &value)
                .map_err(|error| MachineError::Budget(error.to_string()))?;
            state.copy_grant = None;
            state.flags.cleanup.cleanup_done = true;
            state.disk_summary.cleanup.push(crate::DiskCleanupReport {
                root_id: "execution".into(),
                owner: "hoimin".into(),
                status: crate::DiskCleanupStatus::Clean,
                examined_entries: 0,
                removed_entries: 0,
                details: Vec::new(),
                omitted_detail_count: 0,
                remaining_root: None,
            });
            state.post_cleanup_effects()?
        }
        RunEvent::EffectFailed(failed) if state.phase == RunPhase::Cleaning => {
            let cleanup_status = if failed.failure.code() == crate::WORKSPACE_CLEANUP_DEFERRED {
                crate::DiskCleanupStatus::Deferred
            } else {
                crate::DiskCleanupStatus::Failed
            };
            state.flags.outcome.infrastructure_error = true;
            state.flags.scheduling.stop_requested = true;
            state.retire_pending();
            state.copy_grant = None;
            state.flags.cleanup.cleanup_done = true;
            state.disk_summary.cleanup.push(crate::DiskCleanupReport {
                root_id: "execution".into(),
                owner: "hoimin".into(),
                status: cleanup_status,
                examined_entries: 0,
                removed_entries: 0,
                details: vec![failed.failure.message()],
                omitted_detail_count: 0,
                remaining_root: None,
            });
            state.diagnostic_effect(&failed)?
        }
        RunEvent::EffectFailed(failed)
            if completed_output == Some(OutputContinuation::Diagnostic) =>
        {
            state.flags.outcome.infrastructure_error = true;
            state.flags.scheduling.stop_requested = true;
            if is_report_failure(&failed.failure) {
                state.flags.report.delivery = ReportDelivery::Failed;
            }
            state.retire_pending();
            if state.flags.cleanup.cleanup_done {
                state.post_cleanup_effects()?
            } else {
                state.cleanup_effects()?
            }
        }
        RunEvent::EffectFailed(_) if completed_kind == Some(CompletionKind::SessionFinished) => {
            state.flags.outcome.infrastructure_error = true;
            state.phase = RunPhase::Finished;
            Vec::new()
        }
        RunEvent::EffectFailed(failed) => {
            state.flags.outcome.infrastructure_error = true;
            state.flags.scheduling.stop_requested = true;
            let was_mutating = state.phase == RunPhase::Mutants;
            let output_delivery_failed = is_report_failure(&failed.failure);
            if output_delivery_failed {
                state.flags.report.delivery = ReportDelivery::Failed;
            }
            state.retire_pending();
            if was_mutating && !output_delivery_failed {
                state.flags.scheduling.candidate_exhausted = true;
                state.pending_failure = Some(failed);
                state.begin_stopped_mutant_drain()?
            } else if !state.flags.report.report_started {
                if completed_output == Some(OutputContinuation::RunStarted) {
                    state.cleanup_effects()?
                } else {
                    state.pending_failure = Some(failed);
                    state.start_run_effects()?
                }
            } else if final_report_pending {
                state.phase = RunPhase::Finished;
                Vec::new()
            } else {
                state.diagnostic_effect(&failed)?
            }
        }
        RunEvent::DeadlineReached => {
            state.flags.outcome.incomplete = true;
            state.begin_stop()?
        }
        RunEvent::DiskStopRequested(value) => {
            state.flags.outcome.infrastructure_error = true;
            state.flags.outcome.incomplete = true;
            if state.disk_summary.stop.is_none() {
                state.disk_summary.stop = Some(value.failure.into());
            }
            state.begin_stop()?
        }
        RunEvent::CancellationRequested => {
            state.flags.report.interrupted = true;
            state.begin_stop()?
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
            RunPhase::MaterializationVerification
                | RunPhase::Baseline
                | RunPhase::BaselineRestoration
                | RunPhase::BudgetCheck
                | RunPhase::Analyze
                | RunPhase::Mutants
        ) || state.copy_grant.is_some())
            && state.worker_invariant(),
        (
            &state.phase,
            &state.copy_grant,
            &state.workers,
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
                    pending.kind(),
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
        RunEffect::ObserveRemainingBudget(_) => CompletionKind::RemainingBudgetObserved,
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
        RunEffect::RunBaseline(value) => Some(value.context.worker),
        RunEffect::RunMutant(value) => Some(value.context.worker),
        RunEffect::ReadCandidate(value) => Some(value.worker),
        RunEffect::ApplyMutation(value) => Some(value.worker),
        RunEffect::ResetWorker(value) => Some(value.worker),
        RunEffect::LookupStoredResult(value) => Some(value.worker),
        RunEffect::PersistResult(value) => Some(value.worker),
        _ => None,
    }
}

fn is_report_failure(failure: &EffectFailure) -> bool {
    matches!(
        failure,
        EffectFailure::ReportIo { .. }
            | EffectFailure::ReportSerialization { .. }
            | EffectFailure::ReportState { .. }
    )
}

fn completion(event: &RunEvent) -> Option<(EffectId, CompletionKind)> {
    Some(match event {
        RunEvent::StartRequested(_)
        | RunEvent::DiskStopRequested(_)
        | RunEvent::DeadlineReached
        | RunEvent::CancellationRequested => return None,
        RunEvent::TargetsResolved(value) => (value.id, CompletionKind::TargetsResolved),
        RunEvent::PreflightCompleted(value) => (value.id, CompletionKind::PreflightCompleted),
        RunEvent::WorkerCreated(value) => (value.id, CompletionKind::WorkerCreated),
        RunEvent::BaselineFinished(value) => (value.id, CompletionKind::BaselineFinished),
        RunEvent::RemainingBudgetObserved(value) => {
            (value.id, CompletionKind::RemainingBudgetObserved)
        }
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

fn cleanup_reservations_match(expected: &[ReservationId], received: &[ReservationId]) -> bool {
    let expected_set = expected.iter().copied().collect::<BTreeSet<_>>();
    let received_set = received.iter().copied().collect::<BTreeSet<_>>();
    expected_set.len() == expected.len()
        && received_set.len() == received.len()
        && received_set == expected_set
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde::Deserialize;

    use super::{RunState, cleanup_reservations_match};
    use crate::{
        CommandArg, ExitPolicy, MutationStatus, OutputConfig, OutputEvent, RawRunConfig,
        RawRunLimits, ReservationId, RunConfig, RunEffect, exit_code_for, summarize,
    };

    #[test]
    fn output_registration_makes_ack_dispatch_available_immediately() {
        let mut state = RunState::new(
            "registration",
            audit_config(),
            crate::ResourceControl {
                mode: crate::ResourceMode::Hard,
                mechanism: "test".into(),
            },
        );
        let id = state.allocate_id().unwrap();
        let output = crate::EmitOutput {
            id,
            event: OutputEvent::Diagnostic(crate::Diagnostic::new(
                "registration",
                1,
                "warning",
                "test",
                "test",
            )),
        };

        let effect = state.register_output(output, super::OutputContinuation::Diagnostic);
        let (state, effects) = super::transition(
            state,
            crate::RunEvent::OutputEmitted(crate::OutputEmitted { id }),
        )
        .unwrap();

        let duplicate = super::transition(
            state,
            crate::RunEvent::OutputEmitted(crate::OutputEmitted { id }),
        );

        assert_eq!(effect.id(), id);
        assert!(matches!(effects.as_slice(), [RunEffect::Cleanup(_)]));
        assert!(
            matches!(duplicate, Err(super::MachineError::DuplicateEffect(duplicate)) if duplicate == id)
        );
    }

    #[test]
    fn cleanup_acknowledgement_requires_exact_set_without_duplicates() {
        let first = ReservationId(1);
        let second = ReservationId(2);
        let unexpected = ReservationId(3);

        let expected = [first, second];
        let cases = [
            (vec![second, first], true),
            (vec![], false),
            (vec![first], false),
            (vec![first, second, second], false),
            (vec![first, second, unexpected], false),
        ];

        for (received, accepted) in cases {
            let matches = cleanup_reservations_match(&expected, &received);

            assert_eq!(matches, accepted, "{received:?}");
        }
    }

    fn audit_config() -> RunConfig {
        RunConfig::try_from(RawRunConfig {
            root: ".".into(),
            sources: vec!["src".into()],
            test_argv: vec![CommandArg::Unix(b"python".to_vec())],
            limits: RawRunLimits {
                baseline_timeout: Duration::from_secs(1),
                ..RawRunLimits::default()
            },
            output: OutputConfig::default(),
            ..RawRunConfig::default()
        })
        .unwrap()
        .with_environment(crate::EnvironmentSnapshot::Untracked)
        .unwrap()
    }

    const MUTATION_SCORE_POLICY_CORPUS: &str =
        include_str!("../../../formal/HoiminOracle/corpus/mutation-score-exit-policy.jsonl");

    #[derive(Deserialize)]
    struct ComposedAuditCase {
        id: String,
        scenario: String,
        statuses: Vec<String>,
        run_flags: AuditRunFlags,
        expected_counts: AuditCounts,
        expected_score: Option<AuditFraction>,
        expected_policy: AuditPolicy,
        expected_complete: bool,
        expected_exit_code: i32,
    }

    #[derive(Deserialize)]
    #[allow(clippy::struct_excessive_bools)]
    struct AuditRunFlags {
        infrastructure_error: bool,
        baseline_failed: bool,
        incomplete: bool,
        interrupted: bool,
    }

    #[derive(Deserialize)]
    #[allow(clippy::struct_excessive_bools)]
    struct AuditPolicy {
        infrastructure_error: bool,
        baseline_failed: bool,
        incomplete: bool,
        survivors: bool,
        interrupted: bool,
    }

    #[derive(Deserialize)]
    struct AuditCounts {
        killed: u64,
        survived: u64,
        timeout: u64,
        out_of_memory: u64,
        process_limit: u64,
        error: u64,
        not_run: u64,
        inconclusive: u64,
    }

    #[derive(Deserialize)]
    struct AuditFraction {
        numerator: u64,
        denominator: u64,
    }

    fn audit_status(name: &str) -> MutationStatus {
        match name {
            "killed" => MutationStatus::Killed,
            "survived" => MutationStatus::Survived,
            "timeout" => MutationStatus::Timeout,
            "out_of_memory" => MutationStatus::OutOfMemory,
            "process_limit" => MutationStatus::ProcessLimit,
            "error" => MutationStatus::Error,
            "not_run" => MutationStatus::NotRun,
            _ => panic!("unexpected Lean audit status: {name}"),
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn audit_score(score: &AuditFraction) -> f64 {
        assert!(score.numerator <= 1_u64 << 53);
        assert!(score.denominator <= 1_u64 << 53);
        score.numerator as f64 / score.denominator as f64
    }

    #[test]
    fn lean_composed_rows_drive_run_state_and_public_run_finished() {
        let cases = MUTATION_SCORE_POLICY_CORPUS
            .lines()
            .map(|line| serde_json::from_str::<ComposedAuditCase>(line).unwrap())
            .filter(|case| case.scenario == "composed")
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 5);

        for case in cases {
            let statuses = case
                .statuses
                .iter()
                .map(|name| audit_status(name))
                .collect::<Vec<_>>();
            let mut state = RunState::new(
                "lean-policy-audit",
                audit_config(),
                crate::ResourceControl {
                    mode: crate::ResourceMode::Hard,
                    mechanism: "test_supplied_hard".into(),
                },
            );
            state.summary = summarize(&statuses);
            state.flags.outcome.infrastructure_error = case.run_flags.infrastructure_error;
            state.flags.outcome.baseline_failed = case.run_flags.baseline_failed;
            state.flags.outcome.incomplete = case.run_flags.incomplete;
            state.flags.report.interrupted = case.run_flags.interrupted;

            let expected_policy = ExitPolicy {
                infrastructure_error: case.expected_policy.infrastructure_error,
                baseline_failed: case.expected_policy.baseline_failed,
                incomplete: case.expected_policy.incomplete,
                survivors: case.expected_policy.survivors,
                interrupted: case.expected_policy.interrupted,
            };
            let policy = state.exit_policy();
            let complete = state.complete();
            let exit_code = state.exit_code();
            let effects = state.final_report_effects().unwrap();
            let RunEffect::EmitOutput(output) = &effects[0] else {
                panic!("expected final output effect");
            };
            let encoded = serde_json::to_string(&output.event).unwrap();
            let decoded: OutputEvent = serde_json::from_str(&encoded).unwrap();
            let OutputEvent::RunFinished(finished) = decoded else {
                panic!("expected public RunFinished event");
            };

            assert_eq!(policy, expected_policy, "{}", case.id);
            assert_eq!(complete, case.expected_complete, "{}", case.id);
            assert_eq!(exit_code, case.expected_exit_code, "{}", case.id);
            assert_eq!(
                finished.counts.killed, case.expected_counts.killed,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.survived, case.expected_counts.survived,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.timeout, case.expected_counts.timeout,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.out_of_memory, case.expected_counts.out_of_memory,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.process_limit, case.expected_counts.process_limit,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.error, case.expected_counts.error,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.not_run, case.expected_counts.not_run,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.inconclusive, case.expected_counts.inconclusive,
                "{}",
                case.id
            );
            assert_eq!(
                finished.counts.score.map(f64::to_bits),
                case.expected_score
                    .as_ref()
                    .map(audit_score)
                    .map(f64::to_bits),
                "{}",
                case.id
            );
            assert_eq!(finished.complete, case.expected_complete, "{}", case.id);
            assert_eq!(finished.exit_code, case.expected_exit_code, "{}", case.id);
        }
    }

    #[test]
    fn composed_exit_policy_matches_all_run_flag_combinations() {
        let booleans = [false, true];
        let status_sets = [
            vec![],
            vec![MutationStatus::Survived],
            vec![MutationStatus::Error],
            vec![MutationStatus::NotRun, MutationStatus::Survived],
        ];

        for statuses in status_sets {
            for infrastructure_error in booleans {
                for baseline_failed in booleans {
                    for incomplete in booleans {
                        for interrupted in booleans {
                            let mut state = RunState::new(
                                "lean-policy-audit",
                                audit_config(),
                                crate::ResourceControl {
                                    mode: crate::ResourceMode::Hard,
                                    mechanism: "test_supplied_hard".into(),
                                },
                            );
                            state.summary = summarize(&statuses);
                            state.flags.outcome.infrastructure_error = infrastructure_error;
                            state.flags.outcome.baseline_failed = baseline_failed;
                            state.flags.outcome.incomplete = incomplete;
                            state.flags.report.interrupted = interrupted;

                            let summary = ExitPolicy::from_summary(&state.summary);
                            let expected = ExitPolicy {
                                infrastructure_error: infrastructure_error
                                    || summary.infrastructure_error,
                                baseline_failed,
                                incomplete: incomplete || summary.incomplete,
                                survivors: summary.survivors,
                                interrupted,
                            };
                            let policy = state.exit_policy();
                            let exit_code = state.exit_code();
                            let complete = state.complete();
                            let effects = state.final_report_effects().unwrap();
                            let RunEffect::EmitOutput(output) = &effects[0] else {
                                panic!("expected final output effect");
                            };
                            let encoded = serde_json::to_string(&output.event).unwrap();
                            let decoded: OutputEvent = serde_json::from_str(&encoded).unwrap();
                            let OutputEvent::RunFinished(finished) = decoded else {
                                panic!("expected public RunFinished event");
                            };

                            assert_eq!(policy, expected);
                            assert_eq!(exit_code, exit_code_for(expected));
                            assert_eq!(
                                complete,
                                !expected.infrastructure_error
                                    && !expected.baseline_failed
                                    && !expected.incomplete
                                    && !expected.interrupted
                            );
                            assert_eq!(finished.counts, state.summary);
                            assert_eq!(finished.complete, complete);
                            assert_eq!(finished.exit_code, exit_code);
                        }
                    }
                }
            }
        }
    }
}
