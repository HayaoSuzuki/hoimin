use std::time::Duration;

use hoimin_cli::session::SessionHandler;
use hoimin_core::{
    BeginSession, ByteSpan, EffectFailure, EffectId, FinishSession, LoadSession,
    LookupStoredResult, MutantResult, MutationCandidate, MutationStatus, OutputSpoolRef,
    PersistResult, ResourceMode, ResumeDecision, RunFingerprint, resume_policy,
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

    let persisted = handler.persist(&persist_request(2, "run-1", "m1")).unwrap();
    assert_eq!(persisted.id, EffectId(2));
    assert_eq!(persisted.mutant_id, "m1");

    let loaded = handler
        .load(&LoadSession {
            id: EffectId(3),
            fingerprint,
        })
        .unwrap();
    assert_eq!(loaded.id, EffectId(3));
    assert_eq!(loaded.resume.unwrap().run_id, "run-1");
    let stored = handler.lookup(&lookup_request(30, "run-1", "m1")).unwrap();
    assert_eq!(stored.id, EffectId(30));
    assert_eq!(stored.result.unwrap().status, MutationStatus::Killed);

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
        2
    );
    assert_eq!(
        observer
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "wal"
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
fn timeout_can_be_replaced_then_resumed_and_completed_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let mut handler = SessionHandler::open(temp.path().join("sessions.sqlite3")).unwrap();
    let fingerprint = RunFingerprint::from_bytes([1; 32]);
    handler.begin(begin_request(1, "run-1")).unwrap();
    handler
        .persist(&persist_with_status(
            2,
            "run-1",
            "m1",
            MutationStatus::Timeout,
        ))
        .unwrap();
    handler
        .finish(FinishSession {
            id: EffectId(3),
            run_id: "run-1".to_owned(),
            complete: false,
        })
        .unwrap();

    let loaded = handler
        .load(&LoadSession {
            id: EffectId(4),
            fingerprint,
        })
        .unwrap();
    assert_eq!(loaded.resume.unwrap().run_id, "run-1");
    let stored = handler.lookup(&lookup_request(5, "run-1", "m1")).unwrap();
    assert_eq!(resume_policy(stored.result.as_ref()), ResumeDecision::Rerun);

    handler
        .persist(&persist_with_status(
            6,
            "run-1",
            "m1",
            MutationStatus::Killed,
        ))
        .unwrap();
    let stored = handler.lookup(&lookup_request(7, "run-1", "m1")).unwrap();
    assert_eq!(resume_policy(stored.result.as_ref()), ResumeDecision::Reuse);
    handler.finish(finish_request(8, "run-1")).unwrap();
    assert!(
        handler
            .load(&LoadSession {
                id: EffectId(9),
                fingerprint,
            })
            .unwrap()
            .resume
            .is_none()
    );

    let failed = handler
        .persist(&persist_with_status(
            10,
            "run-1",
            "m2",
            MutationStatus::Killed,
        ))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(10));
    assert_eq!(failed.failure.code(), "session.persist.complete");
}

#[test]
fn load_selects_only_the_newest_compatible_incomplete_run() {
    let temp = tempfile::tempdir().unwrap();
    let mut handler = SessionHandler::open(temp.path().join("sessions.sqlite3")).unwrap();
    let wanted = RunFingerprint::from_bytes([1; 32]);
    handler.begin(begin_request(1, "old")).unwrap();
    handler.begin(begin_request(2, "new")).unwrap();
    handler
        .begin(BeginSession {
            id: EffectId(3),
            run_id: "other".to_owned(),
            fingerprint: RunFingerprint::from_bytes([2; 32]),
        })
        .unwrap();
    handler.begin(begin_request(4, "complete")).unwrap();
    handler.finish(finish_request(5, "complete")).unwrap();

    let loaded = handler
        .load(&LoadSession {
            id: EffectId(6),
            fingerprint: wanted,
        })
        .unwrap();
    assert_eq!(loaded.id, EffectId(6));
    assert_eq!(loaded.resume.unwrap().run_id, "new");
    assert!(
        handler
            .load(&LoadSession {
                id: EffectId(7),
                fingerprint: RunFingerprint::from_bytes([9; 32]),
            })
            .unwrap()
            .resume
            .is_none()
    );
}

#[test]
fn incomplete_finish_is_idempotent_but_completion_is_final() {
    let temp = tempfile::tempdir().unwrap();
    let mut handler = SessionHandler::open(temp.path().join("sessions.sqlite3")).unwrap();
    handler.begin(begin_request(1, "partial")).unwrap();
    handler
        .finish(FinishSession {
            id: EffectId(2),
            run_id: "partial".to_owned(),
            complete: false,
        })
        .unwrap();
    handler
        .finish(FinishSession {
            id: EffectId(3),
            run_id: "partial".to_owned(),
            complete: false,
        })
        .unwrap();
    handler.finish(finish_request(4, "partial")).unwrap();
    assert_eq!(
        handler
            .finish(finish_request(5, "partial"))
            .unwrap_err()
            .failure
            .code(),
        "session.finish.state"
    );

    handler.begin(begin_request(6, "direct")).unwrap();
    handler.finish(finish_request(7, "direct")).unwrap();
}

#[test]
fn each_mutant_transaction_rolls_back_when_commit_fails() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();

    let failed = handler
        .persist(&persist_request(9, "missing-run", "m1"))
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
fn failed_inconclusive_replacement_restores_the_previous_result() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    handler.begin(begin_request(1, "run-1")).unwrap();
    handler
        .persist(&persist_with_status(
            2,
            "run-1",
            "m1",
            MutationStatus::Timeout,
        ))
        .unwrap();

    let mut replacement = persist_with_status(3, "run-1", "m1", MutationStatus::Killed);
    replacement.result.diagnostics[0].mutant_id = "wrong-id".to_owned();
    let failed = handler.persist(&replacement).unwrap_err();
    assert_eq!(failed.id, EffectId(3));
    assert_eq!(failed.failure.code(), "session.persist.diagnostic");

    let stored = handler.lookup(&lookup_request(4, "run-1", "m1")).unwrap();
    assert_eq!(stored.result.unwrap().status, MutationStatus::Timeout);
    let observer = Connection::open(path).unwrap();
    assert_eq!(
        observer
            .query_row("SELECT count(*) FROM diagnostics", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn determinate_results_cannot_be_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let mut handler = SessionHandler::open(temp.path().join("sessions.sqlite3")).unwrap();
    for (index, status) in [MutationStatus::Killed, MutationStatus::Survived]
        .into_iter()
        .enumerate()
    {
        let run_id = format!("run-{index}");
        handler
            .begin(begin_request(10 * index as u64 + 1, &run_id))
            .unwrap();
        handler
            .persist(&persist_with_status(
                10 * index as u64 + 2,
                &run_id,
                "m1",
                status,
            ))
            .unwrap();

        let duplicate = handler
            .persist(&persist_request(10 * index as u64 + 3, &run_id, "m1"))
            .unwrap_err();
        assert_eq!(duplicate.id, EffectId(10 * index as u64 + 3));
        assert_eq!(duplicate.failure.code(), "session.duplicate_result");
    }
}

#[test]
fn every_inconclusive_result_can_be_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let mut handler = SessionHandler::open(temp.path().join("sessions.sqlite3")).unwrap();
    for (index, status) in [
        MutationStatus::Timeout,
        MutationStatus::OutOfMemory,
        MutationStatus::ProcessLimit,
        MutationStatus::Error,
        MutationStatus::NotRun,
    ]
    .into_iter()
    .enumerate()
    {
        let run_id = format!("run-{index}");
        handler
            .begin(begin_request(10 * index as u64 + 1, &run_id))
            .unwrap();
        handler
            .persist(&persist_with_status(
                10 * index as u64 + 2,
                &run_id,
                "m1",
                status,
            ))
            .unwrap();
        handler
            .persist(&persist_request(10 * index as u64 + 3, &run_id, "m1"))
            .unwrap();
        assert_eq!(
            handler
                .lookup(&lookup_request(10 * index as u64 + 4, &run_id, "m1"))
                .unwrap()
                .result
                .unwrap()
                .status,
            MutationStatus::Killed
        );
    }
}

#[test]
fn lookup_rejects_corrupt_status_candidate_mismatch_and_completed_runs() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    handler.begin(begin_request(1, "run-1")).unwrap();
    handler.persist(&persist_request(2, "run-1", "m1")).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute("UPDATE results SET status='bogus'", [])
        .unwrap();
    let failed = handler
        .lookup(&lookup_request(7, "run-1", "m1"))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(7));
    assert_eq!(failed.failure.code(), "session.corrupt");

    drop(handler);
    let mismatch_path = temp.path().join("mismatch.sqlite3");
    let mut handler = SessionHandler::open(&mismatch_path).unwrap();
    handler.begin(begin_request(8, "run-mismatch")).unwrap();
    handler
        .persist(&persist_request(9, "run-mismatch", "m1"))
        .unwrap();
    let corrupter = Connection::open(&mismatch_path).unwrap();
    corrupter
        .pragma_update(None, "foreign_keys", "OFF")
        .unwrap();
    corrupter
        .execute("DELETE FROM candidates WHERE mutant_id='m1'", [])
        .unwrap();
    drop(corrupter);
    let failed = handler
        .lookup(&lookup_request(10, "run-mismatch", "m1"))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(10));
    assert_eq!(failed.failure.code(), "session.corrupt");

    drop(handler);
    let complete_path = temp.path().join("complete.sqlite3");
    let mut handler = SessionHandler::open(&complete_path).unwrap();
    handler.begin(begin_request(11, "run-complete")).unwrap();
    handler.finish(finish_request(12, "run-complete")).unwrap();
    let failed = handler
        .lookup(&lookup_request(13, "run-complete", "m1"))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(13));
    assert_eq!(failed.failure.code(), "session.lookup.complete");

    let null_path = temp.path().join("null.sqlite3");
    create_nullable_corrupt_database(&null_path);
    let mut handler = SessionHandler::open(&null_path).unwrap();
    let failed = handler
        .lookup(&lookup_request(14, "run-null", "m-null"))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(14));
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
    persist_with_status(id, run_id, mutant_id, MutationStatus::Killed)
}

fn persist_with_status(
    id: u64,
    run_id: &str,
    mutant_id: &str,
    status: MutationStatus,
) -> PersistResult {
    PersistResult {
        id: EffectId(id),
        worker: 0,
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
            status,
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

fn lookup_request(id: u64, run_id: &str, mutant_id: &str) -> LookupStoredResult {
    LookupStoredResult {
        id: EffectId(id),
        worker: 0,
        run_id: run_id.to_owned(),
        mutant_id: mutant_id.to_owned(),
    }
}

fn create_nullable_corrupt_database(path: &std::path::Path) {
    let db = Connection::open(path).unwrap();
    db.execute_batch(
        "PRAGMA user_version=2;
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
