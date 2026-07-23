use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[tokio::test]
async fn unittest_command_produces_the_expected_mutant_statuses() {
    let unittest = run_fixture(&["-m", "unittest", "discover", "-s", "tests"]).await;

    assert_eq!(unittest.exit_code, 0);
    assert_eq!(unittest.statuses, ["killed"]);
    assert_eq!(
        unittest.document["run"]["versions"],
        serde_json::json!({
            "os": std::env::consts::OS,
            "hoimin": env!("CARGO_PKG_VERSION"),
        })
    );
}
#[tokio::test]
async fn ty_kills_a_nullable_contract_mutant() {
    let run = run_type_checker(&ty_executable(), "killed").await;

    assert_eq!(run.statuses, ["killed"]);
}

#[tokio::test]
async fn ty_reports_a_surviving_nullable_contract_mutant() {
    let run = run_type_checker(&ty_executable(), "survived").await;

    assert_eq!(run.statuses, ["survived"]);
}

#[tokio::test]
async fn mypy_kills_a_nullable_contract_mutant() {
    let run = run_type_checker(&mypy_executable(), "killed").await;

    assert_eq!(run.statuses, ["killed"]);
}

#[tokio::test]
async fn mypy_reports_a_surviving_nullable_contract_mutant() {
    let run = run_type_checker(&mypy_executable(), "survived").await;

    assert_eq!(run.statuses, ["survived"]);
}

#[tokio::test]
async fn jobs_one_and_four_produce_the_same_candidates_and_statuses() {
    let one = tempfile::tempdir().unwrap();
    let four = tempfile::tempdir().unwrap();
    write_parallel_project(one.path());
    write_parallel_project(four.path());
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";

    let serial = run_project(one.path(), 1, command).await;
    let parallel = run_project(four.path(), 4, command).await;
    let project_results = |run: &FixtureRun| {
        let mut results = run.document["mutants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mutant| {
                (
                    mutant["candidate"]["id"].as_str().unwrap().to_owned(),
                    mutant["status"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        results.sort();
        results
    };

    assert_eq!(serial.exit_code, parallel.exit_code);
    assert_eq!(project_results(&serial), project_results(&parallel));
    assert!(parallel.document["mutants"].as_array().unwrap().len() >= 4);
    for run in [&serial, &parallel] {
        let sequences = run.document["mutants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|mutant| mutant["sequence"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
    }
}

#[tokio::test]
async fn jobs_four_reaches_a_cross_process_barrier() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    write_parallel_project(directory.path());
    let markers = coordinator.path().join("markers");
    std::fs::create_dir(&markers).unwrap();
    let overlap = coordinator.path().join("overlap-proven");
    let original = "return a + b + c + d + e";
    let mutant = format!(
        "marker=markers/str(os.getpid())\nmarker.write_text('running')\ntry:\n    deadline=time.monotonic()+2\n    while len(list(markers.iterdir())) < 2 and time.monotonic() < deadline: time.sleep(.02)\n    if len(list(markers.iterdir())) >= 2: Path({:?}).write_text('proven')\n    from src.calc import total\n    assert total(1,2,3,4,5) == 15\nfinally:\n    marker.unlink(missing_ok=True)",
        overlap.to_string_lossy(),
    );
    let command = format!(
        "from pathlib import Path; import os,time; source=Path('src/calc.py').read_text(); markers=Path({:?}); exec({:?}) if {:?} not in source else exec('from src.calc import total; assert total(1,2,3,4,5) == 15')",
        markers.to_string_lossy(),
        mutant,
        original,
    );

    let run = run_project(directory.path(), 4, &command).await;

    assert_eq!(
        run.exit_code, 0,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    assert!(overlap.exists(), "two live worker processes must overlap");
    assert_eq!(std::fs::read_dir(markers).unwrap().count(), 0);
    assert!(run.statuses.iter().all(|status| status == "killed"));
}

#[tokio::test]
async fn jobs_four_processes_receive_isolated_run_mutant_and_worker_metadata() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    let records = coordinator.path().join("records");
    std::fs::create_dir(&records).unwrap();
    write_parallel_project(project.path());
    let original = "return a + b + c + d + e";
    let mutant = format!(
        "assert Path(os.environ['HOIMIN_WORKER_ROOT']).resolve() == Path.cwd().resolve()\nPath({:?}, os.environ['HOIMIN_MUTANT_ID']).write_text(os.environ['HOIMIN_RUN_ID'])",
        records.to_string_lossy(),
    );
    let command = format!(
        r#"from pathlib import Path; import os; source=Path('src/calc.py').read_text(); exec({mutant:?}) if {original:?} not in source else exec("assert 'HOIMIN_MUTANT_ID' not in os.environ; assert os.environ['HOIMIN_RUN_ID']"); from src.calc import total; assert total(1,2,3,4,5) == 15"#,
    );

    let run = run_project(project.path(), 4, &command).await;

    assert_eq!(
        run.exit_code, 0,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    let run_id = run.document["run"]["run_id"].as_str().unwrap();
    for mutant in run.document["mutants"].as_array().unwrap() {
        let mutant_id = mutant["candidate"]["id"].as_str().unwrap();
        assert_eq!(
            std::fs::read_to_string(records.join(mutant_id)).unwrap(),
            run_id
        );
    }
}

#[tokio::test]
async fn joinset_and_completion_queue_stay_bounded_across_many_mutants() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    let expression = (1..=24)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(" + ");
    std::fs::write(
        source.join("calc.py"),
        format!("def total():\n    return {expression}\n"),
    )
    .unwrap();
    let python = python_executable();
    let config = hoimin_cli::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from("4"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from src.calc import total; assert total() == 300"),
    ])
    .unwrap();
    let stdout = SharedBuffer::default();
    let control = hoimin_cli::shell::RunControl::new();

    let exit = hoimin_cli::shell::run_loop_with_control(
        config,
        stdout.clone(),
        SharedBuffer::default(),
        control.clone(),
    )
    .await
    .unwrap();

    assert_eq!(exit, 0);
    let document: serde_json::Value = serde_json::from_slice(&stdout.bytes()).unwrap();
    assert!(document["mutants"].as_array().unwrap().len() >= 20);
    assert!(control.max_process_tasks() <= 4);
    assert!(control.max_completion_in_flight() <= 5);
}

#[tokio::test]
async fn metrics_sidecar_observes_complete_parallel_run_without_changing_report() {
    let directory = tempfile::tempdir().unwrap();
    let metrics_path = directory.path().join("metrics.json");
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def total():\n    return 1 + 2 + 3\n",
    )
    .unwrap();
    let run = run_project_options(
        project.path(),
        2,
        "from src.calc import total; assert total() == 6",
        &[
            "--max-mutants",
            "2",
            "--metrics",
            metrics_path.to_str().unwrap(),
        ],
    )
    .await;

    assert_eq!(run.exit_code, 0, "stderr={}", run.stderr);
    assert_eq!(
        run.document.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["baseline", "mutants", "run", "schema_version", "summary"]
    );
    let metrics_text = std::fs::read_to_string(&metrics_path).unwrap();
    assert!(!metrics_text.contains(project.path().to_str().unwrap()));
    let metrics_value: serde_json::Value = serde_json::from_str(&metrics_text).unwrap();
    let metrics_schema: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("docs/json-schema/run-metrics.schema.json")).unwrap(),
    )
    .unwrap();
    assert_schema_valid(&metrics_schema, &metrics_value);
    let metrics: hoimin_core::RunMetrics = serde_json::from_str(&metrics_text).unwrap();
    metrics.validate().unwrap();
    assert_eq!(metrics.run_id, run.document["run"]["run_id"]);
    assert_eq!(
        metrics
            .stages
            .iter()
            .map(|stage| stage.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "analysis",
            "baseline",
            "cleanup",
            "copy",
            "mutants",
            "preflight",
            "targets"
        ]
    );
    assert_eq!(metrics.discovered, 2);
    assert_eq!(metrics.executed, 2);
    assert!(metrics.workers.iter().all(|worker| worker.processes > 0));
    assert!(
        metrics
            .workers
            .windows(2)
            .all(|workers| workers[0].worker < workers[1].worker)
    );
}

fn assert_schema_valid(schema: &serde_json::Value, instance: &serde_json::Value) {
    if let Err(error) = validate_schema(schema, instance, schema, "$") {
        panic!("schema validation failed: {error}\ninstance: {instance}");
    }
}

fn validate_schema(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    root: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| format!("{path}: unsupported schema reference {reference}"))?;
        let target = root
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: unresolved schema reference {reference}"))?;
        return validate_schema(target, instance, root, path);
    }
    if let Some(expected) = schema.get("const")
        && instance != expected
    {
        return Err(format!("{path}: expected const {expected}, got {instance}"));
    }
    if let Some(kind) = schema.get("type").and_then(serde_json::Value::as_str) {
        let matches = match kind {
            "object" => instance.is_object(),
            "array" => instance.is_array(),
            "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
            "string" => instance.is_string(),
            _ => false,
        };
        if !matches {
            return Err(format!("{path}: {instance} does not have type {kind}"));
        }
    }
    if let Some(minimum) = schema.get("minimum").and_then(serde_json::Value::as_f64)
        && instance.as_f64().is_some_and(|value| value < minimum)
    {
        return Err(format!("{path}: number is below {minimum}"));
    }
    if let Some(object) = instance.as_object() {
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object);
        if let Some(required) = schema.get("required").and_then(serde_json::Value::as_array) {
            for name in required.iter().filter_map(serde_json::Value::as_str) {
                if !object.contains_key(name) {
                    return Err(format!("{path}: missing required property {name}"));
                }
            }
        }
        if let Some(properties) = properties {
            for (name, value) in object {
                let property_schema = properties
                    .get(name)
                    .ok_or_else(|| format!("{path}: unexpected property {name}"))?;
                validate_schema(property_schema, value, root, &format!("{path}.{name}"))?;
            }
        }
    }
    if let (Some(items), Some(values)) = (schema.get("items"), instance.as_array()) {
        for (index, value) in values.iter().enumerate() {
            validate_schema(items, value, root, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn metrics_write_failure_warns_without_changing_run_result() {
    let directory = tempfile::tempdir().unwrap();
    let ordinary = run_fixture_options_extra(
        &["-m", "unittest", "discover", "-s", "tests"],
        None,
        false,
        &["--jobs", "2", "--max-mutants", "2"],
    )
    .await;
    let failed = run_fixture_options_extra(
        &["-m", "unittest", "discover", "-s", "tests"],
        None,
        false,
        &[
            "--jobs",
            "2",
            "--max-mutants",
            "2",
            "--metrics",
            directory.path().to_str().unwrap(),
        ],
    )
    .await;

    assert_eq!(failed.exit_code, ordinary.exit_code);
    assert_eq!(failed.statuses, ordinary.statuses);
    assert_eq!(
        failed.document["summary"]["counts"],
        ordinary.document["summary"]["counts"]
    );
    assert_eq!(
        failed.document["summary"]["exit_code"],
        ordinary.document["summary"]["exit_code"]
    );
    assert_eq!(
        failed
            .document
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ordinary
            .document
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>()
    );
    assert!(directory.path().is_dir());
    assert!(failed.stderr.contains("metrics.write"), "{}", failed.stderr);
}

#[tokio::test]
async fn metrics_finish_baseline_timing_before_early_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let metrics_path = directory.path().join("metrics.json");
    let run = run_fixture_options_extra(
        &["-c", "raise SystemExit(1)"],
        None,
        false,
        &["--metrics", metrics_path.to_str().unwrap()],
    )
    .await;

    assert_eq!(run.exit_code, 3, "stderr={}", run.stderr);
    let metrics: hoimin_core::RunMetrics =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    metrics.validate().unwrap();
    assert!(metrics.stages.iter().any(|stage| stage.name == "baseline"));
    assert!(metrics.stages.iter().any(|stage| stage.name == "cleanup"));
    assert!(!metrics.stages.iter().any(|stage| stage.name == "analysis"));
    assert!(!metrics.stages.iter().any(|stage| stage.name == "mutants"));
}

#[tokio::test]
async fn failing_baseline_runs_no_mutants_and_returns_three() {
    let run = run_fixture(&["-c", "raise SystemExit(1)"]).await;

    assert_eq!(run.exit_code, 3);
    assert!(run.statuses.is_empty());
}

#[tokio::test]
async fn session_is_not_created_when_the_option_is_absent_and_stdout_is_one_json_document() {
    let run = run_fixture(&["-m", "unittest", "discover", "-s", "tests"]).await;

    assert!(!fixture_root().join(".hoimin.sqlite3").exists());
    assert_eq!(run.stdout.lines().count(), 1);
    assert!(
        run.stderr.is_empty(),
        "unexpected diagnostics: {}",
        run.stderr
    );
}

#[tokio::test]
async fn fingerprint_include_unmatched_fails_before_creating_session() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let root = fixture_root();
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        database.as_os_str().to_owned(),
        OsString::from("--fingerprint-include"),
        OsString::from("missing.toml"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-m"),
        OsString::from("unittest"),
        OsString::from("discover"),
        OsString::from("-s"),
        OsString::from("tests"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stderr = String::from_utf8(stderr).unwrap();

    assert_eq!(exit_code, 2);
    assert!(stderr.contains("fingerprint.include.unmatched"));
    assert!(!database.exists());
}

#[tokio::test]
async fn fingerprint_file_missing_fails_before_creating_session() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let root = fixture_root();
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        database.as_os_str().to_owned(),
        OsString::from("--fingerprint-file"),
        OsString::from("missing.toml"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-m"),
        OsString::from("unittest"),
        OsString::from("discover"),
        OsString::from("-s"),
        OsString::from("tests"),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(exit_code, 2);
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("fingerprint.file.not_found")
    );
    assert!(!database.exists());
}

#[tokio::test]
async fn fingerprint_include_is_reported() {
    let options = ["--fingerprint-include", "pyproject.toml"];
    let test_args = ["-m", "unittest", "discover", "-s", "tests"];

    let json = run_fixture_options_extra(&test_args, None, false, &options).await;
    assert_eq!(json.exit_code, 0, "stderr={}", json.stderr);
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_includes"],
        serde_json::json!(["pyproject.toml"])
    );
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_inputs"][0]["path"],
        "pyproject.toml"
    );

    let jsonl =
        run_fixture_options_extra_with_format(&test_args, None, false, "jsonl", &options).await;
    assert_eq!(jsonl.exit_code, 0, "stderr={}", jsonl.stderr);
    let run_started = jsonl
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["kind"] == "run_started")
        .unwrap();
    assert_eq!(
        run_started["normalized_config"]["fingerprint_includes"],
        serde_json::json!(["pyproject.toml"])
    );
    assert_eq!(
        run_started["normalized_config"]["fingerprint_inputs"][0]["path"],
        "pyproject.toml"
    );
}

#[tokio::test]
async fn fingerprint_file_is_reported() {
    let options = ["--fingerprint-file", "pyproject.toml"];
    let test_args = ["-m", "unittest", "discover", "-s", "tests"];

    let json = run_fixture_options_extra(&test_args, None, false, &options).await;
    assert_eq!(json.exit_code, 0, "stderr={}", json.stderr);
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_files"],
        serde_json::json!(["pyproject.toml"])
    );
    assert_eq!(
        json.document["run"]["normalized_config"]["fingerprint_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| record["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["pyproject.toml"]
    );

    let jsonl =
        run_fixture_options_extra_with_format(&test_args, None, false, "jsonl", &options).await;
    assert_eq!(jsonl.exit_code, 0, "stderr={}", jsonl.stderr);
    let run_started = jsonl
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["kind"] == "run_started")
        .unwrap();
    assert_eq!(
        run_started["normalized_config"]["fingerprint_files"],
        serde_json::json!(["pyproject.toml"])
    );
}

#[tokio::test]
async fn shell_context_construction_performs_no_project_io() {
    let directory = tempfile::tempdir().unwrap();
    let missing_root = directory.path().join("missing-project");
    let session = missing_root.join("session.sqlite3");
    let config = hoimin_cli::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        missing_root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        session.as_os_str().to_owned(),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        OsString::from("test-command"),
    ])
    .unwrap();
    let context = hoimin_cli::shell::ShellContext::new(&config, Vec::new(), Vec::new())
        .await
        .unwrap();

    assert!(!missing_root.exists());
    assert!(!session.exists());
    drop(context);
}

#[tokio::test]
async fn sqlite_session_saves_and_resumes_a_determinate_result_without_reexecution() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let first = run_fixture_with_session(
        &["-m", "unittest", "discover", "-s", "tests"],
        &database,
        false,
    )
    .await;
    assert_eq!(
        first.exit_code, 0,
        "stderr={} stdout={}",
        first.stderr, first.stdout
    );
    assert_eq!(first.statuses, ["killed"]);
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);

    let resumed = run_fixture_with_session(
        &["-m", "unittest", "discover", "-s", "tests"],
        &database,
        true,
    )
    .await;
    assert_eq!(resumed.exit_code, 0);
    assert_eq!(resumed.statuses, ["killed"]);
    let resumed_run_id = resumed.document["run"]["run_id"].as_str().unwrap();
    assert_eq!(
        resumed.document["baseline"]["run_id"].as_str().unwrap(),
        resumed_run_id
    );
    assert_eq!(
        resumed.document["mutants"][0]["run_id"].as_str().unwrap(),
        resumed_run_id
    );
    assert_eq!(
        resumed.document["summary"]["run_id"].as_str().unwrap(),
        resumed_run_id
    );
}

#[tokio::test]
async fn fingerprint_include_change_starts_a_distinct_session_run() {
    let project = tempfile::tempdir().unwrap();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");
    let watched = project.path().join("watched.toml");
    write_parallel_project(project.path());
    std::fs::write(&watched, "value = 1\n").unwrap();
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";
    let fingerprint_include = ["--fingerprint-include", "watched.toml"];

    let first = run_project_with_session_options(
        project.path(),
        &database,
        false,
        1,
        command,
        &fingerprint_include,
    )
    .await;
    assert_eq!(first.exit_code, 4, "stderr={}", first.stderr);
    let first_run_id = first.document["run"]["run_id"].as_str().unwrap().to_owned();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);
    std::fs::write(&watched, "value = 2\n").unwrap();

    let resumed = run_project_with_session_options(
        project.path(),
        &database,
        true,
        1,
        command,
        &fingerprint_include,
    )
    .await;

    assert_eq!(resumed.exit_code, 4, "stderr={}", resumed.stderr);
    assert_ne!(
        resumed.document["run"]["run_id"].as_str().unwrap(),
        first_run_id
    );
    let connection = rusqlite::Connection::open(&database).unwrap();
    let run_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 2);
}

#[tokio::test]
async fn fingerprint_file_ignores_nested_names_but_tracks_the_exact_file() {
    let project = tempfile::tempdir().unwrap();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");
    write_parallel_project(project.path());
    std::fs::write(project.path().join("pyproject.toml"), "value = 1\n").unwrap();
    let nested = project.path().join(".worktrees/a/pyproject.toml");
    std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
    std::fs::write(&nested, "nested = 1\n").unwrap();
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";
    let options = ["--fingerprint-file", "pyproject.toml"];

    let first =
        run_project_with_session_options(project.path(), &database, false, 1, command, &options)
            .await;
    assert_eq!(first.exit_code, 4, "stderr={}", first.stderr);
    let first_run_id = first.document["run"]["run_id"].as_str().unwrap().to_owned();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);

    std::fs::write(&nested, "nested = 2\n").unwrap();
    let resumed =
        run_project_with_session_options(project.path(), &database, true, 1, command, &options)
            .await;
    assert_eq!(resumed.exit_code, 4, "stderr={}", resumed.stderr);
    assert_eq!(
        resumed.document["run"]["run_id"].as_str().unwrap(),
        first_run_id
    );

    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute("UPDATE runs SET complete=0", [])
        .unwrap();
    drop(connection);
    std::fs::write(project.path().join("pyproject.toml"), "value = 2\n").unwrap();
    let changed =
        run_project_with_session_options(project.path(), &database, true, 1, command, &options)
            .await;
    assert_eq!(changed.exit_code, 4, "stderr={}", changed.stderr);
    assert_ne!(
        changed.document["run"]["run_id"].as_str().unwrap(),
        first_run_id
    );
}

#[tokio::test]
async fn sqlite_session_can_be_resumed_after_repeated_mutant_limits() {
    let project = tempfile::tempdir().unwrap();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");
    write_parallel_project(project.path());
    let command = "from src.calc import total; assert total(1, 2, 3, 4, 5) == 15";

    let mut run_id = None;
    for resume in [false, true, true] {
        let run = run_project_with_session(project.path(), &database, resume, 1, command).await;
        assert_eq!(run.exit_code, 4, "stderr={}", run.stderr);
        assert!(!run.stderr.contains("session.finish.state"));
        let connection = rusqlite::Connection::open(&database).unwrap();
        let current_run_id = run.document["run"]["run_id"].as_str().unwrap();
        if let Some(first_run_id) = &run_id {
            assert_eq!(current_run_id, first_run_id);
        } else {
            run_id = Some(current_run_id.to_owned());
        }
        let (run_count, complete): (i64, i64) = connection
            .query_row("SELECT COUNT(*), MAX(complete) FROM runs", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(run_count, 1);
        assert_eq!(complete, 0);
    }
}

#[tokio::test]
async fn focused_profile_is_reported_and_omits_arid_candidates() {
    let project = focused_profile_project();
    let run = run_focused_profile(project.path(), "focused", "json", None, false, 100).await;

    assert_eq!(
        run.document["run"]["normalized_config"]["profile"],
        "focused"
    );
    let lines: Vec<_> = run.document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["candidate"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![4, 9]);

    let jsonl = run_focused_profile(project.path(), "focused", "jsonl", None, false, 100).await;
    let run_started = jsonl
        .stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["kind"] == "run_started")
        .unwrap();
    assert_eq!(run_started["normalized_config"]["profile"], "focused");

    let human = run_focused_profile(project.path(), "focused", "human", None, false, 100).await;
    assert!(
        human
            .stdout
            .lines()
            .next()
            .unwrap()
            .contains("(profile: focused)"),
        "human output was: {}",
        human.stdout
    );
}

#[tokio::test]
async fn focused_profile_does_not_resume_full_profile_session() {
    let project = focused_profile_project();
    let sessions = tempfile::tempdir().unwrap();
    let database = sessions.path().join("session.sqlite3");

    let full = run_focused_profile(project.path(), "full", "json", Some(&database), false, 1).await;
    assert_eq!(full.exit_code, 4, "stderr={}", full.stderr);
    assert_eq!(full.document["summary"]["complete"], false);

    let focused =
        run_focused_profile(project.path(), "focused", "json", Some(&database), true, 1).await;
    assert_eq!(focused.exit_code, 4, "stderr={}", focused.stderr);
    assert_eq!(focused.document["summary"]["complete"], false);

    let connection = rusqlite::Connection::open(database).unwrap();
    let run_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 2);
}

#[test]
fn readme_documents_focused_profile_selection_and_session_compatibility() {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let windows_readme = readme.replace("\r\n", "\n").replace('\n', "\r\n");

    for documented_readme in [&readme, &windows_readme] {
        let documented_readme = documented_readme.replace("\r\n", "\n");

        assert!(documented_readme.contains("`--profile full|focused`"));
        assert!(documented_readme.contains(
            "`--profile full` is the default and considers every candidate produced by the selected\noperators."
        ));
        assert!(documented_readme.contains("Python `__main__` guards, bare\n`print(...)` calls, `assert` statements, and function default expressions."));
        assert!(documented_readme.contains(
            "Profile selection is part of session compatibility, so a focused run never resumes results from a full run and vice versa."
        ));
    }
}

#[test]
fn readme_documents_agent_plan_workflow() {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let documented_readme = readme.replace("\r\n", "\n");

    for expected in [
        "hoimin plan",
        "--allow-best-effort-memory",
        "--total-timeout 15m",
        "--candidate 'ID1' \\",
        "--candidate 'ID2' \\",
        "inherits the test command, execution limits, timeout settings, and resource policy",
        "create a new plan",
        "identical candidate-ID set",
        "After every test improvement, rerun every stable batch and save each report separately.",
        "same current test revision",
        "must not be combined",
        "`--fingerprint-include GLOB`",
        "`truncated`",
    ] {
        assert!(documented_readme.contains(expected), "missing {expected}");
    }
}

#[tokio::test]
async fn sqlite_save_failure_is_fatal_and_leaves_no_partial_result() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("session.sqlite3");
    let first = run_fixture_with_session(
        &["-m", "unittest", "discover", "-s", "tests"],
        &database,
        false,
    )
    .await;
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "UPDATE runs SET complete=0;
         DELETE FROM results;
         DELETE FROM candidates;
         CREATE TRIGGER fail_result BEFORE INSERT ON results
         BEGIN SELECT RAISE(ABORT, 'injected save failure'); END;",
        )
        .unwrap();
    drop(connection);

    let failed = run_fixture_with_session(
        &["-m", "unittest", "discover", "-s", "tests"],
        &database,
        true,
    )
    .await;
    assert_eq!(failed.exit_code, 2);
    assert_eq!(failed.document["summary"]["counts"]["killed"], 0);
    assert!(failed.document["mutants"].as_array().unwrap().is_empty());
    assert!(failed.stderr.contains("session"), "{}", failed.stderr);
    let connection = rusqlite::Connection::open(&database).unwrap();
    let results: i64 = connection
        .query_row("SELECT COUNT(*) FROM results", [], |row| row.get(0))
        .unwrap();
    assert_eq!(results, 0);
}

#[tokio::test]
async fn total_timeout_cancels_and_reaps_descendants_before_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("descendant-survived");
    let child = format!(
        "import pathlib,time; time.sleep(2); pathlib.Path({:?}).write_text('leak')",
        marker.to_string_lossy()
    );
    let command = format!(
        "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',{child:?}]); time.sleep(20)"
    );
    let started = Instant::now();
    let run =
        run_fixture_options_extra(&["-c", &command], None, false, &["--total-timeout", "1s"]).await;

    assert_eq!(
        run.exit_code, 4,
        "stderr={} stdout={}",
        run.stderr, run.stdout
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert!(!marker.exists(), "timed-out descendant outlived the run");
    assert_eq!(run.document["summary"]["complete"], false);
}

#[tokio::test]
async fn injected_ctrl_c_uses_the_production_cancel_path_and_finishes_session_incomplete() {
    let project = tempfile::tempdir().unwrap();
    let coordinator = tempfile::tempdir().unwrap();
    write_parallel_project(project.path());
    let active = coordinator.path().join("active");
    std::fs::create_dir(&active).unwrap();
    let descendant_ready = coordinator.path().join("descendant-ready");
    let descendant_ready_temp = coordinator.path().join("descendant-ready.tmp");
    let session = coordinator.path().join("session.sqlite3");
    let child = "import time; time.sleep(20)";
    let mutant = format!(
        "from pathlib import Path; import os,subprocess,sys,time; Path({:?},str(os.getpid())).write_text('running'); time.sleep(0.5); child=subprocess.Popen([sys.executable,'-c',{:?}]); ready_temp=Path({:?}); ready_temp.write_text(str(child.pid)); ready_temp.replace({:?}); time.sleep(20)",
        active.to_string_lossy(),
        child,
        descendant_ready_temp.to_string_lossy(),
        descendant_ready.to_string_lossy(),
    );
    let original = "return a + b + c + d + e";
    let command = format!(
        "from pathlib import Path; source=Path('src/calc.py').read_text(); exec({mutant:?}) if {original:?} not in source else exec('from src.calc import total; assert total(1,2,3,4,5) == 15')",
    );
    let python = python_executable();
    let config = hoimin_cli::cli::parse_config_from([
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from("4"),
        OsString::from("--session"),
        session.as_os_str().to_owned(),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ])
    .unwrap();
    let stdout = SharedBuffer::default();
    let stderr = SharedBuffer::default();
    let control = hoimin_cli::shell::RunControl::new();
    let run = hoimin_cli::shell::run_loop_with_control(
        config,
        stdout.clone(),
        stderr.clone(),
        control.clone(),
    );
    let cancel = async {
        let descendant =
            wait_for_descendant_process(&descendant_ready, Duration::from_secs(15)).await;
        control.cancel();
        descendant
    };
    let (exit, descendant) = Box::pin(tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(run, cancel)
    }))
    .await
    .expect("cancelled run must finish promptly");
    let exit = exit.unwrap();

    assert_eq!(exit, 130);
    let document: serde_json::Value =
        serde_json::from_slice(&stdout.bytes()).expect("parseable cancelled report");
    assert_eq!(document["summary"]["complete"], false);
    let not_run_count = document["summary"]["counts"]["not_run"]
        .as_u64()
        .expect("not_run summary count");
    let not_run_mutants = document["mutants"]
        .as_array()
        .expect("cancelled mutant array")
        .iter()
        .filter(|mutant| mutant["status"] == "not_run")
        .count() as u64;
    assert!(not_run_count > 0);
    assert_eq!(not_run_count, not_run_mutants);
    assert!(control.max_process_tasks() <= 4);
    assert!(control.max_completion_in_flight() <= 5);
    let connection = rusqlite::Connection::open(session).unwrap();
    let complete: i64 = connection
        .query_row("SELECT complete FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(complete, 0);
    drop(connection);
    assert!(
        descendant.wait_until_stops(Duration::from_secs(5)).await,
        "cancelled descendant {} outlived the run",
        descendant.pid()
    );
}

#[tokio::test]
async fn serial_output_that_requests_stop_is_accepted_before_cancellation() {
    let project = tempfile::tempdir().unwrap();
    let metrics_directory = tempfile::tempdir().unwrap();
    let metrics_path = metrics_directory.path().join("metrics.json");
    write_parallel_project(project.path());
    let python = python_executable();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        project.path().as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from("4"),
        OsString::from("--format"),
        OsString::from("jsonl"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--metrics"),
        metrics_path.as_os_str().to_owned(),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from src.calc import total; assert total(1,2,3,4,5) == 15"),
    ];
    let config = hoimin_cli::cli::parse_config_from(args).unwrap();
    let control = hoimin_cli::shell::RunControl::new();
    let mut stdout = CancelOnMutantFinished {
        accepted: Vec::new(),
        cancelled: false,
        control: control.clone(),
    };
    let mut stderr = Vec::new();

    let exit = hoimin_cli::shell::run_loop_with_control(config, &mut stdout, &mut stderr, control)
        .await
        .unwrap();

    assert_eq!(exit, 130);
    let events = String::from_utf8(stdout.accepted).unwrap();
    let events = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let mutants = events
        .iter()
        .filter(|event| event["kind"] == "mutant_finished")
        .collect::<Vec<_>>();
    let ids = mutants
        .iter()
        .map(|event| event["candidate"]["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        mutants.len(),
        "a completed output was re-emitted"
    );
    assert!(mutants.iter().any(|event| event["status"] != "not_run"));
    let metrics: hoimin_core::RunMetrics =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    metrics.validate().unwrap();
    assert_eq!(
        metrics.executed,
        mutants
            .iter()
            .filter(|event| event["status"] != "not_run")
            .count() as u64
    );
    assert!(metrics.workers.iter().all(|worker| worker.processes > 0));
}

#[tokio::test]
async fn failed_mutant_started_output_prevents_process_start() {
    let directory = tempfile::tempdir().unwrap();
    let counter = directory.path().join("executions");
    let command = format!(
        "from pathlib import Path; p=Path({:?}); p.write_text(str(int(p.read_text())+1) if p.exists() else '1'); from src.calc import add; assert add(1, 2) == 3",
        counter.to_string_lossy()
    );
    let python = python_executable();
    let root = fixture_root();
    let args = [
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--format"),
        OsString::from("jsonl"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ];
    let mut stdout = RejectMutantStarted::default();
    let mut stderr = Vec::new();

    let exit = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;

    assert_eq!(exit, 2);
    assert_eq!(std::fs::read_to_string(counter).unwrap(), "1");
}

#[tokio::test]
async fn worker_pythonpath_rewrites_an_original_src_root_to_the_mutated_copy() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let package = root.join("src/hoimin_worker_import_probe");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("__init__.py"), "").unwrap();
    std::fs::write(package.join("calc.py"), "def value():\n    return 1 + 1\n").unwrap();
    let python = python_executable();
    let args = [
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/hoimin_worker_import_probe/calc.py"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from hoimin_worker_import_probe.calc import value; assert value() == 2"),
    ];
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"))
        .args(args)
        .env("PYTHONPATH", root.join("src"))
        .output()
        .await
        .unwrap();

    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("complete JSON report");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(document["mutants"][0]["status"], "killed");
}

struct FixtureRun {
    exit_code: i32,
    statuses: Vec<String>,
    stdout: String,
    stderr: String,
    document: serde_json::Value,
}

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

async fn run_fixture(test_args: &[&str]) -> FixtureRun {
    run_fixture_options(test_args, None, false).await
}
async fn run_type_checker(checker: &Path, expected_status: &str) -> FixtureRun {
    let root = type_checking_fixture_root();
    let line = match expected_status {
        "killed" => "src/contracts.py:1",
        "survived" => "src/contracts.py:6",
        _ => panic!("unsupported type-checker expectation: {expected_status}"),
    };
    let checker_args: &[&str] = match checker.file_stem().and_then(|name| name.to_str()) {
        Some("ty") => &["check"],
        Some("mypy") => &["src"],
        Some(name) => panic!("unsupported type checker: {name}"),
        None => panic!("type checker has no executable name: {}", checker.display()),
    };
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--line"),
        OsString::from(line),
        OsString::from("--operators"),
        OsString::from("type_nullable"),
        OsString::from("--max-mutants"),
        OsString::from("1"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        checker.as_os_str().to_owned(),
    ];
    args.extend(checker_args.iter().map(OsString::from));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    fixture_run(exit_code, stdout, stderr)
}

async fn run_fixture_with_session(test_args: &[&str], session: &Path, resume: bool) -> FixtureRun {
    run_fixture_options(test_args, Some(session), resume).await
}

async fn run_fixture_options(
    test_args: &[&str],
    session: Option<&Path>,
    resume: bool,
) -> FixtureRun {
    run_fixture_options_extra(test_args, session, resume, &[]).await
}

async fn run_fixture_options_extra(
    test_args: &[&str],
    session: Option<&Path>,
    resume: bool,
    extra_options: &[&str],
) -> FixtureRun {
    run_fixture_options_extra_with_format(test_args, session, resume, "json", extra_options).await
}

async fn run_fixture_options_extra_with_format(
    test_args: &[&str],
    session: Option<&Path>,
    resume: bool,
    format: &str,
    extra_options: &[&str],
) -> FixtureRun {
    let root = fixture_root();
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--format"),
        OsString::from(format),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
    ];
    if let Some(session) = session {
        let separator = args.iter().position(|arg| arg == "--").unwrap();
        let mut options = vec![OsString::from("--session"), session.as_os_str().to_owned()];
        if resume {
            options.push(OsString::from("--resume"));
        }
        args.splice(separator..separator, options);
    }
    let separator = args.iter().position(|arg| arg == "--").unwrap();
    args.splice(
        separator..separator,
        extra_options.iter().copied().map(OsString::from),
    );
    args.extend(test_args.iter().map(OsString::from));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document = if format == "json" {
        serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
            )
        })
    } else {
        serde_json::Value::Null
    };
    let statuses = document["mutants"]
        .as_array()
        .map(|mutants| {
            mutants
                .iter()
                .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}
fn fixture_run(exit_code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> FixtureRun {
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
        )
    });
    let statuses = document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
        .collect();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}

#[derive(Default)]
struct RejectMutantStarted {
    accepted: Vec<u8>,
}

struct CancelOnMutantFinished {
    accepted: Vec<u8>,
    cancelled: bool,
    control: hoimin_cli::shell::RunControl,
}

impl Write for CancelOnMutantFinished {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.accepted.extend_from_slice(bytes);
        if !self.cancelled
            && self
                .accepted
                .windows(b"mutant_finished".len())
                .any(|window| window == b"mutant_finished")
        {
            self.cancelled = true;
            self.control.cancel();
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Write for RejectMutantStarted {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut combined = self.accepted.clone();
        combined.extend_from_slice(bytes);
        if combined
            .windows(b"mutant_started".len())
            .any(|window| window == b"mutant_started")
        {
            return Err(io::Error::other("injected mutant_started failure"));
        }
        self.accepted.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn write_parallel_project(root: &Path) {
    let source = root.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("calc.py"),
        "def total(a, b, c, d, e):\n    return a + b + c + d + e\n",
    )
    .unwrap();
}

fn focused_profile_project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("src");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("__init__.py"), "").unwrap();
    std::fs::write(
        source.join("focused.py"),
        "def decide(value=True):\n    print(1 + 2)\n    assert value is True\n    return 3 + 4\n\nif __name__ == \"__main__\":\n    launch = 5 + 6\nelse:\n    fallback = 7 + 8\n",
    )
    .unwrap();
    project
}

async fn run_focused_profile(
    root: &Path,
    profile: &str,
    format: &str,
    session: Option<&Path>,
    resume: bool,
    max_mutants: usize,
) -> FixtureRun {
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/focused.py"),
        OsString::from("--profile"),
        OsString::from(profile),
        OsString::from("--max-mutants"),
        OsString::from(max_mutants.to_string()),
        OsString::from("--format"),
        OsString::from(format),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from("from src.focused import decide; assert decide() == 7"),
    ];
    let separator = args.iter().position(|argument| argument == "--").unwrap();
    if let Some(session) = session {
        args.splice(
            separator..separator,
            [OsString::from("--session"), session.as_os_str().to_owned()],
        );
    }
    if resume {
        let separator = args.iter().position(|argument| argument == "--").unwrap();
        args.splice(separator..separator, [OsString::from("--resume")]);
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document = if format == "json" {
        serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
            )
        })
    } else {
        serde_json::Value::Null
    };
    let statuses = document["mutants"]
        .as_array()
        .map(|mutants| {
            mutants
                .iter()
                .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}

async fn run_project_with_session(
    root: &Path,
    session: &Path,
    resume: bool,
    max_mutants: usize,
    command: &str,
) -> FixtureRun {
    run_project_with_session_options(root, session, resume, max_mutants, command, &[]).await
}

async fn run_project_with_session_options(
    root: &Path,
    session: &Path,
    resume: bool,
    max_mutants: usize,
    command: &str,
    options: &[&str],
) -> FixtureRun {
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--session"),
        session.as_os_str().to_owned(),
    ];
    if resume {
        args.push(OsString::from("--resume"));
    }
    args.extend(options.iter().copied().map(OsString::from));
    args.extend([
        OsString::from("--max-mutants"),
        OsString::from(max_mutants.to_string()),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ]);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
        )
    });
    let statuses = document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
        .collect();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}
async fn run_project(root: &Path, jobs: usize, command: &str) -> FixtureRun {
    run_project_options(root, jobs, command, &[]).await
}

async fn run_project_options(
    root: &Path,
    jobs: usize,
    command: &str,
    extra_options: &[&str],
) -> FixtureRun {
    let python = python_executable();
    let mut args = vec![
        OsString::from("hoimin"),
        OsString::from("run"),
        OsString::from("--root"),
        root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--jobs"),
        OsString::from(jobs.to_string()),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--"),
        python.as_os_str().to_owned(),
        OsString::from("-c"),
        OsString::from(command),
    ];
    let separator = args.iter().position(|arg| arg == "--").unwrap();
    args.splice(
        separator..separator,
        extra_options.iter().copied().map(OsString::from),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!(
            "invalid JSON report ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
        )
    });
    let statuses = document["mutants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutant| mutant["status"].as_str().unwrap().to_owned())
        .collect();
    FixtureRun {
        exit_code,
        statuses,
        stdout,
        stderr,
        document,
    }
}

fn fixture_root() -> PathBuf {
    repo_root().join("tests/fixtures/projects/basic")
}
fn type_checking_fixture_root() -> PathBuf {
    repo_root().join("tests/fixtures/projects/type-checking")
}

fn python_executable() -> PathBuf {
    let executable = if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    };
    assert!(
        executable.is_file(),
        "missing controlled test Python interpreter: {}",
        executable.display()
    );
    executable
}
fn ty_executable() -> PathBuf {
    checker_executable("ty")
}

fn mypy_executable() -> PathBuf {
    checker_executable("mypy")
}

fn checker_executable(name: &str) -> PathBuf {
    let executable = if cfg!(windows) {
        repo_root().join(format!(".venv/Scripts/{name}.exe"))
    } else {
        repo_root().join(format!(".venv/bin/{name}"))
    };
    assert!(
        executable.is_file(),
        "missing controlled type checker: {}",
        executable.display()
    );
    executable
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

#[cfg(unix)]
struct DescendantProcess {
    // `libc::kill` accepts a signed Unix process ID. Validate the marker value
    // once when opening it and retain that native representation thereafter.
    pid: i32,
}

#[cfg(windows)]
struct DescendantProcess {
    pid: u32,
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl DescendantProcess {
    fn pid(&self) -> u32 {
        #[cfg(unix)]
        {
            self.pid.unsigned_abs()
        }

        #[cfg(windows)]
        {
            self.pid
        }
    }

    #[cfg(unix)]
    fn open(pid: u32) -> Option<Self> {
        i32::try_from(pid).ok().map(|pid| Self { pid })
    }

    #[cfg(windows)]
    fn open(pid: u32) -> Option<Self> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        // SAFETY: the fixture PID came from the child process and the handle is owned on success.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            (!handle.is_null()).then_some(Self { pid, handle })
        }
    }

    #[cfg(unix)]
    fn is_alive(&self) -> bool {
        // SAFETY: signal 0 performs no mutation and the process ID was validated in `open`.
        unsafe { libc::kill(self.pid, 0) == 0 }
    }

    #[cfg(windows)]
    fn is_alive(&self) -> bool {
        use windows_sys::Win32::Foundation::STILL_ACTIVE;
        use windows_sys::Win32::System::Threading::GetExitCodeProcess;

        // SAFETY: handle is retained by this fixture and valid until Drop.
        unsafe {
            let mut exit_code = 0;
            GetExitCodeProcess(self.handle, &raw mut exit_code) != 0
                && exit_code == STILL_ACTIVE as u32
        }
    }

    async fn wait_until_stops(&self, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while self.is_alive() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        !self.is_alive()
    }
}

#[cfg(windows)]
impl Drop for DescendantProcess {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        // SAFETY: this instance owns the successful OpenProcess handle.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

async fn wait_for_descendant_process(marker: &Path, timeout: Duration) -> DescendantProcess {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(pid) = std::fs::read_to_string(marker)
            .ok()
            .and_then(|value| value.trim().parse().ok())
        {
            if let Some(process) = DescendantProcess::open(pid) {
                return process;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "descendant-ready marker did not yield an open process before cancellation"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
