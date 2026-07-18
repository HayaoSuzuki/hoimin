use std::time::Duration;

use rusqlite::Connection;
use thiserror::Error;

pub(crate) const SCHEMA_VERSION: i64 = 1;

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
    match version {
        0 => migrate_v1(connection)?,
        SCHEMA_VERSION => {}
        newer => return Err(SchemaError::FutureVersion(newer)),
    }
    Ok(())
}

fn migrate_v1(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE fingerprints (
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
         PRAGMA user_version=1;
         COMMIT;",
    )
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
}
