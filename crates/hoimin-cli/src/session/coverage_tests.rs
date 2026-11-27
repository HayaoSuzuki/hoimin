use super::*;
use hoimin_core::{
    ByteSpan, MutationCandidate, OutputSpoolRef, PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE,
    ProcessOutputState, ProcessTermination, ResumeDecision, ReusedMutationStatus, RunFingerprint,
    resume_policy,
};
use serde::Deserialize;
use std::num::NonZeroUsize;
use std::time::Duration;

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

pub(super) fn reject_commit(handler: &SessionHandler, event: &str, table: &str) {
    handler.connection.execute_batch(&format!(
        "CREATE TEMP TABLE fault_parent(id INTEGER PRIMARY KEY);
         CREATE TEMP TABLE deferred_fault(parent INTEGER REFERENCES fault_parent(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TEMP TRIGGER reject_commit AFTER {event} ON main.{table} BEGIN INSERT INTO deferred_fault VALUES(42); END;"
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadRecheckCase {
    schema: u64,
    mode: String,
    finish_before_acquire: Option<bool>,
    expected_status: String,
    expected_loader_owns: bool,
    expected_lock_held: bool,
    expected_run_finished: Option<bool>,
    expected_run_complete: Option<bool>,
}

#[test]
fn resume_candidate_interleaving_matches_lean() {
    let cases: Vec<LoadRecheckCase> =
        include_str!("../../../../formal/HoiminOracle/corpus/session-load-recheck.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    let candidates: std::collections::BTreeSet<_> = cases
        .iter()
        .map(|case| case.finish_before_acquire)
        .collect();
    assert_eq!(candidates, [None, Some(false), Some(true)].into());
    assert_eq!(cases.len(), candidates.len());
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        check_resume_interleaving(case);
    }
}

fn check_resume_interleaving(case: &LoadRecheckCase) {
    let (directory, mut owner) = fixture();
    let begin = begin_request();
    let run_id = begin.run_id.clone();
    owner.begin(begin).unwrap();
    let path = directory.path().join("session.sqlite3");
    let mut loader = SessionHandler::open(&path).unwrap();
    let mut owner = Some(owner);
    let finish = case
        .finish_before_acquire
        .map(|complete| (owner.take().unwrap(), complete));
    let selected_run = run_id.clone();
    loader.resume_after_candidate = Some(Box::new(move |candidate| {
        assert_eq!(candidate, selected_run);
        if let Some((mut owner, complete)) = finish {
            owner
                .finish(FinishSession {
                    id: EffectId(5),
                    run_id: selected_run,
                    complete,
                })
                .unwrap();
        }
    }));

    let actual = loader.load(&load_request());
    assert!(loader.resume_after_candidate.is_none(), "{case:?}");
    assert_eq!(
        load_outcome_name(&actual, &run_id),
        case.expected_status,
        "{case:?}"
    );
    assert_eq!(
        loader.ownerships.contains_key(&run_id),
        case.expected_loader_owns,
        "{case:?}"
    );

    let observer = SessionHandler::open(path).unwrap();
    let probe = observer.acquire_ownership(EffectId(6), &run_id, "inspect lock after load");
    if case.expected_lock_held {
        assert_eq!(
            probe.err().unwrap().failure.code(),
            "session.resume.active",
            "{case:?}"
        );
    } else {
        drop(probe.unwrap());
    }
    drop(owner);
    let row: Option<(bool, bool)> = observer
        .connection
        .query_row(
            "SELECT finished, complete FROM runs WHERE run_id=?1",
            [&run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .unwrap();
    assert_eq!(row.map(|row| row.0), case.expected_run_finished, "{case:?}");
    assert_eq!(row.map(|row| row.1), case.expected_run_complete, "{case:?}");
}

fn load_outcome_name<'a>(actual: &'a Result<SessionLoaded, EffectFailed>, run_id: &str) -> &'a str {
    match actual {
        Ok(SessionLoaded {
            outcome: SessionLoadOutcome::Resumed(resume),
            ..
        }) => {
            assert_eq!(resume.run_id, run_id);
            "resumed"
        }
        Ok(SessionLoaded {
            outcome: SessionLoadOutcome::Fresh(reason),
            ..
        }) => reason.code(),
        Err(error) => error.failure.code(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResumeBudgetRecheckCase {
    schema: u8,
    mode: String,
    observed_budget: NonZeroUsize,
    intervening_budget: NonZeroUsize,
    requested_budget: NonZeroUsize,
    expected_first: String,
    expected_after_first: u64,
    expected_retry: String,
    expected_after_retry: u64,
}

#[test]
fn resume_budget_recheck_and_retry_match_lean() {
    let cases: Vec<ResumeBudgetRecheckCase> =
        include_str!("../../../../formal/HoiminOracle/corpus/resume-budget-recheck.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(cases.len(), 12);
    let mut seen = std::collections::BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert!(seen.insert((
            case.observed_budget,
            case.intervening_budget,
            case.requested_budget
        )));
        check_resume_budget_recheck(case);
    }
}

fn saved_mutant_budget(handler: &SessionHandler, run_id: &str) -> u64 {
    let bytes: Vec<u8> = handler
        .connection
        .query_row(
            "SELECT max_mutants FROM runs WHERE run_id=?1",
            [run_id],
            |row| row.get(0),
        )
        .unwrap();
    u64::from_be_bytes(bytes.try_into().unwrap())
}

fn check_resume_budget_recheck(case: &ResumeBudgetRecheckCase) {
    let (directory, mut owner) = fixture();
    let mut begin = begin_request();
    begin.max_mutants = case.observed_budget;
    let run_id = begin.run_id.clone();
    owner.begin(begin).unwrap();
    assert_eq!(
        saved_mutant_budget(&owner, &run_id),
        case.observed_budget.get() as u64
    );
    let path = directory.path().join("session.sqlite3");
    let mut loader = SessionHandler::open(&path).unwrap();
    let mut intervening = load_request();
    intervening.max_mutants = case.intervening_budget;
    let selected_run = run_id.clone();
    loader.resume_after_candidate = Some(Box::new(move |candidate| {
        assert_eq!(candidate, selected_run);
        assert_eq!(
            load_outcome_name(&owner.load(&intervening), candidate),
            "resumed"
        );
        assert_eq!(
            saved_mutant_budget(&owner, candidate),
            intervening.max_mutants.get() as u64
        );
        owner
            .finish(FinishSession {
                id: EffectId(5),
                run_id: selected_run,
                complete: false,
            })
            .unwrap();
    }));

    let mut request = load_request();
    request.max_mutants = case.requested_budget;
    let first = loader.load(&request);
    assert!(loader.resume_after_candidate.is_none(), "{case:?}");
    assert_eq!(
        load_outcome_name(&first, &run_id),
        case.expected_first,
        "{case:?}"
    );
    assert_eq!(
        saved_mutant_budget(&loader, &run_id),
        case.expected_after_first,
        "{case:?}"
    );
    let mut retry = SessionHandler::open(path).unwrap();
    if case.expected_first == "candidate_changed" {
        drop(
            retry
                .acquire_ownership(EffectId(6), &run_id, "inspect stale budget lock")
                .unwrap(),
        );
    }
    drop(loader);
    assert_eq!(
        load_outcome_name(&retry.load(&request), &run_id),
        case.expected_retry,
        "{case:?}"
    );
    assert_eq!(
        saved_mutant_budget(&retry, &run_id),
        case.expected_after_retry,
        "{case:?}"
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

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinishWaiter {
    Awaited,
    Cancelled,
    DriverDropped,
}

#[derive(Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FinishResumeProbe {
    #[serde(rename = "session.resume.active")]
    Active,
    Resumed,
    MatchingRunComplete,
}

#[derive(Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishWaitSnapshot {
    finished: bool,
    complete: bool,
    resume: FinishResumeProbe,
    result_count: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishWaitCase {
    schema: u8,
    mode: String,
    id: String,
    waiter: FinishWaiter,
    prior_finished: bool,
    complete: bool,
    commit: bool,
    pending: FinishWaitSnapshot,
    settled: FinishWaitSnapshot,
    reply: Option<bool>,
}

fn observe_finish_wait(probe: &mut SessionHandler, run_id: &str) -> FinishWaitSnapshot {
    let (finished, complete) = probe
        .connection
        .query_row(
            "SELECT finished, complete FROM runs WHERE run_id=?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let result_count = probe
        .connection
        .query_row(
            "SELECT count(*) FROM results WHERE run_id=?1",
            [run_id],
            |row| row.get(0),
        )
        .unwrap();
    let resume = match probe.load(&load_request()) {
        Ok(SessionLoaded {
            outcome: SessionLoadOutcome::Resumed(result),
            ..
        }) => {
            assert_eq!(result.run_id, run_id);
            FinishResumeProbe::Resumed
        }
        Ok(SessionLoaded {
            outcome: SessionLoadOutcome::Fresh(hoimin_core::ResumeFreshReason::MatchingRunComplete),
            ..
        }) => FinishResumeProbe::MatchingRunComplete,
        Err(EffectFailed {
            failure: EffectFailure::SessionDatabase { code, .. },
            ..
        }) if code == "session.resume.active" => FinishResumeProbe::Active,
        result => panic!("unexpected resume probe: {result:?}"),
    };
    FinishWaitSnapshot {
        finished,
        complete,
        resume,
        result_count,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn finish_commit_and_ownership_after_waiter_cancellation_match_lean() {
    let mut ids = std::collections::BTreeSet::new();
    for line in
        include_str!("../../../../formal/HoiminOracle/corpus/session-finish-wait.jsonl").lines()
    {
        let case: FinishWaitCase = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()));
        replay_finish_wait(case).await;
    }
    assert_eq!(ids.len(), 24);
}

fn seed_finish_wait(
    owner: &mut SessionHandler,
    case: &FinishWaitCase,
) -> (FinishSession, MutantResult) {
    let begin = begin_request();
    let request = FinishSession {
        id: EffectId(30),
        run_id: begin.run_id.clone(),
        complete: case.complete,
    };
    owner.begin(begin).unwrap();
    let persisted = persist_request();
    owner.persist(&persisted).unwrap();
    let saved = owner
        .read_result(&request.run_id, &persisted.result.candidate.id)
        .unwrap();
    if case.prior_finished {
        owner
            .finish(FinishSession {
                complete: false,
                ..request.clone()
            })
            .unwrap();
        assert!(matches!(
            owner.load(&load_request()).unwrap().outcome,
            SessionLoadOutcome::Resumed(_)
        ));
    }
    if !case.commit {
        reject_commit(owner, "UPDATE", "runs");
    }
    (request, saved)
}

async fn replay_finish_wait(case: FinishWaitCase) {
    let id = &case.id;
    let (directory, mut owner) = fixture();
    let (request, saved) = seed_finish_wait(&mut owner, &case);
    let run = &request.run_id;
    let mutant = &saved.candidate.id;
    let mut probe = SessionHandler::open(directory.path().join("session.sqlite3")).unwrap();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, hold) = std::sync::mpsc::channel();
    owner.finish_before_commit = Some(Box::new(move || {
        entered.send(()).unwrap();
        hold.recv_timeout(Duration::from_secs(3)).unwrap();
    }));
    let dispatcher = SessionDispatcher {
        handler: Arc::new(Mutex::new(owner)),
    };
    let handler = Arc::downgrade(&dispatcher.handler);
    let mut finish = Box::pin(dispatcher.finish(request.clone()));
    tokio::select! {
        result = &mut finish => panic!("{id}: finish returned before the commit gate: {result:?}"),
        before_commit = ready => before_commit.unwrap(),
    }

    let mut check_pending = || {
        assert!(handler.upgrade().is_some(), "{id}: worker lost its handler");
        assert_eq!(
            observe_finish_wait(&mut probe, run),
            case.pending,
            "{id}: pending"
        );
        assert_eq!(
            probe.read_result(run, mutant).unwrap(),
            saved,
            "{id}: pending result"
        );
        release.send(()).unwrap();
    };
    let reply = match case.waiter {
        FinishWaiter::Awaited => {
            check_pending();
            match finish.await {
                Ok(reply) => {
                    assert_eq!(
                        (reply.id, reply.run_id.as_str(), reply.complete),
                        (request.id, request.run_id.as_str(), request.complete)
                    );
                    Some(true)
                }
                Err(error) => {
                    assert_eq!(error.id, request.id);
                    assert_database_error(error, "commit run finish");
                    Some(false)
                }
            }
        }
        FinishWaiter::Cancelled => {
            drop(finish);
            check_pending();
            // The serialized dispatcher lock is held until the finish operation returns.
            dispatcher.call(EffectId(31), |_| Ok(())).await.unwrap();
            None
        }
        FinishWaiter::DriverDropped => {
            drop(finish);
            drop(dispatcher);
            check_pending();
            SessionDispatcher::wait_for_session_worker_drop(&handler).await;
            None
        }
    };
    assert_eq!(reply, case.reply, "{id}");
    assert_eq!(
        observe_finish_wait(&mut probe, run),
        case.settled,
        "{id}: settled"
    );
    assert_eq!(
        probe.read_result(run, mutant).unwrap(),
        saved,
        "{id}: settled result"
    );
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessResumeCase {
    schema: u64,
    id: String,
    termination: String,
    close_timed_out: bool,
    expected_stored: Option<String>,
    expected_reused: Option<String>,
    expected_fault_error: String,
    expected_stored_after_fault: Option<String>,
    expected_replacement_error: Option<String>,
    expected_stored_after_replacement: Option<String>,
}

fn executed_outcome(
    termination: ProcessTermination,
    timed_out: bool,
    token: &str,
) -> MutationOutcome {
    let (output_state, retained, observed) = if timed_out {
        (ProcessOutputState::CloseTimedOut, 0, 0)
    } else {
        (ProcessOutputState::Complete, 3, 8)
    };
    MutationOutcome::Executed {
        termination,
        output_state,
        output: OutputSpoolRef {
            token: token.to_owned(),
            retained,
            observed,
        },
        elapsed: Duration::from_millis(5),
    }
}

#[test]
fn reopened_process_results_and_replacement_match_lean() {
    let cases: Vec<ProcessResumeCase> =
        include_str!("../../../../formal/HoiminOracle/corpus/process-output-resume.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(cases.len(), 12);
    let mut seen = std::collections::BTreeSet::new();
    for case in &cases {
        assert_eq!(case.schema, 2);
        assert!(seen.insert((&case.termination, case.close_timed_out)));
        check_process_resume(case);
    }
}

fn check_process_resume(case: &ProcessResumeCase) {
    let termination = match case.termination.as_str() {
        "exit_success" => ProcessTermination::Exit(0),
        "exit_failure" => ProcessTermination::Exit(7),
        "timeout" => ProcessTermination::Timeout,
        "out_of_memory" => ProcessTermination::OutOfMemory,
        "process_limit" => ProcessTermination::ProcessLimit,
        "cancelled" => ProcessTermination::Cancelled,
        other => panic!("unknown process termination {other}"),
    };

    let (directory, mut handler) = fixture();
    handler.begin(begin_request()).unwrap();
    let mut request = persist_request();
    request.result.outcome = executed_outcome(termination, case.close_timed_out, "capture");
    request.result.diagnostics.clear();
    if case.close_timed_out {
        request.result.diagnostics.push(SessionDiagnostic {
            mutant_id: request.result.candidate.id.clone(),
            level: "error".to_owned(),
            code: PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE.to_owned(),
            message: "capture timed out".to_owned(),
        });
    }
    handler.persist(&request).unwrap();
    let lookup = LookupStoredResult {
        id: EffectId(5),
        worker: request.worker,
        run_id: request.result.run_id.clone(),
        mutant_id: request.result.candidate.id.clone(),
    };
    handler
        .finish(FinishSession {
            id: EffectId(4),
            run_id: lookup.run_id.clone(),
            complete: false,
        })
        .unwrap();
    drop(handler);

    let path = directory.path().join("session.sqlite3");
    let mut handler = reopen_process_session(&path, &lookup.run_id, &case.id);
    let stored = handler.lookup(&lookup).unwrap().result.unwrap();
    assert_eq!(stored.mutant_id, lookup.mutant_id, "{}", case.id);
    assert_eq!(
        Some(stored.status.as_str()),
        case.expected_stored.as_deref(),
        "{}",
        case.id
    );
    let reused = match resume_policy(Some(&stored)) {
        ResumeDecision::Rerun => None,
        ResumeDecision::Reuse(ReusedMutationStatus::Killed) => Some("killed"),
        ResumeDecision::Reuse(ReusedMutationStatus::Survived) => Some("survived"),
    };
    assert_eq!(reused, case.expected_reused.as_deref(), "{}", case.id);
    assert_eq!(
        handler
            .read_result(&lookup.run_id, &lookup.mutant_id)
            .unwrap(),
        request.result,
        "{}",
        case.id
    );

    check_process_replacement(case, &path, handler, &request, &lookup);
}

fn reopen_process_session(path: &Path, run_id: &str, case: &str) -> SessionHandler {
    let mut handler = SessionHandler::open(path).unwrap();
    let loaded = handler.load(&load_request()).unwrap();
    let SessionLoadOutcome::Resumed(resume) = loaded.outcome else {
        panic!("{case}: expected the stored run to resume");
    };
    assert_eq!(resume.run_id, run_id, "{case}");
    handler
}

fn check_process_replacement(
    case: &ProcessResumeCase,
    path: &Path,
    mut handler: SessionHandler,
    request: &PersistResult,
    lookup: &LookupStoredResult,
) {
    let mut replacement = request.clone();
    replacement.id = EffectId(6);
    replacement.result.outcome = executed_outcome(ProcessTermination::Exit(0), false, "rerun");
    replacement.result.diagnostics.clear();
    reject_commit(&handler, "INSERT", "results");
    let failed = handler.persist(&replacement).unwrap_err();
    assert_eq!(
        failed.failure.code(),
        case.expected_fault_error,
        "{}",
        case.id
    );
    if case.expected_fault_error == "session.commit" {
        assert_database_error(failed, "commit mutant result");
    }
    assert!(handler.connection.is_autocommit());
    let fault_rows: i64 = handler
        .connection
        .query_row("SELECT count(*) FROM temp.deferred_fault", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(fault_rows, 0, "{}", case.id);
    drop(handler);

    let mut handler = reopen_process_session(path, &lookup.run_id, &case.id);
    let stored = handler.lookup(lookup).unwrap().result.unwrap();
    assert_eq!(
        Some(stored.status.as_str()),
        case.expected_stored_after_fault.as_deref(),
        "{}",
        case.id
    );
    assert_eq!(
        handler
            .read_result(&lookup.run_id, &lookup.mutant_id)
            .unwrap(),
        request.result,
        "{}",
        case.id
    );
    replacement.id = EffectId(7);
    let error = handler.persist(&replacement).err();
    assert_eq!(
        error.as_ref().map(|error| error.failure.code()),
        case.expected_replacement_error.as_deref(),
        "{}",
        case.id
    );
    let stored = handler.lookup(lookup).unwrap().result.unwrap();
    assert_eq!(
        Some(stored.status.as_str()),
        case.expected_stored_after_replacement.as_deref(),
        "{}",
        case.id
    );
    let expected_result = if case.expected_replacement_error.is_some() {
        &request.result
    } else {
        &replacement.result
    };
    assert_eq!(
        &handler
            .read_result(&lookup.run_id, &lookup.mutant_id)
            .unwrap(),
        expected_result,
        "{}",
        case.id
    );
}
