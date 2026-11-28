use std::path::Path;

use serde::Deserialize;

use super::{
    Connection, EffectFailed, EffectFailure, EffectId, LookupStoredResult, MutationStatus,
    ResumeDecision, ReusedMutationStatus, SessionHandler, Value, begin_request, lookup_request,
    persist_request, resume_policy,
};

const CORPUS: &str = include_str!("../../../../formal/HoiminOracle/corpus/stored-result.jsonl");
const RUN: &str = "oracle-run";
const MUTANT: &str = "oracle-mutant";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    owned: bool,
    run: RunState,
    candidate: bool,
    row: OutcomeRow,
    lookup: String,
    persist: String,
    reuse: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RunState {
    Missing,
    Incomplete,
    Complete,
    InvalidFlag,
    WrongType,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum OutcomeRow {
    Absent,
    Blob,
    Text { json: String },
}

fn seed(path: &Path, case: &Case) -> (SessionHandler, Connection) {
    let mut owner = SessionHandler::open(path).unwrap();
    owner.begin(begin_request(1, RUN)).unwrap();
    owner.persist(&persist_request(2, RUN, MUTANT)).unwrap();
    owner
        .persist(&persist_request(3, RUN, "neighbor-mutant"))
        .unwrap();
    owner.begin(begin_request(4, "neighbor-run")).unwrap();
    owner
        .persist(&persist_request(5, "neighbor-run", MUTANT))
        .unwrap();

    let observer = Connection::open(path).unwrap();
    observer
        .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    if !case.candidate {
        observer
            .execute(
                "DELETE FROM candidates WHERE run_id=?1 AND mutant_id=?2",
                [RUN, MUTANT],
            )
            .unwrap();
    }
    match &case.row {
        OutcomeRow::Absent => {
            observer
                .execute(
                    "DELETE FROM results WHERE run_id=?1 AND mutant_id=?2",
                    [RUN, MUTANT],
                )
                .unwrap();
        }
        OutcomeRow::Blob => {
            observer
                .execute(
                    "UPDATE results SET outcome_json=x'00FF' WHERE run_id=?1 AND mutant_id=?2",
                    [RUN, MUTANT],
                )
                .unwrap();
        }
        OutcomeRow::Text { json } => {
            observer
                .execute(
                    "UPDATE results SET outcome_json=?1 WHERE run_id=?2 AND mutant_id=?3",
                    [json.as_str(), RUN, MUTANT],
                )
                .unwrap();
        }
    }
    if !case.candidate || matches!(case.row, OutcomeRow::Absent) {
        observer
            .execute(
                "DELETE FROM diagnostics WHERE run_id=?1 AND mutant_id=?2",
                [RUN, MUTANT],
            )
            .unwrap();
    }
    let change_run = match case.run {
        RunState::Missing => "DELETE FROM runs WHERE run_id=?1",
        RunState::Incomplete => "",
        RunState::Complete => "UPDATE runs SET complete=1 WHERE run_id=?1",
        RunState::InvalidFlag => "UPDATE runs SET complete=7 WHERE run_id=?1",
        RunState::WrongType => "UPDATE runs SET complete='invalid' WHERE run_id=?1",
    };
    if !change_run.is_empty() {
        assert_eq!(observer.execute(change_run, [RUN]).unwrap(), 1);
    }
    (owner, observer)
}

fn snapshot(connection: &Connection, target: bool) -> Vec<Vec<Value>> {
    let predicate = if target {
        "run_id=?1 AND mutant_id=?2"
    } else {
        "run_id<>?1 OR mutant_id<>?2"
    };
    ["candidates", "results", "diagnostics"]
        .into_iter()
        .flat_map(|table| {
            let mut statement = connection
                .prepare(&format!(
                    "SELECT * FROM {table} WHERE {predicate} ORDER BY rowid"
                ))
                .unwrap();
            let columns = statement.column_count();
            statement
                .query_map([RUN, MUTANT], |row| {
                    (0..columns)
                        .map(|column| row.get::<_, Value>(column))
                        .collect()
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        })
        .collect()
}

fn observed_error(error: &EffectFailed) -> &str {
    match &error.failure {
        EffectFailure::SessionDatabase {
            code,
            operation,
            message,
        } if code == "session.corrupt" => {
            assert_eq!(operation, "decode session row");
            message
        }
        other => other.code(),
    }
}

fn check_lookup(handler: &mut SessionHandler, request: &LookupStoredResult, case: &Case) {
    let result = handler.lookup(request);
    let (label, reusable) = match &result {
        Ok(found) => {
            assert_eq!(found.id, request.id);
            assert_eq!(found.worker, request.worker);
            if let Some(result) = &found.result {
                assert_eq!(result.mutant_id, MUTANT);
            }
            let reusable = match resume_policy(found.result.as_ref()) {
                ResumeDecision::Reuse(status) => {
                    let reused = match status {
                        ReusedMutationStatus::Killed => "killed",
                        ReusedMutationStatus::Survived => "survived",
                    };
                    assert_eq!(reused, case.lookup, "{} reused status", case.id);
                    true
                }
                ResumeDecision::Rerun => false,
            };
            (
                found
                    .result
                    .as_ref()
                    .map_or("missing", |result| result.status.as_str()),
                reusable,
            )
        }
        Err(error) => {
            assert_eq!(error.id, request.id);
            (observed_error(error), false)
        }
    };
    assert_eq!(
        (label, reusable),
        (case.lookup.as_str(), case.reuse),
        "{} lookup",
        case.id
    );
}

fn check_case(case: &Case) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("session.sqlite3");
    let (mut owner, observer) = seed(&path, case);
    let mut other;
    let handler = if case.owned {
        &mut owner
    } else {
        other = SessionHandler::open(&path).unwrap();
        &mut other
    };
    let before = snapshot(&observer, true);
    let neighbors = snapshot(&observer, false);
    let mut lookup = lookup_request(10, RUN, MUTANT);
    lookup.worker = 7;
    check_lookup(handler, &lookup, case);
    assert_eq!(
        snapshot(&observer, true),
        before,
        "{} lookup mutated the rows",
        case.id
    );

    let mut replacement = persist_request(11, RUN, MUTANT);
    replacement.worker = 8;
    replacement.result.candidate.sequence = 17;
    replacement.result.diagnostics[0].message = "replacement diagnostic".into();
    let write_result = handler.persist(&replacement);
    let label = match &write_result {
        Ok(stored) => {
            assert_eq!(stored.id, EffectId(11));
            assert_eq!(stored.worker, 8);
            assert_eq!(stored.run_id, RUN);
            assert_eq!(stored.mutant_id, MUTANT);
            "stored"
        }
        Err(error) => {
            assert_eq!(error.id, EffectId(11));
            observed_error(error)
        }
    };
    assert_eq!(label, case.persist, "{} persist", case.id);
    assert_eq!(
        snapshot(&observer, false),
        neighbors,
        "{} changed other results",
        case.id
    );
    if write_result.is_ok() {
        let result = handler.lookup(&lookup).unwrap().result.unwrap();
        assert_eq!(result.status, MutationStatus::Killed, "{}", case.id);
        let sequence: i64 = observer
            .query_row(
                "SELECT sequence FROM candidates WHERE run_id=?1 AND mutant_id=?2",
                [RUN, MUTANT],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sequence, 17, "{}", case.id);
        let diagnostic: (i64, String) = observer
            .query_row(
                "SELECT count(*), message FROM diagnostics WHERE run_id=?1 AND mutant_id=?2",
                [RUN, MUTANT],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            diagnostic,
            (1, "replacement diagnostic".into()),
            "{}",
            case.id
        );
    } else {
        assert_eq!(
            snapshot(&observer, true),
            before,
            "{} rejected persist mutated the rows",
            case.id
        );
    }
}

#[test]
fn stored_result_matches_lean_oracle() {
    let mut count = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        check_case(&case);
        count += 1;
    }
    assert!(count > 0, "stored result corpus is empty");
}
