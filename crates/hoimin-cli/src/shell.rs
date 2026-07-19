use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use camino::Utf8PathBuf;
use hoimin_core::{
    CandidateLoaded, CommandArg, EffectFailed, EffectId, FingerprintInput, ProcessLimits,
    ProcessTermination, RunConfig, RunEffect, RunEvent, RunPhase, RunProcess, RunState, SourceHash,
    StartRequested, TargetSlice, fingerprint, transition,
};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::analyzer::{AnalyzerHandler, CandidateStore};
use crate::process::{ProcessCancellation, ProcessHandler, ProcessRequest, ProcessStartGate};
use crate::report::ReportHandler;
#[cfg(not(any(windows, target_os = "linux")))]
use crate::resource::PortableBackend;
use crate::resource::ResourceBackend;
use crate::session::SessionHandler;
use crate::target::TargetHandler;
use crate::workspace::{CopyOptions, WorkspaceHandler};

#[derive(Clone, Debug)]
pub struct RunControl {
    request: ProcessStartGate,
    max_process_tasks: Arc<AtomicUsize>,
    max_completion_in_flight: Arc<AtomicUsize>,
}

impl RunControl {
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

    pub fn max_process_tasks(&self) -> usize {
        self.max_process_tasks.load(Ordering::Acquire)
    }

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
}

pub struct ShellContext<Stdout, Stderr> {
    workspace: WorkspaceHandler,
    analyzer: AnalyzerHandler,
    process: Arc<ProcessHandler>,
    report: ReportHandler<Stdout, Stderr>,
    session: Option<SessionHandler>,
    session_path: Option<Utf8PathBuf>,
    active_candidates: BTreeMap<u32, hoimin_core::MutationCandidate>,
    _spool_dir: TempDir,
    resolved_targets: Option<Vec<TargetSlice>>,
    config: RunConfig,
}

impl<Stdout, Stderr> ShellContext<Stdout, Stderr>
where
    Stdout: Write,
    Stderr: Write,
{
    pub async fn new(config: &RunConfig, stdout: Stdout, stderr: Stderr) -> Result<Self, String> {
        let python = config.python.clone().unwrap_or_default();
        let spool_dir = tempfile::tempdir().map_err(|error| error.to_string())?;
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
            python.clone(),
            config.limits.analyzer_timeout.get(),
            backend,
            config.limits.max_memory.get(),
            config.limits.max_processes.get() as u32,
        )
        .map_err(|error| error.to_string())?;
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
        })
    }
}

async fn python_versions<Stdout, Stderr>(
    context: &ShellContext<Stdout, Stderr>,
    id: EffectId,
    cancellation: ProcessCancellation,
) -> Result<(String, String), String> {
    let python = context
        .config
        .python
        .as_ref()
        .ok_or("--python is required")?;
    let request = RunProcess {
        id,
        worker: None,
        run_id: None,
        mutant_id: None,
        argv: [
            python.as_str(),
            "-c",
            "import sys; from importlib.metadata import version; print(sys.version.split()[0]); print(version('libcst'))",
        ]
        .into_iter()
        .map(command_arg)
        .collect(),
        cwd: context.config.root.clone(),
        limits: ProcessLimits {
            timeout: context.config.limits.analyzer_timeout.get(),
            max_output_bytes: 4096,
            max_memory_bytes: context.config.limits.max_memory.get(),
            max_processes: context.config.limits.max_processes.get() as u32,
        },
    };
    let finished = context
        .process
        .run(ProcessRequest::from(request).with_cancellation(cancellation))
        .await
        .map_err(|error| format!("verify Python/LibCST: {:?}", error.failure))?;
    if finished.termination != ProcessTermination::Exit(0) {
        return Err(format!(
            "verify Python/LibCST exited as {:?}",
            finished.termination
        ));
    }
    let path = context
        .process
        .spool_path(&finished.output)
        .map_err(|error| error.to_string())?;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut lines = text.lines();
    let python = lines.next().ok_or("missing Python version")?.to_owned();
    let libcst = lines.next().ok_or("missing LibCST version")?.to_owned();
    Ok((python, libcst))
}

#[cfg(windows)]
fn command_arg(value: &str) -> CommandArg {
    CommandArg::Windows(value.encode_utf16().collect())
}

#[cfg(unix)]
fn command_arg(value: &str) -> CommandArg {
    CommandArg::Unix(value.as_bytes().to_vec())
}

fn mutation_operators() -> Vec<String> {
    [
        "compare_eq_ne",
        "compare_order",
        "membership",
        "identity",
        "boolean_and_or",
        "binary_add_sub",
        "augmented_add_sub",
        "binary_mul_div",
        "binary_floor_mod",
        "unary_sign",
        "break_continue",
        "remove_not",
        "boolean_literal",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

async fn prepare_fingerprint<Stdout, Stderr>(
    context: &ShellContext<Stdout, Stderr>,
    cancellation: ProcessCancellation,
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
    let (python_version, libcst_version) = python_versions(context, id, cancellation)
        .await
        .map_err(|error| EffectFailed::other(id, "fingerprint.runtime", error))?;
    Ok(fingerprint(&FingerprintInput {
        sources,
        targets: targets.clone(),
        operators: mutation_operators(),
        test_argv: context.config.test_argv.clone(),
        limits: context.config.limits.clone(),
        python_version,
        libcst_version,
        resource_mode: context.process.mode(),
    }))
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

async fn execute_effect_with_cancellation<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    effect: RunEffect,
    cancellation: ProcessCancellation,
) -> RunEvent
where
    Stdout: Write,
    Stderr: Write,
{
    let id = effect.id();
    let result: Result<RunEvent, EffectFailed> = match effect {
        RunEffect::ResolveTargets(request) => match TargetHandler::handle(request).await {
            Ok(value) => {
                context.resolved_targets = Some(value.targets.clone());
                Ok(RunEvent::TargetsResolved(value))
            }
            Err(error) => Err(error),
        },
        RunEffect::Preflight(request) => match context.workspace.handle_preflight(request) {
            Ok(mut value) => match prepare_fingerprint(context, cancellation.clone()).await {
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
        },
        RunEffect::CreateWorker(request) => context
            .workspace
            .handle_create_worker(request)
            .map(RunEvent::WorkerCreated),
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
            .handle_with_cancellation(request, cancellation.clone())
            .await
            .map(RunEvent::AnalysisFinished),
        RunEffect::ReadCandidate(request) => {
            match CandidateStore::replay_one(&request.spool, request.offset) {
                Ok(Some((candidate, next_offset))) => {
                    context
                        .active_candidates
                        .insert(request.worker, candidate.clone());
                    Ok(RunEvent::CandidateLoaded(CandidateLoaded {
                        id: request.id,
                        worker: request.worker,
                        candidate: Some(candidate),
                        next_offset,
                    }))
                }
                Ok(None) => {
                    context.active_candidates.remove(&request.worker);
                    Ok(RunEvent::CandidateLoaded(CandidateLoaded {
                        id: request.id,
                        worker: request.worker,
                        candidate: None,
                        next_offset: request.offset,
                    }))
                }
                Err(error) => Err(EffectFailed::other(
                    id,
                    "candidate.replay",
                    error.to_string(),
                )),
            }
        }
        RunEffect::ApplyMutation(request) => match context.active_candidates.get(&request.worker) {
            Some(candidate) => context
                .workspace
                .handle_apply_mutation(request, candidate)
                .map(RunEvent::MutationApplied),
            None => Err(EffectFailed::other(
                id,
                "shell.candidate.missing",
                "active candidate is missing",
            )),
        },
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
        RunEffect::EmitOutput(request) => {
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
        RunEffect::LoadSession(request) => match session(context, id) {
            Ok(handler) => handler.load(request).map(RunEvent::SessionLoaded),
            Err(error) => Err(error),
        },
        RunEffect::LookupStoredResult(request) => match session(context, id) {
            Ok(handler) => handler.lookup(request).map(RunEvent::StoredResultLoaded),
            Err(error) => Err(error),
        },
        RunEffect::BeginSession(request) => match session(context, id) {
            Ok(handler) => handler.begin(request).map(RunEvent::SessionStarted),
            Err(error) => Err(error),
        },
        RunEffect::PersistResult(request) => match session(context, id) {
            Ok(handler) => handler.persist(request).map(RunEvent::ResultPersisted),
            Err(error) => Err(error),
        },
        RunEffect::FinishSession(request) => match session(context, id) {
            Ok(handler) => handler.finish(request).map(RunEvent::SessionFinished),
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

fn session<Stdout, Stderr>(
    context: &mut ShellContext<Stdout, Stderr>,
    id: EffectId,
) -> Result<&mut SessionHandler, EffectFailed> {
    if context.session.is_none() {
        let path = context.session_path.as_ref().ok_or_else(|| {
            EffectFailed::other(id, "session.missing", "session effect without --session")
        })?;
        context.session = Some(
            SessionHandler::open(path)
                .map_err(|error| EffectFailed::other(id, "session.open", error.to_string()))?,
        );
    }
    Ok(context.session.as_mut().expect("initialized above"))
}

pub async fn run_loop<Stdout, Stderr>(
    config: RunConfig,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<i32, String>
where
    Stdout: Write,
    Stderr: Write,
{
    run_loop_with_control(config, stdout, stderr, RunControl::new()).await
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
    let mut context = ShellContext::new(&config, stdout, stderr).await?;
    let deadline = tokio::time::Instant::now() + config.limits.total_timeout.get();
    let max_jobs = config.limits.jobs.get();
    let channel_capacity = config.limits.jobs.get().saturating_add(1);
    let run_result = async {
        let mut state = RunState::new(Uuid::new_v4().to_string(), config);
        let (next, initial) = transition(state, RunEvent::StartRequested(StartRequested))
            .map_err(|error| error.to_string())?;
        state = next;
        let mut effects = VecDeque::from(initial);
        let cancellation = ProcessCancellation::new();
        let (completion_tx, mut completion_rx) = mpsc::channel(channel_capacity);
        let mut process_tasks = JoinSet::new();
        let mut in_flight = 0_usize;
        let mut stop_signalled = false;
        let mut ctrl_c = Box::pin(tokio::signal::ctrl_c());

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
                    continue;
                }
                match effect {
                    RunEffect::RunBaseline(request) => {
                        let id = request.id;
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                if !spawn_process(
                                    Arc::clone(&context.process),
                                    request,
                                    true,
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    &control,
                                ) {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    priority_event = Some(RunEvent::CancellationRequested);
                                    break;
                                }
                            }
                            Err(error) => {
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
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
                        match worker_process_request(
                            &context,
                            id,
                            request,
                            cancellation.clone(),
                            Some(control.start_gate()),
                        ) {
                            Ok(request) => {
                                if !spawn_process(
                                    Arc::clone(&context.process),
                                    request,
                                    false,
                                    completion_tx.clone(),
                                    &mut process_tasks,
                                    &control,
                                ) {
                                    cancellation.cancel();
                                    stop_signalled = true;
                                    priority_event = Some(RunEvent::CancellationRequested);
                                    break;
                                }
                            }
                            Err(error) => {
                                serial_completion = Some(ShellCompletion {
                                    event: RunEvent::EffectFailed(error),
                                    process_task: false,
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
                                signal = &mut ctrl_c => {
                                    cancellation.cancel();
                                    let _ = execution.await;
                                    stop_signalled = true;
                                    match ctrl_c_event(signal) {
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
                let drain_failure =
                    drain_processes(&mut process_tasks, &mut completion_rx, &mut in_flight)
                        .await
                        .err();
                return Err(match drain_failure {
                    Some(drain_failure) => format!("{error}; {drain_failure}"),
                    None => error,
                });
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
                        }
                    }
                    () = tokio::time::sleep_until(deadline) => {
                        cancellation.cancel();
                        stop_signalled = true;
                        ShellCompletion {
                            event: RunEvent::DeadlineReached,
                            process_task: false,
                        }
                    }
                    signal = &mut ctrl_c => {
                        cancellation.cancel();
                        stop_signalled = true;
                        match ctrl_c_event(signal) {
                            Ok(event) => ShellCompletion {
                                event,
                                process_task: false,
                            },
                            Err(error) => {
                                signal_failure = Some(error);
                                ShellCompletion {
                                    event: RunEvent::CancellationRequested,
                                    process_task: false,
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
                let drain_failure =
                    drain_processes(&mut process_tasks, &mut completion_rx, &mut in_flight)
                        .await
                        .err();
                return Err(match drain_failure {
                    Some(drain_failure) => format!("{error}; {drain_failure}"),
                    None => error,
                });
            }
            let ShellCompletion {
                event,
                process_task: process_completion,
            } = completion;
            let external_stop = matches!(
                event,
                RunEvent::DeadlineReached | RunEvent::CancellationRequested
            );
            let failed = matches!(event, RunEvent::EffectFailed(_));
            if !external_stop && process_completion {
                in_flight = in_flight.saturating_sub(1);
            }
            if failed {
                cancellation.cancel();
                stop_signalled = true;
            }
            let transition_result = transition(state, event);
            let (next, produced) = match transition_result {
                Ok(value) => value,
                Err(error) => {
                    cancellation.cancel();
                    drain_processes(&mut process_tasks, &mut completion_rx, &mut in_flight).await?;
                    return Err(error.to_string());
                }
            };
            state = next;

            if external_stop || failed {
                effects.clear();
                drain_processes(&mut process_tasks, &mut completion_rx, &mut in_flight).await?;
            } else {
                if process_completion {
                    let process_failure = match process_tasks.join_next().await {
                        Some(Ok(())) => None,
                        Some(Err(error)) => Some(format!("process task failed: {error}")),
                        None => Some("process completion had no task".to_owned()),
                    };
                    if let Some(process_failure) = process_failure {
                        cancellation.cancel();
                        let drain_failure =
                            drain_processes(&mut process_tasks, &mut completion_rx, &mut in_flight)
                                .await
                                .err();
                        return Err(match drain_failure {
                            Some(drain_failure) => {
                                format!("{process_failure}; {drain_failure}")
                            }
                            None => process_failure,
                        });
                    }
                }
            }
            effects.extend(produced);
        }
        Ok(state.exit_code())
    }
    .await;
    let close_result = context.process.close().map_err(|error| error.to_string());
    let workspace_close = context.workspace.close().map_err(|error| error.to_string());
    combine_close_results(run_result, workspace_close, close_result)
}

fn ctrl_c_event(signal: std::io::Result<()>) -> Result<RunEvent, String> {
    signal
        .map(|()| RunEvent::CancellationRequested)
        .map_err(|error| format!("install Ctrl+C handler: {error}"))
}

fn spawn_process(
    process: Arc<ProcessHandler>,
    request: ProcessRequest,
    baseline: bool,
    sender: mpsc::Sender<ShellCompletion>,
    tasks: &mut JoinSet<()>,
    control: &RunControl,
) -> bool {
    let Some(_dispatch) = control.begin_dispatch() else {
        return false;
    };
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
            })
            .await;
    });
    true
}

async fn drain_processes(
    tasks: &mut JoinSet<()>,
    receiver: &mut mpsc::Receiver<ShellCompletion>,
    in_flight: &mut usize,
) -> Result<(), String> {
    let mut first_failure = None;
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result
            && first_failure.is_none()
        {
            first_failure = Some(format!("process task failed while stopping: {error}"));
        }
    }
    while receiver.try_recv().is_ok() {
        *in_flight = in_flight.saturating_sub(1);
    }
    *in_flight = 0;
    match first_failure {
        Some(error) => Err(error),
        None => Ok(()),
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
    fn ctrl_c_handler_failure_is_infrastructure_not_cancellation() {
        assert!(matches!(
            ctrl_c_event(Ok(())),
            Ok(RunEvent::CancellationRequested)
        ));
        let error = ctrl_c_event(Err(std::io::Error::other("fixture"))).unwrap_err();
        assert!(error.contains("install Ctrl+C handler"));
        assert!(error.contains("fixture"));
    }
}
