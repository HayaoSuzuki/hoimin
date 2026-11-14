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

    let actual_0 = parse_corpus(&input);

    assert!(actual_0.unwrap_err().contains(message));
}

#[test]
fn corpus_requires_exact_count() {
    let actual_0 = parse_corpus("");

    assert!(actual_0.unwrap_err().contains("expected 12 cases, found 0"));
}

#[test]
fn corpus_requires_valid_json() {
    let actual_0 = parse_corpus("not JSON");

    assert!(actual_0.unwrap_err().starts_with("line 1:"));
}

#[test]
fn strict_projection_rejects_exit_code_outside_i32() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.expected.exit_code = Some(u64::MAX);

    let actual_0 = project_strict(&case);

    assert!(actual_0.unwrap_err().contains("not observable as i32"));
}

#[cfg(unix)]
#[test]
fn report_projection_rejects_invalid_json() {
    let actual_0 = observe_report(b"not JSON");

    assert!(actual_0.unwrap_err().contains("invalid JSON report"));
}

#[cfg(unix)]
#[test]
fn report_projection_requires_boolean_completion() {
    let document = b"{\"summary\":{\"complete\":\"invalid\"}}";

    let actual_0 = observe_report(document);

    assert!(actual_0.unwrap_err().contains("boolean summary.complete"));
}

#[cfg(unix)]
#[test]
fn session_projection_rejects_directory() {
    let root = tempfile::tempdir().unwrap();

    let actual_0 = observe_session(root.path());

    assert!(actual_0.is_err());
}

#[cfg(unix)]
#[test]
fn session_projection_rejects_missing_file() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("session.sqlite3");

    let actual_0 = observe_session(&database);

    assert!(actual_0.is_err());
}

#[cfg(unix)]
#[test]
fn session_projection_rejects_invalid_completion() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("session.sqlite3");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch("CREATE TABLE runs (complete INTEGER); INSERT INTO runs VALUES (2)")
        .unwrap();

    let actual_0 = observe_session(&database);

    assert_eq!(actual_0.unwrap_err(), "invalid session complete value: 2");
}

#[cfg(unix)]
#[test]
fn session_projection_observes_incomplete() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("session.sqlite3");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch("CREATE TABLE runs (complete INTEGER); INSERT INTO runs VALUES (2)")
        .unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();

    let actual_0 = observe_session(&database);

    assert_eq!(actual_0.unwrap(), "incomplete");
}

#[cfg(unix)]
#[test]
fn project_writer_reports_unusable_root() {
    let file = tempfile::NamedTempFile::new().unwrap();

    let actual_0 = write_parallel_project(file.path());

    assert!(actual_0.is_err());
}

#[cfg(unix)]
#[test]
fn liveness_rejects_invalid_pid() {
    let actual_0 = process_is_alive(u32::MAX);

    assert!(!actual_0);
}

#[cfg(unix)]
#[tokio::test]
async fn interrupt_rejects_exited_process() {
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    child.wait().await.unwrap();

    let actual_0 = send_first_interrupt(&mut child);

    assert_eq!(actual_0.unwrap_err(), "hoimin exited before SIGINT");
}

#[cfg(unix)]
#[tokio::test]
async fn readiness_rejects_exited_process() {
    let paths = FixturePaths::create().unwrap();
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    child.wait().await.unwrap();

    let actual_0 = wait_for_readiness(&paths, &mut child).await;

    assert!(
        actual_0
            .unwrap_err()
            .contains("exited before fixture readiness")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn reaping_tolerates_invalid_pid() {
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
    let result = wait_for_readiness(&paths, &mut child).await;

    child.kill().await.unwrap();

    assert_eq!(
        result.unwrap_err(),
        "fixture processes did not become ready"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn case_execution_reports_projection_failure() {
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
}

#[cfg(unix)]
#[tokio::test]
async fn case_execution_reports_execution_failure() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.expected.returned = true;
    let failed = run_strict_case_with_execution(&case, |_| {
        std::future::ready(Err("injected execution failure".into()))
    })
    .await;

    let actual_1 = failed.detail.as_deref();

    assert_eq!(failed.class, CaseClass::InfrastructureError);
    assert_eq!(actual_1, Some("injected execution failure"));
}

#[cfg(unix)]
#[tokio::test]
async fn case_execution_reports_panic() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.expected.returned = true;

    let panicked =
        run_strict_case_with_execution(&case, |_| async { panic!("injected case panic") }).await;

    assert_eq!(panicked.class, CaseClass::InfrastructureError);
    assert!(panicked.detail.unwrap().starts_with("case panicked:"));
}

#[cfg(unix)]
#[tokio::test]
async fn case_execution_reports_mismatch() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.expected.returned = true;
    let mut actual = project_strict(&case).unwrap();
    actual.exit_code = Some(42);

    let mismatch = run_strict_case_with_execution(&case, |_| std::future::ready(Ok(actual))).await;

    assert_eq!(mismatch.class, CaseClass::Mismatch);
    assert!(mismatch.detail.unwrap().starts_with("expected="));
    assert_eq!(mismatch.actual.unwrap().exit_code, Some(42));
}

#[cfg(unix)]
#[rstest::rstest]
#[case::first(std::collections::VecDeque::from([Err(std::io::Error::other("injected tempdir failure"))]), 1)]
#[case::second(std::collections::VecDeque::from([tempfile::tempdir(), Err(std::io::Error::other("injected tempdir failure"))]), 2)]
fn fixture_creation_reports_tempdir_failure(
    #[case] mut directories: std::collections::VecDeque<std::io::Result<tempfile::TempDir>>,
    #[case] expected_calls: usize,
) {
    let mut calls = 0;

    let result = FixturePaths::with_tempdirs(&mut || {
        calls += 1;
        directories.pop_front().unwrap()
    });

    assert_eq!(result.err().unwrap(), "injected tempdir failure");
    assert_eq!(calls, expected_calls);
}

#[cfg(unix)]
#[rstest::rstest]
#[case::source(|project: &Path, _: &Path| std::fs::write(project.join("src"), "blocking file").unwrap())]
#[case::package(|project: &Path, _: &Path| std::fs::create_dir_all(project.join("src/__init__.py")).unwrap())]
#[case::module(|project: &Path, _: &Path| std::fs::create_dir_all(project.join("src/calc.py")).unwrap())]
#[case::active(|_: &Path, coordinator: &Path| std::fs::create_dir(coordinator.join("active")).unwrap())]
fn fixture_creation_reports_filesystem_failures(#[case] obstruct: fn(&Path, &Path)) {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    obstruct(project.path(), coordinator.path());
    let mut directories = std::collections::VecDeque::from([Ok(project), Ok(coordinator)]);

    let result = FixturePaths::with_tempdirs(&mut || directories.pop_front().unwrap());

    assert!(result.is_err());
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn native_pid_rejects_overflow() {
    let actual_0 = native_pid(u32::MAX);

    assert!(actual_0.is_err());
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn interrupt_reports_signal_failure() {
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let interrupt = send_first_interrupt_with_signal(&mut child, |_, _| {
        Err(std::io::Error::other("injected interrupt failure"))
    })
    .unwrap_err();

    child.kill().await.unwrap();

    assert_eq!(interrupt, "SIGINT failed: injected interrupt failure");
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn reaping_reports_signal_failure() {
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let processes = FixtureProcesses { pids: vec![pid] };
    let kill = reap_fixture_processes_with_signal(&processes, |_, _| {
        Err(std::io::Error::other("injected kill failure"))
    })
    .await
    .unwrap_err();

    child.kill().await.unwrap();

    assert_eq!(
        kill,
        format!("failed to kill fixture process {pid}: injected kill failure")
    );
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn reaping_reports_surviving_process() {
    let mut child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let processes = FixtureProcesses { pids: vec![pid] };
    let survival = reap_fixture_processes_with_signal(&processes, |_, _| Ok(()))
        .await
        .unwrap_err();

    child.kill().await.unwrap();

    assert_eq!(survival, format!("fixture process {pid} survived teardown"));
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
#[rstest::rstest]
#[case::missing(|_: &OracleCase| "missing-case".to_owned(), false, "unknown oracle case")]
#[case::selected(|case: &OracleCase| case.id.clone(), true, "test result: ok")]
fn oracle_selection_checks_identity(
    #[case] select: fn(&OracleCase) -> String,
    #[case] succeeds: bool,
    #[case] message: &str,
) {
    let first = parse_corpus(corpus_text())
        .unwrap()
        .into_iter()
        .find(|case| case.mode == "strict")
        .unwrap();
    let id = select(&first);
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
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_0 = output.status.success();

    assert_eq!(actual_0, succeeds);
    assert!(combined.contains(message), "{combined}");
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn exit_status_reports_wait_error() {
    let actual_0 = bounded_exit_status(Ok(Err(std::io::Error::other("injected wait failure"))));

    assert_eq!(actual_0.unwrap_err(), "injected wait failure");
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn exit_status_reports_elapsed_budget() {
    let elapsed = tokio::time::timeout(
        Duration::from_secs(1),
        std::future::pending::<std::io::Result<std::process::ExitStatus>>(),
    )
    .await;

    let actual_0 = bounded_exit_status(elapsed);

    assert_eq!(actual_0.unwrap_err(), "hoimin exceeded bounded exit");
}

#[cfg(unix)]
#[rstest::rstest]
#[case::read_stdout(false, || Err(std::io::Error::other("injected output read failure")), "injected output read failure")]
#[case::read_stderr(true, || Err(std::io::Error::other("injected output read failure")), "injected output read failure")]
#[case::panic_stdout(false, || panic!("injected output task panic"), "injected output task panic")]
#[case::panic_stderr(true, || panic!("injected output task panic"), "injected output task panic")]
#[tokio::test]
async fn output_collection_reports_read_and_join_errors(
    #[case] fail_stderr: bool,
    #[case] fail: fn() -> std::io::Result<Vec<u8>>,
    #[case] expected: &str,
) {
    let failed = tokio::spawn(async move { fail() });
    let succeeded = tokio::spawn(async { Ok(Vec::new()) });
    let mut readers = [failed, succeeded];
    readers.swap(0, usize::from(fail_stderr));
    let [stdout, stderr] = readers;

    let result = collect_output(stdout, stderr).await;

    assert!(result.unwrap_err().contains(expected));
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
async fn readiness_reports_external_reap() {
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id().unwrap()).unwrap();
    let mut status = 0;
    // SAFETY: this exact retained child belongs to this fixture; synchronous reap simulates an external supervisor.
    let reaped = unsafe { libc::waitpid(pid, &raw mut status, 0) };
    let mut child = child;

    let result = wait_for_readiness(&paths, &mut child).await;

    assert_eq!(reaped, pid);
    assert!(result.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn interrupt_reports_external_reap() {
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id().unwrap()).unwrap();
    let mut status = 0;
    // SAFETY: this exact retained child belongs to this fixture; synchronous reap simulates an external supervisor.
    let reaped = unsafe { libc::waitpid(pid, &raw mut status, 0) };
    let mut child = child;

    let result = send_first_interrupt(&mut child);

    assert_eq!(reaped, pid);
    assert!(result.unwrap_err().starts_with("SIGINT failed:"));
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
#[rstest::rstest]
#[case::stdout(Stdio::null, "missing stdout pipe")]
#[case::stderr(Stdio::piped, "missing stderr pipe")]
#[tokio::test(start_paused = true)]
async fn fixture_execution_reports_missing_pipes(
    #[case] stdout: fn() -> Stdio,
    #[case] expected: &str,
) {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "pass"])
        .stdout(stdout())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let result = execute_fixture(case, paths, child).await;

    assert_eq!(result.unwrap_err(), expected);
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn fixture_execution_reports_bounded_exit() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "import time; time.sleep(60)"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let actual_0 = execute_fixture(case, paths, child).await;

    assert!(
        actual_0
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
#[rstest::rstest]
#[case::report("not JSON", "invalid JSON report")]
#[case::session(r#"{"summary":{"complete":true}}"#, "no such table: runs")]
#[tokio::test]
async fn fixture_execution_validates_report_and_session_surfaces(
    #[case] output: &str,
    #[case] expected: &str,
) {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let paths = FixturePaths::create().unwrap();
    let child = tokio::process::Command::new(python_executable())
        .args(["-c", "import sys; print(sys.argv[1])", output])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let result = execute_fixture(case, paths, child).await;

    assert!(result.unwrap_err().contains(expected));
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
    let reaped = unsafe { libc::waitpid(pid, &raw mut status, 0) };

    let actual_0 = execute_fixture(case, paths, child).await;

    assert!(actual_0.is_err());
    assert_eq!(reaped, pid);
}
