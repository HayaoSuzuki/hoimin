use std::fs;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, Error, ErrorCode, OpenFlags, params};
use serde::Deserialize;

use super::super::{SessionError, SessionHandler};
use super::{
    SchemaError, SchemaObjects, configure, configure_with_schema, current_schema, enable_wal,
    initialize, is_lock_contention, schema_objects,
};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/session-configuration.jsonl");
const EXISTING_FINGERPRINT: [u8; 32] = [73; 32];

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    before: Durable,
    temporary: bool,
    peer_initializes: bool,
    full_during_schema: bool,
    journal_reply: JournalReply,
    first: InitialObservation,
    second: RetryObservation,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum JournalReply {
    Available,
    Busy,
    Refused,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Durable {
    version: i64,
    objects: Objects,
    rows: usize,
    journal: Journal,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Objects {
    Empty,
    Canonical,
    Foreign,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Journal {
    Rollback,
    Wal,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Opened,
    UnsupportedVersion { version: i64 },
    InvalidIdentity,
    DiskFull,
    Busy,
    JournalRefused,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct InitialObservation {
    first_version: Option<i64>,
    writer_version: Option<i64>,
    before_journal: Option<Durable>,
    outcome: Outcome,
    after: Durable,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct RetryObservation {
    outcome: Outcome,
    after: Durable,
}

fn outcome(result: &Result<(), SchemaError>, id: &str) -> Outcome {
    match result {
        Ok(()) => Outcome::Opened,
        Err(SchemaError::UnsupportedVersion(version)) => {
            Outcome::UnsupportedVersion { version: *version }
        }
        Err(SchemaError::InvalidIdentity) => Outcome::InvalidIdentity,
        Err(SchemaError::Database(Error::SqliteFailure(failure, _)))
            if failure.code == ErrorCode::DiskFull =>
        {
            Outcome::DiskFull
        }
        Err(SchemaError::Database(error)) if is_lock_contention(error) => Outcome::Busy,
        Err(SchemaError::JournalMode(mode)) if mode == "delete" => Outcome::JournalRefused,
        Err(error) => panic!("{id} unexpected configuration error: {error:?}"),
    }
}

fn create_schema(connection: &mut Connection, rows: usize) {
    let transaction = connection.transaction().unwrap();
    initialize(&transaction).unwrap();
    assert!(rows <= 1, "the fixture supports one stored fingerprint");
    if rows == 1 {
        transaction
            .execute(
                "INSERT INTO fingerprints VALUES (?1, 17)",
                params![&EXISTING_FINGERPRINT[..]],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}

fn seed(connection: &mut Connection, case: &Case) {
    match case.before.objects {
        Objects::Empty if !case.temporary => connection.execute_batch("VACUUM").unwrap(),
        Objects::Empty => {}
        Objects::Canonical => create_schema(connection, case.before.rows),
        Objects::Foreign => connection
            .execute_batch("CREATE TABLE example(value); INSERT INTO example VALUES ('keep')")
            .unwrap(),
    }
    connection
        .pragma_update(None, "user_version", case.before.version)
        .unwrap();
    if case.before.journal == Journal::Wal {
        enable_wal(connection, Duration::ZERO).unwrap();
    }
}

fn observe(connection: &Connection, expected_schema: &SchemaObjects) -> Durable {
    let objects = schema_objects(connection).unwrap();
    let (objects, rows) = if objects == *expected_schema {
        let mut statement = connection
            .prepare("SELECT digest, schema_version FROM fingerprints ORDER BY digest")
            .unwrap();
        let fingerprints = statement
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for (digest, version) in &fingerprints {
            assert_eq!(digest.as_slice(), EXISTING_FINGERPRINT);
            assert_eq!(*version, 17);
        }
        (Objects::Canonical, fingerprints.len())
    } else if objects.is_empty() {
        (Objects::Empty, 0)
    } else {
        let marker: String = connection
            .query_row("SELECT value FROM example", [], |row| row.get(0))
            .unwrap();
        assert_eq!(marker, "keep");
        (Objects::Foreign, 0)
    };
    let mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    let journal = match mode.as_str() {
        "delete" => Journal::Rollback,
        "wal" => Journal::Wal,
        other => panic!("unexpected journal mode {other}"),
    };
    Durable {
        version: connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap(),
        objects,
        rows,
        journal,
    }
}

fn observe_disk(path: &Path, schema: &SchemaObjects) -> Durable {
    let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    observe(&reader, schema)
}

fn configure_first(
    connection: &mut Connection,
    path: &Path,
    schema: &SchemaObjects,
    case: &Case,
) -> InitialObservation {
    if case.full_during_schema {
        connection.execute_batch("PRAGMA max_page_count=3").unwrap();
    }
    let mut first_version = None;
    let mut writer_version = None;
    let mut before_journal = None;
    let result = configure_with_schema(
        connection,
        schema,
        |version| {
            first_version = Some(version);
            if case.peer_initializes {
                let mut peer = Connection::open(path).unwrap();
                create_schema(&mut peer, 1);
            }
        },
        |version| writer_version = Some(version),
        |database, timeout| {
            assert!(
                database.is_autocommit(),
                "{} journal change inside transaction",
                case.id
            );
            let committed = observe(database, schema);
            if !case.temporary {
                assert_eq!(
                    observe_disk(path, schema),
                    committed,
                    "{} separate reader",
                    case.id
                );
            }
            before_journal = Some(committed);
            match case.journal_reply {
                JournalReply::Available | JournalReply::Refused => enable_wal(database, timeout),
                JournalReply::Busy => {
                    let holder = Connection::open(path).unwrap();
                    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
                    database.busy_timeout(Duration::ZERO).unwrap();
                    let result = enable_wal(database, Duration::ZERO);
                    holder.execute_batch("ROLLBACK").unwrap();
                    result
                }
            }
        },
    );
    InitialObservation {
        first_version,
        writer_version,
        before_journal,
        outcome: outcome(&result, &case.id),
        after: observe(connection, schema),
    }
}

fn check_case(case: &Case, schema: &SchemaObjects) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    let mut database = if case.temporary {
        Connection::open("").unwrap()
    } else {
        Connection::open(&path).unwrap()
    };
    seed(&mut database, case);
    assert_eq!(observe(&database, schema), case.before, "{} seed", case.id);
    let initial_bytes = (!case.temporary).then(|| fs::read(&path).unwrap());
    let first = configure_first(&mut database, &path, schema, case);
    assert_eq!(first, case.first, "{} first attempt", case.id);
    let second = if case.temporary {
        let result = configure(&mut database);
        RetryObservation {
            outcome: outcome(&result, &case.id),
            after: observe(&database, schema),
        }
    } else {
        database.close().unwrap();
        if first.outcome != Outcome::Opened && first.after == case.before {
            assert!(
                initial_bytes == Some(fs::read(&path).unwrap()),
                "{} database bytes",
                case.id
            );
        }
        let persisted = observe_disk(&path, schema);
        assert_eq!(
            persisted, first.after,
            "{} after closing first connection",
            case.id
        );
        let result = SessionHandler::open(&path)
            .map(drop)
            .map_err(|error| match error {
                SessionError::Schema(schema) => schema,
                other => panic!("{} unexpected reopen error: {other:?}", case.id),
            });
        RetryObservation {
            outcome: outcome(&result, &case.id),
            after: observe_disk(&path, schema),
        }
    };
    assert_eq!(second, case.second, "{} second attempt", case.id);
}

#[test]
fn session_configuration_matches_lean_oracle() {
    let expected_schema = current_schema().unwrap();
    let mut checked = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        check_case(&case, &expected_schema);
        checked += 1;
    }
    assert!(checked > 0, "session configuration corpus is empty");
}
