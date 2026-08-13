use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;

use rusqlite::Connection;
use serde::Deserialize;
use tokio::io::AsyncReadExt;

const MODE_ENV: &str = "HOIMIN_RESULT_LIFECYCLE_MODE";
const CASE_ENV: &str = "HOIMIN_RESULT_LIFECYCLE_CASE";

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
struct ResultObservation {
    mutant: String,
    status: String,
    executed: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    session: bool,
    metrics: bool,
    discovered: Vec<String>,
    seeded_durable: Vec<ResultObservation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)] // Mirrors the flat, Lean-owned wire schema.
struct ExpectedObservation {
    accepted: Vec<ResultObservation>,
    durable: Vec<ResultObservation>,
    reported: Vec<ResultObservation>,
    summary: Vec<String>,
    summary_counts: BTreeMap<String, u64>,
    metrics_executed: u64,
    metrics_observed: bool,
    stopped: bool,
    session_finished: bool,
    session_complete: bool,
    metrics_finished: bool,
    run_complete: bool,
    returned: bool,
    exit_code: i32,
    diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    setup: Setup,
    schedule: Vec<String>,
    expected: ExpectedObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Keeps expected and actual projections isomorphic.
struct ImplementationObservation {
    executed: Vec<String>,
    durable: Vec<ResultObservation>,
    reported: Vec<ResultObservation>,
    summary: BTreeMap<String, u64>,
    metrics_executed: Option<u64>,
    stopped: bool,
    session_finished: bool,
    session_complete: bool,
    metrics_finished: bool,
    run_complete: bool,
    returned: bool,
    exit_code: i32,
    diagnostics: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaseClass {
    Match,
    Mismatch,
    InfrastructureError,
}

#[derive(Debug)]
struct CaseResult {
    id: String,
    class: CaseClass,
    expected: Option<ImplementationObservation>,
    actual: Option<ImplementationObservation>,
    detail: Option<String>,
}

struct FixtureRun {
    exit_code: i32,
    stdout: String,
    stderr: String,
    document: serde_json::Value,
}

struct Fixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    marker: PathBuf,
    metrics: PathBuf,
    session: PathBuf,
}

fn corpus_text() -> &'static str {
    include_str!("../../../formal/HoiminOracle/corpus/result-lifecycle.jsonl")
}

fn known_status(value: &str) -> bool {
    matches!(
        value,
        "killed" | "survived" | "timeout" | "out_of_memory" | "process_limit" | "error" | "not_run"
    )
}

fn known_result(result: &ResultObservation) -> bool {
    matches!(result.mutant.as_str(), "m0" | "m1") && known_status(&result.status)
}

fn known_event(event: &str) -> bool {
    let parts = event.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        ["accept", role, status] => matches!(*role, "m0" | "m1") && known_status(status),
        [action, value] => {
            (matches!(
                *action,
                "discover"
                    | "persist_ok"
                    | "persist_failed"
                    | "record_result"
                    | "report_ok"
                    | "report_failed"
                    | "mark_not_run"
            ) && matches!(*value, "m0" | "m1"))
                || (*action == "finish_session" && matches!(*value, "true" | "false"))
        }
        [action] => matches!(
            *action,
            "stop" | "finish_metrics" | "metrics_failed" | "return_run"
        ),
        _ => false,
    }
}

fn validate_case(case: &OracleCase) -> Result<(), String> {
    if case.schema != 1 {
        return Err(format!("unsupported schema {}", case.schema));
    }
    if !matches!(
        case.mode.as_str(),
        "strict" | "model-only" | "internal-fixture"
    ) {
        return Err(format!("unknown mode {}", case.mode));
    }
    if case.id.is_empty()
        || !matches!(
            case.scenario.as_str(),
            "sessionless_complete"
                | "session_complete"
                | "resume_reuses_determinate"
                | "stop_preserves_accepted"
                | "metrics_write_failure"
                | "session_persistence_failure"
                | "stop_during_persist"
                | "duplicate_completion"
                | "stop_after_summary_before_report"
        )
        || case.schedule.is_empty()
    {
        return Err(format!(
            "case {} has an empty identity or schedule",
            case.id
        ));
    }
    if !case.setup.metrics {
        return Err(format!("case {} does not exercise metrics", case.id));
    }
    if case
        .setup
        .discovered
        .iter()
        .any(|role| !matches!(role.as_str(), "m0" | "m1"))
        || case
            .setup
            .seeded_durable
            .iter()
            .any(|result| !known_result(result))
        || case
            .expected
            .accepted
            .iter()
            .chain(&case.expected.durable)
            .chain(&case.expected.reported)
            .any(|result| !known_result(result))
        || case
            .expected
            .summary
            .iter()
            .any(|status| !known_status(status))
    {
        return Err(format!(
            "case {} contains an unknown role or status",
            case.id
        ));
    }
    if case.schedule.iter().any(|event| !known_event(event)) {
        return Err(format!("case {} contains an unknown event", case.id));
    }
    if case.expected.diagnostics.iter().any(|diagnostic| {
        !matches!(
            diagnostic.as_str(),
            "persistence_failed" | "report_failed" | "metrics_failed"
        )
    }) {
        return Err(format!("case {} contains an unknown diagnostic", case.id));
    }
    if case.expected.metrics_observed
        == case
            .expected
            .diagnostics
            .contains(&"metrics_failed".to_owned())
    {
        return Err(format!(
            "case {} has inconsistent metrics observability",
            case.id
        ));
    }
    if case.expected.summary_counts != summary_counts(&case.expected.summary) {
        return Err(format!(
            "case {} has inconsistent Lean summary projections",
            case.id
        ));
    }
    if !case.setup.session
        && (case.expected.session_finished
            || case.expected.session_complete
            || !case.expected.durable.is_empty())
    {
        return Err(format!("case {} persists state without a session", case.id));
    }
    Ok(())
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let case: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&case)?;
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id {}", case.id));
        }
        cases.push(case);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn summary_counts(statuses: &[String]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for status in statuses {
        *counts.entry(status.clone()).or_default() += 1;
    }
    counts
}

fn report_summary_counts(document: &serde_json::Value) -> Result<BTreeMap<String, u64>, String> {
    let counts = document["summary"]["counts"]
        .as_object()
        .ok_or_else(|| "summary counts is not an object".to_owned())?;
    for required in [
        "killed",
        "survived",
        "timeout",
        "out_of_memory",
        "process_limit",
        "error",
        "not_run",
        "inconclusive",
        "score",
    ] {
        if !counts.contains_key(required) {
            return Err(format!("summary counts is missing {required}"));
        }
    }
    let mut normalized = BTreeMap::new();
    let mut inconclusive = None;
    for (status, count) in counts {
        if status == "score" {
            if !count.is_null() && !count.is_number() {
                return Err("summary score is neither numeric nor null".to_owned());
            }
            continue;
        }
        let count = count
            .as_u64()
            .ok_or_else(|| format!("summary count for {status} is not a nonnegative integer"))?;
        if status == "inconclusive" {
            inconclusive = Some(count);
            continue;
        }
        if !known_status(status) {
            return Err(format!("summary counts contains unknown status {status}"));
        }
        if count > 0 {
            normalized.insert(status.clone(), count);
        }
    }
    let expected_inconclusive = [
        "timeout",
        "out_of_memory",
        "process_limit",
        "error",
        "not_run",
    ]
    .into_iter()
    .map(|status| normalized.get(status).copied().unwrap_or_default())
    .sum();
    if inconclusive != Some(expected_inconclusive) {
        return Err(format!(
            "summary inconclusive count {inconclusive:?} disagrees with derived count {expected_inconclusive}"
        ));
    }
    Ok(normalized)
}

fn expected_observation(case: &OracleCase) -> ImplementationObservation {
    ImplementationObservation {
        executed: case
            .expected
            .accepted
            .iter()
            .map(|result| result.mutant.clone())
            .collect(),
        durable: sorted(case.expected.durable.clone()),
        reported: sorted(case.expected.reported.clone()),
        summary: case.expected.summary_counts.clone(),
        metrics_executed: case
            .expected
            .metrics_observed
            .then_some(case.expected.metrics_executed),
        stopped: case.expected.stopped,
        session_finished: case.expected.session_finished,
        session_complete: case.expected.session_complete,
        metrics_finished: case.expected.metrics_finished,
        run_complete: case.expected.run_complete,
        returned: case.expected.returned,
        exit_code: case.expected.exit_code,
        diagnostics: case.expected.diagnostics.clone(),
    }
}

fn sorted(mut values: Vec<ResultObservation>) -> Vec<ResultObservation> {
    values.sort();
    values
}

impl Fixture {
    fn new(two_mutants: bool) -> Result<Self, String> {
        let temp = tempfile::tempdir().map_err(|error| format!("create temp dir: {error}"))?;
        let root = temp.path().join("project");
        let source = root.join("src");
        std::fs::create_dir_all(&source).map_err(|error| format!("create source: {error}"))?;
        std::fs::write(source.join("__init__.py"), "")
            .map_err(|error| format!("write package: {error}"))?;
        let second = if two_mutants {
            "\ndef second(second_left, second_right):\n    return second_left + second_right\n"
        } else {
            ""
        };
        std::fs::write(
            source.join("calc.py"),
            format!(
                "def first(first_left, first_right):\n    return first_left + first_right\n{second}"
            ),
        )
        .map_err(|error| format!("write source: {error}"))?;
        Ok(Self {
            marker: temp.path().join("executed"),
            metrics: temp.path().join("metrics.json"),
            session: temp.path().join("session.sqlite3"),
            root,
            temp,
        })
    }

    fn clear_outputs(&self) -> Result<(), String> {
        for path in [&self.marker, &self.metrics] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("remove {}: {error}", path.display())),
            }
        }
        Ok(())
    }

    fn test_command(&self, two_mutants: bool) -> String {
        let marker = self.marker.to_string_lossy();
        let first_mutant_delay = if two_mutants && cfg!(unix) {
            "if mutated_first:\n    time.sleep(2)\n"
        } else {
            ""
        };
        let test_body = if two_mutants {
            concat!(
                "mutated_second = 'return second_left - second_right' in source\n",
                "try:\n",
                "    if mutated_second:\n",
                "        time.sleep(20)\n",
                "    from src.calc import first, second\n",
                "    assert first(3, 2) == 5\n",
                "    assert second(3, 2) == 5\n",
                "finally:\n",
                "    if mutated_first:\n",
                "        marker.open('a').write('m0\\n')\n",
                "    if mutated_second:\n",
                "        marker.open('a').write('m1\\n')\n",
            )
        } else {
            concat!(
                "try:\n",
                "    from src.calc import first\n",
                "    assert first(3, 2) == 5\n",
                "finally:\n",
                "    if mutated_first:\n",
                "        marker.open('a').write('m0\\n')\n",
            )
        };
        format!(
            concat!(
                "from pathlib import Path\n",
                "import time\n",
                "source = Path('src/calc.py').read_text()\n",
                "marker = Path({marker:?})\n",
                "mutated_first = 'return first_left - first_right' in source\n",
                "{first_mutant_delay}",
                "{test_body}",
            ),
            marker = marker,
            first_mutant_delay = first_mutant_delay,
            test_body = test_body,
        )
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace crates directory")
        .parent()
        .expect("workspace root")
        .to_owned()
}

fn python_executable() -> Result<PathBuf, String> {
    let executable = if cfg!(windows) {
        repo_root().join(".venv/Scripts/python.exe")
    } else {
        repo_root().join(".venv/bin/python")
    };
    executable
        .is_file()
        .then_some(executable.clone())
        .ok_or_else(|| {
            format!(
                "missing controlled Python interpreter: {}",
                executable.display()
            )
        })
}

fn build_cli_command(
    fixture: &Fixture,
    two_mutants: bool,
    resume: bool,
    metrics_path: &Path,
    total_timeout: Option<&str>,
) -> Result<tokio::process::Command, String> {
    let python = python_executable()?;
    let mut args = vec![
        OsString::from("run"),
        OsString::from("--root"),
        fixture.root.as_os_str().to_owned(),
        OsString::from("--source"),
        OsString::from("src"),
        OsString::from("--file"),
        OsString::from("src/calc.py"),
        OsString::from("--operators"),
        OsString::from("binary_add_sub"),
        OsString::from("--jobs"),
        OsString::from("1"),
        OsString::from("--max-mutants"),
        OsString::from(if two_mutants { "2" } else { "1" }),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--allow-best-effort-memory"),
        OsString::from("--metrics"),
        metrics_path.as_os_str().to_owned(),
    ];
    if fixture.session.exists() || resume {
        args.push(OsString::from("--session"));
        args.push(fixture.session.as_os_str().to_owned());
    }
    if resume {
        args.push(OsString::from("--resume"));
    }
    if let Some(total_timeout) = total_timeout {
        args.extend([
            OsString::from("--total-timeout"),
            OsString::from(total_timeout),
            OsString::from("--mutant-timeout"),
            OsString::from("30s"),
        ]);
    }
    args.extend([
        OsString::from("--"),
        python.into_os_string(),
        OsString::from("-c"),
        OsString::from(fixture.test_command(two_mutants)),
    ]);
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    Ok(command)
}

async fn run_cli(
    fixture: &Fixture,
    two_mutants: bool,
    resume: bool,
    metrics_path: &Path,
    total_timeout: bool,
) -> Result<FixtureRun, String> {
    let mut command = build_cli_command(
        fixture,
        two_mutants,
        resume,
        metrics_path,
        total_timeout.then_some("1s"),
    )?;
    let output = bounded_cli_output(&mut command, Duration::from_secs(12)).await?;
    fixture_run_from_output(output)
}

fn fixture_run_from_output(output: Output) -> Result<FixtureRun, String> {
    let exit_code = output.status.code().ok_or_else(|| {
        format!(
            "hoimin CLI terminated without an exit code: {}",
            output.status
        )
    })?;
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("stdout is not UTF-8: {error}"))?;
    let stderr = String::from_utf8(output.stderr)
        .map_err(|error| format!("stderr is not UTF-8: {error}"))?;
    let document = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!(
            "invalid report JSON ({error}); exit={exit_code}; stdout={stdout:?}; stderr={stderr:?}"
        )
    })?;
    Ok(FixtureRun {
        exit_code,
        stdout,
        stderr,
        document,
    })
}

#[cfg(unix)]
async fn wait_for_first_durable_kill(
    child: &mut tokio::process::Child,
    session: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            format!("inspect hoimin CLI while waiting for durable result: {error}")
        })? {
            return Err(format!(
                "hoimin CLI exited before the first durable killed result: {status}"
            ));
        }
        let ready = session.is_file()
            && Connection::open_with_flags(session, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .and_then(|connection| {
                    connection.query_row(
                        "SELECT COUNT(*) = 1 AND MIN(status) = 'killed' AND MAX(status) = 'killed' FROM results",
                        [],
                        |row| row.get::<_, bool>(0),
                    )
                })
                .unwrap_or(false);
        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("timed out waiting for the first durable killed result".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
async fn run_cli_with_durable_timeout_premise(
    fixture: &Fixture,
    metrics_path: &Path,
) -> Result<FixtureRun, String> {
    let mut command = build_cli_command(fixture, true, false, metrics_path, Some("12s"))?;
    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn hoimin CLI: {error}"))?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let cleanup = cleanup_cli_process_tree(&mut child).await;
        return Err(format!(
            "hoimin CLI output was not piped; cleanup: {cleanup}"
        ));
    };
    let stdout = tokio::spawn(read_output(stdout));
    let stderr = tokio::spawn(read_output(stderr));

    let trigger = async {
        wait_for_first_durable_kill(&mut child, &fixture.session, Duration::from_secs(8)).await?;
        tokio::time::timeout(Duration::from_secs(12), child.wait())
            .await
            .map_err(|_| "hoimin CLI did not stop after its total timeout".to_owned())?
            .map_err(|error| format!("wait for hoimin CLI total timeout: {error}"))
    }
    .await;

    match trigger {
        Ok(status) => {
            let stdout = join_output(stdout, "stdout").await?;
            let stderr = join_output(stderr, "stderr").await?;
            fixture_run_from_output(Output {
                status,
                stdout,
                stderr,
            })
        }
        Err(error) => {
            let cleanup = cleanup_cli_process_tree(&mut child).await;
            let stdout = join_output(stdout, "stdout").await;
            let stderr = join_output(stderr, "stderr").await;
            Err(format!(
                "premise-guarded timeout failed: {error}; cleanup: {cleanup}; stdout: {stdout:?}; stderr: {stderr:?}"
            ))
        }
    }
}

async fn bounded_cli_output(
    command: &mut tokio::process::Command,
    timeout: Duration,
) -> Result<Output, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn hoimin CLI: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "hoimin CLI stdout was not piped".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "hoimin CLI stderr was not piped".to_owned())?;
    let stdout = tokio::spawn(read_output(stdout));
    let stderr = tokio::spawn(read_output(stderr));

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            let cleanup = cleanup_cli_process_tree(&mut child).await;
            let stdout = join_output(stdout, "stdout").await;
            let stderr = join_output(stderr, "stderr").await;
            return Err(format!(
                "wait for hoimin CLI failed: {error}; cleanup: {cleanup}; stdout: {stdout:?}; stderr: {stderr:?}"
            ));
        }
        Err(_) => {
            let cleanup = cleanup_cli_process_tree(&mut child).await;
            let stdout = join_output(stdout, "stdout").await;
            let stderr = join_output(stderr, "stderr").await;
            return Err(format!(
                "hoimin CLI timed out after {} seconds; cleanup: {cleanup}; stdout: {stdout:?}; stderr: {stderr:?}",
                timeout.as_secs()
            ));
        }
    };
    let stdout = join_output(stdout, "stdout").await?;
    let stderr = join_output(stderr, "stderr").await?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

async fn read_output<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    Ok(bytes)
}

async fn join_output(
    mut reader: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, String> {
    if let Ok(result) = tokio::time::timeout(Duration::from_secs(2), &mut reader).await {
        result
            .map_err(|error| format!("join hoimin CLI {name}: {error}"))?
            .map_err(|error| format!("read hoimin CLI {name}: {error}"))
    } else {
        reader.abort();
        Err(format!("timed out draining hoimin CLI {name}"))
    }
}

#[cfg(unix)]
async fn cleanup_cli_process_tree(child: &mut tokio::process::Child) -> String {
    let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) else {
        return "child already exited".to_owned();
    };
    let mut diagnostics = Vec::new();
    // SAFETY: pid identifies the retained child; SIGINT lets hoimin run its own tree cleanup.
    if unsafe { libc::kill(pid, libc::SIGINT) } != 0 {
        diagnostics.push(format!(
            "SIGINT failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let reaped = matches!(
        tokio::time::timeout(Duration::from_secs(4), child.wait()).await,
        Ok(Ok(_))
    );
    // SAFETY: run_cli placed the child in a process group whose id is the positive child pid.
    if unsafe { libc::kill(-pid, libc::SIGKILL) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            diagnostics.push(format!("process-group SIGKILL failed: {error}"));
        }
    }
    if !reaped
        && !matches!(
            tokio::time::timeout(Duration::from_secs(2), child.wait()).await,
            Ok(Ok(_))
        )
    {
        diagnostics.push("child was not reaped after process-group cleanup".to_owned());
    }
    if diagnostics.is_empty() {
        "process tree terminated and root reaped".to_owned()
    } else {
        diagnostics.join(", ")
    }
}

#[cfg(windows)]
async fn cleanup_cli_process_tree(child: &mut tokio::process::Child) -> String {
    let mut diagnostics = Vec::new();
    if let Some(pid) = child.id() {
        let pid = pid.to_string();
        let mut taskkill = tokio::process::Command::new("taskkill");
        taskkill.args(["/PID", &pid, "/T", "/F"]).kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(5), taskkill.output()).await;
        if !matches!(output, Ok(Ok(ref value)) if value.status.success()) {
            diagnostics.push("taskkill did not confirm process-tree termination".to_owned());
        }
    }
    if !matches!(
        tokio::time::timeout(Duration::from_secs(5), child.wait()).await,
        Ok(Ok(_))
    ) {
        diagnostics.push("child was not reaped after taskkill".to_owned());
    }
    if diagnostics.is_empty() {
        "process tree terminated and root reaped".to_owned()
    } else {
        diagnostics.join(", ")
    }
}

fn enable_session(fixture: &Fixture) -> Result<(), String> {
    if fixture.session.exists() {
        return Ok(());
    }
    Connection::open(&fixture.session)
        .map(drop)
        .map_err(|error| format!("create session placeholder: {error}"))
}

fn reopen_incomplete(fixture: &Fixture, fail_persist: bool) -> Result<(), String> {
    let connection = Connection::open(&fixture.session)
        .map_err(|error| format!("open seeded session: {error}"))?;
    if fail_persist {
        connection
            .execute_batch(
                "UPDATE runs SET complete=0;
                 DELETE FROM results;
                 DELETE FROM candidates;
                 CREATE TRIGGER fail_result BEFORE INSERT ON results
                 BEGIN SELECT RAISE(ABORT, 'injected save failure'); END;",
            )
            .map_err(|error| format!("inject persistence failure: {error}"))?;
    } else {
        connection
            .execute("UPDATE runs SET complete=0", [])
            .map_err(|error| format!("mark session incomplete: {error}"))?;
    }
    Ok(())
}

async fn execute_strict(case: &OracleCase) -> Result<(ImplementationObservation, bool), String> {
    let two_mutants = case.scenario == "stop_preserves_accepted";
    let fixture = Fixture::new(two_mutants)?;
    let has_session = case.setup.session;
    if has_session {
        enable_session(&fixture)?;
    }
    let metrics_directory = fixture.temp.path().join("metrics-target");
    let metrics_path = if case.scenario == "metrics_write_failure" {
        std::fs::create_dir(&metrics_directory)
            .map_err(|error| format!("create metrics failure target: {error}"))?;
        metrics_directory.as_path()
    } else {
        fixture.metrics.as_path()
    };

    let run = match case.scenario.as_str() {
        "resume_reuses_determinate" | "session_persistence_failure" => {
            let seed = run_cli(&fixture, false, false, &fixture.metrics, false).await?;
            if seed.exit_code != 0 {
                return Err(format!(
                    "seed run failed: exit={} stderr={} stdout={}",
                    seed.exit_code, seed.stderr, seed.stdout
                ));
            }
            reopen_incomplete(&fixture, case.scenario == "session_persistence_failure")?;
            fixture.clear_outputs()?;
            run_cli(&fixture, false, true, &fixture.metrics, false).await?
        }
        "stop_preserves_accepted" => {
            #[cfg(unix)]
            {
                run_cli_with_durable_timeout_premise(&fixture, metrics_path).await?
            }
            #[cfg(windows)]
            {
                run_cli(&fixture, true, false, metrics_path, true).await?
            }
        }
        "sessionless_complete" | "session_complete" | "metrics_write_failure" => {
            run_cli(&fixture, false, false, metrics_path, false).await?
        }
        scenario => {
            return Err(format!(
                "strict scenario {scenario} has no implementation adapter"
            ));
        }
    };
    observe_run(&fixture, &run, has_session, metrics_path)
}

fn report_results(
    document: &serde_json::Value,
    executed_roles: &BTreeSet<String>,
) -> Result<(Vec<ResultObservation>, BTreeMap<String, String>), String> {
    let mutants = document["mutants"]
        .as_array()
        .ok_or_else(|| "report mutants is not an array".to_owned())?;
    let mut roles = BTreeMap::new();
    let mut results = Vec::new();
    for (index, mutant) in mutants.iter().enumerate() {
        let symbol = mutant["candidate"]["symbol"]
            .as_str()
            .ok_or_else(|| format!("mutant at index {index} has no symbol"))?;
        let role = match symbol {
            "first" => "m0".to_owned(),
            "second" => "m1".to_owned(),
            _ => return Err(format!("report has unsupported candidate symbol {symbol}")),
        };
        let id = mutant["candidate"]["id"]
            .as_str()
            .ok_or_else(|| format!("mutant {role} has no candidate id"))?;
        let status = mutant["status"]
            .as_str()
            .filter(|status| known_status(status))
            .ok_or_else(|| format!("mutant {role} has unknown status"))?;
        if roles.insert(id.to_owned(), role.clone()).is_some()
            || results
                .iter()
                .any(|result: &ResultObservation| result.mutant == role)
        {
            return Err(format!("report repeats semantic role {role}"));
        }
        results.push(ResultObservation {
            mutant: role.clone(),
            status: status.to_owned(),
            executed: status != "not_run" && executed_roles.contains(&role),
        });
    }
    Ok((sorted(results), roles))
}

fn marker_roles(path: &Path) -> Result<Vec<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text.lines().map(str::to_owned).collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("read execution marker: {error}")),
    }
}

fn session_observation(
    path: &Path,
    roles: &BTreeMap<String, String>,
    executed_roles: &BTreeSet<String>,
) -> Result<(Vec<ResultObservation>, bool, bool, Option<String>), String> {
    let connection = Connection::open(path).map_err(|error| format!("open session: {error}"))?;
    let (run_id, finished, complete): (String, bool, bool) = connection
        .query_row(
            "SELECT run_id, finished, complete FROM runs ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|error| format!("read latest run: {error}"))?;
    let mut statement = connection
        .prepare("SELECT mutant_id, status FROM results WHERE run_id=?1 ORDER BY mutant_id")
        .map_err(|error| format!("prepare result query: {error}"))?;
    let rows = statement
        .query_map([&run_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("query results: {error}"))?;
    let mut durable = Vec::new();
    for row in rows {
        let (id, status) = row.map_err(|error| format!("read result: {error}"))?;
        let role = roles
            .get(&id)
            .ok_or_else(|| format!("durable result {id} is absent from report"))?
            .clone();
        durable.push(ResultObservation {
            executed: executed_roles.contains(&role),
            mutant: role,
            status,
        });
    }
    Ok((sorted(durable), finished, complete, Some(run_id)))
}

fn diagnostics(stderr: &str) -> Vec<String> {
    let metrics = stderr.lines().any(|line| line.contains("metrics.write"));
    let persistence = stderr.lines().any(|line| {
        line.contains("session") && (line.contains("persist") || line.contains("commit"))
    });
    let mut values = Vec::new();
    if persistence {
        values.push("persistence_failed".to_owned());
    }
    if metrics {
        values.push("metrics_failed".to_owned());
    }
    values
}

fn observe_run(
    fixture: &Fixture,
    run: &FixtureRun,
    has_session: bool,
    metrics_path: &Path,
) -> Result<(ImplementationObservation, bool), String> {
    let executed_roles = marker_roles(&fixture.marker)?;
    let executed_role_set = executed_roles.iter().cloned().collect::<BTreeSet<_>>();
    let (reported, roles) = report_results(&run.document, &executed_role_set)?;
    let (durable, session_finished, session_complete, session_run_id) = if has_session {
        session_observation(&fixture.session, &roles, &executed_role_set)?
    } else {
        (Vec::new(), false, false, None)
    };
    let metrics = if metrics_path.is_file() {
        let text = std::fs::read_to_string(metrics_path)
            .map_err(|error| format!("read metrics: {error}"))?;
        let metrics: hoimin_core::RunMetrics =
            serde_json::from_str(&text).map_err(|error| format!("parse metrics: {error}"))?;
        metrics
            .validate()
            .map_err(|error| format!("validate metrics: {error}"))?;
        Some(metrics)
    } else {
        None
    };
    let report_run_id = run.document["run"]["run_id"].as_str();
    let result_run_ids_match = run.document["mutants"].as_array().is_some_and(|mutants| {
        mutants
            .iter()
            .all(|mutant| mutant["run_id"].as_str() == report_run_id)
    });
    let run_ids_consistent = report_run_id.is_some()
        && run.document["summary"]["run_id"].as_str() == report_run_id
        && run.document["baseline"]["run_id"].as_str() == report_run_id
        && session_run_id
            .as_deref()
            .is_none_or(|id| Some(id) == report_run_id)
        && metrics
            .as_ref()
            .is_none_or(|value| Some(value.run_id.as_str()) == report_run_id)
        && result_run_ids_match;
    let summary = report_summary_counts(&run.document)?;
    let normalized_diagnostics = diagnostics(&run.stderr);
    let stopped = run.exit_code == 4 && reported.iter().any(|result| result.status == "not_run");
    Ok((
        ImplementationObservation {
            executed: executed_roles,
            durable,
            reported,
            summary,
            metrics_executed: metrics.as_ref().map(|value| value.executed),
            stopped,
            session_finished,
            session_complete,
            metrics_finished: metrics.is_some()
                || normalized_diagnostics.contains(&"metrics_failed".to_owned()),
            run_complete: run.document["summary"]["complete"]
                .as_bool()
                .ok_or_else(|| "summary complete is not a bool".to_owned())?,
            returned: true,
            exit_code: run.exit_code,
            diagnostics: normalized_diagnostics,
        },
        run_ids_consistent,
    ))
}

async fn run_case(case: &OracleCase) -> CaseResult {
    let expected = expected_observation(case);
    let owned = case.clone();
    let execution = tokio::spawn(async move { execute_strict(&owned).await })
        .await
        .map_err(|error| format!("case task panicked: {error}"));
    match execution {
        Ok(Ok((actual, run_ids_consistent))) => CaseResult {
            id: case.id.clone(),
            class: if actual == expected && run_ids_consistent {
                CaseClass::Match
            } else {
                CaseClass::Mismatch
            },
            expected: Some(expected),
            actual: Some(actual),
            detail: (!run_ids_consistent)
                .then_some("run IDs disagree across public surfaces".to_owned()),
        },
        Ok(Err(error)) | Err(error) => CaseResult {
            id: case.id.clone(),
            class: CaseClass::InfrastructureError,
            expected: Some(expected),
            actual: None,
            detail: Some(error),
        },
    }
}

#[test]
fn result_lifecycle_corpus_is_typed_and_complete() {
    let cases = parse_corpus(corpus_text()).expect("valid Lean corpus");
    assert_eq!(cases.len(), 9);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        9
    );
    assert!(cases.iter().any(|case| case.mode == "strict"));
    assert!(cases.iter().any(|case| case.mode == "model-only"));
    assert!(cases.iter().any(|case| case.mode == "internal-fixture"));
}

#[test]
fn result_lifecycle_parser_rejects_schema_drift_and_duplicate_ids() {
    let first = corpus_text().lines().next().expect("nonempty corpus");
    let mut unknown_field: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown_field["unexpected"] = serde_json::json!(true);
    assert!(parse_corpus(&unknown_field.to_string()).is_err());

    let mut unknown_event: serde_json::Value = serde_json::from_str(first).unwrap();
    unknown_event["schedule"][0] = serde_json::json!("accept:m9:killed");
    assert!(parse_corpus(&unknown_event.to_string()).is_err());

    assert!(parse_corpus(&format!("{first}\n{first}\n")).is_err());
}

#[test]
fn execution_marker_preserves_duplicate_completions() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("executed");
    std::fs::write(&marker, "m0\nm0\n").unwrap();

    let roles = marker_roles(&marker).unwrap();

    assert_eq!(roles, ["m0".to_owned(), "m0".to_owned()]);
}

#[test]
fn report_summary_rejects_unknown_and_noninteger_counts() {
    let valid = serde_json::json!({"summary": {"counts": {
        "killed": 1, "survived": 0, "timeout": 0, "out_of_memory": 0,
        "process_limit": 0, "error": 0, "not_run": 0, "inconclusive": 0,
        "score": 1.0
    }}});
    let mut unknown = valid.clone();
    unknown["summary"]["counts"]["future_status"] = serde_json::json!(1);
    assert_eq!(
        report_summary_counts(&unknown).unwrap_err(),
        "summary counts contains unknown status future_status"
    );

    let mut fractional = valid;
    fractional["summary"]["counts"]["killed"] = serde_json::json!(1.5);
    assert_eq!(
        report_summary_counts(&fractional).unwrap_err(),
        "summary count for killed is not a nonnegative integer"
    );

    let inconsistent = serde_json::json!({"summary": {"counts": {
        "killed": 0, "survived": 0, "timeout": 1, "out_of_memory": 0,
        "process_limit": 0, "error": 0, "not_run": 0, "inconclusive": 0,
        "score": null
    }}});
    assert_eq!(
        report_summary_counts(&inconsistent).unwrap_err(),
        "summary inconclusive count Some(0) disagrees with derived count 1"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn bounded_cli_timeout_terminates_descendants() {
    use std::os::unix::process::CommandExt;

    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("descendant.pid");
    let mut command = tokio::process::Command::new("sh");
    command
        .args([
            "-c",
            "trap '' INT; sleep 20 & echo $! > \"$DESCENDANT_PID_MARKER\"; wait",
        ])
        .env("DESCENDANT_PID_MARKER", &marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command.as_std_mut().process_group(0);

    let error = bounded_cli_output(&mut command, Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    let pid = std::fs::read_to_string(&marker)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            // SAFETY: signal zero only checks the fixture descendant recorded above.
            if unsafe { libc::kill(pid, 0) } != 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(stopped.is_ok(), "fixture descendant {pid} survived cleanup");
}

#[tokio::test]
async fn result_lifecycle_oracle_correspondence() {
    let cases = parse_corpus(corpus_text()).expect("valid Lean corpus");
    let mode = std::env::var(MODE_ENV).unwrap_or_else(|_| "strict".to_owned());
    assert!(
        matches!(mode.as_str(), "strict" | "report"),
        "unknown {MODE_ENV}={mode}"
    );
    let selected = std::env::var(CASE_ENV).ok();
    if let Some(id) = &selected {
        assert!(
            cases.iter().any(|case| &case.id == id),
            "unknown {CASE_ENV}={id}"
        );
    }
    let selected_cases = cases
        .iter()
        .filter(|case| selected.as_ref().is_none_or(|id| &case.id == id))
        .collect::<Vec<_>>();
    if mode == "strict"
        && selected.is_some()
        && let Some(case) = selected_cases.iter().find(|case| case.mode != "strict")
    {
        panic!(
            "{CASE_ENV}={} has mode {}; only strict cases are executable",
            case.id, case.mode
        );
    }
    let strict = selected_cases
        .iter()
        .copied()
        .filter(|case| case.mode == "strict")
        .collect::<Vec<_>>();
    assert!(
        mode == "report" || !strict.is_empty(),
        "no executable strict result lifecycle cases selected"
    );
    let mut failures = Vec::new();
    for case in strict {
        let result = run_case(case).await;
        eprintln!("result-lifecycle {}: {:?}", result.id, result.class);
        let classification_consistent = match (&result.class, &result.expected, &result.actual) {
            (CaseClass::Match, Some(expected), Some(actual)) => expected == actual,
            (CaseClass::Mismatch, Some(expected), Some(actual)) => expected != actual,
            (CaseClass::InfrastructureError, Some(_), None) => result.detail.is_some(),
            _ => false,
        };
        if result.class != CaseClass::Match || !classification_consistent {
            eprintln!("  expected={:#?}", result.expected);
            eprintln!("  actual={:#?}", result.actual);
            eprintln!("  detail={:#?}", result.detail);
            failures.push(result.id);
        }
    }
    if mode == "report" {
        for case in selected_cases.iter().filter(|case| case.mode != "strict") {
            eprintln!(
                "result-lifecycle {}: NotExecuted (mode={}, evidence={})",
                case.id, case.mode, case.scenario
            );
        }
    }
    if mode == "strict" {
        assert!(
            failures.is_empty(),
            "result lifecycle mismatches: {}",
            failures.join(", ")
        );
    }
}
