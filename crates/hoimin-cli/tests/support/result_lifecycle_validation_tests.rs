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
    assert!(parse_corpus(input).is_err());
}

#[test]
fn empty_corpus_lines_are_ignored_between_valid_rows() {
    let first = corpus_text().lines().next().unwrap();
    assert_eq!(parse_corpus(&format!("\n{first}\n\n")).unwrap().len(), 1);
}

#[rstest::rstest]
#[case("accept:m0:unknown")]
#[case("accept:m9:killed")]
#[case("accept:m0:killed:extra")]
#[case("finish_session:unknown")]
#[case("discover:m9")]
fn rejects_invalid_event_shapes(#[case] event: &str) {
    assert!(!known_event(event));
}

fn valid_summary() -> serde_json::Value {
    json!({"summary":{"counts":{"killed":1,"survived":0,"timeout":0,"out_of_memory":0,"process_limit":0,"error":0,"not_run":0,"inconclusive":0,"score":1.0}}})
}

#[test]
fn summary_requires_object_complete_keys_and_numeric_score() {
    assert_eq!(
        report_summary_counts(&json!({})).unwrap_err(),
        "summary counts is not an object"
    );
    assert!(
        report_summary_counts(&json!({"summary":{"counts":{}}}))
            .unwrap_err()
            .contains("missing killed")
    );
    let mut summary = valid_summary();
    summary["summary"]["counts"]["score"] = json!("invalid");
    assert_eq!(
        report_summary_counts(&summary).unwrap_err(),
        "summary score is neither numeric nor null"
    );
}

#[rstest::rstest]
#[case(json!({}), "not an array")]
#[case(json!({"mutants":[{}]}), "no symbol")]
#[case(json!({"mutants":[{"candidate":{"symbol":"other"}}]}), "unsupported candidate symbol")]
#[case(json!({"mutants":[{"candidate":{"symbol":"first"}}]}), "no candidate id")]
fn report_rejects_invalid_mutant_records(
    #[case] document: serde_json::Value,
    #[case] message: &str,
) {
    assert!(
        report_results(&document, &BTreeSet::new())
            .unwrap_err()
            .contains(message)
    );
}

#[test]
fn report_rejects_duplicate_ids_and_duplicate_roles() {
    let outcome = serde_json::to_value(mutation_outcome::executed(
        hoimin_core::MutationStatus::Killed,
    ))
    .unwrap();
    for (symbol, id) in [("second", "id"), ("first", "other")] {
        let document = json!({"mutants":[{"candidate":{"symbol":"first","id":"id"},"outcome":outcome},{"candidate":{"symbol":symbol,"id":id},"outcome":outcome}]});
        assert!(
            report_results(&document, &BTreeSet::new())
                .unwrap_err()
                .contains("repeats semantic role")
        );
    }
}

#[cfg(unix)]
#[test]
fn fixture_output_rejects_invalid_utf8_and_reports_json_context() {
    use std::os::unix::process::ExitStatusExt;
    for (stdout, stderr, message) in [
        (vec![255], vec![], "stdout is not UTF-8"),
        (vec![], vec![255], "stderr is not UTF-8"),
        (
            b"not JSON".to_vec(),
            b"detail".to_vec(),
            "invalid report JSON",
        ),
    ] {
        let error = fixture_run_from_output(Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout,
            stderr,
        })
        .err()
        .unwrap();
        assert!(error.contains(message));
    }
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
fn output_cleanup_and_marker_read_errors_are_observable() {
    let fixture = Fixture::new(false).unwrap();
    std::fs::create_dir(&fixture.marker).unwrap();
    assert!(fixture.clear_outputs().unwrap_err().contains("remove"));
    assert!(
        marker_roles(&fixture.marker)
            .unwrap_err()
            .contains("read execution marker")
    );
    std::fs::remove_dir(&fixture.marker).unwrap();
    assert_eq!(marker_roles(&fixture.marker).unwrap(), Vec::<String>::new());
    std::fs::write(&fixture.marker, "m0\n").unwrap();
    std::fs::write(&fixture.metrics, "{}").unwrap();
    fixture.clear_outputs().unwrap();
    assert!(!fixture.marker.exists());
    assert!(!fixture.metrics.exists());
}

#[rstest::rstest]
#[case("missing-results", "prepare result query")]
#[case("row-type", "read result")]
#[case("invalid-outcome", "decode durable mutation outcome")]
#[case("unreported-id", "absent from report")]
fn session_projection_rejects_invalid_durable_records(#[case] damage: &str, #[case] message: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE runs (id INTEGER, run_id TEXT, finished INTEGER, complete INTEGER); INSERT INTO runs VALUES (1, 'run', 0, 0);").unwrap();
    if damage != "missing-results" {
        connection
            .execute_batch("CREATE TABLE results (mutant_id TEXT, outcome_json, run_id TEXT)")
            .unwrap();
        let value = if damage == "row-type" {
            serde_json::Value::Null
        } else {
            json!("invalid")
        };
        if damage == "unreported-id" {
            let outcome = serde_json::to_string(&mutation_outcome::executed(
                hoimin_core::MutationStatus::Killed,
            ))
            .unwrap();
            connection
                .execute(
                    "INSERT INTO results VALUES ('unknown', ?1, 'run')",
                    [outcome],
                )
                .unwrap();
        } else if value.is_null() {
            connection
                .execute("INSERT INTO results VALUES ('unknown', NULL, 'run')", [])
                .unwrap();
        } else {
            connection
                .execute(
                    "INSERT INTO results VALUES ('unknown', 'invalid', 'run')",
                    [],
                )
                .unwrap();
        }
    }
    drop(connection);
    assert!(
        session_observation(&path, &BTreeMap::new(), &BTreeSet::new())
            .unwrap_err()
            .contains(message)
    );
}

#[test]
fn session_projection_reports_open_and_missing_run_errors() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        session_observation(directory.path(), &BTreeMap::new(), &BTreeSet::new())
            .unwrap_err()
            .contains("open session")
    );
    assert!(
        session_observation(
            &directory.path().join("empty.sqlite3"),
            &BTreeMap::new(),
            &BTreeSet::new()
        )
        .unwrap_err()
        .contains("read latest run")
    );
}

#[test]
fn observation_reports_metrics_io_parse_and_summary_contract_failures() {
    let fixture = Fixture::new(false).unwrap();
    let mut run = FixtureRun {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        document: valid_summary(),
    };
    run.document["mutants"] = json!([]);
    std::fs::write(&fixture.metrics, [255]).unwrap();
    assert!(
        observe_run(&fixture, &run, false, &fixture.metrics)
            .unwrap_err()
            .contains("read metrics")
    );
    std::fs::write(&fixture.metrics, "invalid").unwrap();
    assert!(
        observe_run(&fixture, &run, false, &fixture.metrics)
            .unwrap_err()
            .contains("parse metrics")
    );
    std::fs::remove_file(&fixture.metrics).unwrap();
    assert!(
        observe_run(&fixture, &run, false, &fixture.metrics)
            .unwrap_err()
            .contains("summary complete is not a bool")
    );
}

#[tokio::test(start_paused = true)]
async fn output_join_reports_reader_error_task_failure_and_drain_timeout() {
    let io_error = tokio::spawn(async { Err(std::io::Error::other("injected reader failure")) });
    assert!(
        join_output(io_error, "stdout")
            .await
            .unwrap_err()
            .contains("read hoimin CLI stdout")
    );
    let panic: tokio::task::JoinHandle<std::io::Result<Vec<u8>>> = tokio::spawn(async {
        panic!("injected output task panic");
    });
    assert!(
        join_output(panic, "stderr")
            .await
            .unwrap_err()
            .contains("join hoimin CLI stderr")
    );
    let pending = tokio::spawn(std::future::pending());
    assert_eq!(
        join_output(pending, "stdout").await.unwrap_err(),
        "timed out draining hoimin CLI stdout"
    );
}

#[tokio::test]
async fn bounded_process_output_requires_pipes_and_existing_executable() {
    let mut absent = tokio::process::Command::new("/nonexistent/hoimin-test-executable");
    assert!(
        bounded_cli_output(&mut absent, Duration::from_secs(1))
            .await
            .unwrap_err()
            .contains("spawn hoimin CLI")
    );
    let mut unpiped = tokio::process::Command::new(python_executable().unwrap());
    unpiped.args(["-c", "pass"]).kill_on_drop(true);
    assert_eq!(
        bounded_cli_output(&mut unpiped, Duration::from_secs(1))
            .await
            .unwrap_err(),
        "hoimin CLI stdout was not piped"
    );
    let mut no_stderr = tokio::process::Command::new(python_executable().unwrap());
    no_stderr
        .args(["-c", "pass"])
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    assert_eq!(
        bounded_cli_output(&mut no_stderr, Duration::from_secs(1))
            .await
            .unwrap_err(),
        "hoimin CLI stderr was not piped"
    );
}

#[tokio::test]
async fn durable_premise_reports_child_exit_and_deadline_expiry() {
    let fixture = Fixture::new(false).unwrap();
    let mut exited = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    exited.wait().await.unwrap();
    assert!(
        wait_for_first_durable_kill(&mut exited, &fixture.session, Duration::from_secs(1))
            .await
            .unwrap_err()
            .contains("exited before the first durable killed result")
    );
    assert_eq!(
        cleanup_cli_process_tree(&mut exited).await,
        "child already exited"
    );
    let mut sleeping = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    assert_eq!(
        wait_for_first_durable_kill(&mut sleeping, &fixture.session, Duration::ZERO)
            .await
            .unwrap_err(),
        "timed out waiting for the first durable killed result"
    );
    sleeping.kill().await.unwrap();
}

#[test]
fn session_reopen_reports_missing_schema_and_marker_write_failures() {
    let fixture = Fixture::new(false).unwrap();
    assert!(
        reopen_incomplete(&fixture, false)
            .unwrap_err()
            .contains("mark session incomplete")
    );
    assert!(
        reopen_incomplete(&fixture, true)
            .unwrap_err()
            .contains("clear seeded results")
    );
    let connection = Connection::open(&fixture.session).unwrap();
    connection.execute_batch("CREATE TABLE runs (complete INTEGER); CREATE TABLE results (id INTEGER); CREATE TABLE candidates (id INTEGER);").unwrap();
    std::fs::create_dir(fixture.session.with_extension("fail-persist")).unwrap();
    assert!(
        reopen_incomplete(&fixture, true)
            .unwrap_err()
            .contains("arm persistence failure")
    );
}

#[tokio::test]
async fn case_execution_classifies_mismatch_run_identity_failure_and_infrastructure() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let mut observation = expected_observation(&case);
    observation.exit_code = 42;
    let mismatch =
        run_case_with_execution(&case, |_| std::future::ready(Ok((observation, true)))).await;
    assert_eq!(mismatch.class, CaseClass::Mismatch);
    assert_eq!(mismatch.actual.unwrap().exit_code, 42);
    assert!(mismatch.detail.is_none());
    let expected = expected_observation(&case);
    let identity_mismatch =
        run_case_with_execution(&case, |_| std::future::ready(Ok((expected, false)))).await;
    assert_eq!(identity_mismatch.class, CaseClass::Mismatch);
    assert_eq!(
        identity_mismatch.detail.as_deref(),
        Some("run IDs disagree across public surfaces")
    );
    let failed = run_case_with_execution(&case, |_| {
        std::future::ready(Err("injected execution failure".into()))
    })
    .await;
    assert_eq!(failed.class, CaseClass::InfrastructureError);
    assert_eq!(failed.detail.as_deref(), Some("injected execution failure"));
    assert!(failed.actual.is_none());
    assert!(failed.expected.is_some());
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
#[case("source", "create source")]
#[case("package", "write package")]
#[case("module", "write source")]
fn fixture_creation_reports_each_filesystem_stage(#[case] stage: &str, #[case] message: &str) {
    let temporary = tempfile::tempdir().unwrap();
    if stage == "source" {
        std::fs::write(temporary.path().join("project"), "blocking file").unwrap();
    } else {
        let source = temporary.path().join("project/src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir(source.join(if stage == "package" {
            "__init__.py"
        } else {
            "calc.py"
        }))
        .unwrap();
    }
    assert!(
        Fixture::from_tempdir(Ok(temporary), false)
            .err()
            .unwrap()
            .contains(message)
    );
}

#[test]
fn clear_outputs_accepts_missing_files_and_session_placeholder_is_idempotent() {
    let fixture = Fixture::new(false).unwrap();
    fixture.clear_outputs().unwrap();
    enable_session(&fixture).unwrap();
    enable_session(&fixture).unwrap();
}

#[test]
fn seeded_session_open_failure_has_context() {
    let mut fixture = Fixture::new(false).unwrap();
    fixture.session = fixture.root.join("absent/database.sqlite3");
    assert!(
        reopen_incomplete(&fixture, false)
            .unwrap_err()
            .starts_with("open seeded session:")
    );
    assert!(
        enable_session(&fixture)
            .unwrap_err()
            .starts_with("create session placeholder:")
    );
}

#[test]
fn controlled_interpreter_requires_an_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("absent-python");
    assert_eq!(
        require_python(&path).unwrap_err(),
        format!("missing controlled Python interpreter: {}", path.display())
    );
}

#[test]
fn classification_rejects_malformed_or_contradictory_observations() {
    let case = parse_corpus(corpus_text()).unwrap().remove(0);
    let expected = expected_observation(&case);
    let mut result = CaseResult {
        id: case.id,
        class: CaseClass::Mismatch,
        expected: Some(expected.clone()),
        actual: Some(expected),
        detail: None,
    };
    assert!(!classification_consistent(&result));
    result.actual.as_mut().unwrap().exit_code = 42;
    assert!(classification_consistent(&result));
    result.class = CaseClass::InfrastructureError;
    result.actual = None;
    assert!(!classification_consistent(&result));
    result.detail = Some("injected execution error".into());
    assert!(classification_consistent(&result));
    result.class = CaseClass::Match;
    assert!(!classification_consistent(&result));
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
    assert!(
        observe_run(&fixture, &run, false, &fixture.metrics)
            .unwrap_err()
            .contains("validate metrics: unsupported metrics schema version 9")
    );
}

#[tokio::test]
async fn durable_timeout_command_reports_spawn_pipe_and_premise_failures() {
    let fixture = Fixture::new(false).unwrap();
    let absent = tokio::process::Command::new("/absent/hoimin-test-executable");
    assert!(
        run_durable_timeout_command(absent, &fixture.session)
            .await
            .err()
            .unwrap()
            .starts_with("spawn hoimin CLI:")
    );
    let mut unpiped = tokio::process::Command::new(python_executable().unwrap());
    unpiped.args(["-c", "pass"]).kill_on_drop(true);
    assert!(
        run_durable_timeout_command(unpiped, &fixture.session)
            .await
            .err()
            .unwrap()
            .starts_with("hoimin CLI output was not piped; cleanup:")
    );
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
    assert!(diagnostics.contains("SIGINT failed: injected signal failure"));
    assert!(diagnostics.contains("process-group SIGKILL failed: injected signal failure"));
    assert!(diagnostics.contains("child was not reaped after process-group cleanup"));
    child.kill().await.unwrap();
}

#[test]
fn diagnostics_recognizes_session_commit_failure_and_metrics_failure_together() {
    assert_eq!(
        diagnostics("session commit failed\nmetrics.write failed\n"),
        ["persistence_failed", "metrics_failed"]
    );
}

#[tokio::test(start_paused = true)]
async fn durable_wait_status_distinguishes_wait_failure_and_timeout() {
    let error = durable_timeout_status(Ok(Err(std::io::Error::other("injected wait failure"))))
        .unwrap_err();
    assert_eq!(
        error,
        "wait for hoimin CLI total timeout: injected wait failure"
    );
    let elapsed = tokio::time::timeout(
        Duration::from_secs(1),
        std::future::pending::<std::io::Result<std::process::ExitStatus>>(),
    )
    .await;
    assert_eq!(
        durable_timeout_status(elapsed).unwrap_err(),
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

#[test]
fn oracle_environment_selection_is_validated_in_isolated_processes() {
    let cases = parse_corpus(corpus_text()).unwrap();
    let model = cases.iter().find(|case| case.mode != "strict").unwrap();
    let strict = cases.iter().find(|case| case.mode == "strict").unwrap();
    for (mode, selected, succeeds, message) in [
        ("future", "", false, "unknown HOIMIN_RESULT_LIFECYCLE_MODE"),
        (
            "strict",
            "missing-case",
            false,
            "unknown HOIMIN_RESULT_LIFECYCLE_CASE",
        ),
        (
            "strict",
            model.id.as_str(),
            false,
            "only strict cases are executable",
        ),
        ("report", model.id.as_str(), true, "NotExecuted"),
        ("strict", strict.id.as_str(), true, "Match"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "result_lifecycle_oracle_correspondence",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MODE_ENV, mode)
            .env(CASE_ENV, selected)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            succeeds,
            "mode={mode}; selected={selected}"
        );
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(combined.contains(message), "{combined}");
    }
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
    assert_eq!(unsafe { libc::waitpid(pid, &raw mut status, 0) }, pid);
    let mut child = child;
    assert!(
        wait_for_first_durable_kill(&mut child, &fixture.session, Duration::from_secs(1))
            .await
            .unwrap_err()
            .starts_with("inspect hoimin CLI while waiting for durable result:")
    );
    assert!(signal_process(pid, libc::SIGINT).is_err());
}

#[test]
fn reporter_records_mismatches_infrastructure_and_malformed_classifications() {
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
    assert_eq!(failures, ["mismatch", "infrastructure", "malformed"]);
}

#[test]
fn seed_and_resumed_persistence_premises_reject_wrong_evidence() {
    let mut run = FixtureRun {
        exit_code: 7,
        stdout: "partial stdout".into(),
        stderr: "seed failed".into(),
        document: json!({}),
    };
    assert_eq!(
        validate_seed(&run).unwrap_err(),
        "seed run failed: exit=7 stderr=seed failed stdout=partial stdout"
    );
    run.exit_code = 0;
    validate_seed(&run).unwrap();
    validate_resumed_persistence_failure("resume_reuses_determinate", &run).unwrap();
    assert_eq!(
        validate_resumed_persistence_failure("session_persistence_failure", &run).unwrap_err(),
        "resumed run did not reach injected SQLite ABORT: seed failed"
    );
    run.stderr = "injected save failure".into();
    validate_resumed_persistence_failure("session_persistence_failure", &run).unwrap();
}

#[test]
fn metrics_failure_destination_reports_existing_path() {
    let fixture = Fixture::new(false).unwrap();
    std::fs::write(fixture.temp.path().join("metrics-target"), "occupied").unwrap();
    assert!(
        metrics_destination(&fixture, true)
            .unwrap_err()
            .starts_with("create metrics failure target:")
    );
}

#[tokio::test]
async fn strict_execution_rejects_an_unadapted_scenario() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.scenario = "unsupported".into();
    assert_eq!(
        execute_strict(&case).await.unwrap_err(),
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
    assert_eq!(
        wait_for_first_durable_kill(&mut child, &fixture.session, Duration::ZERO)
            .await
            .unwrap_err(),
        "timed out waiting for the first durable killed result"
    );
    child.kill().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn descendant_wait_distinguishes_live_gone_and_inaccessible_processes() {
    let mut child = tokio::process::Command::new(python_executable().unwrap())
        .args(["-c", "import time; time.sleep(60)"])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id().unwrap()).unwrap();
    assert!(!wait_for_descendant_exit(pid, Duration::ZERO).await);
    assert!(!wait_for_descendant_exit(1, Duration::ZERO).await);
    child.kill().await.unwrap();
    assert!(wait_for_descendant_exit(pid, Duration::from_secs(1)).await);
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
    assert!(
        session_observation(&path, &BTreeMap::new(), &BTreeSet::new())
            .unwrap_err()
            .starts_with("read latest run:")
    );
}

#[test]
fn session_projection_rejects_invalid_result_identifier_type() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("session.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE runs (id INTEGER, run_id TEXT, finished INTEGER, complete INTEGER); INSERT INTO runs VALUES (1,'run',0,0); CREATE TABLE results (mutant_id, outcome_json TEXT, run_id TEXT); INSERT INTO results VALUES (NULL, '{}', 'run')").unwrap();
    drop(connection);
    assert!(
        session_observation(&path, &BTreeMap::new(), &BTreeSet::new())
            .unwrap_err()
            .starts_with("read result:")
    );
}
