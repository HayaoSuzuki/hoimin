use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

#[derive(Clone, Debug)]
pub struct RunControl {
    request: ProcessStartGate,
    max_process_tasks: Arc<AtomicUsize>,
    max_completion_in_flight: Arc<AtomicUsize>,
}

impl RunControl {
    #[must_use]
    pub fn new() -> Self {
        Self {
            request: ProcessStartGate::new(),
            max_process_tasks: Arc::new(AtomicUsize::new(0)),
            max_completion_in_flight: Arc::new(AtomicUsize::new(0)),
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
}

impl BlockingEffect {
    fn id(&self) -> EffectId {
        match self {
            Self::Workspace(task) => task.id(),
            Self::Candidate(request) => request.id,
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
        RunEffect::CreateWorker(request) => context.workspace.prepare_create_task(request),
        RunEffect::ReadCandidate(request) => return Ok(BlockingEffect::Candidate(request)),
        RunEffect::ApplyMutation(request) => {
            let candidate = request.candidate.clone();
            context.active_candidates.insert(request.worker, candidate);
            context.workspace.prepare_apply_task(request)
        }
        RunEffect::ResetWorker(request) => context.workspace.prepare_reset_task(request),
        RunEffect::VerifyOriginals(request) => context.workspace.prepare_verify_task(request),
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
            .workspace
            .accept_task_completion(*completion)
            .unwrap_or_else(RunEvent::EffectFailed),
        BlockingEffectCompletion::Candidate(event) => *event,
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
            .workspace
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
                .workspace
                .handle_apply_mutation(request, &candidate)
                .map(RunEvent::MutationApplied)
        }
        RunEffect::ResetWorker(request) => {
            let worker = request.worker;
            let result = context
                .workspace
                .handle_reset_worker(request)
                .map(RunEvent::WorkerReset);
            if result.is_ok() {
                context.active_candidates.remove(&worker);
            }
            result
        }
        RunEffect::VerifyOriginals(request) => context
            .workspace
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

pub struct ShellContext<Stdout, Stderr> {
    workspace: WorkspaceHandler,
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
            workspace,
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
    if is_blocking_io_effect(&effect) {
        return execute_direct_io_effect(context, effect);
    }
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
        RunEffect::Cleanup(request) => match context.process.close() {
            Ok(()) => context
                .workspace
                .handle_cleanup(request)
                .map(RunEvent::CleanupFinished),
            Err(error) => Err(EffectFailed::other(
                id,
                "process.resource.close",
                error.to_string(),
            )),
        },
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
    if let Some(fingerprint_copy_inputs) = fingerprint_copy_inputs {
        context.fingerprint_copy_inputs = fingerprint_copy_inputs;
    }
    let deadline = tokio::time::Instant::now() + config.limits.total_timeout.get();
    let max_jobs = config.limits.jobs.get();
    let channel_capacity = config.limits.jobs.get().saturating_add(1);
    let mut metrics = None;
    let mut metrics_warnings = Vec::new();
    let mut discovered = 0_u64;
    let mut executed = 0_u64;
    let initial_run_id = Uuid::new_v4().to_string();
    let mut diagnostic_run_id = initial_run_id.clone();
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
        let mut interrupts = crate::interrupt::InterruptMonitor::spawn();

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
                    priority_event = Some(if control.is_cancelled() {
                        RunEvent::CancellationRequested
                    } else {
                        RunEvent::DeadlineReached
                    });
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
                                    priority_event = Some(RunEvent::CancellationRequested);
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
                                    priority_event = Some(RunEvent::CancellationRequested);
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
                            execute_effect_with_cancellation(
                                &mut context,
                                effect,
                                cancellation.clone(),
                            )
                            .await
                        } else {
                            let mut execution = Box::pin(execute_effect_with_cancellation(
                                &mut context,
                                effect,
                                cancellation.clone(),
                            ));
                            tokio::select! {
                                biased;
                                event = &mut execution => event,
                                () = control.cancelled() => {
                                    cancellation.cancel();
                                    let _ = execution.await;
                                    stop_signalled = true;
                                    RunEvent::CancellationRequested
                                }
                                () = tokio::time::sleep_until(deadline) => {
                                    cancellation.cancel();
                                    let _ = execution.await;
                                    stop_signalled = true;
                                    RunEvent::DeadlineReached
                                }
                                signal = interrupts.first() => {
                                    cancellation.cancel();
                                    let _ = execution.await;
                                    stop_signalled = true;
                                    match first_interrupt_event(signal) {
                                        Ok(event) => event,
                                        Err(error) => {
                                            signal_failure = Some(error);
                                            RunEvent::CancellationRequested
                                        }
                                    }
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
                let drain_failure = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
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
                completion_rx
                    .recv()
                    .await
                    .ok_or_else(|| "completion channel closed".to_owned())?
            } else {
                tokio::select! {
                    biased;
                    () = control.cancelled() => {
                        cancellation.cancel();
                        stop_signalled = true;
                        ShellCompletion {
                            event: RunEvent::CancellationRequested,
                            process_task: false,
                            io_task: false,
                            process: None,
                            blocking: None,
                        }
                    }
                    () = tokio::time::sleep_until(deadline) => {
                        cancellation.cancel();
                        stop_signalled = true;
                        ShellCompletion {
                            event: RunEvent::DeadlineReached,
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
                            Ok(event) => ShellCompletion {
                                event,
                                process_task: false,
                                io_task: false,
                                process: None,
                                blocking: None,
                            },
                            Err(error) => {
                                signal_failure = Some(error);
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
                let drain_failure = drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
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
            let failed = matches!(event, RunEvent::EffectFailed(_));
            if !external_stop && (process_completion || io_completion) {
                in_flight = in_flight.saturating_sub(1);
            }
            if !external_stop && io_completion {
                io_in_flight = io_in_flight.saturating_sub(1);
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
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
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
                drain_processes(
                    &mut process_tasks,
                    &mut io_tasks,
                    &mut completion_rx,
                    &mut in_flight,
                    &mut metrics,
                    &mut metrics_warnings,
                    |completion| {
                        let _ = accept_blocking_completion(&mut context, completion);
                    },
                )
                .await?;
            } else if process_completion {
                let process_failure = match process_tasks.join_next().await {
                    Some(Ok(())) => None,
                    Some(Err(error)) => Some(format!("process task failed: {error}")),
                    None => Some("process completion had no task".to_owned()),
                };
                if let Some(process_failure) = process_failure {
                    cancellation.cancel();
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
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
                    let drain_failure = drain_processes(
                        &mut process_tasks,
                        &mut io_tasks,
                        &mut completion_rx,
                        &mut in_flight,
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
    let close_result = context.process.close().map_err(|error| error.to_string());
    let workspace_close = context.workspace.close().map_err(|error| error.to_string());
    if let Some(path) = metrics_path {
        finalize_metrics(
            path.as_std_path(),
            metrics,
            run_result.as_ref().err().map(String::as_str),
            discovered,
            executed,
            &mut metrics_warnings,
        );
        for (code, message) in metrics_warnings {
            emit_metrics_warning(&mut context, &diagnostic_run_id, code, message);
        }
    }
    combine_close_results(
        run_result.map(|(exit_code, _)| exit_code),
        workspace_close,
        close_result,
    )
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
            if let Some(stage) = phase_stage(previous)
                && !(previous == RunPhase::Cleaning && cleanup_finished)
            {
                metrics.finish_stage(stage)?;
            }
            if let Some(stage) = phase_stage(next) {
                metrics.begin_stage(stage)?;
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
        RunPhase::Copy => Some("copy"),
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
                    BlockingEffectCompletion::Candidate(event) => event.as_ref().clone(),
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

async fn drain_processes(
    process_tasks: &mut JoinSet<()>,
    io_tasks: &mut JoinSet<()>,
    receiver: &mut mpsc::Receiver<ShellCompletion>,
    in_flight: &mut usize,
    metrics: &mut Option<MetricsCollector>,
    metrics_warnings: &mut Vec<(&'static str, String)>,
    mut accept_blocking: impl FnMut(BlockingEffectCompletion),
) -> Result<(), String> {
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
            }
        }
    }
    while let Ok(completion) = receiver.try_recv() {
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
    *in_flight = 0;
    match first_failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn combine_shutdown_errors(primary: String, drain_failure: Option<String>) -> String {
    match drain_failure {
        Some(drain_failure) => format!("{primary}; {drain_failure}"),
        None => primary,
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
    use std::time::Duration;

    use hoimin_core::{
        ApplyMutation, BudgetLedger, ByteSpan, CommandArg, CreateWorker, IntegrityCheckpoint,
        MutationCandidate, ObserveRemainingBudget, Preflight, ResetWorker, RunBudgets,
        VerifyOriginals, reserve_workspace_copy,
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
            .workspace
            .handle_preflight(Preflight { id: EffectId(1) })
            .unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        context
            .workspace
            .handle_create_worker(grant.create_worker(EffectId(2), 0).unwrap())
            .unwrap();
        let hash = context
            .workspace
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
        ];

        assert!(effects.iter().all(is_blocking_io_effect));
        assert!(!is_blocking_io_effect(&process_effect(0)));
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
                .workspace
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
        assert!(context.workspace.worker(0).is_none());
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

        let mut drain = Box::pin(drain_processes(
            &mut process_tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
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
                .workspace
                .worker(0)
                .unwrap()
                .read("pkg/a.py")
                .unwrap(),
            b"mutated!\n"
        );
        assert!(metrics.unwrap().finish(0, 0).unwrap().workers.is_empty());
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
            .workspace
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
            .workspace
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
            .workspace
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
            .workspace
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

        drain_processes(
            &mut tasks,
            &mut io_tasks,
            &mut receiver,
            &mut in_flight,
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
