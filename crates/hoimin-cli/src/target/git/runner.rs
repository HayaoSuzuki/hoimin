//! Owned lifecycles for target-inspection Git commands.
//!
//! Dropping a waiter requests cancellation but leaves its task owning the child.
//! Run shutdown drains that task under the shell's existing absolute deadline.
//! Standalone plan/verify callers await the same reap/pipe contract on completion;
//! when they drop a waiter, their runtime must stay alive to complete its cleanup.

use std::process::{Output, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use camino::Utf8Path;
use hoimin_core::TargetError;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::Notify;

use crate::process::{ProcessCancellation, ProcessStartGate};

// Per-command resource policy, including whole-repository rename/unmerged diffs.
// 64 MiB permits large patches and inventories without allowing their capture to
// grow with repository size. Larger valid results fail explicitly, never return
// partial changed lines, and need no unbounded temporary disk storage.
const STDOUT_LIMIT: usize = 64 * 1024 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;
const STDERR_TRUNCATED: &[u8] = b"\n[Git stderr truncated after 65536 bytes]";

#[derive(Debug)]
enum CaptureError {
    Io(std::io::Error),
    StdoutLimit(usize),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => std::fmt::Display::fmt(error, formatter),
            Self::StdoutLimit(limit) => {
                write!(formatter, "Git stdout exceeded {limit}-byte capture limit")
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum GitError {
    Cancelled,
    Failed(TargetError),
}

impl GitError {
    pub(crate) fn into_target(self) -> TargetError {
        match self {
            Self::Cancelled => TargetError::GitFailed("target Git command cancelled".into()),
            Self::Failed(error) => error,
        }
    }
}

impl From<TargetError> for GitError {
    fn from(error: TargetError) -> Self {
        Self::Failed(error)
    }
}

impl From<std::io::Error> for GitError {
    fn from(error: std::io::Error) -> Self {
        TargetError::GitFailed(error.to_string()).into()
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitRunner {
    state: Arc<RunnerState>,
    admission: Option<(ProcessStartGate, tokio::time::Instant)>,
}

#[derive(Debug, Default)]
struct RunnerState {
    active: AtomicUsize,
    changed: Notify,
    failures: Mutex<Vec<String>>,
}

struct ActiveLifecycle {
    state: Arc<RunnerState>,
    settled: bool,
}

impl ActiveLifecycle {
    fn failure(&self, message: String) {
        self.state
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(message);
    }
}

impl Drop for ActiveLifecycle {
    fn drop(&mut self) {
        if !self.settled {
            self.failure(
                "target.git.lifecycle: owned Git lifecycle ended without reap/pipe proof".into(),
            );
        }
        self.state.active.fetch_sub(1, Ordering::AcqRel);
        self.state.changed.notify_waiters();
    }
}

struct CancelOnDrop(Option<ProcessCancellation>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancellation) = &self.0 {
            cancellation.cancel();
        }
    }
}

impl GitRunner {
    pub(crate) fn for_run(start_gate: ProcessStartGate, deadline: tokio::time::Instant) -> Self {
        Self {
            state: Arc::default(),
            admission: Some((start_gate, deadline)),
        }
    }

    pub(super) async fn output(
        &self,
        root: &Utf8Path,
        args: &[&str],
        cancellation: &ProcessCancellation,
    ) -> Result<Output, GitError> {
        self.run_command(super::git_command(root, args), cancellation)
            .await
    }

    async fn run_command(
        &self,
        mut command: Command,
        cancellation: &ProcessCancellation,
    ) -> Result<Output, GitError> {
        let mut cancel_on_drop = CancelOnDrop(Some(cancellation.clone()));
        let cancellation = cancellation.clone();
        let admission = self.admission.clone();
        self.state.active.fetch_add(1, Ordering::AcqRel);
        let lifecycle = ActiveLifecycle {
            state: Arc::clone(&self.state),
            settled: false,
        };
        let task = tokio::spawn(async move {
            let mut lifecycle = lifecycle;
            let mut child = {
                // Match test-process gate order: external request, then lifecycle cancellation.
                let _request = admission.as_ref().map(|(gate, _)| gate.begin_spawn());
                let _spawn = cancellation.begin_spawn();
                if cancellation.is_cancelled()
                    || admission.as_ref().is_some_and(|(gate, deadline)| {
                        gate.is_cancelled() || tokio::time::Instant::now() >= *deadline
                    })
                {
                    lifecycle.settled = true;
                    return Err(GitError::Cancelled);
                }
                match command
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(child) => child,
                    Err(error) => {
                        lifecycle.settled = true; // No child was created.
                        return Err(error.into());
                    }
                }
            };
            let stdout = child.stdout.take().expect("configured stdout pipe");
            let stderr = child.stderr.take().expect("configured stderr pipe");
            let completed = {
                // These readers are owned inline. Cancelling drops and closes both pipes before reap.
                let collect = async {
                    let (status, stdout, stderr) = tokio::try_join!(
                        async { child.wait().await.map_err(CaptureError::Io) },
                        read_pipe(stdout, STDOUT_LIMIT, false),
                        read_pipe(stderr, STDERR_LIMIT, true),
                    )?;
                    Ok::<_, CaptureError>(Output {
                        status,
                        stdout,
                        stderr,
                    })
                };
                tokio::select! {
                    biased;
                    () = cancellation.cancelled() => None,
                    result = collect => Some(result),
                }
            };
            match completed {
                Some(Ok(output)) => {
                    lifecycle.settled = true;
                    Ok(output)
                }
                stopped_or_failed => {
                    // Keep ownership even on termination failure. The shell can time out its drain,
                    // while this task still owns and eventually reaps the direct child.
                    let primary = match stopped_or_failed {
                        Some(Err(error)) => Some(error),
                        None => None,
                        Some(Ok(_)) => unreachable!(),
                    };
                    // A resource-policy rejection is the command's reported failure,
                    // not a lost lifecycle proof. Preserve actual I/O failures for drain.
                    let primary_message = primary.as_ref().map(ToString::to_string);
                    let lifecycle_failure = primary.as_ref().and_then(|error| match error {
                        CaptureError::Io(_) => primary_message.as_deref(),
                        CaptureError::StdoutLimit(_) => None,
                    });
                    let cleanup =
                        terminate_and_reap(&mut child, &mut lifecycle, lifecycle_failure).await;
                    match (primary_message, cleanup) {
                        (None, Ok(())) => Err(GitError::Cancelled),
                        (Some(primary), Ok(())) => Err(TargetError::GitFailed(primary).into()),
                        (None, Err(cleanup)) => Err(TargetError::GitFailed(cleanup).into()),
                        (Some(primary), Err(cleanup)) => {
                            Err(TargetError::GitFailed(format!("{primary}; {cleanup}")).into())
                        }
                    }
                }
            }
        });
        let result = task.await.map_err(|error| {
            GitError::from(TargetError::GitFailed(format!(
                "target.git.lifecycle: {error}"
            )))
        })?;
        cancel_on_drop.0 = None;
        result
    }

    /// Observe all admitted Git lifecycles within the caller's remaining shutdown budget.
    /// Expiry never releases a live child's owner or erases a recorded cleanup failure.
    ///
    /// # Errors
    ///
    /// Returns recorded termination/reap failures or an incomplete lifecycle at budget expiry.
    pub async fn drain_for_shutdown(&self, remaining: Duration) -> Result<(), String> {
        let wait = async {
            loop {
                let changed = self.state.changed.notified();
                if self.state.active.load(Ordering::Acquire) == 0 {
                    break;
                }
                changed.await;
            }
        };
        let expired = tokio::time::timeout(remaining, wait).await.is_err();
        let mut failures = self
            .state
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if expired {
            failures.push("target.git.cleanup: Git lifecycle exceeded shutdown budget".into());
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

// Keep termination/reap sequencing independent of process creation so every caller
// uses the same ownership rule, including when the OS reports a transient wait error.
trait GitChild {
    fn start_kill(&mut self) -> std::io::Result<()>;
    async fn reap(&mut self) -> std::io::Result<()>;
}

impl GitChild for tokio::process::Child {
    fn start_kill(&mut self) -> std::io::Result<()> {
        self.start_kill()
    }
    async fn reap(&mut self) -> std::io::Result<()> {
        self.wait().await.map(|_| ())
    }
}

async fn terminate_and_reap(
    child: &mut impl GitChild,
    lifecycle: &mut ActiveLifecycle,
    primary: Option<&str>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Some(primary) = primary {
        lifecycle.failure(format!("target.git.lifecycle: {primary}"));
    }
    if let Err(error) = child.start_kill() {
        let message = format!("target.git.cleanup: termination failed: {error}");
        lifecycle.failure(message.clone());
        failures.push(message);
    }
    loop {
        match child.reap().await {
            Ok(()) => {
                lifecycle.settled = true;
                break;
            }
            Err(error) => {
                if !failures
                    .iter()
                    .any(|failure| failure.starts_with("target.git.cleanup: reap failed:"))
                {
                    let message = format!("target.git.cleanup: reap failed: {error}");
                    lifecycle.failure(message.clone());
                    failures.push(message);
                }
                // A failed wait does not transfer ownership or prove the child gone.
                // Keep this task and child alive; the shell bounds its own wait separately.
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

async fn read_pipe(
    mut pipe: impl AsyncRead + Unpin,
    limit: usize,
    diagnostic: bool,
) -> Result<Vec<u8>, CaptureError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    // Reserve growth explicitly: Vec::extend's geometric growth must not turn
    // the configured byte budget into an allocation of up to twice that budget.
    let capacity_limit = limit
        + if diagnostic {
            STDERR_TRUNCATED.len()
        } else {
            0
        };
    loop {
        let count = pipe.read(&mut buffer).await.map_err(CaptureError::Io)?;
        if count == 0 {
            break;
        }
        let retained = count.min(limit - output.len());
        if retained != count {
            if !diagnostic {
                return Err(CaptureError::StdoutLimit(limit));
            }
            truncated = true;
        }
        append_capped(&mut output, &buffer[..retained], capacity_limit);
        // Continue draining even after stderr retention is exhausted, so a
        // diagnostic flood cannot block stdout, normal exit, or cancellation.
    }
    if truncated {
        append_capped(&mut output, STDERR_TRUNCATED, capacity_limit);
    }
    Ok(output)
}

fn append_capped(output: &mut Vec<u8>, bytes: &[u8], capacity_limit: usize) {
    let needed = output.len() + bytes.len();
    if needed > output.capacity() {
        let capacity = output.capacity().max(8192).saturating_mul(2).max(needed);
        output.reserve_exact(capacity.min(capacity_limit) - output.len());
    }
    output.extend_from_slice(bytes);
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    #[tokio::test]
    async fn stdout_limit_preserves_exact_bytes_and_accepts_boundary() {
        let bytes = b"M\0space name.py\0R100\0old.py\0new.py\0\xff\r\n";
        let captured = read_pipe(bytes.as_slice(), bytes.len(), false)
            .await
            .unwrap();
        assert_eq!(captured, bytes);
        assert!(captured.capacity() <= bytes.len());
    }

    #[tokio::test]
    async fn stdout_limit_rejects_a_huge_unterminated_line() {
        let error = read_pipe(tokio::io::repeat(b'x').take(1_000_000), 1024, false)
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Git stdout exceeded 1024-byte capture limit"
        );
    }

    #[tokio::test]
    async fn stdout_limit_rejects_many_nul_delimited_targets_without_partial_results() {
        let records = b"M\0app.py\0".repeat(300);
        assert!(read_pipe(records.as_slice(), 1024, false).await.is_err());
    }

    #[tokio::test]
    async fn stderr_limit_drains_and_marks_truncation_with_bounded_capacity() {
        let captured = read_pipe(tokio::io::repeat(0xff).take(1_000_000), 1024, true)
            .await
            .unwrap();
        assert_eq!(&captured[..1024], &[0xff; 1024]);
        assert_eq!(&captured[1024..], STDERR_TRUNCATED);
        assert!(captured.capacity() <= 1024 + STDERR_TRUNCATED.len());
        // Lossy UTF-8 diagnostics can expand each captured byte to three bytes.
        assert!(String::from_utf8_lossy(&captured).len() <= 3 * 1024 + STDERR_TRUNCATED.len());
    }

    #[tokio::test]
    async fn stderr_at_boundary_has_no_truncation_marker() {
        assert_eq!(
            read_pipe(b"diagnostic".as_slice(), 10, true).await.unwrap(),
            b"diagnostic"
        );
        assert_eq!(
            read_pipe(b"".as_slice(), 0, false).await.unwrap(),
            [] as [u8; 0]
        );
        assert!(read_pipe(b"x".as_slice(), 0, false).await.is_err());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn python(script: &str, marker: &Path) -> Command {
        let mut command =
            Command::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.venv/bin/python"));
        command.args(["-c", script]).arg(marker);
        command
    }

    async fn announced(path: &Path) -> i32 {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(value) = tokio::fs::read_to_string(path).await
                    && let Ok(pid) = value.parse()
                {
                    return pid;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("helper start handshake")
    }

    fn alive(pid: i32) -> bool {
        // SAFETY: signal zero probes the fixture PID without sending a signal.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[tokio::test]
    async fn stdout_overflow_terminates_and_reaps_with_both_pipes_busy() {
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("started");
        let runner = GitRunner::default();
        let command = python(
            "import os,pathlib,sys,threading,time\npathlib.Path(sys.argv[1]).write_text(str(os.getpid()))\ndef flood_stderr():\n while True: os.write(2, b'e' * 8192)\nthreading.Thread(target=flood_stderr, daemon=True).start()\nfor _ in range(8193): os.write(1, b'x' * 8192)\ntime.sleep(30)\n",
            &marker,
        );
        let task_runner = runner.clone();
        let task = tokio::spawn(async move {
            task_runner
                .run_command(command, &ProcessCancellation::new())
                .await
        });
        let mut pid = FixturePid::new(announced(&marker).await);

        let error = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();

        pid.disarm();
        assert!(
            error
                .into_target()
                .to_string()
                .contains("Git stdout exceeded 67108864-byte capture limit")
        );
        assert!(
            !alive(pid.pid),
            "overflow returned before reaping its Git root"
        );
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
    }

    struct FixturePid {
        pid: i32,
        armed: bool,
    }
    impl FixturePid {
        fn new(pid: i32) -> Self {
            Self { pid, armed: true }
        }
        fn disarm(&mut self) {
            self.armed = false;
        }
    }
    impl Drop for FixturePid {
        fn drop(&mut self) {
            // SAFETY: PID is supplied by this isolated fixture's start handshake.
            unsafe {
                if self.armed {
                    libc::kill(self.pid, libc::SIGKILL);
                }
            }
        }
    }

    #[tokio::test]
    async fn cancelled_token_never_starts_a_command() {
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("started");
        let runner = GitRunner::default();
        let cancellation = ProcessCancellation::new();
        cancellation.cancel();
        let command = python(
            "import os,pathlib,sys; pathlib.Path(sys.argv[1]).write_text(str(os.getpid()))",
            &marker,
        );

        let result = runner.run_command(command, &cancellation).await;

        assert!(matches!(result, Err(GitError::Cancelled)));
        assert!(!marker.exists());
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn request_cancelled_between_commands_blocks_next_spawn() {
        // Arrange: finish one command before requesting stop at the public run gate.
        let fixture = tempfile::tempdir().unwrap();
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        let gate = ProcessStartGate::new();
        let runner = GitRunner::for_run(
            gate.clone(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        );
        let cancellation = ProcessCancellation::new();
        let script =
            "import os,pathlib,sys; pathlib.Path(sys.argv[1]).write_text(str(os.getpid()))";
        runner
            .run_command(python(script, &first), &cancellation)
            .await
            .unwrap();
        gate.cancel();

        // Act: the scheduler has not propagated its lifecycle cancellation yet.
        let result = runner
            .run_command(python(script, &second), &cancellation)
            .await;

        // Assert.
        assert!(first.exists());
        assert!(matches!(result, Err(GitError::Cancelled)));
        assert!(!second.exists());
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_run_deadline_blocks_spawn_without_scheduler_poll() {
        // Arrange.
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("started");
        let runner = GitRunner::for_run(ProcessStartGate::new(), tokio::time::Instant::now());
        let command = python(
            "import os,pathlib,sys; pathlib.Path(sys.argv[1]).write_text(str(os.getpid()))",
            &marker,
        );

        // Act.
        let result = runner
            .run_command(command, &ProcessCancellation::new())
            .await;

        // Assert.
        assert!(matches!(result, Err(GitError::Cancelled)));
        assert!(!marker.exists());
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn successful_cleanup_preserves_initial_wait_failure_for_drain() {
        struct ReapedChild;
        impl GitChild for ReapedChild {
            fn start_kill(&mut self) -> std::io::Result<()> {
                Ok(())
            }
            fn reap(&mut self) -> impl std::future::Future<Output = std::io::Result<()>> {
                std::future::ready(Ok(()))
            }
        }
        // Arrange: a wait/read failed, but the subsequent cleanup wait succeeds.
        let runner = GitRunner::default();
        runner.state.active.fetch_add(1, Ordering::AcqRel);
        let mut lifecycle = ActiveLifecycle {
            state: Arc::clone(&runner.state),
            settled: false,
        };

        // Act: the shell can discard the original effect after observing a stop.
        let _ = terminate_and_reap(
            &mut ReapedChild,
            &mut lifecycle,
            Some("injected initial wait failure"),
        )
        .await;
        drop(lifecycle);
        let result = runner.drain_for_shutdown(Duration::from_secs(1)).await;

        // Assert: shutdown still reports the original lifecycle failure.
        assert!(
            result
                .unwrap_err()
                .contains("injected initial wait failure")
        );
    }

    #[tokio::test]
    async fn cancelling_started_child_reaps_before_returning() {
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("started");
        let runner = GitRunner::default();
        let cancellation = ProcessCancellation::new();
        let command = python(
            "import os,pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); time.sleep(30)",
            &marker,
        );
        let task_runner = runner.clone();
        let task_cancellation = cancellation.clone();
        let task =
            tokio::spawn(async move { task_runner.run_command(command, &task_cancellation).await });
        let mut pid = FixturePid::new(announced(&marker).await);

        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();

        pid.disarm(); // The runner returned only after reap; do not signal a reusable PID.
        assert!(matches!(result, Err(GitError::Cancelled)));
        assert!(!alive(pid.pid), "Git root was not reaped");
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn dropping_waiter_cancels_and_preserves_lifecycle_owner() {
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("started");
        let runner = GitRunner::default();
        let command = python(
            "import os,pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); time.sleep(30)",
            &marker,
        );
        let task_runner = runner.clone();
        let task = tokio::spawn(async move {
            task_runner
                .run_command(command, &ProcessCancellation::new())
                .await
        });
        let mut pid = FixturePid::new(announced(&marker).await);

        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();

        pid.disarm();
        assert!(!alive(pid.pid), "waiter drop abandoned its Git root");
    }

    #[tokio::test]
    async fn cancellation_closes_pipes_after_root_has_already_been_reaped() {
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("started");
        let runner = GitRunner::default();
        let cancellation = ProcessCancellation::new();
        let command = python(
            "import os,pathlib,subprocess,sys; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); pathlib.Path(sys.argv[1] + '.root').write_text(str(os.getpid()))",
            &marker,
        );
        let task_runner = runner.clone();
        let task_cancellation = cancellation.clone();
        let task =
            tokio::spawn(async move { task_runner.run_command(command, &task_cancellation).await });
        let descendant = FixturePid::new(announced(&marker).await);
        let root = announced(&marker.with_extension("root")).await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while alive(root) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("Git root was not reaped before pipe cancellation");

        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();

        assert!(matches!(result, Err(GitError::Cancelled)));
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
        assert!(
            alive(descendant.pid),
            "fixture must exercise a pipe held beyond direct root exit"
        );
    }

    #[tokio::test]
    async fn normal_completion_keeps_stdout_stderr_and_status() {
        let fixture = tempfile::tempdir().unwrap();
        let runner = GitRunner::default();
        let command = python(
            "import sys; sys.stdout.buffer.write(b'\\0bytes\\n'); sys.stderr.write('git diagnostic'); sys.exit(7)",
            fixture.path(),
        );

        let output = runner
            .run_command(command, &ProcessCancellation::new())
            .await
            .unwrap();

        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"\0bytes\n");
        assert_eq!(output.stderr, b"git diagnostic");
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn cancellation_racing_normal_exit_keeps_reap_and_output_contract() {
        let fixture = tempfile::tempdir().unwrap();
        let marker = fixture.path().join("started");
        let runner = GitRunner::default();
        let cancellation = ProcessCancellation::new();
        let command = python(
            "import os,pathlib,sys\np=pathlib.Path(sys.argv[1])\np.write_text(str(os.getpid()))\nwhile not pathlib.Path(str(p) + '.release').exists(): pass\nsys.stdout.write('complete')\n",
            &marker,
        );
        let task_runner = runner.clone();
        let task_cancellation = cancellation.clone();
        let task =
            tokio::spawn(async move { task_runner.run_command(command, &task_cancellation).await });
        let mut pid = FixturePid::new(announced(&marker).await);

        std::fs::write(marker.with_extension("release"), "release").unwrap();
        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();

        pid.disarm();
        match result {
            Ok(output) => {
                assert!(output.status.success());
                assert_eq!(output.stdout, b"complete");
            }
            Err(error) => assert!(matches!(error, GitError::Cancelled)),
        }
        assert!(!alive(pid.pid));
        runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap();
    }
    struct FailedReapChild {
        first_reap: tokio::sync::oneshot::Sender<()>,
        retry: tokio::sync::oneshot::Receiver<()>,
        fail_kill: bool,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Drop for FailedReapChild {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Release);
        }
    }

    impl GitChild for FailedReapChild {
        fn start_kill(&mut self) -> std::io::Result<()> {
            if self.fail_kill {
                Err(std::io::Error::other("injected kill failure"))
            } else {
                Ok(())
            }
        }
        async fn reap(&mut self) -> std::io::Result<()> {
            let (placeholder, _) = tokio::sync::oneshot::channel();
            let first = std::mem::replace(&mut self.first_reap, placeholder);
            if first.send(()).is_ok() {
                Err(std::io::Error::other("injected wait failure"))
            } else {
                (&mut self.retry).await.unwrap();
                Ok(())
            }
        }
    }

    async fn failed_reap_case(fail_kill: bool) {
        let runner = GitRunner::default();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (first_reap, observed) = tokio::sync::oneshot::channel();
        let (release, retry) = tokio::sync::oneshot::channel();
        let child = FailedReapChild {
            first_reap,
            retry,
            fail_kill,
            dropped: Arc::clone(&dropped),
        };
        runner.state.active.fetch_add(1, Ordering::AcqRel);
        let lifecycle = ActiveLifecycle {
            state: Arc::clone(&runner.state),
            settled: false,
        };
        let task = tokio::spawn(async move {
            let mut child = child;
            let mut lifecycle = lifecycle;
            terminate_and_reap(&mut child, &mut lifecycle, None).await
        });
        observed.await.unwrap();

        let premature = runner.drain_for_shutdown(Duration::ZERO).await.unwrap_err();

        assert!(
            !dropped.load(Ordering::Acquire),
            "failed wait released its child owner"
        );
        assert_eq!(runner.state.active.load(Ordering::Acquire), 1);
        assert!(premature.contains("injected wait failure"));
        if fail_kill {
            assert!(premature.contains("injected kill failure"));
        }
        release.send(()).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.contains("injected wait failure"));
        if fail_kill {
            assert!(error.contains("injected kill failure"));
        }
        assert!(dropped.load(Ordering::Acquire));
        let persistent = runner
            .drain_for_shutdown(Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(persistent.contains("injected wait failure"));
        if fail_kill {
            assert!(persistent.contains("injected kill failure"));
        }
    }

    #[tokio::test]
    async fn failed_reap_retains_owner_and_never_reports_a_successful_drain() {
        failed_reap_case(false).await;
    }

    #[tokio::test]
    async fn simultaneous_kill_and_reap_failures_are_both_preserved() {
        failed_reap_case(true).await;
    }
}
