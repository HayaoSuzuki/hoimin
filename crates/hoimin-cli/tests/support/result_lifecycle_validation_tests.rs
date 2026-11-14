use super::*;
use serde_json::json;

#[rstest::rstest]
#[case("/schema", json!(2), "unsupported schema")]
#[case("/mode", json!("unknown"), "unknown mode")]
#[case("/id", json!(""), "empty identity or schedule")]
#[case("/scenario", json!("unknown"), "empty identity or schedule")]
#[case("/schedule", json!([]), "empty identity or schedule")]
#[case("/setup/metrics", json!(false), "does not exercise metrics")]
#[case("/setup/discovered", json!(["m9"]), "unknown role or status")]
#[case("/setup/seeded_durable", json!([{"mutant":"m9","status":"killed","executed":true}]), "unknown role or status")]
#[case("/expected/accepted/0/status", json!("unknown"), "unknown role or status")]
#[case("/expected/summary", json!(["unknown"]), "unknown role or status")]
#[case("/schedule/0", json!("unknown"), "unknown event")]
#[case("/expected/diagnostics", json!(["unknown"]), "unknown diagnostic")]
#[case("/expected/metrics_observed", json!(false), "inconsistent metrics observability")]
#[case("/expected/summary_counts", json!({}), "inconsistent Lean summary projections")]
#[case("/expected/session_finished", json!(true), "persists state without a session")]
fn rejects_invalid_case_contract(
    #[case] pointer: &str,
    #[case] value: serde_json::Value,
    #[case] message: &str,
) {
    let mut case: serde_json::Value =
        serde_json::from_str(corpus_text().lines().next().unwrap()).unwrap();
    *case.pointer_mut(pointer).unwrap() = value;
    let case: OracleCase = serde_json::from_value(case).unwrap();
    assert!(validate_case(&case).unwrap_err().contains(message));
}

#[rstest::rstest]
#[case("")]
#[case("\n\n")]
#[case("not JSON")]
fn rejects_empty_or_malformed_corpus(#[case] input: &str) {
    let actual_0 = parse_corpus(input);

    assert!(actual_0.is_err());
}

#[test]
fn empty_corpus_lines_are_ignored_between_valid_rows() {
    let first = corpus_text().lines().next().unwrap();

    let actual_0 = parse_corpus(&format!("\n{first}\n\n"));

    assert_eq!(actual_0.unwrap().len(), 1);
}

#[rstest::rstest]
#[case("accept:m0:unknown")]
#[case("accept:m9:killed")]
#[case("accept:m0:killed:extra")]
#[case("finish_session:unknown")]
#[case("discover:m9")]
fn rejects_invalid_event_shapes(#[case] event: &str) {
    let actual_0 = known_event(event);

    assert!(!actual_0);
}

fn valid_summary() -> serde_json::Value {
    json!({"summary":{"counts":{"killed":1,"survived":0,"timeout":0,"out_of_memory":0,"process_limit":0,"error":0,"not_run":0,"inconclusive":0,"score":1.0}}})
}

#[test]
fn summary_requires_object() {
    let actual_0 = report_summary_counts(&json!({}));

    assert_eq!(actual_0.unwrap_err(), "summary counts is not an object");
}

#[test]
fn summary_requires_count_keys() {
    let actual_0 = report_summary_counts(&json!({"summary":{"counts":{}}}));

    assert!(actual_0.unwrap_err().contains("missing killed"));
}

#[test]
fn summary_requires_numeric_score() {
    let mut summary = valid_summary();
    summary["summary"]["counts"]["score"] = json!("invalid");

    let actual_0 = report_summary_counts(&summary);

    assert_eq!(
        actual_0.unwrap_err(),
        "summary score is neither numeric nor null"
    );
}

#[rstest::rstest]
#[case(&json!({}), "not an array")]
#[case(&json!({"mutants":[{}]}), "no symbol")]
#[case(&json!({"mutants":[{"candidate":{"symbol":"other"}}]}), "unsupported candidate symbol")]
#[case(&json!({"mutants":[{"candidate":{"symbol":"first"}}]}), "no candidate id")]
fn report_rejects_invalid_mutant_records(
    #[case] document: &serde_json::Value,
    #[case] message: &str,
) {
    assert!(
        report_results(document, &BTreeSet::new())
            .unwrap_err()
            .contains(message)
    );
}

#[rstest::rstest]
#[case::case_0("second", "id")]
#[case::case_1("first", "other")]
fn report_rejects_duplicate_ids_and_duplicate_roles(#[case] symbol: &str, #[case] id: &str) {
    let outcome = serde_json::to_value(mutation_outcome::executed(
        hoimin_core::MutationStatus::Killed,
    ))
    .unwrap();
    let document = json!({"mutants":[{"candidate":{"symbol":"first","id":"id"},"outcome":outcome},{"candidate":{"symbol":symbol,"id":id},"outcome":outcome}]});

    let actual_0 = report_results(&document, &BTreeSet::new());

    assert!(actual_0.unwrap_err().contains("repeats semantic role"));
}

#[rstest::rstest]
#[case::case_0(vec![255], vec![], "stdout is not UTF-8")]
#[case::case_1(vec![], vec![255], "stderr is not UTF-8")]
#[case::case_2(
            b"not JSON".to_vec(),
            b"detail".to_vec(),
            "invalid report JSON",
        )]
#[cfg(unix)]
fn fixture_output_rejects_invalid_payload(
    #[case] stdout: Vec<u8>,
    #[case] stderr: Vec<u8>,
    #[case] message: &str,
) {
    use std::os::unix::process::ExitStatusExt;

    let error = fixture_run_from_output(Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout,
        stderr,
    })
    .err()
    .unwrap();

    assert!(error.contains(message));
}

#[cfg(unix)]
#[test]
fn fixture_output_rejects_signal_exit() {
    use std::os::unix::process::ExitStatusExt;

    let error = fixture_run_from_output(Output {
        status: std::process::ExitStatus::from_raw(libc::SIGTERM),
        stdout: vec![],
        stderr: vec![],
    })
    .err()
    .unwrap();

    assert!(error.contains("terminated without an exit code"));
}

#[test]
fn output_cleanup_reports_marker_directory() {
    let fixture = Fixture::new(false).unwrap();
    std::fs::create_dir(&fixture.marker).unwrap();

    let actual_0 = fixture.clear_outputs();

    assert!(actual_0.unwrap_err().contains("remove"));
}

#[test]
fn marker_read_reports_directory() {
    let fixture = Fixture::new(false).unwrap();
    std::fs::create_dir(&fixture.marker).unwrap();

    let actual_0 = marker_roles(&fixture.marker);

    assert!(actual_0.unwrap_err().contains("read execution marker"));
}

#[test]
fn marker_read_tolerates_missing_file() {
    let fixture = Fixture::new(false).unwrap();

    let actual_0 = marker_roles(&fixture.marker);

    assert_eq!(actual_0.unwrap(), Vec::<String>::new());
}

#[test]
fn output_cleanup_removes_existing_outputs() {
    let fixture = Fixture::new(false).unwrap();
    std::fs::write(&fixture.marker, "m0\n").unwrap();
    std::fs::write(&fixture.metrics, "{}").unwrap();
    fixture.clear_outputs().unwrap();

    let actual_0 = fixture.marker.exists();
    let actual_1 = fixture.metrics.exists();

    assert!(!actual_0);
    assert!(!actual_1);
}

#[rstest::rstest]
#[case::missing_table(|_: &Connection| {}, "prepare result query")]
#[case::row_type(|connection: &Connection| { connection.execute_batch("CREATE TABLE results (mutant_id TEXT, outcome_json, run_id TEXT); INSERT INTO results VALUES ('unknown', NULL, 'run')").unwrap(); }, "read result")]
#[case::invalid_outcome(|connection: &Connection| { connection.execute_batch("CREATE TABLE results (mutant_id TEXT, outcome_json, run_id TEXT); INSERT INTO results VALUES ('unknown', 'invalid', 'run')").unwrap(); }, "decode durable mutation outcome")]
#[case::unreported_id(|connection: &Connection| {
    connection.execute_batch("CREATE TABLE results (mutant_id TEXT, outcome_json, run_id TEXT)").unwrap();
    let outcome = serde_json::to_string(&mutation_outcome::executed(hoimin_core::MutationStatus::Killed)).unwrap();
    connection.execute("INSERT INTO results VALUES ('unknown', ?1, 'run')", [outcome]).unwrap();
}, "absent from report")]
fn session_projection_rejects_invalid_durable_records(
    #[case] damage: fn(&Connection),
    #[case] message: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE runs (id INTEGER, run_id TEXT, finished INTEGER, complete INTEGER); INSERT INTO runs VALUES (1, 'run', 0, 0);").unwrap();
    damage(&connection);
    drop(connection);

    let result = session_observation(&path, &BTreeMap::new(), &BTreeSet::new());

    assert!(result.unwrap_err().contains(message));
}

#[test]
fn session_projection_reports_open_failure() {
    let directory = tempfile::tempdir().unwrap();

    let actual_0 = session_observation(directory.path(), &BTreeMap::new(), &BTreeSet::new());

    assert!(actual_0.unwrap_err().contains("open session"));
}

#[test]
fn session_projection_reports_missing_runs() {
    let directory = tempfile::tempdir().unwrap();

    let actual_0 = session_observation(
        &directory.path().join("empty.sqlite3"),
        &BTreeMap::new(),
        &BTreeSet::new(),
    );

    assert!(actual_0.unwrap_err().contains("read latest run"));
}

#[test]
fn observation_reports_metrics_read_failure() {
    let fixture = Fixture::new(false).unwrap();
    let mut run = FixtureRun {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        document: valid_summary(),
    };
    run.document["mutants"] = json!([]);
    std::fs::write(&fixture.metrics, [255]).unwrap();

    let actual_0 = observe_run(&fixture, &run, false, &fixture.metrics);

    assert!(actual_0.unwrap_err().contains("read metrics"));
}

#[test]
fn observation_reports_metrics_parse_failure() {
    let fixture = Fixture::new(false).unwrap();
    let mut run = FixtureRun {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        document: valid_summary(),
    };
    run.document["mutants"] = json!([]);
    std::fs::write(&fixture.metrics, "invalid").unwrap();

    let actual_0 = observe_run(&fixture, &run, false, &fixture.metrics);

    assert!(actual_0.unwrap_err().contains("parse metrics"));
}

#[test]
fn observation_requires_boolean_summary_completion() {
    let fixture = Fixture::new(false).unwrap();
    let mut run = FixtureRun {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        document: valid_summary(),
    };
    run.document["mutants"] = json!([]);

    let actual_0 = observe_run(&fixture, &run, false, &fixture.metrics);

    assert!(
        actual_0
            .unwrap_err()
            .contains("summary complete is not a bool")
    );
}

#[tokio::test(start_paused = true)]
async fn output_join_reports_reader_failure() {
    let io_error = tokio::spawn(async { Err(std::io::Error::other("injected reader failure")) });

    let actual_0 = join_output(io_error, "stdout").await;

    assert!(actual_0.unwrap_err().contains("read hoimin CLI stdout"));
}

#[tokio::test(start_paused = true)]
async fn output_join_reports_task_panic() {
    let panic: tokio::task::JoinHandle<std::io::Result<Vec<u8>>> = tokio::spawn(async {
        panic!("injected output task panic");
    });

    let actual_0 = join_output(panic, "stderr").await;

    assert!(actual_0.unwrap_err().contains("join hoimin CLI stderr"));
}

#[tokio::test(start_paused = true)]
async fn output_join_reports_drain_timeout() {
    let pending = tokio::spawn(std::future::pending());

    let actual_0 = join_output(pending, "stdout").await;

    assert_eq!(
        actual_0.unwrap_err(),
        "timed out draining hoimin CLI stdout"
    );
}

#[tokio::test]
async fn bounded_output_reports_missing_executable() {
    let mut absent = tokio::process::Command::new("/nonexistent/hoimin-test-executable");

    let actual_0 = bounded_cli_output(&mut absent, Duration::from_secs(1)).await;

    assert!(actual_0.unwrap_err().contains("spawn hoimin CLI"));
}

#[tokio::test]
async fn bounded_output_requires_stdout_pipe() {
    let mut unpiped = tokio::process::Command::new(python_executable().unwrap());
    unpiped.args(["-c", "pass"]).kill_on_drop(true);

    let actual_0 = bounded_cli_output(&mut unpiped, Duration::from_secs(1)).await;

    assert_eq!(actual_0.unwrap_err(), "hoimin CLI stdout was not piped");
}

#[tokio::test]
async fn bounded_output_requires_stderr_pipe() {
    let mut no_stderr = tokio::process::Command::new(python_executable().unwrap());
    no_stderr
        .args(["-c", "pass"])
        .stdout(Stdio::piped())
        .kill_on_drop(true);

    let actual_0 = bounded_cli_output(&mut no_stderr, Duration::from_secs(1)).await;

    assert_eq!(actual_0.unwrap_err(), "hoimin CLI stderr was not piped");
}

#[tokio::test]
async fn durable_premise_reports_exited_child() {
    let fixture = Fixture::new(false).unwrap();
    let mut exited = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    exited.wait().await.unwrap();

    let actual_0 =
        wait_for_first_durable_kill(&mut exited, &fixture.session, Duration::from_secs(1)).await;

    assert!(
        actual_0
            .unwrap_err()
            .contains("exited before the first durable killed result")
    );
}

#[tokio::test]
async fn cleanup_tolerates_exited_child() {
    let mut exited = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    exited.wait().await.unwrap();

    let actual_0 = cleanup_cli_process_tree(&mut exited).await;

    assert_eq!(actual_0, "child already exited");
}

#[tokio::test]
async fn durable_premise_reports_deadline() {
    let fixture = Fixture::new(false).unwrap();
    let mut sleeping = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let result = wait_for_first_durable_kill(&mut sleeping, &fixture.session, Duration::ZERO).await;

    sleeping.kill().await.unwrap();
    assert_eq!(
        result.unwrap_err(),
        "timed out waiting for the first durable killed result"
    );
}

#[test]
fn session_reopen_requires_runs() {
    let fixture = Fixture::new(false).unwrap();

    let actual_0 = reopen_incomplete(&fixture, false);

    assert!(actual_0.unwrap_err().contains("mark session incomplete"));
}

#[test]
fn session_reopen_requires_results() {
    let fixture = Fixture::new(false).unwrap();

    let actual_0 = reopen_incomplete(&fixture, true);

    assert!(actual_0.unwrap_err().contains("clear seeded results"));
}

#[test]
fn session_reopen_reports_marker_obstruction() {
    let fixture = Fixture::new(false).unwrap();
    let connection = Connection::open(&fixture.session).unwrap();
    connection.execute_batch("CREATE TABLE runs (complete INTEGER); CREATE TABLE results (id INTEGER); CREATE TABLE candidates (id INTEGER);").unwrap();
    std::fs::create_dir(fixture.session.with_extension("fail-persist")).unwrap();

    let actual_0 = reopen_incomplete(&fixture, true);

    assert!(actual_0.unwrap_err().contains("arm persistence failure"));
}

#[tokio::test]
async fn case_execution_reports_mismatch() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut observation = expected_observation(&case);
    observation.exit_code = 42;

    let mismatch =
        run_case_with_execution(&case, |_| std::future::ready(Ok((observation, true)))).await;

    assert_eq!(mismatch.class, CaseClass::Mismatch);
    assert_eq!(mismatch.actual.unwrap().exit_code, 42);
    assert!(mismatch.detail.is_none());
}

#[tokio::test]
async fn case_execution_reports_run_identity_mismatch() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let identity_mismatch =
        run_case_with_execution(&case, |_| std::future::ready(Ok((expected, false)))).await;

    let actual_1 = identity_mismatch.detail.as_deref();

    assert_eq!(identity_mismatch.class, CaseClass::Mismatch);
    assert_eq!(actual_1, Some("run IDs disagree across public surfaces"));
}

#[tokio::test]
async fn case_execution_reports_infrastructure_error() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let failed = run_case_with_execution(&case, |_| {
        std::future::ready(Err("injected execution failure".into()))
    })
    .await;

    let actual_1 = failed.detail.as_deref();

    assert_eq!(failed.class, CaseClass::InfrastructureError);
    assert_eq!(actual_1, Some("injected execution failure"));
    assert!(failed.actual.is_none());
    assert!(failed.expected.is_some());
}

#[tokio::test]
async fn case_execution_reports_panic() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);

    let panicked =
        run_case_with_execution(&case, |_| async { panic!("injected case panic") }).await;

    assert_eq!(panicked.class, CaseClass::InfrastructureError);
    assert!(panicked.detail.unwrap().starts_with("case task panicked:"));
}

#[test]
fn fixture_creation_reports_tempdir_failure() {
    let error = Fixture::from_tempdir(
        Err(std::io::Error::other(
            "injected temporary directory failure",
        )),
        false,
    )
    .err()
    .unwrap();

    assert_eq!(
        error,
        "create temp dir: injected temporary directory failure"
    );
}

#[rstest::rstest]
#[case::source(|root: &Path| std::fs::write(root.join("project"), "blocking file").unwrap(), "create source")]
#[case::package(|root: &Path| std::fs::create_dir_all(root.join("project/src/__init__.py")).unwrap(), "write package")]
#[case::module(|root: &Path| std::fs::create_dir_all(root.join("project/src/calc.py")).unwrap(), "write source")]
fn fixture_creation_reports_each_filesystem_stage(
    #[case] obstruct: fn(&Path),
    #[case] message: &str,
) {
    let temporary = tempfile::tempdir().unwrap();
    obstruct(temporary.path());

    let result = Fixture::from_tempdir(Ok(temporary), false);

    assert!(result.err().unwrap().contains(message));
}

#[test]
fn clear_outputs_accepts_missing_files() {
    let fixture = Fixture::new(false).unwrap();

    let result = fixture.clear_outputs();

    assert!(result.is_ok());
}

#[test]
fn session_placeholder_is_idempotent() {
    let fixture = Fixture::new(false).unwrap();
    let first = enable_session(&fixture);

    let second = enable_session(&fixture);

    assert!(first.is_ok());
    assert!(second.is_ok());
}

#[test]
fn seeded_session_open_failure_has_context() {
    let mut fixture = Fixture::new(false).unwrap();

    fixture.session = fixture.root.join("absent/database.sqlite3");

    let actual_0 = reopen_incomplete(&fixture, false);

    assert!(actual_0.unwrap_err().starts_with("open seeded session:"));
}

#[test]
fn session_placeholder_creation_failure_has_context() {
    let mut fixture = Fixture::new(false).unwrap();

    fixture.session = fixture.root.join("absent/database.sqlite3");

    let actual_1 = enable_session(&fixture);

    assert!(
        actual_1
            .unwrap_err()
            .starts_with("create session placeholder:")
    );
}

#[test]
fn controlled_interpreter_requires_an_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("absent-python");

    let actual_0 = require_python(&path);

    assert_eq!(
        actual_0.unwrap_err(),
        format!("missing controlled Python interpreter: {}", path.display())
    );
}

#[test]
fn classification_rejects_equal_mismatch() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let result = CaseResult {
        id: case.id,
        class: CaseClass::Mismatch,
        expected: Some(expected.clone()),
        actual: Some(expected),
        detail: None,
    };

    let actual_0 = classification_consistent(&result);

    assert!(!actual_0);
}

#[test]
fn classification_accepts_different_mismatch() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let mut result = CaseResult {
        id: case.id,
        class: CaseClass::Mismatch,
        expected: Some(expected.clone()),
        actual: Some(expected),
        detail: None,
    };
    result.actual.as_mut().unwrap().exit_code = 42;

    let actual_0 = classification_consistent(&result);

    assert!(actual_0);
}

#[test]
fn classification_rejects_infrastructure_without_detail() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let mut result = CaseResult {
        id: case.id,
        class: CaseClass::Mismatch,
        expected: Some(expected.clone()),
        actual: Some(expected),
        detail: None,
    };
    result.class = CaseClass::InfrastructureError;
    result.actual = None;

    let actual_0 = classification_consistent(&result);

    assert!(!actual_0);
}

#[test]
fn classification_accepts_infrastructure_with_detail() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let mut result = CaseResult {
        id: case.id,
        class: CaseClass::Mismatch,
        expected: Some(expected.clone()),
        actual: Some(expected),
        detail: None,
    };
    result.class = CaseClass::InfrastructureError;
    result.actual = None;
    result.detail = Some("injected execution error".into());

    let actual_0 = classification_consistent(&result);

    assert!(actual_0);
}

#[test]
fn classification_rejects_match_without_actual() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let mut result = CaseResult {
        id: case.id,
        class: CaseClass::Mismatch,
        expected: Some(expected.clone()),
        actual: Some(expected),
        detail: None,
    };
    result.actual = None;
    result.detail = Some("injected execution error".into());
    result.class = CaseClass::Match;

    let actual_0 = classification_consistent(&result);

    assert!(!actual_0);
}

#[test]
fn metrics_validation_error_retains_its_context() {
    let fixture = Fixture::new(false).unwrap();
    let mut run = FixtureRun {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        document: valid_summary(),
    };
    run.document["mutants"] = json!([]);
    let mut metrics = hoimin_core::RunMetrics::empty("run");
    metrics.schema_version = 9;
    std::fs::write(&fixture.metrics, serde_json::to_vec(&metrics).unwrap()).unwrap();

    let actual_0 = observe_run(&fixture, &run, false, &fixture.metrics);

    assert!(
        actual_0
            .unwrap_err()
            .contains("validate metrics: unsupported metrics schema version 9")
    );
}

#[tokio::test]
async fn durable_timeout_reports_spawn_failure() {
    let fixture = Fixture::new(false).unwrap();
    let absent = tokio::process::Command::new("/absent/hoimin-test-executable");

    let actual_0 = run_durable_timeout_command(absent, &fixture.session).await;

    assert!(actual_0.err().unwrap().starts_with("spawn hoimin CLI:"));
}

#[tokio::test]
async fn durable_timeout_requires_pipes() {
    let fixture = Fixture::new(false).unwrap();
    let mut unpiped = tokio::process::Command::new(python_executable().unwrap());
    unpiped.args(["-c", "pass"]).kill_on_drop(true);

    let actual_0 = run_durable_timeout_command(unpiped, &fixture.session).await;

    assert!(
        actual_0
            .err()
            .unwrap()
            .starts_with("hoimin CLI output was not piped; cleanup:")
    );
}

#[tokio::test]
async fn durable_timeout_preserves_failed_premise_output() {
    let fixture = Fixture::new(false).unwrap();
    let mut exits = tokio::process::Command::new(python_executable().unwrap());
    exits
        .args(["-c", "print('premise failed')"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let error = run_durable_timeout_command(exits, &fixture.session)
        .await
        .err()
        .unwrap();

    assert!(error.starts_with("premise-guarded timeout failed:"));
    assert!(error.contains("stdout: Ok("));
    assert!(error.contains(&format!("{:?}", b"premise failed\n".to_vec())));
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn cleanup_preserves_signal_failures_and_unreaped_child_diagnostic() {
    let mut child = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let diagnostics = cleanup_cli_process_tree_with_signal(&mut child, |_, _| {
        Err(std::io::Error::other("injected signal failure"))
    })
    .await;
    child.kill().await.unwrap();

    assert!(diagnostics.contains("SIGINT failed: injected signal failure"));
    assert!(diagnostics.contains("process-group SIGKILL failed: injected signal failure"));
    assert!(diagnostics.contains("child was not reaped after process-group cleanup"));
}

#[test]
fn diagnostics_recognizes_session_commit_failure_and_metrics_failure_together() {
    let actual_0 = diagnostics("session commit failed\nmetrics.write failed\n");

    assert_eq!(actual_0, ["persistence_failed", "metrics_failed"]);
}

#[tokio::test(start_paused = true)]
async fn durable_status_reports_wait_failure() {
    let error = durable_timeout_status(Ok(Err(std::io::Error::other("injected wait failure"))))
        .unwrap_err();

    assert_eq!(
        error,
        "wait for hoimin CLI total timeout: injected wait failure"
    );
}

#[tokio::test(start_paused = true)]
async fn durable_status_reports_timeout() {
    let elapsed = tokio::time::timeout(
        Duration::from_secs(1),
        std::future::pending::<std::io::Result<std::process::ExitStatus>>(),
    )
    .await;

    let actual_0 = durable_timeout_status(elapsed);

    assert_eq!(
        actual_0.unwrap_err(),
        "hoimin CLI exceeded its total timeout plus shutdown and telemetry budgets"
    );
}

#[tokio::test]
async fn output_finisher_retains_wait_failure_and_both_output_streams() {
    let mut child = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    child.wait().await.unwrap();
    let stdout = tokio::spawn(async { Ok(b"partial stdout".to_vec()) });
    let stderr = tokio::spawn(async { Ok(b"partial stderr".to_vec()) });

    let error = finish_cli_output(
        &mut child,
        Duration::from_secs(1),
        Ok(Err(std::io::Error::other("injected wait failure"))),
        stdout,
        stderr,
    )
    .await
    .unwrap_err();

    assert!(error.starts_with(
        "wait for hoimin CLI failed: injected wait failure; cleanup: child already exited;"
    ));
    assert!(error.contains("stdout: Ok("));
    assert!(error.contains("stderr: Ok("));
}

#[tokio::test]
async fn output_reader_propagates_async_read_failure() {
    struct FailedReader;

    impl tokio::io::AsyncRead for FailedReader {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
            _buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::other("injected async read failure")))
        }
    }
    assert_eq!(
        read_output(&mut FailedReader)
            .await
            .unwrap_err()
            .to_string(),
        "injected async read failure"
    );
}

#[rstest::rstest]
#[case::mode("future", |_: &[OracleCase]| String::new(), false, "unknown HOIMIN_RESULT_LIFECYCLE_MODE")]
#[case::missing("strict", |_: &[OracleCase]| "missing-case".into(), false, "unknown HOIMIN_RESULT_LIFECYCLE_CASE")]
#[case::model_strict("strict", |cases: &[OracleCase]| cases.iter().find(|case| case.mode != "strict").unwrap().id.clone(), false, "only strict cases are executable")]
#[case::model_report("report", |cases: &[OracleCase]| cases.iter().find(|case| case.mode != "strict").unwrap().id.clone(), true, "NotExecuted")]
#[case::strict("strict", |cases: &[OracleCase]| cases.iter().find(|case| case.mode == "strict").unwrap().id.clone(), true, "Match")]
fn oracle_environment_selection_is_validated_in_isolated_processes(
    #[case] mode: &str,
    #[case] select: fn(&[OracleCase]) -> String,
    #[case] succeeds: bool,
    #[case] message: &str,
) {
    let cases = parse_corpus(corpus_text()).unwrap();
    let selected = select(&cases);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "result_lifecycle_oracle_correspondence",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MODE_ENV, mode)
        .env(CASE_ENV, &selected)
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let actual_0 = output.status.success();

    assert_eq!(actual_0, succeeds, "mode={mode}; selected={selected}");
    assert!(combined.contains(message), "{combined}");
}

#[cfg(unix)]
#[tokio::test]
async fn durable_premise_reports_a_child_reaped_outside_tokio() {
    let fixture = Fixture::new(false).unwrap();
    let child = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id().unwrap()).unwrap();
    let mut status = 0;
    // SAFETY: this exact retained child belongs to this fixture; synchronous reap simulates an external supervisor.
    let reaped = unsafe { libc::waitpid(pid, &raw mut status, 0) };
    let mut child = child;
    let actual_0 =
        wait_for_first_durable_kill(&mut child, &fixture.session, Duration::from_secs(1)).await;
    let actual_1 = signal_process(pid, libc::SIGINT);
    assert!(
        actual_0
            .unwrap_err()
            .starts_with("inspect hoimin CLI while waiting for durable result:")
    );
    assert!(actual_1.is_err());

    assert_eq!(reaped, pid);
}

#[test]
fn reporter_accepts_matching_classification() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let mut failures = Vec::new();

    record_case_result(
        CaseResult {
            id: "matching".into(),
            class: CaseClass::Match,
            expected: Some(expected.clone()),
            actual: Some(expected.clone()),
            detail: None,
        },
        &mut failures,
    );

    assert_eq!(failures, Vec::<String>::new());
}

#[test]
fn reporter_appends_failures_without_discarding_previous_entries() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let mut actual = expected.clone();
    actual.exit_code = 42;
    let mismatch = CaseResult {
        id: "mismatch".into(),
        class: CaseClass::Mismatch,
        expected: Some(expected.clone()),
        actual: Some(actual),
        detail: None,
    };
    let infrastructure = CaseResult {
        id: "infrastructure".into(),
        class: CaseClass::InfrastructureError,
        expected: Some(expected),
        actual: None,
        detail: Some("injected failure".into()),
    };
    let malformed = CaseResult {
        id: "malformed".into(),
        class: CaseClass::Match,
        expected: None,
        actual: None,
        detail: None,
    };
    let mut failures = vec!["existing".to_owned()];

    record_case_result(mismatch, &mut failures);
    record_case_result(infrastructure, &mut failures);
    record_case_result(malformed, &mut failures);

    assert_eq!(
        failures,
        ["existing", "mismatch", "infrastructure", "malformed"]
    );
}

#[test]
fn reporter_records_mismatch() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);

    let expected = expected_observation(&case);

    let mut failures = Vec::new();

    let mut actual = expected.clone();

    actual.exit_code = 42;

    record_case_result(
        CaseResult {
            id: "mismatch".into(),
            class: CaseClass::Mismatch,
            expected: Some(expected.clone()),
            actual: Some(actual),
            detail: None,
        },
        &mut failures,
    );

    assert_eq!(failures, ["mismatch"]);
}

#[test]
fn reporter_records_infrastructure_error() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);

    let expected = expected_observation(&case);

    let mut failures = Vec::new();

    record_case_result(
        CaseResult {
            id: "infrastructure".into(),
            class: CaseClass::InfrastructureError,
            expected: Some(expected),
            actual: None,
            detail: Some("injected failure".into()),
        },
        &mut failures,
    );

    assert_eq!(failures, ["infrastructure"]);
}

#[test]
fn reporter_records_malformed_classification() {
    let mut failures = Vec::new();

    record_case_result(
        CaseResult {
            id: "malformed".into(),
            class: CaseClass::Match,
            expected: None,
            actual: None,
            detail: None,
        },
        &mut failures,
    );

    assert_eq!(failures, ["malformed"]);
}

#[test]
fn seed_rejects_unsuccessful_run() {
    let run = FixtureRun {
        exit_code: 7,
        stdout: "partial stdout".into(),
        stderr: "seed failed".into(),
        document: json!({}),
    };

    let actual_0 = validate_seed(&run);

    assert_eq!(
        actual_0.unwrap_err(),
        "seed run failed: exit=7 stderr=seed failed stdout=partial stdout"
    );
}

#[test]
fn seed_accepts_successful_run() {
    let mut run = FixtureRun {
        exit_code: 7,
        stdout: "partial stdout".into(),
        stderr: "seed failed".into(),
        document: json!({}),
    };
    run.exit_code = 0;

    let result = validate_seed(&run);

    assert!(result.is_ok());
}

#[test]
fn resumed_reuse_requires_no_failure_evidence() {
    let mut run = FixtureRun {
        exit_code: 7,
        stdout: "partial stdout".into(),
        stderr: "seed failed".into(),
        document: json!({}),
    };
    run.exit_code = 0;

    let result = validate_resumed_persistence_failure("resume_reuses_determinate", &run);

    assert!(result.is_ok());
}

#[test]
fn resumed_persistence_requires_failure_evidence() {
    let mut run = FixtureRun {
        exit_code: 7,
        stdout: "partial stdout".into(),
        stderr: "seed failed".into(),
        document: json!({}),
    };
    run.exit_code = 0;

    let actual_0 = validate_resumed_persistence_failure("session_persistence_failure", &run);

    assert_eq!(
        actual_0.unwrap_err(),
        "resumed run did not reach injected SQLite ABORT: seed failed"
    );
}

#[test]
fn resumed_persistence_accepts_failure_evidence() {
    let mut run = FixtureRun {
        exit_code: 7,
        stdout: "partial stdout".into(),
        stderr: "seed failed".into(),
        document: json!({}),
    };
    run.exit_code = 0;
    run.stderr = "injected save failure".into();

    let result = validate_resumed_persistence_failure("session_persistence_failure", &run);

    assert!(result.is_ok());
}

#[test]
fn metrics_failure_destination_reports_existing_path() {
    let fixture = Fixture::new(false).unwrap();
    std::fs::write(fixture.temp.path().join("metrics-target"), "occupied").unwrap();

    let actual_0 = metrics_destination(&fixture, true);

    assert!(
        actual_0
            .unwrap_err()
            .starts_with("create metrics failure target:")
    );
}

#[tokio::test]
async fn strict_execution_rejects_an_unadapted_scenario() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.scenario = "unsupported".into();

    let actual_0 = execute_strict(&case).await;

    assert_eq!(
        actual_0.unwrap_err(),
        "strict scenario unsupported has no implementation adapter"
    );
}

#[tokio::test]
async fn durable_premise_treats_malformed_result_rows_as_not_ready() {
    let fixture = Fixture::new(false).unwrap();
    let connection = Connection::open(&fixture.session).unwrap();
    connection
        .execute_batch("CREATE TABLE results (outcome_json); INSERT INTO results VALUES (NULL)")
        .unwrap();
    drop(connection);
    let mut child = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let actual_0 = wait_for_first_durable_kill(&mut child, &fixture.session, Duration::ZERO).await;
    child.kill().await.unwrap();

    assert_eq!(
        actual_0.unwrap_err(),
        "timed out waiting for the first durable killed result"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn descendant_wait_observes_live_then_reaped_child() {
    let mut child = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id().unwrap()).unwrap();
    let live = wait_for_descendant_exit(pid, Duration::ZERO).await;
    child.kill().await.unwrap();

    let gone = wait_for_descendant_exit(pid, Duration::from_secs(1)).await;

    assert!(!live);
    assert!(gone);
}

#[cfg(unix)]
#[tokio::test]
async fn descendant_wait_does_not_report_inaccessible_process_as_gone() {
    let result = wait_for_descendant_exit(1, Duration::ZERO).await;

    assert!(!result);
}

#[test]
fn result_binding_reports_sqlite_connection_length_limit() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE results (mutant_id TEXT, outcome_json TEXT, run_id TEXT)")
        .unwrap();
    let mut statement = connection
        .prepare("SELECT mutant_id, outcome_json FROM results WHERE run_id=?1 ORDER BY mutant_id")
        .unwrap();
    // SAFETY: this exclusive test connection owns the handle; only its length limit changes.
    let previous = unsafe {
        rusqlite::ffi::sqlite3_limit(connection.handle(), rusqlite::ffi::SQLITE_LIMIT_LENGTH, 8)
    };
    let error = query_result_rows(&mut statement, "12345678-1234-1234-1234-123456789012")
        .err()
        .unwrap();

    // SAFETY: restore the exact limit on the same exclusively owned connection.
    unsafe {
        rusqlite::ffi::sqlite3_limit(
            connection.handle(),
            rusqlite::ffi::SQLITE_LIMIT_LENGTH,
            previous,
        )
    };

    assert!(error.starts_with("query results:"));
    assert!(error.contains("too big"));
}

#[rstest::rstest]
#[case("NULL, 0, 0")]
#[case("'run', NULL, 0")]
#[case("'run', 0, NULL")]
fn session_projection_rejects_invalid_run_column_types(#[case] columns: &str) {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("session.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE runs (id INTEGER, run_id, finished, complete)")
        .unwrap();
    connection
        .execute_batch(&format!("INSERT INTO runs VALUES (1,{columns})"))
        .unwrap();
    drop(connection);

    let actual_0 = session_observation(&path, &BTreeMap::new(), &BTreeSet::new());

    assert!(actual_0.unwrap_err().starts_with("read latest run:"));
}

#[test]
fn session_projection_rejects_invalid_result_identifier_type() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("session.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE runs (id INTEGER, run_id TEXT, finished INTEGER, complete INTEGER); INSERT INTO runs VALUES (1,'run',0,0); CREATE TABLE results (mutant_id, outcome_json TEXT, run_id TEXT); INSERT INTO results VALUES (NULL, '{}', 'run')").unwrap();
    drop(connection);

    let actual_0 = session_observation(&path, &BTreeMap::new(), &BTreeSet::new());

    assert!(actual_0.unwrap_err().starts_with("read result:"));
}
