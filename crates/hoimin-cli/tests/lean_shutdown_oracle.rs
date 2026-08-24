use std::collections::HashSet;
#[cfg(unix)]
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use serde::Deserialize;
#[cfg(unix)]
use tokio::io::AsyncReadExt;

#[cfg(unix)]
const REVIEWED_MISMATCHES: &[&str] = &[];
const EVENTS: &[&str] = &[
    "boot",
    "start_process",
    "first_interrupt",
    "second_interrupt",
    "deadline_reached",
    "process_exited",
    "process_failed",
    "infrastructure_failed",
    "request_termination",
    "reap_process",
    "start_output_drain",
    "output_drained",
    "output_failed",
    "start_blocking",
    "blocking_completed",
    "detach_blocking",
    "start_cleanup",
    "cleanup_completed",
    "cleanup_failed",
    "start_session_finish",
    "session_finished",
    "session_failed",
    "start_report",
    "report_written",
    "report_failed",
    "start_metrics",
    "metrics_written",
    "metrics_failed",
    "metrics_skipped",
    "return_success",
    "return_failure",
];
const COMPONENTS: &[&str] = &["absent", "pending", "complete", "failed", "detached"];
const PROCESSES: &[&str] = &[
    "not_started",
    "running",
    "termination_requested",
    "exited",
    "reaped",
];
const CAUSES: &[&str] = &[
    "interrupt",
    "forced_interrupt",
    "deadline",
    "process_failure",
    "infrastructure_failure",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalObservation {
    cause: Option<String>,
    exit_code: Option<u64>,
    process: String,
    output: String,
    blocking: String,
    workspace: String,
    session: String,
    report: String,
    metrics: String,
    primary_error: Option<String>,
    appended_errors: Vec<String>,
    dispatches: [u64; 4],
    session_complete_flag: bool,
    report_complete_flag: bool,
    returned: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    schedule: Vec<String>,
    expected: TerminalObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(unix)]
enum CaseClass {
    Match,
    Mismatch,
    InfrastructureError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CliObservation {
    exit_code: Option<i32>,
    report: String,
    session: String,
    descendant_running: bool,
    bounded_exit: bool,
}

#[derive(Debug)]
#[cfg(unix)]
struct CaseResult {
    id: String,
    class: CaseClass,
    expected: CliObservation,
    actual: Option<CliObservation>,
    detail: Option<String>,
}

fn corpus_text() -> &'static str {
    include_str!("../../../formal/HoiminOracle/corpus/shutdown-orchestration.jsonl")
}

fn one_of(value: &str, allowed: &[&str], field: &str) -> Result<(), String> {
    allowed
        .contains(&value)
        .then_some(())
        .ok_or_else(|| format!("unknown {field}: {value}"))
}

fn validate_case(case: &OracleCase) -> Result<(), String> {
    if case.schema != 1 {
        return Err(format!("unsupported schema: {}", case.schema));
    }
    one_of(
        &case.mode,
        &["strict", "internal-fixture", "model-only"],
        "mode",
    )?;
    one_of(
        &case.scenario,
        &[
            "normal_completion",
            "first_interrupt_running",
            "total_timeout_reaps_descendant",
            "second_interrupt_blocked_finish",
            "total_timeout_blocked_finish",
            "process_exit_vs_cancellation",
            "cleanup_failure_after_process_failure",
            "output_eof_vs_expiry",
            "cleanup_completion_vs_expiry",
            "session_finish_vs_late_cancellation",
            "report_write_vs_late_error",
            "blocking_completion_vs_detach",
        ],
        "scenario",
    )?;
    if case.schedule.is_empty() {
        return Err(format!("{} has an empty schedule", case.id));
    }
    for event in &case.schedule {
        one_of(event, EVENTS, "event")?;
    }
    one_of(&case.expected.process, PROCESSES, "process state")?;
    for state in [
        &case.expected.output,
        &case.expected.blocking,
        &case.expected.workspace,
        &case.expected.session,
        &case.expected.report,
        &case.expected.metrics,
    ] {
        one_of(state, COMPONENTS, "component state")?;
    }
    for cause in case
        .expected
        .cause
        .iter()
        .chain(case.expected.primary_error.iter())
        .chain(case.expected.appended_errors.iter())
    {
        one_of(cause, CAUSES, "stop cause")?;
    }
    if case.expected.dispatches.iter().any(|&count| count > 1) {
        return Err("singleton dispatch count exceeds one".to_owned());
    }
    if case.expected.session_complete_flag && case.expected.session != "complete" {
        return Err("session completion flag contradicts session state".to_owned());
    }
    if case.expected.report_complete_flag && case.expected.report != "complete" {
        return Err("report completion flag contradicts report state".to_owned());
    }
    Ok(())
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut cases = Vec::new();
    let mut ids = HashSet::new();
    for (index, line) in input.lines().enumerate() {
        let case: OracleCase =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        validate_case(&case)?;
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id: {}", case.id));
        }
        cases.push(case);
    }
    if cases.len() != 12 {
        return Err(format!("expected 12 cases, found {}", cases.len()));
    }
    Ok(cases)
}

fn project_strict(case: &OracleCase) -> Result<CliObservation, String> {
    let complete = case.expected.cause.is_none();
    if !case.expected.returned || case.expected.exit_code.is_none() {
        return Err(format!("{} has no observable terminal exit", case.id));
    }
    let exit_code = case
        .expected
        .exit_code
        .map(i32::try_from)
        .transpose()
        .map_err(|error| format!("exit code is not observable as i32: {error}"))?;
    Ok(CliObservation {
        exit_code,
        report: if complete { "complete" } else { "incomplete" }.to_owned(),
        session: if case.expected.session_complete_flag {
            "complete"
        } else {
            "incomplete"
        }
        .to_owned(),
        descendant_running: false,
        bounded_exit: true,
    })
}

#[cfg(unix)]
struct FixturePaths {
    _project_guard: tempfile::TempDir,
    _coordinator_guard: tempfile::TempDir,
    project: PathBuf,
    session: PathBuf,
    active: PathBuf,
    descendant_marker: PathBuf,
}

#[cfg(unix)]
impl FixturePaths {
    fn create() -> Result<Self, String> {
        let project_guard = tempfile::tempdir().map_err(|error| error.to_string())?;
        let coordinator_guard = tempfile::tempdir().map_err(|error| error.to_string())?;
        write_parallel_project(project_guard.path())?;
        let active = coordinator_guard.path().join("active");
        std::fs::create_dir(&active).map_err(|error| error.to_string())?;
        Ok(Self {
            project: project_guard.path().to_owned(),
            session: coordinator_guard.path().join("session.sqlite3"),
            descendant_marker: coordinator_guard.path().join("descendant-ready"),
            active,
            _project_guard: project_guard,
            _coordinator_guard: coordinator_guard,
        })
    }
}

#[derive(Clone, Debug)]
#[cfg(unix)]
struct FixtureProcesses {
    pids: Vec<u32>,
}

#[cfg(unix)]
fn write_parallel_project(root: &Path) -> Result<(), String> {
    let source = root.join("src");
    std::fs::create_dir_all(&source).map_err(|error| error.to_string())?;
    std::fs::write(source.join("__init__.py"), "").map_err(|error| error.to_string())?;
    std::fs::write(
        source.join("calc.py"),
        "def total(a, b, c, d, e):\n    return a + b + c + d + e\n",
    )
    .map_err(|error| error.to_string())
}

#[cfg(unix)]
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is nested below repository root")
        .to_owned()
}

#[cfg(unix)]
fn python_executable() -> PathBuf {
    let executable = repo_root().join(".venv/bin/python");
    assert!(
        executable.is_file(),
        "missing controlled Python interpreter: {}",
        executable.display()
    );
    executable
}

#[cfg(unix)]
fn spawn_scenario(
    case: &OracleCase,
    paths: &FixturePaths,
) -> Result<tokio::process::Child, String> {
    let original = "return a + b + c + d + e";
    let test_command = if case.scenario == "normal_completion" {
        "from src.calc import total; assert total(1,2,3,4,5) == 15".to_owned()
    } else {
        let descendant_temp = paths.descendant_marker.with_extension("tmp");
        let descendant = format!(
            "from pathlib import Path; import os,signal,time; signal.signal(signal.SIGINT, signal.SIG_IGN); p=Path({:?}); p.write_text(str(os.getpid())); p.replace({:?}); time.sleep(20)",
            descendant_temp.to_string_lossy(),
            paths.descendant_marker.to_string_lossy(),
        );
        let mutant = format!(
            "from pathlib import Path; import os,signal,subprocess,sys,time; signal.signal(signal.SIGINT, signal.SIG_IGN); Path({:?},str(os.getpid())).write_text('running'); time.sleep(0.3); subprocess.Popen([sys.executable,'-c',{:?}]); time.sleep(20)",
            paths.active.to_string_lossy(),
            descendant,
        );
        format!(
            "from pathlib import Path; source=Path('src/calc.py').read_text(); exec({mutant:?}) if {original:?} not in source else exec('from src.calc import total; assert total(1,2,3,4,5) == 15')"
        )
    };
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hoimin"));
    command
        .arg("run")
        .arg("--root")
        .arg(&paths.project)
        .arg("--source")
        .arg("src")
        .arg("--file")
        .arg("src/calc.py")
        .arg("--jobs")
        .arg("1")
        .arg("--session")
        .arg(&paths.session)
        .arg("--format")
        .arg("json")
        .arg("--allow-best-effort-memory");
    if case.scenario == "total_timeout_reaps_descendant" {
        command.arg("--total-timeout").arg("1s");
    }
    command
        .arg("--")
        .arg(python_executable())
        .arg("-c")
        .arg(test_command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("spawn hoimin: {error}"))
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    i32::try_from(pid).is_ok_and(|pid| unsafe { libc::kill(pid, 0) } == 0)
}

#[cfg(unix)]
async fn wait_for_readiness(
    paths: &FixturePaths,
    child: &mut tokio::process::Child,
) -> Result<FixtureProcesses, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!("hoimin exited before fixture readiness: {status}"));
        }
        let descendant = std::fs::read_to_string(&paths.descendant_marker)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok());
        let active = std::fs::read_dir(&paths.active).ok().and_then(|entries| {
            entries
                .flatten()
                .find_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        });
        if let (Some(active), Some(descendant)) = (active, descendant)
            && process_is_alive(active)
            && process_is_alive(descendant)
        {
            return Ok(FixtureProcesses {
                pids: vec![active, descendant],
            });
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("fixture processes did not become ready".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
fn send_first_interrupt(child: &mut tokio::process::Child) -> Result<(), String> {
    let pid = child
        .id()
        .ok_or_else(|| "hoimin exited before SIGINT".to_owned())?;
    let pid = i32::try_from(pid).map_err(|error| error.to_string())?;
    if unsafe { libc::kill(pid, libc::SIGINT) } == 0 {
        Ok(())
    } else {
        Err(format!(
            "SIGINT failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(unix)]
async fn reap_fixture_processes(processes: &FixtureProcesses) -> Result<(), String> {
    for &pid in &processes.pids {
        if !process_is_alive(pid) {
            continue;
        }
        let native = i32::try_from(pid).map_err(|error| error.to_string())?;
        if unsafe { libc::kill(native, libc::SIGKILL) } != 0 {
            return Err(format!(
                "failed to kill fixture process {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while process_is_alive(pid) && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if process_is_alive(pid) {
            return Err(format!("fixture process {pid} survived teardown"));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn observe_report(bytes: &[u8]) -> Result<String, String> {
    let report: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "invalid JSON report: {error}; output={:?}",
            String::from_utf8_lossy(bytes)
        )
    })?;
    report["summary"]["complete"]
        .as_bool()
        .map(|complete| if complete { "complete" } else { "incomplete" }.to_owned())
        .ok_or_else(|| "report has no boolean summary.complete".to_owned())
}

#[cfg(unix)]
fn observe_session(path: &Path) -> Result<String, String> {
    let complete: i64 = rusqlite::Connection::open(path)
        .map_err(|error| error.to_string())?
        .query_row("SELECT complete FROM runs", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    match complete {
        0 => Ok("incomplete".to_owned()),
        1 => Ok("complete".to_owned()),
        value => Err(format!("invalid session complete value: {value}")),
    }
}

#[cfg(unix)]
async fn execute_strict_case(case: &OracleCase) -> Result<CliObservation, String> {
    let paths = FixturePaths::create()?;
    let mut child = spawn_scenario(case, &paths)?;
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| "missing stdout pipe".to_owned())?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| "missing stderr pipe".to_owned())?;
    let stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout_pipe.read_to_end(&mut bytes).await.map(|_| bytes)
    });
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr_pipe.read_to_end(&mut bytes).await.map(|_| bytes)
    });
    let started = Instant::now();
    let mut processes = None;
    let outcome: Result<_, String> = async {
        if case.scenario != "normal_completion" {
            processes = Some(wait_for_readiness(&paths, &mut child).await?);
        }
        if case.scenario == "first_interrupt_running" {
            send_first_interrupt(&mut child)?;
        }
        let status = tokio::time::timeout(Duration::from_secs(12), child.wait())
            .await
            .map_err(|_| "hoimin exceeded bounded exit".to_owned())?
            .map_err(|error| error.to_string())?;
        Ok(status)
    }
    .await;
    if child
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_none()
    {
        child.start_kill().map_err(|error| error.to_string())?;
        let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    }
    if let Some(processes) = &processes {
        reap_fixture_processes(processes).await?;
    }
    let stdout = stdout_task
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    let stderr = stderr_task
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    let status =
        outcome.map_err(|error| format!("{error}; stderr={}", String::from_utf8_lossy(&stderr)))?;
    let descendant_running = processes
        .as_ref()
        .is_some_and(|items| items.pids.iter().any(|&pid| process_is_alive(pid)));
    Ok(CliObservation {
        exit_code: status.code(),
        report: observe_report(&stdout)?,
        session: observe_session(&paths.session)?,
        descendant_running,
        bounded_exit: started.elapsed() < Duration::from_secs(12),
    })
}

#[cfg(unix)]
async fn run_strict_case(case: &OracleCase) -> CaseResult {
    let expected = match project_strict(case) {
        Ok(expected) => expected,
        Err(error) => {
            return CaseResult {
                id: case.id.clone(),
                class: CaseClass::InfrastructureError,
                expected: CliObservation {
                    exit_code: None,
                    report: String::new(),
                    session: String::new(),
                    descendant_running: false,
                    bounded_exit: false,
                },
                actual: None,
                detail: Some(error),
            };
        }
    };
    let owned = case.clone();
    let execution = tokio::spawn(async move { execute_strict_case(&owned).await }).await;
    let actual = match execution {
        Ok(Ok(actual)) => actual,
        Ok(Err(error)) => {
            return CaseResult {
                id: case.id.clone(),
                class: CaseClass::InfrastructureError,
                expected,
                actual: None,
                detail: Some(error),
            };
        }
        Err(error) => {
            return CaseResult {
                id: case.id.clone(),
                class: CaseClass::InfrastructureError,
                expected,
                actual: None,
                detail: Some(format!("case panicked: {error}")),
            };
        }
    };
    let class = if actual == expected {
        CaseClass::Match
    } else {
        CaseClass::Mismatch
    };
    CaseResult {
        id: case.id.clone(),
        class,
        detail: (class == CaseClass::Mismatch)
            .then(|| format!("expected={expected:?} actual={actual:?}")),
        expected,
        actual: Some(actual),
    }
}

#[test]
fn corpus_is_well_formed() {
    let cases = parse_corpus(corpus_text()).expect("valid shutdown corpus");
    assert_eq!(cases.iter().filter(|case| case.mode == "strict").count(), 3);
}

fn corpus_with_first_case_changed(change: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut lines = corpus_text().lines().map(str::to_owned).collect::<Vec<_>>();
    let mut value: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    change(&mut value);
    lines[0] = serde_json::to_string(&value).unwrap();
    lines.join("\n")
}

#[test]
fn corpus_rejects_unknown_schema_mode_scenario_event_and_state() {
    for (field, value) in [
        ("schema", serde_json::json!(2)),
        ("mode", serde_json::json!("future")),
        ("scenario", serde_json::json!("future")),
    ] {
        let input = corpus_with_first_case_changed(|case| case[field] = value);
        assert!(parse_corpus(&input).is_err(), "accepted invalid {field}");
    }
    let event =
        corpus_with_first_case_changed(|case| case["schedule"][0] = serde_json::json!("future"));
    assert!(parse_corpus(&event).is_err());
    let state = corpus_with_first_case_changed(|case| {
        case["expected"]["process"] = serde_json::json!("future");
    });
    assert!(parse_corpus(&state).is_err());
}

#[test]
fn corpus_rejects_duplicate_case_ids() {
    let mut lines = corpus_text().lines().map(str::to_owned).collect::<Vec<_>>();
    let first: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    let mut second: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    second["id"] = first["id"].clone();
    lines[1] = serde_json::to_string(&second).unwrap();
    assert!(parse_corpus(&lines.join("\n")).is_err());
}

#[test]
fn strict_projection_rejects_unobservable_expectations() {
    let mut case = parse_corpus(corpus_text()).unwrap().remove(0);
    case.expected.returned = false;
    assert!(project_strict(&case).is_err());
}

#[tokio::test]
async fn case_panics_are_infrastructure_errors_and_teardown_runs() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Teardown(Arc<AtomicBool>);
    impl Drop for Teardown {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let guard_flag = Arc::clone(&dropped);
    let joined = tokio::spawn(async move {
        let _guard = Teardown(guard_flag);
        panic!("injected adapter panic");
    })
    .await;
    assert!(joined.is_err_and(|error| error.is_panic()));
    assert!(dropped.load(Ordering::SeqCst));
}

#[cfg(unix)]
#[tokio::test]
async fn oracle_correspondence() {
    let cases = parse_corpus(corpus_text()).expect("valid shutdown corpus");
    let selected = std::env::var("HOIMIN_SHUTDOWN_ORACLE_CASE").ok();
    if let Some(id) = &selected {
        let case = cases
            .iter()
            .find(|case| &case.id == id)
            .unwrap_or_else(|| panic!("unknown oracle case: {id}"));
        assert_eq!(case.mode, "strict", "selected case is not strict: {id}");
    }
    let mut results = Vec::new();
    for case in cases
        .iter()
        .filter(|case| case.mode == "strict" && selected.as_ref().is_none_or(|id| id == &case.id))
    {
        results.push(run_strict_case(case).await);
    }
    let infrastructure = results
        .iter()
        .filter(|result| result.class == CaseClass::InfrastructureError)
        .map(|result| {
            format!(
                "{}: {}",
                result.id,
                result.detail.as_deref().unwrap_or("unknown")
            )
        })
        .collect::<Vec<_>>();
    assert!(
        infrastructure.is_empty(),
        "infrastructure errors: {infrastructure:?}"
    );
    let mismatches = results
        .iter()
        .filter(|result| result.class == CaseClass::Mismatch)
        .map(|result| result.id.as_str())
        .collect::<Vec<_>>();
    let mismatch_detail = results
        .iter()
        .filter(|result| result.class == CaseClass::Mismatch)
        .map(|result| {
            format!(
                "{}: expected={:?} actual={:?}",
                result.id, result.expected, result.actual
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(mismatches, REVIEWED_MISMATCHES, "{mismatch_detail:?}");
}
