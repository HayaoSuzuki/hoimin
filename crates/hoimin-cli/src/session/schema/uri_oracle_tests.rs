use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};
use serde::Deserialize;

use super::super::{SessionError, SessionHandler};
use super::{SchemaError, immutable_uri};

const CORPUS: &str = include_str!("../../../../../formal/HoiminOracle/corpus/session-uri.jsonl");

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    unix_only: bool,
    sqlite_file: bool,
    bytes: Vec<u8>,
    uri: String,
}

#[cfg(unix)]
fn native_path(bytes: &[u8]) -> &Path {
    use std::os::unix::ffi::OsStrExt;
    Path::new(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn native_path(bytes: &[u8]) -> &Path {
    Path::new(std::str::from_utf8(bytes).unwrap())
}

fn contents(directory: &Path) -> BTreeMap<OsString, Vec<u8>> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect()
}

fn seed_database(connection: Connection, marker: &str) {
    let mode: String = connection
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    connection
        .execute_batch("CREATE TABLE fixture(marker TEXT NOT NULL)")
        .unwrap();
    connection
        .execute("INSERT INTO fixture VALUES (?1)", params![marker])
        .unwrap();
    connection.close().unwrap();
}

fn check_sqlite_file(spelling: &Path, id: &str) -> bool {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(spelling);
    let directory = path.parent().unwrap();
    fs::create_dir_all(directory).unwrap();
    let connection = match Connection::open(&path) {
        Ok(connection) => connection,
        Err(error) if spelling.to_str().is_none() => {
            assert!(
                contents(directory).is_empty(),
                "{id} direct open changed files"
            );
            eprintln!("{id}: direct SQLite open cannot create this pathname: {error}");
            return false;
        }
        Err(error) => panic!("{id}: could not initialize intended SQLite path: {error}"),
    };
    seed_database(connection, "intended file");
    seed_database(
        Connection::open(directory.join("session")).unwrap(),
        "query neighbor",
    );
    seed_database(
        Connection::open(directory.join("session%.sqlite3")).unwrap(),
        "percent neighbor",
    );
    let before = contents(directory);

    let canonical = fs::canonicalize(&path).unwrap();
    let reader = Connection::open_with_flags(
        immutable_uri(&canonical).unwrap(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap_or_else(|error| panic!("{id}: immutable open failed: {error}"));
    assert!(reader.is_readonly("main").unwrap(), "{id}");
    let marker: String = reader
        .query_row("SELECT marker FROM fixture", [], |row| row.get(0))
        .unwrap();
    assert_eq!(marker, "intended file", "{id} SQLite opened the wrong path");
    drop(reader);

    let rejected = SessionHandler::open(&path);
    assert!(
        matches!(
            rejected,
            Err(SessionError::Schema(SchemaError::InvalidIdentity))
        ),
        "{id} expected rejection of the intended foreign schema"
    );
    assert!(
        contents(directory) == before,
        "{id} admission changed files"
    );
    true
}

#[test]
fn session_immutable_uri_matches_lean_oracle() {
    let mut encodings = 0;
    let mut opened = 0;
    let mut unavailable = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        if case.unix_only && !cfg!(unix) {
            continue;
        }
        let spelling = native_path(&case.bytes);
        assert_eq!(immutable_uri(spelling).unwrap(), case.uri, "{}", case.id);
        encodings += 1;
        if case.sqlite_file {
            if check_sqlite_file(spelling, &case.id) {
                opened += 1;
            } else {
                unavailable += 1;
            }
        }
    }
    assert!(encodings > 0 && opened > 0, "missing URI oracle coverage");
    eprintln!("session URI encodings={encodings} SQLite_files={opened} unavailable={unavailable}");
}

#[cfg(windows)]
#[test]
fn session_immutable_uri_rejects_unpaired_windows_surrogate() {
    use std::os::windows::ffi::OsStringExt;
    let spelling = OsString::from_wide(&[0xD800]);
    let path = Path::new(&spelling);
    assert!(
        matches!(immutable_uri(path), Err(rusqlite::Error::InvalidPath(rejected)) if rejected == path)
    );
}
