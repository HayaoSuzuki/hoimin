use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::Duration;

use hoimin_cli::session::{SchemaError, SessionError, SessionHandler};
use rusqlite::Connection;
use serde::Deserialize;

const CORPUS: &str =
    include_str!("../../../formal/HoiminOracle/corpus/schema-migration-concurrency.jsonl");
const CURRENT_VERSION: i64 = 4;
// Version labels refer to the current-only initialization contract.

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    initial_version: String,
    initial_legacy_row: bool,
    trace: Vec<String>,
    expected_version: String,
    expected_legacy_row: bool,
    expected_open_results: Vec<String>,
}

fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let item: OracleCase = serde_json::from_str(line)
            .map_err(|error| format!("line {} is invalid JSON: {error}", index + 1))?;
        validate_case(&item)?;
        if !ids.insert(item.id.clone()) {
            return Err(format!("duplicate case id {}", item.id));
        }
        cases.push(item);
    }
    if cases.is_empty() {
        return Err("corpus contains no cases".to_owned());
    }
    Ok(cases)
}

fn validate_case(item: &OracleCase) -> Result<(), String> {
    if item.schema != 1
        || item.id.is_empty()
        || !matches!(item.mode.as_str(), "strict" | "model-only")
        || item.expected_open_results.len() != 2
        || item.trace.is_empty()
        || item.trace.iter().any(|event| !known_event(event))
    {
        return Err(format!(
            "{} has an invalid schema initialization case",
            item.id
        ));
    }
    let correspondence_is_exact = match item.scenario.as_str() {
        "concurrent_fresh" => {
            item.mode == "strict"
                && item.initial_version == "fresh"
                && !item.initial_legacy_row
                && item.expected_version == "current"
                && !item.expected_legacy_row
                && results_are(item, "ok", "ok")
        }
        "stale_reread" => {
            item.mode == "model-only"
                && item.initial_version == "fresh"
                && !item.initial_legacy_row
                && item.expected_version == "current"
                && !item.expected_legacy_row
                && results_are(item, "ok", "ok")
        }
        "rollback" => {
            item.mode == "strict"
                && item.initial_version == "fresh"
                && item.initial_legacy_row
                && item.expected_version == "fresh"
                && item.expected_legacy_row
                && results_are(item, "database_error", "not_run")
        }
        "unsupported" => {
            item.mode == "strict"
                && matches!(item.initial_version.as_str(), "v1" | "v2" | "v3" | "future")
                && item.initial_legacy_row
                && item.expected_version == item.initial_version
                && item.expected_legacy_row
                && results_are(item, "unsupported_version", "not_run")
        }
        _ => false,
    };
    if !correspondence_is_exact {
        return Err(format!(
            "{} has a mismatched correspondence premise",
            item.id
        ));
    }
    Ok(())
}
fn results_are(item: &OracleCase, left: &str, right: &str) -> bool {
    item.expected_open_results[0] == left && item.expected_open_results[1] == right
}

fn known_event(value: &str) -> bool {
    let Some((kind, actor)) = value.split_once(':') else {
        return false;
    };
    matches!(kind, "observe" | "begin" | "initialize" | "commit" | "fail")
        && matches!(actor, "left" | "right")
}

#[test]
fn lean_schema_migration_corpus_is_valid() {
    let cases = parse_corpus(CORPUS).expect("Lean schema migration corpus must be valid");
    assert_eq!(cases.len(), 7);
    assert_eq!(cases.iter().filter(|item| item.mode == "strict").count(), 6);
}

#[test]
fn schema_migration_corpus_rejects_unknown_fields_and_duplicate_ids() {
    let extra = CORPUS
        .lines()
        .next()
        .unwrap()
        .replacen('{', "{\"unknown\":true,", 1);
    assert!(parse_corpus(&extra).is_err());

    let first = CORPUS.lines().next().unwrap();
    assert!(parse_corpus(&format!("{first}\n{first}\n")).is_err());

    let wrong_premise = first.replacen(
        "\"initial_version\":\"fresh\"",
        "\"initial_version\":\"v1\"",
        1,
    );
    assert!(parse_corpus(&wrong_premise).is_err());
}

#[test]
fn public_session_open_matches_strict_schema_migration_cases() {
    let cases = parse_corpus(CORPUS).expect("valid Lean schema migration corpus");
    for item in cases.iter().filter(|item| item.mode == "strict") {
        run_strict_case(item).unwrap_or_else(|error| {
            panic!("infrastructure error for schema case {}: {error}", item.id)
        });
    }
}

fn run_strict_case(item: &OracleCase) -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = directory.path().join("session.sqlite3");
    prepare_fixture(item, &path)?;

    let expected_unsupported = if item.scenario == "unsupported" {
        Some(expected_version_number(&item.initial_version)?)
    } else {
        None
    };
    let actual_results = match item.scenario.as_str() {
        "concurrent_fresh" => open_concurrently(&path)?,
        "rollback" | "unsupported" => vec![
            classify_open(SessionHandler::open(&path), expected_unsupported),
            "not_run".to_owned(),
        ],
        scenario => return Err(format!("strict case has unsupported scenario {scenario}")),
    };
    if actual_results != item.expected_open_results {
        return Err(format!(
            "{} open results mismatch: actual={actual_results:?} expected={:?}",
            item.id, item.expected_open_results
        ));
    }

    let connection = Connection::open(&path).map_err(|error| error.to_string())?;
    let actual_version = user_version(&connection)?;
    if actual_version != expected_version_number(&item.expected_version)? {
        return Err(format!(
            "{} version mismatch: actual={actual_version} expected={}",
            item.id, item.expected_version
        ));
    }
    let actual_legacy_row = observe_marker(item, &connection)?;
    if actual_legacy_row != item.expected_legacy_row {
        return Err(format!(
            "{} marker mismatch: actual={actual_legacy_row} expected={}",
            item.id, item.expected_legacy_row
        ));
    }
    Ok(())
}

fn prepare_fixture(item: &OracleCase, path: &Path) -> Result<(), String> {
    match item.scenario.as_str() {
        "concurrent_fresh" => Ok(()),
        "rollback" => {
            let connection = Connection::open(path).map_err(|error| error.to_string())?;
            connection
                .execute_batch(
                    "PRAGMA user_version=0;
                     CREATE TABLE diagnostics (
                         id INTEGER PRIMARY KEY,
                         run_id TEXT NOT NULL,
                         level TEXT NOT NULL,
                         code TEXT NOT NULL,
                         message TEXT NOT NULL
                     );
                     INSERT INTO diagnostics(id,run_id,level,code,message)
                     VALUES (7,'run-1','warning','fixture','preserve me');",
                )
                .map_err(|error| error.to_string())
        }
        "unsupported" => {
            let connection = Connection::open(path).map_err(|error| error.to_string())?;
            connection
                .execute_batch(&format!(
                    "CREATE TABLE unsupported_schema_marker(value TEXT NOT NULL);
                     INSERT INTO unsupported_schema_marker(value) VALUES ('preserve me');
                     PRAGMA user_version={};",
                    expected_version_number(&item.initial_version)?
                ))
                .map_err(|error| error.to_string())
        }
        scenario => Err(format!("cannot prepare strict scenario {scenario}")),
    }
}

fn open_concurrently(path: &Path) -> Result<Vec<String>, String> {
    let start = Arc::new(Barrier::new(3));
    let (sender, receiver) = mpsc::channel();
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.to_owned();
            let start = Arc::clone(&start);
            let sender = sender.clone();
            thread::spawn(move || {
                start.wait();
                let result = classify_open(SessionHandler::open(path), None);
                let _ = sender.send(result);
            })
        })
        .collect();
    drop(sender);
    start.wait();

    let mut results = Vec::with_capacity(2);
    for _ in 0..2 {
        results.push(
            receiver
                .recv_timeout(Duration::from_secs(10))
                .map_err(|error| format!("concurrent open timed out: {error}"))?,
        );
    }
    for handle in handles {
        handle
            .join()
            .map_err(|_| "concurrent open thread panicked".to_owned())?;
    }
    results.sort();
    Ok(results)
}

fn classify_open(
    result: Result<SessionHandler, SessionError>,
    expected_unsupported: Option<i64>,
) -> String {
    match result {
        Ok(handler) => {
            drop(handler);
            "ok".to_owned()
        }
        Err(SessionError::Schema(SchemaError::UnsupportedVersion(version)))
            if expected_unsupported == Some(version) =>
        {
            "unsupported_version".to_owned()
        }
        Err(SessionError::Schema(SchemaError::Database(_))) => "database_error".to_owned(),
        Err(error) => format!("unexpected_error:{error}"),
    }
}

fn expected_version_number(value: &str) -> Result<i64, String> {
    match value {
        "fresh" => Ok(0),
        "v1" => Ok(1),
        "v2" => Ok(2),
        "v3" => Ok(3),
        "current" => Ok(CURRENT_VERSION),
        "future" => Ok(5),
        _ => Err(format!("unknown expected version {value}")),
    }
}

fn user_version(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn observe_marker(item: &OracleCase, connection: &Connection) -> Result<bool, String> {
    let sql = match item.scenario.as_str() {
        "concurrent_fresh" => "SELECT count(*) FROM diagnostics",

        "rollback" => "SELECT count(*) FROM diagnostics WHERE id=7 AND message='preserve me'",
        "unsupported" => "SELECT count(*) FROM unsupported_schema_marker WHERE value='preserve me'",
        scenario => return Err(format!("cannot observe marker for {scenario}")),
    };
    connection
        .query_row(sql, [], |row| row.get::<_, i64>(0))
        .map(|count| count > 0)
        .map_err(|error| error.to_string())
}
