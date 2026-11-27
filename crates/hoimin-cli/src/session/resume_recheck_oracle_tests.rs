use std::num::NonZeroUsize;
use std::path::Path;

use serde::Deserialize;

use super::{
    BeginSession, Connection, EffectFailure, EffectId, FINGERPRINT_SCHEMA_VERSION, LoadSession,
    SessionHandler, SessionLoadOutcome, params,
};
use hoimin_core::RunFingerprint;

const CORPUS: &str = include_str!("../../../../formal/HoiminOracle/corpus/resume-recheck.jsonl");
const SELECTED: &str = "selected";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    owner: Owner,
    write: Write,
    edit: Edit,
    requested: NonZeroUsize,
    outcome: String,
    after_owner: Owner,
    rows: Vec<Row>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Owner {
    Free,
    Loader,
    Peer,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Write {
    Normal,
    Ignore,
    Abort,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Edit {
    None,
    RaiseBudget,
    RestoreBudget,
    ChangeFingerprint,
    Finish,
    Complete,
    Delete,
    InsertNewer,
    ReplaceWithSameKey,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Row {
    name: String,
    ordinal: i64,
    fingerprint: u8,
    budget: u64,
    finished: bool,
    complete: bool,
}

fn begin(owner: &mut SessionHandler, name: &str, budget: usize) {
    owner
        .begin(BeginSession {
            id: EffectId(1),
            run_id: name.into(),
            fingerprint: RunFingerprint::from_bytes([1; 32]),
            max_mutants: NonZeroUsize::new(budget).unwrap(),
        })
        .unwrap();
}

fn write_budget(connection: &Connection, budget: u64) {
    let changed = connection
        .execute(
            "UPDATE runs SET max_mutants=?1 WHERE run_id=?2",
            params![budget.to_be_bytes().as_slice(), SELECTED],
        )
        .unwrap();
    assert_eq!(changed, 1);
}

fn insert(connection: &Connection, name: &str, budget: u64) {
    connection
        .execute(
            "INSERT INTO runs(run_id, fingerprint, max_mutants) VALUES (?1,?2,?3)",
            params![name, &[1_u8; 32][..], budget.to_be_bytes().as_slice()],
        )
        .unwrap();
}

fn apply_edit(path: &Path, edit: Edit) {
    let mut connection = Connection::open(path).unwrap();
    match edit {
        Edit::None => {}
        Edit::RaiseBudget => write_budget(&connection, 256),
        Edit::RestoreBudget => {
            write_budget(&connection, 256);
            write_budget(&connection, 255);
        }
        Edit::ChangeFingerprint => {
            let digest = [2_u8; 32];
            connection
                .execute(
                    "INSERT INTO fingerprints(digest, schema_version) VALUES (?1,?2)",
                    params![&digest[..], FINGERPRINT_SCHEMA_VERSION],
                )
                .unwrap();
            let changed = connection
                .execute(
                    "UPDATE runs SET fingerprint=?1 WHERE run_id=?2",
                    params![&digest[..], SELECTED],
                )
                .unwrap();
            assert_eq!(changed, 1);
        }
        Edit::Finish => {
            assert_eq!(
                connection
                    .execute("UPDATE runs SET finished=1 WHERE run_id=?1", [SELECTED])
                    .unwrap(),
                1
            );
        }
        Edit::Complete => {
            assert_eq!(
                connection
                    .execute(
                        "UPDATE runs SET finished=1, complete=1 WHERE run_id=?1",
                        [SELECTED]
                    )
                    .unwrap(),
                1
            );
        }
        Edit::Delete => {
            assert_eq!(
                connection
                    .execute("DELETE FROM runs WHERE run_id=?1", [SELECTED])
                    .unwrap(),
                1
            );
        }
        Edit::InsertNewer => insert(&connection, "newer", 256),
        Edit::ReplaceWithSameKey => {
            let transaction = connection.transaction().unwrap();
            assert_eq!(
                transaction
                    .execute("DELETE FROM runs WHERE run_id=?1", [SELECTED])
                    .unwrap(),
                1
            );
            insert(&transaction, SELECTED, 255);
            transaction.commit().unwrap();
        }
    }
}

fn stored(connection: &Connection) -> Vec<Row> {
    connection
        .prepare(
            "SELECT run_id, id, fingerprint, max_mutants, finished, complete FROM runs ORDER BY id",
        )
        .unwrap()
        .query_map([], |row| {
            let digest: Vec<u8> = row.get(2)?;
            let budget: Vec<u8> = row.get(3)?;
            assert_eq!(digest, vec![digest[0]; 32]);
            Ok(Row {
                name: row.get(0)?,
                ordinal: row.get(1)?,
                fingerprint: digest[0],
                budget: u64::from_be_bytes(budget.try_into().unwrap()),
                finished: row.get(4)?,
                complete: row.get(5)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn configure_write(connection: &Connection, mode: &Write) {
    let action = match mode {
        Write::Normal => return,
        Write::Ignore => "RAISE(IGNORE)",
        Write::Abort => "RAISE(ABORT, 'oracle update refusal')",
    };
    connection
        .execute_batch(&format!(
            "CREATE TEMP TRIGGER oracle_write BEFORE UPDATE ON main.runs
             BEGIN SELECT {action}; END;"
        ))
        .unwrap();
}

fn assert_ownership(path: &Path, loader: &SessionHandler, case: &Case) {
    let loader_owns = case.after_owner == Owner::Loader;
    assert_eq!(
        loader.ownerships.len(),
        usize::from(loader_owns),
        "{}",
        case.id
    );
    assert_eq!(
        loader.ownerships.contains_key(SELECTED),
        loader_owns,
        "{}",
        case.id
    );
    let witness = SessionHandler::open(path).unwrap();
    let lock = witness.acquire_ownership(EffectId(3), SELECTED, "probe recheck ownership");
    if case.after_owner == Owner::Free {
        drop(lock.unwrap());
    } else {
        assert_eq!(
            lock.err().unwrap().failure.code(),
            "session.resume.active",
            "{}",
            case.id
        );
    }
}

fn check_case(case: &Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    let mut original_owner = SessionHandler::open(&path).unwrap();
    begin(&mut original_owner, "older", 1);
    drop(original_owner);
    let mut loader = SessionHandler::open(&path).unwrap();
    let peer_owner = if case.owner == Owner::Loader {
        begin(&mut loader, SELECTED, 255);
        None
    } else {
        let mut peer = SessionHandler::open(&path).unwrap();
        begin(&mut peer, SELECTED, 255);
        if case.owner == Owner::Peer {
            Some(peer)
        } else {
            None
        }
    };
    configure_write(&loader.connection, &case.write);
    let edit = case.edit;
    let edit_path = path.clone();
    loader.resume_after_candidate = Some(Box::new(move |chosen| {
        assert_eq!(chosen, SELECTED);
        apply_edit(&edit_path, edit);
    }));

    let attempt = loader.load(&LoadSession {
        id: EffectId(2),
        fingerprint: RunFingerprint::from_bytes([1; 32]),
        max_mutants: case.requested,
    });
    assert!(loader.resume_after_candidate.is_none(), "{}", case.id);
    let label = match &attempt {
        Ok(reply) => {
            assert_eq!(reply.id, EffectId(2));
            match &reply.outcome {
                SessionLoadOutcome::Resumed(reference) => {
                    assert_eq!(reference.run_id, SELECTED, "{}", case.id);
                    "resumed"
                }
                SessionLoadOutcome::Fresh(reason) => reason.code(),
            }
        }
        Err(error) => {
            assert_eq!(error.id, EffectId(2));
            if let EffectFailure::SessionDatabase {
                code,
                operation,
                message,
            } = &error.failure
                && code == "session.read"
            {
                assert_eq!(operation, "re-read resume run");
                assert!(message.contains("oracle update refusal"));
            }
            error.failure.code()
        }
    };
    assert_eq!(label, case.outcome, "{}", case.id);
    assert_eq!(stored(&loader.connection), case.rows, "{} rows", case.id);
    assert_ownership(&path, &loader, case);
    drop(peer_owner);
}

#[test]
fn resume_recheck_matches_lean_oracle() {
    let mut checked = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        check_case(&case);
        checked += 1;
    }
    assert!(checked > 0, "resume recheck corpus is empty");
}
