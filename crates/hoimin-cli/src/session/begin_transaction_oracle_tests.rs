use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::time::Duration;

use hoimin_core::RunFingerprint;
use serde::Deserialize;

use super::{
    BeginSession, Connection, EffectFailure, EffectId, FINGERPRINT_SCHEMA_VERSION,
    OptionalExtension, SessionHandler, coverage_tests::reject_commit, params,
};

const CORPUS: &str = include_str!("../../../../formal/HoiminOracle/corpus/begin-transaction.jsonl");
const SELECTED: &str = "selected";
const OLDER: &str = "older";
const OLDER_ROW: Row = Row {
    digest: 1,
    budget: 1,
    finished: false,
    complete: false,
};
const LEGACY_VERSION: u8 = FINGERPRINT_SCHEMA_VERSION - 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    before: State,
    fault: Fault,
    digest: u8,
    first_budget: NonZeroUsize,
    retry_budget: NonZeroUsize,
    first: Observation,
    retry: Observation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Owner {
    Free,
    Caller,
    Peer,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Metadata {
    Absent,
    Legacy,
    Current,
}

impl Metadata {
    fn version(&self) -> Option<u8> {
        match self {
            Self::Absent => None,
            Self::Legacy => Some(LEGACY_VERSION),
            Self::Current => Some(FINGERPRINT_SCHEMA_VERSION),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Row {
    digest: u8,
    budget: u64,
    finished: bool,
    complete: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    owner: Owner,
    fingerprint: Metadata,
    row: Option<Row>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fault {
    None,
    TransactionBusy,
    FingerprintAbort,
    RunAbort,
    CommitReject,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Started,
    Active,
    BeginError,
    FingerprintError,
    RunError,
    DuplicateRun,
    CommitError,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    outcome: Outcome,
    after: State,
}

fn request(id: EffectId, name: &str, digest: u8, budget: NonZeroUsize) -> BeginSession {
    BeginSession {
        id,
        run_id: name.into(),
        fingerprint: RunFingerprint::from_bytes([digest; 32]),
        max_mutants: budget,
    }
}

fn seed_target(caller: &mut SessionHandler, peer: &mut SessionHandler, before: &State) {
    if before.owner == Owner::Free && before.row.is_none() {
        return;
    }
    let seeder = if before.owner == Owner::Caller {
        &mut *caller
    } else {
        &mut *peer
    };
    let (digest, budget) = before.row.map_or((2, 1), |row| (row.digest, row.budget));
    let required = NonZeroUsize::try_from(usize::try_from(budget).unwrap()).unwrap();
    seeder
        .begin(request(EffectId(1), SELECTED, digest, required))
        .unwrap();
    if before.owner == Owner::Free {
        drop(seeder.ownerships.remove(SELECTED).unwrap());
    }
    let changed = if let Some(row) = before.row {
        peer.connection.execute(
            "UPDATE runs SET finished=?1, complete=?2 WHERE run_id=?3",
            params![row.finished, row.complete, SELECTED],
        )
    } else {
        peer.connection
            .execute("DELETE FROM runs WHERE run_id=?1", [SELECTED])
    };
    assert_eq!(changed.unwrap(), 1);
}

fn configure_fault(caller: &SessionHandler, witness: &Connection, fault: Fault) {
    let (table, message) = match fault {
        Fault::None => return,
        Fault::TransactionBusy => {
            caller.connection.busy_timeout(Duration::ZERO).unwrap();
            witness.execute_batch("BEGIN IMMEDIATE").unwrap();
            return;
        }
        Fault::FingerprintAbort => ("fingerprints", "oracle fingerprint refusal"),
        Fault::RunAbort => ("runs", "oracle run refusal"),
        Fault::CommitReject => {
            reject_commit(caller, "INSERT", "runs");
            return;
        }
    };
    caller
        .connection
        .execute_batch(&format!(
            "CREATE TEMP TRIGGER oracle_begin BEFORE INSERT ON main.{table}
             BEGIN SELECT RAISE(ABORT, '{message}'); END;"
        ))
        .unwrap();
}

fn clear_fault(writer: &Connection, witness: &Connection, fault: Fault, id: &str) {
    match fault {
        Fault::None => {}
        Fault::TransactionBusy => witness.execute_batch("ROLLBACK").unwrap(),
        Fault::FingerprintAbort | Fault::RunAbort => writer
            .execute_batch("DROP TRIGGER temp.oracle_begin")
            .unwrap(),
        Fault::CommitReject => {
            let pending: i64 = writer
                .query_row("SELECT count(*) FROM deferred_fault", [], |row| row.get(0))
                .unwrap();
            assert_eq!(pending, 0, "{id}: deferred insert must roll back");
            writer
                .execute_batch("DROP TRIGGER temp.reject_commit")
                .unwrap();
        }
    }
}

fn stored_row(connection: &Connection, name: &str) -> Option<Row> {
    connection
        .query_row(
            "SELECT fingerprint, max_mutants, finished, complete FROM runs WHERE run_id=?1",
            [name],
            |row| {
                let digest: Vec<u8> = row.get(0)?;
                let budget: Vec<u8> = row.get(1)?;
                assert_eq!(digest, [digest[0]; 32], "{name}");
                Ok(Row {
                    digest: digest[0],
                    budget: u64::from_be_bytes(budget.try_into().unwrap()),
                    finished: row.get(2)?,
                    complete: row.get(3)?,
                })
            },
        )
        .optional()
        .unwrap()
}

fn fingerprint_version(connection: &Connection, digest: u8) -> Option<u8> {
    connection
        .query_row(
            "SELECT schema_version FROM fingerprints WHERE digest=?1",
            [&[digest; 32][..]],
            |row| row.get(0),
        )
        .optional()
        .unwrap()
}

fn attempt(caller: &mut SessionHandler, digest: u8, budget: NonZeroUsize, id: EffectId) -> Outcome {
    let reply = caller.begin(request(id, SELECTED, digest, budget));
    assert!(caller.connection.is_autocommit());
    match reply {
        Ok(started) => {
            assert_eq!(started.id, id);
            assert_eq!(started.run_id, SELECTED);
            Outcome::Started
        }
        Err(error) => {
            assert_eq!(error.id, id);
            let EffectFailure::SessionDatabase {
                code,
                operation,
                message,
            } = error.failure
            else {
                panic!("unexpected begin failure: {:?}", error.failure);
            };
            match (code.as_str(), operation.as_str()) {
                ("session.resume.active", "claim new run") => Outcome::Active,
                ("session.begin", "begin run transaction") => Outcome::BeginError,
                ("session.begin", "insert run")
                    if message.contains("oracle fingerprint refusal") =>
                {
                    Outcome::FingerprintError
                }
                ("session.begin", "insert run") if message.contains("oracle run refusal") => {
                    Outcome::RunError
                }
                ("session.begin", "insert run")
                    if message.contains("UNIQUE constraint failed: runs.run_id") =>
                {
                    Outcome::DuplicateRun
                }
                ("session.commit", "commit run") => Outcome::CommitError,
                stage => panic!("unexpected begin stage: {stage:?} ({message})"),
            }
        }
    }
}

fn assert_ownership(caller: &SessionHandler, peer: &SessionHandler, expected: Owner, id: &str) {
    assert!(caller.ownerships.contains_key(OLDER), "{id}: older owner");
    assert_eq!(
        caller.ownerships.contains_key(SELECTED),
        expected == Owner::Caller,
        "{id}: caller",
    );
    assert_eq!(
        peer.ownerships.contains_key(SELECTED),
        expected == Owner::Peer,
        "{id}: peer",
    );
    let lock = peer.acquire_ownership(EffectId(5), SELECTED, "probe begin ownership");
    if expected == Owner::Free {
        drop(lock.unwrap());
    } else {
        assert_eq!(
            lock.err().unwrap().failure.code(),
            "session.resume.active",
            "{id}"
        );
    }
}

fn assert_state(
    caller: &SessionHandler,
    peer: &SessionHandler,
    expected: &State,
    digest: u8,
    id: &str,
) {
    assert_eq!(stored_row(&peer.connection, SELECTED), expected.row, "{id}");
    assert_eq!(stored_row(&peer.connection, OLDER), Some(OLDER_ROW), "{id}");
    assert_eq!(
        fingerprint_version(&peer.connection, digest),
        expected.fingerprint.version(),
        "{id}: requested fingerprint",
    );
    assert_eq!(
        fingerprint_version(&peer.connection, OLDER_ROW.digest),
        Some(FINGERPRINT_SCHEMA_VERSION),
        "{id}: older fingerprint",
    );
    assert_ownership(caller, peer, expected.owner, id);
}

fn replay(case: &Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    let mut caller = SessionHandler::open(&path).unwrap();
    let mut peer = SessionHandler::open(&path).unwrap();
    caller
        .begin(request(
            EffectId(1),
            OLDER,
            OLDER_ROW.digest,
            NonZeroUsize::MIN,
        ))
        .unwrap();
    seed_target(&mut caller, &mut peer, &case.before);
    if let Some(version) = case.before.fingerprint.version() {
        let inserted = peer
            .connection
            .execute(
                "INSERT INTO fingerprints(digest, schema_version) VALUES (?1, ?2)",
                params![&[case.digest; 32][..], version],
            )
            .unwrap();
        assert_eq!(inserted, 1, "{}", case.id);
    }
    assert_state(&caller, &peer, &case.before, case.digest, &case.id);
    configure_fault(&caller, &peer.connection, case.fault);

    let first = attempt(&mut caller, case.digest, case.first_budget, EffectId(2));
    clear_fault(&caller.connection, &peer.connection, case.fault, &case.id);
    assert_eq!(first, case.first.outcome, "{}: first attempt", case.id);
    assert_state(&caller, &peer, &case.first.after, case.digest, &case.id);

    let retry = attempt(&mut caller, case.digest, case.retry_budget, EffectId(3));
    assert_eq!(retry, case.retry.outcome, "{}: retry", case.id);
    assert_state(&caller, &peer, &case.retry.after, case.digest, &case.id);
}

#[test]
fn begin_transaction_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        replay(&case);
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 90);
}
