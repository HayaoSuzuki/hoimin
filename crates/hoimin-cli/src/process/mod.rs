mod output;

use std::ffi::OsString;
use std::future::Future;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use hoimin_core::{
    CommandArg, EffectFailed, EffectFailure, EffectId, OutputSpoolRef, ProcessFinished,
    ProcessTermination, RunProcess, contract_ensure,
};
use tokio::process::{Child, Command};
use tokio::sync::Notify;
use uuid::Uuid;

use crate::resource::{ProcessSupervisor, ResourceBackend, ResourceError};
use crate::workspace::CommandEnvironment;

const POST_TERMINATION_GRACE: Duration = Duration::from_secs(1);

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

#[derive(Debug)]
pub struct ProcessHandler {
    backend: ResourceBackend,
    output_dir: Utf8PathBuf,
}

impl ProcessHandler {
    #[must_use]
    pub fn new(backend: ResourceBackend, output_dir: Utf8PathBuf) -> Self {
        Self {
            backend,
            output_dir,
        }
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
    // This coordinates the complete child lifecycle; extracting stages would obscure cleanup ordering.
    #[allow(clippy::too_many_lines)]
    pub async fn run(&self, request: ProcessRequest) -> Result<ProcessFinished, EffectFailed> {
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
        let mut supervisor =
            self.backend
                .prepare(&mut command, process.limits)
                .map_err(|error| {
                    resource_failure(
                        id,
                        "process.resource.setup",
                        "prepare resource supervisor",
                        &error,
                    )
                })?;
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
        let mut child = command.spawn().map_err(|error| {
            io_failure(
                id,
                "process.spawn",
                "spawn process",
                Some(process.cwd.clone()),
                &error,
            )
        })?;
        if let Err(error) = supervisor.attach(&child) {
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
            return Err(attach_failure(id, &error, cleanup_error.as_ref()));
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
        let stdout_task = tokio::spawn(output::drain_pipe(stdout, sender.clone()));
        let stderr_task = tokio::spawn(output::drain_pipe(stderr, sender));
        let collector_task = tokio::spawn(output::collect_output(
            spool_path,
            token,
            process.limits.max_output_bytes,
            receiver,
        ));

        let process_result = match select_process_result(
            child.wait(),
            cancellation.cancelled(),
            tokio::time::sleep_until(deadline),
        )
        .await
        {
            ProcessSelection::Exited(status) => match status {
                Ok(status) => supervisor
                    .classify(exit_termination(status))
                    .map_err(|error| {
                        resource_failure(
                            id,
                            "process.resource.classify",
                            "classify process termination",
                            &error,
                        )
                    })
                    .and_then(|termination| {
                        terminate_supervised(id, &mut supervisor, false).map(|()| termination)
                    }),
                Err(error) => Err(io_failure(
                    id,
                    "process.wait",
                    "wait for process",
                    None,
                    &error,
                )),
            },
            ProcessSelection::Cancelled => terminate_and_reap(id, &mut supervisor, &mut child)
                .await
                .map(|()| ProcessTermination::Cancelled),
            ProcessSelection::Timeout => terminate_and_reap(id, &mut supervisor, &mut child)
                .await
                .map(|()| ProcessTermination::Timeout),
        };

        let output_result = await_output(
            id,
            stdout_task,
            stderr_task,
            collector_task,
            tokio::time::Instant::now() + POST_TERMINATION_GRACE,
        )
        .await;
        let (termination, output) = combine_process_and_output(process_result, output_result)?;
        contract_ensure!(
            "process.output.post",
            output.retained <= process.limits.max_output_bytes
                && output.retained <= output.observed,
            &output
        );
        Ok(ProcessFinished {
            id,
            worker: process.worker,
            termination,
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
) -> Result<OutputSpoolRef, EffectFailed> {
    if let Err(error) = await_pipe_until(id, "stdout", &mut stdout_task, deadline).await {
        stderr_task.abort();
        collector_task.abort();
        return Err(error);
    }
    if let Err(error) = await_pipe_until(id, "stderr", &mut stderr_task, deadline).await {
        collector_task.abort();
        return Err(error);
    }
    if let Ok(result) = tokio::time::timeout_at(deadline, &mut collector_task).await {
        result
            .map_err(|error| EffectFailed::other(id, "process.output.join", error.to_string()))?
            .map_err(|error| {
                io_failure(
                    id,
                    "process.output.write",
                    "write output spool",
                    None,
                    &error,
                )
            })
    } else {
        collector_task.abort();
        Err(EffectFailed::other(
            id,
            "process.output.close.timeout",
            "timed out draining process output after termination",
        ))
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
        task.abort();
        Err(EffectFailed::other(
            id,
            "process.output.close.timeout",
            "timed out draining process output after termination",
        ))
    }
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

async fn terminate_and_reap(
    id: EffectId,
    supervisor: &mut ProcessSupervisor,
    child: &mut Child,
) -> Result<(), EffectFailed> {
    match terminate_supervised(id, supervisor, true) {
        Ok(()) => wait_after_termination(id, child).await,
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
            let tree_retry = terminate_supervised(id, supervisor, true).err();
            let root_wait = wait_after_termination(id, child).await.err();

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

fn combine_process_and_output(
    process: Result<ProcessTermination, EffectFailed>,
    output: Result<OutputSpoolRef, EffectFailed>,
) -> Result<(ProcessTermination, OutputSpoolRef), EffectFailed> {
    match process {
        Ok(termination) => output.map(|output| (termination, output)),
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

pub(crate) fn terminate_supervised(
    id: EffectId,
    supervisor: &mut ProcessSupervisor,
    live_root_owned: bool,
) -> Result<(), EffectFailed> {
    supervisor.terminate(live_root_owned).map_err(|error| {
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
    use std::future::{pending, ready};
    use std::process::Stdio;
    use std::time::Duration;

    use hoimin_core::{EffectFailure, EffectId, ProcessTermination};
    use tokio::process::Command;

    use super::{
        POST_TERMINATION_GRACE, ProcessCancellation, ProcessSelection, ProcessStartGate,
        append_cleanup_failure, attach_failure, combine_process_and_output, select_process_result,
        wait_after_termination,
    };
    use crate::resource::ResourceError;

    #[test]
    #[ignore = "subprocess fixture for bounded reap tests"]
    fn root_child_fixture_waits() {
        std::thread::sleep(Duration::from_secs(10));
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

        assert!(started.elapsed() >= POST_TERMINATION_GRACE);
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
        let primary =
            hoimin_core::EffectFailed::other(EffectId(41), "process.resource.terminate", "primary");
        let cleanup = hoimin_core::EffectFailed::other(
            EffectId(41),
            "process.output.close.timeout",
            "cleanup",
        );

        let error = combine_process_and_output(Err(primary.clone()), Err(cleanup))
            .expect_err("primary process failure wins");

        assert_eq!(error, primary);
    }

    #[test]
    fn cleanup_failures_preserve_primary_error_and_append_details_in_order() {
        let mut primary = hoimin_core::EffectFailed {
            id: EffectId(44),
            failure: EffectFailure::Io {
                code: "process.resource.terminate".into(),
                operation: "terminate supervised process tree".into(),
                path: None,
                message: "primary termination failure".into(),
            },
        };
        let root_kill =
            hoimin_core::EffectFailed::other(EffectId(44), "process.kill", "root kill detail");
        let tree_retry = hoimin_core::EffectFailed::other(
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

    #[test]
    fn successful_process_requires_successful_output_cleanup() {
        let cleanup = hoimin_core::EffectFailed::other(
            EffectId(42),
            "process.output.close.timeout",
            "cleanup",
        );

        let error =
            combine_process_and_output(Ok(ProcessTermination::Exit(0)), Err(cleanup.clone()))
                .expect_err("output cleanup failure is returned");

        assert_eq!(error, cleanup);
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
