mod output;

use std::ffi::OsString;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

const POST_TERMINATION_GRACE: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct ProcessCancellation {
    state: Arc<CancellationState>,
}

#[derive(Debug)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

impl ProcessCancellation {
    pub fn new() -> Self {
        Self {
            state: Arc::new(CancellationState {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    pub fn cancel(&self) {
        if !self.state.cancelled.swap(true, Ordering::AcqRel) {
            self.state.notify.notify_waiters();
        }
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.state.notify.notified();
            if self.state.cancelled.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
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
}

impl ProcessRequest {
    pub fn with_cancellation(mut self, cancellation: ProcessCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }
}

impl From<RunProcess> for ProcessRequest {
    fn from(process: RunProcess) -> Self {
        Self {
            process,
            cancellation: ProcessCancellation::new(),
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
    pub fn new(backend: ResourceBackend, output_dir: Utf8PathBuf) -> Self {
        Self {
            backend,
            output_dir,
        }
    }

    pub fn spool_path(&self, output: &OutputSpoolRef) -> Result<Utf8PathBuf, ProcessError> {
        Uuid::parse_str(&output.token)
            .map_err(|_| ProcessError::InvalidSpoolToken(output.token.clone()))?;
        Ok(self.output_dir.join(format!("{}.bin", output.token)))
    }

    pub async fn handle(&self, request: RunProcess) -> Result<ProcessFinished, EffectFailed> {
        self.run(request.into()).await
    }

    pub async fn run(&self, request: ProcessRequest) -> Result<ProcessFinished, EffectFailed> {
        let ProcessRequest {
            process,
            cancellation,
        } = request;
        let id = process.id;
        let argv = native_argv(&process.argv)
            .map_err(|message| EffectFailed::other(id, "process.argv.invalid", message))?;
        let (program, arguments) = argv.split_first().ok_or_else(|| {
            EffectFailed::other(id, "process.argv.empty", "process argv must not be empty")
        })?;
        tokio::fs::create_dir_all(&self.output_dir)
            .await
            .map_err(|error| {
                io_failure(
                    id,
                    "process.output.create",
                    "create output directory",
                    Some(self.output_dir.clone()),
                    error,
                )
            })?;

        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(&process.cwd)
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
                        error,
                    )
                })?;
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + process.limits.timeout;
        let mut child = command.spawn().map_err(|error| {
            io_failure(
                id,
                "process.spawn",
                "spawn process",
                Some(process.cwd.clone()),
                error,
            )
        })?;
        if let Err(error) = supervisor.attach(&child) {
            terminate_unattached_child(&mut child).await;
            return Err(resource_failure(
                id,
                "process.resource.attach",
                "attach resource supervisor",
                error,
            ));
        }

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

        let termination = tokio::select! {
            status = child.wait() => {
                let status = status.map_err(|error| io_failure(id, "process.wait", "wait for process", None, error))?;
                terminate_supervised(id, &mut supervisor)?;
                exit_termination(status)
            }
            () = cancellation.cancelled() => {
                terminate_supervised(id, &mut supervisor)?;
                wait_after_termination(id, &mut child).await?;
                ProcessTermination::Cancelled
            }
            () = tokio::time::sleep_until(deadline) => {
                terminate_supervised(id, &mut supervisor)?;
                wait_after_termination(id, &mut child).await?;
                ProcessTermination::Timeout
            }
        };

        let output = await_output(
            id,
            stdout_task,
            stderr_task,
            collector_task,
            tokio::time::Instant::now() + POST_TERMINATION_GRACE,
        )
        .await?;
        contract_ensure!(
            "process.output.post",
            output.retained <= process.limits.max_output_bytes
                && output.retained <= output.observed,
            &output
        );
        Ok(ProcessFinished {
            id,
            termination,
            output,
            elapsed: started.elapsed(),
            resource_mode: self.backend.mode(),
        })
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
    match tokio::time::timeout_at(deadline, &mut collector_task).await {
        Ok(result) => result
            .map_err(|error| EffectFailed::other(id, "process.output.join", error.to_string()))?
            .map_err(|error| {
                io_failure(
                    id,
                    "process.output.write",
                    "write output spool",
                    None,
                    error,
                )
            }),
        Err(_) => {
            collector_task.abort();
            Err(EffectFailed::other(
                id,
                "process.output.close.timeout",
                "timed out draining process output after termination",
            ))
        }
    }
}

async fn await_pipe_until(
    id: EffectId,
    name: &'static str,
    task: &mut tokio::task::JoinHandle<std::io::Result<()>>,
    deadline: tokio::time::Instant,
) -> Result<(), EffectFailed> {
    match tokio::time::timeout_at(deadline, &mut *task).await {
        Ok(result) => result
            .map_err(|error| {
                EffectFailed::other(id, format!("process.{name}.join"), error.to_string())
            })?
            .map_err(|error| {
                io_failure(
                    id,
                    format!("process.{name}.read"),
                    "drain process pipe",
                    None,
                    error,
                )
            }),
        Err(_) => {
            task.abort();
            Err(EffectFailed::other(
                id,
                "process.output.close.timeout",
                "timed out draining process output after termination",
            ))
        }
    }
}

async fn wait_after_termination(id: EffectId, child: &mut Child) -> Result<(), EffectFailed> {
    match tokio::time::timeout(POST_TERMINATION_GRACE, child.wait()).await {
        Ok(result) => result.map(|_| ()).map_err(|error| {
            io_failure(
                id,
                "process.wait",
                "wait after process termination",
                None,
                error,
            )
        }),
        Err(_) => {
            let _ = child.start_kill();
            Err(EffectFailed::other(
                id,
                "process.wait.timeout",
                "timed out waiting for process termination",
            ))
        }
    }
}

async fn terminate_unattached_child(child: &mut Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

fn terminate_supervised(
    id: EffectId,
    supervisor: &mut ProcessSupervisor,
) -> Result<(), EffectFailed> {
    supervisor.terminate().map_err(|error| {
        resource_failure(
            id,
            "process.resource.terminate",
            "terminate supervised process tree",
            error,
        )
    })
}

fn io_failure(
    id: EffectId,
    code: impl Into<String>,
    operation: impl Into<String>,
    path: Option<Utf8PathBuf>,
    error: std::io::Error,
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
    error: ResourceError,
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

fn exit_termination(status: std::process::ExitStatus) -> ProcessTermination {
    if let Some(code) = status.code() {
        return ProcessTermination::Exit(code);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        return ProcessTermination::Exit(status.signal().map_or(-1, |signal| 128 + signal));
    }
    #[cfg(not(unix))]
    ProcessTermination::Exit(-1)
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
