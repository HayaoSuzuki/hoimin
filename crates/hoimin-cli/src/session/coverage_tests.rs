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

#[rstest::rstest]
#[case::begin(|handler: &mut SessionHandler| handler.begin(begin_request()).unwrap_err(), "begin run transaction")]
#[case::persist(|handler: &mut SessionHandler| handler.persist(&persist_request()).unwrap_err(), "begin result transaction")]
#[case::finish(|handler: &mut SessionHandler| handler.finish(FinishSession { id: EffectId(4), run_id: "coverage-run".into(), complete: true }).unwrap_err(), "begin finish transaction")]
fn transaction_begin_failures_are_reported_at_each_lifecycle_boundary(
    #[case] execute: fn(&mut SessionHandler) -> EffectFailed,
    #[case] expected: &str,
) {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler
        .connection
        .execute_batch("PRAGMA query_only=ON")
        .unwrap();

    let error = execute(&mut handler);

    assert_database_error(error, expected);
}

#[rstest::rstest]
#[case::begin(|_: &mut SessionHandler| {}, "INSERT", |handler: &mut SessionHandler| handler.begin(begin_request()).unwrap_err(), "insert run")]
#[case::finish(|handler: &mut SessionHandler| { handler.begin(begin_request()).unwrap(); }, "UPDATE", |handler: &mut SessionHandler| handler.finish(FinishSession { id: EffectId(4), run_id: "coverage-run".into(), complete: true }).unwrap_err(), "mark run finished")]
fn run_insert_and_finish_update_errors_preserve_the_operation(
    #[case] prepare: fn(&mut SessionHandler),
    #[case] event: &str,
    #[case] execute: fn(&mut SessionHandler) -> EffectFailed,
    #[case] expected: &str,
) {
    let (_directory, mut handler) = fixture();
    prepare(&mut handler);
    reject_write(&handler, event, "runs");

    let error = execute(&mut handler);

    assert_database_error(error, expected);
}

#[rstest::rstest]
#[case::begin(|_: &mut SessionHandler| {}, "INSERT", "runs", |handler: &mut SessionHandler| handler.begin(begin_request()).unwrap_err(), "commit run")]
#[case::persist(|handler: &mut SessionHandler| { handler.begin(begin_request()).unwrap(); }, "INSERT", "results", |handler: &mut SessionHandler| handler.persist(&persist_request()).unwrap_err(), "commit mutant result")]
#[case::finish(|handler: &mut SessionHandler| { handler.begin(begin_request()).unwrap(); }, "UPDATE", "runs", |handler: &mut SessionHandler| handler.finish(FinishSession { id: EffectId(4), run_id: "coverage-run".into(), complete: true }).unwrap_err(), "commit run finish")]
fn deferred_constraint_errors_are_reported_as_commit_failures(
    #[case] prepare: fn(&mut SessionHandler),
    #[case] event: &str,
    #[case] table: &str,
    #[case] execute: fn(&mut SessionHandler) -> EffectFailed,
    #[case] expected: &str,
) {
    let (_directory, mut handler) = fixture();
    prepare(&mut handler);
    reject_commit(&handler, event, table);

    let error = execute(&mut handler);
    let pending_faults = handler
        .connection
        .query_row("SELECT count(*) FROM deferred_fault", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();

    let autocommit_restored = handler.connection.is_autocommit();

    assert_database_error(error, expected);
    assert!(autocommit_restored);
    assert_eq!(pending_faults, 0);
}

#[rstest::rstest]
#[case::schema(|handler: &mut SessionHandler| handler.connection.execute_batch("DROP TABLE fingerprints").unwrap(), |handler: &mut SessionHandler| handler.load(&load_request()).unwrap_err(), "inspect resume schema")]
#[case::run(|handler: &mut SessionHandler| handler.connection.execute_batch("DROP TABLE fingerprints; DROP TABLE runs").unwrap(), |handler: &mut SessionHandler| handler.load(&load_request()).unwrap_err(), "load resume run")]
#[case::history(|handler: &mut SessionHandler| handler.connection.execute_batch("DROP TABLE fingerprints; DROP TABLE runs").unwrap(), |handler: &mut SessionHandler| handler.fresh_resume_reason(&load_request()).unwrap_err(), "inspect resume history")]
#[case::resume_recheck(|handler: &mut SessionHandler| { handler.begin(begin_request()).unwrap(); reject_write(handler, "UPDATE", "runs"); }, |handler: &mut SessionHandler| handler.load(&load_request()).unwrap_err(), "re-read resume run")]
fn loading_reports_query_and_resume_recheck_failures(
    #[case] prepare: fn(&mut SessionHandler),
    #[case] execute: fn(&mut SessionHandler) -> EffectFailed,
    #[case] expected: &str,
) {
    let (_directory, mut handler) = fixture();
    prepare(&mut handler);

    let error = execute(&mut handler);

    assert_database_error(error, expected);
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

    let actual = handler.load(&load_request()).unwrap().outcome;

    assert_eq!(
        actual,
        SessionLoadOutcome::Fresh(hoimin_core::ResumeFreshReason::CandidateChanged)
    );
}

#[rstest::rstest]
#[case(|_: &mut SessionHandler| {}, "INSERT", "candidates", "insert candidate", 0)]
#[case(|_: &mut SessionHandler| {}, "INSERT", "results", "insert result", 0)]
#[case(|_: &mut SessionHandler| {}, "INSERT", "diagnostics", "insert diagnostic", 0)]
#[case(|handler: &mut SessionHandler| { handler.persist(&persist_request()).unwrap(); }, "DELETE", "diagnostics", "replace inconclusive result", 1)]
#[case(|handler: &mut SessionHandler| { handler.persist(&persist_request()).unwrap(); }, "DELETE", "results", "replace inconclusive result", 1)]
#[case(|handler: &mut SessionHandler| { handler.persist(&persist_request()).unwrap(); }, "DELETE", "candidates", "replace inconclusive result", 1)]
fn persistence_distinguishes_candidate_result_diagnostic_and_replacement_errors(
    #[case] prepare: fn(&mut SessionHandler),
    #[case] event: &str,
    #[case] table: &str,
    #[case] expected: &str,
    #[case] retained_rows: i64,
) {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    prepare(&mut handler);
    reject_write(&handler, event, table);

    let error = handler.persist(&persist_request()).unwrap_err();
    let row_counts = ["candidates", "results", "diagnostics"].map(|relation| {
        handler
            .connection
            .query_row(&format!("SELECT count(*) FROM {relation}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    });

    assert_database_error(error, expected);

    assert_eq!(row_counts, [retained_rows; 3]);
}

#[test]
fn candidate_overflow_is_a_database_conversion_error() {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    let mut request = persist_request();
    request.result.candidate.sequence = u64::MAX;
    assert_database_error(handler.persist(&request).unwrap_err(), "insert candidate");
}

#[test]
fn unsigned_database_conversion_rejects_overflow() {
    let result = to_i64(u64::MAX);

    let actual = result.is_err();

    assert!(actual);
}

#[test]
fn invalid_sql_is_not_a_constraint_violation() {
    let classified = is_constraint(&rusqlite::Error::InvalidQuery);

    assert!(!classified);
}

#[test]
fn ownership_io_errors_do_not_claim_a_run() {
    let (directory, mut handler) = fixture();
    handler.lock_directory = directory.path().join("not-a-directory");
    std::fs::write(&handler.lock_directory, "file").unwrap();
    let error = handler.begin(begin_request()).unwrap_err();

    let actual = matches!(error.failure, EffectFailure::SessionDatabase { code, .. } if code == "session.ownership");
    let actual_2 = handler.ownerships.is_empty();

    assert!(actual);
    assert!(actual_2);
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
    let panic_reported =
        matches!(failed.failure, EffectFailure::Other { code, .. } if code == "session.dispatch");
    let failed = dispatcher.call(EffectId(21), operation).await.unwrap_err();
    let poison_reported = matches!(failed.failure, EffectFailure::Other { message, .. } if message == "session lock poisoned");

    assert!(panic_reported);
    assert!(poison_reported);
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
#[rstest::rstest]
#[case(|row: &mut RawResult| row.resource_mode = "unknown".into())]
#[case(|row: &mut RawResult| row.outcome_json = "invalid".into())]
#[case(|row: &mut RawResult| row.sequence = -1)]
#[case(|row: &mut RawResult| row.span_start = -1)]
#[case(|row: &mut RawResult| row.span_length = -1)]
#[case(|row: &mut RawResult| row.line = i64::MAX)]
#[case(|row: &mut RawResult| row.column = -1)]
fn contract_readback_rejects_invalid_resource_outcome_and_numeric_columns(
    #[case] corrupt: fn(&mut RawResult),
) {
    let mut row = raw_result();
    corrupt(&mut row);

    let result = row.into_result("run", "mutant");

    let actual = result.is_err();

    assert!(actual);
}

#[cfg(feature = "contracts")]
#[test]
fn contract_readback_preserves_best_effort_resource_mode() {
    let mut row = raw_result();
    row.resource_mode = "best_effort".into();

    let result = row.into_result("run", "mutant").unwrap();

    assert_eq!(result.resource_mode, ResourceMode::BestEffort);
}

fn unconstrained_runs(handler: &SessionHandler) {
    handler.connection.execute_batch("ALTER TABLE runs RENAME TO intact_runs; CREATE TABLE runs AS SELECT * FROM intact_runs;").unwrap();
}

#[rstest::rstest]
fn lookup_and_persist_reject_corrupt_run_state_without_writing_results(
    #[values("NULL", "2")] value: &str,
) {
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

    let actual = matches!(handler.lookup(&lookup).unwrap_err().failure, EffectFailure::SessionDatabase { code, .. } if code == "session.corrupt");
    let actual_2 = matches!(handler.persist(&persist_request()).unwrap_err().failure, EffectFailure::SessionDatabase { code, .. } if code == "session.corrupt");
    let actual_3 = handler
        .connection
        .query_row("SELECT count(*) FROM results", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();

    assert!(actual);
    assert!(actual_2);
    assert_eq!(actual_3, 0);
}

#[rstest::rstest]
#[case::deleted_run("DELETE FROM runs", |handler: &mut SessionHandler| handler.lookup(&LookupStoredResult { id: EffectId(10), worker: 0, run_id: "coverage-run".into(), mutant_id: "mutant".into() }).unwrap_err(), |error: &EffectFailed| matches!(&error.failure, EffectFailure::SessionDatabase { code, .. } if code == "session.lookup.state"))]
#[case::lookup_query("DROP TABLE runs", |handler: &mut SessionHandler| handler.lookup(&LookupStoredResult { id: EffectId(10), worker: 0, run_id: "coverage-run".into(), mutant_id: "mutant".into() }).unwrap_err(), |error: &EffectFailed| matches!(&error.failure, EffectFailure::SessionDatabase { operation, .. } if operation == "lookup run state"))]
#[case::persist_query("DROP TABLE runs", |handler: &mut SessionHandler| handler.persist(&persist_request()).unwrap_err(), |error: &EffectFailed| matches!(&error.failure, EffectFailure::SessionDatabase { operation, .. } if operation == "read run state"))]
fn lookup_rejects_a_deleted_owned_run_and_database_read_errors(
    #[case] sql: &str,
    #[case] execute: fn(&mut SessionHandler) -> EffectFailed,
    #[case] expected: fn(&EffectFailed) -> bool,
) {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler.connection.execute_batch(sql).unwrap();

    let error = execute(&mut handler);

    assert!(expected(&error), "{error:?}");
}

#[rstest::rstest]
#[case::lookup(|handler: &mut SessionHandler| handler.lookup(&LookupStoredResult { id: EffectId(10), worker: 0, run_id: "coverage-run".into(), mutant_id: "mutant".into() }).unwrap_err(), "lookup stored result")]
#[case::persist(|handler: &mut SessionHandler| handler.persist(&persist_request()).unwrap_err(), "read existing result")]
fn missing_result_table_is_reported_at_lookup_and_persist_read_boundaries(
    #[case] execute: fn(&mut SessionHandler) -> EffectFailed,
    #[case] expected: &str,
) {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler
        .connection
        .execute_batch("DROP TABLE results")
        .unwrap();

    let error = execute(&mut handler);

    assert_database_error(error, expected);
}

#[rstest::rstest]
#[case::run_id(|handler: &mut SessionHandler| { unconstrained_runs(handler); handler.connection.execute_batch("UPDATE runs SET run_id=NULL").unwrap(); }, [7; 32], "NULL run ID")]
#[case::fingerprint_version(|handler: &mut SessionHandler| handler.connection.execute_batch("ALTER TABLE fingerprints RENAME TO intact_fingerprints; CREATE TABLE fingerprints AS SELECT * FROM intact_fingerprints; UPDATE fingerprints SET schema_version=NULL;").unwrap(), [8; 32], "NULL fingerprint schema version")]
fn loading_rejects_null_run_identifiers_and_fingerprint_versions(
    #[case] corrupt: fn(&mut SessionHandler),
    #[case] fingerprint: [u8; 32],
    #[case] expected: &str,
) {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    corrupt(&mut handler);
    let mut request = load_request();
    request.fingerprint = RunFingerprint::from_bytes(fingerprint);

    let failure = handler.load(&request).unwrap_err().failure;

    let actual =
        matches!(failure, EffectFailure::SessionDatabase { message, .. } if message == expected);

    assert!(actual);
}

#[cfg(feature = "contracts")]
#[test]
fn contract_readback_reports_missing_rows() {
    let (_directory, handler) = fixture();

    let result = handler.read_result("missing", "missing");

    let actual = result.is_err();

    assert!(actual);
}

#[cfg(feature = "contracts")]
#[rstest::rstest]
#[case::null_level(
    "ALTER TABLE diagnostics RENAME TO intact_diagnostics; CREATE TABLE diagnostics AS SELECT * FROM intact_diagnostics; UPDATE diagnostics SET level=NULL",
    "Null"
)]
#[case::missing_table("DROP TABLE diagnostics", "diagnostics")]
fn contract_readback_reports_invalid_diagnostics(#[case] sql: &str, #[case] expected: &str) {
    let (_directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    handler.persist(&persist_request()).unwrap();
    handler.connection.execute_batch(sql).unwrap();

    let error = handler.read_result("coverage-run", "mutant").unwrap_err();

    let actual = error.contains(expected);

    assert!(actual);
}

#[test]
fn resolved_session_artifacts_reject_empty_names() {
    let error = SessionArtifacts::resolve("").unwrap_err();

    let actual = error.kind();

    assert_eq!(actual, std::io::ErrorKind::InvalidInput);
}

#[test]
fn resolved_session_artifacts_reject_a_directory_as_the_database() {
    let directory = tempfile::tempdir().unwrap();

    let error = SessionArtifacts::resolve(directory.path()).unwrap_err();

    let actual = error.kind();

    assert_eq!(actual, std::io::ErrorKind::InvalidInput);
}

#[cfg(unix)]
#[test]
fn resolved_session_artifacts_reject_symlink_ancestor_loops() {
    let directory = tempfile::tempdir().unwrap();
    let link = directory.path().join("cycle");
    std::os::unix::fs::symlink(&link, &link).unwrap();

    let result = SessionArtifacts::resolve(link.join("session.sqlite3"));

    let actual = result.is_err();

    assert!(actual);
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
    let external_finish_rejected = matches!(error.failure, EffectFailure::SessionDatabase { code, .. } if code == "session.finish.state");
    let owned_after_rejection = handler.ownerships.contains_key("coverage-run");
    handler
        .connection
        .execute_batch("UPDATE runs SET complete=0")
        .unwrap();
    handler.finish(request).unwrap();
    let released_after_retry = handler.ownerships.is_empty();

    assert!(external_finish_rejected);
    assert!(owned_after_rejection);
    assert!(released_after_retry);
}

#[cfg(feature = "contracts")]
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

// read_result is the contracts-only database readback used by persistence checks.
#[cfg(feature = "contracts")]
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
    let observed_1 = unsafe {
        rusqlite::ffi::sqlite3_set_authorizer(
            database,
            Some(restrict_diagnostic_bindings),
            database.cast(),
        )
    };
    let result = handler.read_result(&run_id, "mutant");
    // SAFETY: this removes the callback before restoring the still-live connection.
    let observed_2 =
        unsafe { rusqlite::ffi::sqlite3_set_authorizer(database, None, std::ptr::null_mut()) };
    // SAFETY: the handle remains owned by this handler; restore the default limit.
    unsafe {
        rusqlite::ffi::sqlite3_limit(database, rusqlite::ffi::SQLITE_LIMIT_LENGTH, original_limit);
    }
    let binding_error = result.unwrap_err();
    let observed_3 = handler
        .read_result(&run_id, "mutant")
        .unwrap()
        .diagnostics
        .len();

    let actual = binding_error.contains("string or blob too big");

    assert!(actual);
    assert_eq!(observed_1, rusqlite::ffi::SQLITE_OK);
    assert_eq!(observed_2, rusqlite::ffi::SQLITE_OK);
    assert_eq!(observed_3, 1);
}

#[test]
fn a_relative_session_filename_resolves_against_the_current_directory_without_creating_it() {
    let name = format!("hoimin-session-{}.sqlite3", uuid::Uuid::new_v4());
    let artifacts = SessionArtifacts::resolve(&name).unwrap();

    let actual = artifacts.database();
    let actual_2 = !artifacts.database().exists();

    assert_eq!(actual, std::fs::canonicalize(".").unwrap().join(name));
    assert!(actual_2);
}

#[tokio::test]
async fn dispatcher_load_reports_an_empty_history_without_claiming_a_run() {
    let directory = tempfile::tempdir().unwrap();
    let dispatcher = SessionDispatcher::open(directory.path().join("session.sqlite3"))
        .await
        .unwrap();
    let loaded = dispatcher.load(load_request()).await.unwrap();

    let actual = dispatcher.handler.lock().unwrap().ownerships.is_empty();

    assert_eq!(
        loaded.outcome,
        SessionLoadOutcome::Fresh(hoimin_core::ResumeFreshReason::NoPriorRun)
    );
    assert!(actual);
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
    let ownerships_before_finish = handler.ownerships.len();
    handler
        .finish(FinishSession {
            id: EffectId(40),
            run_id: "coverage-run".into(),
            complete: true,
        })
        .unwrap();
    let released_after_finish = handler.ownerships.is_empty();

    assert_eq!(ownerships_before_finish, 1);
    assert!(released_after_finish);
}
