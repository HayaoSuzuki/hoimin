use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use hoimin_cli::session::SessionHandler;
use hoimin_core::{
    BeginSession, ByteSpan, EffectFailed, EffectFailure, EffectId, FinishSession, LoadSession,
    LookupStoredResult, MutantResult, MutationCandidate, MutationStatus, OutputSpoolRef,
    PersistResult, ProcessTermination, ResourceMode, ResultPersisted, ResumeDecision,
    RunFingerprint, SessionFinished, SessionLoaded, SessionResumeRef, SessionStarted, StoredResult,
    StoredResultLoaded, resume_policy,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ContendedOperation {
    Begin,
    Persist,
    Lookup,
    Finish,
    Load,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum HeldLock {
    BeginImmediate,
    ReadTransaction,
}

const CELLS: [(HeldLock, ContendedOperation); 10] = [
    (HeldLock::BeginImmediate, ContendedOperation::Begin),
    (HeldLock::BeginImmediate, ContendedOperation::Persist),
    (HeldLock::BeginImmediate, ContendedOperation::Lookup),
    (HeldLock::BeginImmediate, ContendedOperation::Finish),
    (HeldLock::BeginImmediate, ContendedOperation::Load),
    (HeldLock::ReadTransaction, ContendedOperation::Begin),
    (HeldLock::ReadTransaction, ContendedOperation::Persist),
    (HeldLock::ReadTransaction, ContendedOperation::Lookup),
    (HeldLock::ReadTransaction, ContendedOperation::Finish),
    (HeldLock::ReadTransaction, ContendedOperation::Load),
];

struct GoldenSessionRows {
    symbol: &'static str,
    output_token: &'static str,
    output_retained: i64,
    output_observed: i64,
    diagnostic_level: &'static str,
    diagnostic_code: &'static str,
    diagnostic_message: &'static str,
    termination: Option<(&'static str, i64)>,
}

fn all_optional_session_rows(schema_version: i64) -> GoldenSessionRows {
    GoldenSessionRows {
        symbol: "calculate",
        output_token: "spool",
        output_retained: 3,
        output_observed: 8,
        diagnostic_level: "warning",
        diagnostic_code: "fixture.warning",
        diagnostic_message: "fixture diagnostic",
        termination: (schema_version >= 3).then_some(("exit", 7)),
    }
}

#[test]
fn golden_session_schema_eras_migrate_without_semantic_loss() {
    let root = repo_root().join("crates/hoimin-cli/tests/golden/sessions");
    for version in 1..=4 {
        let source = root.join(format!("schema-v{version}.sqlite3"));
        let temp = tempfile::tempdir().unwrap();
        let migrated_path = temp.path().join("session.sqlite3");
        std::fs::copy(&source, &migrated_path).unwrap();
        let before =
            Connection::open_with_flags(&migrated_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(user_version(&before), version);
        assert_golden_session_rows(&before, version);
        drop(before);

        let mut handler = SessionHandler::open(&migrated_path).unwrap();
        let resumed = handler.load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(89),
            fingerprint: RunFingerprint::from_bytes([1; 32]),
        });
        if version < 4 {
            assert_eq!(
                resumed.unwrap_err().failure.code(),
                "session.resume.incompatible"
            );
        } else {
            assert_eq!(resumed.unwrap().resume.unwrap().run_id, "golden-run");
            let loaded = handler
                .lookup(&lookup_request(90, "golden-run", "golden-mutant"))
                .unwrap();
            assert_eq!(
                loaded.result,
                Some(StoredResult {
                    mutant_id: "golden-mutant".into(),
                    status: MutationStatus::Killed,
                })
            );
        }
        drop(handler);

        let migrated = Connection::open(&migrated_path).unwrap();
        assert_eq!(user_version(&migrated), 4);
        let budget: Option<Vec<u8>> = migrated
            .query_row("SELECT max_mutants FROM runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            budget,
            (version == 4).then(|| 100u64.to_be_bytes().to_vec())
        );
        assert_golden_session_rows(&migrated, version);
    }
}

#[test]
#[should_panic(expected = "golden session table cardinalities differ")]
fn golden_session_corpus_rejects_extra_legacy_rows() {
    let source = repo_root().join("crates/hoimin-cli/tests/golden/sessions/schema-v1.sqlite3");
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("session.sqlite3");
    std::fs::copy(source, &path).unwrap();
    let connection = Connection::open(path).unwrap();
    connection
        .execute(
            "INSERT INTO fingerprints(digest,schema_version) VALUES (?1,4)",
            [[2_u8; 32].as_slice()],
        )
        .unwrap();

    assert_golden_session_rows(&connection, 1);
}

#[test]
fn current_session_golden_matches_semantic_regeneration() {
    let checked_path =
        repo_root().join("crates/hoimin-cli/tests/golden/sessions/schema-v4.sqlite3");
    let temp = tempfile::tempdir().unwrap();
    let checked_copy = temp.path().join("checked.sqlite3");
    std::fs::copy(&checked_path, &checked_copy).unwrap();
    let regenerated_path = temp.path().join("session.sqlite3");
    let mut handler = SessionHandler::open(&regenerated_path).unwrap();
    handler.begin(begin_request(1, "golden-run")).unwrap();
    handler.persist(&golden_persist_request()).unwrap();
    drop(handler);

    let checked = Connection::open(checked_copy).unwrap();
    let regenerated = Connection::open(regenerated_path).unwrap();
    assert_eq!(user_version(&checked), 4);
    assert_eq!(user_version(&regenerated), 4);
    assert_eq!(schema_sql(&checked), schema_sql(&regenerated));
    let checked_rows = logical_rows(&checked);
    let regenerated_rows = logical_rows(&regenerated);
    assert_eq!(
        checked_rows[0].1[1],
        Value::Integer(i64::from(hoimin_core::FINGERPRINT_SCHEMA_VERSION))
    );
    assert_eq!(
        regenerated_rows[0].1[1],
        Value::Integer(i64::from(hoimin_core::FINGERPRINT_SCHEMA_VERSION))
    );
    assert_eq!(checked_rows, regenerated_rows);
}

fn assert_golden_session_rows(connection: &Connection, original_version: i64) {
    assert_eq!(
        table_counts(connection),
        [
            ("fingerprints".to_owned(), 1),
            ("runs".to_owned(), 1),
            ("candidates".to_owned(), 1),
            ("results".to_owned(), 1),
            ("diagnostics".to_owned(), 1),
        ],
        "golden session table cardinalities differ"
    );
    let run_counts = connection
        .query_row(
            "SELECT count(*),
                    sum(CASE WHEN finished=0 AND complete=0 THEN 1 ELSE 0 END)
             FROM runs",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .unwrap();
    assert_eq!(
        run_counts,
        (1, 1),
        "golden session must contain exactly one incomplete run"
    );
    assert_eq!(
        logical_rows(connection),
        expected_logical_rows(original_version),
        "golden session logical rows differ"
    );
}

fn golden_persist_request() -> PersistResult {
    let rows = all_optional_session_rows(3);
    let mut request = persist_request(2, "golden-run", "golden-mutant");
    request.result.candidate.symbol = Some(rows.symbol.to_owned());
    request.result.output = Some(OutputSpoolRef {
        token: rows.output_token.to_owned(),
        retained: u64::try_from(rows.output_retained).unwrap(),
        observed: u64::try_from(rows.output_observed).unwrap(),
    });
    let diagnostic = &mut request.result.diagnostics[0];
    rows.diagnostic_level.clone_into(&mut diagnostic.level);
    rows.diagnostic_code.clone_into(&mut diagnostic.code);
    rows.diagnostic_message.clone_into(&mut diagnostic.message);
    request.result.termination = rows.termination.map(|(kind, code)| match kind {
        "exit" => ProcessTermination::Exit(i32::try_from(code).unwrap()),
        _ => unreachable!("golden v3 pins an exit termination"),
    });
    request
}

fn user_version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap()
        .to_path_buf()
}

fn schema_sql(connection: &Connection) -> Vec<(String, String, String, String)> {
    connection
        .prepare(
            "SELECT type,name,tbl_name,sql FROM sqlite_master
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
             ORDER BY type,name",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn logical_rows(connection: &Connection) -> Vec<(String, Vec<Value>)> {
    let result_query = if user_version(connection) >= 3 {
        "SELECT run_id,mutant_id,status,elapsed_secs,elapsed_nanos,resource_mode,output_token,output_retained,output_observed,termination_kind,termination_exit_code FROM results ORDER BY run_id,mutant_id"
    } else {
        "SELECT run_id,mutant_id,status,elapsed_secs,elapsed_nanos,resource_mode,output_token,output_retained,output_observed FROM results ORDER BY run_id,mutant_id"
    };
    let queries = [
        (
            "fingerprints",
            "SELECT hex(digest),schema_version FROM fingerprints ORDER BY digest",
        ),
        (
            "runs",
            "SELECT run_id,hex(fingerprint),finished,complete FROM runs ORDER BY run_id",
        ),
        (
            "candidates",
            "SELECT run_id,mutant_id,sequence,path,span_start,span_length,original,replacement,operator,line,column_number,symbol,file_hash FROM candidates ORDER BY run_id,mutant_id",
        ),
        ("results", result_query),
        (
            "diagnostics",
            "SELECT run_id,mutant_id,level,code,message FROM diagnostics ORDER BY run_id,mutant_id,level,code,message",
        ),
    ];
    let mut result = Vec::new();
    for (table, sql) in queries {
        let mut statement = connection.prepare(sql).unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get::<_, Value>(column))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        for row in rows {
            let mut row = row.unwrap();
            if table == "results" && row.len() == 9 {
                row.extend([Value::Null, Value::Null]);
            }
            result.push((table.to_owned(), row));
        }
    }
    result
}

fn table_counts(connection: &Connection) -> [(String, i64); 5] {
    [
        "fingerprints",
        "runs",
        "candidates",
        "results",
        "diagnostics",
    ]
    .map(|table| {
        let count = connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        (table.to_owned(), count)
    })
}

fn expected_logical_rows(original_version: i64) -> Vec<(String, Vec<Value>)> {
    let rows = all_optional_session_rows(original_version);
    let fingerprint = "01".repeat(32);
    let (termination_kind, termination_code) = rows
        .termination
        .map_or((Value::Null, Value::Null), |(kind, code)| {
            (Value::Text(kind.to_owned()), Value::Integer(code))
        });
    vec![
        (
            "fingerprints".to_owned(),
            vec![
                Value::Text(fingerprint.clone()),
                Value::Integer(if original_version >= 4 {
                    i64::from(hoimin_core::FINGERPRINT_SCHEMA_VERSION)
                } else {
                    4
                }),
            ],
        ),
        (
            "runs".to_owned(),
            vec![
                Value::Text("golden-run".to_owned()),
                Value::Text(fingerprint),
                Value::Integer(0),
                Value::Integer(0),
            ],
        ),
        (
            "candidates".to_owned(),
            vec![
                Value::Text("golden-run".to_owned()),
                Value::Text("golden-mutant".to_owned()),
                Value::Integer(0),
                Value::Text("src/example.py".to_owned()),
                Value::Integer(0),
                Value::Integer(1),
                Value::Text("+".to_owned()),
                Value::Text("-".to_owned()),
                Value::Text("binary".to_owned()),
                Value::Integer(1),
                Value::Integer(0),
                Value::Text(rows.symbol.to_owned()),
                Value::Text("hash".to_owned()),
            ],
        ),
        (
            "results".to_owned(),
            vec![
                Value::Text("golden-run".to_owned()),
                Value::Text("golden-mutant".to_owned()),
                Value::Text("killed".to_owned()),
                Value::Integer(0),
                Value::Integer(5_000_000),
                Value::Text("hard".to_owned()),
                Value::Text(rows.output_token.to_owned()),
                Value::Integer(rows.output_retained),
                Value::Integer(rows.output_observed),
                termination_kind,
                termination_code,
            ],
        ),
        (
            "diagnostics".to_owned(),
            vec![
                Value::Text("golden-run".to_owned()),
                Value::Text("golden-mutant".to_owned()),
                Value::Text(rows.diagnostic_level.to_owned()),
                Value::Text(rows.diagnostic_code.to_owned()),
                Value::Text(rows.diagnostic_message.to_owned()),
            ],
        ),
    ]
}

#[derive(Debug)]
enum ContendedOutcome {
    Begin(Result<SessionStarted, EffectFailed>),
    Persist(Result<ResultPersisted, EffectFailed>),
    Lookup(Result<StoredResultLoaded, EffectFailed>),
    Finish(Result<SessionFinished, EffectFailed>),
    Load(Result<SessionLoaded, EffectFailed>),
}

struct Blocker {
    established: Receiver<Result<(), String>>,
    release: Sender<()>,
    released: Receiver<Result<(), String>>,
    thread: thread::JoinHandle<()>,
}

struct OperationTask {
    dispatched: Receiver<()>,
    outcome: Receiver<ContendedOutcome>,
    thread: thread::JoinHandle<()>,
}

#[test]
fn session_operations_complete_under_contention_matrix() {
    let mut visited = HashSet::new();
    let mut failures = Vec::new();

    for (index, cell) in CELLS.into_iter().enumerate() {
        visited.insert(cell);
        if let Err(error) = run_contention_cell(index, cell) {
            failures.push(format!("{cell:?}: {error}"));
        }
    }

    assert!(
        failures.is_empty(),
        "session contention matrix failures:\n{}",
        failures.join("\n")
    );
    assert_eq!(visited.len(), CELLS.len(), "not every matrix cell ran");
    assert_eq!(
        visited,
        CELLS.into_iter().collect(),
        "visited matrix cells differ from CELLS"
    );
}

fn run_contention_cell(
    index: usize,
    (lock, operation): (HeldLock, ContendedOperation),
) -> Result<(), String> {
    const READY_TIMEOUT: Duration = Duration::from_secs(1);
    const PRE_RELEASE_WINDOW: Duration = Duration::from_millis(250);
    const COMPLETION_TIMEOUT: Duration = Duration::from_secs(6);

    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).map_err(|error| error.to_string())?;
    let seed_id = 100 + index as u64 * 10;
    let operation_id = EffectId(seed_id + 9);
    let run_id = format!("contended-run-{index}");
    let mutant_id = format!("contended-mutant-{index}");

    seed_contention_cell(&mut handler, operation, seed_id, &run_id, &mutant_id)?;

    let blocker = await_blocker_establishment(spawn_blocker(path.clone(), lock))?;

    let OperationTask {
        dispatched: dispatched_rx,
        outcome: outcome_rx,
        thread: operation_thread,
    } = spawn_contended_operation(handler, operation, operation_id, &run_id, &mutant_id);

    let dispatch_started = Instant::now();
    if let Err(error) = dispatched_rx.recv_timeout(READY_TIMEOUT) {
        drop(dispatched_rx);
        let release = blocker.release.send(());
        let released = blocker.released.recv_timeout(READY_TIMEOUT);
        let operation_cleanup = join_finished_thread(operation_thread, "operation");
        let blocker_cleanup = join_finished_thread(blocker.thread, "blocker");
        return Err(format!(
            "operation dispatch signal failed: {error}; \
             release={release:?}; released={released:?}; \
             operation_cleanup={operation_cleanup:?}; blocker_cleanup={blocker_cleanup:?}"
        ));
    }

    let must_still_be_pending = lock == HeldLock::BeginImmediate
        && matches!(
            operation,
            ContendedOperation::Begin | ContendedOperation::Persist | ContendedOperation::Finish
        );
    let mut pre_release_failure = None;
    let mut outcome = match outcome_rx.recv_timeout(PRE_RELEASE_WINDOW) {
        Ok(outcome) if must_still_be_pending => {
            pre_release_failure = Some(format!(
                "operation was not pending before BEGIN IMMEDIATE release: {outcome:?}"
            ));
            Some(outcome)
        }
        Ok(outcome) => Some(outcome),
        Err(RecvTimeoutError::Timeout) => None,
        Err(RecvTimeoutError::Disconnected) => {
            pre_release_failure = Some("operation channel disconnected before release".to_owned());
            None
        }
    };

    let release_elapsed = dispatch_started.elapsed();
    let release_deadline_failure = (release_elapsed > READY_TIMEOUT).then(|| {
        format!("blocker was held {release_elapsed:?} after dispatch, exceeding 1 second")
    });
    let release_failure = blocker
        .release
        .send(())
        .err()
        .map(|error| format!("blocker release signal failed: {error}"));
    let release_observation_failure = match blocker.released.recv_timeout(READY_TIMEOUT) {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(error) => Some(format!("blocker released signal failed: {error}")),
    };

    let mut completion_failure = None;
    if outcome.is_none() {
        let remaining = COMPLETION_TIMEOUT.saturating_sub(dispatch_started.elapsed());
        match outcome_rx.recv_timeout(remaining) {
            Ok(received) => outcome = Some(received),
            Err(error) => {
                completion_failure = Some(format!(
                    "operation did not complete within 6 seconds: {error}"
                ));
                if let Ok(received) = outcome_rx.recv_timeout(READY_TIMEOUT) {
                    outcome = Some(received);
                }
            }
        }
    }

    let operation_join_failure = join_finished_thread(operation_thread, "operation").err();
    let blocker_join_failure = join_finished_thread(blocker.thread, "blocker").err();

    if let Some(error) = pre_release_failure
        .or(release_deadline_failure)
        .or(release_failure)
        .or(release_observation_failure)
        .or(completion_failure)
        .or(operation_join_failure)
        .or(blocker_join_failure)
    {
        return Err(error);
    }
    assert_exact_outcome(
        outcome.expect("outcome was populated above"),
        operation,
        operation_id,
        &run_id,
        &mutant_id,
    )
}

fn await_blocker_establishment(blocker: Blocker) -> Result<Blocker, String> {
    match blocker.established.recv_timeout(Duration::from_secs(1)) {
        Ok(Ok(())) => Ok(blocker),
        Ok(Err(error)) => cleanup_failed_blocker(blocker, error),
        Err(error) => cleanup_failed_blocker(
            blocker,
            format!("blocker establishment signal failed: {error}"),
        ),
    }
}

fn seed_contention_cell(
    handler: &mut SessionHandler,
    operation: ContendedOperation,
    seed_id: u64,
    run_id: &str,
    mutant_id: &str,
) -> Result<(), String> {
    match operation {
        ContendedOperation::Begin => Ok(()),
        ContendedOperation::Persist | ContendedOperation::Finish | ContendedOperation::Load => {
            handler
                .begin(begin_request(seed_id, run_id))
                .map(|_| ())
                .map_err(|error| format!("seed run failed: {error:?}"))
        }
        ContendedOperation::Lookup => {
            handler
                .begin(begin_request(seed_id, run_id))
                .map_err(|error| format!("seed run failed: {error:?}"))?;
            handler
                .persist(&persist_request(seed_id + 1, run_id, mutant_id))
                .map(|_| ())
                .map_err(|error| format!("seed result failed: {error:?}"))
        }
    }
}

fn spawn_contended_operation(
    mut handler: SessionHandler,
    operation: ContendedOperation,
    id: EffectId,
    run_id: &str,
    mutant_id: &str,
) -> OperationTask {
    let (dispatched_tx, dispatched) = mpsc::sync_channel(0);
    let (outcome_tx, outcome) = mpsc::sync_channel(1);
    let run_id = run_id.to_owned();
    let mutant_id = mutant_id.to_owned();
    let thread = thread::spawn(move || {
        if dispatched_tx.send(()).is_err() {
            return;
        }
        let actual = match operation {
            ContendedOperation::Begin => {
                ContendedOutcome::Begin(handler.begin(begin_request(id.0, &run_id)))
            }
            ContendedOperation::Persist => ContendedOutcome::Persist(
                handler.persist(&persist_request(id.0, &run_id, &mutant_id)),
            ),
            ContendedOperation::Lookup => {
                ContendedOutcome::Lookup(handler.lookup(&lookup_request(id.0, &run_id, &mutant_id)))
            }
            ContendedOperation::Finish => {
                ContendedOutcome::Finish(handler.finish(finish_request(id.0, &run_id)))
            }
            ContendedOperation::Load => ContendedOutcome::Load(handler.load(&LoadSession {
                max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
                id,
                fingerprint: RunFingerprint::from_bytes([1; 32]),
            })),
        };
        let _ = outcome_tx.send(actual);
    });
    OperationTask {
        dispatched,
        outcome,
        thread,
    }
}

fn cleanup_failed_blocker<T>(blocker: Blocker, error: String) -> Result<T, String> {
    drop(blocker.release);
    let released = blocker.released.recv_timeout(Duration::from_secs(3));
    match join_finished_thread(blocker.thread, "blocker") {
        Ok(()) => Err(error),
        Err(cleanup) => Err(format!("{error}; {cleanup}; released={released:?}")),
    }
}

fn join_finished_thread(thread: thread::JoinHandle<()>, name: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !thread.is_finished() && Instant::now() < deadline {
        thread::yield_now();
    }
    if !thread.is_finished() {
        return Err(format!("{name} thread remained live after bounded cleanup"));
    }
    match thread.join() {
        Ok(()) => Ok(()),
        Err(panic) => Err(format!("{name} panicked: {panic:?}")),
    }
}

fn spawn_blocker(path: PathBuf, lock: HeldLock) -> Blocker {
    let (established_tx, established) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::channel();
    let (released_tx, released) = mpsc::sync_channel(1);
    let thread = thread::spawn(move || {
        let connection = match Connection::open(path) {
            Ok(connection) => connection,
            Err(error) => {
                let _ = established_tx.send(Err(format!("open blocker failed: {error}")));
                return;
            }
        };
        let begin = match lock {
            HeldLock::BeginImmediate => connection.execute_batch("BEGIN IMMEDIATE"),
            HeldLock::ReadTransaction => {
                connection.execute_batch("BEGIN; SELECT count(*) FROM runs;")
            }
        };
        if let Err(error) = begin {
            let _ = established_tx.send(Err(format!("establish {lock:?} failed: {error}")));
            return;
        }
        if established_tx.send(Ok(())).is_err() {
            let _ = connection.execute_batch("ROLLBACK");
            return;
        }

        let release_result = match release_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(()) => connection
                .execute_batch("ROLLBACK")
                .map_err(|error| format!("release {lock:?} failed: {error}")),
            Err(error) => {
                let rollback = connection.execute_batch("ROLLBACK");
                Err(format!(
                    "release signal for {lock:?} failed: {error}; rollback={rollback:?}"
                ))
            }
        };
        let _ = released_tx.send(release_result);
    });

    Blocker {
        established,
        release,
        released,
        thread,
    }
}

fn assert_exact_outcome(
    actual: ContendedOutcome,
    operation: ContendedOperation,
    id: EffectId,
    run_id: &str,
    mutant_id: &str,
) -> Result<(), String> {
    let expected_mismatch = match (operation, actual) {
        (ContendedOperation::Begin, ContendedOutcome::Begin(Ok(actual))) => {
            let expected = SessionStarted {
                id,
                run_id: run_id.to_owned(),
            };
            (actual != expected).then(|| format!("expected {expected:?}, got {actual:?}"))
        }
        (ContendedOperation::Persist, ContendedOutcome::Persist(Ok(actual))) => {
            let expected = ResultPersisted {
                id,
                worker: 0,
                run_id: run_id.to_owned(),
                mutant_id: mutant_id.to_owned(),
            };
            (actual != expected).then(|| format!("expected {expected:?}, got {actual:?}"))
        }
        (ContendedOperation::Lookup, ContendedOutcome::Lookup(Ok(actual))) => {
            let expected = StoredResultLoaded {
                id,
                worker: 0,
                result: Some(StoredResult {
                    mutant_id: mutant_id.to_owned(),
                    status: MutationStatus::Killed,
                }),
            };
            (actual != expected).then(|| format!("expected {expected:?}, got {actual:?}"))
        }
        (ContendedOperation::Finish, ContendedOutcome::Finish(Ok(actual))) => {
            let expected = SessionFinished {
                id,
                run_id: run_id.to_owned(),
                complete: true,
            };
            (actual != expected).then(|| format!("expected {expected:?}, got {actual:?}"))
        }
        (ContendedOperation::Load, ContendedOutcome::Load(Ok(actual))) => {
            let expected = SessionLoaded {
                fresh_reason: None,
                id,
                resume: Some(SessionResumeRef {
                    run_id: run_id.to_owned(),
                }),
            };
            (actual != expected).then(|| format!("expected {expected:?}, got {actual:?}"))
        }
        (_, unexpected) => Some(format!("unexpected typed outcome: {unexpected:?}")),
    };
    match expected_mismatch {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[test]
fn migrates_schema_enables_wal_and_echoes_typed_completion_events() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    let fingerprint = RunFingerprint::from_bytes([3; 32]);

    let started = handler
        .begin(BeginSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(1),
            run_id: "run-1".to_owned(),
            fingerprint,
        })
        .unwrap();
    assert_eq!(started.id, EffectId(1));
    assert_eq!(started.run_id, "run-1");

    let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let stored_schema: i64 = connection
        .query_row("SELECT schema_version FROM fingerprints", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        stored_schema,
        i64::from(hoimin_core::FINGERPRINT_SCHEMA_VERSION)
    );

    let persisted = handler.persist(&persist_request(2, "run-1", "m1")).unwrap();
    assert_eq!(persisted.id, EffectId(2));
    assert_eq!(persisted.mutant_id, "m1");

    let loaded = handler
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
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
        4
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
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
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
                max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
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
    assert_eq!(failed.failure.code(), "session.persist.owner");
}

#[test]
fn persists_every_typed_process_termination_shape() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    handler.begin(begin_request(1, "run-1")).unwrap();
    let cases = [
        (Some(ProcessTermination::Exit(-7)), Some("exit"), Some(-7)),
        (Some(ProcessTermination::Timeout), Some("timeout"), None),
        (
            Some(ProcessTermination::OutOfMemory),
            Some("out_of_memory"),
            None,
        ),
        (
            Some(ProcessTermination::ProcessLimit),
            Some("process_limit"),
            None,
        ),
        (Some(ProcessTermination::Cancelled), Some("cancelled"), None),
        (None, None, None),
    ];

    for (index, (termination, expected_kind, expected_code)) in cases.into_iter().enumerate() {
        let mutant_id = format!("m{index}");
        let mut request = persist_request(index as u64 + 2, "run-1", &mutant_id);
        request.result.termination = termination;
        handler.persist(&request).unwrap();

        let connection = Connection::open(&path).unwrap();
        let stored = connection
            .query_row(
                "SELECT termination_kind,termination_exit_code FROM results
                 WHERE run_id='run-1' AND mutant_id=?1",
                [&mutant_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(stored, (expected_kind.map(str::to_owned), expected_code));
    }
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
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(3),
            run_id: "other".to_owned(),
            fingerprint: RunFingerprint::from_bytes([2; 32]),
        })
        .unwrap();
    handler.begin(begin_request(4, "complete")).unwrap();
    handler.finish(finish_request(5, "complete")).unwrap();

    let loaded = handler
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(6),
            fingerprint: wanted,
        })
        .unwrap();
    assert_eq!(loaded.id, EffectId(6));
    assert_eq!(loaded.resume.unwrap().run_id, "new");
    assert!(
        handler
            .load(&LoadSession {
                max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
                id: EffectId(7),
                fingerprint: RunFingerprint::from_bytes([9; 32]),
            })
            .unwrap()
            .resume
            .is_none()
    );
}

#[test]
fn explicit_resume_rejects_the_previous_fingerprint_schema() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    handler.begin(begin_request(1, "legacy")).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE fingerprints SET schema_version=6", [])
        .unwrap();
    drop(connection);

    let failed = handler
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(2),
            fingerprint: RunFingerprint::from_bytes([9; 32]),
        })
        .unwrap_err();

    assert_eq!(failed.id, EffectId(2));
    assert_eq!(failed.failure.code(), "session.resume.incompatible");
    assert!(failed.failure.message().contains("start a new session"));
}

#[test]
fn a_live_run_cannot_be_resumed_by_another_handler() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut owner = SessionHandler::open(&path).unwrap();
    let mut contender = SessionHandler::open(&path).unwrap();
    let fingerprint = RunFingerprint::from_bytes([1; 32]);
    owner.begin(begin_request(1, "owned")).unwrap();

    let failed = contender
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(2),
            fingerprint,
        })
        .unwrap_err();

    assert_eq!(failed.id, EffectId(2));
    assert_eq!(failed.failure.code(), "session.resume.active");
}

#[test]
fn non_owner_finish_is_rejected_without_changing_run_state() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut owner = SessionHandler::open(&path).unwrap();
    let mut contender = SessionHandler::open(&path).unwrap();
    let fingerprint = RunFingerprint::from_bytes([1; 32]);
    owner.begin(begin_request(1, "owned")).unwrap();

    let failed = contender
        .finish(FinishSession {
            id: EffectId(2),
            run_id: "owned".to_owned(),
            complete: false,
        })
        .unwrap_err();

    assert_eq!(failed.id, EffectId(2));
    assert_eq!(failed.failure.code(), "session.finish.owner");
    let still_active = contender
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(3),
            fingerprint,
        })
        .unwrap_err();
    assert_eq!(still_active.failure.code(), "session.resume.active");

    drop(owner);
    let loaded = contender
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(4),
            fingerprint,
        })
        .unwrap();
    assert_eq!(loaded.resume.unwrap().run_id, "owned");
}

#[test]
fn non_owner_persist_cannot_insert_or_replace_results_in_another_run() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut owner = SessionHandler::open(&path).unwrap();
    let mut contender = SessionHandler::open(&path).unwrap();
    owner.begin(begin_request(1, "owned")).unwrap();
    owner
        .persist(&persist_with_status(
            2,
            "owned",
            "retry",
            MutationStatus::Timeout,
        ))
        .unwrap();
    contender.begin(begin_request(3, "other-run")).unwrap();
    let observer = Connection::open(&path).unwrap();
    // Unauthorized calls must fail before attempting a write transaction, even if SQLite's
    // writer slot is occupied. The owner's file lock is independent of this database lock.
    observer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let before = logical_rows(&observer);

    for (id, mutant_id) in [(4, "new"), (5, "retry")] {
        let failed = contender
            .persist(&persist_request(id, "owned", mutant_id))
            .unwrap_err();
        assert_eq!(failed.id, EffectId(id));
        assert_eq!(failed.failure.code(), "session.persist.owner");
        assert_eq!(logical_rows(&observer), before);
    }
    observer.execute_batch("COMMIT").unwrap();
    owner
        .persist(&persist_request(6, "owned", "retry"))
        .unwrap();
}

#[test]
fn non_owner_lookup_requires_successful_load_for_the_requested_run() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut owner = SessionHandler::open(&path).unwrap();
    let mut contender = SessionHandler::open(&path).unwrap();
    owner.begin(begin_request(1, "owned")).unwrap();
    owner.persist(&persist_request(2, "owned", "m1")).unwrap();
    let mut other = begin_request(3, "other-run");
    other.fingerprint = RunFingerprint::from_bytes([2; 32]);
    contender.begin(other).unwrap();
    let load = LoadSession {
        max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
        id: EffectId(4),
        fingerprint: RunFingerprint::from_bytes([1; 32]),
    };
    assert_eq!(
        contender.load(&load).unwrap_err().failure.code(),
        "session.resume.active"
    );

    let failed = contender
        .lookup(&lookup_request(5, "owned", "m1"))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(5));
    assert_eq!(failed.failure.code(), "session.lookup.owner");
    let failed = contender
        .persist(&persist_request(6, "owned", "new"))
        .unwrap_err();
    assert_eq!(failed.failure.code(), "session.persist.owner");

    drop(owner);
    assert_eq!(
        contender.load(&load).unwrap().resume.unwrap().run_id,
        "owned"
    );
    let stored = contender.lookup(&lookup_request(7, "owned", "m1")).unwrap();
    assert_eq!(stored.result.unwrap().status, MutationStatus::Killed);
    contender
        .persist(&persist_request(8, "owned", "new"))
        .unwrap();
}

#[test]
fn former_owner_result_operations_require_reacquisition_after_finish() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut former = SessionHandler::open(&path).unwrap();
    let mut resumer = SessionHandler::open(&path).unwrap();
    former.begin(begin_request(1, "partial")).unwrap();
    former
        .finish(FinishSession {
            id: EffectId(2),
            run_id: "partial".to_owned(),
            complete: false,
        })
        .unwrap();
    let load = LoadSession {
        max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
        id: EffectId(3),
        fingerprint: RunFingerprint::from_bytes([1; 32]),
    };
    // Finishing releases authority even before another handler acquires it.
    assert_eq!(
        former
            .persist(&persist_request(4, "partial", "m1"))
            .unwrap_err()
            .failure
            .code(),
        "session.persist.owner"
    );
    assert_eq!(
        former
            .lookup(&lookup_request(5, "partial", "m1"))
            .unwrap_err()
            .failure
            .code(),
        "session.lookup.owner"
    );
    resumer.load(&load).unwrap().resume.unwrap();
    let observer = Connection::open(&path).unwrap();
    let before = logical_rows(&observer);
    assert_eq!(
        former
            .persist(&persist_request(6, "partial", "m1"))
            .unwrap_err()
            .failure
            .code(),
        "session.persist.owner"
    );
    assert_eq!(
        former
            .lookup(&lookup_request(7, "partial", "m1"))
            .unwrap_err()
            .failure
            .code(),
        "session.lookup.owner"
    );
    assert_eq!(logical_rows(&observer), before);

    resumer
        .persist(&persist_request(8, "partial", "m1"))
        .unwrap();
    resumer
        .finish(FinishSession {
            id: EffectId(9),
            run_id: "partial".to_owned(),
            complete: false,
        })
        .unwrap();
    former.load(&load).unwrap().resume.unwrap();
    assert_eq!(
        former
            .lookup(&lookup_request(10, "partial", "m1"))
            .unwrap()
            .result
            .unwrap()
            .status,
        MutationStatus::Killed
    );
    former
        .persist(&persist_request(11, "partial", "m2"))
        .unwrap();
}

#[test]
fn incomplete_finish_releases_run_ownership_for_resume() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut owner = SessionHandler::open(&path).unwrap();
    let mut resumer = SessionHandler::open(&path).unwrap();
    let fingerprint = RunFingerprint::from_bytes([1; 32]);
    owner.begin(begin_request(1, "partial")).unwrap();

    owner
        .finish(FinishSession {
            id: EffectId(2),
            run_id: "partial".to_owned(),
            complete: false,
        })
        .unwrap();

    let loaded = resumer
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(3),
            fingerprint,
        })
        .unwrap();
    assert_eq!(loaded.resume.unwrap().run_id, "partial");
}

#[test]
fn dropping_handler_releases_run_ownership_for_resume() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let fingerprint = RunFingerprint::from_bytes([1; 32]);
    {
        let mut owner = SessionHandler::open(&path).unwrap();
        owner.begin(begin_request(1, "abandoned")).unwrap();
    }

    let mut resumer = SessionHandler::open(&path).unwrap();
    let loaded = resumer
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(2),
            fingerprint,
        })
        .unwrap();
    assert_eq!(loaded.resume.unwrap().run_id, "abandoned");
}

#[test]
fn completed_finish_releases_ownership_and_removes_run_from_resume() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut owner = SessionHandler::open(&path).unwrap();
    let mut observer = SessionHandler::open(&path).unwrap();
    let fingerprint = RunFingerprint::from_bytes([1; 32]);
    owner.begin(begin_request(1, "complete")).unwrap();

    owner.finish(finish_request(2, "complete")).unwrap();

    assert!(
        observer
            .load(&LoadSession {
                max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
                id: EffectId(3),
                fingerprint,
            })
            .unwrap()
            .resume
            .is_none()
    );
}

#[test]
fn different_runs_can_be_owned_concurrently() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut first = SessionHandler::open(&path).unwrap();
    let mut second = SessionHandler::open(&path).unwrap();

    first.begin(begin_request(1, "first")).unwrap();
    second
        .begin(BeginSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(2),
            run_id: "second".to_owned(),
            fingerprint: RunFingerprint::from_bytes([2; 32]),
        })
        .unwrap();

    let mut contender = SessionHandler::open(&path).unwrap();
    for (id, fingerprint) in [(3, [1; 32]), (4, [2; 32])] {
        let failed = contender
            .load(&LoadSession {
                max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
                id: EffectId(id),
                fingerprint: RunFingerprint::from_bytes(fingerprint),
            })
            .unwrap_err();
        assert_eq!(failed.failure.code(), "session.resume.active");
    }
}

#[test]
fn process_death_releases_run_ownership_for_immediate_resume() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let ready = temp.path().join("ready");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "ownership_child_fixture_holds_run"])
        .env("HOIMIN_OWNERSHIP_DATABASE", &path)
        .env("HOIMIN_OWNERSHIP_READY", &ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() && Instant::now() < deadline {
        assert!(
            child.try_wait().unwrap().is_none(),
            "ownership child exited"
        );
        thread::sleep(Duration::from_millis(10));
    }
    if !ready.exists() {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("ownership child did not become ready");
    }

    let mut contender = SessionHandler::open(&path).unwrap();
    let contention = contender.load(&LoadSession {
        max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
        id: EffectId(2),
        fingerprint: RunFingerprint::from_bytes([1; 32]),
    });
    child.kill().unwrap();
    child.wait().unwrap();

    assert_eq!(
        contention.unwrap_err().failure.code(),
        "session.resume.active"
    );
    let resumed = contender
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(3),
            fingerprint: RunFingerprint::from_bytes([1; 32]),
        })
        .unwrap();
    assert_eq!(resumed.resume.unwrap().run_id, "crashed");
}

#[test]
#[ignore = "subprocess fixture for session ownership crash test"]
fn ownership_child_fixture_holds_run() {
    let path = std::env::var_os("HOIMIN_OWNERSHIP_DATABASE").unwrap();
    let ready = std::env::var_os("HOIMIN_OWNERSHIP_READY").unwrap();
    let mut handler = SessionHandler::open(path).unwrap();
    handler.begin(begin_request(1, "crashed")).unwrap();
    std::fs::write(ready, b"ready").unwrap();
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

#[test]
fn successful_finish_releases_the_handlers_finish_authority() {
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
    assert_eq!(
        handler
            .finish(FinishSession {
                id: EffectId(3),
                run_id: "partial".to_owned(),
                complete: false,
            })
            .unwrap_err()
            .failure
            .code(),
        "session.finish.owner"
    );
    assert_eq!(
        handler
            .finish(finish_request(4, "partial"))
            .unwrap_err()
            .failure
            .code(),
        "session.finish.owner"
    );

    handler.begin(begin_request(5, "direct")).unwrap();
    handler.finish(finish_request(6, "direct")).unwrap();
    assert_eq!(
        handler
            .finish(finish_request(7, "direct"))
            .unwrap_err()
            .failure
            .code(),
        "session.finish.owner"
    );
}

#[test]
fn each_mutant_transaction_rolls_back_when_commit_fails() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite3");
    let mut handler = SessionHandler::open(&path).unwrap();
    handler.begin(begin_request(1, "missing-run")).unwrap();
    // Keep ownership while removing the referenced row to exercise the deferred foreign-key
    // failure at commit, rather than the earlier ownership guard.
    Connection::open(&path)
        .unwrap()
        .execute("DELETE FROM runs", [])
        .unwrap();

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
    // Corrupt the stored state without releasing this handler's ownership so the database
    // completion checks remain covered after adding the ownership guard.
    Connection::open(&complete_path)
        .unwrap()
        .execute("UPDATE runs SET complete=1", [])
        .unwrap();
    let failed = handler
        .persist(&persist_request(12, "run-complete", "m1"))
        .unwrap_err();
    assert_eq!(failed.failure.code(), "session.persist.complete");
    let failed = handler
        .lookup(&lookup_request(13, "run-complete", "m1"))
        .unwrap_err();
    assert_eq!(failed.id, EffectId(13));
    assert_eq!(failed.failure.code(), "session.lookup.complete");

    let null_path = temp.path().join("null.sqlite3");
    create_nullable_corrupt_database(&null_path);
    let mut handler = SessionHandler::open(&null_path).unwrap();
    let resumed = handler
        .load(&LoadSession {
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
            id: EffectId(13),
            fingerprint: RunFingerprint::from_bytes([0; 32]),
        })
        .unwrap();
    assert_eq!(resumed.resume.unwrap().run_id, "run-null");
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
        max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
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
            termination: None,
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
        "PRAGMA user_version=4;
         CREATE TABLE runs (id INTEGER PRIMARY KEY, run_id TEXT, fingerprint BLOB, complete INTEGER, max_mutants BLOB);
         CREATE TABLE results (
             run_id TEXT, mutant_id TEXT, status TEXT,
             termination_kind TEXT, termination_exit_code INTEGER
         );
         CREATE TABLE fingerprints (digest BLOB PRIMARY KEY, schema_version INTEGER);
         CREATE TABLE candidates (run_id TEXT, mutant_id TEXT);
         CREATE TABLE diagnostics (id INTEGER PRIMARY KEY, run_id TEXT, level TEXT, code TEXT, message TEXT);
         INSERT INTO runs(id, run_id, fingerprint, complete, max_mutants) VALUES (1, 'run-null', zeroblob(32), 0, x'0000000000000064');
         INSERT INTO results(run_id, mutant_id, status) VALUES ('run-null', 'm-null', NULL);",
    )
    .unwrap();
}

#[test]
fn session_artifacts_resolve_without_creating_the_database() {
    use hoimin_cli::session::SessionArtifacts;
    let directory = tempfile::tempdir().unwrap();
    for name in ["session.sqlite3", "active[1].db", "!active.db"] {
        let path = directory.path().join(name);
        let artifacts = SessionArtifacts::resolve(&path).unwrap();
        assert!(!path.exists());
        let canonical_parent = std::fs::canonicalize(directory.path()).unwrap();
        assert_eq!(artifacts.database(), canonical_parent.join(name));
        assert_eq!(
            artifacts.files(),
            [
                canonical_parent.join(name),
                canonical_parent.join(format!("{name}-wal")),
                canonical_parent.join(format!("{name}-shm")),
                canonical_parent.join(format!("{name}-journal"))
            ]
        );
        assert_eq!(
            artifacts.lock_directory(),
            canonical_parent.join(format!(".{name}.hoimin-locks"))
        );
        drop(SessionHandler::open(&path).unwrap());
        assert_eq!(
            SessionArtifacts::resolve(&path).unwrap().database(),
            artifacts.database()
        );
    }
    assert!(SessionArtifacts::resolve(directory.path()).is_err());
    assert!(SessionArtifacts::resolve(directory.path().join("missing/db")).is_err());
    assert!(!directory.path().join("missing").exists());
}

#[cfg(unix)]
#[test]
fn session_artifacts_resolve_aliases_and_reject_dangling_leaf_without_side_effects() {
    use hoimin_cli::session::SessionArtifacts;
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("actual");
    std::fs::create_dir(&parent).unwrap();
    let alias = directory.path().join("alias");
    symlink(&parent, &alias).unwrap();
    let path = parent.join("active*?[1].db");
    let artifacts = SessionArtifacts::resolve(alias.join("active*?[1].db")).unwrap();
    assert!(!path.exists());
    drop(SessionHandler::open(&path).unwrap());
    let leaf_alias = directory.path().join("leaf.db");
    symlink(&path, &leaf_alias).unwrap();
    assert_eq!(
        SessionArtifacts::resolve(&leaf_alias).unwrap().database(),
        artifacts.database()
    );
    let missing = parent.join("absent.db");
    let dangling = directory.path().join("dangling.db");
    symlink(&missing, &dangling).unwrap();
    assert!(SessionArtifacts::resolve(&dangling).is_err());
    assert!(SessionHandler::open(&dangling).is_err());
    assert!(!missing.exists());
}

#[test]
fn session_lock_trees_do_not_create_artifacts_and_deduplicate_existing_directory() {
    use hoimin_cli::session::SessionArtifacts;
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let database = root.join("session.db");
    let locks = root.join(".session.db.hoimin-locks");
    let artifacts = SessionArtifacts::resolve(&database).unwrap();
    assert_eq!(artifacts.lock_trees().unwrap(), vec![locks.clone()]);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::create_dir(&locks).unwrap();
    std::fs::write(locks.join("existing.lock"), b"owned").unwrap();
    assert_eq!(artifacts.lock_trees().unwrap(), vec![locks.clone()]);
    assert_eq!(
        std::fs::read(locks.join("existing.lock")).unwrap(),
        b"owned"
    );
    assert!(!database.exists());
}

#[cfg(unix)]
#[test]
fn session_lock_trees_retain_alias_and_actual_identity_without_creating_missing_targets() {
    use hoimin_cli::session::SessionArtifacts;
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let database = root.join("session.db");
    let actual = root.join("actual");
    let alias = root.join(".session.db.hoimin-locks");
    std::os::unix::fs::symlink(&actual, &alias).unwrap();
    let artifacts = SessionArtifacts::resolve(&database).unwrap();
    assert_eq!(artifacts.lock_trees().unwrap(), vec![alias.clone()]);
    assert!(!actual.exists());
    std::fs::create_dir(&actual).unwrap();
    assert_eq!(
        artifacts.lock_trees().unwrap(),
        vec![alias.clone(), actual.clone()]
    );
    assert_eq!(std::fs::canonicalize(alias).unwrap(), actual);
    assert!(!database.exists());
    assert_eq!(std::fs::read_dir(actual).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn session_lock_trees_propagate_existing_alias_resolution_failure() {
    use hoimin_cli::session::SessionArtifacts;
    let directory = tempfile::tempdir().unwrap();
    let alias = directory.path().join(".session.db.hoimin-locks");
    std::os::unix::fs::symlink(&alias, &alias).unwrap();
    let native_error = std::fs::canonicalize(&alias).unwrap_err();
    assert_ne!(native_error.kind(), std::io::ErrorKind::NotFound);
    let artifacts = SessionArtifacts::resolve(directory.path().join("session.db")).unwrap();
    let error = artifacts.lock_trees().unwrap_err();
    assert_eq!(error.raw_os_error(), native_error.raw_os_error());
    assert!(!directory.path().join("session.db").exists());
}

#[test]
fn resume_budget_is_monotone_and_selects_latest_eligible_incomplete_run() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("budgets.sqlite");
    let mut handler = SessionHandler::open(&path).unwrap();
    for (name, budget) in [("older", 1), ("newer", 3)] {
        let mut request = begin_request(1, name);
        request.max_mutants = std::num::NonZeroUsize::new(budget).unwrap();
        handler.begin(request).unwrap();
    }
    let load = |budget| LoadSession {
        id: EffectId(2),
        fingerprint: RunFingerprint::from_bytes([1; 32]),
        max_mutants: std::num::NonZeroUsize::new(budget).unwrap(),
    };
    let initial = handler.load(&load(2)).unwrap();
    assert_eq!(initial.resume.unwrap().run_id, "older");
    assert!(handler.load(&load(1)).unwrap().resume.is_none());
    assert_eq!(
        handler.load(&load(2)).unwrap().resume.unwrap().run_id,
        "older"
    );
    assert_eq!(
        handler.load(&load(3)).unwrap().resume.unwrap().run_id,
        "newer"
    );
    let connection = Connection::open(&path).unwrap();
    let budget: Vec<u8> = connection
        .query_row(
            "SELECT max_mutants FROM runs WHERE run_id='older'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(budget, 2_u64.to_be_bytes());
    handler
        .finish(FinishSession {
            id: EffectId(3),
            run_id: "newer".into(),
            complete: true,
        })
        .unwrap();
    assert_eq!(
        handler.load(&load(4)).unwrap().resume.unwrap().run_id,
        "older"
    );
}

#[test]
fn persisted_budget_preserves_unsigned_range_and_rejects_malformed_storage() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("budget-range.sqlite");
    let mut handler = SessionHandler::open(&path).unwrap();
    let mut begin = begin_request(1, "wide");
    begin.max_mutants = std::num::NonZeroUsize::new(usize::MAX / 2 + 1).unwrap();
    handler.begin(begin).unwrap();
    let request = LoadSession {
        id: EffectId(2),
        fingerprint: RunFingerprint::from_bytes([1; 32]),
        max_mutants: std::num::NonZeroUsize::new(usize::MAX).unwrap(),
    };
    assert_eq!(
        handler.load(&request).unwrap().resume.unwrap().run_id,
        "wide"
    );
    let connection = Connection::open(&path).unwrap();
    let budget: Vec<u8> = connection
        .query_row("SELECT max_mutants FROM runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(budget, (usize::MAX as u64).to_be_bytes());
    for malformed in ["zeroblob(8)", "zeroblob(7)", "1", "'100'"] {
        assert!(
            connection
                .execute(&format!("UPDATE runs SET max_mutants={malformed}"), [])
                .is_err()
        );
    }
    connection
        .execute("UPDATE runs SET max_mutants=NULL", [])
        .unwrap();
    let failure = handler.load(&request).unwrap_err();
    assert_eq!(failure.failure.code(), "session.resume.incompatible");
}

#[test]
fn corrupt_persisted_budget_is_rejected_without_repair() {
    for malformed in [
        Value::Blob(vec![]),
        Value::Blob(vec![0]),
        Value::Blob(vec![0; 8]),
        Value::Blob(vec![0; 9]),
        Value::Blob(vec![255]),
        Value::Blob(vec![255; 9]),
        Value::Integer(1),
        Value::Text("100".into()),
        Value::Real(1.5),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("corrupt-budget.sqlite");
        let mut handler = SessionHandler::open(&path).unwrap();
        handler.begin(begin_request(1, "corrupt-budget")).unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA ignore_check_constraints=ON")
            .unwrap();
        connection
            .execute("UPDATE runs SET max_mutants=?1", [&malformed])
            .unwrap();
        let request = LoadSession {
            id: EffectId(2),
            fingerprint: RunFingerprint::from_bytes([1; 32]),
            max_mutants: std::num::NonZeroUsize::new(100).unwrap(),
        };
        assert_eq!(
            handler.load(&request).unwrap_err().failure.code(),
            "session.corrupt"
        );
        let after: Value = connection
            .query_row("SELECT max_mutants FROM runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after, malformed);
    }
}

#[test]
fn lean_resume_budget_cases_match_persistent_selection_and_reuse() {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(clippy::struct_excessive_bools)] // Exact independent Lean wire contract.
    struct Case {
        schema: u8,
        id: String,
        mode: String,
        old_budget: usize,
        new_budget: usize,
        complete: bool,
        compatible: bool,
        status: String,
        eligible: bool,
        reuse: bool,
    }
    let cases = include_str!("../../../formal/HoiminOracle/corpus/resume-budget.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<Case>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 108);
    let mut ids = HashSet::new();
    for case in cases {
        assert_eq!(case.schema, 1);
        assert_eq!(
            case.id,
            format!(
                "{}-{}-{}-{}-{}",
                case.old_budget, case.new_budget, case.complete, case.compatible, case.status
            )
        );
        assert!(ids.insert(case.id.clone()));
        assert_eq!(case.mode, "strict");
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("oracle.sqlite");
        let mut handler = SessionHandler::open(&path).unwrap();
        let mut begin = begin_request(1, "oracle");
        begin.max_mutants = std::num::NonZeroUsize::new(case.old_budget).unwrap();
        handler.begin(begin).unwrap();
        let status = match case.status.as_str() {
            "killed" => MutationStatus::Killed,
            "survived" => MutationStatus::Survived,
            "timeout" => MutationStatus::Timeout,
            other => panic!("unknown status {other}"),
        };
        handler
            .persist(&persist_with_status(2, "oracle", "mutant", status))
            .unwrap();
        handler
            .finish(FinishSession {
                id: EffectId(3),
                run_id: "oracle".into(),
                complete: case.complete,
            })
            .unwrap();
        drop(handler);
        let mut handler = SessionHandler::open(&path).unwrap();
        let resumed = handler
            .load(&LoadSession {
                id: EffectId(4),
                fingerprint: RunFingerprint::from_bytes([if case.compatible { 1 } else { 2 }; 32]),
                max_mutants: std::num::NonZeroUsize::new(case.new_budget).unwrap(),
            })
            .unwrap()
            .resume;
        assert_eq!(resumed.is_some(), case.eligible);
        let stored = resumed.map(|reference| {
            assert_eq!(reference.run_id, "oracle");
            handler
                .lookup(&lookup_request(5, &reference.run_id, "mutant"))
                .unwrap()
                .result
                .unwrap()
        });
        assert_eq!(
            resume_policy(stored.as_ref()) == ResumeDecision::Reuse,
            case.reuse
        );
        let db = Connection::open(&path).unwrap();
        let budget: Vec<u8> = db
            .query_row("SELECT max_mutants FROM runs", [], |row| row.get(0))
            .unwrap();
        let expected = if case.eligible {
            case.new_budget
        } else {
            case.old_budget
        };
        assert_eq!(budget, (expected as u64).to_be_bytes());
    }
}
