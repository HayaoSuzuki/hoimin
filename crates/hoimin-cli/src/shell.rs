use std::collections::VecDeque;
use std::io::Write;

use camino::Utf8PathBuf;
use hoimin_core::{
    CandidateLoaded, CommandArg, EffectFailed, EffectId, FingerprintInput, ProcessLimits,
    ProcessTermination, RunConfig, RunEffect, RunEvent, RunPhase, RunProcess, RunState, SourceHash,
    StartRequested, TargetSlice, fingerprint, transition,
};
use tempfile::TempDir;
use uuid::Uuid;

use crate::analyzer::{AnalyzerHandler, CandidateStore};
use crate::process::{ProcessCancellation, ProcessHandler, ProcessRequest};
use crate::report::ReportHandler;
#[cfg(not(any(windows, target_os = "linux")))]
use crate::resource::PortableBackend;
use crate::resource::ResourceBackend;
use crate::session::SessionHandler;
use crate::target::TargetHandler;
use crate::workspace::{CopyOptions, WorkspaceHandler};

pub struct ShellContext<Stdout, Stderr> {
    workspace: WorkspaceHandler,
    analyzer: AnalyzerHandler,
    process: ProcessHandler,
    report: ReportHandler<Stdout, Stderr>,
    session: Option<SessionHandler>,
    session_path: Option<Utf8PathBuf>,
    active_candidate: Option<hoimin_core::MutationCandidate>,
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
        let python = config
            .python
            .clone()
            .ok_or_else(|| "--python is required for analyzer execution".to_owned())?;
        let spool_dir = tempfile::tempdir().map_err(|error| error.to_string())?;
        let spool_path = Utf8PathBuf::from_path_buf(spool_dir.path().to_owned())
            .map_err(|_| "temporary spool path is not UTF-8".to_owned())?;
        std::fs::create_dir_all(spool_path.join("report")).map_err(|error| error.to_string())?;
        let source_roots = config.selection.sources.clone();
        let workspace = WorkspaceHandler::new(
            config.root.clone(),
            source_roots,
            1,
            CopyOptions {
                includes: config.selection.includes.clone(),
                excludes: config.selection.excludes.clone(),
            },
        );
        let backend = resource_backend(config).map_err(|error| error.to_string())?;
        let process = ProcessHandler::new(backend.clone(), spool_path.join("process"));
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
            active_candidate: None,
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
            match worker_process_request(context, id, request, cancellation.clone()) {
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
                    context.active_candidate = Some(candidate.clone());
                    Ok(RunEvent::CandidateLoaded(CandidateLoaded {
                        id: request.id,
                        candidate: Some(candidate),
                        next_offset,
                    }))
                }
                Ok(None) => {
                    context.active_candidate = None;
                    Ok(RunEvent::CandidateLoaded(CandidateLoaded {
                        id: request.id,
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
        RunEffect::ApplyMutation(request) => match context.active_candidate.as_ref() {
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
            match worker_process_request(context, id, request, cancellation.clone()) {
                Ok(request) => context
                    .process
                    .run(request)
                    .await
                    .map(RunEvent::MutantFinished),
                Err(error) => Err(error),
            }
        }
        RunEffect::ResetWorker(request) => context
            .workspace
            .handle_reset_worker(request)
            .map(RunEvent::WorkerReset),
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
) -> Result<ProcessRequest, EffectFailed> {
    let inherited = std::env::vars_os().collect();
    let environment = context
        .workspace
        .command_environment(0, &inherited)
        .map_err(|error| EffectFailed::other(id, "shell.worker.environment", error.to_string()))?;
    request.cwd.clone_from(&environment.cwd);
    Ok(ProcessRequest::from(request)
        .with_environment(environment)
        .with_cancellation(cancellation))
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
    let mut context = ShellContext::new(&config, stdout, stderr).await?;
    let deadline = tokio::time::Instant::now() + config.limits.total_timeout.get();
    let run_result = async {
        let mut state = RunState::new(Uuid::new_v4().to_string(), config);
        let (next, initial) = transition(state, RunEvent::StartRequested(StartRequested))
            .map_err(|error| error.to_string())?;
        state = next;
        let mut effects = VecDeque::from(initial);
        let mut deadline_signalled = false;
        while let Some(effect) = effects.pop_front() {
            if !state.is_effect_pending(effect.id()) {
                continue;
            }
            let event = if deadline_signalled
                || matches!(state.phase(), RunPhase::Finalize | RunPhase::Cleaning)
            {
                execute_effect(&mut context, effect).await
            } else if tokio::time::Instant::now() >= deadline {
                deadline_signalled = true;
                let (next, produced) = transition(state, RunEvent::DeadlineReached)
                    .map_err(|error| error.to_string())?;
                state = next;
                effects.extend(produced);
                continue;
            } else {
                let cancellation = ProcessCancellation::new();
                let mut execution = Box::pin(execute_effect_with_cancellation(
                    &mut context,
                    effect,
                    cancellation.clone(),
                ));
                tokio::select! {
                    event = &mut execution => event,
                    () = tokio::time::sleep_until(deadline) => {
                        cancellation.cancel();
                        let _ = execution.await;
                        deadline_signalled = true;
                        let (next, produced) = transition(state, RunEvent::DeadlineReached)
                            .map_err(|error| error.to_string())?;
                        state = next;
                        effects.extend(produced);
                        continue;
                    }
                }
            };
            if !deadline_signalled
                && !matches!(state.phase(), RunPhase::Finalize | RunPhase::Cleaning)
                && tokio::time::Instant::now() >= deadline
            {
                // Synchronous handlers cannot be aborted safely mid-filesystem operation.
                // Retire their completion immediately after they return and let core drive
                // integrity verification and cleanup.
                deadline_signalled = true;
                let (next, produced) = transition(state, RunEvent::DeadlineReached)
                    .map_err(|error| error.to_string())?;
                state = next;
                effects.extend(produced);
                continue;
            }
            let (next, produced) = transition(state, event).map_err(|error| error.to_string())?;
            state = next;
            effects.extend(produced);
        }
        if state.phase() != RunPhase::Finished {
            return Err(format!("run stalled in {:?}", state.phase()));
        }
        Ok(state.exit_code())
    }
    .await;
    let close_result = context.process.close().map_err(|error| error.to_string());
    let workspace_close = context.workspace.close().map_err(|error| error.to_string());
    combine_close_results(run_result, workspace_close, close_result)
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
