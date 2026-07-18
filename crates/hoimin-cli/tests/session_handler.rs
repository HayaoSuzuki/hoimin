use std::time::Duration;

use hoimin_cli::session::SessionHandler;
use hoimin_core::{
    BeginSession, ByteSpan, EffectFailure, EffectId, FinishSession, LoadSession, MutantResult,
    MutationCandidate, MutationStatus, OutputSpoolRef, PersistResult, ResourceMode, RunFingerprint,
};
use rusqlite::Connection;

#[test]
fn migrates_schema_enables_wal_and_echoes_typed_completion_events() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    let fingerprint = RunFingerprint::from_bytes([3; 32]);

    let started = handler
        .begin(BeginSession {
            id: EffectId(1),
            run_id: "run-1".to_owned(),
            fingerprint,
        })
        .unwrap();
    assert_eq!(started.id, EffectId(1));
    assert_eq!(started.run_id, "run-1");

    let persisted = handler.persist(persist_request(2, "run-1", "m1")).unwrap();
    assert_eq!(persisted.id, EffectId(2));
    assert_eq!(persisted.mutant_id, "m1");

    let loaded = handler.load(LoadSession { id: EffectId(3) }).unwrap();
    assert_eq!(loaded.id, EffectId(3));
    assert_eq!(loaded.runs.len(), 1);
    assert_eq!(loaded.runs[0].results[0].status, MutationStatus::Killed);
    assert_eq!(loaded.runs[0].diagnostics[0].code, "fixture.warning");

    let finished = handler
        .finish(FinishSession {
            id: EffectId(4),
            run_id: "run-1".to_owned(),
            complete: true,
        })
        .unwrap();
    assert_eq!(finished.id, EffectId(4));
    assert!(finished.complete);

    let observer = Connection::open(&path).unwrap();
    assert_eq!(
        observer
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        observer
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert!(
        observer
            .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))
            .unwrap()
            >= 5_000
    );
    for table in [
        "fingerprints",
        "runs",
        "candidates",
        "results",
        "diagnostics",
    ] {
        let exists: i64 = observer
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "missing table {table}");
    }
    assert_eq!(
        observer
            .query_row("SELECT count(*) FROM diagnostics", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn each_mutant_transaction_rolls_back_when_commit_fails() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();

    let failed = handler
        .persist(persist_request(9, "missing-run", "m1"))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(9));
    assert_eq!(failed.failure.code(), "session.commit");
    assert!(matches!(
        failed.failure,
        EffectFailure::SessionDatabase { .. }
    ));

    let observer = Connection::open(&path).unwrap();
    let candidates: i64 = observer
        .query_row("SELECT count(*) FROM candidates", [], |row| row.get(0))
        .unwrap();
    let results: i64 = observer
        .query_row("SELECT count(*) FROM results", [], |row| row.get(0))
        .unwrap();
    assert_eq!((candidates, results), (0, 0));
}

#[test]
fn duplicate_results_and_duplicate_finish_are_typed_failures() {
    let temp = tempfile::tempdir().unwrap();
    let mut handler = SessionHandler::open(temp.path().join("sessions.sqlite3")).unwrap();
    handler.begin(begin_request(1, "run-1")).unwrap();
    handler.persist(persist_request(2, "run-1", "m1")).unwrap();

    let duplicate = handler
        .persist(persist_request(3, "run-1", "m1"))
        .unwrap_err();
    assert_eq!(duplicate.id, EffectId(3));
    assert_eq!(duplicate.failure.code(), "session.duplicate_result");

    handler
        .finish(FinishSession {
            id: EffectId(4),
            run_id: "run-1".to_owned(),
            complete: false,
        })
        .unwrap();
    let duplicate = handler.finish(finish_request(5, "run-1")).unwrap_err();
    assert_eq!(duplicate.id, EffectId(5));
    assert_eq!(duplicate.failure.code(), "session.finish.state");
}

#[test]
fn corrupt_status_and_null_rows_are_typed_instead_of_panicking() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    handler.begin(begin_request(1, "run-1")).unwrap();
    handler.persist(persist_request(2, "run-1", "m1")).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute("UPDATE results SET status='bogus'", [])
        .unwrap();
    let failed = handler.load(LoadSession { id: EffectId(7) }).unwrap_err();
    assert_eq!(failed.id, EffectId(7));
    assert_eq!(failed.failure.code(), "session.corrupt");

    drop(handler);
    let null_path = temp.path().join("null.sqlite3");
    create_nullable_corrupt_database(&null_path);
    let mut handler = SessionHandler::open(&null_path).unwrap();
    let failed = handler.load(LoadSession { id: EffectId(8) }).unwrap_err();
    assert_eq!(failed.id, EffectId(8));
    assert_eq!(failed.failure.code(), "session.corrupt");
}

#[test]
fn rejects_unknown_future_schema_and_releases_temporary_database_on_drop() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("future.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    drop(connection);
    assert!(SessionHandler::open(&path).is_err());

    let cleanup = tempfile::tempdir().unwrap();
    let cleanup_path = cleanup.path().join("session.sqlite3");
    let handler = SessionHandler::open(&cleanup_path).unwrap();
    drop(handler);
    cleanup.close().unwrap();
}

fn begin_request(id: u64, run_id: &str) -> BeginSession {
    BeginSession {
        id: EffectId(id),
        run_id: run_id.to_owned(),
        fingerprint: RunFingerprint::from_bytes([1; 32]),
    }
}

fn finish_request(id: u64, run_id: &str) -> FinishSession {
    FinishSession {
        id: EffectId(id),
        run_id: run_id.to_owned(),
        complete: true,
    }
}

fn persist_request(id: u64, run_id: &str, mutant_id: &str) -> PersistResult {
    PersistResult {
        id: EffectId(id),
        result: MutantResult {
            run_id: run_id.to_owned(),
            candidate: MutationCandidate {
                id: mutant_id.to_owned(),
                sequence: 0,
                path: "src/example.py".into(),
                span: ByteSpan {
                    start: 0,
                    length: 1,
                },
                original: "+".to_owned(),
                replacement: "-".to_owned(),
                operator: "binary".to_owned(),
                line: 1,
                column: 0,
                symbol: None,
                file_hash: "hash".to_owned(),
            },
            status: MutationStatus::Killed,
            elapsed: Duration::from_millis(5),
            resource_mode: ResourceMode::Hard,
            output: Some(OutputSpoolRef {
                token: "spool".to_owned(),
                retained: 3,
                observed: 8,
            }),
            diagnostics: vec![hoimin_core::SessionDiagnostic {
                mutant_id: mutant_id.to_owned(),
                level: "warning".to_owned(),
                code: "fixture.warning".to_owned(),
                message: "fixture diagnostic".to_owned(),
            }],
        },
    }
}

fn create_nullable_corrupt_database(path: &std::path::Path) {
    let db = Connection::open(path).unwrap();
    db.execute_batch(
        "PRAGMA user_version=1;
         CREATE TABLE runs (id INTEGER PRIMARY KEY, run_id TEXT, fingerprint BLOB, complete INTEGER);
         CREATE TABLE results (run_id TEXT, mutant_id TEXT, status TEXT);
         CREATE TABLE fingerprints (digest BLOB PRIMARY KEY, schema_version INTEGER);
         CREATE TABLE candidates (run_id TEXT, mutant_id TEXT);
         CREATE TABLE diagnostics (id INTEGER PRIMARY KEY, run_id TEXT, level TEXT, code TEXT, message TEXT);
         INSERT INTO runs(id, run_id, fingerprint, complete) VALUES (1, 'run-null', zeroblob(32), 0);
         INSERT INTO results(run_id, mutant_id, status) VALUES ('run-null', 'm-null', NULL);",
    )
    .unwrap();
}
