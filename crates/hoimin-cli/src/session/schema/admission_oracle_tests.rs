use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, ErrorCode, params};
use serde::Deserialize;

use super::{
    SchemaError, current_schema, initialize, inspect_existing_database, open, schema_objects,
};
use crate::artifacts::SqliteCompanions;
use crate::file_links::file_link_count;

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/session-admission.jsonl");
const DIGEST: [u8; 32] = [42; 32];

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    unix_only: bool,
    database: Database,
    journal: Journal,
    companions: Companions,
    expected: Outcome,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Database {
    Missing,
    ZeroByte,
    Directory,
    Shared,
    Malformed,
    Sqlite {
        version: i64,
        application_id: i64,
        objects: Objects,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Journal {
    Rollback,
    Wal,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Objects {
    Empty,
    Canonical,
    Foreign,
}

#[derive(Deserialize)]
struct Companions {
    journal: Companion,
    wal: Companion,
    shm: Companion,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Companion {
    Missing,
    Regular,
    Shared,
    Directory,
    Symlink,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Slot {
    Journal,
    Wal,
    Shm,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Rejection {
    NonregularCompanion { slot: Slot },
    NonregularDatabase,
    SharedFile,
    Malformed,
    UnsupportedVersion { version: i64 },
    InvalidIdentity,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Rejected {
        reason: Rejection,
    },
    Opened {
        version: i64,
        foreign_keys: bool,
        journal: String,
        fingerprint_rows: usize,
    },
}

#[derive(Debug, Eq, PartialEq)]
enum Contents {
    File { bytes: Vec<u8>, links: u64 },
    Directory(Vec<OsString>),
    Link(PathBuf),
}

fn directory_state(root: &Path) -> BTreeMap<OsString, (fs::Permissions, Contents)> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = entry.metadata().unwrap();
            let contents = if metadata.is_symlink() {
                Contents::Link(fs::read_link(&path).unwrap())
            } else if metadata.is_dir() {
                let mut children = fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect::<Vec<_>>();
                children.sort();
                Contents::Directory(children)
            } else {
                assert!(
                    metadata.is_file(),
                    "unexpected fixture node: {}",
                    path.display()
                );
                Contents::File {
                    bytes: fs::read(&path).unwrap(),
                    links: file_link_count(&fs::File::open(&path).unwrap()).unwrap(),
                }
            };
            (entry.file_name(), (metadata.permissions(), contents))
        })
        .collect()
}

fn create_shared(path: &Path, source: &Path, bytes: &[u8]) {
    fs::write(source, bytes).unwrap();
    fs::hard_link(source, path).unwrap();
}

fn create_database(path: &Path, spec: &Database, journal: Journal) {
    match spec {
        Database::Missing => {}
        Database::ZeroByte => fs::write(path, []).unwrap(),
        Database::Directory => fs::create_dir(path).unwrap(),
        Database::Shared => create_shared(path, &path.with_extension("source"), b""),
        Database::Malformed => fs::write(path, b"ordinary bytes in a non-SQLite file").unwrap(),
        Database::Sqlite {
            version,
            application_id,
            objects,
        } => {
            let mut connection = Connection::open(path).unwrap();
            match objects {
                Objects::Empty => {}
                Objects::Canonical => {
                    let transaction = connection.transaction().unwrap();
                    initialize(&transaction).unwrap();
                    transaction
                        .execute(
                            "INSERT INTO fingerprints VALUES (?1, 17)",
                            params![&DIGEST[..]],
                        )
                        .unwrap();
                    transaction.commit().unwrap();
                }
                Objects::Foreign => connection
                    .execute_batch("CREATE TABLE example(value); INSERT INTO example VALUES (17)")
                    .unwrap(),
            }
            connection
                .pragma_update(None, "user_version", version)
                .unwrap();
            connection
                .pragma_update(None, "application_id", application_id)
                .unwrap();
            if matches!(journal, Journal::Wal) {
                let mode: String = connection
                    .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
                    .unwrap();
                assert_eq!(mode, "wal");
            }
            drop(connection);
            assert_eq!(
                inspect_existing_database(path).unwrap(),
                matches!(journal, Journal::Wal),
                "fixture header must agree with its configured journal mode"
            );
        }
    }
}

fn create_companion(path: &Path, kind: Companion) {
    match kind {
        Companion::Missing => assert!(!path.exists(), "unexpected fixture companion"),
        Companion::Regular => fs::write(path, []).unwrap(),
        Companion::Shared => create_shared(
            path,
            &path.with_extension({
                let mut suffix = path.extension().unwrap().to_owned();
                suffix.push(".source");
                suffix
            }),
            b"ordinary hardlinked file",
        ),
        Companion::Directory => fs::create_dir(path).unwrap(),
        Companion::Symlink => {
            let target = path.with_extension("absent-target");
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, path).unwrap();
            #[cfg(not(unix))]
            panic!("Unix-only symlink fixture: {target:?}");
        }
    }
}

fn classify_rejection(error: &SchemaError, companions: &SqliteCompanions) -> Rejection {
    match error {
        SchemaError::SharedFile => Rejection::SharedFile,
        SchemaError::UnsupportedVersion(version) => {
            Rejection::UnsupportedVersion { version: *version }
        }
        SchemaError::InvalidIdentity => Rejection::InvalidIdentity,
        SchemaError::Inspect(error) if error.kind() == std::io::ErrorKind::InvalidInput => {
            let message = error.to_string();
            for (slot, path) in [
                (Slot::Journal, &companions.journal),
                (Slot::Wal, &companions.wal),
                (Slot::Shm, &companions.shm),
            ] {
                if message.ends_with(&path.to_string_lossy()[..]) {
                    return Rejection::NonregularCompanion { slot };
                }
            }
            assert_eq!(message, "session database must be a regular file");
            Rejection::NonregularDatabase
        }
        SchemaError::Database(rusqlite::Error::SqliteFailure(failure, _))
            if failure.code == ErrorCode::NotADatabase =>
        {
            Rejection::Malformed
        }
        other => panic!("unexpected session admission error: {other:?}"),
    }
}

fn inspect_opened(connection: &Connection, id: &str) -> Outcome {
    assert_eq!(
        schema_objects(connection).unwrap(),
        current_schema().unwrap(),
        "{id} schema after opening"
    );
    let fingerprints = connection
        .prepare("SELECT digest, schema_version FROM fingerprints ORDER BY digest")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (digest, version) in &fingerprints {
        assert_eq!(digest.as_slice(), DIGEST);
        assert_eq!(*version, 17);
    }
    Outcome::Opened {
        version: connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap(),
        foreign_keys: connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap(),
        journal: connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap(),
        fingerprint_rows: fingerprints.len(),
    }
}

fn check_case(case: &Case) {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("session #%[1].db");
    create_database(&database, &case.database, case.journal);
    let companions = SqliteCompanions::new(&database);
    for (path, kind) in [
        (&companions.journal, case.companions.journal),
        (&companions.wal, case.companions.wal),
        (&companions.shm, case.companions.shm),
    ] {
        create_companion(path, kind);
    }
    let before = directory_state(temp.path());
    let outcome = match open(&database) {
        Ok(connection) => inspect_opened(&connection, &case.id),
        Err(error) => {
            assert_eq!(
                directory_state(temp.path()),
                before,
                "{} side effects",
                case.id
            );
            Outcome::Rejected {
                reason: classify_rejection(&error, &companions),
            }
        }
    };
    assert_eq!(outcome, case.expected, "{}", case.id);
}

#[test]
fn session_admission_matches_lean_oracle() {
    let mut checked = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        if !case.unix_only || cfg!(unix) {
            check_case(&case);
            checked += 1;
        }
    }
    assert!(checked > 0, "no comparable session admission cases");
}
