use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::time::Duration;

use hoimin_core::RunFingerprint;
use serde::Deserialize;

use super::{
    BeginSession, Connection, EffectFailure, EffectId, FinishSession, OptionalExtension,
    SessionHandler,
};

const CORPUS: &str =
    include_str!("../../../../formal/HoiminOracle/corpus/finish-transaction.jsonl");
const SELECTED: &str = "selected";
const OLDER: &str = "older";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    before: State,
    fault: Fault,
    complete: bool,
    first: Observation,
    retry: Observation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Row {
    Missing,
    Unfinished,
    Resumable,
    Complete,
    CompleteUnfinished,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fault {
    None,
    BeginBusy,
    UpdateAbort,
    UpdateIgnore,
    CommitReject,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Finished,
    OwnerRequired,
    BeginError,
    UpdateError,
    StateError,
    CommitError,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    row: Row,
    owned: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    outcome: Outcome,
    commit_reached: bool,
    after: State,
}

fn seed_run(handler: &mut SessionHandler, name: &str) {
    handler
        .begin(BeginSession {
            id: EffectId(1),
            run_id: name.into(),
            fingerprint: RunFingerprint::from_bytes([1; 32]),
            max_mutants: NonZeroUsize::MIN,
        })
        .unwrap();
}

fn seed_target(handler: &mut SessionHandler, observer: &mut SessionHandler, state: &State) {
    if state.owned {
        seed_run(handler, SELECTED);
    } else {
        seed_run(observer, SELECTED);
        // Release the fixture lock while leaving the committed row in place.
        drop(observer.ownerships.remove(SELECTED).unwrap());
    }
    let sql = match state.row {
        Row::Unfinished => return,
        Row::Missing => "DELETE FROM runs WHERE run_id=?1",
        Row::Resumable => "UPDATE runs SET finished=1 WHERE run_id=?1",
        Row::Complete => "UPDATE runs SET finished=1, complete=1 WHERE run_id=?1",
        Row::CompleteUnfinished => "UPDATE runs SET complete=1 WHERE run_id=?1",
    };
    assert_eq!(observer.connection.execute(sql, [SELECTED]).unwrap(), 1);
}

fn configure_fault(writer: &Connection, observer: &Connection, fault: Fault) {
    let sql = match fault {
        Fault::None => return,
        Fault::BeginBusy => {
            writer.busy_timeout(Duration::ZERO).unwrap();
            observer.execute_batch("BEGIN IMMEDIATE").unwrap();
            return;
        }
        Fault::UpdateAbort => {
            "CREATE TEMP TRIGGER oracle_finish BEFORE UPDATE ON main.runs
             BEGIN SELECT RAISE(ABORT, 'oracle update refusal'); END;"
        }
        Fault::UpdateIgnore => {
            "CREATE TEMP TRIGGER oracle_finish BEFORE UPDATE ON main.runs
             BEGIN SELECT RAISE(IGNORE); END;"
        }
        Fault::CommitReject => {
            "CREATE TEMP TABLE finish_parent(id INTEGER PRIMARY KEY);
             CREATE TEMP TABLE finish_child(
                parent_id INTEGER REFERENCES finish_parent(id) DEFERRABLE INITIALLY DEFERRED
             );
             CREATE TEMP TRIGGER oracle_finish AFTER UPDATE ON main.runs
             BEGIN INSERT INTO finish_child VALUES(42); END;"
        }
    };
    writer.execute_batch(sql).unwrap();
}

fn clear_fault(writer: &Connection, observer: &Connection, fault: Fault, id: &str) {
    if matches!(fault, Fault::BeginBusy) {
        observer.execute_batch("ROLLBACK").unwrap();
    }
    if matches!(fault, Fault::CommitReject) {
        let count: i64 = writer
            .query_row("SELECT count(*) FROM finish_child", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "{id}: deferred write must roll back");
    }
    writer
        .execute_batch("DROP TRIGGER IF EXISTS temp.oracle_finish")
        .unwrap();
}

fn stored_row(connection: &Connection, name: &str) -> Row {
    let flags: Option<(bool, bool)> = connection
        .query_row(
            "SELECT finished, complete FROM runs WHERE run_id=?1",
            [name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .unwrap();
    match flags {
        None => Row::Missing,
        Some((false, false)) => Row::Unfinished,
        Some((true, false)) => Row::Resumable,
        Some((true, true)) => Row::Complete,
        Some((false, true)) => Row::CompleteUnfinished,
    }
}

fn attempt(handler: &mut SessionHandler, effect: EffectId, complete: bool) -> (Outcome, bool) {
    handler.finish_before_commit = Some(Box::new(|| {}));
    let reply = handler.finish(FinishSession {
        id: effect,
        run_id: SELECTED.into(),
        complete,
    });
    let outcome = match reply {
        Ok(success) => {
            assert_eq!(success.id, effect);
            assert_eq!(success.run_id, SELECTED);
            assert_eq!(success.complete, complete);
            Outcome::Finished
        }
        Err(error) => {
            assert_eq!(error.id, effect);
            let EffectFailure::SessionDatabase {
                code, operation, ..
            } = error.failure
            else {
                panic!("unexpected failure: {:?}", error.failure);
            };
            match (code.as_str(), operation.as_str()) {
                ("session.finish.owner", "validate session lifecycle") => Outcome::OwnerRequired,
                ("session.finish", "begin finish transaction") => Outcome::BeginError,
                ("session.finish", "mark run finished") => Outcome::UpdateError,
                ("session.finish.state", "mark run finished") => Outcome::StateError,
                ("session.commit", "commit run finish") => Outcome::CommitError,
                unexpected => panic!("unexpected finish stage: {unexpected:?}"),
            }
        }
    };
    assert!(handler.connection.is_autocommit());
    (outcome, handler.finish_before_commit.take().is_none())
}

fn assert_ownership(observer: &SessionHandler, name: &str, owned: bool, id: &str) {
    let lock = observer.acquire_ownership(EffectId(5), name, "probe finish ownership");
    if owned {
        assert_eq!(
            lock.err().unwrap().failure.code(),
            "session.resume.active",
            "{id}: {name}",
        );
    } else {
        drop(lock.unwrap());
    }
}

fn assert_observation(
    handler: &SessionHandler,
    observer: &SessionHandler,
    expected: &Observation,
    actual: (Outcome, bool),
    id: &str,
) {
    assert_eq!(actual, (expected.outcome, expected.commit_reached), "{id}");
    assert_eq!(
        stored_row(&observer.connection, SELECTED),
        expected.after.row,
        "{id}"
    );
    assert_eq!(
        stored_row(&observer.connection, OLDER),
        Row::Unfinished,
        "{id}"
    );
    assert!(handler.ownerships.contains_key(OLDER), "{id}");
    assert_eq!(
        handler.ownerships.contains_key(SELECTED),
        expected.after.owned,
        "{id}",
    );
    assert_ownership(observer, SELECTED, expected.after.owned, id);
    assert_ownership(observer, OLDER, true, id);
}

fn replay(case: &Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    let mut observer = SessionHandler::open(&path).unwrap();
    seed_run(&mut handler, OLDER);
    seed_target(&mut handler, &mut observer, &case.before);
    configure_fault(&handler.connection, &observer.connection, case.fault);

    let first = attempt(&mut handler, EffectId(2), case.complete);
    clear_fault(
        &handler.connection,
        &observer.connection,
        case.fault,
        &case.id,
    );
    assert_observation(&handler, &observer, &case.first, first, &case.id);

    let retry = attempt(&mut handler, EffectId(3), !case.complete);
    assert_observation(&handler, &observer, &case.retry, retry, &case.id);
}

#[test]
fn finish_transaction_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        replay(&case);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 100);
}
