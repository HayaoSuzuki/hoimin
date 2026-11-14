use std::io::Read;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, Transaction, TransactionBehavior};
use thiserror::Error;

pub(crate) const SCHEMA_VERSION: i64 = 5;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error(
        "SQLite schema version {0} is unsupported by this hoimin build (supported version: {SCHEMA_VERSION}); start a new session at a different --session path"
    )]
    UnsupportedVersion(i64),
    #[error(
        "SQLite database is not an empty database or a compatible hoimin session; start a new session at a different --session path"
    )]
    InvalidIdentity,
    #[error(
        "SQLite session file shares its file with another hard link; start a new session at a different --session path"
    )]
    SharedFile,
    #[error("failed to inspect SQLite session: {0}")]
    Inspect(#[from] std::io::Error),
    #[error("SQLite refused WAL journal mode and remained in {0} mode")]
    JournalMode(String),
    #[error("failed to configure or initialize SQLite session: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Inspects an existing database read-only before opening it for session writes.
/// Existing companions must be regular, single-link files before any SQLite
/// connection can touch them. Only empty databases and the current schema may enable WAL.
pub(crate) fn open(path: &Path) -> Result<Connection, SchemaError> {
    inspect_existing_companions(path)?;
    let expected = current_schema()?;
    if path.exists() {
        // Closed WAL databases need an immutable advisory read to avoid
        // creating companion files. Rollback-mode and empty databases use
        // ordinary SQLite reads so a concurrent initializer's locks apply.
        let connection = if inspect_existing_database(path)? {
            Connection::open_with_flags(
                immutable_uri(path)?,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
            )?
        } else {
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?
        };
        connection.busy_timeout(Duration::from_secs(5))?;
        admit(&connection, &expected)?;
    }
    let mut connection = Connection::open(path)?;
    configure_with_schema(&mut connection, &expected, || {})?;
    Ok(connection)
}

fn inspect_existing_companions(database: &Path) -> Result<(), SchemaError> {
    let companions = crate::artifacts::SqliteCompanions::new(database);
    // Even a read-only SQLite connection can touch sidecars. Validate every
    // existing descriptor before opening the database, without creating files.
    for path in [&companions.journal, &companions.wal, &companions.shm] {
        let invalid = || {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "session companion must be a regular file, not a symlink: {}",
                    path.display()
                ),
            )
        };
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.is_file() => return Err(invalid().into()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        let file = crate::artifacts::open_regular_file(path)?.ok_or_else(invalid)?;
        if crate::file_links::file_link_count(&file)? > 1 {
            return Err(SchemaError::SharedFile);
        }
    }
    Ok(())
}

fn inspect_existing_database(path: &Path) -> Result<bool, SchemaError> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "session database must be a regular file",
        )
        .into());
    }
    inspect_database_file(std::fs::File::open(path)?)
}

// Inspect the opened capability again: the path may have changed after the preliminary metadata.
fn inspect_database_file(mut file: std::fs::File) -> Result<bool, SchemaError> {
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "session database must be a regular file",
        )
        .into());
    }
    if crate::file_links::file_link_count(&file)? > 1 {
        return Err(SchemaError::SharedFile);
    }
    let mut header = [0; 20];
    match file.read_exact(&mut header) {
        Ok(()) => {
            Ok(header.starts_with(b"SQLite format 3\0") && header[18] == 2 && header[19] == 2)
        }
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg_attr(
    unix,
    expect(
        clippy::unnecessary_wraps,
        reason = "Windows pathname conversion remains fallible"
    )
)]
fn immutable_uri(path: &Path) -> Result<String, rusqlite::Error> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = path
        .to_str()
        .ok_or_else(|| rusqlite::Error::InvalidPath(path.to_owned()))?
        .as_bytes();

    // Escape every native pathname byte, including separators. SQLite decodes
    // these after parsing URI authority/query syntax, preserving Unix bytes and
    // Windows drive, UNC, and verbatim paths without treating them as a host.
    let mut uri = String::from("file:");
    for &byte in bytes {
        uri.push('%');
        uri.push(char::from(HEX[usize::from(byte >> 4)]));
        uri.push(char::from(HEX[usize::from(byte & 15)]));
    }
    uri.push_str("?immutable=1");
    Ok(uri)
}

#[cfg(test)]
pub(crate) fn configure(connection: &mut Connection) -> Result<(), SchemaError> {
    configure_observed(connection, || {})
}

#[cfg(test)]
fn configure_observed(
    connection: &mut Connection,
    after_initial_version_read: impl FnOnce(),
) -> Result<(), SchemaError> {
    configure_with_schema(connection, &current_schema()?, after_initial_version_read)
}

type SchemaObjects = Vec<(String, String, String, Option<String>)>;

fn schema_objects(connection: &Connection) -> Result<SchemaObjects, rusqlite::Error> {
    let mut statement = connection
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")?;
    statement
        .query_map([], |row| {
            let sql: Option<String> = row.get(3)?;
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                sql.map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" ")),
            ))
        })?
        .collect()
}

fn current_schema() -> Result<SchemaObjects, rusqlite::Error> {
    // The canonical schema includes constraints, foreign keys, and indexes, not
    // merely table names shared with an unrelated schema using the same version.
    let mut reference = Connection::open_in_memory()?;
    let transaction = reference.transaction()?;
    initialize(&transaction)?;
    transaction.commit()?;
    schema_objects(&reference)
}

fn admit(connection: &Connection, expected: &SchemaObjects) -> Result<i64, SchemaError> {
    admit_observed(connection, expected, || {})
}

fn admit_observed(
    connection: &Connection,
    expected: &SchemaObjects,
    after_version_read: impl FnOnce(),
) -> Result<i64, SchemaError> {
    // Keep version, application ID, and schema objects in the same SQLite
    // snapshot. Reuse an existing IMMEDIATE transaction during initialization.
    let snapshot = if connection.is_autocommit() {
        Some(connection.unchecked_transaction()?)
    } else {
        None
    };
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    after_version_read();
    if !matches!(version, 0 | SCHEMA_VERSION) {
        return Err(SchemaError::UnsupportedVersion(version));
    }
    let application_id: i64 =
        connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if application_id != 0 {
        return Err(SchemaError::InvalidIdentity);
    }
    let objects = schema_objects(connection)?;
    if (version == 0 && objects.is_empty()) || (version == SCHEMA_VERSION && objects == *expected) {
        if let Some(snapshot) = snapshot {
            snapshot.commit()?;
        }
        Ok(version)
    } else {
        Err(SchemaError::InvalidIdentity)
    }
}

fn configure_with_schema(
    connection: &mut Connection,
    expected: &SchemaObjects,
    after_initial_version_read: impl FnOnce(),
) -> Result<(), SchemaError> {
    const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    let version = admit(connection, expected)?;
    after_initial_version_read();
    if version == 0 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Another initializer may have committed after the first inspection.
        if admit(&transaction, expected)? == 0 {
            initialize(&transaction)?;
        }
        transaction.commit()?;
    }
    enable_wal(connection, BUSY_TIMEOUT)
}

fn initialize(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(
        "CREATE TABLE fingerprints (
             digest BLOB PRIMARY KEY NOT NULL CHECK(length(digest) = 32),
             schema_version INTEGER NOT NULL
         );
         CREATE TABLE runs (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             run_id TEXT NOT NULL UNIQUE,
             fingerprint BLOB NOT NULL REFERENCES fingerprints(digest),
             finished INTEGER NOT NULL DEFAULT 0 CHECK(finished IN (0, 1)),
             complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0, 1)),
             max_mutants BLOB NOT NULL
             CHECK(typeof(max_mutants)='blob' AND length(max_mutants)=8
                   AND max_mutants > zeroblob(8))
         );
         CREATE INDEX runs_resume ON runs(fingerprint, complete, id DESC);
         CREATE TABLE candidates (
             run_id TEXT NOT NULL,
             mutant_id TEXT NOT NULL,
             sequence INTEGER NOT NULL,
             path TEXT NOT NULL,
             span_start INTEGER NOT NULL,
             span_length INTEGER NOT NULL,
             original TEXT NOT NULL,
             replacement TEXT NOT NULL,
             operator TEXT NOT NULL,
             line INTEGER NOT NULL,
             column_number INTEGER NOT NULL,
             symbol TEXT,
             file_hash TEXT NOT NULL,
             PRIMARY KEY(run_id, mutant_id),
             FOREIGN KEY(run_id) REFERENCES runs(run_id) DEFERRABLE INITIALLY DEFERRED
         );
         CREATE TABLE results (
             run_id TEXT NOT NULL,
             mutant_id TEXT NOT NULL,
             outcome_json TEXT NOT NULL,
             resource_mode TEXT NOT NULL,
             PRIMARY KEY(run_id, mutant_id),
             FOREIGN KEY(run_id, mutant_id) REFERENCES candidates(run_id, mutant_id)
                 DEFERRABLE INITIALLY DEFERRED
         );
         CREATE TABLE diagnostics (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             run_id TEXT NOT NULL,
             mutant_id TEXT NOT NULL,
             level TEXT NOT NULL,
             code TEXT NOT NULL,
             message TEXT NOT NULL,
             FOREIGN KEY(run_id, mutant_id) REFERENCES candidates(run_id, mutant_id)
                 DEFERRABLE INITIALLY DEFERRED
         );
         CREATE INDEX diagnostics_result ON diagnostics(run_id, mutant_id, id);",
    )?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)
}

fn enable_wal(connection: &Connection, timeout: Duration) -> Result<(), SchemaError> {
    const RETRY_BACKOFF: Duration = Duration::from_millis(5);

    let deadline = Instant::now() + timeout;
    loop {
        match connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0)) {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") || mode.eq_ignore_ascii_case("memory") => {
                return Ok(());
            }
            Ok(mode) => return Err(SchemaError::JournalMode(mode)),
            Err(error) if is_lock_contention(&error) && Instant::now() < deadline => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                thread::sleep(RETRY_BACKOFF.min(remaining));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn is_lock_contention(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(failure.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{Arc, Condvar, Mutex, PoisonError};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn issue_1085_single_link_zero_byte_session_initializes_and_reopens() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        std::fs::write(&path, []).unwrap();
        let handler = super::super::SessionHandler::open(&path).unwrap();

        assert_eq!(user_version(&handler.connection), SCHEMA_VERSION);
        drop(handler);
        let reopened = super::super::SessionHandler::open(&path).unwrap();
        assert_eq!(user_version(&reopened.connection), SCHEMA_VERSION);
    }

    #[test]
    fn issue_1085_empty_hardlinked_session_is_rejected_without_disk_changes() {
        assert_hardlinked_session_rejected(false);
    }

    #[test]
    fn issue_1085_current_hardlinked_session_is_rejected_without_disk_changes() {
        assert_hardlinked_session_rejected(true);
    }

    fn assert_hardlinked_session_rejected(current: bool) {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        if current {
            drop(super::super::SessionHandler::open(&source).unwrap());
        } else {
            std::fs::write(&source, []).unwrap();
        }
        let alias = temp.path().join("session.sqlite3");
        std::fs::hard_link(&source, &alias).unwrap();
        let files_before = directory_bytes(temp.path());
        let result = super::super::SessionHandler::open(&alias);
        let rejected = result.is_err();
        drop(result);
        assert!(
            directory_bytes(temp.path()) == files_before,
            "hardlink admission changed database, companion, or lock artifacts"
        );
        assert!(rejected, "hardlinked session was admitted");
    }

    #[derive(Clone, Copy, Debug)]
    enum CompanionDatabase {
        Missing,
        Empty,
        Current,
        Foreign,
        Unsupported,
    }

    fn prepare_companion_database(path: &Path, state: CompanionDatabase) {
        match state {
            CompanionDatabase::Missing => {}
            CompanionDatabase::Empty => std::fs::write(path, []).unwrap(),
            CompanionDatabase::Current => drop(super::super::SessionHandler::open(path).unwrap()),
            CompanionDatabase::Foreign => Connection::open(path)
                .unwrap()
                .execute_batch(
                    "CREATE TABLE app_data(value); INSERT INTO app_data VALUES ('keep');",
                )
                .unwrap(),
            CompanionDatabase::Unsupported => Connection::open(path)
                .unwrap()
                .pragma_update(None, "user_version", 99)
                .unwrap(),
        }
    }

    fn directory_snapshot(path: &Path) -> Vec<(std::ffi::OsString, Vec<u8>, std::fs::Permissions)> {
        let mut entries = std::fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name(),
                    std::fs::read(entry.path()).unwrap(),
                    entry.metadata().unwrap().permissions(),
                )
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    #[rstest::rstest]
    #[case::missing(CompanionDatabase::Missing)]
    #[case::empty(CompanionDatabase::Empty)]
    #[case::current(CompanionDatabase::Current)]
    #[case::foreign(CompanionDatabase::Foreign)]
    #[case::unsupported(CompanionDatabase::Unsupported)]
    fn issue_1094_shared_companions_are_rejected_before_any_sqlite_side_effects(
        #[case] state: CompanionDatabase,
        #[values("-journal", "-wal", "-shm")] suffix: &str,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("session.sqlite3");
        prepare_companion_database(&database, state);
        let source = temp.path().join("source.py");
        std::fs::write(&source, []).unwrap();
        let companion = database.with_file_name(format!("session.sqlite3{suffix}"));
        std::fs::hard_link(&source, &companion).unwrap();
        let before = directory_snapshot(temp.path());

        let result = super::super::SessionHandler::open(&database);
        let rejected = matches!(
            &result,
            Err(super::super::SessionError::Schema(SchemaError::SharedFile))
        );
        drop(result);
        let after = directory_snapshot(temp.path());

        assert!(rejected, "{state:?} {suffix}");
        assert_eq!(before, after, "{state:?} {suffix}");
    }

    #[rstest::rstest]
    #[case::companiondatabase_missing_1_journal_1(CompanionDatabase::Missing, "-journal")]
    #[case::companiondatabase_missing_1_wal_2(CompanionDatabase::Missing, "-wal")]
    #[case::companiondatabase_missing_1_shm_3(CompanionDatabase::Missing, "-shm")]
    #[case::companiondatabase_empty_2_journal_1(CompanionDatabase::Empty, "-journal")]
    #[case::companiondatabase_empty_2_wal_2(CompanionDatabase::Empty, "-wal")]
    #[case::companiondatabase_empty_2_shm_3(CompanionDatabase::Empty, "-shm")]
    #[case::companiondatabase_current_3_journal_1(CompanionDatabase::Current, "-journal")]
    #[case::companiondatabase_current_3_wal_2(CompanionDatabase::Current, "-wal")]
    #[case::companiondatabase_current_3_shm_3(CompanionDatabase::Current, "-shm")]
    fn issue_1094_single_link_companions_allow_initialization_and_reopen(
        #[case] state: CompanionDatabase,
        #[case] suffix: &str,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        prepare_companion_database(&path, state);
        std::fs::write(path.with_file_name(format!("session.sqlite3{suffix}")), []).unwrap();
        let handler = super::super::SessionHandler::open(&path).unwrap();

        assert_eq!(user_version(&handler.connection), SCHEMA_VERSION);
        drop(handler);
        let reopened = super::super::SessionHandler::open(&path).unwrap();
        assert_eq!(user_version(&reopened.connection), SCHEMA_VERSION);
    }

    #[test]
    fn issue_1094_populated_single_link_wal_and_shm_allow_session_open() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        drop(super::super::SessionHandler::open(&path).unwrap());
        let writer = Connection::open(&path).unwrap();
        writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        // Populate WAL with ordinary header updates, ending at the current
        // version. Keep the idle connection alive so SQLite retains WAL/SHM.
        writer
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        writer
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        let companions = crate::artifacts::SqliteCompanions::new(&path);
        for companion in [&companions.wal, &companions.shm] {
            let file = std::fs::File::open(companion).unwrap();

            assert!(file.metadata().unwrap().len() > 0);
            assert_eq!(crate::file_links::file_link_count(&file).unwrap(), 1);
        }
        let handler = super::super::SessionHandler::open(&path).unwrap();
        assert_eq!(user_version(&handler.connection), SCHEMA_VERSION);
        drop(handler);
        drop(writer);
        let reopened = super::super::SessionHandler::open(&path).unwrap();
        assert_eq!(user_version(&reopened.connection), SCHEMA_VERSION);
    }

    #[rstest::rstest]
    #[case::journal_1("-journal")]
    #[case::wal_2("-wal")]
    #[case::shm_3("-shm")]
    fn issue_1094_companion_directories_are_rejected_before_database_creation(
        #[case] suffix: &str,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let directory = path.with_file_name(format!("session.sqlite3{suffix}"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("keep"), b"keep").unwrap();

        assert!(matches!(
            super::super::SessionHandler::open(&path),
            Err(super::super::SessionError::Schema(SchemaError::Inspect(_)))
        ));
        assert!(!path.exists());
        assert_eq!(std::fs::read(directory.join("keep")).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[rstest::rstest]
    #[case::journal_1_false_1("-journal", false)]
    #[case::journal_1_true_2("-journal", true)]
    #[case::wal_2_false_1("-wal", false)]
    #[case::wal_2_true_2("-wal", true)]
    #[case::shm_3_false_1("-shm", false)]
    #[case::shm_3_true_2("-shm", true)]
    fn issue_1094_companion_symlinks_are_rejected_before_database_creation(
        #[case] suffix: &str,
        #[case] dangling: bool,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let source = temp.path().join("source.py");
        if !dangling {
            std::fs::write(&source, b"keep").unwrap();
        }
        let companion = path.with_file_name(format!("session.sqlite3{suffix}"));
        std::os::unix::fs::symlink(&source, &companion).unwrap();

        assert!(matches!(
            super::super::SessionHandler::open(&path),
            Err(super::super::SessionError::Schema(SchemaError::Inspect(_)))
        ));
        assert!(!path.exists());
        assert!(
            std::fs::symlink_metadata(&companion)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        if dangling {
            assert!(!source.exists());
        } else {
            assert_eq!(std::fs::read(&source).unwrap(), b"keep");
        }
    }

    #[cfg(unix)]
    #[test]
    fn issue_1094_unreadable_companion_fails_before_database_creation() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid has no arguments or memory preconditions.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let companion = path.with_file_name("session.sqlite3-wal");
        std::fs::write(&companion, b"keep").unwrap();
        let permissions = std::fs::metadata(&companion).unwrap().permissions();
        std::fs::set_permissions(&companion, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = super::super::SessionHandler::open(&path);
        std::fs::set_permissions(&companion, permissions).unwrap();

        assert!(matches!(
            result,
            Err(super::super::SessionError::Schema(SchemaError::Inspect(_)))
        ));
        assert!(!path.exists());
        assert_eq!(std::fs::read(&companion).unwrap(), b"keep");
    }

    // These fixtures are closed before open: admission must not change their
    // journal mode, headers, contents, or leave SQLite companion files behind.
    #[rstest::rstest]
    #[case::create_table_application_data_value_text_insert_into_ap_1(
        "CREATE TABLE application_data(value TEXT); INSERT INTO application_data VALUES ('keep');"
    )]
    #[case::create_table_diagnostics_marker_text_insert_into_diagno_2(
        "CREATE TABLE diagnostics(marker TEXT); INSERT INTO diagnostics VALUES ('keep');"
    )]
    #[case::pragma_application_id_12345_3("PRAGMA application_id=12345;")]
    #[case::create_table_application_data_value_text_insert_into_ap_4(
        "CREATE TABLE application_data(value TEXT); INSERT INTO application_data VALUES ('keep'); PRAGMA user_version=5;"
    )]
    #[case::pragma_user_version_1_5("PRAGMA user_version=-1;")]
    #[case::pragma_user_version_6_6("PRAGMA user_version=6;")]
    #[case::create_view_application_view_as_select_1_7("CREATE VIEW application_view AS SELECT 1;")]
    #[case::pragma_journal_mode_wal_create_table_application_data_v_8(
        "PRAGMA journal_mode=WAL; CREATE TABLE application_data(value TEXT); INSERT INTO application_data VALUES ('keep');"
    )]
    fn admission_rejects_foreign_databases_without_disk_changes(#[case] sql: &str) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("foreign.sqlite3");
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(sql).unwrap();
        }
        assert_rejected_without_disk_changes(&path);
    }

    #[test]
    fn admission_rejects_old_golden_files_without_disk_changes() {
        for version in 1..=4 {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("old.sqlite3");
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/golden/sessions/schema-v{version}.sqlite3")),
                &path,
            )
            .unwrap();
            assert_rejected_without_disk_changes(&path);
        }
    }

    #[test]
    fn admission_rejects_previous_nullable_schema_without_disk_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("previous.sqlite3");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/sessions/schema-v5.sqlite3"),
            &path,
        )
        .unwrap();
        assert_rejected_without_disk_changes(&path);
    }

    fn closed_database_snapshot(path: &Path) -> (Vec<String>, i64, String) {
        // Fixture paths are generated temporary ASCII names. Immutable reads
        // keep the observer itself from creating WAL/SHM companion files.
        let connection = Connection::open_with_flags(
            format!("file:{}?immutable=1", path.display()),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .unwrap();
        (
            database_snapshot(&connection),
            user_version(&connection),
            connection
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap(),
        )
    }

    fn directory_bytes(directory: &Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (entry.file_name(), std::fs::read(entry.path()).unwrap())
            })
            .collect();
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
    }

    fn assert_rejected_without_disk_changes(path: &Path) {
        let before = closed_database_snapshot(path);
        let files_before = directory_bytes(path.parent().unwrap());
        // Use the public open boundary, including its connection-open behavior.
        let rejected = super::super::SessionHandler::open(path).is_err();
        let files_after = directory_bytes(path.parent().unwrap());
        assert!(
            files_after == files_before,
            "rejection changed database or companion files"
        );
        assert_eq!(closed_database_snapshot(path), before);
        assert!(rejected, "foreign or unsupported database was admitted");
    }

    #[rstest::rstest]
    #[case::false_1(false)]
    #[case::true_2(true)]
    fn admission_initializes_new_and_empty_on_disk_databases(#[case] existing_empty: bool) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        if existing_empty {
            Connection::open(&path)
                .unwrap()
                .execute_batch("VACUUM;")
                .unwrap();
        }
        let handler = super::super::SessionHandler::open(&path).unwrap();

        assert_eq!(user_version(&handler.connection), 5);
        assert_eq!(
            handler
                .connection
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
    }

    #[test]
    fn admission_rejects_current_schema_with_foreign_application_identity() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/golden/sessions/schema-v5-required-budget.sqlite3"),
            &path,
        )
        .unwrap();
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "application_id", 12345)
                .unwrap();
        }
        assert_rejected_without_disk_changes(&path);
    }

    #[test]
    fn admission_reopens_session_paths_with_uri_metacharacters() {
        let names = [
            "session with spaces.sqlite3",
            "日本語-session.sqlite3",
            "session#%25.sqlite3",
        ];
        for name in names {
            assert_session_path_reopens(std::ffi::OsStr::new(name));
            assert_foreign_path_untouched(std::ffi::OsStr::new(name));
        }
    }

    #[cfg(unix)]
    #[test]
    fn admission_reopens_session_paths_with_non_utf8_and_question_marks() {
        use std::os::unix::ffi::OsStrExt;
        let question = std::ffi::OsStr::from_bytes(b"session?immutable=0&mode=rw.sqlite3");
        assert_session_path_reopens(question);
        assert_foreign_path_untouched(question);

        let name = std::ffi::OsStr::from_bytes(b"session-\xff.sqlite3");
        let probe = tempfile::tempdir().unwrap();
        let probe_path = probe.path().join(name);
        if Connection::open(&probe_path).is_ok() {
            assert_session_path_reopens(name);
            assert_foreign_path_untouched(name);
        } else {
            // Some SQLite/VFS or filesystem environments reject non-UTF8
            // filenames even without URI access. Preserve that rejection.
            let temp = tempfile::tempdir().unwrap();
            let before = directory_bytes(temp.path());

            assert!(super::super::SessionHandler::open(temp.path().join(name)).is_err());
            assert!(
                directory_bytes(temp.path()) == before,
                "rejected path changed files"
            );
        }
    }

    fn assert_session_path_reopens(name: &std::ffi::OsStr) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(name);
        for _ in 0..2 {
            let handler = super::super::SessionHandler::open(&path).unwrap();
            assert_eq!(user_version(&handler.connection), 5);
        }
        let names: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [name.to_owned()]);
    }

    fn assert_foreign_path_untouched(name: &std::ffi::OsStr) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(name);
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE application_data(value TEXT); INSERT INTO application_data VALUES ('keep');").unwrap();
        }
        let before = directory_bytes(temp.path());
        assert!(super::super::SessionHandler::open(&path).is_err());
        assert!(
            directory_bytes(temp.path()) == before,
            "rejection changed literal pathname fixture or companions"
        );
    }

    #[test]
    fn admission_waits_for_rollback_mode_initializer_before_inspecting_schema() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let mut initializer = Connection::open(&path).unwrap();
        initializer.execute_batch("VACUUM;").unwrap();
        let transaction = initializer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        transaction.execute_batch("CREATE TABLE fingerprints (digest BLOB PRIMARY KEY NOT NULL CHECK(length(digest)=32), schema_version INTEGER NOT NULL);").unwrap();
        // Spill an unfinished initializer's schema to the main file while its
        // rollback journal and exclusive SQLite lock still protect it.
        transaction.cache_flush().unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            started_tx.send(()).unwrap();

            let result = super::super::SessionHandler::open(&path)
                .map(|_| ())
                .map_err(
                    // This callback only formats an unexpected failure of the waiting fixture opener.
                    #[cfg_attr(coverage_nightly, coverage(off))]
                    |error| error.to_string(),
                );
            result_tx.send(result).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let premature = result_rx.recv_timeout(Duration::from_millis(100));
        // The first initializer fails; the waiting opener can now initialize
        // the genuinely empty database rather than rejecting partial pages.
        transaction.rollback().unwrap();

        let result = match premature {
            Ok(result) => {
                // This branch only joins and diagnoses a failed initializer-lock assertion.
                #[cfg(coverage_nightly)]
                {
                    (#[coverage(off)]
                    || -> ! {
                        worker.join().unwrap();
                        panic!("admission bypassed initializer lock: {result:?}");
                    })()
                }
                #[cfg(not(coverage_nightly))]
                {
                    worker.join().unwrap();
                    panic!("admission bypassed initializer lock: {result:?}");
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                result_rx.recv_timeout(Duration::from_secs(5)).unwrap()
            }
            Err(error) => {
                // Disconnection means the fixture worker failed before publishing its result.
                #[cfg(coverage_nightly)]
                {
                    (#[coverage(off)]
                    || -> ! {
                        panic!("initializer worker disconnected: {error}");
                    })()
                }
                #[cfg(not(coverage_nightly))]
                {
                    panic!("initializer worker disconnected: {error}");
                }
            }
        };
        worker.join().unwrap();

        assert!(result.is_ok(), "waiting opener failed: {result:?}");
    }

    #[test]
    fn admission_reads_version_and_schema_from_one_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=WAL;")
            .unwrap();
        let observed = admit_observed(&connection, &current_schema().unwrap(), || {
            let mut initializer = Connection::open(&path).unwrap();
            configure(&mut initializer).unwrap();
        });

        assert!(
            matches!(observed, Ok(0)),
            "admission mixed an old version with newly committed schema: {observed:?}"
        );
        let mut connection = connection;
        configure(&mut connection).unwrap();
        assert_eq!(user_version(&connection), 5);
    }

    #[test]
    fn admission_rechecks_empty_schema_after_acquiring_initialization_lock() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        let mut before = None;

        let result = configure_observed(&mut connection, || {
            let other = Connection::open(&path).unwrap();
            other.execute_batch("CREATE TABLE application_data(value TEXT); INSERT INTO application_data VALUES ('keep');").unwrap();
            before = Some(database_snapshot(&other));
        });

        assert!(
            result.is_err(),
            "schema populated before lock must be rejected"
        );
        assert_eq!(user_version(&connection), 0);
        assert_eq!(database_snapshot(&connection), before.unwrap());
    }

    #[test]
    fn old_golden_databases_are_rejected_without_schema_or_data_changes() {
        for version in 1..=4 {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("session.sqlite3");
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/golden/sessions/schema-v{version}.sqlite3")),
                &path,
            )
            .unwrap();
            let mut connection = Connection::open(path).unwrap();
            let before = database_snapshot(&connection);

            let error = configure(&mut connection).expect_err("old schema must be rejected");

            let message = error.to_string();

            assert!(message.contains("unsupported"), "{message}");
            assert!(message.contains("new session"), "{message}");
            assert_eq!(user_version(&connection), version);
            assert_eq!(database_snapshot(&connection), before);
        }
    }

    fn database_snapshot(connection: &Connection) -> Vec<String> {
        let mut snapshot = Vec::new();
        let mut statement = connection
            .prepare("SELECT name,sql FROM sqlite_master ORDER BY type,name")
            .unwrap();
        let objects: Vec<(String, Option<String>)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (name, sql) in objects {
            snapshot.push(format!("{name}: {sql:?}"));
            if sql
                .as_deref()
                .is_some_and(|sql| sql.starts_with("CREATE TABLE"))
            {
                let mut rows = connection
                    .prepare(&format!("SELECT * FROM \"{name}\" ORDER BY rowid"))
                    .unwrap();
                let count = rows.column_count();
                snapshot.extend(
                    rows.query_map([], |row| {
                        let values = (0..count)
                            .map(|i| {
                                row.get::<_, rusqlite::types::Value>(i)
                                    .expect("column index is below count and Value accepts every SQLite type")
                            })
                            .collect::<Vec<_>>();
                        Ok(format!("{values:?}"))
                    })
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap(),
                );
            }
        }
        snapshot
    }

    #[test]
    fn configures_busy_timeout_on_the_handler_connection() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure(&mut connection).unwrap();

        let timeout: i64 = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();

        assert_eq!(timeout, 5_000);
    }

    #[rstest::rstest]
    #[case::select_mutant_id_level_code_message_from_diagnostics_where_run_id(
        "SELECT mutant_id,level,code,message FROM diagnostics
             WHERE run_id=?1 AND mutant_id=?2 ORDER BY id"
    )]
    #[case::delete_from_diagnostics_where_run_id_1_and_mutant_id_2(
        "DELETE FROM diagnostics WHERE run_id=?1 AND mutant_id=?2"
    )]
    fn diagnostics_queries_use_the_result_index(#[case] sql: &str) {
        let mut connection = Connection::open_in_memory().unwrap();
        configure(&mut connection).unwrap();

        let details = query_plan(&connection, sql);

        assert!(
            details
                .iter()
                .any(|detail| detail.contains("INDEX diagnostics_result")),
            "query did not use diagnostics_result: {details:?}"
        );
    }

    #[test]
    fn current_golden_reopens_without_changing_schema_or_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/golden/sessions/schema-v5-required-budget.sqlite3"),
            &path,
        )
        .unwrap();
        let before = database_snapshot(&Connection::open(&path).unwrap());
        for _ in 0..2 {
            let mut connection = Connection::open(&path).unwrap();
            configure(&mut connection).unwrap();

            assert_eq!(user_version(&connection), SCHEMA_VERSION);
            assert_eq!(database_snapshot(&connection), before);
        }
    }

    #[test]
    fn admitted_initialization_failure_rolls_back_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("VACUUM; PRAGMA max_page_count=3;")
            .unwrap();
        let files_before = directory_bytes(temp.path());
        // fingerprints and its primary-key index fit, but runs cannot be
        // allocated. This fails inside initialization after admission succeeds.
        assert!(matches!(
            configure(&mut connection),
            Err(SchemaError::Database(_))
        ));
        assert_eq!(user_version(&connection), 0);
        assert!(
            database_snapshot(&connection).is_empty(),
            "failed initialization retained schema objects"
        );
        drop(connection);
        assert!(
            directory_bytes(temp.path()) == files_before,
            "failed initialization changed files"
        );
    }

    #[test]
    fn conflicting_table_is_rejected_without_initialization() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE diagnostics(marker TEXT); INSERT INTO diagnostics VALUES ('preserve me');").unwrap();
        let before = database_snapshot(&connection);

        assert!(matches!(
            configure(&mut connection),
            Err(SchemaError::InvalidIdentity)
        ));
        assert_eq!(user_version(&connection), 0);
        assert_eq!(database_snapshot(&connection), before);
    }

    #[test]
    fn negative_version_is_rejected_without_initialization() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("PRAGMA user_version=-1;").unwrap();

        assert!(matches!(
            configure(&mut connection),
            Err(SchemaError::UnsupportedVersion(-1))
        ));
        assert_eq!(user_version(&connection), -1);
        assert_eq!(database_snapshot(&connection), Vec::<String>::new());
    }

    #[test]
    fn configures_foreign_keys_on_the_handler_connection() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure(&mut connection).unwrap();

        assert_eq!(
            connection
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO runs(run_id,fingerprint) VALUES ('invalid',zeroblob(32))",
                    []
                )
                .is_err()
        );
    }

    #[test]
    fn outcome_columns_replace_parallel_evidence() {
        // Arrange
        let mut connection = Connection::open_in_memory().unwrap();
        configure(&mut connection).unwrap();
        // Act
        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(results)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        // Assert
        assert_eq!(
            columns,
            ["run_id", "mutant_id", "outcome_json", "resource_mode"]
        );
    }

    #[test]
    fn future_version_is_rejected_without_modifying_the_database() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE future_schema_marker(value TEXT);
                 INSERT INTO future_schema_marker VALUES ('preserve me');
                 PRAGMA user_version=6;",
            )
            .unwrap();

        let before = database_snapshot(&connection);

        assert!(matches!(
            configure(&mut connection),
            Err(SchemaError::UnsupportedVersion(6))
        ));
        assert_eq!(user_version(&connection), 6);
        assert_eq!(database_snapshot(&connection), before);
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master
                     WHERE type='table' AND name='future_schema_marker'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn concurrent_configure_of_fresh_database_succeeds_for_both_connections() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");

        let results = configure_concurrently(&path);

        assert!(
            results.iter().all(Result::is_ok),
            "concurrent schema configuration failed: {results:?}"
        );
        let connection = Connection::open(path).unwrap();
        assert_eq!(user_version(&connection), SCHEMA_VERSION);
    }

    fn configure_concurrently(path: &Path) -> Vec<Result<(), String>> {
        let start = Arc::new(InitializationRendezvous::default());
        let initialization = Arc::new(InitializationRendezvous::default());
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let path = path.to_owned();
                let start = Arc::clone(&start);
                let initialization = Arc::clone(&initialization);
                thread::spawn(move || {
                    let mut connection =
                        Connection::open(path).map_err(|error| error.to_string())?;
                    start.arrive_and_wait();
                    configure_observed(&mut connection, || {
                        initialization.arrive_and_wait();
                    })
                    .map_err(
                        // A failed concurrent initializer is reported by the fixture assertions below.
                        #[cfg_attr(coverage_nightly, coverage(off))]
                        |error| error.to_string(),
                    )
                })
            })
            .collect();
        start.release_when_ready();
        initialization.release_when_ready();
        let joined: Vec<_> = handles.into_iter().map(thread::JoinHandle::join).collect();
        assert_eq!(start.failure(), None, "configure start rendezvous failed");
        assert_eq!(
            initialization.failure(),
            None,
            "initialization rendezvous failed"
        );
        joined
            .into_iter()
            .map(|result| result.expect("configure thread panicked"))
            .collect()
    }

    #[derive(Default)]
    struct InitializationRendezvous {
        state: Mutex<RendezvousState>,
        changed: Condvar,
        timeout: Option<Duration>,
    }

    #[derive(Default)]
    struct RendezvousState {
        arrived: usize,
        released: bool,
        failure: Option<&'static str>,
    }

    impl InitializationRendezvous {
        fn arrive_and_wait(&self) {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.arrived += 1;
            self.changed.notify_all();
            let (mut state, timeout) = self
                .changed
                .wait_timeout_while(
                    state,
                    self.timeout.unwrap_or(Duration::from_secs(5)),
                    |state| !state.released,
                )
                .unwrap_or_else(PoisonError::into_inner);
            if timeout.timed_out() && !state.released {
                state.failure.get_or_insert("participant timed out");
            }
        }

        fn release_when_ready(&self) {
            let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let (mut state, timeout) = self
                .changed
                .wait_timeout_while(
                    state,
                    self.timeout.unwrap_or(Duration::from_secs(5)),
                    |state| state.arrived < 2,
                )
                .unwrap_or_else(PoisonError::into_inner);
            if timeout.timed_out() && state.arrived < 2 {
                state.failure.get_or_insert("coordinator timed out");
            }
            state.released = true;
            self.changed.notify_all();
        }

        fn failure(&self) -> Option<&'static str> {
            self.state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .failure
        }
    }

    #[rstest::rstest]
    #[case::participant(
        InitializationRendezvous::arrive_and_wait,
        InitializationRendezvous::release_when_ready,
        "participant timed out"
    )]
    #[case::coordinator(
        InitializationRendezvous::release_when_ready,
        InitializationRendezvous::arrive_and_wait,
        "coordinator timed out"
    )]
    fn initialization_rendezvous_reports_each_timeout_and_retains_the_first_failure(
        #[case] first: fn(&InitializationRendezvous),
        #[case] second: fn(&InitializationRendezvous),
        #[case] expected: &str,
    ) {
        let rendezvous = InitializationRendezvous {
            timeout: Some(Duration::ZERO),
            ..Default::default()
        };

        first(&rendezvous);
        let first_failure = rendezvous.failure();
        second(&rendezvous);
        let second_failure = rendezvous.failure();

        assert_eq!(first_failure, Some(expected));
        assert_eq!(second_failure, Some(expected));
    }

    fn query_plan(connection: &Connection, sql: &str) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        statement
            .query_map(["run-1", "m1"], |row| row.get(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn user_version(connection: &Connection) -> i64 {
        connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[rstest::rstest]
    #[case(
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None
        ),
        true
    )]
    #[case(
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
            None
        ),
        true
    )]
    #[case(rusqlite::Error::InvalidQuery, false)]
    #[case(
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
            None
        ),
        false
    )]
    fn lock_contention_classification_rejects_unrelated_sqlite_errors(
        #[case]
        #[by_ref]
        error: &rusqlite::Error,
        #[case] expected: bool,
    ) {
        let actual = is_lock_contention(error);

        assert_eq!(actual, expected);
    }

    #[test]
    fn wal_refusal_in_a_temporary_database_is_reported() {
        let connection = Connection::open("").unwrap();

        let actual = matches!(enable_wal(&connection, Duration::ZERO), Err(SchemaError::JournalMode(mode)) if mode == "delete");

        assert!(actual);
    }

    #[test]
    fn wal_lock_retry_expires_without_changing_the_competing_transaction() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.sqlite3");
        let owner = Connection::open(&path).unwrap();
        owner
            .execute_batch(
                "CREATE TABLE original(value); BEGIN EXCLUSIVE; INSERT INTO original VALUES(1);",
            )
            .unwrap();
        let contender = Connection::open(&path).unwrap();
        contender.busy_timeout(Duration::ZERO).unwrap();
        let error = enable_wal(&contender, Duration::from_millis(15)).unwrap_err();
        let observed_1 =
            matches!(error, SchemaError::Database(error) if is_lock_contention(&error));
        let observed_2 = owner
            .query_row("SELECT count(*) FROM original", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        owner.execute_batch("ROLLBACK").unwrap();
        enable_wal(&contender, Duration::from_millis(15)).unwrap();

        assert!(observed_1);
        assert_eq!(observed_2, 1);
    }

    #[test]
    fn readonly_wal_conversion_reports_a_database_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE original(value)")
            .unwrap();
        drop(connection);
        let readonly =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();

        let actual = matches!(
            enable_wal(&readonly, Duration::ZERO),
            Err(SchemaError::Database(_))
        );

        assert!(actual);
    }
    #[test]
    fn database_inspection_rejects_a_directory_before_reading_its_header() {
        let directory = tempfile::tempdir().unwrap();

        let actual = matches!(inspect_existing_database(directory.path()), Err(SchemaError::Inspect(error)) if error.kind() == std::io::ErrorKind::InvalidInput);

        assert!(actual);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn database_header_read_errors_preserve_the_original_io_error() {
        let actual = matches!(inspect_existing_database(Path::new("/proc/self/mem")), Err(SchemaError::Inspect(error)) if error.raw_os_error() == Some(libc::EIO));

        assert!(actual);
    }
    #[cfg(unix)]
    #[test]
    fn database_inspection_rejects_a_directory_opened_after_the_path_check() {
        let directory = tempfile::tempdir().unwrap();
        let opened = std::fs::File::open(directory.path()).unwrap();

        let actual = matches!(inspect_database_file(opened), Err(SchemaError::Inspect(error)) if error.kind() == std::io::ErrorKind::InvalidInput);

        assert!(actual);
    }
    #[test]
    fn database_inspection_preserves_a_write_only_capability_read_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.sqlite3");
        let opened = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap();
        let error = inspect_database_file(opened).unwrap_err();

        let actual = matches!(error, SchemaError::Inspect(error) if error.kind() != std::io::ErrorKind::UnexpectedEof);

        assert!(actual);
    }

    #[cfg(unix)]
    #[test]
    fn companion_metadata_permission_errors_stop_admission_before_database_creation() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid has no arguments or memory preconditions.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("inaccessible");
        std::fs::create_dir(&parent).unwrap();
        let database = parent.join("session.sqlite3");
        let permissions = std::fs::metadata(&parent).unwrap().permissions();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = inspect_existing_companions(&database);
        std::fs::set_permissions(&parent, permissions).unwrap();

        let actual = matches!(result, Err(SchemaError::Inspect(error)) if error.kind() == std::io::ErrorKind::PermissionDenied);
        let actual_2 = !database.exists();

        assert!(actual);
        assert!(actual_2);
    }
}
