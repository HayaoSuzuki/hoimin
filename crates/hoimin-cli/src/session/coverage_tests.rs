use super::*;
use hoimin_core::{ByteSpan, MutationCandidate, RunFingerprint};
use std::num::NonZeroUsize;

fn begin_request() -> BeginSession {
    BeginSession {
        id: EffectId(1),
        run_id: "coverage-run".into(),
        fingerprint: RunFingerprint::from_bytes([7; 32]),
        max_mutants: NonZeroUsize::new(10).unwrap(),
    }
}

fn load_request() -> LoadSession {
    LoadSession {
        id: EffectId(2),
        fingerprint: RunFingerprint::from_bytes([7; 32]),
        max_mutants: NonZeroUsize::new(10).unwrap(),
    }
}

fn persist_request() -> PersistResult {
    PersistResult {
        id: EffectId(3),
        worker: 0,
        result: MutantResult {
            run_id: "coverage-run".into(),
            candidate: MutationCandidate {
                id: "mutant".into(),
                sequence: 0,
                path: "sample.py".into(),
                span: ByteSpan {
                    start: 0,
                    length: 1,
                },
                original: "+".into(),
                replacement: "-".into(),
                operator: "binary".into(),
                line: 1,
                column: 0,
                symbol: None,
                file_hash: "hash".into(),
            },
            outcome: MutationOutcome::NotRun,
            resource_mode: ResourceMode::Hard,
            diagnostics: vec![SessionDiagnostic {
                mutant_id: "mutant".into(),
                level: "warning".into(),
                code: "test".into(),
                message: "diagnostic".into(),
            }],
        },
    }
}

fn fixture() -> (tempfile::TempDir, SessionHandler) {
    let directory = tempfile::tempdir().unwrap();
    let handler = SessionHandler::open(directory.path().join("session.sqlite3")).unwrap();
    (directory, handler)
}

fn assert_database_error(error: EffectFailed, expected_operation: &str) {
    assert!(
        matches!(error.failure, EffectFailure::SessionDatabase { operation, .. } if operation == expected_operation)
    );
}

fn reject_write(handler: &SessionHandler, event: &str, table: &str) {
    handler.connection.execute_batch(&format!(
        "CREATE TRIGGER reject_write BEFORE {event} ON {table} BEGIN SELECT RAISE(FAIL,'injected write failure'); END;"
    )).unwrap();
}

fn reject_commit(handler: &SessionHandler, event: &str, table: &str) {
    handler.connection.execute_batch(&format!(
        "CREATE TABLE fault_parent(id INTEGER PRIMARY KEY);
         CREATE TABLE deferred_fault(parent INTEGER REFERENCES fault_parent(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER reject_commit AFTER {event} ON {table} BEGIN INSERT INTO deferred_fault VALUES(42); END;"
    )).unwrap();
}

#[test]
fn transaction_begin_failures_are_reported_at_each_lifecycle_boundary() {
    for operation in ["begin", "persist", "finish"] {
        let (_directory, mut handler) = fixture();
        handler.begin(begin_request()).unwrap();
        handler
            .connection
            .execute_batch("PRAGMA query_only=ON")
            .unwrap();
        let error = match operation {
            "begin" => handler.begin(begin_request()).unwrap_err(),
            "persist" => handler.persist(&persist_request()).unwrap_err(),
            _ => handler
                .finish(FinishSession {
                    id: EffectId(4),
                    run_id: "coverage-run".into(),
                    complete: true,
                })
                .unwrap_err(),
        };
        assert_database_error(
            error,
            match operation {
                "begin" => "begin run transaction",
                "persist" => "begin result transaction",
                _ => "begin finish transaction",
            },
        );
    }
}

#[test]
fn run_insert_and_finish_update_errors_preserve_the_operation() {
    let (_directory, mut handler) = fixture();
    reject_write(&handler, "INSERT", "runs");
    assert_database_error(handler.begin(begin_request()).unwrap_err(), "insert run");
    handler
        .connection
        .execute_batch("DROP TRIGGER reject_write")
        .unwrap();
    handler.begin(begin_request()).unwrap();
    reject_write(&handler, "UPDATE", "runs");
    assert_database_error(
        handler
            .finish(FinishSession {
                id: EffectId(4),
                run_id: "coverage-run".into(),
                complete: true,
            })
            .unwrap_err(),
        "mark run finished",
    );
}

#[test]
fn deferred_constraint_errors_are_reported_as_commit_failures() {
    for operation in ["begin", "persist", "finish"] {
        let (_directory, mut handler) = fixture();
        if operation != "begin" {
            handler.begin(begin_request()).unwrap();
        }
        let (event, table) = match operation {
            "begin" => ("INSERT", "runs"),
            "persist" => ("INSERT", "results"),
            _ => ("UPDATE", "runs"),
        };
        reject_commit(&handler, event, table);
        let error = match operation {
            "begin" => handler.begin(begin_request()).unwrap_err(),
            "persist" => handler.persist(&persist_request()).unwrap_err(),
            _ => handler
                .finish(FinishSession {
                    id: EffectId(4),
                    run_id: "coverage-run".into(),
                    complete: true,
                })
                .unwrap_err(),
        };
        assert_database_error(
            error,
            match operation {
                "begin" => "commit run",
                "persist" => "commit mutant result",
                _ => "commit run finish",
            },
        );
        assert!(handler.connection.is_autocommit());
        assert_eq!(
            handler
                .connection
                .query_row("SELECT count(*) FROM deferred_fault", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn loading_reports_query_and_resume_recheck_failures() {
    let (_directory, mut handler) = fixture();
    handler
        .connection
        .execute_batch("DROP TABLE fingerprints")
        .unwrap();
    assert_database_error(
        handler.load(&load_request()).unwrap_err(),
        "inspect resume schema",
    );
    handler.connection.execute_batch("DROP TABLE runs").unwrap();
    assert_database_error(
        handler.load(&load_request()).unwrap_err(),
        "load resume run",
    );
    assert_database_error(
        handler.fresh_resume_reason(&load_request()).unwrap_err(),
        "inspect resume history",
    );

    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    reject_write(&handler, "UPDATE", "runs");
    assert_database_error(
        handler.load(&load_request()).unwrap_err(),
        "re-read resume run",
    );
}

#[test]
fn ignored_resume_update_reports_candidate_changed() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler
        .connection
        .execute_batch(
            "CREATE TRIGGER skip_resume BEFORE UPDATE ON runs BEGIN SELECT RAISE(IGNORE); END;",
        )
        .unwrap();
    assert_eq!(
        handler.load(&load_request()).unwrap().outcome,
        SessionLoadOutcome::Fresh(hoimin_core::ResumeFreshReason::CandidateChanged)
    );
}

#[test]
fn persistence_distinguishes_candidate_result_diagnostic_and_replacement_errors() {
    for (event, table, expected) in [
        ("INSERT", "candidates", "insert candidate"),
        ("INSERT", "results", "insert result"),
        ("INSERT", "diagnostics", "insert diagnostic"),
        ("DELETE", "diagnostics", "replace inconclusive result"),
        ("DELETE", "results", "replace inconclusive result"),
        ("DELETE", "candidates", "replace inconclusive result"),
    ] {
        let (_directory, mut handler) = fixture();
        handler.begin(begin_request()).unwrap();
        if event == "DELETE" {
            handler.persist(&persist_request()).unwrap();
        }
        reject_write(&handler, event, table);
        assert_database_error(handler.persist(&persist_request()).unwrap_err(), expected);
        for relation in ["candidates", "results", "diagnostics"] {
            assert_eq!(
                handler
                    .connection
                    .query_row(&format!("SELECT count(*) FROM {relation}"), [], |row| row
                        .get::<_, i64>(
                        0
                    ))
                    .unwrap(),
                i64::from(event == "DELETE")
            );
        }
    }
}

#[test]
fn candidate_overflow_is_a_database_conversion_error() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    let mut request = persist_request();
    request.result.candidate.sequence = u64::MAX;
    assert_database_error(handler.persist(&request).unwrap_err(), "insert candidate");
    assert!(to_i64(u64::MAX).is_err());
    assert!(!is_constraint(&rusqlite::Error::InvalidQuery));
}

#[test]
fn ownership_io_errors_do_not_claim_a_run() {
    let (directory, mut handler) = fixture();
    handler.lock_directory = directory.path().join("not-a-directory");
    std::fs::write(&handler.lock_directory, "file").unwrap();
    let error = handler.begin(begin_request()).unwrap_err();
    assert!(
        matches!(error.failure, EffectFailure::SessionDatabase { code, .. } if code == "session.ownership")
    );
    assert!(handler.ownerships.is_empty());
}

#[tokio::test]
async fn dispatch_reports_poison_and_panicking_operations() {
    let (_directory, handler) = fixture();
    let dispatcher = SessionDispatcher {
        handler: Arc::new(Mutex::new(handler)),
    };
    let operation = |_: &mut SessionHandler| Ok(());
    dispatcher.call(EffectId(19), operation).await.unwrap();
    let failed = dispatcher
        .call::<(), _>(EffectId(20), |_| panic!("operation panicked"))
        .await
        .unwrap_err();
    assert!(
        matches!(failed.failure, EffectFailure::Other { code, .. } if code == "session.dispatch")
    );
    let failed = dispatcher.call(EffectId(21), operation).await.unwrap_err();
    assert!(
        matches!(failed.failure, EffectFailure::Other { message, .. } if message == "session lock poisoned")
    );
}

#[cfg(feature = "contracts")]
fn raw_result() -> RawResult {
    RawResult {
        sequence: 0,
        path: "sample.py".into(),
        span_start: 0,
        span_length: 1,
        original: "+".into(),
        replacement: "-".into(),
        operator: "binary".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: "hash".into(),
        outcome_json: serde_json::to_string(&MutationOutcome::NotRun).unwrap(),
        resource_mode: "hard".into(),
    }
}

#[cfg(feature = "contracts")]
#[test]
fn contract_readback_rejects_invalid_resource_outcome_and_numeric_columns() {
    for invalid in [
        "resource",
        "outcome",
        "sequence",
        "span start",
        "span length",
        "line",
        "column",
    ] {
        let mut row = raw_result();
        match invalid {
            "resource" => row.resource_mode = "unknown".into(),
            "outcome" => row.outcome_json = "invalid".into(),
            "sequence" => row.sequence = -1,
            "span start" => row.span_start = -1,
            "span length" => row.span_length = -1,
            "line" => row.line = i64::MAX,
            _ => row.column = -1,
        }
        assert!(row.into_result("run", "mutant").is_err());
    }
    let mut row = raw_result();
    row.resource_mode = "best_effort".into();
    assert_eq!(
        row.into_result("run", "mutant").unwrap().resource_mode,
        ResourceMode::BestEffort
    );
}

fn unconstrained_runs(handler: &SessionHandler) {
    handler.connection.execute_batch("ALTER TABLE runs RENAME TO intact_runs; CREATE TABLE runs AS SELECT * FROM intact_runs;").unwrap();
}

#[test]
fn lookup_and_persist_reject_corrupt_run_state_without_writing_results() {
    for value in ["NULL", "2"] {
        let (_directory, mut handler) = fixture();
        handler.begin(begin_request()).unwrap();
        unconstrained_runs(&handler);
        handler
            .connection
            .execute_batch(&format!("UPDATE runs SET complete={value}"))
            .unwrap();
        let lookup = LookupStoredResult {
            id: EffectId(10),
            worker: 0,
            run_id: "coverage-run".into(),
            mutant_id: "mutant".into(),
        };
        assert!(
            matches!(handler.lookup(&lookup).unwrap_err().failure, EffectFailure::SessionDatabase { code, .. } if code == "session.corrupt")
        );
        assert!(
            matches!(handler.persist(&persist_request()).unwrap_err().failure, EffectFailure::SessionDatabase { code, .. } if code == "session.corrupt")
        );
        assert_eq!(
            handler
                .connection
                .query_row("SELECT count(*) FROM results", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn lookup_rejects_a_deleted_owned_run_and_database_read_errors() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    let lookup = LookupStoredResult {
        id: EffectId(10),
        worker: 0,
        run_id: "coverage-run".into(),
        mutant_id: "mutant".into(),
    };
    handler
        .connection
        .execute_batch("DELETE FROM runs")
        .unwrap();
    assert!(
        matches!(handler.lookup(&lookup).unwrap_err().failure, EffectFailure::SessionDatabase { code, .. } if code == "session.lookup.state")
    );
    handler.connection.execute_batch("DROP TABLE runs").unwrap();
    assert_database_error(handler.lookup(&lookup).unwrap_err(), "lookup run state");
    assert_database_error(
        handler.persist(&persist_request()).unwrap_err(),
        "read run state",
    );
}

#[test]
fn missing_result_table_is_reported_at_lookup_and_persist_read_boundaries() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler
        .connection
        .execute_batch("DROP TABLE results")
        .unwrap();
    let lookup = LookupStoredResult {
        id: EffectId(10),
        worker: 0,
        run_id: "coverage-run".into(),
        mutant_id: "mutant".into(),
    };
    assert_database_error(handler.lookup(&lookup).unwrap_err(), "lookup stored result");
    assert_database_error(
        handler.persist(&persist_request()).unwrap_err(),
        "read existing result",
    );
}

#[test]
fn loading_rejects_null_run_identifiers_and_fingerprint_versions() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    unconstrained_runs(&handler);
    handler
        .connection
        .execute_batch("UPDATE runs SET run_id=NULL")
        .unwrap();
    assert!(
        matches!(handler.load(&load_request()).unwrap_err().failure, EffectFailure::SessionDatabase { message, .. } if message == "NULL run ID")
    );

    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler.connection.execute_batch("ALTER TABLE fingerprints RENAME TO intact_fingerprints; CREATE TABLE fingerprints AS SELECT * FROM intact_fingerprints; UPDATE fingerprints SET schema_version=NULL;").unwrap();
    let mut request = load_request();
    request.fingerprint = RunFingerprint::from_bytes([8; 32]);
    assert!(
        matches!(handler.load(&request).unwrap_err().failure, EffectFailure::SessionDatabase { message, .. } if message == "NULL fingerprint schema version")
    );
}

#[cfg(feature = "contracts")]
#[test]
fn contract_readback_reports_missing_rows_and_invalid_diagnostics() {
    let (_directory, mut handler) = fixture();
    assert!(handler.read_result("missing", "missing").is_err());
    handler.begin(begin_request()).unwrap();
    handler.persist(&persist_request()).unwrap();
    handler.connection.execute_batch("ALTER TABLE diagnostics RENAME TO intact_diagnostics; CREATE TABLE diagnostics AS SELECT * FROM intact_diagnostics; UPDATE diagnostics SET level=NULL").unwrap();
    assert!(
        handler
            .read_result("coverage-run", "mutant")
            .unwrap_err()
            .contains("Null")
    );
    handler
        .connection
        .execute_batch("DROP TABLE diagnostics")
        .unwrap();
    assert!(
        handler
            .read_result("coverage-run", "mutant")
            .unwrap_err()
            .contains("diagnostics")
    );
}

#[test]
fn resolved_session_artifacts_reject_empty_names_and_symlink_ancestor_loops() {
    assert_eq!(
        SessionArtifacts::resolve("").unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        SessionArtifacts::resolve(directory.path())
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    #[cfg(unix)]
    {
        let link = directory.path().join("cycle");
        std::os::unix::fs::symlink(&link, &link).unwrap();
        assert!(SessionArtifacts::resolve(link.join("session.sqlite3")).is_err());
    }
}

#[test]
fn finish_rejects_an_externally_completed_owned_run_and_keeps_ownership_for_retry() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler
        .connection
        .execute_batch("UPDATE runs SET complete=1")
        .unwrap();
    let request = FinishSession {
        id: EffectId(30),
        run_id: "coverage-run".into(),
        complete: true,
    };
    let error = handler.finish(request.clone()).unwrap_err();
    assert!(
        matches!(error.failure, EffectFailure::SessionDatabase { code, .. } if code == "session.finish.state")
    );
    assert!(handler.ownerships.contains_key("coverage-run"));
    handler
        .connection
        .execute_batch("UPDATE runs SET complete=0")
        .unwrap();
    handler.finish(request).unwrap();
    assert!(handler.ownerships.is_empty());
}

unsafe extern "C" fn restrict_diagnostic_bindings(
    database: *mut std::ffi::c_void,
    action: std::ffi::c_int,
    table: *const std::ffi::c_char,
    _column: *const std::ffi::c_char,
    _schema: *const std::ffi::c_char,
    _trigger: *const std::ffi::c_char,
) -> std::ffi::c_int {
    if action == rusqlite::ffi::SQLITE_READ && !table.is_null() {
        // SAFETY: SQLite supplies a live, NUL-terminated table name for SQLITE_READ.
        let table = unsafe { std::ffi::CStr::from_ptr(table) };
        if table.to_bytes() == b"diagnostics" {
            // SAFETY: the caller retains this connection until the authorizer is removed;
            // sqlite3_limit changes this connection's limits without executing SQL.
            unsafe {
                rusqlite::ffi::sqlite3_limit(
                    database.cast(),
                    rusqlite::ffi::SQLITE_LIMIT_LENGTH,
                    5,
                );
            }
        }
    }
    rusqlite::ffi::SQLITE_OK
}

#[test]
fn diagnostic_query_binding_errors_are_returned_without_partial_readback() {
    let (_directory, mut handler) = fixture();
    let run_id = "r".repeat(100);
    let mut begin = begin_request();
    begin.run_id.clone_from(&run_id);
    handler.begin(begin).unwrap();
    let mut persisted = persist_request();
    persisted.result.run_id.clone_from(&run_id);
    handler.persist(&persisted).unwrap();
    // SAFETY: the connection remains live and unused by other threads throughout
    // authorizer installation, synchronous readback, and authorizer removal.
    let database = unsafe { handler.connection.handle() };
    // SAFETY: a negative limit queries this live connection without changing it.
    let original_limit =
        unsafe { rusqlite::ffi::sqlite3_limit(database, rusqlite::ffi::SQLITE_LIMIT_LENGTH, -1) };
    // SAFETY: the callback receives precisely the live SQLite handle passed here.
    assert_eq!(
        unsafe {
            rusqlite::ffi::sqlite3_set_authorizer(
                database,
                Some(restrict_diagnostic_bindings),
                database.cast(),
            )
        },
        rusqlite::ffi::SQLITE_OK
    );
    let result = handler.read_result(&run_id, "mutant");
    // SAFETY: this removes the callback before restoring the still-live connection.
    assert_eq!(
        unsafe { rusqlite::ffi::sqlite3_set_authorizer(database, None, std::ptr::null_mut()) },
        rusqlite::ffi::SQLITE_OK
    );
    // SAFETY: the handle remains owned by this handler; restore the default limit.
    unsafe {
        rusqlite::ffi::sqlite3_limit(database, rusqlite::ffi::SQLITE_LIMIT_LENGTH, original_limit);
    }
    assert!(result.unwrap_err().contains("string or blob too big"));
    assert_eq!(
        handler
            .read_result(&run_id, "mutant")
            .unwrap()
            .diagnostics
            .len(),
        1
    );
}

#[test]
fn a_relative_session_filename_resolves_against_the_current_directory_without_creating_it() {
    let name = format!("hoimin-session-{}.sqlite3", uuid::Uuid::new_v4());
    let artifacts = SessionArtifacts::resolve(&name).unwrap();
    assert_eq!(
        artifacts.database(),
        std::fs::canonicalize(".").unwrap().join(name)
    );
    assert!(!artifacts.database().exists());
}

#[tokio::test]
async fn dispatcher_load_reports_an_empty_history_without_claiming_a_run() {
    let directory = tempfile::tempdir().unwrap();
    let dispatcher = SessionDispatcher::open(directory.path().join("session.sqlite3"))
        .await
        .unwrap();
    let loaded = dispatcher.load(load_request()).await.unwrap();
    assert_eq!(
        loaded.outcome,
        SessionLoadOutcome::Fresh(hoimin_core::ResumeFreshReason::NoPriorRun)
    );
    assert!(dispatcher.handler.lock().unwrap().ownerships.is_empty());
}

#[test]
fn beginning_again_after_external_row_removal_retains_the_existing_finish_authority() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler
        .connection
        .execute_batch("DELETE FROM runs")
        .unwrap();
    handler.begin(begin_request()).unwrap();
    assert_eq!(handler.ownerships.len(), 1);
    handler
        .finish(FinishSession {
            id: EffectId(40),
            run_id: "coverage-run".into(),
            complete: true,
        })
        .unwrap();
    assert!(handler.ownerships.is_empty());
}
