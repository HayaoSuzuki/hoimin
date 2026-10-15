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
    #[error("SQLite refused WAL journal mode and remained in {0} mode")]
    JournalMode(String),
    #[error("failed to configure or initialize SQLite session: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Configures session access and initializes fresh databases at the current schema.
/// Rejects unsupported schemas without changing their tables or data.
pub(crate) fn configure(connection: &mut Connection) -> Result<(), SchemaError> {
    configure_observed(connection, || {})
}

fn configure_observed(
    connection: &mut Connection,
    after_initial_version_read: impl FnOnce(),
) -> Result<(), SchemaError> {
    const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    enable_wal(connection, BUSY_TIMEOUT)?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    after_initial_version_read();
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        0 => initialize(&transaction)?,
        SCHEMA_VERSION => {}
        _ => return Err(SchemaError::UnsupportedVersion(version)),
    }
    transaction.commit()?;
    Ok(())
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
                            .map(|i| row.get::<_, rusqlite::types::Value>(i))
                            .collect::<Result<Vec<_>, _>>()?;
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

    #[test]
    fn diagnostics_queries_use_the_result_index() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure(&mut connection).unwrap();

        for sql in [
            "SELECT mutant_id,level,code,message FROM diagnostics
             WHERE run_id=?1 AND mutant_id=?2 ORDER BY id",
            "DELETE FROM diagnostics WHERE run_id=?1 AND mutant_id=?2",
        ] {
            let details = query_plan(&connection, sql);
            assert!(
                details
                    .iter()
                    .any(|detail| detail.contains("INDEX diagnostics_result")),
                "query did not use diagnostics_result: {details:?}"
            );
        }
    }

    #[test]
    fn current_golden_reopens_without_changing_schema_or_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/sessions/schema-v5.sqlite3"),
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
    fn failed_fresh_initialization_rolls_back_tables_indexes_and_version() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE diagnostics(marker TEXT); INSERT INTO diagnostics VALUES ('preserve me');").unwrap();
        let before = database_snapshot(&connection);
        assert!(matches!(
            configure(&mut connection),
            Err(SchemaError::Database(_))
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
                    .map_err(|error| error.to_string())
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
                .wait_timeout_while(state, Duration::from_secs(5), |state| !state.released)
                .unwrap_or_else(PoisonError::into_inner);
            if timeout.timed_out() && !state.released {
                state.failure.get_or_insert("participant timed out");
            }
        }

        fn release_when_ready(&self) {
            let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            let (mut state, timeout) = self
                .changed
                .wait_timeout_while(state, Duration::from_secs(5), |state| state.arrived < 2)
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
