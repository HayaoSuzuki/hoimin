use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[tokio::test]
async fn pytest_and_unittest_commands_produce_the_same_mutant_statuses() {
    let pytest = run_fixture(&["-m", "pytest", "-q"]).await;
    let unittest = run_fixture(&["-m", "unittest", "discover", "-s", "tests"]).await;

    assert_eq!(pytest.statuses, unittest.statuses);
    assert_eq!(pytest.exit_code, unittest.exit_code);
    assert_eq!(pytest.statuses, ["killed"]);
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
        OsString::from("--python"),
        missing_root.join("python").into_os_string(),
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
        "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',{:?}]); time.sleep(20)",
        child
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
async fn failed_mutant_started_output_retires_the_sibling_process() {
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
        OsString::from("--python"),
        python.as_os_str().to_owned(),
        OsString::from("--format"),
        OsString::from("jsonl"),
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
        OsString::from("--python"),
        python.as_os_str().to_owned(),
        OsString::from("--format"),
        OsString::from("json"),
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

async fn run_fixture(test_args: &[&str]) -> FixtureRun {
    run_fixture_options(test_args, None, false).await
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
        OsString::from("--python"),
        python.as_os_str().to_owned(),
        OsString::from("--format"),
        OsString::from("json"),
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

fn fixture_root() -> PathBuf {
    repo_root().join("tests/fixtures/projects/basic")
}

fn python_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("HOIMIN_TEST_PYTHON") {
        return path.into();
    }
    let executable = if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    };
    assert!(
        executable.is_file(),
        "set HOIMIN_TEST_PYTHON to a Python with LibCST and pytest: {}",
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
