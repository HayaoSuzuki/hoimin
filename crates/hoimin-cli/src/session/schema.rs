use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, Transaction, TransactionBehavior};
use thiserror::Error;

pub(crate) const SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("SQLite schema version {0} is newer than this hoimin build")]
    FutureVersion(i64),
    #[error("SQLite refused WAL journal mode and remained in {0} mode")]
    JournalMode(String),
    #[error("failed to configure or migrate SQLite session: {0}")]
    Database(#[from] rusqlite::Error),
}

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
    if version > SCHEMA_VERSION {
        return Err(SchemaError::FutureVersion(version));
    }
    if version == 0 {
        migrate_v1(&transaction)?;
    }
    if version <= 1 {
        migrate_v2(&transaction)?;
    }
    transaction.commit()?;
    Ok(())
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

fn migrate_v1(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
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
             complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0, 1))
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
             status TEXT NOT NULL,
             elapsed_secs INTEGER NOT NULL,
             elapsed_nanos INTEGER NOT NULL,
             resource_mode TEXT NOT NULL,
             output_token TEXT,
             output_retained INTEGER,
             output_observed INTEGER,
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
         PRAGMA user_version=1;",
    )?;
    Ok(())
}

fn migrate_v2(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute_batch(
        "CREATE INDEX diagnostics_result ON diagnostics(run_id, mutant_id, id);
         PRAGMA user_version=2;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{Arc, Condvar, Mutex, PoisonError};
    use std::thread;
    use std::time::Duration;

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
    fn upgrades_v1_preserves_data_and_reopens_idempotently() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        create_v1_fixture(&mut connection, true);
        drop(connection);

        for _ in 0..2 {
            let mut connection = Connection::open(&path).unwrap();
            configure(&mut connection).unwrap();
            assert_eq!(user_version(&connection), 2);
            assert_eq!(
                connection
                    .query_row("SELECT message FROM diagnostics WHERE id=7", [], |row| {
                        row.get::<_, String>(0)
                    })
                    .unwrap(),
                "preserve me"
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT count(*) FROM sqlite_master
                         WHERE type='index' AND name='diagnostics_result'",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                1
            );
        }
    }

    #[test]
    fn failed_v1_upgrade_rolls_back_version_and_index() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v1_fixture(&mut connection, false);

        assert!(configure(&mut connection).is_err());
        assert_eq!(user_version(&connection), 1);
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master
                     WHERE type='index' AND name='diagnostics_result'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn future_version_is_rejected_without_modifying_the_database() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE future_schema_marker(value TEXT);
                 PRAGMA user_version=3;",
            )
            .unwrap();

        assert!(matches!(
            configure(&mut connection),
            Err(SchemaError::FutureVersion(3))
        ));
        assert_eq!(user_version(&connection), 3);
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

    #[test]
    fn concurrent_v1_upgrade_succeeds_and_preserves_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        create_v1_fixture(&mut connection, true);
        connection
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))
            .unwrap();
        drop(connection);

        let results = configure_concurrently(&path);

        assert!(
            results.iter().all(Result::is_ok),
            "concurrent schema upgrade failed: {results:?}"
        );
        let connection = Connection::open(path).unwrap();
        assert_eq!(user_version(&connection), SCHEMA_VERSION);
        assert_eq!(
            connection
                .query_row("SELECT message FROM diagnostics WHERE id=7", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "preserve me"
        );
    }

    fn configure_concurrently(path: &Path) -> Vec<Result<(), String>> {
        let start = Arc::new(MigrationRendezvous::default());
        let migration = Arc::new(MigrationRendezvous::default());
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let path = path.to_owned();
                let start = Arc::clone(&start);
                let migration = Arc::clone(&migration);
                thread::spawn(move || {
                    let mut connection =
                        Connection::open(path).map_err(|error| error.to_string())?;
                    start.arrive_and_wait();
                    configure_observed(&mut connection, || {
                        migration.arrive_and_wait();
                    })
                    .map_err(|error| error.to_string())
                })
            })
            .collect();
        start.release_when_ready();
        migration.release_when_ready();
        let joined: Vec<_> = handles.into_iter().map(thread::JoinHandle::join).collect();
        assert_eq!(start.failure(), None, "configure start rendezvous failed");
        assert_eq!(migration.failure(), None, "migration rendezvous failed");
        joined
            .into_iter()
            .map(|result| result.expect("configure thread panicked"))
            .collect()
    }

    #[derive(Default)]
    struct MigrationRendezvous {
        state: Mutex<RendezvousState>,
        changed: Condvar,
    }

    #[derive(Default)]
    struct RendezvousState {
        arrived: usize,
        released: bool,
        failure: Option<&'static str>,
    }

    impl MigrationRendezvous {
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

    fn create_v1_fixture(connection: &mut Connection, valid: bool) {
        if valid {
            let transaction = connection.transaction().unwrap();
            migrate_v1(&transaction).unwrap();
            transaction.commit().unwrap();
            connection
                .execute_batch(
                    "INSERT INTO fingerprints(digest,schema_version) VALUES (zeroblob(32),1);
                     INSERT INTO runs(run_id,fingerprint) VALUES ('run-1',zeroblob(32));
                     INSERT INTO candidates(
                         run_id,mutant_id,sequence,path,span_start,span_length,original,
                         replacement,operator,line,column_number,symbol,file_hash
                     ) VALUES (
                         'run-1','m1',0,'src/a.py',0,1,'+','-','binary',1,0,NULL,'hash'
                     );",
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO diagnostics(id,run_id,mutant_id,level,code,message)
                     VALUES (7,'run-1','m1','warning','fixture','preserve me')",
                    [],
                )
                .unwrap();
        } else {
            connection
                .execute_batch(
                    "PRAGMA user_version=1;
                     CREATE TABLE diagnostics (
                         id INTEGER PRIMARY KEY,
                         run_id TEXT NOT NULL,
                         level TEXT NOT NULL,
                         code TEXT NOT NULL,
                         message TEXT NOT NULL
                     );",
                )
                .unwrap();
        }
    }
}
