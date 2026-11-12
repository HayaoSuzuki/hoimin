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
fn default_cancellation_and_invalid_spool_identity_are_explicit() {
    assert!(!ProcessCancellation::default().is_cancelled());
    let directory = tempfile::tempdir().unwrap();
    let error = handler(directory.path())
        .spool_path(&OutputSpoolRef {
            token: "not-a-uuid".into(),
            retained: 0,
            observed: 0,
        })
        .unwrap_err();
    assert!(matches!(error, ProcessError::InvalidSpoolToken(value) if value == "not-a-uuid"));
}

#[tokio::test]
async fn argument_rejection_precedes_output_creation() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("uncreated");
    let handler = handler(&output);
    let error = handler.run_owned(request(Vec::new())).await.unwrap_err();
    assert_eq!(error.failure.code(), "process.argv.empty");
    #[cfg(unix)]
    let foreign = CommandArg::Windows(vec![65]);
    #[cfg(windows)]
    let foreign = CommandArg::Unix(vec![65]);
    let error = handler.run_owned(request(vec![foreign])).await.unwrap_err();
    assert_eq!(error.failure.code(), "process.argv.invalid");
    assert!(!output.exists());
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
    assert_eq!(error.failure.code(), "process.output.create");
    assert_eq!(std::fs::read_to_string(output).unwrap(), "occupied");
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
    assert_eq!(error.failure.code(), "process.cancelled.before_spawn");
}

#[tokio::test]
async fn pipe_join_and_io_errors_keep_the_stream_identity() {
    let id = EffectId(81);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let mut failed_read = tokio::spawn(async { Err(std::io::Error::other("read failed")) });
    let error = await_pipe_until(id, "stdout", &mut failed_read, deadline)
        .await
        .unwrap_err();
    assert_eq!(error.failure.code(), "process.stdout.read");
    let mut failed_join: tokio::task::JoinHandle<std::io::Result<()>> =
        tokio::spawn(async { panic!("pipe task failed") });
    let error = await_pipe_until(id, "stderr", &mut failed_join, deadline)
        .await
        .unwrap_err();
    assert_eq!(error.failure.code(), "process.stderr.join");
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
    assert!(outcome.quiescent);
    assert_eq!(
        outcome.result.unwrap_err().failure.code(),
        "process.output.join"
    );
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
    assert!(outcome.quiescent);
    assert_eq!(
        outcome.result.unwrap_err().failure.code(),
        "process.stderr.read"
    );
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
    assert_eq!(unsafe { libc::waitpid(pid, &raw mut status, 0) }, pid);
    let error = wait_after_termination(EffectId(84), &mut child)
        .await
        .unwrap_err();
    assert_eq!(error.failure.code(), "process.wait");
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
    assert_eq!(result.unwrap_err().failure.code(), "process.resource.owner");
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
    assert_eq!(failure_message(&error), "non-process cleanup failure");
    let mut primary = EffectFailed::other(EffectId(86), "process", "primary");
    append_cleanup_failure(&mut primary, "foreign", &error);
    assert_eq!(
        failure_message(&primary),
        "primary; foreign: non-process cleanup failure"
    );
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
    assert_eq!(error.failure.code(), "process.cancelled.before_spawn");
    assert!(std::fs::read_dir(output).unwrap().next().is_none());
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
    assert_eq!(error.failure.code(), "process.resource.classify");
    let message = failure_message(&error);
    assert!(message.contains("supervised termination also failed"));
    assert!(message.contains("supervisor termination retry failed"));
    assert!(!reaped);
    owner.finish(EffectId(87)).await.unwrap();
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
    assert_eq!(error.failure.code(), "process.resource.owner");
    assert!(failure_message(&error).contains("supervisor termination retry failed"));
    assert!(child.try_wait().unwrap().is_some());
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

#[tokio::test(start_paused = true)]
async fn bounded_root_reaping_preserves_kill_and_wait_errors_in_order() {
    let wait_error = || std::io::Error::other("injected wait error");
    let kill_error = || std::io::Error::other("injected kill error");
    let cases = [
        (vec![WaitStep::Ready(Ok(()))], Ok(()), None, 1, 0, None),
        (
            vec![WaitStep::Ready(Err(wait_error()))],
            Ok(()),
            Some("process.wait"),
            1,
            0,
            Some("wait after process termination"),
        ),
        (
            vec![WaitStep::Pending, WaitStep::Ready(Ok(()))],
            Ok(()),
            None,
            2,
            1,
            None,
        ),
        (
            vec![WaitStep::Pending, WaitStep::Ready(Ok(()))],
            Err(kill_error()),
            Some("process.kill"),
            2,
            1,
            Some("injected kill error"),
        ),
        (
            vec![WaitStep::Pending, WaitStep::Ready(Err(wait_error()))],
            Ok(()),
            Some("process.wait"),
            2,
            1,
            Some("wait after root kill"),
        ),
        (
            vec![WaitStep::Pending, WaitStep::Ready(Err(wait_error()))],
            Err(kill_error()),
            Some("process.kill"),
            2,
            1,
            Some("root wait also failed: injected wait error"),
        ),
        (
            vec![WaitStep::Pending, WaitStep::Pending],
            Ok(()),
            Some("process.wait.timeout"),
            2,
            1,
            Some("timed out reaping root after kill"),
        ),
        (
            vec![WaitStep::Pending, WaitStep::Pending],
            Err(kill_error()),
            Some("process.kill"),
            2,
            1,
            Some("root wait also failed: timed out reaping root after kill"),
        ),
    ];
    for (steps, kill, expected_code, expected_waits, expected_kills, expected_detail) in cases {
        let mut root = ScriptedRoot {
            waits: steps.into(),
            kill,
            wait_count: 0,
            kill_count: 0,
        };
        let result = wait_after_termination_for(EffectId(90), &mut root).await;
        assert_eq!(root.wait_count, expected_waits);
        assert_eq!(root.kill_count, expected_kills);
        assert!(root.waits.is_empty());
        assert_eq!(
            result.as_ref().err().map(|error| error.failure.code()),
            expected_code
        );
        if let Some(detail) = expected_detail {
            let error = result.unwrap_err();
            assert!(format!("{:?}", error.failure).contains(detail), "{error:?}");
        }
    }
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
    let pid = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(pid) = std::fs::read_to_string(&pid_path)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    std::fs::write(release, "release").unwrap();
    let mut status = 0;
    // Keep this current-thread runtime unpolled until waitpid has reaped our released fixture.
    // SAFETY: pid belongs to this test, and status is valid writable storage.
    assert_eq!(unsafe { libc::waitpid(pid, &raw mut status, 0) }, pid);
    let failure = running.await.unwrap().unwrap_err();
    assert_eq!(failure.failure.code(), "process.wait");
    assert!(failure_message(&failure).contains("supervised cleanup after wait error failed"));
    let drained = observed.drain_for_shutdown(Duration::from_secs(1)).await;
    assert!(!drained.all_reaped);
    assert!(drained.output_drains_joined);
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
    assert!(report.all_reaped);
    assert!(report.output_drains_joined);
    assert_eq!(report.secondary_errors.len(), 1);
    assert!(report.secondary_errors[0].contains("process resource close task failed"));
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn quiescence_probe_errors_are_reported_and_an_unknown_tree_remains_unproven() {
    let backend = ResourceBackend::Portable(PortableBackend::for_tests());
    let mut command = Command::new("unused");
    let mut supervisor = backend
        .prepare(&mut command, request(Vec::new()).process.limits)
        .unwrap();
    #[allow(
        irrefutable_let_patterns,
        reason = "other platforms also compile non-portable supervisors"
    )]
    if let ProcessSupervisor::Portable(portable) = &mut supervisor {
        portable
            .inject_quiescence_failure_for_tests(std::io::Error::other("injected probe failure"));
    }
    let mut owner = BlockingOwner::new(supervisor);
    let error = await_tree_quiescence_after_root_reap(EffectId(91), &mut owner)
        .await
        .unwrap_err();
    assert_eq!(error.failure.code(), "process.resource.quiescence");
    assert!(failure_message(&error).contains("injected probe failure"));
    let started = tokio::time::Instant::now();
    assert!(
        !await_tree_quiescence_after_root_reap(EffectId(91), &mut owner)
            .await
            .unwrap()
    );
    assert!(started.elapsed() >= PROCESS_TREE_QUIESCENCE_GRACE);
    owner.finish(EffectId(91)).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn unattached_root_cleanup_keeps_termination_errors_and_bounds_reaping() {
    let cases = [
        (
            vec![],
            Some(std::io::Error::other("kill failed")),
            Some(std::io::ErrorKind::Other),
            0,
        ),
        (vec![WaitStep::Ready(Ok(()))], None, None, 1),
        (
            vec![WaitStep::Ready(Err(std::io::Error::other("wait failed")))],
            None,
            Some(std::io::ErrorKind::Other),
            1,
        ),
        (
            vec![WaitStep::Pending],
            None,
            Some(std::io::ErrorKind::TimedOut),
            1,
        ),
    ];
    for (steps, terminate_error, expected, expected_waits) in cases {
        let mut root = ScriptedRoot {
            waits: steps.into(),
            kill: Ok(()),
            wait_count: 0,
            kill_count: 0,
        };
        let result = reap_unattached_root(&mut root, terminate_error).await;
        assert_eq!(result.as_ref().map(std::io::Error::kind), expected);
        assert_eq!(root.wait_count, expected_waits);
        assert_eq!(root.kill_count, 0);
        assert!(root.waits.is_empty());
    }
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
    assert_eq!(error.failure.code(), "process.resource.attach");
    assert!(failure_message(&error).contains("injected portable attachment failure"));
    let report = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    assert!(report.all_reaped);
    assert!(report.output_drains_joined);
    assert_eq!(report.secondary_errors, Vec::<String>::new());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
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
    assert!(report.all_reaped);
    assert!(report.output_drains_joined);
    assert_eq!(report.secondary_errors.len(), 1);
    assert!(report.secondary_errors[0].contains("injected portable close failure"));
    let retried = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    assert!(retried.all_reaped);
    assert!(retried.output_drains_joined);
    assert_eq!(retried.secondary_errors, Vec::<String>::new());
}

#[test]
fn supervisor_cleanup_preserves_primary_failure_and_never_invents_reap_evidence() {
    let primary = EffectFailed::other(EffectId(92), "process.wait", "primary");
    let secondary = EffectFailed::other(EffectId(92), "process.resource.join", "cleanup");
    for process_failed in [false, true] {
        for cleanup_failed in [false, true] {
            for initially_reaped in [false, true] {
                let mut result = if process_failed {
                    Err(primary.clone())
                } else {
                    Ok(ProcessTermination::Exit(0))
                };
                let mut reaped = initially_reaped;
                apply_supervisor_cleanup(
                    &mut result,
                    &mut reaped,
                    if cleanup_failed {
                        Err(secondary.clone())
                    } else {
                        Ok(())
                    },
                );
                assert_eq!(reaped, initially_reaped && !cleanup_failed);
                let expected = if process_failed {
                    Err(EffectFailed::other(
                        EffectId(92),
                        "process.wait",
                        if cleanup_failed {
                            "primary; supervisor destruction failed: cleanup"
                        } else {
                            "primary"
                        },
                    ))
                } else if cleanup_failed {
                    Err(secondary.clone())
                } else {
                    Ok(ProcessTermination::Exit(0))
                };
                assert_eq!(result, expected);
            }
        }
    }
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
    assert_eq!(failure.failure.code(), "process.resource.terminate");
    let message = failure_message(&failure);
    let direct_kill = message
        .find("direct root kill failed: kill failure")
        .unwrap();
    let retry = message
        .find("supervisor termination retry failed:")
        .unwrap();
    let wait = message.find("root reap failed: wait failure").unwrap();
    assert!(direct_kill < retry && retry < wait);
    assert_eq!(root.kill_count, 1);
    assert_eq!(root.wait_count, 1);
    owner.finish(EffectId(93)).await.unwrap();
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
    assert_eq!(root.kill_count, 0);
    assert_eq!(root.wait_count, 1);
    owner.finish(EffectId(94)).await.unwrap();
}

#[test]
fn attachment_failure_retains_each_cleanup_error_and_requires_both_completion_proofs() {
    let attachment = ResourceError::io(
        "attach supervisor",
        std::io::Error::other("attachment failure"),
    );
    for root_failed in [false, true] {
        for supervisor_failed in [false, true] {
            let root_cleanup = root_failed.then(|| std::io::Error::other("root cleanup failure"));
            let supervisor = if supervisor_failed {
                Err(EffectFailed::other(
                    EffectId(95),
                    "process.resource.join",
                    "supervisor cleanup failure",
                ))
            } else {
                Ok(())
            };
            let (failure, reaped) = failed_attachment_outcome(
                EffectId(95),
                &attachment,
                root_cleanup.as_ref(),
                supervisor,
            );
            assert_eq!(failure.failure.code(), "process.resource.attach");
            assert_eq!(reaped, !root_failed && !supervisor_failed);
            let message = failure_message(&failure);
            assert!(message.contains("attachment failure"));
            assert_eq!(
                message.contains("spawned-child cleanup failed: root cleanup failure"),
                root_failed
            );
            assert_eq!(
                message.contains("supervisor destruction failed: supervisor cleanup failure"),
                supervisor_failed
            );
        }
    }
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
    assert_eq!(error.failure.code(), "process.cancelled.before_spawn");
    assert!(!output.exists());
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
    assert!(stderr_abort.is_finished());
    assert!(outcome.quiescent);
    assert_eq!(
        outcome.result.unwrap_err().failure.code(),
        "process.stdout.read"
    );
}

#[cfg(unix)]
#[test]
fn signal_exit_status_uses_the_conventional_signal_number() {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        exit_termination(std::process::ExitStatus::from_raw(0)),
        ProcessTermination::Exit(0)
    );
    assert_eq!(
        exit_termination(std::process::ExitStatus::from_raw(libc::SIGTERM)),
        ProcessTermination::Exit(128 + libc::SIGTERM)
    );
}

#[tokio::test(start_paused = true)]
async fn pipe_drain_deadline_aborts_and_joins_the_owned_task() {
    let mut pipe = tokio::spawn(std::future::pending::<std::io::Result<()>>());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    let error = await_pipe_until(EffectId(99), "stdout", &mut pipe, deadline)
        .await
        .unwrap_err();
    assert_eq!(error.failure.code(), "process.output.close.timeout");
    assert!(pipe.is_finished());
    assert_eq!(tokio::time::Instant::now(), deadline);
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
    assert_eq!(error.failure.code(), "process.resource.terminate");
    assert!(!failure_message(&error).contains("supervisor termination retry failed"));
    assert!(!reaped);
    owner.finish(EffectId(100)).await.unwrap();
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
    assert_eq!(error.failure.code(), "process.resource.attach");
    assert!(failure_message(&error).contains("spawned-child cleanup failed"));
    let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    assert!(!drain.all_reaped);
    assert!(drain.output_drains_joined);
    assert_eq!(drain.secondary_errors, Vec::<String>::new());
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .next()
            .is_none()
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case::released_collector(true)]
#[case::stalled_collector(false)]
#[tokio::test(flavor = "current_thread")]
async fn collector_completion_is_required_for_the_handlers_output_drain_proof(
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
    if release {
        gate.release.notify_one();
    }
    let result = running.await.unwrap();
    if release {
        assert_eq!(result.unwrap().termination, ProcessTermination::Exit(0));
    } else {
        assert_eq!(
            result.unwrap_err().failure.code(),
            "process.output.close.timeout"
        );
    }
    let drain = handler.drain_for_shutdown(Duration::from_secs(1)).await;
    assert!(drain.all_reaped);
    assert_eq!(drain.output_drains_joined, release);
    assert_eq!(drain.secondary_errors, Vec::<String>::new());
}
