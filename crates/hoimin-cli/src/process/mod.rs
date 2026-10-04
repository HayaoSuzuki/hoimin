mod blocking;
mod output;

use std::ffi::OsString;
use std::future::Future;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use hoimin_core::{
    CommandArg, EffectFailed, EffectFailure, EffectId, OutputSpoolRef,
    PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE, ProcessFinished, ProcessOutputState, ProcessTermination,
    RunProcess, contract_ensure,
};
use tokio::process::{Child, Command};
use tokio::sync::Notify;
use tracing::Instrument;
use uuid::Uuid;

use crate::resource::{ProcessSupervisor, ResourceBackend, ResourceError};
use crate::workspace::CommandEnvironment;
use blocking::BlockingOwner;

const POST_TERMINATION_GRACE: Duration = Duration::from_secs(1);
const PROCESS_TREE_QUIESCENCE_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug, Eq, PartialEq)]
enum ProcessSelection<T> {
    Cancelled,
    Timeout,
    Exited(T),
}

#[derive(Clone, Debug)]
pub struct ProcessCancellation {
    state: Arc<CancellationState>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessStartGate {
    state: Arc<StartGateState>,
}

#[derive(Debug)]
struct StartGateState {
    cancelled: AtomicBool,
    notify: Notify,
    spawn_gate: Mutex<()>,
}

impl ProcessStartGate {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(StartGateState {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
                spawn_gate: Mutex::new(()),
            }),
        }
    }

    pub(crate) fn cancel(&self) {
        let _spawn = self.begin_spawn();
        if !self.state.cancelled.swap(true, Ordering::AcqRel) {
            self.state.notify.notify_waiters();
        }
    }

    pub(crate) async fn cancelled(&self) {
        loop {
            let notified = self.state.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn begin_spawn(&self) -> std::sync::MutexGuard<'_, ()> {
        self.state
            .spawn_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
    spawn_gate: Mutex<()>,
}

impl ProcessCancellation {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(CancellationState {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
                spawn_gate: Mutex::new(()),
            }),
        }
    }

    pub fn cancel(&self) {
        let _spawn = self
            .state
            .spawn_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.state.cancelled.swap(true, Ordering::AcqRel) {
            self.state.notify.notify_waiters();
        }
    }

    pub(crate) async fn cancelled(&self) {
        loop {
            let notified = self.state.notify.notified();
            if self.state.cancelled.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    fn begin_spawn(&self) -> std::sync::MutexGuard<'_, ()> {
        self.state
            .spawn_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for ProcessCancellation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct ProcessRequest {
    process: RunProcess,
    cancellation: ProcessCancellation,
    start_gate: Option<ProcessStartGate>,
    environment: Option<CommandEnvironment>,
}

impl ProcessRequest {
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: ProcessCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    pub(crate) fn with_start_gate(mut self, start_gate: ProcessStartGate) -> Self {
        self.start_gate = Some(start_gate);
        self
    }

    #[must_use]
    pub fn with_environment(mut self, environment: CommandEnvironment) -> Self {
        self.environment = Some(environment);
        self
    }
}

impl From<RunProcess> for ProcessRequest {
    fn from(process: RunProcess) -> Self {
        Self {
            process,
            cancellation: ProcessCancellation::new(),
            start_gate: None,
            environment: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("invalid output spool token: {0}")]
    InvalidSpoolToken(String),
}

#[derive(Clone, Debug)]
pub struct ProcessHandler {
    backend: ResourceBackend,
    output_dir: Utf8PathBuf,
    active_processes: Arc<AtomicUsize>,
    active_output_drains: Arc<AtomicUsize>,
    process_reap_failed: Arc<AtomicBool>,
    output_drain_failed: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessDrainReport {
    pub all_reaped: bool,
    pub output_drains_joined: bool,
    pub secondary_errors: Vec<String>,
}

struct ActiveCounter<'a>(&'a AtomicUsize);

impl<'a> ActiveCounter<'a> {
    fn enter(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(counter)
    }
}

struct CompletionGuard<'a> {
    _active: ActiveCounter<'a>,
    failed: &'a AtomicBool,
    completed: bool,
}

struct OutputDrainOutcome {
    result: Result<OutputSpoolRef, EffectFailed>,
    quiescent: bool,
}

impl<'a> CompletionGuard<'a> {
    fn enter(active: &'a AtomicUsize, failed: &'a AtomicBool) -> Self {
        Self {
            _active: ActiveCounter::enter(active),
            failed,
            completed: false,
        }
    }

    fn complete(&mut self) {
        self.completed = true;
    }
}

impl Drop for CompletionGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.failed.store(true, Ordering::Release);
        }
    }
}

impl Drop for ActiveCounter<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl ProcessHandler {
    #[must_use]
    pub fn new(backend: ResourceBackend, output_dir: Utf8PathBuf) -> Self {
        Self {
            backend,
            output_dir,
            active_processes: Arc::new(AtomicUsize::new(0)),
            active_output_drains: Arc::new(AtomicUsize::new(0)),
            process_reap_failed: Arc::new(AtomicBool::new(false)),
            output_drain_failed: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(test)]
    pub(crate) fn inject_process_reap_failure(&self) {
        self.process_reap_failed.store(true, Ordering::Release);
    }

    /// Returns the spool path represented by `output`.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessError::InvalidSpoolToken`] if `output` does not contain a UUID token.
    pub fn spool_path(&self, output: &OutputSpoolRef) -> Result<Utf8PathBuf, ProcessError> {
        Uuid::parse_str(&output.token)
            .map_err(|_| ProcessError::InvalidSpoolToken(output.token.clone()))?;
        Ok(self.output_dir.join(format!("{}.bin", output.token)))
    }

    /// Returns the selected backend description for run initialization.
    #[must_use]
    pub fn resource_control(&self) -> hoimin_core::ResourceControl {
        self.backend.resource_control()
    }

    #[must_use]
    pub fn mode(&self) -> hoimin_core::ResourceMode {
        self.backend.mode()
    }

    /// Releases resources owned by the process backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot release its resources.
    pub fn close(&self) -> Result<(), ResourceError> {
        self.backend.close()
    }

    pub async fn drain_for_shutdown(&self, budget: Duration) -> ProcessDrainReport {
        let deadline = tokio::time::Instant::now()
            .checked_add(budget)
            .unwrap_or_else(tokio::time::Instant::now);
        loop {
            let process_tasks_settled = self.active_processes.load(Ordering::Acquire) == 0;
            let output_tasks_settled = self.active_output_drains.load(Ordering::Acquire) == 0;
            if process_tasks_settled && output_tasks_settled {
                let all_reaped = !self.process_reap_failed.load(Ordering::Acquire);
                let output_drains_joined = !self.output_drain_failed.load(Ordering::Acquire);
                let backend = self.backend.clone();
                let close = tokio::task::spawn_blocking(move || backend.close());
                let secondary_errors = match tokio::time::timeout_at(deadline, close).await {
                    Ok(Ok(Ok(()))) => Vec::new(),
                    Ok(Ok(Err(error))) => vec![format!("process.resource.close: {error}")],
                    Ok(Err(error)) => vec![format!(
                        "process.resource.close: process resource close task failed: {error}"
                    )],
                    Err(_) => vec![
                        "process.resource.close: process resource close exceeded shutdown budget"
                            .to_owned(),
                    ],
                };
                return ProcessDrainReport {
                    all_reaped,
                    output_drains_joined,
                    secondary_errors,
                };
            }
            if tokio::time::Instant::now() >= deadline {
                return ProcessDrainReport {
                    all_reaped: false,
                    output_drains_joined: false,
                    secondary_errors: Vec::new(),
                };
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Runs a process with default cancellation and environment settings.
    ///
    /// # Errors
    ///
    /// Returns an effect failure when process setup, execution, termination, or output collection fails.
    pub async fn handle(&self, request: RunProcess) -> Result<ProcessFinished, EffectFailed> {
        self.run(request.into()).await
    }

    /// Runs a process request, including cancellation, resource supervision, and output collection.
    ///
    /// # Errors
    ///
    /// Returns an effect failure for cancellation, invalid arguments, resource setup, process I/O,
    /// termination, or output collection errors.
    #[tracing::instrument(name = "test_process", level = "debug", skip_all, fields(effect_id = request.process.id.0, worker = request.process.worker, mutant_id = request.process.mutant_id.as_deref()))]
    pub async fn run(&self, request: ProcessRequest) -> Result<ProcessFinished, EffectFailed> {
        let id = request.process.id;
        let handler = self.clone();
        tracing::debug!("starting test process");
        match tokio::spawn(async move { handler.run_owned(request).await }.in_current_span()).await
        {
            Ok(result) => result,
            Err(error) => Err(EffectFailed::other(
                id,
                "process.lifecycle.join",
                format!("owned process lifecycle task failed: {error}"),
            )),
        }
    }

    // This coordinates the complete child lifecycle; extracting stages would obscure cleanup ordering.
    #[allow(clippy::too_many_lines)]
    async fn run_owned(&self, request: ProcessRequest) -> Result<ProcessFinished, EffectFailed> {
        let ProcessRequest {
            process,
            cancellation,
            start_gate,
            environment,
        } = request;
        let id = process.id;
        if cancellation.is_cancelled() {
            return Err(EffectFailed::other(
                id,
                "process.cancelled.before_spawn",
                "process was cancelled before spawn",
            ));
        }
        let argv = native_argv(&process.argv)
            .map_err(|message| EffectFailed::other(id, "process.argv.invalid", message))?;
        if argv.is_empty() {
            return Err(EffectFailed::other(
                id,
                "process.argv.empty",
                "process argv must not be empty",
            ));
        }
        let argv = self.backend.wrap_argv(argv);
        let Some((program, arguments)) = argv.split_first() else {
            return Err(EffectFailed::other(
                id,
                "process.argv.backend_invalid",
                "resource backend produced an empty argv",
            ));
        };
        tokio::fs::create_dir_all(&self.output_dir)
            .await
            .map_err(|error| {
                io_failure(
                    id,
                    "process.output.create",
                    "create output directory",
                    Some(self.output_dir.clone()),
                    &error,
                )
            })?;

        let mut command = Command::new(program);
        command.args(arguments);
        if let Some(environment) = environment {
            command
                .current_dir(environment.cwd)
                .env_clear()
                .envs(environment.env);
        } else {
            command.current_dir(&process.cwd);
        }
        command
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let supervisor = self
            .backend
            .prepare(&mut command, process.limits)
            .map_err(|error| {
                resource_failure(
                    id,
                    "process.resource.setup",
                    "prepare resource supervisor",
                    &error,
                )
            })?;
        let mut supervisor = BlockingOwner::new(supervisor);
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + process.limits.timeout;
        let start_guard = start_gate.as_ref().map(ProcessStartGate::begin_spawn);
        if start_gate
            .as_ref()
            .is_some_and(ProcessStartGate::is_cancelled)
        {
            return Err(EffectFailed::other(
                id,
                "process.cancelled.before_spawn",
                "process was cancelled before spawn",
            ));
        }
        let spawn_guard = cancellation.begin_spawn();
        if cancellation.is_cancelled() {
            return Err(EffectFailed::other(
                id,
                "process.cancelled.before_spawn",
                "process was cancelled before spawn",
            ));
        }
        let mut active_process =
            CompletionGuard::enter(&self.active_processes, &self.process_reap_failed);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                active_process.complete();
                return Err(io_failure(
                    id,
                    "process.spawn",
                    "spawn process",
                    Some(process.cwd.clone()),
                    &error,
                ));
            }
        };
        if let Err(error) = supervisor.value_mut(id)?.attach(&child) {
            let terminate_error = child.start_kill().err();
            drop(spawn_guard);
            drop(start_guard);
            let cleanup_error = match terminate_error {
                Some(error) => Some(error),
                None => match tokio::time::timeout(POST_TERMINATION_GRACE, child.wait()).await {
                    Ok(Ok(_)) => None,
                    Ok(Err(error)) => Some(error),
                    Err(_) => Some(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "timed out waiting for unattached child termination",
                    )),
                },
            };
            if cleanup_error.is_none() {
                supervisor.value_mut(id)?.record_root_reaped();
            }
            let supervisor_cleanup = supervisor.finish(id).await;
            if cleanup_error.is_none() && supervisor_cleanup.is_ok() {
                active_process.complete();
            }
            let mut primary = attach_failure(id, &error, cleanup_error.as_ref());
            if let Err(cleanup) = supervisor_cleanup {
                append_cleanup_failure(&mut primary, "supervisor destruction failed", &cleanup);
            }
            return Err(primary);
        }
        drop(spawn_guard);
        drop(start_guard);

        let stdout = child.stdout.take().ok_or_else(|| {
            EffectFailed::other(id, "process.stdout.missing", "stdout pipe was not created")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            EffectFailed::other(id, "process.stderr.missing", "stderr pipe was not created")
        })?;
        let token = Uuid::new_v4().to_string();
        let output_ref = OutputSpoolRef {
            token: token.clone(),
            retained: 0,
            observed: 0,
        };
        let spool_path = self
            .spool_path(&output_ref)
            .map_err(|error| EffectFailed::other(id, "process.output.token", error.to_string()))?;
        let (sender, receiver) = output::pipe_channel();
        let mut active_output_drain =
            CompletionGuard::enter(&self.active_output_drains, &self.output_drain_failed);
        let stdout_task = tokio::spawn(output::drain_pipe(stdout, sender.clone()));
        let stderr_task = tokio::spawn(output::drain_pipe(stderr, sender));
        let collector_task = tokio::spawn(output::collect_output(
            spool_path,
            token,
            process.limits.max_output_bytes,
            receiver,
        ));

        let (mut process_result, mut process_reaped) = match select_process_result(
            child.wait(),
            cancellation.cancelled(),
            tokio::time::sleep_until(deadline),
        )
        .await
        {
            ProcessSelection::Exited(status) => match status {
                Ok(status) => {
                    classify_and_terminate(id, &mut supervisor, exit_termination(status)).await
                }
                Err(error) => {
                    let cleanup = terminate_and_reap(id, &mut supervisor, &mut child).await;
                    let reaped = cleanup.as_ref().is_ok_and(|quiescent| *quiescent);
                    (
                        Err(preserve_primary_after_cleanup(
                            io_failure(id, "process.wait", "wait for process", None, &error),
                            "supervised cleanup after wait error failed",
                            async { cleanup.map(|_| ()) },
                        )
                        .await),
                        reaped,
                    )
                }
            },
            ProcessSelection::Cancelled => {
                let result = terminate_and_reap(id, &mut supervisor, &mut child).await;
                let reaped = result.as_ref().is_ok_and(|quiescent| *quiescent);
                (result.map(|_| ProcessTermination::Cancelled), reaped)
            }
            ProcessSelection::Timeout => {
                let result = terminate_and_reap(id, &mut supervisor, &mut child).await;
                let reaped = result.as_ref().is_ok_and(|quiescent| *quiescent);
                (result.map(|_| ProcessTermination::Timeout), reaped)
            }
        };

        if let Err(cleanup) = supervisor.finish(id).await {
            process_reaped = false;
            match &mut process_result {
                Err(primary) => {
                    append_cleanup_failure(primary, "supervisor destruction failed", &cleanup);
                }
                Ok(_) => process_result = Err(cleanup),
            }
        }
        if process_reaped {
            active_process.complete();
        }
        drop(active_process);

        let output_outcome = await_output(
            id,
            stdout_task,
            stderr_task,
            collector_task,
            tokio::time::Instant::now() + POST_TERMINATION_GRACE,
        )
        .await;
        if output_outcome.quiescent {
            active_output_drain.complete();
        }
        drop(active_output_drain);
        let (termination, output, output_state) = combine_process_and_output(
            process_result,
            output_outcome.result,
            process.mutant_id.as_deref(),
            output_ref,
        )?;
        contract_ensure!(
            "process.output.post",
            output.retained <= process.limits.max_output_bytes
                && output.retained <= output.observed,
            &output
        );
        tracing::debug!(
            ?termination,
            elapsed_ms = started.elapsed().as_millis(),
            "test process finished"
        );
        Ok(ProcessFinished {
            id,
            worker: process.worker,
            termination,
            output_state,
            output,
            elapsed: started.elapsed(),
            resource_mode: self.backend.mode(),
        })
    }
}

async fn select_process_result<T, W, C, D>(
    wait: W,
    cancellation: C,
    deadline: D,
) -> ProcessSelection<T>
where
    W: Future<Output = T>,
    C: Future<Output = ()>,
    D: Future<Output = ()>,
{
    tokio::select! {
        biased;
        () = cancellation => ProcessSelection::Cancelled,
        () = deadline => ProcessSelection::Timeout,
        value = wait => ProcessSelection::Exited(value),
    }
}

async fn await_output(
    id: EffectId,
    mut stdout_task: tokio::task::JoinHandle<std::io::Result<()>>,
    mut stderr_task: tokio::task::JoinHandle<std::io::Result<()>>,
    mut collector_task: tokio::task::JoinHandle<std::io::Result<OutputSpoolRef>>,
    deadline: tokio::time::Instant,
) -> OutputDrainOutcome {
    if let Err(error) = await_pipe_until(id, "stdout", &mut stdout_task, deadline).await {
        abort_and_join(&mut stderr_task).await;
        let collector = await_collector(id, &mut collector_task, deadline).await;
        return OutputDrainOutcome {
            result: Err(error),
            quiescent: collector.quiescent,
        };
    }
    if let Err(error) = await_pipe_until(id, "stderr", &mut stderr_task, deadline).await {
        let collector = await_collector(id, &mut collector_task, deadline).await;
        return OutputDrainOutcome {
            result: Err(error),
            quiescent: collector.quiescent,
        };
    }
    await_collector(id, &mut collector_task, deadline).await
}

async fn await_collector(
    id: EffectId,
    collector_task: &mut tokio::task::JoinHandle<std::io::Result<OutputSpoolRef>>,
    deadline: tokio::time::Instant,
) -> OutputDrainOutcome {
    if let Ok(result) = tokio::time::timeout_at(deadline, &mut *collector_task).await {
        let result = match result {
            Ok(result) => result.map_err(|error| {
                io_failure(
                    id,
                    "process.output.write",
                    "write output spool",
                    None,
                    &error,
                )
            }),
            Err(error) => Err(EffectFailed::other(
                id,
                "process.output.join",
                error.to_string(),
            )),
        };
        OutputDrainOutcome {
            result,
            quiescent: true,
        }
    } else {
        abort_and_join(collector_task).await;
        OutputDrainOutcome {
            result: Err(EffectFailed::other(
                id,
                "process.output.close.timeout",
                "timed out draining process output after termination",
            )),
            quiescent: false,
        }
    }
}

async fn await_pipe_until(
    id: EffectId,
    name: &'static str,
    task: &mut tokio::task::JoinHandle<std::io::Result<()>>,
    deadline: tokio::time::Instant,
) -> Result<(), EffectFailed> {
    if let Ok(result) = tokio::time::timeout_at(deadline, &mut *task).await {
        result
            .map_err(|error| {
                EffectFailed::other(id, format!("process.{name}.join"), error.to_string())
            })?
            .map_err(|error| {
                io_failure(
                    id,
                    format!("process.{name}.read"),
                    "drain process pipe",
                    None,
                    &error,
                )
            })
    } else {
        abort_and_join(task).await;
        Err(EffectFailed::other(
            id,
            "process.output.close.timeout",
            "timed out draining process output after termination",
        ))
    }
}

async fn abort_and_join<T>(task: &mut tokio::task::JoinHandle<T>) {
    task.abort();
    let _ = task.await;
}

pub(crate) async fn wait_after_termination(
    id: EffectId,
    child: &mut Child,
) -> Result<(), EffectFailed> {
    if let Ok(result) = tokio::time::timeout(POST_TERMINATION_GRACE, child.wait()).await {
        return result.map(|_| ()).map_err(|error| {
            io_failure(
                id,
                "process.wait",
                "wait after process termination",
                None,
                &error,
            )
        });
    }

    let kill_failure = child.start_kill().err().map(|error| {
        io_failure(
            id,
            "process.kill",
            "kill root after wait timeout",
            None,
            &error,
        )
    });
    let wait_result = tokio::time::timeout(POST_TERMINATION_GRACE, child.wait()).await;
    let wait_failure = match wait_result {
        Ok(Ok(_)) => None,
        Ok(Err(error)) => Some(io_failure(
            id,
            "process.wait",
            "wait after root kill",
            None,
            &error,
        )),
        Err(_) => Some(EffectFailed::other(
            id,
            "process.wait.timeout",
            "timed out reaping root after kill",
        )),
    };

    match (kill_failure, wait_failure) {
        (None, None) => Ok(()),
        (Some(error), None) | (None, Some(error)) => Err(error),
        (Some(mut primary), Some(cleanup)) => {
            append_cleanup_failure(&mut primary, "root wait also failed", &cleanup);
            Err(primary)
        }
    }
}

fn failure_message(failure: &EffectFailed) -> &str {
    match &failure.failure {
        EffectFailure::Io { message, .. } | EffectFailure::Other { message, .. } => message,
        _ => "non-process cleanup failure",
    }
}

fn append_cleanup_failure(primary: &mut EffectFailed, label: &str, cleanup: &EffectFailed) {
    let detail = failure_message(cleanup);
    match &mut primary.failure {
        EffectFailure::Io { message, .. } | EffectFailure::Other { message, .. } => {
            message.push_str("; ");
            message.push_str(label);
            message.push_str(": ");
            message.push_str(detail);
        }
        _ => unreachable!("process cleanup produces only I/O or other failures"),
    }
}

async fn preserve_primary_after_cleanup<F>(
    mut primary: EffectFailed,
    label: &str,
    cleanup: F,
) -> EffectFailed
where
    F: Future<Output = Result<(), EffectFailed>>,
{
    if let Err(cleanup) = cleanup.await {
        append_cleanup_failure(&mut primary, label, &cleanup);
    }
    primary
}

#[cfg(test)]
async fn wait_failure_after_cleanup<F>(
    id: EffectId,
    error: &std::io::Error,
    cleanup: F,
) -> EffectFailed
where
    F: Future<Output = Result<(), EffectFailed>>,
{
    let primary = io_failure(id, "process.wait", "wait for process", None, error);
    preserve_primary_after_cleanup(
        primary,
        "supervised cleanup after wait error failed",
        cleanup,
    )
    .await
}

async fn classify_and_terminate(
    id: EffectId,
    supervisor: &mut BlockingOwner<ProcessSupervisor>,
    termination: ProcessTermination,
) -> (Result<ProcessTermination, EffectFailed>, bool) {
    match supervisor.value_mut(id) {
        Ok(supervisor) => supervisor.record_root_reaped(),
        Err(error) => return (Err(error), false),
    }
    let classification = supervisor
        .run(id, move |supervisor| supervisor.classify(termination))
        .await
        .and_then(|result| {
            result.map_err(|error| {
                resource_failure(
                    id,
                    "process.resource.classify",
                    "classify process termination",
                    &error,
                )
            })
        });
    let termination = match terminate_supervised(id, supervisor, false).await {
        Ok(true) => Ok(true),
        Ok(false) => await_tree_quiescence_after_root_reap(id, supervisor).await,
        Err(mut primary) => {
            if let Err(retry) = terminate_supervised(id, supervisor, false).await {
                append_cleanup_failure(&mut primary, "supervisor termination retry failed", &retry);
            }
            Err(primary)
        }
    };

    let reaped = termination.as_ref().is_ok_and(|quiescent| *quiescent);
    let result = match (classification, termination) {
        (Ok(classified), Ok(_)) => Ok(classified),
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
        (Err(mut primary), Err(cleanup)) => {
            append_cleanup_failure(&mut primary, "supervised termination also failed", &cleanup);
            Err(primary)
        }
    };
    (result, reaped)
}

async fn terminate_and_reap(
    id: EffectId,
    supervisor: &mut BlockingOwner<ProcessSupervisor>,
    child: &mut Child,
) -> Result<bool, EffectFailed> {
    match terminate_supervised(id, supervisor, true).await {
        Ok(_) => {
            wait_after_termination(id, child).await?;
            supervisor.value_mut(id)?.record_root_reaped();
            await_tree_quiescence_after_root_reap(id, supervisor).await
        }
        Err(mut primary) => {
            let root_kill = child.start_kill().err().map(|error| {
                io_failure(
                    id,
                    "process.kill",
                    "kill root after supervisor termination failure",
                    None,
                    &error,
                )
            });
            let tree_retry = terminate_supervised(id, supervisor, true).await.err();
            let root_wait = wait_after_termination(id, child).await.err();
            if root_wait.is_none()
                && let Ok(supervisor) = supervisor.value_mut(id)
            {
                supervisor.record_root_reaped();
            }

            for (label, cleanup) in [
                ("direct root kill failed", root_kill),
                ("supervisor termination retry failed", tree_retry),
                ("root reap failed", root_wait),
            ] {
                if let Some(cleanup) = cleanup {
                    append_cleanup_failure(&mut primary, label, &cleanup);
                }
            }
            Err(primary)
        }
    }
}

async fn await_tree_quiescence_after_root_reap(
    id: EffectId,
    supervisor: &mut BlockingOwner<ProcessSupervisor>,
) -> Result<bool, EffectFailed> {
    // Keep this proof attempt strictly inside the shell's fixed two-second shutdown grace.
    // Failure to prove absence is safe: the lifecycle retains the workspace for the janitor.
    // Output drains and the completion channel still need time to settle after this check.
    let deadline = tokio::time::Instant::now() + PROCESS_TREE_QUIESCENCE_GRACE;
    loop {
        let quiescent = supervisor
            .run(
                id,
                ProcessSupervisor::refresh_tree_quiescence_after_root_reap,
            )
            .await?
            .map_err(|error| {
                resource_failure(
                    id,
                    "process.resource.quiescence",
                    "verify process-tree quiescence after root reap",
                    &error,
                )
            })?;
        if quiescent {
            return Ok(true);
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn combine_process_and_output(
    process: Result<ProcessTermination, EffectFailed>,
    output: Result<OutputSpoolRef, EffectFailed>,
    mutant_id: Option<&str>,
    fallback_output: OutputSpoolRef,
) -> Result<(ProcessTermination, OutputSpoolRef, ProcessOutputState), EffectFailed> {
    match process {
        Ok(termination) => match output {
            Ok(output) => Ok((termination, output, ProcessOutputState::Complete)),
            Err(error)
                if mutant_id.is_some()
                    && error.failure.code() == PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE =>
            {
                Ok((
                    termination,
                    fallback_output,
                    ProcessOutputState::CloseTimedOut,
                ))
            }
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn attach_failure(
    id: EffectId,
    attach: &ResourceError,
    cleanup: Option<&std::io::Error>,
) -> EffectFailed {
    let mut message = attach.to_string();
    if let Some(cleanup) = cleanup {
        message.push_str("; spawned-child cleanup failed: ");
        message.push_str(&cleanup.to_string());
    }
    EffectFailed {
        id,
        failure: EffectFailure::Io {
            code: "process.resource.attach".into(),
            operation: "attach resource supervisor".into(),
            path: None,
            message,
        },
    }
}

async fn terminate_supervised(
    id: EffectId,
    supervisor: &mut BlockingOwner<ProcessSupervisor>,
    live_root_owned: bool,
) -> Result<bool, EffectFailed> {
    supervisor
        .run(id, move |supervisor| supervisor.terminate(live_root_owned))
        .await?
        .map_err(|error| {
            resource_failure(
                id,
                "process.resource.terminate",
                "terminate supervised process tree",
                &error,
            )
        })
}

fn io_failure(
    id: EffectId,
    code: impl Into<String>,
    operation: impl Into<String>,
    path: Option<Utf8PathBuf>,
    error: &std::io::Error,
) -> EffectFailed {
    EffectFailed {
        id,
        failure: EffectFailure::Io {
            code: code.into(),
            operation: operation.into(),
            path,
            message: error.to_string(),
        },
    }
}

fn resource_failure(
    id: EffectId,
    code: &'static str,
    operation: &'static str,
    error: &ResourceError,
) -> EffectFailed {
    EffectFailed {
        id,
        failure: EffectFailure::Io {
            code: code.into(),
            operation: operation.into(),
            path: None,
            message: error.to_string(),
        },
    }
}

pub(crate) fn exit_termination(status: std::process::ExitStatus) -> ProcessTermination {
    if let Some(code) = status.code() {
        return ProcessTermination::Exit(code);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ProcessTermination::Exit(status.signal().map_or(-1, |signal| 128 + signal))
    }
    #[cfg(not(unix))]
    {
        ProcessTermination::Exit(-1)
    }
}

#[cfg(unix)]
fn native_argv(argv: &[CommandArg]) -> Result<Vec<OsString>, String> {
    use std::os::unix::ffi::OsStringExt;

    argv.iter()
        .map(|argument| match argument {
            CommandArg::Unix(bytes) => Ok(OsString::from_vec(bytes.clone())),
            CommandArg::Windows(_) => Err("Windows command argument used on Unix".into()),
        })
        .collect()
}

#[cfg(windows)]
fn native_argv(argv: &[CommandArg]) -> Result<Vec<OsString>, String> {
    use std::os::windows::ffi::OsStringExt;

    argv.iter()
        .map(|argument| match argument {
            CommandArg::Windows(wide) => Ok(OsString::from_wide(wide)),
            CommandArg::Unix(_) => Err("Unix command argument used on Windows".into()),
        })
        .collect()
}

#[cfg(not(any(unix, windows)))]
fn native_argv(_argv: &[CommandArg]) -> Result<Vec<OsString>, String> {
    Err("unsupported process platform".into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::ffi::OsStr;
    use std::future::{pending, ready};
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    use hoimin_core::{
        CommandArg, EffectFailed, EffectFailure, EffectId, MutationStatus, OutputSpoolRef,
        ProcessLimits, ProcessOutputState, ProcessTermination, RunProcess, classify_mutant_result,
    };
    use serde::Deserialize;
    use tokio::process::Command;

    use super::{
        CompletionGuard, POST_TERMINATION_GRACE, ProcessCancellation, ProcessHandler,
        ProcessRequest, ProcessSelection, ProcessStartGate, abort_and_join, append_cleanup_failure,
        attach_failure, await_output, combine_process_and_output, select_process_result,
        wait_after_termination, wait_failure_after_cleanup,
    };
    use crate::resource::{PortableBackend, ResourceBackend, ResourceError};

    #[tokio::test]
    async fn shutdown_drain_reports_reap_output_and_close_status_separately() {
        let output = tempfile::tempdir().unwrap();
        let output = camino::Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap();
        let handler = ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output,
        );

        let report = handler.drain_for_shutdown(Duration::from_secs(1)).await;

        assert!(report.all_reaped);
        assert!(report.output_drains_joined);
        assert_eq!(report.secondary_errors, Vec::<String>::new());
    }

    #[test]
    fn shutdown_close_timeout_does_not_erase_completed_reap_proof() {
        let output = tempfile::tempdir().unwrap();
        let output = camino::Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap();
        let handler = ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output,
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async move {
            let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
            let blocker = tokio::task::spawn_blocking(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            });
            entered_rx.await.unwrap();

            let report = handler.drain_for_shutdown(Duration::from_millis(20)).await;
            release_tx.send(()).unwrap();
            blocker.await.unwrap();

            assert!(report.all_reaped);
            assert!(report.output_drains_joined);
            assert_eq!(
                report.secondary_errors,
                ["process.resource.close: process resource close exceeded shutdown budget"]
            );
        });
    }

    #[tokio::test]
    async fn shutdown_drain_never_turns_failed_completion_proofs_into_quiescence() {
        let output = tempfile::tempdir().unwrap();
        let output = camino::Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap();
        let handler = ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output,
        );
        handler.process_reap_failed.store(true, Ordering::Release);
        handler.output_drain_failed.store(true, Ordering::Release);

        let report = handler.drain_for_shutdown(Duration::from_secs(1)).await;

        assert!(!report.all_reaped);
        assert!(!report.output_drains_joined);
    }

    #[test]
    fn completion_guard_records_failure_unless_completion_is_explicitly_proven() {
        let active = AtomicUsize::new(0);
        let failed = AtomicBool::new(false);
        drop(CompletionGuard::enter(&active, &failed));
        assert_eq!(active.load(Ordering::Acquire), 0);
        assert!(failed.load(Ordering::Acquire));

        let active = AtomicUsize::new(0);
        let failed = AtomicBool::new(false);
        let mut completed = CompletionGuard::enter(&active, &failed);
        completed.complete();
        drop(completed);
        assert_eq!(active.load(Ordering::Acquire), 0);
        assert!(!failed.load(Ordering::Acquire));
    }

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[tokio::test]
    async fn aborting_an_output_task_waits_until_its_future_is_dropped() {
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task_dropped = Arc::clone(&dropped);
        let mut task = tokio::spawn(async move {
            let _drop = DropFlag(task_dropped);
            started_tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started_rx.await.unwrap();

        abort_and_join(&mut task).await;

        assert!(dropped.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn aborting_a_timed_out_collector_does_not_prove_output_quiescence() {
        let stdout = tokio::spawn(async { Ok(()) });
        let stderr = tokio::spawn(async { Ok(()) });
        let collector = tokio::spawn(async { pending::<std::io::Result<OutputSpoolRef>>().await });
        let active = AtomicUsize::new(0);
        let failed = AtomicBool::new(false);
        let mut completion = CompletionGuard::enter(&active, &failed);

        let outcome = await_output(
            EffectId(48),
            stdout,
            stderr,
            collector,
            tokio::time::Instant::now() + Duration::from_millis(20),
        )
        .await;
        if outcome.quiescent {
            completion.complete();
        }
        drop(completion);

        assert!(!outcome.quiescent);
        assert_eq!(
            outcome.result.unwrap_err().failure.code(),
            "process.output.close.timeout"
        );
        assert!(failed.load(Ordering::Acquire));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn settled_output_tasks_remain_joined_when_the_spool_write_fails() {
        let temporary = tempfile::tempdir().unwrap();
        let output = camino::Utf8PathBuf::from_path_buf(temporary.path().join("spool")).unwrap();
        let handler = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output.clone(),
        ));
        let start_gate = ProcessStartGate::new();
        let spawn_guard = start_gate.begin_spawn();
        let executable = std::env::current_exe().unwrap();
        let request = ProcessRequest::from(RunProcess {
            id: EffectId(49),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                native_test_arg(executable.as_os_str()),
                native_test_arg(OsStr::new("--list")),
            ],
            cwd: camino::Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: ProcessLimits {
                timeout: Duration::from_secs(5),
                max_output_bytes: 1024,
                max_memory_bytes: 256 * 1024 * 1024,
                max_processes: 8,
            },
        })
        .with_start_gate(start_gate.clone());
        let task = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move { handler.run(request).await }
        });
        // Keep the synchronous spawn gate locked while allowing the runtime to make progress.
        tokio::task::block_in_place(|| {
            let setup_deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !output.is_dir() {
                assert!(
                    std::time::Instant::now() < setup_deadline,
                    "process output directory was not prepared before spawn"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            std::fs::remove_dir(&output).unwrap();
            std::fs::write(&output, b"blocks spool creation").unwrap();
            drop(spawn_guard);
        });

        let error = task.await.unwrap().unwrap_err();
        assert_eq!(error.failure.code(), "process.output.write");
        let report = handler.drain_for_shutdown(Duration::from_secs(1)).await;
        assert!(report.all_reaped);
        assert!(
            report.output_drains_joined,
            "a terminal spool error must not erase the joined-task proof"
        );
    }

    fn output_ref() -> OutputSpoolRef {
        OutputSpoolRef {
            token: "output".to_owned(),
            retained: 0,
            observed: 0,
        }
    }

    #[cfg(unix)]
    fn native_test_arg(value: &OsStr) -> CommandArg {
        use std::os::unix::ffi::OsStrExt;

        CommandArg::Unix(value.as_bytes().to_vec())
    }

    #[cfg(windows)]
    fn native_test_arg(value: &OsStr) -> CommandArg {
        use std::os::windows::ffi::OsStrExt;

        CommandArg::Windows(value.encode_wide().collect())
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(clippy::struct_excessive_bools)]
    struct ProcessOutputCase {
        schema: u64,
        id: String,
        mode: String,
        execution: String,
        process: String,
        output: String,
        expected_fatal: bool,
        expected_status: Option<String>,
        expected_termination: Option<String>,
        expected_output_incomplete: bool,
        expected_diagnostic: bool,
        expected_continue: bool,
        input_termination: Option<String>,
    }

    fn named_termination(name: &str) -> ProcessTermination {
        match name {
            "exit_success" => ProcessTermination::Exit(0),
            "exit_failure" => ProcessTermination::Exit(7),
            "timeout" => ProcessTermination::Timeout,
            "out_of_memory" => ProcessTermination::OutOfMemory,
            "process_limit" => ProcessTermination::ProcessLimit,
            "cancelled" => ProcessTermination::Cancelled,
            other => panic!("unknown termination {other}"),
        }
    }

    fn named_status(name: &str) -> MutationStatus {
        match name {
            "killed" => MutationStatus::Killed,
            "survived" => MutationStatus::Survived,
            "timeout" => MutationStatus::Timeout,
            "out_of_memory" => MutationStatus::OutOfMemory,
            "process_limit" => MutationStatus::ProcessLimit,
            "not_run" => MutationStatus::NotRun,
            "error" => MutationStatus::Error,
            other => panic!("unknown status {other}"),
        }
    }

    fn process_output_cases() -> Vec<ProcessOutputCase> {
        const CORPUS: &str =
            include_str!("../../../../formal/HoiminOracle/corpus/process-output-outcome.jsonl");
        let cases = CORPUS
            .lines()
            .map(|line| serde_json::from_str::<ProcessOutputCase>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 42);
        assert_eq!(
            cases
                .iter()
                .map(|case| case.id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            42
        );
        cases
    }

    #[test]
    fn process_output_combiner_matches_every_lean_decision_row() {
        for case in process_output_cases() {
            assert_eq!(case.schema, 1, "{}", case.id);
            assert_eq!(case.mode, "strict", "{}", case.id);
            let process_failure = EffectFailed::other(
                EffectId(7),
                "process.wait.failed",
                "fixture process failure",
            );
            let output_failure = match case.output.as_str() {
                "close_timed_out" => EffectFailed::other(
                    EffectId(7),
                    "process.output.close.timeout",
                    "fixture output timeout",
                ),
                "failed" => EffectFailed::other(
                    EffectId(7),
                    "process.stdout.read",
                    "fixture output failure",
                ),
                "complete" => {
                    EffectFailed::other(EffectId(7), "unreachable.complete", "unreachable")
                }
                other => panic!("unknown output {other}"),
            };
            let process = match case.input_termination.as_deref() {
                Some(termination) => Ok(named_termination(termination)),
                None => Err(process_failure.clone()),
            };
            let collected = OutputSpoolRef {
                token: "collected".to_owned(),
                retained: 3,
                observed: 5,
            };
            let fallback = output_ref();
            let output = if case.output == "complete" {
                Ok(collected.clone())
            } else {
                Err(output_failure)
            };
            let mutant_id = (case.execution == "mutant").then_some("mutant");
            let actual = combine_process_and_output(process, output, mutant_id, fallback.clone());

            assert_eq!(actual.is_err(), case.expected_fatal, "{}", case.id);
            assert_eq!(case.expected_continue, !case.expected_fatal, "{}", case.id);
            let Ok((termination, output, output_state)) = actual else {
                continue;
            };
            assert_eq!(
                Some(termination),
                case.expected_termination.as_deref().map(named_termination),
                "{}",
                case.id
            );
            assert_eq!(
                output_state == ProcessOutputState::CloseTimedOut,
                case.expected_output_incomplete,
                "{}",
                case.id
            );
            assert_eq!(
                output_state == ProcessOutputState::CloseTimedOut,
                case.expected_diagnostic,
                "{}",
                case.id
            );
            assert_eq!(
                output,
                if case.expected_output_incomplete {
                    fallback
                } else {
                    collected
                },
                "{}",
                case.id
            );
            if case.execution == "mutant" {
                assert_eq!(
                    classify_mutant_result(termination, output_state),
                    named_status(case.expected_status.as_deref().unwrap()),
                    "{}",
                    case.id
                );
            } else {
                assert!(case.expected_status.is_none(), "{}", case.id);
            }
            assert_eq!(case.process == "failed", case.input_termination.is_none());
        }
    }

    #[test]
    #[ignore = "subprocess fixture for bounded reap tests"]
    fn root_child_fixture_waits() {
        std::thread::sleep(Duration::from_secs(10));
    }

    #[tokio::test]
    async fn aborting_the_caller_does_not_report_quiescence_before_reap_and_output_drain() {
        let output = tempfile::tempdir().unwrap();
        let output = camino::Utf8PathBuf::from_path_buf(output.path().to_owned()).unwrap();
        let handler = Arc::new(ProcessHandler::new(
            ResourceBackend::Portable(PortableBackend::for_tests()),
            output,
        ));
        let cancellation = ProcessCancellation::new();
        let executable = std::env::current_exe().unwrap();
        let request = ProcessRequest::from(RunProcess {
            id: EffectId(48),
            worker: None,
            run_id: None,
            mutant_id: None,
            argv: vec![
                native_test_arg(executable.as_os_str()),
                native_test_arg(OsStr::new("--ignored")),
                native_test_arg(OsStr::new("--exact")),
                native_test_arg(OsStr::new("process::tests::root_child_fixture_waits")),
            ],
            cwd: camino::Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
            limits: ProcessLimits {
                timeout: Duration::from_secs(10),
                max_output_bytes: 1024,
                max_memory_bytes: 256 * 1024 * 1024,
                max_processes: 8,
            },
        })
        .with_cancellation(cancellation.clone());
        let task = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move { handler.run(request).await }
        });

        let started_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while handler.active_processes.load(Ordering::Acquire) == 0 {
            assert!(
                tokio::time::Instant::now() < started_deadline,
                "fixture process did not enter the active-process registry"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let premature = handler.drain_for_shutdown(Duration::from_millis(20)).await;
        assert!(
            !premature.all_reaped || !premature.output_drains_joined,
            "aborting the caller falsely reported child and output quiescence"
        );

        cancellation.cancel();
        let drained = handler.drain_for_shutdown(Duration::from_secs(3)).await;
        assert!(
            drained.all_reaped,
            "child was not reaped after cancellation"
        );
        assert!(
            drained.output_drains_joined,
            "output drains did not join after cancellation"
        );
    }

    #[tokio::test]
    async fn wait_after_termination_reaps_the_root_child() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        wait_after_termination(EffectId(43), &mut child)
            .await
            .unwrap();

        assert_eq!(child.id(), None);
    }

    #[tokio::test]
    async fn wait_after_termination_kills_and_reaps_after_grace_expires() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "process::tests::root_child_fixture_waits",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = tokio::time::Instant::now();

        wait_after_termination(EffectId(45), &mut child)
            .await
            .unwrap();

        let elapsed = started.elapsed();
        assert!(elapsed >= POST_TERMINATION_GRACE);
        assert!(
            elapsed < Duration::from_secs(5),
            "root kill and bounded reap took {elapsed:?}"
        );
        assert_eq!(child.id(), None);
    }

    #[test]
    fn cancellation_and_spawn_are_linearized_by_the_spawn_gate() {
        let cancellation = ProcessCancellation::new();
        let spawn = cancellation.begin_spawn();
        let worker_cancellation = cancellation.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            worker_cancellation.cancel();
        });
        started_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert!(!cancellation.is_cancelled());

        drop(spawn);
        worker.join().unwrap();

        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn external_request_and_spawn_are_linearized_by_the_same_gate() {
        let request = ProcessStartGate::new();
        let spawn = request.begin_spawn();
        let worker_request = request.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            worker_request.cancel();
        });
        started_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert!(!request.is_cancelled());

        drop(spawn);
        worker.join().unwrap();

        assert!(request.is_cancelled());
    }

    #[test]
    fn process_error_precedes_output_cleanup_error() {
        let primary = EffectFailed::other(EffectId(41), "process.resource.terminate", "primary");
        let cleanup = EffectFailed::other(EffectId(41), "process.output.close.timeout", "cleanup");

        let error = combine_process_and_output(
            Err(primary.clone()),
            Err(cleanup),
            Some("mutant"),
            output_ref(),
        )
        .expect_err("primary process failure wins");

        assert_eq!(error, primary);
    }

    #[test]
    fn cleanup_failures_preserve_primary_error_and_append_details_in_order() {
        let mut primary = EffectFailed {
            id: EffectId(44),
            failure: EffectFailure::Io {
                code: "process.resource.terminate".into(),
                operation: "terminate supervised process tree".into(),
                path: None,
                message: "primary termination failure".into(),
            },
        };
        let root_kill = EffectFailed::other(EffectId(44), "process.kill", "root kill detail");
        let tree_retry = EffectFailed::other(
            EffectId(44),
            "process.resource.terminate",
            "tree retry detail",
        );

        append_cleanup_failure(&mut primary, "direct root kill failed", &root_kill);
        append_cleanup_failure(
            &mut primary,
            "supervisor termination retry failed",
            &tree_retry,
        );

        assert_eq!(primary.id, EffectId(44));
        assert!(matches!(
            primary.failure,
            EffectFailure::Io {
                ref code,
                ref operation,
                ref message,
                ..
            } if code == "process.resource.terminate"
                && operation == "terminate supervised process tree"
                && message.starts_with("primary termination failure")
                && message.ends_with(
                    "direct root kill failed: root kill detail; \
                     supervisor termination retry failed: tree retry detail"
                )
        ));
    }

    #[tokio::test]
    async fn wait_error_cleanup_runs_and_preserves_the_primary_failure() {
        let cleanup_ran = AtomicBool::new(false);
        let wait_error = std::io::Error::other("wait failed");

        let error = wait_failure_after_cleanup(EffectId(46), &wait_error, async {
            cleanup_ran.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        assert!(cleanup_ran.load(Ordering::SeqCst));
        assert!(matches!(
            error.failure,
            EffectFailure::Io {
                ref code,
                ref operation,
                ref message,
                ..
            } if code == "process.wait"
                && operation == "wait for process"
                && message == "wait failed"
        ));
    }

    #[tokio::test]
    async fn wait_error_cleanup_appends_cleanup_failure_to_the_primary_error() {
        let cleanup_ran = AtomicBool::new(false);
        let wait_error = std::io::Error::other("wait failed");
        let cleanup = EffectFailed::other(
            EffectId(47),
            "process.resource.terminate",
            "tree cleanup failed",
        );

        let error = wait_failure_after_cleanup(EffectId(47), &wait_error, async {
            cleanup_ran.store(true, Ordering::SeqCst);
            Err(cleanup)
        })
        .await;

        assert!(cleanup_ran.load(Ordering::SeqCst));
        assert!(matches!(
            error.failure,
            EffectFailure::Io {
                ref code,
                ref operation,
                ref message,
                ..
            } if code == "process.wait"
                && operation == "wait for process"
                && message == "wait failed; supervised cleanup after wait error failed: \
                    tree cleanup failed"
        ));
    }

    #[test]
    fn mutant_output_close_timeout_preserves_the_known_termination() {
        let cleanup = EffectFailed::other(EffectId(42), "process.output.close.timeout", "cleanup");
        let fallback = output_ref();

        let result = combine_process_and_output(
            Ok(ProcessTermination::Timeout),
            Err(cleanup),
            Some("mutant"),
            fallback.clone(),
        )
        .expect("mutant close timeout is a classified result");

        assert_eq!(
            result,
            (
                ProcessTermination::Timeout,
                fallback,
                ProcessOutputState::CloseTimedOut
            )
        );
    }

    #[test]
    fn complete_output_preserves_normal_classification_inputs() {
        let output = output_ref();

        let result = combine_process_and_output(
            Ok(ProcessTermination::Exit(0)),
            Ok(output.clone()),
            Some("mutant"),
            output_ref(),
        )
        .expect("complete output is returned");

        assert_eq!(
            result,
            (
                ProcessTermination::Exit(0),
                output,
                ProcessOutputState::Complete
            )
        );
    }

    #[test]
    fn baseline_output_close_timeout_remains_fatal() {
        let cleanup = EffectFailed::other(EffectId(42), "process.output.close.timeout", "cleanup");

        let error = combine_process_and_output(
            Ok(ProcessTermination::Exit(0)),
            Err(cleanup.clone()),
            None,
            output_ref(),
        )
        .expect_err("a real baseline worker keeps output cleanup failure fatal");

        assert_eq!(error, cleanup);
    }

    #[test]
    fn mutant_output_io_failure_remains_fatal() {
        let failure = EffectFailed::other(EffectId(42), "process.stdout.read", "read failed");

        let error = combine_process_and_output(
            Ok(ProcessTermination::Exit(0)),
            Err(failure.clone()),
            Some("mutant"),
            output_ref(),
        )
        .expect_err("ordinary output failures remain fatal");

        assert_eq!(error, failure);
    }

    #[test]
    fn attach_failure_preserves_cleanup_failure_detail() {
        let cleanup = std::io::Error::new(std::io::ErrorKind::TimedOut, "cleanup timed out");

        let attach = ResourceError::MissingProcessId;
        let error = attach_failure(EffectId(43), &attach, Some(&cleanup));

        assert_eq!(error.id, EffectId(43));
        assert!(matches!(
            error.failure,
            EffectFailure::Io { ref code, ref message, .. }
                if code == "process.resource.attach"
                    && message.contains("spawned process did not expose a process id")
                    && message.contains("cleanup timed out")
        ));
    }

    #[tokio::test]
    async fn cancellation_and_timeout_precede_a_simultaneous_exit() {
        let cancelled = select_process_result(ready(7_u8), ready(()), ready(())).await;
        assert_eq!(cancelled, ProcessSelection::Cancelled);

        let timed_out = select_process_result(ready(7_u8), pending::<()>(), ready(())).await;
        assert_eq!(timed_out, ProcessSelection::Timeout);
    }
}
