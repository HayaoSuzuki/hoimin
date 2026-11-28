use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde::Deserialize;

use super::super::{SessionError, SessionHandler};
use super::{SCHEMA_VERSION, SchemaError, initialize};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/session-catalog.jsonl");

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    wal: bool,
    edits: Vec<Edit>,
    admitted: bool,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Edit {
    Replace {
        name: String,
        before: String,
        after: String,
    },
    Remove {
        name: String,
    },
    Add {
        sql: String,
    },
    ReverseCreation,
    ShiftRootPages,
}

#[derive(Clone)]
struct Definition {
    kind: String,
    name: String,
    sql: String,
    root_page: i64,
}

fn definitions(connection: &Connection) -> Vec<Definition> {
    connection
        .prepare(
            "SELECT type,name,sql,rootpage FROM sqlite_schema
             WHERE sql IS NOT NULL AND name NOT GLOB 'sqlite_*'
             ORDER BY rowid",
        )
        .unwrap()
        .query_map([], |row| {
            Ok(Definition {
                kind: row.get(0)?,
                name: row.get(1)?,
                sql: row.get(2)?,
                root_page: row.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn reference_definitions() -> Vec<Definition> {
    let mut connection = Connection::open_in_memory().unwrap();
    let transaction = connection.transaction().unwrap();
    initialize(&transaction).unwrap();
    transaction.commit().unwrap();
    definitions(&connection)
}

fn edited_definitions(reference: &[Definition], case: &Case) -> Vec<Definition> {
    let mut objects = reference.to_vec();
    for edit in &case.edits {
        match edit {
            Edit::Replace {
                name,
                before,
                after,
            } => {
                let object = objects
                    .iter_mut()
                    .find(|row| &row.name == name)
                    .unwrap_or_else(|| panic!("{} missing definition {name}", case.id));
                assert_eq!(object.sql.matches(before).count(), 1, "{} {name}", case.id);
                object.sql = object.sql.replacen(before, after, 1);
            }
            Edit::Remove { name } => {
                let index = objects
                    .iter()
                    .position(|row| &row.name == name)
                    .unwrap_or_else(|| panic!("{} missing definition {name}", case.id));
                objects.remove(index);
            }
            Edit::ReverseCreation => {
                objects.reverse();
                objects.sort_by_key(|row| row.kind != "table");
            }
            Edit::Add { .. } | Edit::ShiftRootPages => {}
        }
    }
    objects
}

fn check_seed(connection: &Connection, reference: &[Definition], case: &Case) {
    let observed = definitions(connection);
    for edit in &case.edits {
        match edit {
            Edit::Replace { name, after, .. } => {
                let changed = observed.iter().find(|row| &row.name == name).unwrap();
                assert!(
                    changed.sql.contains(after),
                    "{} SQLite stored {:?}",
                    case.id,
                    changed.sql
                );
            }
            Edit::ReverseCreation => {
                assert!(
                    reference
                        .iter()
                        .map(|row| &row.name)
                        .ne(observed.iter().map(|row| &row.name)),
                    "{} creation order did not change",
                    case.id
                );
            }
            Edit::ShiftRootPages => {
                let pages = |rows: &[Definition]| {
                    rows.iter()
                        .map(|row| (row.name.clone(), row.root_page))
                        .collect::<BTreeMap<_, _>>()
                };
                assert_ne!(
                    pages(reference),
                    pages(&observed),
                    "{} unchanged root pages",
                    case.id
                );
            }
            Edit::Remove { .. } | Edit::Add { .. } => {}
        }
    }
}

fn seed(path: &Path, reference: &[Definition], case: &Case) {
    let connection = Connection::open(path).unwrap();
    let shift_pages = case
        .edits
        .iter()
        .any(|edit| matches!(edit, Edit::ShiftRootPages));
    if shift_pages {
        connection
            .execute_batch("CREATE TABLE page_shift(value INTEGER)")
            .unwrap();
    }
    for row in edited_definitions(reference, case) {
        connection
            .execute_batch(&row.sql)
            .unwrap_or_else(|error| panic!("{} {}: {error}", case.id, row.name));
    }
    if shift_pages {
        connection.execute_batch("DROP TABLE page_shift").unwrap();
    }
    for edit in &case.edits {
        if let Edit::Add { sql } = edit {
            connection
                .execute_batch(sql)
                .unwrap_or_else(|error| panic!("{}: {error}", case.id));
        }
    }
    check_seed(&connection, reference, case);
    connection
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .unwrap();
    let journal = if case.wal { "wal" } else { "delete" };
    let mode: String = connection
        .query_row(&format!("PRAGMA journal_mode={journal}"), [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(mode, journal, "{}", case.id);
    connection.close().unwrap();
}

fn disk_contents(path: &Path) -> BTreeMap<OsString, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect()
}

fn check_case(reference: &[Definition], case: &Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    seed(&path, reference, case);
    let before = disk_contents(directory.path());
    let admitted = match SessionHandler::open(&path) {
        Ok(_) => true,
        Err(SessionError::Schema(SchemaError::InvalidIdentity)) => {
            assert!(
                before == disk_contents(directory.path()),
                "{} rejected open changed disk",
                case.id
            );
            false
        }
        Err(other) => panic!("{} unexpected admission error: {other:?}", case.id),
    };
    assert_eq!(admitted, case.admitted, "{} catalog admission", case.id);
}

#[test]
fn session_catalog_matches_lean_oracle() {
    let reference = reference_definitions();
    let mut checked = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        check_case(&reference, &case);
        checked += 1;
    }
    assert!(checked > 0, "no comparable catalog cases");
}
