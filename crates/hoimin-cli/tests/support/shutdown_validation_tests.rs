use super::*;
use serde_json::json;

#[rstest::rstest]
#[case("/schedule", json!([]), "empty schedule")]
#[case("/expected/dispatches/0", json!(2), "singleton dispatch")]
#[case("/expected/session", json!("pending"), "session completion flag contradicts")]
#[case("/expected/report", json!("pending"), "report completion flag contradicts")]
#[case("/expected/output", json!("unknown"), "component state")]
#[case("/expected/cause", json!("unknown"), "stop cause")]
fn rejects_invalid_shutdown_contract(
    #[case] pointer: &str,
    #[case] value: serde_json::Value,
    #[case] message: &str,
) {
    let input = corpus_with_first_case_changed(|case| *case.pointer_mut(pointer).unwrap() = value);
    assert!(parse_corpus(&input).unwrap_err().contains(message));
}

#[test]
fn corpus_requires_exact_count_and_valid_json() {
    assert!(
        parse_corpus("")
            .unwrap_err()
            .contains("expected 12 cases, found 0")
    );
    assert!(parse_corpus("not JSON").unwrap_err().starts_with("line 1:"));
}

#[test]
fn strict_projection_rejects_exit_code_outside_i32() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.expected.exit_code = Some(u64::MAX);
    assert!(
        project_strict(&case)
            .unwrap_err()
            .contains("not observable as i32")
    );
}

#[cfg(unix)]
#[test]
fn report_projection_rejects_missing_summary_and_invalid_json() {
    assert!(
        observe_report(b"not JSON")
            .unwrap_err()
            .contains("invalid JSON report")
    );
    let document = b"{\"summary\":{\"complete\":\"invalid\"}}";
    assert!(
        observe_report(document)
            .unwrap_err()
            .contains("boolean summary.complete")
    );
}

#[cfg(unix)]
#[test]
fn session_projection_reports_open_schema_and_completion_errors() {
    let root = tempfile::tempdir().unwrap();
    assert!(observe_session(root.path()).is_err());
    let database = root.path().join("session.sqlite3");
    assert!(observe_session(&database).is_err());
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch("CREATE TABLE runs (complete INTEGER); INSERT INTO runs VALUES (2)")
        .unwrap();
    assert_eq!(
        observe_session(&database).unwrap_err(),
        "invalid session complete value: 2"
    );
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    assert_eq!(observe_session(&database).unwrap(), "incomplete");
}

#[cfg(unix)]
#[test]
fn project_writer_reports_unusable_root_and_process_liveness_rejects_invalid_pid() {
    let file = tempfile::NamedTempFile::new().unwrap();
    assert!(write_parallel_project(file.path()).is_err());
    assert!(!process_is_alive(u32::MAX));
}

#[cfg(unix)]
#[tokio::test]
async fn readiness_and_interrupt_reject_already_exited_process() {
    let paths = FixturePaths::create().unwrap();
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    child.wait().await.unwrap();
    assert_eq!(
        send_first_interrupt(&mut child).unwrap_err(),
        "hoimin exited before SIGINT"
    );
    assert!(
        wait_for_readiness(&paths, &mut child)
            .await
            .unwrap_err()
            .contains("exited before fixture readiness")
    );
    reap_fixture_processes(&FixtureProcesses {
        pids: vec![u32::MAX],
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn readiness_deadline_rejects_fixture_without_marker_processes() {
    let paths = FixturePaths::create().unwrap();
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    assert_eq!(
        wait_for_readiness(&paths, &mut child).await.unwrap_err(),
        "fixture processes did not become ready"
    );
    child.kill().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn case_execution_classifies_projection_failure_errors_panics_and_mismatch() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.expected.returned = false;
    let projection = run_strict_case_with_execution(&case, execute_strict_case).await;
    assert_eq!(projection.class, CaseClass::InfrastructureError);
    assert!(
        projection
            .detail
            .unwrap()
            .contains("no observable terminal exit")
    );
    assert!(projection.actual.is_none());
    case.expected.returned = true;
    let failed = run_strict_case_with_execution(&case, |_| {
        std::future::ready(Err("injected execution failure".into()))
    })
    .await;
    assert_eq!(failed.class, CaseClass::InfrastructureError);
    assert_eq!(failed.detail.as_deref(), Some("injected execution failure"));
    let panicked =
        run_strict_case_with_execution(&case, |_| async { panic!("injected case panic") }).await;
    assert_eq!(panicked.class, CaseClass::InfrastructureError);
    assert!(panicked.detail.unwrap().starts_with("case panicked:"));
    let mut actual = project_strict(&case).unwrap();
    actual.exit_code = Some(42);
    let mismatch = run_strict_case_with_execution(&case, |_| std::future::ready(Ok(actual))).await;
    assert_eq!(mismatch.class, CaseClass::Mismatch);
    assert!(mismatch.detail.unwrap().starts_with("expected="));
    assert_eq!(mismatch.actual.unwrap().exit_code, Some(42));
}

#[cfg(unix)]
#[test]
fn fixture_creation_reports_first_and_second_tempdir_failures() {
    for failed_call in [0, 1] {
        let mut calls = 0;
        let result = FixturePaths::with_tempdirs(&mut || {
            let current = calls;
            calls += 1;
            if current == failed_call {
                Err(std::io::Error::other("injected tempdir failure"))
            } else {
                tempfile::tempdir()
            }
        });
        assert_eq!(result.err().unwrap(), "injected tempdir failure");
        assert_eq!(calls, failed_call + 1);
    }
}

#[cfg(unix)]
#[rstest::rstest]
#[case("source")]
#[case("package")]
#[case("module")]
#[case("active")]
fn fixture_creation_reports_filesystem_failures(#[case] stage: &str) {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    if stage == "active" {
        std::fs::create_dir(coordinator.path().join("active")).unwrap();
    } else if stage == "source" {
        std::fs::write(project.path().join("src"), "blocking file").unwrap();
    } else {
        std::fs::create_dir(project.path().join("src")).unwrap();
        std::fs::create_dir(project.path().join("src").join(if stage == "package" {
            "__init__.py"
        } else {
            "calc.py"
        }))
        .unwrap();
    }
    let mut directories = std::collections::VecDeque::from([Ok(project), Ok(coordinator)]);
    assert!(FixturePaths::with_tempdirs(&mut || directories.pop_front().unwrap()).is_err());
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn signals_and_reaping_report_failures_and_surviving_processes() {
    assert!(native_pid(u32::MAX).is_err());
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let interrupt = send_first_interrupt_with_signal(&mut child, |_, _| {
        Err(std::io::Error::other("injected interrupt failure"))
    })
    .unwrap_err();
    assert_eq!(interrupt, "SIGINT failed: injected interrupt failure");
    let processes = FixtureProcesses { pids: vec![pid] };
    let kill = reap_fixture_processes_with_signal(&processes, |_, _| {
        Err(std::io::Error::other("injected kill failure"))
    })
    .await
    .unwrap_err();
    assert_eq!(
        kill,
        format!("failed to kill fixture process {pid}: injected kill failure")
    );
    let survival = reap_fixture_processes_with_signal(&processes, |_, _| Ok(()))
        .await
        .unwrap_err();
    assert_eq!(survival, format!("fixture process {pid} survived teardown"));
    child.kill().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn successful_reaping_waits_for_running_fixture_to_exit() {
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let processes = FixtureProcesses {
        pids: vec![child.id().unwrap()],
    };
    let wait = tokio::spawn(async move { child.wait().await });
    reap_fixture_processes(&processes).await.unwrap();
    assert!(!wait.await.unwrap().unwrap().success());
}

#[cfg(unix)]
#[test]
fn oracle_selection_rejects_unknown_cases_and_runs_selected_strict_case() {
    let first = parse_corpus(corpus_text())
        .unwrap()
        .into_iter()
        .find(|case| case.mode == "strict")
        .unwrap();
    for (id, succeeds, message) in [
        ("missing-case", false, "unknown oracle case"),
        (first.id.as_str(), true, "test result: ok"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "oracle_correspondence",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("HOIMIN_SHUTDOWN_ORACLE_CASE", id)
            .output()
            .unwrap();
        assert_eq!(output.status.success(), succeeds);
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(combined.contains(message), "{combined}");
    }
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn exit_status_projection_distinguishes_io_error_and_elapsed_budget() {
    assert_eq!(
        bounded_exit_status(Ok(Err(std::io::Error::other("injected wait failure")))).unwrap_err(),
        "injected wait failure"
    );
    let elapsed = tokio::time::timeout(
        Duration::from_secs(1),
        std::future::pending::<std::io::Result<std::process::ExitStatus>>(),
    )
    .await;
    assert_eq!(
        bounded_exit_status(elapsed).unwrap_err(),
        "hoimin exceeded bounded exit"
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case(false, false)]
#[case(false, true)]
#[case(true, false)]
#[case(true, true)]
#[tokio::test]
async fn output_collection_reports_read_and_join_errors(
    #[case] fail_stderr: bool,
    #[case] panic: bool,
) {
    let failed: tokio::task::JoinHandle<std::io::Result<Vec<u8>>> = tokio::spawn(async move {
        assert!(!panic, "injected output task panic");
        Err(std::io::Error::other("injected output read failure"))
    });
    let succeeded = tokio::spawn(async { Ok(Vec::new()) });
    let (stdout, stderr) = if fail_stderr {
        (succeeded, failed)
    } else {
        (failed, succeeded)
    };
    let message = collect_output(stdout, stderr).await.unwrap_err();
    assert!(message.contains(if panic {
        "injected output task panic"
    } else {
        "injected output read failure"
    }));
}

#[cfg(unix)]
#[tokio::test]
async fn output_reader_returns_async_read_errors() {
    struct FailedReader;
    impl tokio::io::AsyncRead for FailedReader {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
            _buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::other("injected reader failure")))
        }
    }
    assert_eq!(
        read_output(&mut FailedReader)
            .await
            .unwrap_err()
            .to_string(),
        "injected reader failure"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn readiness_reports_a_child_reaped_outside_tokio() {
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id().unwrap()).unwrap();
    let mut status = 0;
    // SAFETY: this exact retained child belongs to this fixture; synchronous reap simulates an external supervisor.
    assert_eq!(unsafe { libc::waitpid(pid, &raw mut status, 0) }, pid);
    let mut child = child;
    assert!(wait_for_readiness(&paths, &mut child).await.is_err());
    assert!(
        send_first_interrupt(&mut child)
            .unwrap_err()
            .starts_with("SIGINT failed:")
    );
}

#[cfg(unix)]
#[rstest::rstest]
#[case(
    Some("injected infrastructure failure"),
    "injected infrastructure failure"
)]
#[case(None, "unknown")]
fn correspondence_assertion_reports_infrastructure_details(
    #[case] detail: Option<&str>,
    #[case] message: &str,
) {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let result = CaseResult {
        id: "infrastructure".into(),
        class: CaseClass::InfrastructureError,
        expected: project_strict(&case).unwrap(),
        actual: None,
        detail: detail.map(str::to_owned),
    };
    let panic = std::panic::catch_unwind(|| assert_correspondence_results(&[result])).unwrap_err();
    let panic = panic.downcast::<String>().unwrap();
    assert!(panic.contains("infrastructure errors:"));
    assert!(panic.contains(message));
}

#[cfg(unix)]
#[test]
fn correspondence_assertion_reports_mismatch_expected_and_actual() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = project_strict(&case).unwrap();
    let mut actual = expected.clone();
    actual.exit_code = Some(42);
    let result = CaseResult {
        id: "mismatch".into(),
        class: CaseClass::Mismatch,
        expected,
        actual: Some(actual),
        detail: None,
    };
    let panic = std::panic::catch_unwind(|| assert_correspondence_results(&[result])).unwrap_err();
    let panic = panic.downcast::<String>().unwrap();
    assert!(panic.contains("mismatch"));
    assert!(panic.contains("expected="));
    assert!(panic.contains("actual="));
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn fixture_execution_reports_missing_pipes_and_bounded_exit() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    for stdout_piped in [false, true] {
        let paths = FixturePaths::create().unwrap();
        let mut command = tokio::process::Command::new(python_executable());
        command.args(["-c", "pass"]).kill_on_drop(true);
        if stdout_piped {
            command.stdout(Stdio::piped());
        }
        let error = execute_fixture(case.clone(), paths, command.spawn().unwrap())
            .await
            .unwrap_err();
        assert_eq!(
            error,
            if stdout_piped {
                "missing stderr pipe"
            } else {
                "missing stdout pipe"
            }
        );
    }
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    assert!(
        execute_fixture(case, paths, child)
            .await
            .unwrap_err()
            .starts_with("hoimin exceeded bounded exit; stderr=")
    );
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn fixture_execution_reports_kill_failure() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let error = execute_fixture_with_kill(case, paths, child, |_| {
        Err(std::io::Error::other("injected child kill failure"))
    })
    .await
    .unwrap_err();
    assert_eq!(error, "injected child kill failure");
}

#[cfg(unix)]
#[tokio::test]
async fn fixture_execution_validates_report_and_session_surfaces() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    for output in ["not JSON", r#"{"summary":{"complete":true}}"#] {
        let paths = FixturePaths::create().unwrap();
        let child = tokio::process::Command::new(python_executable())
            .args(["-c", "import sys; print(sys.argv[1])", output])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let error = execute_fixture(case.clone(), paths, child)
            .await
            .unwrap_err();
        assert!(error.contains(if output == "not JSON" {
            "invalid JSON report"
        } else {
            "no such table: runs"
        }));
    }
}

#[cfg(unix)]
#[test]
fn spawning_reports_missing_binary() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let paths = FixturePaths::create().unwrap();
    let error =
        spawn_scenario_with_executable(&case, &paths, Path::new("/absent/hoimin-test-executable"))
            .err()
            .unwrap();
    assert!(error.starts_with("spawn hoimin:"));
}

#[cfg(unix)]
#[tokio::test]
async fn fixture_execution_reports_external_reap_during_teardown() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = native_pid(child.id().unwrap()).unwrap();
    let mut status = 0;
    // SAFETY: this exact retained child is owned by the fixture; emulate another supervisor reaping it.
    assert_eq!(unsafe { libc::waitpid(pid, &raw mut status, 0) }, pid);
    assert!(execute_fixture(case, paths, child).await.is_err());
}
