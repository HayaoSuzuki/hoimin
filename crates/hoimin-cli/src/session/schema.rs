use std::time::Duration;

use rusqlite::Connection;
use thiserror::Error;

pub(crate) const SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("SQLite schema version {0} is newer than this hoimin build")]
    FutureVersion(i64),
    #[error("failed to configure or migrate SQLite session: {0}")]
    Database(#[from] rusqlite::Error),
}

pub(crate) fn configure(connection: &Connection) -> Result<(), SchemaError> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    let _: String = connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(SchemaError::FutureVersion(version));
    }
    if version == 0 {
        migrate_v1(connection)?;
    }
    if version <= 1 {
        migrate_v2(connection)?;
    }
    Ok(())
}

fn migrate_v1(connection: &Connection) -> Result<(), rusqlite::Error> {
    let transaction = connection.unchecked_transaction()?;
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
    transaction.commit()
}

fn migrate_v2(connection: &Connection) -> Result<(), rusqlite::Error> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "CREATE INDEX diagnostics_result ON diagnostics(run_id, mutant_id, id);
         PRAGMA user_version=2;",
    )?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configures_busy_timeout_on_the_handler_connection() {
        let connection = Connection::open_in_memory().unwrap();
        configure(&connection).unwrap();

        let timeout: i64 = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 5_000);
    }

    #[test]
    fn diagnostics_queries_use_the_result_index() {
        let connection = Connection::open_in_memory().unwrap();
        configure(&connection).unwrap();

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
        let connection = Connection::open(&path).unwrap();
        create_v1_fixture(&connection, true);
        drop(connection);

        for _ in 0..2 {
            let connection = Connection::open(&path).unwrap();
            configure(&connection).unwrap();
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
        let connection = Connection::open_in_memory().unwrap();
        create_v1_fixture(&connection, false);

        assert!(configure(&connection).is_err());
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

    fn create_v1_fixture(connection: &Connection, valid: bool) {
        if valid {
            migrate_v1(connection).unwrap();
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
