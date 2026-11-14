fn inject_quiescence_failure(supervisor: &mut ProcessSupervisor) {
    #[allow(
        irrefutable_let_patterns,
        reason = "other platforms also compile non-portable supervisors"
    )]
    if let ProcessSupervisor::Portable(portable) = supervisor {
        portable
            .inject_quiescence_failure_for_tests(std::io::Error::other("injected probe failure"));
    }
}

#[cfg(unix)]
async fn wait_for_fixture_pid(path: &std::path::Path) -> i32 {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(pid) = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap()
}

use super::*;
use crate::resource::{PortableBackend, ResourceBackend};
use hoimin_core::{ProbeContext, ProcessLimits};

fn handler(directory: &std::path::Path) -> ProcessHandler {
    ProcessHandler::new(
        ResourceBackend::Portable(PortableBackend::for_tests()),
        Utf8PathBuf::from_path_buf(directory.to_path_buf()).unwrap(),
    )
}

fn typed_request(argv: Vec<CommandArg>) -> RunProcess<ProbeContext> {
    RunProcess {
        id: EffectId(80),
        context: ProbeContext,
        argv,
        cwd: ".".into(),
        limits: ProcessLimits {
            timeout: Duration::from_secs(1),
            max_output_bytes: 100,
            max_memory_bytes: 1024 * 1024,
            max_processes: 8,
        },
    }
}

fn request(argv: Vec<CommandArg>) -> ProcessRequest {
    typed_request(argv).into()
}

#[test]
fn default_cancellation_is_not_requested() {
    let cancellation = ProcessCancellation::default();

    let actual = !cancellation.is_cancelled();

    assert!(actual);
}

#[test]
fn invalid_spool_identity_is_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let error = handler(directory.path())
        .spool_path(&OutputSpoolRef {
            token: "not-a-uuid".into(),
            retained: 0,
            observed: 0,
        })
        .unwrap_err();

    let actual = matches!(error, ProcessError::InvalidSpoolToken(value) if value == "not-a-uuid");

    assert!(actual);
}

fn foreign_argument() -> CommandArg {
    #[cfg(unix)]
    {
        CommandArg::Windows(vec![65])
    }
    #[cfg(windows)]
    {
        CommandArg::Unix(vec![65])
    }
}

#[rstest::rstest]
#[case::empty(Vec::new(), "process.argv.empty")]
#[case::foreign(vec![foreign_argument()], "process.argv.invalid")]
#[tokio::test]
async fn argument_rejection_precedes_output_creation(
    #[case] argv: Vec<CommandArg>,
    #[case] code: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("uncreated");
    let handler = handler(&output);

    let error = handler.run_owned(request(argv)).await.unwrap_err();

    let actual = (error.failure.code()).to_owned();
    let actual_2 = !output.exists();

    assert_eq!(actual, code);
    assert!(actual_2);
}

#[tokio::test]
async fn output_directory_failure_is_reported_before_spawn() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("file");
    std::fs::write(&output, "occupied").unwrap();
    let program = native_program();
    let error = handler(&output)
        .run_owned(request(vec![program]))
        .await
        .unwrap_err();

    let actual = (error.failure.code()).to_owned();
    let actual_2 = std::fs::read_to_string(output).unwrap();

    assert_eq!(actual, "process.output.create");
    assert_eq!(actual_2, "occupied");
}

#[tokio::test]
async fn a_closed_start_gate_cancels_before_process_spawn() {
    let directory = tempfile::tempdir().unwrap();
    let program = native_program();
    let gate = ProcessStartGate::new();
    gate.cancel();
    let error = handler(directory.path())
        .run_owned(request(vec![program]).with_start_gate(gate))
        .await
        .unwrap_err();

    let actual = (error.failure.code()).to_owned();

    assert_eq!(actual, "process.cancelled.before_spawn");
}

#[rstest::rstest]
#[case::read(|| tokio::spawn(async { Err(std::io::Error::other("read failed")) }), "stdout", "process.stdout.read")]
#[case::join(|| tokio::spawn(async { panic!("pipe task failed") }), "stderr", "process.stderr.join")]
#[tokio::test]
async fn pipe_join_and_io_errors_keep_the_stream_identity(
    #[case] spawn: fn() -> tokio::task::JoinHandle<std::io::Result<()>>,
    #[case] stream: &'static str,
    #[case] code: &str,
) {
    let id = EffectId(81);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut task = spawn();

    let error = await_pipe_until(id, stream, &mut task, deadline)
        .await
        .unwrap_err();

    let actual = (error.failure.code()).to_owned();

    assert_eq!(actual, code);
}

#[tokio::test]
async fn collector_panic_is_distinct_from_an_output_write_failure() {
    let mut collector: tokio::task::JoinHandle<std::io::Result<OutputSpoolRef>> =
        tokio::spawn(async { panic!("collector task failed") });
    let outcome = await_collector(
        EffectId(82),
        &mut collector,
        tokio::time::Instant::now() + Duration::from_secs(1),
    )
    .await;

    let actual = (outcome.result.unwrap_err().failure.code()).to_owned();

    assert!(outcome.quiescent);
    assert_eq!(actual, "process.output.join");
}

#[tokio::test]
async fn a_failed_stderr_pipe_keeps_its_failure_after_collector_completion() {
    let stdout = tokio::spawn(async { Ok(()) });
    let stderr = tokio::spawn(async { Err(std::io::Error::other("stderr failed")) });
    let collector = tokio::spawn(async {
        Ok(OutputSpoolRef {
            token: "output".into(),
            retained: 0,
            observed: 0,
        })
    });
    let outcome = await_output(
        EffectId(83),
        stdout,
        stderr,
        collector,
        tokio::time::Instant::now() + Duration::from_secs(1),
    )
    .await;

    let actual = (outcome.result.unwrap_err().failure.code()).to_owned();

    assert!(outcome.quiescent);
    assert_eq!(actual, "process.stderr.read");
}

fn native_program() -> CommandArg {
    #[cfg(unix)]
    {
        CommandArg::Unix(b"unused".to_vec())
    }
    #[cfg(windows)]
    {
        CommandArg::Windows("unused".encode_utf16().collect())
    }
}

#[cfg(unix)]
#[tokio::test]
async fn root_wait_reports_a_child_reaped_by_another_owner() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id().unwrap()).unwrap();
    let mut status = 0;
    // SAFETY: pid identifies this test's child, and status is valid writable storage.
    let reaped_pid = unsafe { libc::waitpid(pid, &raw mut status, 0) };
    let error = wait_after_termination(EffectId(84), &mut child)
        .await
        .unwrap_err();
    let wait_error_code = (error.failure.code()).to_owned();

    assert_eq!(reaped_pid, pid);
    assert_eq!(wait_error_code, "process.wait");
}

#[tokio::test]
async fn classification_cannot_claim_a_supervisor_after_ownership_is_released() {
    let mut command = Command::new("unused");
    let limits = request(Vec::new()).process.limits;
    let supervisor = ResourceBackend::Portable(PortableBackend::for_tests())
        .prepare(&mut command, limits)
        .unwrap();
    let mut owner = BlockingOwner::new(supervisor);
    owner.finish(EffectId(85)).await.unwrap();
    let (result, reaped) =
        classify_and_terminate(EffectId(85), &mut owner, ProcessTermination::Exit(0)).await;

    let actual = (result.unwrap_err().failure.code()).to_owned();

    assert_eq!(actual, "process.resource.owner");
    assert!(!reaped);
}

#[test]
fn cleanup_fallback_message_preserves_a_foreign_error_without_claiming_a_process_code() {
    let error = EffectFailed {
        id: EffectId(86),
        failure: EffectFailure::SessionDatabase {
            code: "session".into(),
            operation: "foreign cleanup".into(),
            message: "detail".into(),
        },
    };
    let observed_1 = failure_message(&error);
    let mut primary = EffectFailed::other(EffectId(86), "process", "primary");
    append_cleanup_failure(&mut primary, "foreign", &error);
    let observed_2 = failure_message(&primary);

    assert_eq!(observed_1, "non-process cleanup failure");
    assert_eq!(observed_2, "primary; foreign: non-process cleanup failure");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_is_rechecked_after_waiting_for_spawn_authority() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("created-before-spawn-gate");
    let handler = handler(&output);
    let cancellation = ProcessCancellation::new();
    let gate = cancellation.begin_spawn();
    let request = request(vec![native_program()]).with_cancellation(cancellation.clone());
    let running = tokio::spawn(async move { handler.run_owned(request).await });
    tokio::task::block_in_place(|| {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !output.exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        // Complete cancellation while owning its gate, before granting spawn authority.
        cancellation.state.cancelled.store(true, Ordering::Release);
        drop(gate);
    });
    let error = running.await.unwrap().unwrap_err();

    let actual = (error.failure.code()).to_owned();
    let actual_2 = std::fs::read_dir(output).unwrap().next().is_none();

    assert_eq!(actual, "process.cancelled.before_spawn");
    assert!(actual_2);
}

#[tokio::test]
async fn classification_failure_keeps_both_failed_termination_attempts() {
    let mut command = Command::new("unused");
    let supervisor = ResourceBackend::Portable(
        PortableBackend::for_tests_with_classification_and_termination_failure(),
    )
    .prepare(&mut command, request(Vec::new()).process.limits)
    .unwrap();
    let mut owner = BlockingOwner::new(supervisor);
    let (result, reaped) =
        classify_and_terminate(EffectId(87), &mut owner, ProcessTermination::Exit(0)).await;
    let error = result.unwrap_err();
    let classification_error_code = (error.failure.code()).to_owned();
    let message = failure_message(&error);
    let first_termination_failed = message.contains("supervised termination also failed");
    let retry_termination_failed = message.contains("supervisor termination retry failed");
    let not_reaped = !reaped;
    owner.finish(EffectId(87)).await.unwrap();

    assert_eq!(classification_error_code, "process.resource.classify");
    assert!(first_termination_failed);
    assert!(retry_termination_failed);
    assert!(not_reaped);
}

#[tokio::test]
async fn missing_supervisor_still_kills_and_reaps_the_owned_root() {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--ignored",
            "--exact",
            "process::tests::root_child_fixture_waits",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut limits = request(Vec::new()).process.limits;
    // The fixture is the instrumented Rust test binary, not a memory-limit probe.
    limits.max_memory_bytes = 256 * 1024 * 1024;
    let supervisor = ResourceBackend::Portable(PortableBackend::for_tests())
        .prepare(&mut command, limits)
        .unwrap();
    let mut owner = BlockingOwner::new(supervisor);
    owner.finish(EffectId(88)).await.unwrap();
    let mut child = command.spawn().unwrap();
    let error = terminate_and_reap(EffectId(88), &mut owner, &mut child)
        .await
        .unwrap_err();

    let actual = (error.failure.code()).to_owned();
    let actual_2 = failure_message(&error).contains("supervisor termination retry failed");
    let actual_3 = child.try_wait().unwrap().is_some();

    assert_eq!(actual, "process.resource.owner");
    assert!(actual_2);
    assert!(actual_3);
}

#[test]
#[should_panic(expected = "process cleanup produces only I/O or other failures")]
fn cleanup_rejects_a_primary_error_from_an_unrelated_effect_domain() {
    let mut primary = EffectFailed {
        id: EffectId(89),
        failure: EffectFailure::SessionDatabase {
            code: "session".into(),
            operation: "foreign".into(),
            message: "primary".into(),
        },
    };
    let cleanup = EffectFailed::other(EffectId(89), "process.cleanup", "cleanup");
    append_cleanup_failure(&mut primary, "cleanup", &cleanup);
}

enum WaitStep {
    Ready(std::io::Result<()>),
    Pending,
}

struct ScriptedRoot {
    waits: std::collections::VecDeque<WaitStep>,
    kill: std::io::Result<()>,
    wait_count: usize,
    kill_count: usize,
}

impl TerminatingRoot for ScriptedRoot {
    fn wait_root(&mut self) -> impl Future<Output = std::io::Result<()>> + Send {
        self.wait_count += 1;
        let future: std::pin::Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>> =
            match self.waits.pop_front().unwrap() {
                WaitStep::Ready(result) => Box::pin(std::future::ready(result)),
                WaitStep::Pending => Box::pin(std::future::pending()),
            };
        future
    }

    fn kill_root(&mut self) -> std::io::Result<()> {
        self.kill_count += 1;
        std::mem::replace(&mut self.kill, Ok(()))
    }
}

#[rstest::rstest]
#[case(vec![WaitStep::Ready(Ok(()))], Ok(()), None, 1, 0, None)]
#[case(
            vec![WaitStep::Ready(Err(std::io::Error::other("injected wait error")))],
            Ok(()),
            Some("process.wait"),
            1,
            0,
            Some("wait after process termination"),
        )]
#[case(
            vec![WaitStep::Pending, WaitStep::Ready(Ok(()))],
            Ok(()),
            None,
            2,
            1,
            None,
        )]
#[case(
            vec![WaitStep::Pending, WaitStep::Ready(Ok(()))],
            Err(std::io::Error::other("injected kill error")),
            Some("process.kill"),
            2,
            1,
            Some("injected kill error"),
        )]
#[case(
            vec![WaitStep::Pending, WaitStep::Ready(Err(std::io::Error::other("injected wait error")))],
            Ok(()),
            Some("process.wait"),
            2,
            1,
            Some("wait after root kill"),
        )]
#[case(
            vec![WaitStep::Pending, WaitStep::Ready(Err(std::io::Error::other("injected wait error")))],
            Err(std::io::Error::other("injected kill error")),
            Some("process.kill"),
            2,
            1,
            Some("root wait also failed: injected wait error"),
        )]
#[case(
            vec![WaitStep::Pending, WaitStep::Pending],
            Ok(()),
            Some("process.wait.timeout"),
            2,
            1,
            Some("timed out reaping root after kill"),
        )]
#[case(
            vec![WaitStep::Pending, WaitStep::Pending],
            Err(std::io::Error::other("injected kill error")),
            Some("process.kill"),
            2,
            1,
            Some("root wait also failed: timed out reaping root after kill"),
        )]
#[tokio::test(start_paused = true)]
async fn bounded_root_reaping_preserves_kill_and_wait_errors_in_order(
    #[case] steps: Vec<WaitStep>,
    #[case] kill: std::io::Result<()>,
    #[case] expected_code: Option<&str>,
    #[case] expected_waits: usize,
    #[case] expected_kills: usize,
    #[case] expected_detail: Option<&str>,
) {
    let mut root = ScriptedRoot {
        waits: steps.into(),
        kill,
        wait_count: 0,
        kill_count: 0,
    };
    let result = wait_after_termination_for(EffectId(90), &mut root).await;

    let actual = root.waits.is_empty();
    let actual_2 = result.as_ref().err().map(|error| error.failure.code());
    let actual_3 = expected_detail.is_none_or(|detail| {
        result
            .as_ref()
            .err()
            .is_some_and(|error| format!("{:?}", error.failure).contains(detail))
    });

    assert_eq!(root.wait_count, expected_waits);
    assert_eq!(root.kill_count, expected_kills);
    assert!(actual);
    assert_eq!(actual_2, expected_code);
    assert!(actual_3);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn lifecycle_preserves_a_real_wait_error_when_the_root_is_reaped_externally() {
    use std::os::unix::ffi::OsStrExt;
    let directory = tempfile::tempdir().unwrap();
    let pid_path = directory.path().join("root.pid");
    let release = directory.path().join("release");
    let script = b"printf '%s\\n' \"$$\" > \"$1\"; i=0; while [ ! -f \"$2\" ] && [ \"$i\" -lt 200 ]; do i=$((i+1)); sleep .01; done";
    let handler = handler(&directory.path().join("output"));
    let observed = handler.clone();
    let mut process = request(vec![
        CommandArg::Unix(b"/bin/sh".to_vec()),
        CommandArg::Unix(b"-c".to_vec()),
        CommandArg::Unix(script.to_vec()),
        CommandArg::Unix(b"fixture".to_vec()),
        CommandArg::Unix(pid_path.as_os_str().as_bytes().to_vec()),
        CommandArg::Unix(release.as_os_str().as_bytes().to_vec()),
    ]);
    process.process.limits.timeout = Duration::from_secs(5);
    process.process.limits.max_memory_bytes = 256 * 1024 * 1024;
    let running = tokio::spawn(async move { handler.run(process).await });
    let pid = wait_for_fixture_pid(&pid_path).await;
    std::fs::write(release, "release").unwrap();
    let mut status = 0;
    // Keep this current-thread runtime unpolled until waitpid has reaped our released fixture.
    // SAFETY: pid belongs to this test, and status is valid writable storage.
    let externally_reaped_pid = unsafe { libc::waitpid(pid, &raw mut status, 0) };
    let failure = running.await.unwrap().unwrap_err();
    let wait_error_code = (failure.failure.code()).to_owned();
    let cleanup_failure_reported =
        failure_message(&failure).contains("supervised cleanup after wait error failed");
    let drained = observed.drain_for_shutdown(Duration::from_secs(1)).await;
    let reap_unproven = !drained.all_reaped;
    let output_joined = drained.output_drains_joined;

    assert_eq!(externally_reaped_pid, pid);
    assert_eq!(wait_error_code, "process.wait");
    assert!(cleanup_failure_reported);
    assert!(reap_unproven);
    assert!(output_joined);
}

#[cfg(unix)]
#[tokio::test]
async fn debug_span_fields_preserve_successful_process_outcomes() {
    use tracing::instrument::WithSubscriber;
    let directory = tempfile::tempdir().unwrap();
    let handler = handler(directory.path());
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::sink)
        .finish();
    let mut process = typed_request(vec![CommandArg::Unix(b"/usr/bin/true".to_vec())]);
    // This test exercises span fields, not address-space enforcement or loader failure.
    process.limits.max_memory_bytes = 64 * 1024 * 1024;
    let finished = handler
        .handle(process)
        .with_subscriber(subscriber)
        .await
        .unwrap();

    assert_eq!(finished.termination, ProcessTermination::Exit(0));
    assert_eq!(finished.output.observed, 0);
}

#[test]
fn shutdown_drain_reports_backend_task_cancellation_on_a_stopped_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let handler = handler(directory.path());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = runtime.handle().clone();
    runtime.shutdown_background();
    let report = handle.block_on(handler.drain_for_shutdown(Duration::from_secs(1)));

    let actual = report.secondary_errors.len();
    let actual_2 = report.secondary_errors[0].contains("process resource close task failed");

    assert!(report.all_reaped);
    assert!(report.output_drains_joined);
    assert_eq!(actual, 1);
    assert!(actual_2);
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn quiescence_probe_errors_are_reported_and_an_unknown_tree_remains_unproven() {
    let backend = ResourceBackend::Portable(PortableBackend::for_tests());
    let mut command = Command::new("unused");
    let mut supervisor = backend
        .prepare(&mut command, request(Vec::new()).process.limits)
        .unwrap();
    inject_quiescence_failure(&mut supervisor);
    let mut owner = BlockingOwner::new(supervisor);
    let error = await_tree_quiescence_after_root_reap(EffectId(91), &mut owner)
        .await
        .unwrap_err();
    let probe_error_code = (error.failure.code()).to_owned();
    let probe_error_reported = failure_message(&error).contains("injected probe failure");
    let started = tokio::time::Instant::now();
    let tree_unproven = !await_tree_quiescence_after_root_reap(EffectId(91), &mut owner)
        .await
        .unwrap();
    let probe_grace_elapsed = started.elapsed() >= PROCESS_TREE_QUIESCENCE_GRACE;
    owner.finish(EffectId(91)).await.unwrap();

    assert_eq!(probe_error_code, "process.resource.quiescence");
    assert!(probe_error_reported);
    assert!(tree_unproven);
    assert!(probe_grace_elapsed);
}

#[rstest::rstest]
#[case(
            vec![],
            Some(std::io::Error::other("kill failed")),
            Some(std::io::ErrorKind::Other),
            0,
        )]
#[case(vec![WaitStep::Ready(Ok(()))], None, None, 1)]
#[case(
            vec![WaitStep::Ready(Err(std::io::Error::other("wait failed")))],
            None,
            Some(std::io::ErrorKind::Other),
            1,
        )]
#[case(
            vec![WaitStep::Pending],
            None,
            Some(std::io::ErrorKind::TimedOut),
            1,
        )]
#[tokio::test(start_paused = true)]
async fn unattached_root_cleanup_keeps_termination_errors_and_bounds_reaping(
    #[case] steps: Vec<WaitStep>,
    #[case] terminate_error: Option<std::io::Error>,
    #[case] expected: Option<std::io::ErrorKind>,
    #[case] expected_waits: usize,
) {
    let mut root = ScriptedRoot {
        waits: steps.into(),
        kill: Ok(()),
        wait_count: 0,
        kill_count: 0,
    };
    let result = reap_unattached_root(&mut root, terminate_error).await;

    let actual = result.as_ref().map(std::io::Error::kind);
    let actual_2 = root.waits.is_empty();

    assert_eq!(actual, expected);
    assert_eq!(root.wait_count, expected_waits);
    assert_eq!(root.kill_count, 0);
    assert!(actual_2);
}

#[cfg(unix)]
#[tokio::test]
async fn failed_attachment_reaps_the_root_before_releasing_lifecycle_ownership() {
    let directory = tempfile::tempdir().unwrap();
    let handler = ProcessHandler::new(
        ResourceBackend::Portable(PortableBackend::for_tests_with_attachment_failure()),
        Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap(),
    );
    let mut process = request(vec![
        CommandArg::Unix(b"/bin/sh".to_vec()),
        CommandArg::Unix(b"-c".to_vec()),
        CommandArg::Unix(b"while :; do :; done".to_vec()),
    ]);
    process.process.limits.max_memory_bytes = 64 * 1024 * 1024;
    let error = handler.run(process).await.unwrap_err();
    let attachment_error_code = (error.failure.code()).to_owned();
    let injected_failure_reported =
        failure_message(&error).contains("injected portable attachment failure");
    let report = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    let all_reaped = report.all_reaped;
    let output_joined = report.output_drains_joined;
    let secondary_errors = (report.secondary_errors).clone();
    let spool_count = std::fs::read_dir(directory.path()).unwrap().count();

    assert_eq!(attachment_error_code, "process.resource.attach");
    assert!(injected_failure_reported);
    assert!(all_reaped);
    assert!(output_joined);
    assert_eq!(secondary_errors, Vec::<String>::new());
    assert_eq!(spool_count, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn a_backend_close_error_does_not_erase_completed_reap_and_output_proofs() {
    let directory = tempfile::tempdir().unwrap();
    let handler = ProcessHandler::new(
        ResourceBackend::Portable(PortableBackend::for_tests_with_close_failure()),
        Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap(),
    );
    let report = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    let initial_all_reaped = report.all_reaped;
    let initial_output_joined = report.output_drains_joined;
    let initial_error_count = report.secondary_errors.len();
    let close_failure_reported =
        report.secondary_errors[0].contains("injected portable close failure");
    let retried = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    let retry_all_reaped = retried.all_reaped;
    let retry_output_joined = retried.output_drains_joined;
    let retry_errors = (retried.secondary_errors).clone();

    assert!(initial_all_reaped);
    assert!(initial_output_joined);
    assert_eq!(initial_error_count, 1);
    assert!(close_failure_reported);
    assert!(retry_all_reaped);
    assert!(retry_output_joined);
    assert_eq!(retry_errors, Vec::<String>::new());
}

#[rstest::rstest]
#[case::both_succeed(Ok(ProcessTermination::Exit(0)), Ok(()), Ok(ProcessTermination::Exit(0)), false)]
#[case::process_failed(Err(EffectFailed::other(EffectId(92), "process.wait", "primary")), Ok(()), Err(EffectFailed::other(EffectId(92), "process.wait", "primary")), false)]
#[case::cleanup_failed(
    Ok(ProcessTermination::Exit(0)),
    Err(EffectFailed::other(EffectId(92), "process.resource.join", "cleanup")),
    Err(EffectFailed::other(EffectId(92), "process.resource.join", "cleanup")),
    true
)]
#[case::both_failed(
    Err(EffectFailed::other(EffectId(92), "process.wait", "primary")),
    Err(EffectFailed::other(EffectId(92), "process.resource.join", "cleanup")),
    Err(EffectFailed::other(
        EffectId(92),
        "process.wait",
        "primary; supervisor destruction failed: cleanup"
    )),
    true
)]
fn supervisor_cleanup_preserves_primary_failure_and_never_invents_reap_evidence(
    #[case] mut result: Result<ProcessTermination, EffectFailed>,
    #[case] cleanup: Result<(), EffectFailed>,
    #[case]
    #[by_ref]
    expected: &Result<ProcessTermination, EffectFailed>,
    #[case] cleanup_failed: bool,
    #[values(false, true)] initially_reaped: bool,
) {
    let mut reaped = initially_reaped;

    apply_supervisor_cleanup(&mut result, &mut reaped, cleanup);

    assert_eq!(reaped, initially_reaped && !cleanup_failed);
    assert_eq!(&result, expected);
}

#[tokio::test(start_paused = true)]
async fn supervised_cleanup_keeps_all_failed_attempts_in_execution_order() {
    let backend = ResourceBackend::Portable(
        PortableBackend::for_tests_with_classification_and_termination_failure(),
    );
    let mut command = Command::new("unused");
    let supervisor = backend
        .prepare(&mut command, request(Vec::new()).process.limits)
        .unwrap();
    let mut owner = BlockingOwner::new(supervisor);
    let mut root = ScriptedRoot {
        waits: [WaitStep::Ready(Err(std::io::Error::other("wait failure")))].into(),
        kill: Err(std::io::Error::other("kill failure")),
        wait_count: 0,
        kill_count: 0,
    };
    let failure = terminate_and_reap(EffectId(93), &mut owner, &mut root)
        .await
        .unwrap_err();
    let primary_failure_code = (failure.failure.code()).to_owned();
    let message = failure_message(&failure);
    let direct_kill = message
        .find("direct root kill failed: kill failure")
        .unwrap();
    let retry = message
        .find("supervisor termination retry failed:")
        .unwrap();
    let wait = message.find("root reap failed: wait failure").unwrap();
    let errors_in_order = direct_kill < retry && retry < wait;
    let kill_count = root.kill_count;
    let wait_count = root.wait_count;
    owner.finish(EffectId(93)).await.unwrap();

    assert_eq!(primary_failure_code, "process.resource.terminate");
    assert!(errors_in_order);
    assert_eq!(kill_count, 1);
    assert_eq!(wait_count, 1);
}

#[tokio::test(start_paused = true)]
async fn a_root_reap_without_process_group_evidence_remains_unproven() {
    let mut command = Command::new("unused");
    let supervisor = ResourceBackend::Portable(PortableBackend::for_tests())
        .prepare(&mut command, request(Vec::new()).process.limits)
        .unwrap();
    let mut owner = BlockingOwner::new(supervisor);
    let mut root = ScriptedRoot {
        waits: [WaitStep::Ready(Ok(()))].into(),
        kill: Ok(()),
        wait_count: 0,
        kill_count: 0,
    };
    let reaped = terminate_and_reap(EffectId(94), &mut owner, &mut root)
        .await
        .unwrap();
    #[cfg(unix)]
    assert!(!reaped);
    #[cfg(not(unix))]
    assert!(reaped);
    let kill_count = root.kill_count;
    let wait_count = root.wait_count;
    owner.finish(EffectId(94)).await.unwrap();

    assert_eq!(kill_count, 0);
    assert_eq!(wait_count, 1);
}

#[rstest::rstest]
fn attachment_failure_retains_each_cleanup_error_and_requires_both_completion_proofs(
    #[values(false, true)] root_failed: bool,
    #[values(false, true)] supervisor_failed: bool,
) {
    let attachment = ResourceError::io(
        "attach supervisor",
        std::io::Error::other("attachment failure"),
    );
    let root_cleanup = root_failed.then(|| std::io::Error::other("root cleanup failure"));
    let supervisor = supervisor_failed
        .then(|| {
            EffectFailed::other(
                EffectId(95),
                "process.resource.join",
                "supervisor cleanup failure",
            )
        })
        .map_or(Ok(()), Err);

    let (failure, reaped) =
        failed_attachment_outcome(EffectId(95), &attachment, root_cleanup.as_ref(), supervisor);
    let message = failure_message(&failure);

    let actual = (failure.failure.code()).to_owned();
    let actual_2 = message.contains("attachment failure");
    let actual_3 = message.contains("spawned-child cleanup failed: root cleanup failure");
    let actual_4 = message.contains("supervisor destruction failed: supervisor cleanup failure");

    assert_eq!(actual, "process.resource.attach");
    assert_eq!(reaped, !root_failed && !supervisor_failed);
    assert!(actual_2);
    assert_eq!(actual_3, root_failed);
    assert_eq!(actual_4, supervisor_failed);
}

#[tokio::test]
async fn cancellation_before_setup_never_creates_the_output_directory() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("not-created");
    let cancellation = ProcessCancellation::new();
    cancellation.cancel();
    let error = handler(&output)
        .run_owned(request(vec![native_program()]).with_cancellation(cancellation))
        .await
        .unwrap_err();

    let actual = (error.failure.code()).to_owned();
    let actual_2 = !output.exists();

    assert_eq!(actual, "process.cancelled.before_spawn");
    assert!(actual_2);
}

#[tokio::test]
async fn stdout_failure_aborts_stderr_and_still_observes_collector_completion() {
    let stdout = tokio::spawn(async { Err(std::io::Error::other("stdout failed")) });
    let stderr = tokio::spawn(std::future::pending::<std::io::Result<()>>());
    let stderr_abort = stderr.abort_handle();
    let collector = tokio::spawn(async {
        Ok(OutputSpoolRef {
            token: "collected".into(),
            retained: 3,
            observed: 3,
        })
    });
    let outcome = await_output(
        EffectId(98),
        stdout,
        stderr,
        collector,
        tokio::time::Instant::now() + Duration::from_secs(1),
    )
    .await;

    let actual = stderr_abort.is_finished();
    let actual_2 = (outcome.result.unwrap_err().failure.code()).to_owned();

    assert!(actual);
    assert!(outcome.quiescent);
    assert_eq!(actual_2, "process.stdout.read");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::success(0, ProcessTermination::Exit(0))]
#[case::signal(libc::SIGTERM, ProcessTermination::Exit(128 + libc::SIGTERM))]
fn signal_exit_status_uses_the_conventional_signal_number(
    #[case] raw: i32,
    #[case] expected: ProcessTermination,
) {
    use std::os::unix::process::ExitStatusExt;

    let actual = exit_termination(std::process::ExitStatus::from_raw(raw));

    assert_eq!(actual, expected);
}

#[tokio::test(start_paused = true)]
async fn pipe_drain_deadline_aborts_and_joins_the_owned_task() {
    let mut pipe = tokio::spawn(std::future::pending::<std::io::Result<()>>());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let error = await_pipe_until(EffectId(99), "stdout", &mut pipe, deadline)
        .await
        .unwrap_err();

    let actual = (error.failure.code()).to_owned();
    let actual_2 = pipe.is_finished();
    let actual_3 = tokio::time::Instant::now();

    assert_eq!(actual, "process.output.close.timeout");
    assert!(actual_2);
    assert_eq!(actual_3, deadline);
}

#[tokio::test]
async fn a_successful_termination_retry_preserves_the_original_failure_without_inventing_tree_proof()
 {
    let mut command = Command::new("unused");
    let supervisor =
        ResourceBackend::Portable(PortableBackend::for_tests_with_termination_failure())
            .prepare(&mut command, request(Vec::new()).process.limits)
            .unwrap();
    let mut owner = BlockingOwner::new(supervisor);
    let (result, reaped) =
        classify_and_terminate(EffectId(100), &mut owner, ProcessTermination::Exit(0)).await;
    let error = result.unwrap_err();
    let primary_failure_code = (error.failure.code()).to_owned();
    let no_retry_failure = !failure_message(&error).contains("supervisor termination retry failed");
    let reap_unproven = !reaped;
    owner.finish(EffectId(100)).await.unwrap();

    assert_eq!(primary_failure_code, "process.resource.terminate");
    assert!(no_retry_failure);
    assert!(reap_unproven);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn attachment_failure_after_external_reap_does_not_claim_a_cleanup_proof() {
    let directory = tempfile::tempdir().unwrap();
    let handler = ProcessHandler::new(
        ResourceBackend::Portable(
            PortableBackend::for_tests_with_attachment_failure_after_external_reap(),
        ),
        Utf8PathBuf::from_path_buf(directory.path().to_owned()).unwrap(),
    );
    let mut request = typed_request(vec![CommandArg::Unix(b"/usr/bin/true".to_vec())]);
    request.limits.max_memory_bytes = 64 * 1024 * 1024;
    let error = handler.handle(request).await.unwrap_err();
    let attachment_error_code = (error.failure.code()).to_owned();
    let cleanup_failure_reported = failure_message(&error).contains("spawned-child cleanup failed");
    let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    let reap_unproven = !drain.all_reaped;
    let output_joined = drain.output_drains_joined;
    let secondary_errors = (drain.secondary_errors).clone();
    let spool_empty = std::fs::read_dir(directory.path())
        .unwrap()
        .next()
        .is_none();

    assert_eq!(attachment_error_code, "process.resource.attach");
    assert!(cleanup_failure_reported);
    assert!(reap_unproven);
    assert!(output_joined);
    assert_eq!(secondary_errors, Vec::<String>::new());
    assert!(spool_empty);
}

#[cfg(unix)]
#[rstest::rstest]
#[case::released_collector(|gate: &CollectorPause| gate.release.notify_one(), Ok(ProcessTermination::Exit(0)), true)]
#[case::stalled_collector(|_: &CollectorPause| {}, Err("process.output.close.timeout".to_owned()), false)]
#[tokio::test(flavor = "current_thread")]
async fn collector_completion_is_required_for_the_handlers_output_drain_proof(
    #[case] release_collector: fn(&CollectorPause),
    #[case] expected: Result<ProcessTermination, String>,
    #[case] release: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let mut handler = handler(directory.path());
    let gate = Arc::new(CollectorPause::default());
    handler.collector_pause = Some(Arc::clone(&gate));
    let owned = handler.clone();
    let mut request = typed_request(vec![CommandArg::Unix(b"/usr/bin/true".to_vec())]);
    request.limits.max_memory_bytes = 64 * 1024 * 1024;
    let running = tokio::spawn(async move { owned.handle(request).await });
    gate.entered.notified().await;
    release_collector(&gate);
    let result = running
        .await
        .unwrap()
        .map(|result| result.termination)
        .map_err(|error| error.failure.code().to_owned());
    let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;

    assert_eq!(result, expected);
    assert!(drain.all_reaped);
    assert_eq!(drain.output_drains_joined, release);
    assert_eq!(drain.secondary_errors, Vec::<String>::new());
}
