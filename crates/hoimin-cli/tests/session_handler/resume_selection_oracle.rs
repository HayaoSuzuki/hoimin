use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;

use hoimin_core::FINGERPRINT_SCHEMA_VERSION;
use rusqlite::params;
use serde::Deserialize;

use super::{
    Connection, EffectFailure, EffectId, LoadSession, RunFingerprint, SessionHandler,
    SessionLoadOutcome, Value, begin_request, finish_request, lookup_request,
};

const CORPUS: &str = include_str!("../../../../formal/HoiminOracle/corpus/resume-selection.jsonl");

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    requested: u8,
    limit: NonZeroUsize,
    rows: Vec<Row>,
    outcome: String,
}

#[derive(Deserialize)]
struct Row {
    name: String,
    fingerprint: u8,
    complete: bool,
    active: bool,
    schema: FingerprintSchema,
    budget: Budget,
    after: Budget,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum FingerprintSchema {
    Current,
    Legacy,
    Future,
    WrongType,
    Missing,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Budget {
    Valid { value: u64 },
    Zero,
    Short,
    WrongType,
}

impl Budget {
    fn persisted(&self) -> Value {
        match self {
            Self::Valid { value } => Value::Blob(value.to_be_bytes().to_vec()),
            Self::Zero => Value::Blob(vec![0; 8]),
            Self::Short => Value::Blob(vec![0xff]),
            Self::WrongType => Value::Text("256".into()),
        }
    }
}

fn seed_runs(path: &Path, case: &Case) -> Vec<SessionHandler> {
    let mut holders = Vec::new();
    for row in &case.rows {
        let mut owner = SessionHandler::open(path).unwrap();
        let mut request = begin_request(1, &row.name);
        request.fingerprint = RunFingerprint::from_bytes([row.fingerprint; 32]);
        owner.begin(request).unwrap();
        if row.complete {
            owner.finish(finish_request(2, &row.name)).unwrap();
        } else if row.active {
            holders.push(owner);
        }
    }
    holders
}

fn seed_values(connection: &Connection, case: &Case) {
    connection
        .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    let mut versions = BTreeMap::new();
    for row in &case.rows {
        let prior = versions.insert(row.fingerprint, row.schema);
        assert!(
            prior.is_none_or(|schema| schema == row.schema),
            "{} inconsistent fingerprint seed",
            case.id
        );
        assert_eq!(
            connection
                .execute(
                    "UPDATE runs SET max_mutants=?1 WHERE run_id=?2",
                    params![row.budget.persisted(), row.name]
                )
                .unwrap(),
            1
        );
    }
    let current = i64::from(FINGERPRINT_SCHEMA_VERSION);
    for (fingerprint, schema) in versions {
        let digest = [fingerprint; 32];
        let version = match schema {
            FingerprintSchema::Current => Value::Integer(current),
            FingerprintSchema::Legacy => Value::Integer(current - 1),
            FingerprintSchema::Future => Value::Integer(current + 1),
            FingerprintSchema::WrongType => Value::Text("unsupported storage type".into()),
            FingerprintSchema::Missing => {
                assert_eq!(
                    connection
                        .execute("DELETE FROM fingerprints WHERE digest=?1", [&digest[..]])
                        .unwrap(),
                    1
                );
                continue;
            }
        };
        assert_eq!(
            connection
                .execute(
                    "UPDATE fingerprints SET schema_version=?1 WHERE digest=?2",
                    params![version, &digest[..]]
                )
                .unwrap(),
            1
        );
    }
}

fn stored_runs(connection: &Connection) -> Vec<(String, Vec<u8>, i64, Value)> {
    connection
        .prepare("SELECT run_id, fingerprint, complete, max_mutants FROM runs ORDER BY id")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn stored_versions(connection: &Connection) -> Vec<(Vec<u8>, Value)> {
    connection
        .prepare("SELECT digest, schema_version FROM fingerprints ORDER BY digest")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn check_case(case: &Case) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("resume.sqlite3");
    let mut loader = SessionHandler::open(&path).unwrap();
    let holders = seed_runs(&path, case);
    let connection = Connection::open(&path).unwrap();
    seed_values(&connection, case);
    let versions = stored_versions(&connection);
    let result = loader.load(&LoadSession {
        id: EffectId(10),
        fingerprint: RunFingerprint::from_bytes([case.requested; 32]),
        max_mutants: case.limit,
    });
    let label = match result {
        Ok(reply) => {
            assert_eq!(reply.id, EffectId(10));
            match reply.outcome {
                SessionLoadOutcome::Resumed(reference) => format!("resumed:{}", reference.run_id),
                SessionLoadOutcome::Fresh(reason) => reason.code().to_owned(),
            }
        }
        Err(error) => {
            assert_eq!(error.id, EffectId(10));
            match error.failure {
                EffectFailure::SessionDatabase {
                    code,
                    operation,
                    message,
                } if code == "session.corrupt" => {
                    assert_eq!(operation, "decode session row");
                    message
                }
                other => other.code().to_owned(),
            }
        }
    };
    assert_eq!(label, case.outcome, "{} selection", case.id);
    assert_eq!(
        stored_versions(&connection),
        versions,
        "{} fingerprint metadata",
        case.id
    );
    let expected = case
        .rows
        .iter()
        .map(|row| {
            (
                row.name.clone(),
                vec![row.fingerprint; 32],
                i64::from(row.complete),
                row.after.persisted(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stored_runs(&connection),
        expected,
        "{} stored runs",
        case.id
    );
    for row in &case.rows {
        let lookup = loader.lookup(&lookup_request(11, &row.name, "missing"));
        if case.outcome.strip_prefix("resumed:") == Some(row.name.as_str()) {
            assert!(lookup.unwrap().result.is_none(), "{} selected run", case.id);
        } else {
            assert_eq!(
                lookup.unwrap_err().failure.code(),
                "session.lookup.owner",
                "{} other run",
                case.id
            );
        }
    }
    drop(holders);
}

#[test]
fn resume_selection_matches_lean_oracle() {
    let mut total = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        check_case(&case);
        total += 1;
    }
    assert!(total > 0, "resume selection corpus is empty");
}
