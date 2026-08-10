use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Deserialize;

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
struct ExpectedObservation {
    accepted: Vec<ResultObservation>,
    durable: Vec<ResultObservation>,
    reported: Vec<ResultObservation>,
    summary: Vec<String>,
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
struct ImplementationObservation {
    accepted: Vec<ResultObservation>,
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
    run_ids_consistent: bool,
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
    _temp: tempfile::TempDir,
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
        ["discover", "m0" | "m1"]
        | [
            "persist_ok" | "persist_failed" | "record_result" | "report_ok" | "report_failed",
            "m0" | "m1",
        ]
        | ["mark_not_run", "m0" | "m1"] => true,
        ["accept", "m0" | "m1", status] => known_status(status),
        ["finish_session", "true" | "false"] => true,
        ["stop" | "finish_metrics" | "metrics_failed" | "return_run"] => true,
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
    if case.id.is_empty() || case.scenario.is_empty() || case.schedule.is_empty() {
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
        != !case
            .expected
            .diagnostics
            .contains(&"metrics_failed".to_owned())
    {
        return Err(format!(
            "case {} has inconsistent metrics observability",
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

fn expected_observation(case: &OracleCase) -> ImplementationObservation {
    ImplementationObservation {
        accepted: sorted(case.expected.accepted.clone()),
        durable: sorted(case.expected.durable.clone()),
        reported: sorted(case.expected.reported.clone()),
        summary: summary_counts(&case.expected.summary),
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
        run_ids_consistent: true,
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
            _temp: temp,
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
        let second_probe = if two_mutants {
            "mutated_second='return second_left - second_right' in source; marker.open('a').write('m1\\n') if mutated_second else None; time.sleep(20) if mutated_second else None; from src.calc import first,second; assert first(3,2)==5; assert second(3,2)==5"
        } else {
            "from src.calc import first; assert first(3,2)==5"
        };
        format!(
            "from pathlib import Path; import time; source=Path('src/calc.py').read_text(); marker=Path({marker:?}); mutated_first='return first_left - first_right' in source; marker.open('a').write('m0\\n') if mutated_first else None; {second_probe}"
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

async fn run_cli(
    fixture: &Fixture,
    two_mutants: bool,
    resume: bool,
    metrics_path: &Path,
    total_timeout: bool,
) -> Result<FixtureRun, String> {
    let python = python_executable()?;
    let mut args = vec![
        OsString::from("hoimin"),
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
    if total_timeout {
        args.extend([
            OsString::from("--total-timeout"),
            OsString::from("1s"),
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
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    let stdout =
        String::from_utf8(stdout).map_err(|error| format!("stdout is not UTF-8: {error}"))?;
    let stderr =
        String::from_utf8(stderr).map_err(|error| format!("stderr is not UTF-8: {error}"))?;
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

async fn execute_strict(case: &OracleCase) -> Result<ImplementationObservation, String> {
    let two_mutants = case.scenario == "stop_preserves_accepted";
    let fixture = Fixture::new(two_mutants)?;
    let has_session = case.setup.session;
    if has_session {
        enable_session(&fixture)?;
    }
    let metrics_directory = fixture._temp.path().join("metrics-target");
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
        "stop_preserves_accepted" => run_cli(&fixture, true, false, metrics_path, true).await?,
        "sessionless_complete" | "session_complete" | "metrics_write_failure" => {
            run_cli(&fixture, false, false, metrics_path, false).await?
        }
        scenario => {
            return Err(format!(
                "strict scenario {scenario} has no implementation adapter"
            ));
        }
    };
    observe_run(
        &fixture,
        &run,
        has_session,
        metrics_path,
        case.expected.stopped,
    )
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
        let role = format!("m{index}");
        if !matches!(role.as_str(), "m0" | "m1") {
            return Err(format!("report has unsupported mutant role {role}"));
        }
        let id = mutant["candidate"]["id"]
            .as_str()
            .ok_or_else(|| format!("mutant {role} has no candidate id"))?;
        let status = mutant["status"]
            .as_str()
            .filter(|status| known_status(status))
            .ok_or_else(|| format!("mutant {role} has unknown status"))?;
        roles.insert(id.to_owned(), role.clone());
        results.push(ResultObservation {
            mutant: role.clone(),
            status: status.to_owned(),
            executed: status != "not_run" && executed_roles.contains(&role),
        });
    }
    Ok((sorted(results), roles))
}

fn marker_roles(path: &Path) -> Result<BTreeSet<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text.lines().map(str::to_owned).collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
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
    expected_stopped: bool,
) -> Result<ImplementationObservation, String> {
    let executed_roles = marker_roles(&fixture.marker)?;
    let (reported, roles) = report_results(&run.document, &executed_roles)?;
    let accepted = sorted(
        reported
            .iter()
            .filter(|result| result.executed)
            .cloned()
            .collect(),
    );
    let (durable, session_finished, session_complete, session_run_id) = if has_session {
        session_observation(&fixture.session, &roles, &executed_roles)?
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
    let summary = run.document["summary"]["counts"]
        .as_object()
        .ok_or_else(|| "summary counts is not an object".to_owned())?
        .iter()
        .filter_map(|(status, count)| {
            known_status(status).then(|| (status.clone(), count.as_u64().unwrap_or(u64::MAX)))
        })
        .filter(|(_, count)| *count > 0)
        .collect();
    let normalized_diagnostics = diagnostics(&run.stderr);
    Ok(ImplementationObservation {
        accepted,
        durable,
        reported,
        summary,
        metrics_executed: metrics.as_ref().map(|value| value.executed),
        stopped: expected_stopped && run.exit_code == 4,
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
        run_ids_consistent,
    })
}

async fn run_case(case: &OracleCase) -> CaseResult {
    let expected = expected_observation(case);
    match execute_strict(case).await {
        Ok(actual) => CaseResult {
            id: case.id.clone(),
            class: if actual == expected {
                CaseClass::Match
            } else {
                CaseClass::Mismatch
            },
            expected: Some(expected),
            actual: Some(actual),
            detail: None,
        },
        Err(error) => CaseResult {
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
    let strict = cases
        .iter()
        .filter(|case| case.mode == "strict")
        .filter(|case| selected.as_ref().is_none_or(|id| &case.id == id))
        .collect::<Vec<_>>();
    assert!(
        !strict.is_empty(),
        "no strict result lifecycle cases selected"
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
    if mode == "strict" {
        assert!(
            failures.is_empty(),
            "result lifecycle mismatches: {}",
            failures.join(", ")
        );
    }
}
