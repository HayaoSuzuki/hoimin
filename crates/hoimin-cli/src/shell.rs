use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    CandidateLoaded, Diagnostic, EffectFailed, EffectId, EmitOutput, FingerprintInput,
    ObserveRemainingBudget, OutputEvent, RemainingBudgetObserved, ReportVersions, RunConfig,
    RunEffect, RunEvent, RunPhase, RunProcess, RunState, SourceHash, StartRequested, TargetSlice,
    fingerprint, transition,
};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::analyzer::{AnalyzerHandler, CandidateStore};
use crate::metrics::{MetricsCollector, MetricsError, finalize_metrics};
use crate::process::{ProcessCancellation, ProcessHandler, ProcessRequest, ProcessStartGate};
use crate::report::ReportHandler;
#[cfg(not(any(windows, target_os = "linux")))]
use crate::resource::PortableBackend;
use crate::resource::ResourceBackend;
use crate::session::SessionDispatcher;
use crate::target::TargetHandler;
use crate::workspace::{
    CopyOptions, WorkspaceHandler, WorkspaceManifest, WorkspaceTask, WorkspaceTaskCompletion,
};
#[cfg(test)]
use crate::workspace::{MaterializationPause, MaterializationPauseController};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownCause {
    TotalTimeout,
    Cancellation,
    Failure,
}

impl ShutdownCause {
    const fn label(self) -> &'static str {
        match self {
            Self::TotalTimeout => "total timeout",
            Self::Cancellation => "cancellation",
            Self::Failure => "failure",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShutdownBudget {
    cause: ShutdownCause,
    deadline: tokio::time::Instant,
}

impl ShutdownBudget {
    fn for_total_timeout_with_grace(run_deadline: tokio::time::Instant, grace: Duration) -> Self {
        Self {
            cause: ShutdownCause::TotalTimeout,
            deadline: run_deadline + grace,
        }
    }

    fn after_observation_with_grace(
        cause: ShutdownCause,
        observed_at: tokio::time::Instant,
        grace: Duration,
    ) -> Self {
        Self {
            cause,
            deadline: observed_at + grace,
        }
    }

    const fn cause(self) -> ShutdownCause {
        self.cause
    }

    const fn deadline(self) -> tokio::time::Instant {
        self.deadline
    }

    async fn wait<F: Future>(&self, future: F) -> Result<F::Output, tokio::time::error::Elapsed> {
        tokio::time::timeout_at(self.deadline(), future).await
    }

    fn expiry_error(self, process_tasks: usize, io_tasks: usize) -> String {
        format!(
            "{}: shutdown grace expired after {}s (process tasks: {process_tasks}, blocking I/O tasks: {io_tasks})",
            self.cause().label(),
            SHUTDOWN_GRACE.as_secs(),
        )
    }
}

fn establish_shutdown_budget(
    active: &mut Option<ShutdownBudget>,
    candidate: ShutdownBudget,
) -> ShutdownBudget {
    *active.get_or_insert(candidate)
}

fn establish_event_shutdown_budget(
    active: &mut Option<ShutdownBudget>,
    event: &RunEvent,
    run_deadline: tokio::time::Instant,
    observed_at: tokio::time::Instant,
    grace: Duration,
) -> Option<ShutdownBudget> {
    let candidate = match event {
        RunEvent::DeadlineReached => {
            ShutdownBudget::for_total_timeout_with_grace(run_deadline, grace)
        }
        RunEvent::CancellationRequested => ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            observed_at,
            grace,
        ),
        RunEvent::EffectFailed(_) => {
            ShutdownBudget::after_observation_with_grace(ShutdownCause::Failure, observed_at, grace)
        }
        _ => return None,
    };
    Some(establish_shutdown_budget(active, candidate))
}

fn ensure_outer_finalization_budget(
    active: &mut Option<ShutdownBudget>,
    run_failed: bool,
    run_deadline: tokio::time::Instant,
    observed_at: tokio::time::Instant,
    grace: Duration,
) -> ShutdownBudget {
    let candidate = if run_failed {
        ShutdownBudget::after_observation_with_grace(ShutdownCause::Failure, observed_at, grace)
    } else {
        ShutdownBudget::for_total_timeout_with_grace(run_deadline, grace)
    };
    establish_shutdown_budget(active, candidate)
}

async fn finish_with_interrupt_monitor<T>(
    _interrupts: crate::interrupt::InterruptMonitor,
    finalization: impl Future<Output = T>,
) -> T {
    finalization.await
}

#[derive(Clone, Debug)]
pub struct RunControl {
    request: ProcessStartGate,
    max_process_tasks: Arc<AtomicUsize>,
    max_completion_in_flight: Arc<AtomicUsize>,
    #[cfg(test)]
    materialization_pause: Option<MaterializationPause>,
    #[cfg(test)]
    shutdown_grace: Duration,
}

impl RunControl {
    #[must_use]
    pub fn new() -> Self {
        Self {
            request: ProcessStartGate::new(),
            max_process_tasks: Arc::new(AtomicUsize::new(0)),
            max_completion_in_flight: Arc::new(AtomicUsize::new(0)),
            #[cfg(test)]
            materialization_pause: None,
            #[cfg(test)]
            shutdown_grace: SHUTDOWN_GRACE,
        }
    }

    #[cfg(test)]
    fn with_materialization_pause(worker: u32) -> (Self, MaterializationPauseController) {
        let (pause, controller) = MaterializationPause::new(worker);
        let mut control = Self::new();
        control.materialization_pause = Some(pause);
        (control, controller)
    }

    #[cfg(test)]
    fn with_materialization_pause_and_shutdown_grace(
        worker: u32,
        shutdown_grace: Duration,
    ) -> (Self, MaterializationPauseController) {
        let (mut control, controller) = Self::with_materialization_pause(worker);
        control.shutdown_grace = shutdown_grace;
        (control, controller)
    }

    #[allow(
        clippy::unused_self,
        reason = "test builds read the per-run override; production always returns the fixed grace"
    )]
    fn shutdown_grace(&self) -> Duration {
        #[cfg(test)]
        {
            self.shutdown_grace
        }
        #[cfg(not(test))]
        {
            SHUTDOWN_GRACE
        }
    }

    pub fn cancel(&self) {
        self.request.cancel();
    }

    #[must_use]
    pub fn max_process_tasks(&self) -> usize {
        self.max_process_tasks.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn max_completion_in_flight(&self) -> usize {
        self.max_completion_in_flight.load(Ordering::Acquire)
    }

    fn observe_process_tasks(&self, value: usize) {
        self.max_process_tasks.fetch_max(value, Ordering::AcqRel);
    }

    fn observe_completion_in_flight(&self, value: usize) {
        self.max_completion_in_flight
            .fetch_max(value, Ordering::AcqRel);
    }

    async fn cancelled(&self) {
        self.request.cancelled().await;
    }

    fn is_cancelled(&self) -> bool {
        self.request.is_cancelled()
    }

    fn begin_dispatch(&self) -> Option<std::sync::MutexGuard<'_, ()>> {
        let guard = self.request.begin_spawn();
        if self.is_cancelled() {
            None
        } else {
            Some(guard)
        }
    }

    fn start_gate(&self) -> ProcessStartGate {
        self.request.clone()
    }
}

impl Default for RunControl {
    fn default() -> Self {
        Self::new()
    }
}

struct ShellCompletion {
    event: RunEvent,
    process_task: bool,
    io_task: bool,
    process: Option<(u32, bool)>,
    blocking: Option<Box<BlockingEffectCompletion>>,
}

enum BlockingEffect {
    Workspace(Box<WorkspaceTask>),
    Candidate(hoimin_core::ReadCandidate),
    Cleanup {
        id: EffectId,
        process: Arc<ProcessHandler>,
        workspace: Box<WorkspaceHandler>,
        request: hoimin_core::Cleanup,
    },
    #[cfg(test)]
    TestOperation {
        id: EffectId,
        operation: Box<dyn FnOnce() -> RunEvent + Send>,
    },
    #[cfg(test)]
    TestCompletion {
        id: EffectId,
        operation: Box<dyn FnOnce() -> BlockingEffectCompletion + Send>,
    },
}

enum BlockingEffectCompletion {
    Workspace(Box<WorkspaceTaskCompletion>),
    Candidate(Box<RunEvent>),
    Cleanup {
        id: EffectId,
        workspace: Box<WorkspaceHandler>,
        event: Box<RunEvent>,
    },
}

impl BlockingEffect {
    fn id(&self) -> EffectId {
        match self {
            Self::Workspace(task) => task.id(),
            Self::Candidate(request) => request.id,
            Self::Cleanup { id, .. } => *id,
            #[cfg(test)]
            Self::TestOperation { id, .. } => *id,
            #[cfg(test)]
            Self::TestCompletion { id, .. } => *id,
        }
    }

    fn execute(self) -> BlockingEffectCompletion {
        match self {
            Self::Workspace(task) => BlockingEffectCompletion::Workspace(Box::new(task.execute())),
            Self::Candidate(request) => {
                BlockingEffectCompletion::Candidate(Box::new(replay_candidate(&request)))
            }
            Self::Cleanup {
                id,
                process,
                mut workspace,
                request,
            } => {
                let event = match process.close() {
                    Ok(()) => workspace
                        .handle_cleanup(request)
                        .map_or_else(RunEvent::EffectFailed, RunEvent::CleanupFinished),
                    Err(error) => RunEvent::EffectFailed(EffectFailed::other(
                        id,
                        "process.resource.close",
                        error.to_string(),
                    )),
                };
                BlockingEffectCompletion::Cleanup {
                    id,
                    workspace,
                    event: Box::new(event),
                }
            }
            #[cfg(test)]
            Self::TestOperation { operation, .. } => {
                BlockingEffectCompletion::Candidate(Box::new(operation()))
            }
            #[cfg(test)]
            Self::TestCompletion { operation, .. } => operation(),
        }
    }
}

fn replay_candidate(request: &hoimin_core::ReadCandidate) -> RunEvent {
    match CandidateStore::replay_one(&request.spool, request.cursor) {
        Ok(Some((candidate, next_cursor))) => RunEvent::CandidateLoaded(CandidateLoaded {
            id: request.id,
            worker: request.worker,
            next_cursor,
            candidate: Some(candidate),
        }),
        Ok(None) => RunEvent::CandidateLoaded(CandidateLoaded {
            id: request.id,
            worker: request.worker,
            candidate: None,
            next_cursor: request.cursor,
        }),
        Err(error) => RunEvent::EffectFailed(EffectFailed::other(
            request.id,
            "candidate.replay",
            error.to_string(),
        )),
    }
}

fn is_blocking_io_effect(effect: &RunEffect) -> bool {
    matches!(
        effect,
        RunEffect::CreateWorker(_)
            | RunEffect::ReadCandidate(_)
            | RunEffect::ApplyMutation(_)
            | RunEffect::ResetWorker(_)
            | RunEffect::VerifyOriginals(_)
            | RunEffect::Cleanup(_)
    )
}

fn prepare_blocking_effect<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
) -> Result<BlockingEffect, EffectFailed>
where
    Stdout: Write,
    Stderr: Write,
{
    let task = match effect {
        RunEffect::CreateWorker(request) => context.workspace_mut().prepare_create_task(request),
        RunEffect::ReadCandidate(request) => return Ok(BlockingEffect::Candidate(request)),
        RunEffect::ApplyMutation(request) => {
            let candidate = request.candidate.clone();
            context.active_candidates.insert(request.worker, candidate);
            context.workspace_mut().prepare_apply_task(request)
        }
        RunEffect::ResetWorker(request) => context.workspace_mut().prepare_reset_task(request),
        RunEffect::VerifyOriginals(request) => context.workspace().prepare_verify_task(request),
        RunEffect::Cleanup(request) => {
            let id = request.id;
            let workspace = context.workspace.take().ok_or_else(|| {
                EffectFailed::other(
                    id,
                    "shell.blocking_io",
                    "workspace ownership is unavailable for cleanup",
                )
            })?;
            return Ok(BlockingEffect::Cleanup {
                id,
                process: Arc::clone(&context.process),
                workspace: Box::new(workspace),
                request,
            });
        }
        _ => unreachable!("non-blocking effect passed to blocking preparation"),
    };
    task.map(Box::new).map(BlockingEffect::Workspace)
}

fn accept_blocking_completion<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    completion: BlockingEffectCompletion,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    let event = match completion {
        BlockingEffectCompletion::Workspace(completion) => context
            .workspace_mut()
            .accept_task_completion(*completion)
            .unwrap_or_else(RunEvent::EffectFailed),
        BlockingEffectCompletion::Candidate(event) => *event,
        BlockingEffectCompletion::Cleanup {
            id,
            workspace,
            event,
        } => {
            if context.workspace.is_some() {
                return RunEvent::EffectFailed(EffectFailed::other(
                    id,
                    "shell.blocking_io",
                    "cleanup returned duplicate workspace ownership",
                ));
            }
            context.workspace = Some(*workspace);
            *event
        }
    };
    match &event {
        RunEvent::CandidateLoaded(value) => {
            if let Some(candidate) = &value.candidate {
                context
                    .active_candidates
                    .insert(value.worker, candidate.clone());
            } else {
                context.active_candidates.remove(&value.worker);
            }
        }
        RunEvent::WorkerReset(value) => {
            context.active_candidates.remove(&value.worker);
        }
        _ => {}
    }
    event
}

#[cfg(test)]
fn execute_direct_io_effect<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    let result = match effect {
        RunEffect::CreateWorker(request) => context
            .workspace_mut()
            .handle_create_worker(request)
            .map(RunEvent::WorkerCreated),
        RunEffect::ReadCandidate(request) => {
            return accept_blocking_completion(
                context,
                BlockingEffectCompletion::Candidate(Box::new(replay_candidate(&request))),
            );
        }
        RunEffect::ApplyMutation(request) => {
            let candidate = request.candidate.clone();
            context
                .active_candidates
                .insert(request.worker, candidate.clone());
            context
                .workspace_mut()
                .handle_apply_mutation(request, &candidate)
                .map(RunEvent::MutationApplied)
        }
        RunEffect::ResetWorker(request) => {
            let worker = request.worker;
            let result = context
                .workspace_mut()
                .handle_reset_worker(request)
                .map(RunEvent::WorkerReset);
            if result.is_ok() {
                context.active_candidates.remove(&worker);
            }
            result
        }
        RunEffect::VerifyOriginals(request) => context
            .workspace()
            .handle_verify_originals(request)
            .map(RunEvent::OriginalsVerified),
        _ => unreachable!("non-blocking effect passed to direct I/O execution"),
    };
    result.unwrap_or_else(RunEvent::EffectFailed)
}

fn remaining_budget_observed(
    request: &ObserveRemainingBudget,
    deadline: tokio::time::Instant,
    now: tokio::time::Instant,
) -> RemainingBudgetObserved {
    RemainingBudgetObserved {
        id: request.id,
        remaining: deadline.saturating_duration_since(now),
    }
}

async fn run_blocking_io<T>(
    id: EffectId,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, EffectFailed>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| EffectFailed::other(id, "shell.blocking_io", error.to_string()))
}

#[derive(Debug)]
enum OwnedBlockingError {
    Join(String),
    Expired(String),
}

async fn run_owned_blocking_until<T>(
    budget: &ShutdownBudget,
    before_start: OwnedStartHook,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, OwnedBlockingError>
where
    T: Send + 'static,
{
    let deadline = budget.deadline();
    let mut task = tokio::task::spawn_blocking(move || {
        before_start();
        (tokio::time::Instant::now() < deadline).then(operation)
    });
    match budget.wait(&mut task).await {
        Ok(Ok(Some(value))) => Ok(value),
        Ok(Ok(None)) => Err(OwnedBlockingError::Expired(budget.expiry_error(0, 0))),
        Ok(Err(error)) => Err(OwnedBlockingError::Join(format!(
            "blocking I/O task failed: {error}"
        ))),
        Err(_) => {
            task.abort();
            Err(OwnedBlockingError::Expired(budget.expiry_error(0, 1)))
        }
    }
}

pub struct ShellContext<Stdout, Stderr> {
    workspace: Option<WorkspaceHandler>,
    analyzer: AnalyzerHandler,
    process: Arc<ProcessHandler>,
    report: ReportHandler<Stdout, Stderr>,
    session: Option<SessionDispatcher>,
    session_path: Option<Utf8PathBuf>,
    active_candidates: BTreeMap<u32, hoimin_core::MutationCandidate>,
    _spool_dir: Arc<TempDir>,
    resolved_targets: Option<Vec<TargetSlice>>,
    config: RunConfig,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    report_versions: ReportVersions,
}

impl<Stdout, Stderr> ShellContext<Stdout, Stderr>
where
    Stdout: Write,
    Stderr: Write,
{
    /// Creates all handlers required for a run.
    ///
    /// # Errors
    ///
    /// Returns an error when local run infrastructure cannot be initialized.
    #[expect(
        clippy::unused_async,
        reason = "the constructor remains asynchronous for compatibility with the run infrastructure API"
    )]
    pub async fn new(config: &RunConfig, stdout: Stdout, stderr: Stderr) -> Result<Self, String> {
        let spool_dir = Arc::new(tempfile::tempdir().map_err(|error| error.to_string())?);
        let spool_path = Utf8PathBuf::from_path_buf(spool_dir.path().to_owned())
            .map_err(|_| "temporary spool path is not UTF-8".to_owned())?;
        std::fs::create_dir_all(spool_path.join("report")).map_err(|error| error.to_string())?;
        let source_roots = config.selection.sources.clone();
        let requested_workers = u32::try_from(config.limits.jobs.get())
            .map_err(|_| "--jobs exceeds the supported worker count".to_owned())?;
        let workspace = WorkspaceHandler::new(
            config.root.clone(),
            source_roots,
            requested_workers,
            CopyOptions {
                includes: config.selection.includes.clone(),
                excludes: config.selection.excludes.clone(),
            },
        );
        let backend = resource_backend(config).map_err(|error| error.to_string())?;
        let process = Arc::new(ProcessHandler::new(
            backend.clone(),
            spool_path.join("process"),
        ));
        let analyzer = AnalyzerHandler::with_backend(
            config.root.clone(),
            backend,
            config.limits.max_memory.get(),
            u32::try_from(config.limits.max_processes.get())
                .map_err(|_| "--max-processes exceeds the supported process count".to_owned())?,
        )
        .map_err(|error| error.to_string())?
        .with_candidate_spool_owner(spool_dir.clone());
        let report = ReportHandler::new(
            config.output.format,
            stdout,
            stderr,
            spool_path.join("report"),
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            workspace: Some(workspace),
            analyzer,
            process,
            report,
            session: None,
            session_path: config.session.as_ref().map(|value| value.path.clone()),
            active_candidates: BTreeMap::new(),
            _spool_dir: spool_dir,
            resolved_targets: None,
            config: config.clone(),
            fingerprint_copy_inputs: BTreeSet::new(),
            report_versions: ReportVersions {
                os: std::env::consts::OS.to_owned(),
                hoimin: env!("CARGO_PKG_VERSION").to_owned(),
            },
        })
    }

    fn workspace(&self) -> &WorkspaceHandler {
        self.workspace
            .as_ref()
            .expect("workspace ownership is available outside an owned blocking operation")
    }

    fn workspace_mut(&mut self) -> &mut WorkspaceHandler {
        self.workspace
            .as_mut()
            .expect("workspace ownership is available outside an owned blocking operation")
    }
}

async fn prepare_fingerprint<Stdout, Stderr>(
    context: &ShellContext<Stdout, Stderr>,
) -> Result<hoimin_core::RunFingerprint, EffectFailed> {
    let id = EffectId(0);
    let targets = context.resolved_targets.as_ref().ok_or_else(|| {
        EffectFailed::other(
            id,
            "shell.targets.missing",
            "preflight preceded target resolution",
        )
    })?;
    let mut sources = Vec::with_capacity(targets.len());
    for target in targets {
        let bytes = tokio::fs::read(context.config.root.join(&target.path))
            .await
            .map_err(|error| {
                EffectFailed::other(id, "fingerprint.source.read", error.to_string())
            })?;
        sources.push(SourceHash {
            path: target.path.clone(),
            hash: *blake3::hash(&bytes).as_bytes(),
        });
    }
    Ok(fingerprint(&FingerprintInput::from_config(
        &context.config,
        sources,
        targets.clone(),
        context.process.mode(),
    )))
}

fn recheck_fingerprint_inputs(
    config: &RunConfig,
    root: &Utf8Path,
    manifest: &WorkspaceManifest,
    copied_at_start: &BTreeSet<Utf8PathBuf>,
    id: EffectId,
) -> Result<(), EffectFailed> {
    crate::fingerprint_inputs::recheck(
        root,
        &config.fingerprint_includes,
        &config.fingerprint_files,
        &config.fingerprint_inputs,
    )
    .map_err(|error| {
        EffectFailed::other(id, "plan.fingerprint_input.changed", error.to_string())
    })?;
    crate::fingerprint_inputs::recheck_manifest(
        root,
        &config.fingerprint_includes,
        &config.fingerprint_files,
        &config.fingerprint_inputs,
        manifest,
        copied_at_start,
    )
    .map_err(|error| EffectFailed::other(id, "plan.fingerprint_input.changed", error.to_string()))
}

#[cfg(windows)]
fn resource_backend(config: &RunConfig) -> Result<ResourceBackend, crate::resource::ResourceError> {
    crate::resource::WindowsBackend::new(&config.limits).map(ResourceBackend::Windows)
}

#[cfg(target_os = "linux")]
fn resource_backend(config: &RunConfig) -> Result<ResourceBackend, crate::resource::ResourceError> {
    crate::resource::select_linux_backend(
        crate::resource::probe_linux_cgroup(&config.limits),
        config.allow_best_effort_memory,
    )
}

#[cfg(not(any(windows, target_os = "linux")))]
fn resource_backend(config: &RunConfig) -> Result<ResourceBackend, crate::resource::ResourceError> {
    PortableBackend::new(config.allow_best_effort_memory).map(ResourceBackend::Portable)
}

pub async fn execute_effect<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    if is_blocking_io_effect(&effect) {
        return match prepare_blocking_effect(context, effect) {
            Ok(task) => {
                let id = task.id();
                match run_blocking_io(id, move || task.execute()).await {
                    Ok(completion) => accept_blocking_completion(context, completion),
                    Err(error) => RunEvent::EffectFailed(error),
                }
            }
            Err(error) => RunEvent::EffectFailed(error),
        };
    }
    execute_effect_with_cancellation(context, effect, ProcessCancellation::new()).await
}

#[expect(
    clippy::too_many_lines,
    reason = "effect dispatch is intentionally centralized to preserve a one-to-one effect-to-event mapping"
)]
async fn execute_effect_with_cancellation<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
    cancellation: ProcessCancellation,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    debug_assert!(!is_blocking_io_effect(&effect));
    let id = effect.id();
    let result: Result<RunEvent, EffectFailed> = match effect {
        RunEffect::ResolveTargets(request) => match TargetHandler::handle(request).await {
            Ok(value) => {
                context.resolved_targets = Some(value.targets.clone());
                Ok(RunEvent::TargetsResolved(value))
            }
            Err(error) => Err(error),
        },
        RunEffect::Preflight(request) => {
            let config = &context.config;
            let copied_at_start = &context.fingerprint_copy_inputs;
            match context
                .workspace
                .as_mut()
                .expect("workspace ownership is available during preflight")
                .handle_preflight_validated(request, |root, manifest| {
                    recheck_fingerprint_inputs(config, root, manifest, copied_at_start, id)
                }) {
                Ok(mut value) => match prepare_fingerprint(context).await {
                    Ok(run_fingerprint) => {
                        value.fingerprint = Some(run_fingerprint);
                        Ok(RunEvent::PreflightCompleted(value))
                    }
                    Err(mut error) => {
                        error.id = value.id;
                        Err(error)
                    }
                },
                Err(error) => Err(error),
            }
        }
        RunEffect::CreateWorker(_)
        | RunEffect::ReadCandidate(_)
        | RunEffect::ApplyMutation(_)
        | RunEffect::ResetWorker(_)
        | RunEffect::VerifyOriginals(_) => {
            unreachable!("blocking effect bypassed owned dispatch")
        }
        RunEffect::RunBaseline(request) => {
            match worker_process_request(context, id, request, cancellation.clone(), None) {
                Ok(request) => context
                    .process
                    .run(request)
                    .await
                    .map(RunEvent::BaselineFinished),
                Err(error) => Err(error),
            }
        }
        RunEffect::AnalyzeFile(request) => context
            .analyzer
            .handle_with_cancellation(
                request,
                &context.config.operators,
                context.config.profile,
                cancellation.clone(),
            )
            .await
            .map(RunEvent::AnalysisFinished),
        RunEffect::RunMutant(request) => {
            match worker_process_request(context, id, request, cancellation.clone(), None) {
                Ok(request) => context
                    .process
                    .run(request)
                    .await
                    .map(RunEvent::MutantFinished),
                Err(error) => Err(error),
            }
        }
        RunEffect::ObserveRemainingBudget(_) => Err(EffectFailed::other(
            id,
            "shell.budget.scheduler",
            "remaining budget observation must run in the scheduler",
        )),
        RunEffect::EmitOutput(mut request) => {
            if let OutputEvent::RunStarted(run_started) = &mut request.event {
                run_started.versions = context.report_versions.clone();
            }
            context.report.handle(request).map(RunEvent::OutputEmitted)
        }
        RunEffect::Cleanup(_) => unreachable!("cleanup bypassed owned blocking dispatch"),
        RunEffect::LoadSession(request) => match session(context, id).await {
            Ok(handler) => handler.load(request).await.map(RunEvent::SessionLoaded),
            Err(error) => Err(error),
        },
        RunEffect::LookupStoredResult(request) => match session(context, id).await {
            Ok(handler) => handler
                .lookup(request)
                .await
                .map(RunEvent::StoredResultLoaded),
            Err(error) => Err(error),
        },
        RunEffect::BeginSession(request) => match session(context, id).await {
            Ok(handler) => handler.begin(request).await.map(RunEvent::SessionStarted),
            Err(error) => Err(error),
        },
        RunEffect::PersistResult(request) => match session(context, id).await {
            Ok(handler) => handler
                .persist(request)
                .await
                .map(RunEvent::ResultPersisted),
            Err(error) => Err(error),
        },
        RunEffect::FinishSession(request) => match session(context, id).await {
            Ok(handler) => handler.finish(request).await.map(RunEvent::SessionFinished),
            Err(error) => Err(error),
        },
    };
    result.unwrap_or_else(RunEvent::EffectFailed)
}

fn worker_process_request<Stdout, Stderr>(
    context: &ShellContext<Stdout, Stderr>,
    id: EffectId,
    mut request: RunProcess,
    cancellation: ProcessCancellation,
    start_gate: Option<ProcessStartGate>,
) -> Result<ProcessRequest, EffectFailed> {
    let worker = request.worker.ok_or_else(|| {
        EffectFailed::other(
            id,
            "shell.worker.missing",
            "process has no workspace worker",
        )
    })?;
    let inherited = std::env::vars_os().collect();
    let mut environment = context
        .workspace
        .as_ref()
        .expect("workspace ownership is available during process dispatch")
        .command_environment(worker, &inherited)
        .map_err(|error| EffectFailed::other(id, "shell.worker.environment", error.to_string()))?;
    set_worker_metadata(
        &mut environment,
        request.run_id.as_deref(),
        request.mutant_id.as_deref(),
    );
    request.cwd.clone_from(&environment.cwd);
    let request = ProcessRequest::from(request)
        .with_environment(environment)
        .with_cancellation(cancellation);
    Ok(match start_gate {
        Some(start_gate) => request.with_start_gate(start_gate),
        None => request,
    })
}

fn set_worker_metadata(
    environment: &mut crate::workspace::CommandEnvironment,
    run_id: Option<&str>,
    mutant_id: Option<&str>,
) {
    for name in ["HOIMIN_WORKER_ROOT", "HOIMIN_RUN_ID", "HOIMIN_MUTANT_ID"] {
        remove_environment_key(&mut environment.env, name);
    }
    environment.env.insert(
        std::ffi::OsString::from("HOIMIN_WORKER_ROOT"),
        std::ffi::OsString::from(environment.cwd.as_str()),
    );
    if let Some(run_id) = run_id {
        environment.env.insert(
            std::ffi::OsString::from("HOIMIN_RUN_ID"),
            std::ffi::OsString::from(run_id),
        );
    }
    if let Some(mutant_id) = mutant_id {
        environment.env.insert(
            std::ffi::OsString::from("HOIMIN_MUTANT_ID"),
            std::ffi::OsString::from(mutant_id),
        );
    }
}

fn remove_environment_key(
    environment: &mut BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    name: &str,
) {
    let keys = environment
        .keys()
        .filter(|key| key.to_string_lossy().eq_ignore_ascii_case(name))
        .cloned()
        .collect::<Vec<_>>();
    for key in keys {
        environment.remove(&key);
    }
}

async fn session<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    id: EffectId,
) -> Result<SessionDispatcher, EffectFailed> {
    if context.session.is_none() {
        let path = context.session_path.clone().ok_or_else(|| {
            EffectFailed::other(id, "session.missing", "session effect without --session")
        })?;
        context.session = Some(
            SessionDispatcher::open(path)
                .await
                .map_err(|error| EffectFailed::other(id, "session.open", error.to_string()))?,
        );
    }
    Ok(context.session.as_ref().expect("initialized above").clone())
}

/// Resolves filesystem-backed records that participate in a run fingerprint.
///
/// # Errors
///
/// Returns an error when a configured fingerprint input pattern cannot be resolved.
pub fn prepare_run_config(
    mut config: RunConfig,
) -> Result<RunConfig, crate::fingerprint_inputs::FingerprintInputError> {
    config.fingerprint_inputs = crate::fingerprint_inputs::resolve(
        &config.root,
        &config.fingerprint_includes,
        &config.fingerprint_files,
    )?;
    Ok(config)
}

/// Runs the configured mutation-test state machine.
///
/// # Errors
///
/// Returns an error when run infrastructure, state transitions, or cleanup fail.
pub async fn run_loop<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    let config = prepare_run_config(config).map_err(|error| error.to_string())?;
    run_loop_prepared(
        config,
        stdout,
        stderr,
        RunControl::new(),
        CandidateSelection::All,
        None,
    )
    .await
}

/// Runs an already-validated plan configuration for exactly the requested candidate IDs.
///
/// # Errors
///
/// Returns an error when a session or resume configuration is supplied, or when run
/// infrastructure, state transitions, or cleanup fail.
pub async fn run_selected_loop<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: BTreeSet<String>,
    verification_selection: hoimin_core::VerificationSelection,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    run_selected_loop_with_fingerprint_inputs(
        config,
        candidate_ids,
        verification_selection,
        BTreeSet::new(),
        stdout,
        stderr,
    )
    .await
}

#[doc(hidden)]
pub async fn run_selected_loop_with_fingerprint_inputs<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: BTreeSet<String>,
    verification_selection: hoimin_core::VerificationSelection,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    if config.session.is_some() || config.resume {
        return Err("selected candidate execution does not support sessions or resume".to_owned());
    }
    run_loop_prepared(
        config,
        stdout,
        stderr,
        RunControl::new(),
        CandidateSelection::Explicit(candidate_ids, verification_selection),
        Some(fingerprint_copy_inputs),
    )
    .await
}

/// Runs an already-validated plan configuration in saved manifest rank order.
///
/// # Errors
///
/// Returns an error when run infrastructure, state transitions, or cleanup fail.
pub async fn run_ordered_selected_loop<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: Vec<String>,
    verification_selection: hoimin_core::VerificationSelection,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    run_ordered_selected_loop_with_fingerprint_inputs(
        config,
        candidate_ids,
        verification_selection,
        BTreeSet::new(),
        stdout,
        stderr,
    )
    .await
}

pub(crate) async fn run_ordered_selected_loop_with_fingerprint_inputs<Stdout, Stderr>(
    config: RunConfig,
    candidate_ids: Vec<String>,
    verification_selection: hoimin_core::VerificationSelection,
    fingerprint_copy_inputs: BTreeSet<Utf8PathBuf>,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    if config.session.is_some() || config.resume {
        return Err("selected candidate execution does not support sessions or resume".to_owned());
    }
    run_loop_prepared(
        config,
        stdout,
        stderr,
        RunControl::new(),
        CandidateSelection::Ordered(candidate_ids, verification_selection),
        Some(fingerprint_copy_inputs),
    )
    .await
}

#[doc(hidden)]
pub async fn run_loop_with_control<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
    control: RunControl,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    let config = prepare_run_config(config).map_err(|error| error.to_string())?;
    run_loop_prepared(
        config,
        stdout,
        stderr,
        control,
        CandidateSelection::All,
        None,
    )
    .await
}

enum CandidateSelection {
    All,
    Explicit(BTreeSet<String>, hoimin_core::VerificationSelection),
    Ordered(Vec<String>, hoimin_core::VerificationSelection),
}

#[expect(
    clippy::too_many_lines,
    reason = "the loop keeps cancellation, completion, and state-transition ordering in one auditable sequence"
)]
async fn run_loop_prepared<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
    control: RunControl,
    candidate_selection: CandidateSelection,
    fingerprint_copy_inputs: Option<BTreeSet<Utf8PathBuf>>,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    let metrics_path = config.output.metrics.clone();
    let mut context = ShellContext::new(&config, stdout, stderr).await?;
    #[cfg(test)]
    if let Some(pause) = control.materialization_pause.clone() {
        context.workspace_mut().set_materialization_pause(pause);
    }
    if let Some(fingerprint_copy_inputs) = fingerprint_copy_inputs {
        context.fingerprint_copy_inputs = fingerprint_copy_inputs;
    }
    let deadline = tokio::time::Instant::now() + config.limits.total_timeout.get();
    let shutdown_grace = control.shutdown_grace();
    let max_jobs = config.limits.jobs.get();
    let channel_capacity = config.limits.jobs.get().saturating_add(1);
    let mut metrics = None;
    let mut metrics_warnings = Vec::new();
    let mut discovered = 0_u64;
    let mut executed = 0_u64;
    let initial_run_id = Uuid::new_v4().to_string();
    let mut diagnostic_run_id = initial_run_id.clone();
    let mut shutdown_budget = None;
    let mut shutdown_expiry_reported = false;
    let mut interrupts = crate::interrupt::InterruptMonitor::spawn();
    let run_result = async {
        let mut state = Box::new(match candidate_selection {
            CandidateSelection::Explicit(candidate_ids, verification_selection) => {
                RunState::with_candidate_filter(initial_run_id.clone(), config, candidate_ids)
                    .with_verification_selection(verification_selection)
            }
            CandidateSelection::Ordered(candidate_ids, verification_selection) => {
                RunState::with_ordered_candidate_filter(
                    initial_run_id.clone(),
                    config,
                    candidate_ids,
                )
                .with_verification_selection(verification_selection)
            }
            CandidateSelection::All => RunState::new(initial_run_id.clone(), config),
        });
        let (next, initial) = transition(*state, RunEvent::StartRequested(StartRequested))
            .map_err(|error| error.to_string())?;
        *state = next;
        track_diagnostic_run_id(&mut diagnostic_run_id, &state);
        if metrics_path.is_some() {
            let mut collector = MetricsCollector::new(state.run_id());
            if let Err(error) = collector.begin_stage("targets") {
                metrics_warnings.push(("metrics.state", error.to_string()));
            }
            metrics = Some(collector);
        }
        let mut effects = VecDeque::from(initial);
        record_ready_processes(&effects, &mut metrics, &mut metrics_warnings);
        let cancellation = ProcessCancellation::new();
        let (completion_tx, mut completion_rx) = mpsc::channel(channel_capacity);
        let mut process_tasks = JoinSet::new();
        let mut io_tasks = JoinSet::new();
        let mut in_flight = 0_usize;
        let mut io_in_flight = 0_usize;
        let mut stop_signalled = false;

        while state.phase() != RunPhase::Finished {
            let mut serial_completion = None;
            let mut priority_event = None;
            let mut signal_failure = None;
            let ready_process_completion = if !stop_signalled
                && !control.is_cancelled()
                && tokio::time::Instant::now() < deadline
            {
                completion_rx.try_recv().ok()
            } else {
                None
            };
            while ready_process_completion.is_none() {
                let Some(effect) = effects.pop_front() else {
                    break;
                };
                if !stop_signalled
                    && (control.is_cancelled() || tokio::time::Instant::now() >= deadline)
                {
                    cancel_queued_effect(&effect, &mut metrics, &mut metrics_warnings);
                    cancellation.cancel();
                    stop_signalled = true;
                    let event = if control.is_cancelled() {
                        RunEvent::CancellationRequested
                    } else {
                        RunEvent::DeadlineReached
                    };
                    establish_event_shutdown_budget(
                        &mut shutdown_budget,
                        &event,
                        deadline,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    );
                    priority_event = Some(event);
                    break;
                }
                if !state.is_effect_pending(effect.id()) {
                    cancel_queued_effect(&effect, &mut metrics, &mut metrics_warnings);
                    continue;
                }
                match effect {
                    RunEffect::RunBaseline(request) => {
                        let id = request.id;
                        let worker = request.worker;
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                let Some(worker) = worker else {
                                    unreachable!("worker process request accepted without worker")
                                };
                                let Some(dispatch) = control.begin_dispatch() else {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    );
                                    priority_event = Some(event);
                                    break;
                                };
                                spawn_process(
                                    Arc::clone(&context.process),
                                    request,
                                    worker,
                                    true,
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    dispatch,
                                );
                            }
                            Err(error) => {
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                });
                            }
                        }
                        if serial_completion.is_none() {
                            control.observe_process_tasks(process_tasks.len());
                            debug_assert!(process_tasks.len() <= max_jobs);
                            in_flight += 1;
                            control.observe_completion_in_flight(in_flight);
                        }
                    }
                    RunEffect::RunMutant(request) => {
                        let id = request.id;
                        let worker = request.worker;
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                let Some(worker) = worker else {
                                    unreachable!("worker process request accepted without worker")
                                };
                                let Some(dispatch) = accept_process_dispatch(
                                    &control,
                                    &mut metrics,
                                    &mut metrics_warnings,
                                    worker,
                                ) else {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    );
                                    priority_event = Some(event);
                                    break;
                                };
                                spawn_process(
                                    Arc::clone(&context.process),
                                    request,
                                    worker,
                                    false,
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    dispatch,
                                );
                            }
                            Err(error) => {
                                cancel_queued_worker(worker, &mut metrics, &mut metrics_warnings);
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                });
                            }
                        }
                        if serial_completion.is_none() {
                            control.observe_process_tasks(process_tasks.len());
                            debug_assert!(process_tasks.len() <= max_jobs);
                            in_flight += 1;
                            control.observe_completion_in_flight(in_flight);
                        }
                    }
                    effect if is_blocking_io_effect(&effect) => {
                        match prepare_blocking_effect(&mut context, effect) {
                            Ok(task) => {
                                spawn_blocking_effect(task, completion_tx.clone(), &mut io_tasks);
                                in_flight += 1;
                                io_in_flight += 1;
                                debug_assert!(io_in_flight <= max_jobs);
                                control.observe_completion_in_flight(in_flight);
                            }
                            Err(error) => {
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                });
                            }
                        }
                    }
                    // Wall-clock observation is scheduler-owned so it reads the same absolute
                    // deadline that enforces the total timeout, without spawning worker work.
                    RunEffect::ObserveRemainingBudget(request) => {
                        serial_completion = Some(ShellCompletion {
                            event: RunEvent::RemainingBudgetObserved(remaining_budget_observed(
                                &request,
                                deadline,
                                tokio::time::Instant::now(),
                            )),
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        });
                    }
                    effect => {
                        let stopping = stop_signalled;
                        let event = if stopping {
                            let budget = shutdown_budget
                                .ok_or_else(|| "stopping run has no shutdown budget".to_owned())?;
                            let mut execution = Box::pin(execute_effect_with_cancellation(
                                &mut context,
                                effect,
                                cancellation.clone(),
                            ));
                            let Ok(event) = budget.wait(&mut execution).await else {
                                drop(execution);
                                let error = shutdown_expiry_error(
                                    &mut process_tasks,
                                    &mut io_tasks,
                                    &mut completion_rx,
                                    &mut in_flight,
                                    &budget,
                                    &mut shutdown_expiry_reported,
                                    &mut metrics,
                                    &mut metrics_warnings,
                                    |completion| {
                                        let _ =
                                            accept_blocking_completion(&mut context, completion);
                                    },
                                )
                                .await;
                                return Err(error);
                            };
                            event
                        } else {
                            enum SerialSelection {
                                Completed(RunEvent),
                                Stopped {
                                    event: RunEvent,
                                    budget: ShutdownBudget,
                                    signal_failure: Option<String>,
                                },
                            }
                            let mut execution = Box::pin(execute_effect_with_cancellation(
                                &mut context,
                                effect,
                                cancellation.clone(),
                            ));
                            let selection = tokio::select! {
                                biased;
                                event = &mut execution => SerialSelection::Completed(event),
                                () = control.cancelled() => {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::CancellationRequested;
                                    let budget = establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    )
                                    .expect("cancellation establishes a shutdown budget");
                                    SerialSelection::Stopped {
                                        event,
                                        budget,
                                        signal_failure: None,
                                    }
                                }
                                () = tokio::time::sleep_until(deadline) => {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    let event = RunEvent::DeadlineReached;
                                    let budget = establish_event_shutdown_budget(
                                        &mut shutdown_budget,
                                        &event,
                                        deadline,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    )
                                    .expect("deadline establishes a shutdown budget");
                                    SerialSelection::Stopped {
                                        event,
                                        budget,
                                        signal_failure: None,
                                    }
                                }
                                signal = interrupts.first() => {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    match first_interrupt_event(signal) {
                                        Ok(event) => {
                                            let budget = establish_event_shutdown_budget(
                                                &mut shutdown_budget,
                                                &event,
                                                deadline,
                                                tokio::time::Instant::now(),
                                                shutdown_grace,
                                            )
                                            .expect("interrupt establishes a shutdown budget");
                                            SerialSelection::Stopped {
                                                event,
                                                budget,
                                                signal_failure: None,
                                            }
                                        }
                                        Err(error) => {
                                            let budget = establish_shutdown_budget(
                                                &mut shutdown_budget,
                                                ShutdownBudget::after_observation_with_grace(
                                                    ShutdownCause::Failure,
                                                    tokio::time::Instant::now(),
                                                    shutdown_grace,
                                                ),
                                            );
                                            SerialSelection::Stopped {
                                                event: RunEvent::CancellationRequested,
                                                budget,
                                                signal_failure: Some(error),
                                            }
                                        }
                                    }
                                }
                            };
                            match selection {
                                SerialSelection::Completed(event) => event,
                                SerialSelection::Stopped {
                                    event,
                                    budget,
                                    signal_failure: pending_signal_failure,
                                } => {
                                    if budget.wait(&mut execution).await.is_err() {
                                        drop(execution);
                                        let expiry_error = shutdown_expiry_error(
                                            &mut process_tasks,
                                            &mut io_tasks,
                                            &mut completion_rx,
                                            &mut in_flight,
                                            &budget,
                                            &mut shutdown_expiry_reported,
                                            &mut metrics,
                                            &mut metrics_warnings,
                                            |completion| {
                                                let _ = accept_blocking_completion(
                                                    &mut context,
                                                    completion,
                                                );
                                            },
                                        )
                                        .await;
                                        return Err(match pending_signal_failure {
                                            Some(primary) => {
                                                combine_shutdown_errors(primary, Some(expiry_error))
                                            }
                                            None => expiry_error,
                                        });
                                    }
                                    signal_failure = pending_signal_failure;
                                    event
                                }
                            }
                        };
                        if matches!(
                            event,
                            RunEvent::DeadlineReached | RunEvent::CancellationRequested
                        ) {
                            priority_event = Some(event);
                        } else {
                            serial_completion = Some(ShellCompletion {
                                event,
                                process_task: false,
                                io_task: false,
                                process: None,
                                blocking: None,
                            });
                        }
                    }
                }
                if serial_completion.is_some() {
                    break;
                }
            }

            if let Some(error) = signal_failure.take() {
                cancellation.cancel();
                let drain_budget = establish_shutdown_budget(
                    &mut shutdown_budget,
                    ShutdownBudget::after_observation_with_grace(
                        ShutdownCause::Failure,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    ),
                );
                let drain_failure = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
                    &drain_budget,
                    &mut shutdown_expiry_reported,
                    &mut metrics,
                    &mut metrics_warnings,
                    |completion| {
                        let _ = accept_blocking_completion(&mut context, completion);
                    },
                )
                .await
                .err();
                return Err(combine_shutdown_errors(error, drain_failure));
            }

            if in_flight == 0
                && priority_event.is_none()
                && serial_completion.is_none()
                && ready_process_completion.is_none()
            {
                return Err(format!("run stalled in {:?}", state.phase()));
            }

            let completion = if let Some(completion) = serial_completion {
                completion
            } else if let Some(event) = priority_event {
                ShellCompletion {
                    event,
                    process_task: false,
                    io_task: false,
                    process: None,
                    blocking: None,
                }
            } else if let Some(completion) = ready_process_completion {
                completion
            } else if stop_signalled {
                let budget = shutdown_budget
                    .ok_or_else(|| "stopping run has no shutdown budget".to_owned())?;
                match budget.wait(completion_rx.recv()).await {
                    Ok(Some(completion)) => completion,
                    Ok(None) => return Err("completion channel closed".to_owned()),
                    Err(_) => {
                        let error = shutdown_expiry_error(
                            &mut process_tasks,
                            &mut io_tasks,
                            &mut completion_rx,
                            &mut in_flight,
                            &budget,
                            &mut shutdown_expiry_reported,
                            &mut metrics,
                            &mut metrics_warnings,
                            |completion| {
                                let _ = accept_blocking_completion(&mut context, completion);
                            },
                        )
                        .await;
                        return Err(error);
                    }
                }
            } else {
                tokio::select! {
                    biased;
                    () = control.cancelled() => {
                        cancellation.cancel();
                        stop_signalled = true;
                        let event = RunEvent::CancellationRequested;
                        establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion {
                            event,
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        }
                    }
                    () = tokio::time::sleep_until(deadline) => {
                        cancellation.cancel();
                        stop_signalled = true;
                        let event = RunEvent::DeadlineReached;
                        establish_event_shutdown_budget(
                            &mut shutdown_budget,
                            &event,
                            deadline,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        );
                        ShellCompletion {
                            event,
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        }
                    }
                    signal = interrupts.first() => {
                        cancellation.cancel();
                        stop_signalled = true;
                        match first_interrupt_event(signal) {
                            Ok(event) => {
                                establish_event_shutdown_budget(
                                    &mut shutdown_budget,
                                    &event,
                                    deadline,
                                    tokio::time::Instant::now(),
                                    shutdown_grace,
                                );
                                ShellCompletion {
                                    event,
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                }
                            },
                            Err(error) => {
                                signal_failure = Some(error);
                                establish_shutdown_budget(
                                    &mut shutdown_budget,
                                    ShutdownBudget::after_observation_with_grace(
                                        ShutdownCause::Failure,
                                        tokio::time::Instant::now(),
                                        shutdown_grace,
                                    ),
                                );
                                ShellCompletion {
                                    event: RunEvent::CancellationRequested,
                                    process_task: false,
                                    io_task: false,
                                    process: None,
                                    blocking: None,
                                }
                            }
                        }
                    }
                    event = completion_rx.recv() => {
                        event.ok_or_else(|| "completion channel closed".to_owned())?
                    }
                }
            };
            if let Some(error) = signal_failure.take() {
                cancellation.cancel();
                let drain_budget = establish_shutdown_budget(
                    &mut shutdown_budget,
                    ShutdownBudget::after_observation_with_grace(
                        ShutdownCause::Failure,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    ),
                );
                let drain_failure = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
                    &drain_budget,
                    &mut shutdown_expiry_reported,
                    &mut metrics,
                    &mut metrics_warnings,
                    |completion| {
                        let _ = accept_blocking_completion(&mut context, completion);
                    },
                )
                .await
                .err();
                return Err(combine_shutdown_errors(error, drain_failure));
            }
            let ShellCompletion {
                mut event,
                process_task: process_completion,
                io_task: io_completion,
                process,
                blocking,
            } = completion;
            if let Some(blocking) = blocking {
                event = accept_blocking_completion(&mut context, *blocking);
            }
            let previous_phase = state.phase();
            let accepted_mutant = matches!(&event, RunEvent::MutantFinished(_));
            let targets_resolved = matches!(&event, RunEvent::TargetsResolved(_));
            let preflight_completed = matches!(&event, RunEvent::PreflightCompleted(_));
            let cleanup_finished = matches!(&event, RunEvent::CleanupFinished(_));
            let analyzed_records = match &event {
                RunEvent::AnalysisFinished(value) => {
                    value.spool.as_ref().map(|spool| spool.records)
                }
                _ => None,
            };
            let external_stop = matches!(
                event,
                RunEvent::DeadlineReached | RunEvent::CancellationRequested
            );
            let deadline_stop = matches!(event, RunEvent::DeadlineReached);
            let failed = matches!(event, RunEvent::EffectFailed(_));
            let failed_primary = failed_event_primary(&event);
            if !external_stop && (process_completion || io_completion) {
                in_flight = in_flight.saturating_sub(1);
            }
            if !external_stop && io_completion {
                io_in_flight = io_in_flight.saturating_sub(1);
            }
            if external_stop || failed {
                establish_event_shutdown_budget(
                    &mut shutdown_budget,
                    &event,
                    deadline,
                    tokio::time::Instant::now(),
                    shutdown_grace,
                );
            }
            if failed {
                cancellation.cancel();
                stop_signalled = true;
            }
            let transition_result =
                transition(*state, event).map(|(next, produced)| (Box::new(next), produced));
            let (next, produced) = match transition_result {
                Ok(value) => value,
                Err(error) => {
                    cancellation.cancel();
                    let drain_budget = establish_shutdown_budget(
                        &mut shutdown_budget,
                        ShutdownBudget::after_observation_with_grace(
                            ShutdownCause::Failure,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        ),
                    );
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
                        &drain_budget,
                        &mut shutdown_expiry_reported,
                        &mut metrics,
                        &mut metrics_warnings,
                        |completion| {
                            let _ = accept_blocking_completion(&mut context, completion);
                        },
                    )
                    .await
                    .err();
                    return Err(combine_shutdown_errors(error.to_string(), drain_failure));
                }
            };
            state = next;
            track_diagnostic_run_id(&mut diagnostic_run_id, &state);
            if let Some((worker, true)) = process {
                record_metrics(&mut metrics, &mut metrics_warnings, |metrics| {
                    metrics.process_finished(worker, accepted_mutant)
                });
            }
            if let Some(records) = analyzed_records {
                discovered = records;
                if let Some(metrics) = metrics.as_mut() {
                    metrics.discovered(records);
                }
            }
            if accepted_mutant {
                executed = executed.saturating_add(1);
            }
            observe_accepted_transition(
                &mut metrics,
                &mut metrics_warnings,
                previous_phase,
                state.phase(),
                targets_resolved,
                preflight_completed,
                cleanup_finished,
            );

            if external_stop || failed {
                discard_queued_effects(&mut effects, &mut metrics, &mut metrics_warnings);
                let candidate_budget = if deadline_stop {
                    ShutdownBudget::for_total_timeout_with_grace(deadline, shutdown_grace)
                } else {
                    let cause = if external_stop {
                        ShutdownCause::Cancellation
                    } else {
                        ShutdownCause::Failure
                    };
                    ShutdownBudget::after_observation_with_grace(
                        cause,
                        tokio::time::Instant::now(),
                        shutdown_grace,
                    )
                };
                let drain_budget =
                    establish_shutdown_budget(&mut shutdown_budget, candidate_budget);
                let drain_result = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
                    &drain_budget,
                    &mut shutdown_expiry_reported,
                    &mut metrics,
                    &mut metrics_warnings,
                    |completion| {
                        let _ = accept_blocking_completion(&mut context, completion);
                    },
                )
                .await;
                finish_failed_event_drain(failed_primary, drain_result)?;
                io_in_flight = 0;
            } else if process_completion {
                let process_failure = match process_tasks.join_next().await {
                    Some(Ok(())) => None,
                    Some(Err(error)) => Some(format!("process task failed: {error}")),
                    None => Some("process completion had no task".to_owned()),
                };
                if let Some(process_failure) = process_failure {
                    cancellation.cancel();
                    let drain_budget = establish_shutdown_budget(
                        &mut shutdown_budget,
                        ShutdownBudget::after_observation_with_grace(
                            ShutdownCause::Failure,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        ),
                    );
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
                        &drain_budget,
                        &mut shutdown_expiry_reported,
                        &mut metrics,
                        &mut metrics_warnings,
                        |completion| {
                            let _ = accept_blocking_completion(&mut context, completion);
                        },
                    )
                    .await
                    .err();
                    return Err(combine_shutdown_errors(process_failure, drain_failure));
                }
            } else if io_completion {
                let io_failure = match io_tasks.join_next().await {
                    Some(Ok(())) => None,
                    Some(Err(error)) => Some(format!("blocking I/O task failed: {error}")),
                    None => Some("blocking I/O completion had no task".to_owned()),
                };
                if let Some(io_failure) = io_failure {
                    cancellation.cancel();
                    let drain_budget = establish_shutdown_budget(
                        &mut shutdown_budget,
                        ShutdownBudget::after_observation_with_grace(
                            ShutdownCause::Failure,
                            tokio::time::Instant::now(),
                            shutdown_grace,
                        ),
                    );
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
                        &drain_budget,
                        &mut shutdown_expiry_reported,
                        &mut metrics,
                        &mut metrics_warnings,
                        |completion| {
                            let _ = accept_blocking_completion(&mut context, completion);
                        },
                    )
                    .await
                    .err();
                    return Err(combine_shutdown_errors(io_failure, drain_failure));
                }
            }
            record_ready_processes(&produced, &mut metrics, &mut metrics_warnings);
            effects.extend(produced);
        }
        if let Some(metrics) = metrics.as_mut() {
            metrics.set_run_id(state.run_id());
        }
        Ok((state.exit_code(), state.run_id().to_owned()))
    }
    .await;
    let outer_finalization = async {
        ensure_outer_finalization_budget(
            &mut shutdown_budget,
            run_result.is_err(),
            deadline,
            tokio::time::Instant::now(),
            shutdown_grace,
        );
        let close = close_context_resources(
            &mut context,
            shutdown_budget
                .as_ref()
                .expect("outer finalization established a shutdown budget"),
            shutdown_expiry_reported,
            Box::new(|| {}),
        )
        .await;
        if close.expiry.is_some() {
            shutdown_expiry_reported = true;
        }
        let mut run_result = match (run_result, close.expiry) {
            (Ok(_), Some(expiry)) => Err(expiry),
            (Err(primary), Some(expiry)) => Err(combine_shutdown_errors(primary, Some(expiry))),
            (result, None) => result,
        };
        if let Some(path) = metrics_path {
            match (shutdown_budget.as_ref(), shutdown_expiry_reported) {
                (Some(budget), false) => {
                    let finalized = finalize_metrics_with_shutdown(
                        path.as_std_path().to_owned(),
                        metrics,
                        run_result.as_ref().err().cloned(),
                        discovered,
                        executed,
                        metrics_warnings,
                        budget,
                        Box::new(|| {}),
                    )
                    .await;
                    metrics_warnings = finalized.warnings;
                    if let Some(expiry) = finalized.expiry {
                        run_result = match run_result {
                            Ok(_) => Err(expiry),
                            Err(primary) => Err(combine_shutdown_errors(primary, Some(expiry))),
                        };
                    }
                }
                (Some(_), true) => skip_expired_metrics_finalization(
                    metrics,
                    run_result.as_ref().err().map(String::as_str),
                    &mut metrics_warnings,
                ),
                (None, _) => unreachable!("outer finalization always has a shutdown budget"),
            }
            for (code, message) in metrics_warnings {
                emit_metrics_warning(&mut context, &diagnostic_run_id, code, message);
            }
        }
        combine_close_results(
            run_result.map(|(exit_code, _)| exit_code),
            close.workspace,
            close.process,
        )
    };
    finish_with_interrupt_monitor(interrupts, outer_finalization).await
}

fn track_diagnostic_run_id(diagnostic_run_id: &mut String, state: &RunState) {
    diagnostic_run_id.clear();
    diagnostic_run_id.push_str(state.run_id());
}

fn record_metrics(
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
    observation: impl FnOnce(&mut MetricsCollector) -> Result<(), MetricsError>,
) {
    if let Some(collector) = collector.as_mut()
        && let Err(error) = observation(collector)
    {
        warnings.push(("metrics.state", error.to_string()));
    }
}

fn cancel_queued_worker(
    worker: Option<u32>,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    if let Some(worker) = worker {
        record_metrics(collector, warnings, |metrics| metrics.cancel_queued(worker));
    }
}

fn cancel_queued_effect(
    effect: &RunEffect,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    if let RunEffect::RunMutant(request) = effect {
        cancel_queued_worker(request.worker, collector, warnings);
    }
}

fn discard_queued_effects(
    effects: &mut VecDeque<RunEffect>,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    for effect in effects.drain(..) {
        cancel_queued_effect(&effect, collector, warnings);
    }
}

fn record_ready_processes<'a>(
    effects: impl IntoIterator<Item = &'a RunEffect>,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    for worker in effects.into_iter().filter_map(|effect| match effect {
        RunEffect::RunMutant(request) => request.worker,
        _ => None,
    }) {
        record_metrics(collector, warnings, |metrics| metrics.queued(worker));
    }
}

fn accept_process_dispatch<'a>(
    control: &'a RunControl,
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
    worker: u32,
) -> Option<std::sync::MutexGuard<'a, ()>> {
    let Some(dispatch) = control.begin_dispatch() else {
        record_metrics(collector, warnings, |metrics| metrics.cancel_queued(worker));
        return None;
    };
    record_metrics(collector, warnings, |metrics| {
        metrics.process_started(worker)
    });
    Some(dispatch)
}

fn observe_accepted_transition(
    collector: &mut Option<MetricsCollector>,
    warnings: &mut Vec<(&'static str, String)>,
    previous: RunPhase,
    next: RunPhase,
    targets_resolved: bool,
    preflight_completed: bool,
    cleanup_finished: bool,
) {
    record_metrics(collector, warnings, |metrics| {
        if targets_resolved {
            metrics.finish_stage("targets")?;
            metrics.begin_stage("preflight")?;
        }
        if preflight_completed {
            metrics.finish_stage("preflight")?;
        }
        if previous != next {
            let previous_stage = phase_stage(previous);
            let next_stage = phase_stage(next);
            if previous_stage != next_stage {
                if let Some(stage) = previous_stage
                    && !(previous == RunPhase::Cleaning && cleanup_finished)
                {
                    metrics.finish_stage(stage)?;
                }
                if let Some(stage) = next_stage {
                    metrics.begin_stage(stage)?;
                }
            }
        }
        if cleanup_finished {
            metrics.finish_stage("cleanup")?;
        }
        Ok(())
    });
}

fn phase_stage(phase: RunPhase) -> Option<&'static str> {
    match phase {
        RunPhase::Copy | RunPhase::MaterializationVerification => Some("copy"),
        RunPhase::Baseline => Some("baseline"),
        RunPhase::Analyze => Some("analysis"),
        RunPhase::Mutants => Some("mutants"),
        RunPhase::Cleaning => Some("cleanup"),
        _ => None,
    }
}

fn emit_metrics_warning<Stdout: Write, Stderr: Write>(
    context: &mut ShellContext<Stdout, Stderr>,
    run_id: &str,
    code: &str,
    message: String,
) {
    let _ = context.report.handle(EmitOutput {
        id: EffectId(u64::MAX),
        event: OutputEvent::Diagnostic(Diagnostic::new(run_id, u64::MAX, "warning", code, message)),
    });
}

fn first_interrupt_event(signal: Result<(), String>) -> Result<RunEvent, String> {
    signal.map(|()| RunEvent::CancellationRequested)
}

fn spawn_process(
    process: Arc<ProcessHandler>,
    request: ProcessRequest,
    worker: u32,
    baseline: bool,
    sender: mpsc::Sender<ShellCompletion>,
    tasks: &mut JoinSet<()>,
    _dispatch: std::sync::MutexGuard<'_, ()>,
) {
    tasks.spawn(async move {
        let event = match process.run(request).await {
            Ok(value) if baseline => RunEvent::BaselineFinished(value),
            Ok(value) => RunEvent::MutantFinished(value),
            Err(error) => RunEvent::EffectFailed(error),
        };
        let _ = sender
            .send(ShellCompletion {
                event,
                process_task: true,
                io_task: false,
                process: Some((worker, !baseline)),
                blocking: None,
            })
            .await;
    });
}

fn spawn_blocking_effect(
    task: BlockingEffect,
    sender: mpsc::Sender<ShellCompletion>,
    tasks: &mut JoinSet<()>,
) {
    tasks.spawn(async move {
        let id = task.id();
        let (event, blocking) = match run_blocking_io(id, move || task.execute()).await {
            Ok(completion) => {
                let event = match &completion {
                    BlockingEffectCompletion::Workspace(completion) => completion.event().clone(),
                    BlockingEffectCompletion::Candidate(event)
                    | BlockingEffectCompletion::Cleanup { event, .. } => event.as_ref().clone(),
                };
                (event, Some(Box::new(completion)))
            }
            Err(error) => (RunEvent::EffectFailed(error), None),
        };
        let _ = sender
            .send(ShellCompletion {
                event,
                process_task: false,
                io_task: true,
                process: None,
                blocking,
            })
            .await;
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "grace expiry must drain both task sets and preserve buffered ownership accounting"
)]
async fn shutdown_expiry_error(
    process_tasks: &mut JoinSet<()>,
    io_tasks: &mut JoinSet<()>,
    receiver: &mut mpsc::Receiver<ShellCompletion>,
    in_flight: &mut usize,
    budget: &ShutdownBudget,
    shutdown_expiry_reported: &mut bool,
    metrics: &mut Option<MetricsCollector>,
    metrics_warnings: &mut Vec<(&'static str, String)>,
    accept_blocking: impl FnMut(BlockingEffectCompletion),
) -> String {
    *shutdown_expiry_reported = true;
    let process_task_count = process_tasks.len();
    let io_task_count = io_tasks.len();
    let expiry = budget.expiry_error(process_task_count, io_task_count);
    let drain_failure = drain_processes(
        process_tasks,
        io_tasks,
        receiver,
        in_flight,
        budget,
        shutdown_expiry_reported,
        metrics,
        metrics_warnings,
        accept_blocking,
    )
    .await
    .err();
    match drain_failure {
        Some(error) if !error.contains("shutdown grace expired") => {
            combine_shutdown_errors(expiry, Some(error))
        }
        Some(_) | None => expiry,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "shutdown draining needs both task sets, ownership returns, and accounting under one deadline"
)]
async fn drain_processes(
    process_tasks: &mut JoinSet<()>,
    io_tasks: &mut JoinSet<()>,
    receiver: &mut mpsc::Receiver<ShellCompletion>,
    in_flight: &mut usize,
    budget: &ShutdownBudget,
    shutdown_expiry_reported: &mut bool,
    metrics: &mut Option<MetricsCollector>,
    metrics_warnings: &mut Vec<(&'static str, String)>,
    mut accept_blocking: impl FnMut(BlockingEffectCompletion),
) -> Result<(), String> {
    let drain_result = budget
        .wait(async {
            let mut first_failure = None;
            while !process_tasks.is_empty() || !io_tasks.is_empty() {
                tokio::select! {
                    result = process_tasks.join_next(), if !process_tasks.is_empty() => {
                        if let Some(Err(error)) = result
                            && first_failure.is_none()
                        {
                            first_failure = Some(format!("process task failed while stopping: {error}"));
                        }
                    }
                    result = io_tasks.join_next(), if !io_tasks.is_empty() => {
                        if let Some(Err(error)) = result
                            && first_failure.is_none()
                        {
                            first_failure = Some(format!("blocking I/O task failed while stopping: {error}"));
                        }
                    }
                    completion = receiver.recv(), if *in_flight > 0 => {
                        if let Some(completion) = completion {
                            accept_drained_completion(
                                completion,
                                in_flight,
                                metrics,
                                metrics_warnings,
                                &mut accept_blocking,
                            );
                        }
                    }
                }
            }
            first_failure
        })
        .await;

    let process_task_count = process_tasks.len();
    let io_task_count = io_tasks.len();
    while let Ok(completion) = receiver.try_recv() {
        accept_drained_completion(
            completion,
            in_flight,
            metrics,
            metrics_warnings,
            &mut accept_blocking,
        );
    }

    if drain_result.is_err() {
        *shutdown_expiry_reported = true;
        process_tasks.abort_all();
        io_tasks.abort_all();
    }
    *in_flight = 0;

    match drain_result {
        Ok(Some(error)) => Err(error),
        Ok(None) => Ok(()),
        Err(_) => Err(budget.expiry_error(process_task_count, io_task_count)),
    }
}

fn accept_drained_completion(
    completion: ShellCompletion,
    in_flight: &mut usize,
    metrics: &mut Option<MetricsCollector>,
    metrics_warnings: &mut Vec<(&'static str, String)>,
    accept_blocking: &mut impl FnMut(BlockingEffectCompletion),
) {
    if let Some((worker, true)) = completion.process {
        record_metrics(metrics, metrics_warnings, |metrics| {
            metrics.process_finished(worker, false)
        });
    }
    if let Some(blocking) = completion.blocking {
        accept_blocking(*blocking);
    }
    *in_flight = in_flight.saturating_sub(1);
}

fn combine_shutdown_errors(primary: String, drain_failure: Option<String>) -> String {
    match drain_failure {
        Some(drain_failure) => format!("{primary}; {drain_failure}"),
        None => primary,
    }
}

fn failed_event_primary(event: &RunEvent) -> Option<String> {
    let RunEvent::EffectFailed(failed) = event else {
        return None;
    };
    Some(format!(
        "effect failed ({}): {}",
        failed.failure.code(),
        failed.failure.message()
    ))
}

fn finish_failed_event_drain(
    failed_primary: Option<String>,
    drain_result: Result<(), String>,
) -> Result<(), String> {
    match (failed_primary, drain_result) {
        (_, Ok(())) => Ok(()),
        (Some(primary), Err(drain_failure)) => {
            Err(combine_shutdown_errors(primary, Some(drain_failure)))
        }
        (None, Err(drain_failure)) => Err(drain_failure),
    }
}

struct ResourceCloseCompletion {
    workspace: WorkspaceHandler,
    workspace_result: Result<(), String>,
    process_result: Result<(), String>,
}

struct ResourceCloseResults {
    workspace: Result<(), String>,
    process: Result<(), String>,
    expiry: Option<String>,
}

type OwnedStartHook = Box<dyn FnOnce() + Send + 'static>;

fn detach_resource_cleanup(
    mut workspace: Option<WorkspaceHandler>,
    process: Arc<ProcessHandler>,
    before_start: OwnedStartHook,
) {
    drop(tokio::task::spawn_blocking(move || {
        before_start();
        let _ = process.close();
        if let Some(workspace) = workspace.as_mut() {
            let _ = workspace.close();
        }
    }));
}

fn detach_context_resources<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    before_start: OwnedStartHook,
) {
    detach_resource_cleanup(
        context.workspace.take(),
        Arc::clone(&context.process),
        before_start,
    );
}

async fn close_context_resources<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    budget: &ShutdownBudget,
    shutdown_already_expired: bool,
    before_start: OwnedStartHook,
) -> ResourceCloseResults
where
    Stdout: Write,
    Stderr: Write,
{
    if shutdown_already_expired {
        detach_context_resources(context, before_start);
        return ResourceCloseResults {
            workspace: Ok(()),
            process: Ok(()),
            expiry: None,
        };
    }
    if tokio::time::Instant::now() >= budget.deadline() {
        let expiry = budget.expiry_error(0, 0);
        detach_context_resources(context, before_start);
        return ResourceCloseResults {
            workspace: Ok(()),
            process: Ok(()),
            expiry: Some(expiry),
        };
    }
    let Some(workspace) = context.workspace.take() else {
        return ResourceCloseResults {
            workspace: Err("workspace ownership is unavailable during final cleanup".to_owned()),
            process: Ok(()),
            expiry: None,
        };
    };
    let process = Arc::clone(&context.process);
    let mut task = tokio::task::spawn_blocking(move || {
        before_start();
        let mut workspace = workspace;
        let process_result = process.close().map_err(|error| error.to_string());
        let workspace_result = workspace.close().map_err(|error| error.to_string());
        ResourceCloseCompletion {
            workspace,
            workspace_result,
            process_result,
        }
    });
    let completion = match budget.wait(&mut task).await {
        Ok(Ok(completion)) => completion,
        Ok(Err(error)) => {
            return ResourceCloseResults {
                workspace: Err(format!("blocking I/O task failed: {error}")),
                process: Ok(()),
                expiry: None,
            };
        }
        Err(_) => {
            drop(task);
            return ResourceCloseResults {
                workspace: Ok(()),
                process: Ok(()),
                expiry: Some(budget.expiry_error(0, 1)),
            };
        }
    };
    context.workspace = Some(completion.workspace);
    ResourceCloseResults {
        workspace: completion.workspace_result,
        process: completion.process_result,
        expiry: None,
    }
}

struct MetricsFinalizeResults {
    warnings: Vec<(&'static str, String)>,
    expiry: Option<String>,
}

fn skip_expired_metrics_finalization(
    collector: Option<MetricsCollector>,
    run_failure: Option<&str>,
    warnings: &mut Vec<(&'static str, String)>,
) {
    drop(collector);
    let failure = run_failure.unwrap_or("shutdown grace expired");
    warnings.push((
        "metrics.incomplete",
        format!("metrics output was not written because the run failed: {failure}"),
    ));
}

#[expect(
    clippy::too_many_arguments,
    reason = "metrics finalization carries the existing run summary and the shared shutdown deadline"
)]
async fn finalize_metrics_with_shutdown(
    path: std::path::PathBuf,
    collector: Option<MetricsCollector>,
    run_failure: Option<String>,
    discovered: u64,
    executed: u64,
    mut warnings: Vec<(&'static str, String)>,
    budget: &ShutdownBudget,
    before_start: OwnedStartHook,
) -> MetricsFinalizeResults {
    let preserved_warnings = warnings.clone();
    let operation = move || {
        finalize_metrics(
            &path,
            collector,
            run_failure.as_deref(),
            discovered,
            executed,
            &mut warnings,
        );
        warnings
    };
    match run_owned_blocking_until(budget, before_start, operation).await {
        Ok(warnings) => MetricsFinalizeResults {
            warnings,
            expiry: None,
        },
        Err(OwnedBlockingError::Expired(expiry)) => MetricsFinalizeResults {
            warnings: preserved_warnings,
            expiry: Some(expiry),
        },
        Err(OwnedBlockingError::Join(error)) => {
            let mut warnings = preserved_warnings;
            warnings.push(("metrics.write", error));
            MetricsFinalizeResults {
                warnings,
                expiry: None,
            }
        }
    }
}

fn combine_close_results(
    run: Result<i32, String>,
    workspace: Result<(), String>,
    process: Result<(), String>,
) -> Result<i32, String> {
    let mut failures = Vec::new();
    if let Err(error) = workspace {
        failures.push(format!("workspace cleanup failed: {error}"));
    }
    if let Err(error) = process {
        failures.push(format!("process backend close failed: {error}"));
    }
    match (run, failures.is_empty()) {
        (Ok(code), true) => Ok(code),
        (Ok(_), false) => Err(failures.join("; ")),
        (Err(error), true) => Err(error),
        (Err(error), false) => Err(format!("{error}; {}", failures.join("; "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::time::{Duration, Instant};

    use hoimin_core::{
        ApplyMutation, BudgetLedger, ByteSpan, Cleanup, CommandArg, CreateWorker,
        IntegrityCheckpoint, MutationCandidate, ObserveRemainingBudget, Preflight, ResetWorker,
        RunBudgets, VerifyOriginals, reserve_workspace_copy,
    };

    use crate::metrics::write_metrics;
    use crate::resource::PortableBackend;

    #[cfg(unix)]
    fn missing_executable_arg() -> CommandArg {
        CommandArg::Unix(b"definitely-missing-hoimin-executable".to_vec())
    }

    #[cfg(windows)]
    fn missing_executable_arg() -> CommandArg {
        use std::os::windows::ffi::OsStrExt;

        CommandArg::Windows(
            std::ffi::OsStr::new("definitely-missing-hoimin-executable")
                .encode_wide()
                .collect(),
        )
    }

    fn process_effect(worker: u32) -> RunEffect {
        RunEffect::RunMutant(RunProcess {
            id: EffectId(1),
            worker: Some(worker),
            run_id: Some("run-1".into()),
            mutant_id: Some("mutant-1".into()),
            argv: Vec::new(),
            cwd: Utf8PathBuf::from("."),
            limits: hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(1),
                max_output_bytes: 1,
                max_memory_bytes: 1,
                max_processes: 1,
            },
        })
    }

    async fn context_with_worker() -> (
        tempfile::TempDir,
        ShellContext<Vec<u8>, Vec<u8>>,
        MutationCandidate,
        CreateWorker,
    ) {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("pkg")).unwrap();
        std::fs::write(project.path().join("pkg/a.py"), b"original\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("pkg/a.py"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let completed = context
            .workspace_mut()
            .handle_preflight(Preflight { id: EffectId(1) })
            .unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        context
            .workspace_mut()
            .handle_create_worker(grant.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let hash = context
            .workspace()
            .worker(0)
            .unwrap()
            .manifest()
            .entry(Utf8Path::new("pkg/a.py"))
            .unwrap()
            .blake3
            .to_hex()
            .to_string();
        let candidate = MutationCandidate {
            id: "candidate".into(),
            sequence: 1,
            path: "pkg/a.py".into(),
            span: ByteSpan {
                start: 0,
                length: 8,
            },
            original: "original".into(),
            replacement: "mutated!".into(),
            operator: "test".into(),
            line: 1,
            column: 0,
            symbol: None,
            file_hash: hash,
        };
        let retry = grant.create_worker(EffectId(5), 0).unwrap();
        (project, context, candidate, retry)
    }

    async fn wait_until_path_is_removed(path: &Utf8Path) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("detached cleanup did not remove {path}"));
    }

    async fn wait_until_process_handler_is_released(process: &std::sync::Weak<ProcessHandler>) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while process.strong_count() != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("detached cleanup retained process backend ownership");
    }

    #[test]
    fn diagnostic_run_id_tracks_a_session_adopted_id() {
        let config = crate::cli::parse_config_from([
            "hoimin", "run", "--root", ".", "--source", ".", "--", "python", "-m", "pytest",
        ])
        .unwrap();
        let state = RunState::new("persisted-session-run", config);
        let mut diagnostic_run_id = "initial-run".to_owned();

        track_diagnostic_run_id(&mut diagnostic_run_id, &state);

        assert_eq!(diagnostic_run_id, "persisted-session-run");
    }

    #[test]
    fn shutdown_error_keeps_the_primary_failure_when_drain_succeeds() {
        assert_eq!(
            combine_shutdown_errors("transition rejected".to_owned(), None),
            "transition rejected"
        );
    }

    #[test]
    fn shutdown_error_appends_a_drain_failure_after_the_primary_failure() {
        assert_eq!(
            combine_shutdown_errors(
                "transition rejected".to_owned(),
                Some("process task failed while stopping: panic".to_owned()),
            ),
            "transition rejected; process task failed while stopping: panic"
        );
    }

    #[test]
    fn shutdown_budget_total_timeout_deadline_is_anchored_to_run_deadline() {
        let now = tokio::time::Instant::now();
        let run_deadline = now - Duration::from_secs(1);
        let grace = Duration::from_millis(25);

        let budget = ShutdownBudget::for_total_timeout_with_grace(run_deadline, grace);

        assert_eq!(budget.deadline(), run_deadline + grace);
        assert!(budget.deadline() < now);
    }

    #[test]
    fn shutdown_budget_first_activation_cannot_be_extended() {
        let now = tokio::time::Instant::now();
        let first = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            now,
            Duration::from_millis(10),
        );
        let later = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            now + Duration::from_secs(1),
            Duration::from_secs(5),
        );
        let mut active = None;

        let first_deadline = establish_shutdown_budget(&mut active, first).deadline();
        let retained = establish_shutdown_budget(&mut active, later);

        assert_eq!(retained.cause(), ShutdownCause::Cancellation);
        assert_eq!(retained.deadline(), first_deadline);
    }

    #[test]
    fn outer_failure_establishes_a_failure_budget_when_the_loop_has_none() {
        let now = tokio::time::Instant::now();
        let mut active = None;

        ensure_outer_finalization_budget(
            &mut active,
            true,
            now + Duration::from_secs(10),
            now,
            Duration::from_millis(70),
        );

        let budget = active.unwrap();
        assert_eq!(budget.cause(), ShutdownCause::Failure);
        assert_eq!(budget.deadline(), now + Duration::from_millis(70));
    }

    #[test]
    fn remaining_budget_observes_live_deadline_and_preserves_effect_id() {
        let now = tokio::time::Instant::now();
        let id = EffectId(17);

        let observed = remaining_budget_observed(
            &ObserveRemainingBudget { id },
            now + Duration::from_secs(281),
            now,
        );
        let expired = remaining_budget_observed(
            &ObserveRemainingBudget { id },
            now,
            now + Duration::from_secs(1),
        );

        assert_eq!(observed.id, id);
        assert_eq!(observed.remaining, Duration::from_secs(281));
        assert_eq!(expired.id, id);
        assert_eq!(expired.remaining, Duration::ZERO);
    }

    #[tokio::test]
    async fn blocking_effect_classification_covers_every_filesystem_variant() {
        let (_project, _context, candidate, create) = context_with_worker().await;
        let read = RunEffect::ReadCandidate(hoimin_core::ReadCandidate {
            id: EffectId(31),
            worker: 0,
            spool: hoimin_core::CandidateSpoolRef {
                token: "spool.jsonl".into(),
                records: 1,
            },
            cursor: hoimin_core::CandidateCursor::START,
        });
        let effects = [
            RunEffect::CreateWorker(create),
            read,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(32),
                worker: 0,
                candidate,
            }),
            RunEffect::ResetWorker(ResetWorker {
                id: EffectId(33),
                worker: 0,
            }),
            RunEffect::VerifyOriginals(VerifyOriginals {
                id: EffectId(34),
                checkpoint: IntegrityCheckpoint::PreFinalReport,
            }),
            RunEffect::Cleanup(Cleanup {
                id: EffectId(35),
                reservations: Vec::new(),
            }),
        ];

        assert!(effects.iter().all(is_blocking_io_effect));
        assert!(!is_blocking_io_effect(&process_effect(0)));
    }

    #[tokio::test]
    async fn owned_cleanup_restores_workspace_only_when_completion_is_accepted() {
        let (_project, mut context, _candidate, create) = context_with_worker().await;
        let task = prepare_blocking_effect(
            &mut context,
            RunEffect::Cleanup(Cleanup {
                id: EffectId(36),
                reservations: vec![create.reservation_id()],
            }),
        )
        .unwrap();
        assert!(context.workspace.is_none());

        let completion = task.execute();
        assert!(context.workspace.is_none());
        let event = accept_blocking_completion(&mut context, completion);

        assert!(matches!(event, RunEvent::CleanupFinished(_)));
        assert_eq!(context.workspace().worker_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_budget_preempts_an_owned_blocking_close() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            tokio::time::Instant::now(),
            Duration::from_millis(20),
        );

        let close = tokio::spawn(async move {
            run_owned_blocking_until(&budget, Box::new(|| {}), move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                17_u8
            })
            .await
        });
        entered_rx.await.unwrap();
        let error = close.await.unwrap().unwrap_err();
        release_tx.send(()).unwrap();

        let OwnedBlockingError::Expired(error) = error else {
            panic!("blocking close returned a join failure instead of expiry")
        };
        assert!(error.starts_with("cancellation: shutdown grace expired"));
        assert!(error.contains("blocking I/O tasks: 1"));
    }

    #[tokio::test]
    async fn expired_final_close_detaches_cleanup_without_extending_the_wait() {
        let (_project, mut context, _candidate, _create) = context_with_worker().await;
        let worker_root = context.workspace().worker(0).unwrap().root().to_owned();
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() - Duration::from_secs(1),
            Duration::ZERO,
        );
        let started = Instant::now();

        let close = close_context_resources(&mut context, &budget, false, Box::new(|| {})).await;

        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(
            close
                .expiry
                .is_some_and(|error| error.starts_with("total timeout: shutdown grace expired"))
        );
        assert!(
            context.workspace.is_none(),
            "detached cleanup owns the workspace after expiry"
        );
        wait_until_path_is_removed(&worker_root).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn successful_run_outer_close_uses_the_original_deadline_and_detaches_cleanup() {
        let (project, mut context, _candidate, _create) = context_with_worker().await;
        let worker_root = context.workspace().worker(0).unwrap().root().to_owned();
        let process = Arc::downgrade(&context.process);
        let run_deadline = tokio::time::Instant::now() + Duration::from_millis(20);
        let grace = Duration::from_millis(20);
        let mut active = None;
        let budget = ensure_outer_finalization_budget(
            &mut active,
            false,
            run_deadline,
            tokio::time::Instant::now(),
            grace,
        );
        assert_eq!(budget.cause(), ShutdownCause::TotalTimeout);
        assert_eq!(budget.deadline(), run_deadline + grace);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);

        let close = tokio::spawn(async move {
            let result = close_context_resources(
                &mut context,
                &budget,
                false,
                Box::new(move || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }),
            )
            .await;
            (project, context, result)
        });
        entered_rx.await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        let (_project, context, close) = close.await.unwrap();
        assert!(close.expiry.is_some());
        assert!(context.workspace.is_none());
        assert!(worker_root.exists(), "cleanup must still be paused");
        drop(context);

        release_tx.send(()).unwrap();
        wait_until_path_is_removed(&worker_root).await;
        wait_until_process_handler_is_released(&process).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn successful_run_metrics_use_the_original_total_timeout_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        let run_deadline = tokio::time::Instant::now() + Duration::from_millis(20);
        let grace = Duration::from_millis(20);
        let mut active = None;
        let budget = ensure_outer_finalization_budget(
            &mut active,
            false,
            run_deadline,
            tokio::time::Instant::now(),
            grace,
        );
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let task_path = path.clone();

        let finalize = tokio::spawn(async move {
            finalize_metrics_with_shutdown(
                task_path,
                Some(MetricsCollector::new("run-1")),
                None,
                0,
                0,
                Vec::new(),
                &budget,
                Box::new(move || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }),
            )
            .await
        });
        entered_rx.await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        let result = finalize.await.unwrap();
        release_tx.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(result.expiry.is_some());
        assert!(
            !path.exists(),
            "metrics write started after shutdown expiry"
        );
    }

    #[tokio::test]
    async fn interrupt_monitor_remains_live_while_outer_finalization_is_pending() {
        let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
        let (forced_tx, forced_rx) = tokio::sync::oneshot::channel();
        let monitor = crate::interrupt::spawn_test_monitor(signal_rx, move |code| {
            let _ = forced_tx.send(code);
        });
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let finalization = finish_with_interrupt_monitor(monitor, async move {
            let _ = release_rx.await;
        });
        tokio::pin!(finalization);

        tokio::select! {
            () = &mut finalization => panic!("outer finalization was not released"),
            () = tokio::task::yield_now() => {}
        }
        signal_tx.send(Ok(())).unwrap();
        signal_tx.send(Ok(())).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), forced_rx)
                .await
                .expect("second signal must retain process-level precedence")
                .unwrap(),
            130
        );
        release_tx.send(()).unwrap();
        finalization.await;
    }

    #[test]
    fn expired_shutdown_skips_metrics_with_an_incomplete_warning() {
        let mut warnings = vec![("metrics.state", "preserved warning".to_owned())];

        skip_expired_metrics_finalization(
            Some(MetricsCollector::new("run-1")),
            Some("primary failure"),
            &mut warnings,
        );

        assert_eq!(warnings[0].1, "preserved warning");
        assert_eq!(warnings[1].0, "metrics.incomplete");
        assert!(warnings[1].1.contains("primary failure"));
    }

    #[tokio::test]
    async fn shutdown_expiry_keeps_cause_before_an_immediate_join_failure() {
        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(async { panic!("controlled process join failure") });
        tokio::task::yield_now().await;
        let mut io_tasks = JoinSet::new();
        let (_sender, mut receiver) = mpsc::channel(1);
        let mut in_flight = 0;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::TotalTimeout,
            tokio::time::Instant::now(),
            Duration::from_secs(1),
        );

        let error = shutdown_expiry_error(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await;

        assert!(error.starts_with("total timeout: shutdown grace expired"));
        assert!(error.contains("process task failed while stopping"));
    }

    #[tokio::test]
    async fn failed_event_drain_expiry_keeps_effect_code_and_message() {
        let event = RunEvent::EffectFailed(EffectFailed::other(
            EffectId(91),
            "controlled.effect.code",
            "controlled effect message",
        ));
        let primary = failed_event_primary(&event);
        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(std::future::pending::<()>());
        let mut io_tasks = JoinSet::new();
        let (_sender, mut receiver) = mpsc::channel(1);
        let mut in_flight = 1;
        let mut expiry_reported = false;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_millis(10),
        );

        let drain = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await;
        let error = finish_failed_event_drain(primary, drain).unwrap_err();

        assert!(error.contains("controlled.effect.code"), "{error}");
        assert!(error.contains("controlled effect message"), "{error}");
        assert!(error.contains("shutdown grace expired"), "{error}");
    }

    #[tokio::test]
    async fn direct_workspace_effect_completes_before_returning_worker_access() {
        let (_project, mut context, candidate, _retry) = context_with_worker().await;

        let event = execute_direct_io_effect(
            &mut context,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(3),
                worker: 0,
                candidate,
            }),
        );

        assert!(matches!(event, RunEvent::MutationApplied(_)));
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_drain_accepts_owned_workspace_state_without_process_metrics() {
        let (_project, mut context, candidate, _retry) = context_with_worker().await;
        let effect = RunEffect::ApplyMutation(ApplyMutation {
            id: EffectId(51),
            worker: 0,
            candidate,
        });
        let task = prepare_blocking_effect(&mut context, effect).unwrap();
        assert!(context.workspace().worker(0).is_none());
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let task = BlockingEffect::TestCompletion {
            id: EffectId(51),
            operation: Box::new(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                BlockingEffectCompletion::Workspace(Box::new(match task {
                    BlockingEffect::Workspace(task) => task.execute(),
                    _ => unreachable!("prepared apply task was not a workspace task"),
                }))
            }),
        };
        let (sender, mut receiver) = mpsc::channel(1);
        let mut io_tasks = JoinSet::new();
        spawn_blocking_effect(task, sender, &mut io_tasks);
        entered_rx.await.unwrap();
        let mut process_tasks = JoinSet::new();
        let mut in_flight = 1;
        let mut metrics = Some(MetricsCollector::new("run-1"));
        let mut warnings = Vec::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );

        let mut drain = Box::pin(drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            |completion| {
                let event = accept_blocking_completion(&mut context, completion);
                assert!(matches!(event, RunEvent::MutationApplied(_)));
            },
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut drain)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        drain.await.unwrap();

        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
        assert!(metrics.unwrap().finish(0, 0).unwrap().workers.is_empty());
    }

    #[tokio::test]
    async fn shutdown_drain_expiry_reports_process_and_blocking_task_counts() {
        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(std::future::pending::<()>());
        let mut io_tasks = JoinSet::new();
        io_tasks.spawn(std::future::pending::<()>());
        let (_sender, mut receiver) = mpsc::channel(1);
        let mut in_flight = 2;
        let mut metrics = None;
        let mut warnings = Vec::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Cancellation,
            tokio::time::Instant::now(),
            Duration::from_millis(10),
        );

        let error = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await
        .unwrap_err();

        assert_eq!(
            error,
            "cancellation: shutdown grace expired after 2s (process tasks: 1, blocking I/O tasks: 1)"
        );
        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
    }

    #[tokio::test]
    async fn shutdown_drain_expiry_accepts_buffered_workspace_and_process_metrics() {
        let (_project, mut context, candidate, _retry) = context_with_worker().await;
        let task = prepare_blocking_effect(
            &mut context,
            RunEffect::ApplyMutation(ApplyMutation {
                id: EffectId(61),
                worker: 0,
                candidate,
            }),
        )
        .unwrap();
        assert!(context.workspace().worker(0).is_none());
        let blocking = task.execute();

        let mut metrics = Some(MetricsCollector::new("run-1"));
        metrics.as_mut().unwrap().queued(7).unwrap();
        metrics.as_mut().unwrap().process_started(7).unwrap();
        let mut warnings = Vec::new();
        let (sender, mut receiver) = mpsc::channel(2);
        sender
            .send(ShellCompletion {
                event: RunEvent::CancellationRequested,
                process_task: true,
                io_task: false,
                process: Some((7, true)),
                blocking: None,
            })
            .await
            .unwrap();
        sender
            .send(ShellCompletion {
                event: RunEvent::CancellationRequested,
                process_task: false,
                io_task: true,
                process: None,
                blocking: Some(Box::new(blocking)),
            })
            .await
            .unwrap();
        drop(sender);

        let mut process_tasks = JoinSet::new();
        process_tasks.spawn(std::future::pending::<()>());
        let mut io_tasks = JoinSet::new();
        io_tasks.spawn(std::future::pending::<()>());
        let mut in_flight = 2;
        let mut expiry_reported = false;
        let budget = ShutdownBudget::for_total_timeout_with_grace(
            tokio::time::Instant::now() - Duration::from_secs(1),
            Duration::ZERO,
        );

        let error = drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            |completion| {
                let event = accept_blocking_completion(&mut context, completion);
                assert!(matches!(event, RunEvent::MutationApplied(_)));
            },
        )
        .await
        .unwrap_err();

        assert!(error.starts_with("total timeout: shutdown grace expired after 2s"));
        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
        assert_eq!(
            context
                .workspace()
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
        assert!(metrics.unwrap().finish(1, 0).unwrap().workers.is_empty());
    }

    #[tokio::test]
    async fn blocking_io_keeps_the_async_runtime_responsive() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let operation = tokio::spawn(run_blocking_io(EffectId(17), move || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            42_u8
        }));

        entered_rx.await.unwrap();
        tokio::time::timeout(Duration::from_millis(100), async {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(1)).await;
        })
        .await
        .unwrap();
        release_tx.send(()).unwrap();

        assert_eq!(operation.await.unwrap().unwrap(), 42);
    }

    #[tokio::test]
    async fn blocking_io_join_failure_preserves_effect_identity() {
        let id = EffectId(23);

        let error = run_blocking_io(id, || -> () { panic!("controlled blocking panic") })
            .await
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "shell.blocking_io");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn blocking_io_effects_overlap_before_either_completes() {
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(2);
        let (release_first_tx, release_first_rx) = std::sync::mpsc::sync_channel(0);
        let (release_second_tx, release_second_rx) = std::sync::mpsc::sync_channel(0);
        let (completion_tx, mut completion_rx) = tokio::sync::mpsc::channel(2);
        let mut tasks = JoinSet::new();
        let first_entered = entered_tx.clone();
        spawn_blocking_effect(
            BlockingEffect::TestOperation {
                id: EffectId(41),
                operation: Box::new(move || {
                    first_entered.send(1).unwrap();
                    release_first_rx.recv().unwrap();
                    RunEvent::CancellationRequested
                }),
            },
            completion_tx.clone(),
            &mut tasks,
        );
        spawn_blocking_effect(
            BlockingEffect::TestOperation {
                id: EffectId(42),
                operation: Box::new(move || {
                    entered_tx.send(2).unwrap();
                    release_second_rx.recv().unwrap();
                    RunEvent::CancellationRequested
                }),
            },
            completion_tx,
            &mut tasks,
        );

        let mut entered = [
            entered_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            entered_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        ];
        entered.sort_unstable();
        assert_eq!(entered, [1, 2]);
        release_first_tx.send(()).unwrap();
        release_second_tx.send(()).unwrap();

        assert!(completion_rx.recv().await.unwrap().io_task);
        assert!(completion_rx.recv().await.unwrap().io_task);
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn original_change_during_materialization_fails_post_materialization_before_baseline() {
        let project = tempfile::tempdir().unwrap();
        let original = project.path().join("target.py");
        std::fs::write(&original, b"original\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--jobs"),
            OsString::from("2"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let (control, mut pause_controller) = RunControl::with_materialization_pause(0);
        let observed_control = control.clone();
        let mutation = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(5))
                .expect("worker 0 did not enter materialization before the bounded wait expired");
            let release = pause_controller.release_guard();
            let mutation = std::fs::write(original, b"changed during materialization\n");
            drop(release);
            mutation.expect("change original during active worker materialization");
        });
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = run_loop_with_control(config, &mut stdout, &mut stderr, control)
            .await
            .unwrap();
        mutation.await.unwrap();

        assert_ne!(exit, 0);
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("workspace.original.changed"), "{stderr}");
        assert_eq!(
            observed_control.max_process_tasks(),
            0,
            "the baseline must not be dispatched after post-materialization verification fails"
        );
    }

    fn paused_materialization_config(
        project: &tempfile::TempDir,
        total_timeout: &str,
    ) -> RunConfig {
        std::fs::write(project.path().join("target.py"), b"value = 1\n").unwrap();
        crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("target.py"),
            OsString::from("--total-timeout"),
            OsString::from(total_timeout),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn total_timeout_bounds_paused_materialization_at_first_shutdown_deadline() {
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "200ms");
        let (control, mut pause_controller) =
            RunControl::with_materialization_pause_and_shutdown_grace(0, Duration::from_millis(80));
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let controller = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(2))
                .expect("worker 0 did not enter materialization");
            let release = pause_controller.release_guard();
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            drop(release);
        });
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        let result = tokio::time::timeout(Duration::from_millis(500), &mut run).await;
        release_tx.send(()).unwrap();
        controller.await.unwrap();
        let Ok(result) = result else {
            let _ = tokio::time::timeout(Duration::from_secs(3), &mut run).await;
            panic!("paused materialization outlived the first shutdown deadline");
        };
        let error = result.unwrap_err();

        assert!(
            error.contains("total timeout: shutdown grace expired"),
            "{error}"
        );
        assert!(error.contains("blocking I/O tasks: 1"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repeated_cancellation_does_not_extend_paused_materialization_shutdown() {
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "5s");
        let (control, mut pause_controller) =
            RunControl::with_materialization_pause_and_shutdown_grace(
                0,
                Duration::from_millis(250),
            );
        let cancelling = control.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let controller = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(2))
                .expect("worker 0 did not enter materialization");
            let release = pause_controller.release_guard();
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            drop(release);
        });
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        cancelling.cancel();
        tokio::select! {
            result = &mut run => panic!("run finished before the second cancellation: {result:?}"),
            () = tokio::time::sleep(Duration::from_millis(150)) => {}
        }
        cancelling.cancel();

        // About 100 ms remains on the first 250 ms grace period. A reset to the
        // second cancellation would take another 250 ms and exceed this bound.
        let result = tokio::time::timeout(Duration::from_millis(180), &mut run).await;
        release_tx.send(()).unwrap();
        controller.await.unwrap();
        let Ok(result) = result else {
            let _ = tokio::time::timeout(Duration::from_secs(3), &mut run).await;
            panic!("a later cancellation extended the first shutdown deadline");
        };
        let error = result.unwrap_err();

        assert!(
            error.contains("cancellation: shutdown grace expired"),
            "{error}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_released_inside_grace_finishes_orderly() {
        let project = tempfile::tempdir().unwrap();
        let config = paused_materialization_config(&project, "5s");
        let (control, mut pause_controller) =
            RunControl::with_materialization_pause_and_shutdown_grace(
                0,
                Duration::from_millis(300),
            );
        let cancelling = control.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let controller = tokio::task::spawn_blocking(move || {
            pause_controller
                .wait_until_entered(Duration::from_secs(2))
                .expect("worker 0 did not enter materialization");
            let release = pause_controller.release_guard();
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            drop(release);
        });
        let mut run = Box::pin(run_loop_with_control(
            config,
            Vec::new(),
            Vec::new(),
            control,
        ));
        tokio::select! {
            result = &mut run => panic!("run finished before materialization paused: {result:?}"),
            result = entered_rx => result.expect("pause controller stopped before entry"),
        }
        cancelling.cancel();
        tokio::time::sleep(Duration::from_millis(30)).await;
        release_tx.send(()).unwrap();
        controller.await.unwrap();

        let exit = tokio::time::timeout(Duration::from_secs(2), &mut run)
            .await
            .expect("released cancellation must finish inside grace")
            .unwrap();

        assert_eq!(exit, 130);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "manual blocking-I/O scheduler performance evidence"]
    async fn benchmark_blocking_io_dispatch() {
        const OPERATIONS: usize = 4;
        const OPERATION_MILLIS: u64 = 50;
        let serial_started = std::time::Instant::now();
        for sequence in 0..OPERATIONS {
            run_blocking_io(EffectId(u64::try_from(sequence).unwrap() + 1), || {
                std::thread::sleep(Duration::from_millis(OPERATION_MILLIS));
            })
            .await
            .unwrap();
        }
        let serial_millis = serial_started.elapsed().as_millis();

        let (completion_tx, mut completion_rx) = tokio::sync::mpsc::channel(OPERATIONS);
        let mut tasks = JoinSet::new();
        let concurrent_started = std::time::Instant::now();
        let mut io_in_flight = 0_usize;
        let mut max_io_in_flight = 0_usize;
        for sequence in 0..OPERATIONS {
            spawn_blocking_effect(
                BlockingEffect::TestOperation {
                    id: EffectId(u64::try_from(sequence).unwrap() + 1),
                    operation: Box::new(|| {
                        std::thread::sleep(Duration::from_millis(OPERATION_MILLIS));
                        RunEvent::CancellationRequested
                    }),
                },
                completion_tx.clone(),
                &mut tasks,
            );
            io_in_flight += 1;
            max_io_in_flight = max_io_in_flight.max(io_in_flight);
        }
        drop(completion_tx);
        for _ in 0..OPERATIONS {
            completion_rx.recv().await.unwrap();
            io_in_flight -= 1;
        }
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
        let concurrent_millis = concurrent_started.elapsed().as_millis();
        assert_eq!(io_in_flight, 0);

        println!(
            "operations={OPERATIONS} operation_ms={OPERATION_MILLIS} serial_ms={serial_millis} concurrent_ms={concurrent_millis} max_io_in_flight={max_io_in_flight}"
        );
    }

    #[tokio::test]
    async fn fingerprint_recheck_rejects_aba_change_during_validated_preflight() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        let input = project.path().join("config.toml");
        std::fs::write(&input, "version = 'A'\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--fingerprint-include"),
            OsString::from("config.toml"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(41);
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        std::fs::write(&input, "version = 'B'\n").unwrap();
        let error = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                std::fs::write(&input, "version = 'A'\n").unwrap();
                let result =
                    recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id);
                std::fs::write(&input, "version = 'B'\n").unwrap();
                result
            })
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "plan.fingerprint_input.changed");
    }

    #[tokio::test]
    async fn fingerprint_recheck_rejects_delete_restore_delete_aba() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        let input = project.path().join("config.toml");
        std::fs::write(&input, "version = 'A'\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--fingerprint-file"),
            OsString::from("config.toml"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(43);
        context
            .fingerprint_copy_inputs
            .insert(Utf8PathBuf::from("config.toml"));
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        std::fs::remove_file(&input).unwrap();
        let error = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                std::fs::write(&input, "version = 'A'\n").unwrap();
                let result =
                    recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id);
                std::fs::remove_file(&input).unwrap();
                result
            })
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "plan.fingerprint_input.changed");
    }

    #[tokio::test]
    async fn fingerprint_recheck_rejects_add_remove_add_aba() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        std::fs::write(project.path().join("base.cfg"), "base = true\n").unwrap();
        let added = project.path().join("added.cfg");
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--fingerprint-include"),
            OsString::from("*.cfg"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(44);
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        std::fs::write(&added, "added = true\n").unwrap();
        let error = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                std::fs::remove_file(&added).unwrap();
                let result =
                    recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id);
                std::fs::write(&added, "added = true\n").unwrap();
                result
            })
            .unwrap_err();

        assert_eq!(error.id, id);
        assert_eq!(error.failure.code(), "plan.fingerprint_input.changed");
    }

    #[tokio::test]
    async fn fingerprint_recheck_accepts_unchanged_inputs_excluded_from_worker_copy() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("src");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("calc.py"), "def calc():\n    return 1\n").unwrap();
        std::fs::write(project.path().join(".gitignore"), "ignored.cfg\n").unwrap();
        std::fs::write(project.path().join("ignored.cfg"), "glob = true\n").unwrap();
        std::fs::write(project.path().join("excluded.toml"), "exact = true\n").unwrap();
        let config = crate::cli::parse_config_from([
            OsString::from("hoimin"),
            OsString::from("run"),
            OsString::from("--root"),
            project.path().as_os_str().to_owned(),
            OsString::from("--file"),
            OsString::from("src/calc.py"),
            OsString::from("--exclude"),
            OsString::from("excluded.toml"),
            OsString::from("--fingerprint-include"),
            OsString::from("*.cfg"),
            OsString::from("--fingerprint-file"),
            OsString::from("excluded.toml"),
            OsString::from("--allow-best-effort-memory"),
            OsString::from("--"),
            OsString::from("unused-test-command"),
        ])
        .unwrap();
        let config = prepare_run_config(config).unwrap();
        let mut context = ShellContext::new(&config, Vec::new(), Vec::new())
            .await
            .unwrap();
        let id = EffectId(42);
        let copied_at_start = context.fingerprint_copy_inputs.clone();

        let completed = context
            .workspace_mut()
            .handle_preflight_validated(hoimin_core::Preflight { id }, |root, manifest| {
                recheck_fingerprint_inputs(&config, root, manifest, &copied_at_start, id)
            })
            .unwrap();

        assert_eq!(completed.id, id);
    }

    fn assert_metrics_sidecar_finishes(
        metrics: Option<MetricsCollector>,
        warnings: &[(&'static str, String)],
    ) {
        assert!(warnings.is_empty(), "warnings={warnings:?}");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("metrics.json");
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        write_metrics(&path, &metrics).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn rejected_dispatch_does_not_start_a_queued_metrics_process() {
        let control = RunControl::new();
        control.cancel();
        let mut metrics = Some(MetricsCollector::new("run-1"));
        metrics.as_mut().unwrap().queued(0).unwrap();
        let mut warnings = Vec::new();

        let dispatch = accept_process_dispatch(&control, &mut metrics, &mut warnings, 0);

        assert!(dispatch.is_none());
        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert!(metrics.workers.is_empty());
    }

    #[test]
    fn cancellation_before_dispatch_discards_all_queued_process_metrics() {
        let mut metrics = Some(MetricsCollector::new("run-1"));
        for worker in [0, 1] {
            metrics.as_mut().unwrap().queued(worker).unwrap();
        }
        let mut effects = VecDeque::from([process_effect(0), process_effect(1)]);
        let mut warnings = Vec::new();

        let first = effects.pop_front().unwrap();
        cancel_queued_effect(&first, &mut metrics, &mut warnings);
        discard_queued_effects(&mut effects, &mut metrics, &mut warnings);

        assert_metrics_sidecar_finishes(metrics, &warnings);
    }

    #[test]
    fn process_preparation_failure_discards_its_queued_metric() {
        let mut metrics = Some(MetricsCollector::new("run-1"));
        metrics.as_mut().unwrap().queued(7).unwrap();
        let mut warnings = Vec::new();

        cancel_queued_worker(Some(7), &mut metrics, &mut warnings);

        assert_metrics_sidecar_finishes(metrics, &warnings);
    }

    #[test]
    fn ready_process_effects_are_recorded_as_queued() {
        let effects = [process_effect(3)];
        let mut metrics = Some(MetricsCollector::new("run-1"));
        let mut warnings = Vec::new();

        record_ready_processes(&effects, &mut metrics, &mut warnings);
        cancel_queued_effect(&effects[0], &mut metrics, &mut warnings);

        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert!(metrics.workers.is_empty());
    }

    #[tokio::test]
    async fn drain_closes_failed_and_cancelled_process_metrics() {
        let mut metrics = Some(MetricsCollector::new("run-1"));
        for worker in [0, 1] {
            metrics.as_mut().unwrap().queued(worker).unwrap();
            metrics.as_mut().unwrap().process_started(worker).unwrap();
        }
        let mut warnings = Vec::new();
        let mut tasks = JoinSet::new();
        let (sender, mut receiver) = mpsc::channel(2);
        sender
            .send(ShellCompletion {
                event: RunEvent::EffectFailed(hoimin_core::EffectFailed::other(
                    EffectId(1),
                    "process.spawn",
                    "failed",
                )),
                process_task: true,
                io_task: false,
                process: Some((0, true)),
                blocking: None,
            })
            .await
            .unwrap();
        sender
            .send(ShellCompletion {
                event: RunEvent::CancellationRequested,
                process_task: true,
                io_task: false,
                process: Some((1, true)),
                blocking: None,
            })
            .await
            .unwrap();
        drop(sender);
        let mut in_flight = 2;
        let mut io_tasks = JoinSet::new();
        let mut expiry_reported = false;
        let budget = ShutdownBudget::after_observation_with_grace(
            ShutdownCause::Failure,
            tokio::time::Instant::now(),
            Duration::from_secs(5),
        );

        drain_processes(
            &mut tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
            &budget,
            &mut expiry_reported,
            &mut metrics,
            &mut warnings,
            drop,
        )
        .await
        .unwrap();

        assert_eq!(in_flight, 0);
        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(2, 0).unwrap();
        assert_eq!(
            metrics
                .workers
                .iter()
                .map(|worker| worker.processes)
                .sum::<u64>(),
            0
        );
    }

    #[test]
    fn materialization_verification_keeps_copy_metrics_stage_open() {
        let mut collector = MetricsCollector::new("run-1");
        collector.begin_stage("copy").unwrap();
        let mut metrics = Some(collector);
        let mut warnings = Vec::new();

        observe_accepted_transition(
            &mut metrics,
            &mut warnings,
            RunPhase::Copy,
            RunPhase::MaterializationVerification,
            false,
            false,
            false,
        );

        assert!(warnings.is_empty());
        metrics
            .as_mut()
            .unwrap()
            .finish_stage("copy")
            .expect("post-materialization verification remains part of the copy stage");
    }

    #[test]
    fn direct_early_cleanup_finishes_the_departed_active_stage() {
        for (phase, stage) in [
            (RunPhase::Baseline, "baseline"),
            (RunPhase::Analyze, "analysis"),
        ] {
            let mut collector = MetricsCollector::new("run-1");
            collector.begin_stage(stage).unwrap();
            let mut metrics = Some(collector);
            let mut warnings = Vec::new();

            observe_accepted_transition(
                &mut metrics,
                &mut warnings,
                phase,
                RunPhase::Cleaning,
                false,
                false,
                false,
            );
            observe_accepted_transition(
                &mut metrics,
                &mut warnings,
                RunPhase::Cleaning,
                RunPhase::Finished,
                false,
                false,
                true,
            );

            assert!(warnings.is_empty());
            let metrics = metrics.unwrap().finish(0, 0).unwrap();
            assert!(metrics.stages.iter().any(|metric| metric.name == stage));
            assert!(metrics.stages.iter().any(|metric| metric.name == "cleanup"));
        }
    }

    #[test]
    fn leaving_cleanup_without_a_completion_signal_finishes_the_stage() {
        let mut collector = MetricsCollector::new("run-1");
        collector.begin_stage("cleanup").unwrap();
        let mut metrics = Some(collector);
        let mut warnings = Vec::new();

        observe_accepted_transition(
            &mut metrics,
            &mut warnings,
            RunPhase::Cleaning,
            RunPhase::Finished,
            false,
            false,
            false,
        );

        assert!(warnings.is_empty());
        let metrics = metrics.unwrap().finish(0, 0).unwrap();
        assert_eq!(metrics.stages.len(), 1);
        assert_eq!(metrics.stages[0].name, "cleanup");
    }

    #[tokio::test]
    async fn spawned_mutant_reports_completion_and_mutant_accounting() {
        let output = tempfile::tempdir().unwrap();
        let output = Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap();
        let process = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output,
        ));
        let request = RunProcess {
            id: EffectId(9),
            worker: Some(4),
            run_id: Some("run-1".into()),
            mutant_id: Some("mutant-1".into()),
            argv: vec![missing_executable_arg()],
            cwd: Utf8PathBuf::from("."),
            limits: hoimin_core::ProcessLimits {
                timeout: Duration::from_secs(1),
                max_output_bytes: 1,
                max_memory_bytes: 1,
                max_processes: 1,
            },
        };
        let control = RunControl::new();
        let dispatch = control.begin_dispatch().unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let mut tasks = JoinSet::new();

        spawn_process(
            process,
            request.into(),
            4,
            false,
            sender,
            &mut tasks,
            dispatch,
        );

        let completion = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("spawned process must complete")
            .expect("completion channel must remain open");
        assert!(matches!(completion.event, RunEvent::EffectFailed(_)));
        assert!(completion.process_task);
        assert_eq!(completion.process, Some((4, true)));
        tasks.join_next().await.unwrap().unwrap();
    }

    #[test]
    fn worker_metadata_removes_case_variants_and_baseline_mutant_leakage() {
        let mut environment = crate::workspace::CommandEnvironment {
            cwd: "worker/root".into(),
            env: BTreeMap::from([
                (
                    OsString::from("hoimin_worker_root"),
                    OsString::from("parent"),
                ),
                (OsString::from("HoImIn_RuN_Id"), OsString::from("parent")),
                (OsString::from("hoimin_mutant_id"), OsString::from("parent")),
            ]),
        };

        set_worker_metadata(&mut environment, Some("run-1"), None);

        assert_eq!(
            environment.env.get(&OsString::from("HOIMIN_WORKER_ROOT")),
            Some(&OsString::from("worker/root"))
        );
        assert_eq!(
            environment.env.get(&OsString::from("HOIMIN_RUN_ID")),
            Some(&OsString::from("run-1"))
        );
        assert!(environment.env.keys().all(|key| {
            !key.to_string_lossy()
                .eq_ignore_ascii_case("HOIMIN_MUTANT_ID")
        }));
        assert_eq!(
            environment
                .env
                .keys()
                .filter(|key| key.to_string_lossy().eq_ignore_ascii_case("HOIMIN_RUN_ID"))
                .count(),
            1
        );
    }

    #[test]
    fn first_interrupt_maps_success_to_cancellation_and_preserves_failure() {
        assert!(matches!(
            first_interrupt_event(Ok(())),
            Ok(RunEvent::CancellationRequested)
        ));
        let error =
            first_interrupt_event(Err("install Ctrl+C handler: fixture".to_owned())).unwrap_err();
        assert_eq!(error, "install Ctrl+C handler: fixture");
    }
}
