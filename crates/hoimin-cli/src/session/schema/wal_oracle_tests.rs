use std::fs;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, Transaction};
use serde::Deserialize;

use super::super::{SCHEMA_VERSION, admit, immutable_uri};
use super::{
    Database, Journal, Objects, Outcome, SchemaError, SqliteCompanions, create_database,
    current_schema, initialize, inspect_opened, open, schema_objects,
};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/session-wal-admission.jsonl");

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    seed: Database,
    change: Change,
    checkpoint: bool,
    checkpointed: Image,
    committed: Image,
    advisory_result: Decision,
    committed_result: Decision,
    opener_result: Decision,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Change {
    Version { value: i64 },
    ApplicationId { value: i64 },
    CreateForeign,
    DropForeign,
    Initialize,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Image {
    version: i64,
    application_id: i64,
    objects: Objects,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Decision {
    Admitted,
    UnsupportedVersion { version: i64 },
    InvalidIdentity,
}

fn decision(error: SchemaError) -> Decision {
    match error {
        SchemaError::UnsupportedVersion(version) => Decision::UnsupportedVersion { version },
        SchemaError::InvalidIdentity => Decision::InvalidIdentity,
        other => panic!("unexpected WAL admission error: {other:?}"),
    }
}

fn change_image(transaction: &Transaction<'_>, change: &Change) {
    match change {
        Change::Version { value } => transaction
            .pragma_update(None, "user_version", value)
            .unwrap(),
        Change::ApplicationId { value } => transaction
            .pragma_update(None, "application_id", value)
            .unwrap(),
        Change::CreateForeign => transaction
            .execute_batch("CREATE TABLE extra(value); INSERT INTO extra VALUES (42)")
            .unwrap(),
        Change::DropForeign => transaction.execute_batch("DROP TABLE example").unwrap(),
        Change::Initialize => initialize(transaction).unwrap(),
    }
}

fn check_view(reader: &Connection, expected: &Image, acceptance: &Decision, id: &str) {
    let schema = current_schema().unwrap();
    let objects = schema_objects(reader).unwrap();
    let observed = Image {
        version: reader
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap(),
        application_id: reader
            .pragma_query_value(None, "application_id", |row| row.get(0))
            .unwrap(),
        objects: if objects.is_empty() {
            Objects::Empty
        } else if objects == schema {
            Objects::Canonical
        } else {
            Objects::Foreign
        },
    };
    assert_eq!(&observed, expected, "{id} SQLite image");
    let actual = admit(reader, &schema).map_or_else(decision, |_| Decision::Admitted);
    assert_eq!(&actual, acceptance, "{id} image admission");
}

fn check_views(path: &Path, case: &Case) {
    let readonly = OpenFlags::SQLITE_OPEN_READ_ONLY;
    let regular = Connection::open_with_flags(path, readonly).unwrap();
    check_view(&regular, &case.committed, &case.committed_result, &case.id);
    let immutable = Connection::open_with_flags(
        immutable_uri(path).unwrap(),
        readonly | OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    check_view(
        &immutable,
        &case.checkpointed,
        &case.advisory_result,
        &case.id,
    );
}

fn check_case(case: &Case) {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("session.sqlite3");
    create_database(&database, &case.seed, Journal::Wal);
    let wal = SqliteCompanions::new(&database).wal;
    let mut writer = Connection::open(&database).unwrap();
    writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    let main_before = fs::read(&database).unwrap();
    let transaction = writer.transaction().unwrap();
    change_image(&transaction, &case.change);
    transaction.commit().unwrap();
    assert!(
        fs::metadata(&wal).unwrap().len() > 0,
        "{} WAL is empty",
        case.id
    );
    assert!(
        fs::read(&database).unwrap() == main_before,
        "{} main DB changed before checkpoint",
        case.id
    );
    if case.checkpoint {
        let busy: i64 = writer
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy, 0, "{} checkpoint busy", case.id);
        assert_eq!(fs::metadata(&wal).unwrap().len(), 0, "{}", case.id);
    }
    check_views(&database, case);

    let main_before = fs::read(&database).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    let actual = match open(&database) {
        Ok(connection) => {
            let Outcome::Opened {
                version,
                foreign_keys,
                journal,
                ..
            } = inspect_opened(&connection, &case.id)
            else {
                panic!("{} opened connection produced a rejection", case.id);
            };
            assert_eq!(version, SCHEMA_VERSION, "{}", case.id);
            assert!(foreign_keys, "{}", case.id);
            assert_eq!(journal, "wal", "{}", case.id);
            Decision::Admitted
        }
        Err(error) => {
            assert!(
                fs::read(&database).unwrap() == main_before,
                "{} main DB changed on rejection",
                case.id
            );
            assert!(
                fs::read(&wal).unwrap() == wal_before,
                "{} WAL changed on rejection",
                case.id
            );
            decision(error)
        }
    };
    assert_eq!(actual, case.opener_result, "{}", case.id);
    drop(writer);
}

#[test]
fn session_wal_admission_matches_lean_oracle() {
    let selected = std::env::var("HOIMIN_SESSION_WAL_ORACLE_CASE").ok();
    let mut checked = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        if selected.as_ref().is_none_or(|id| id == &case.id) {
            check_case(&case);
            checked += 1;
            if selected.is_some() {
                println!(
                    "{}: committed {:?}, opener {:?}",
                    case.id, case.committed_result, case.opener_result
                );
            }
        }
    }
    assert!(checked > 0, "no WAL admission cases matched {selected:?}");
}
