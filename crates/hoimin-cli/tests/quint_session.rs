//! Real SQLite replay: the reference rules never read `SessionHandler` internals.
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::time::Duration;

use hoimin_cli::session::SessionHandler;
use hoimin_core::{
    BeginSession, ByteSpan, EffectFailed, EffectFailure, EffectId, FinishSession, LoadSession,
    LookupStoredResult, MutantResult, MutationCandidate, MutationOutcome, MutationStatus,
    OutputSpoolRef, PersistResult, ProcessOutputState, ProcessTermination, ResourceMode,
    ResumeDecision, ResumeFreshReason, ReusedMutationStatus, RunFingerprint, SessionDiagnostic,
    SessionLoadOutcome, resume_policy,
};
use proptest::prelude::*;
use rusqlite::Connection;
use serde_json::Value;

#[path = "../../hoimin-core/tests/support/quint.rs"]
mod quint;

#[path = "quint_session/ownership.rs"]
mod ownership;

fn fingerprint(value: u8) -> RunFingerprint {
    RunFingerprint::from_bytes([value; 32])
}
fn begin(run: usize, fp: u8, budget: usize) -> BeginSession {
    BeginSession {
        id: EffectId(1),
        run_id: format!("run-{run}"),
        fingerprint: fingerprint(fp),
        max_mutants: NonZeroUsize::new(budget).unwrap(),
    }
}
fn load(fp: u8, budget: usize) -> LoadSession {
    LoadSession {
        id: EffectId(2),
        fingerprint: fingerprint(fp),
        max_mutants: NonZeroUsize::new(budget).unwrap(),
    }
}
fn finish(run: usize, complete: bool) -> FinishSession {
    FinishSession {
        id: EffectId(3),
        run_id: format!("run-{run}"),
        complete,
    }
}
fn lookup(slot: u8) -> LookupStoredResult {
    LookupStoredResult {
        id: EffectId(4),
        worker: 0,
        run_id: "run-0".into(),
        mutant_id: format!("mutant-{slot}"),
    }
}
fn outcome(status: u8) -> MutationOutcome {
    match status {
        1 => MutationOutcome::Reused {
            status: ReusedMutationStatus::Killed,
        },
        2 => MutationOutcome::Reused {
            status: ReusedMutationStatus::Survived,
        },
        7 => MutationOutcome::NotRun,
        3..=6 => MutationOutcome::Executed {
            termination: match status {
                3 => ProcessTermination::Timeout,
                4 => ProcessTermination::OutOfMemory,
                5 => ProcessTermination::ProcessLimit,
                6 => ProcessTermination::Exit(1),
                _ => unreachable!(),
            },
            output_state: if status == 6 {
                ProcessOutputState::CloseTimedOut
            } else {
                ProcessOutputState::Complete
            },
            output: OutputSpoolRef {
                token: "spool".into(),
                retained: 3,
                observed: 8,
            },
            elapsed: Duration::from_millis(5),
        },
        _ => panic!("invalid status {status}"),
    }
}
fn status_number(status: MutationStatus) -> u8 {
    match status {
        MutationStatus::Killed => 1,
        MutationStatus::Survived => 2,
        MutationStatus::Timeout => 3,
        MutationStatus::OutOfMemory => 4,
        MutationStatus::ProcessLimit => 5,
        MutationStatus::Error => 6,
        MutationStatus::NotRun => 7,
    }
}
fn persist(slot: u8, status: u8) -> PersistResult {
    let mutant_id = format!("mutant-{slot}");
    PersistResult {
        id: EffectId(5),
        worker: 0,
        result: MutantResult {
            run_id: "run-0".into(),
            candidate: MutationCandidate {
                id: mutant_id.clone(),
                sequence: u64::from(slot),
                path: "src/example.py".into(),
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
            outcome: outcome(status),
            resource_mode: ResourceMode::Hard,
            diagnostics: vec![SessionDiagnostic {
                mutant_id,
                level: "warning".into(),
                code: "fixture".into(),
                message: format!("status-{status}"),
            }],
        },
    }
}
fn accepted<T>(result: Result<T, EffectFailed>, allowed: &[&str]) -> bool {
    match result {
        Ok(_) => true,
        Err(failure) => {
            let EffectFailure::SessionDatabase {
                code,
                operation,
                message,
            } = &failure.failure
            else {
                panic!("unexpected failure {failure:?}")
            };
            assert!(
                allowed.contains(&code.as_str()),
                "unexpected session failure {failure:?}"
            );
            // begin uses one code for constraint and I/O failures; only the former is semantic.
            if code == "session.begin" {
                assert_eq!(
                    operation, "insert run",
                    "unexpected begin failure {failure:?}"
                );
                assert_eq!(
                    message, "UNIQUE constraint failed: runs.run_id",
                    "unexpected begin failure {failure:?}"
                );
            }
            false
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Row {
    fp: u8,
    budget: usize,
    complete: bool,
}
/// Owns all database and schema knowledge shared by the two contract drivers.
struct Database {
    handler: Option<SessionHandler>,
    directory: tempfile::TempDir,
}
impl Database {
    fn new() -> Self {
        let mut database = Self {
            handler: None,
            directory: tempfile::tempdir().unwrap(),
        };
        database.handler = Some(database.open_handler());
        database
    }
    fn handler(&mut self) -> &mut SessionHandler {
        self.handler.as_mut().unwrap()
    }
    fn open_handler(&self) -> SessionHandler {
        SessionHandler::open(self.directory.path().join("session.sqlite3")).unwrap()
    }
    fn reopen(&mut self) {
        drop(self.handler.take());
        self.handler = Some(self.open_handler());
    }
    fn connection(&self) -> Connection {
        Connection::open_with_flags(
            self.directory.path().join("session.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap()
    }
    fn rows(&self) -> Vec<Row> {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT fingerprint,max_mutants,complete FROM runs ORDER BY id")
            .unwrap();
        statement
            .query_map([], |row| {
                let fingerprint: Vec<u8> = row.get(0)?;
                let budget: Vec<u8> = row.get(1)?;
                Ok(Row {
                    fp: fingerprint[0],
                    budget: usize::try_from(u64::from_be_bytes(budget.try_into().unwrap()))
                        .unwrap(),
                    complete: row.get(2)?,
                })
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }
    fn results(&self) -> [u8; 2] {
        let connection = self.connection();
        let mut statement = connection
            .prepare("SELECT mutant_id,outcome_json FROM results ORDER BY mutant_id")
            .unwrap();
        let mut results = [0; 2];
        for row in statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
        {
            let (id, json) = row.unwrap();
            let slot = id
                .strip_prefix("mutant-")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let outcome: MutationOutcome = serde_json::from_str(&json).unwrap();
            results[slot] = status_number(outcome.status());
        }
        let stored = i64::try_from(results.iter().filter(|&&s| s != 0).count()).unwrap();
        for table in ["candidates", "diagnostics"] {
            let count: i64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, stored, "one candidate and diagnostic per result");
        }
        results
    }
    /// Logical snapshot also detects payload changes during a rejected transaction.
    fn snapshot(&self) -> BTreeMap<&'static str, Vec<Vec<rusqlite::types::Value>>> {
        let connection = self.connection();
        [
            "fingerprints",
            "runs",
            "candidates",
            "results",
            "diagnostics",
        ]
        .into_iter()
        .map(|table| {
            let mut statement = connection
                .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                .unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|i| row.get(i))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (table, rows)
        })
        .collect()
    }
}

#[derive(Clone, Copy, Debug)]
enum Command {
    Begin,
    Close,
    Load,
    Finish(bool),
    Persist(u8, u8),
    Lookup(u8),
}
#[derive(Debug, Default)]
struct Reference {
    present: bool,
    complete: bool,
    owned: bool,
    results: [u8; 2],
}
impl Reference {
    fn apply(&mut self, command: Command) -> (bool, u8) {
        match command {
            Command::Begin if !self.present => {
                self.present = true;
                self.owned = true;
                (true, 0)
            }
            Command::Close => {
                self.owned = false;
                (true, 0)
            }
            Command::Load => {
                let ok = self.present && !self.complete;
                self.owned |= ok;
                (ok, 0)
            }
            Command::Finish(complete) if self.owned => {
                self.complete = complete;
                self.owned = false;
                (true, 0)
            }
            Command::Persist(slot, status)
                if self.owned && !matches!(self.results[usize::from(slot)], 1 | 2) =>
            {
                self.results[usize::from(slot)] = status;
                (true, 0)
            }
            Command::Lookup(slot) if self.owned => (true, self.results[usize::from(slot)]),
            Command::Begin | Command::Finish(_) | Command::Persist(..) | Command::Lookup(_) => {
                (false, 0)
            }
        }
    }
}
fn apply(database: &mut Database, command: Command) -> (bool, u8) {
    match command {
        Command::Begin => (
            accepted(database.handler().begin(begin(0, 0, 2)), &["session.begin"]),
            0,
        ),
        Command::Close => {
            database.reopen();
            (true, 0)
        }
        Command::Load => {
            let loaded = database.handler().load(&load(0, 2)).unwrap();
            match loaded.outcome {
                SessionLoadOutcome::Resumed(reference) => {
                    assert_eq!(reference.run_id, "run-0");
                    (true, 0)
                }
                SessionLoadOutcome::Fresh(_) => (false, 0),
            }
        }
        Command::Finish(complete) => (
            accepted(
                database.handler().finish(finish(0, complete)),
                &["session.finish.owner"],
            ),
            0,
        ),
        Command::Persist(slot, status) => (
            accepted(
                database.handler().persist(&persist(slot, status)),
                &["session.persist.owner", "session.duplicate_result"],
            ),
            0,
        ),
        Command::Lookup(slot) => match database.handler().lookup(&lookup(slot)) {
            Ok(loaded) => {
                let status = loaded
                    .result
                    .as_ref()
                    .map_or(0, |stored| status_number(stored.status));
                let reuse = resume_policy(loaded.result.as_ref());
                let expected = match status {
                    1 => ResumeDecision::Reuse(ReusedMutationStatus::Killed),
                    2 => ResumeDecision::Reuse(ReusedMutationStatus::Survived),
                    _ => ResumeDecision::Rerun,
                };
                assert_eq!(reuse, expected);
                (true, status)
            }
            Err(error) => (accepted::<()>(Err(error), &["session.lookup.owner"]), 0),
        },
    }
}
fn check_state(database: &mut Database, reference: &Reference) {
    let expected = if reference.present {
        vec![Row {
            fp: 0,
            budget: 2,
            complete: reference.complete,
        }]
    } else {
        vec![]
    };
    assert_eq!(database.rows(), expected);
    assert_eq!(database.results(), reference.results);
    for slot in 0..2u8 {
        let expected = (
            reference.owned,
            if reference.owned {
                reference.results[usize::from(slot)]
            } else {
                0
            },
        );
        assert_eq!(
            apply(database, Command::Lookup(slot)),
            expected,
            "ownership and reuse probe"
        );
    }
}
fn check_history(commands: &[Command]) -> Vec<bool> {
    let mut database = Database::new();
    let mut reference = Reference::default();
    commands
        .iter()
        .map(|&command| {
            let before = database.snapshot();
            let expected = reference.apply(command);
            let actual = apply(&mut database, command);
            assert_eq!(actual, expected, "{command:?}");
            if !actual.0 {
                assert_eq!(
                    database.snapshot(),
                    before,
                    "rejected {command:?} changed durable state"
                );
            }
            check_state(&mut database, &reference);
            actual.0
        })
        .collect()
}
fn replay_lifecycle(trace: &str) -> usize {
    let mut database = Database::new();
    let history = quint::states(trace);
    for state in &history {
        let slot = u8::try_from(quint::number(&state["slot"])).unwrap();
        let status = u8::try_from(quint::number(&state["status"])).unwrap();
        let command = match state["op"].as_str().unwrap() {
            "init" => None,
            "begin" => Some(Command::Begin),
            "close" => Some(Command::Close),
            "load" => Some(Command::Load),
            "finish" => Some(Command::Finish(status == 1)),
            "persist" => Some(Command::Persist(slot, status)),
            "lookup" => Some(Command::Lookup(slot)),
            op => panic!("unknown op {op}"),
        };
        if let Some(command) = command {
            assert_eq!(
                apply(&mut database, command),
                (
                    state["accepted"].as_bool().unwrap(),
                    u8::try_from(quint::number(&state["observed"])).unwrap()
                ),
                "{command:?}"
            );
        }
        let results = state["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| u8::try_from(quint::number(s)).unwrap())
            .collect::<Vec<_>>();
        check_state(
            &mut database,
            &Reference {
                present: state["present"].as_bool().unwrap(),
                complete: state["complete"].as_bool().unwrap(),
                owned: state["owned"].as_bool().unwrap(),
                results: results.try_into().unwrap(),
            },
        );
    }
    history.len()
}

#[rstest::rstest]
#[case::random_zero(include_str!("../../../formal/quint/traces/session_0.itf.json"))]
#[case::random_one(include_str!("../../../formal/quint/traces/session_1.itf.json"))]
#[case::random_two(include_str!("../../../formal/quint/traces/session_2.itf.json"))]
#[case::random_three(include_str!("../../../formal/quint/traces/session_3.itf.json"))]
#[case::recovery(include_str!("../../../formal/quint/traces/session_recoveryStep_0.itf.json"))]
fn quint_lifecycle_matches_sqlite(#[case] trace: &str) {
    // Arrange.
    let history = trace;
    // Act: the driver checks each operation and durable state.
    let checked_states = replay_lifecycle(history);
    // Assert.
    assert!(checked_states > 1);
}
#[rstest::rstest]
#[case::killed_is_immutable(1)]
#[case::survived_is_immutable(2)]
#[case::timeout_is_replaced(3)]
#[case::memory_is_replaced(4)]
#[case::process_limit_is_replaced(5)]
#[case::error_is_replaced(6)]
#[case::not_run_is_replaced(7)]
fn literal_recovery_preserves_result_contract(
    #[case] status: u8,
    #[values(1, 2, 3, 4, 5, 6, 7)] replacement: u8,
) {
    // Arrange.
    let commands = [
        Command::Begin,
        Command::Persist(0, status),
        Command::Close,
        Command::Load,
        Command::Lookup(0),
        Command::Persist(0, replacement),
        Command::Lookup(0),
    ];
    // Act.
    let accepted = check_history(&commands);
    // Assert.
    assert_eq!(accepted, [true, true, true, true, true, status >= 3, true]);
}
fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        Just(Command::Begin),
        Just(Command::Close),
        Just(Command::Load),
        any::<bool>().prop_map(Command::Finish),
        (0..2u8, 1..8u8).prop_map(|(slot, status)| Command::Persist(slot, status)),
        (0..2u8).prop_map(Command::Lookup)
    ]
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn generated_sqlite_lifecycle_matches_reference(commands in prop::collection::vec(command(),0..48)) {
        // Arrange: one bounded history and a fresh real SQLite database.
        // Act.
        let outcomes=check_history(&commands);
        // Assert.
        prop_assert_eq!(outcomes.len(),commands.len());
    }
}

#[derive(Clone, Copy, Debug)]
enum SelectionCommand {
    Create(u8, usize, bool),
    Load(u8, usize),
}
#[derive(Debug, PartialEq, Eq)]
enum SelectionOutcome {
    Created,
    Resumed(usize),
    Fresh(ResumeFreshReason),
}
fn apply_selection(database: &mut Database, command: SelectionCommand) -> SelectionOutcome {
    let selected = match command {
        SelectionCommand::Create(fp, budget, complete) => {
            let run = database.rows().len();
            database.handler().begin(begin(run, fp, budget)).unwrap();
            database.handler().finish(finish(run, complete)).unwrap();
            SelectionOutcome::Created
        }
        SelectionCommand::Load(fp, budget) => {
            match database.handler().load(&load(fp, budget)).unwrap().outcome {
                SessionLoadOutcome::Fresh(reason) => SelectionOutcome::Fresh(reason),
                SessionLoadOutcome::Resumed(reference) => SelectionOutcome::Resumed(
                    reference
                        .run_id
                        .strip_prefix("run-")
                        .unwrap()
                        .parse()
                        .unwrap(),
                ),
            }
        }
    };
    database.reopen();
    selected
}
fn reference_selection(rows: &mut Vec<Row>, command: SelectionCommand) -> SelectionOutcome {
    match command {
        SelectionCommand::Create(fp, budget, complete) => {
            rows.push(Row {
                fp,
                budget,
                complete,
            });
            SelectionOutcome::Created
        }
        SelectionCommand::Load(fp, budget) => {
            let selected = rows
                .iter()
                .rposition(|r| r.fp == fp && !r.complete && r.budget <= budget);
            if let Some(index) = selected {
                rows[index].budget = budget;
                return SelectionOutcome::Resumed(index);
            }
            // The best explanation offered by any row wins, regardless of row order.
            let reason = rows
                .iter()
                .map(|row| match (row.fp == fp, row.complete) {
                    (true, false) => (0, ResumeFreshReason::BudgetDecreased),
                    (true, true) => (1, ResumeFreshReason::MatchingRunComplete),
                    (false, false) => (2, ResumeFreshReason::FingerprintMismatch),
                    (false, true) => (3, ResumeFreshReason::NoIncompleteRun),
                })
                .min_by_key(|&(priority, _)| priority)
                .map_or(ResumeFreshReason::NoPriorRun, |(_, reason)| reason);
            SelectionOutcome::Fresh(reason)
        }
    }
}
fn check_selection_history(commands: &[SelectionCommand]) -> Vec<SelectionOutcome> {
    let mut database = Database::new();
    let mut rows = Vec::new();
    commands
        .iter()
        .map(|&command| {
            let expected = reference_selection(&mut rows, command);
            let actual = apply_selection(&mut database, command);
            assert_eq!(actual, expected, "{command:?}, {rows:?}");
            assert_eq!(database.rows(), rows, "persisted selection state");
            actual
        })
        .collect()
}
fn signed(value: &Value) -> i64 {
    value["#bigint"].as_str().unwrap().parse().unwrap()
}
fn replay_selection(trace: &str) -> usize {
    let mut database = Database::new();
    let history = quint::states(trace);
    for state in &history {
        let fp = u8::try_from(quint::number(&state["fingerprint"])).unwrap();
        let budget = usize::try_from(quint::number(&state["budget"])).unwrap();
        let command = match state["op"].as_str().unwrap() {
            "init" => None,
            "append" => Some(SelectionCommand::Create(
                fp,
                budget,
                state["complete"].as_bool().unwrap(),
            )),
            "load" => Some(SelectionCommand::Load(fp, budget)),
            op => panic!("unknown op {op}"),
        };
        if let Some(command) = command {
            let selected = usize::try_from(signed(&state["selected"])).ok();
            let outcome = apply_selection(&mut database, command);
            let (actual_selected, actual_reason) = match outcome {
                SelectionOutcome::Created => (None, ""),
                SelectionOutcome::Resumed(index) => (Some(index), ""),
                SelectionOutcome::Fresh(reason) => (None, reason.code()),
            };
            assert_eq!(
                (actual_selected, actual_reason),
                (selected, state["freshReason"].as_str().unwrap()),
                "{command:?}"
            );
        }
        let expected = state["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| Row {
                fp: u8::try_from(quint::number(&r["fingerprint"])).unwrap(),
                budget: usize::try_from(quint::number(&r["budget"])).unwrap(),
                complete: r["complete"].as_bool().unwrap(),
            })
            .collect::<Vec<_>>();
        assert_eq!(database.rows(), expected);
    }
    history.len()
}
#[rstest::rstest]
#[case::random_zero(include_str!("../../../formal/quint/traces/session_selection_0.itf.json"))]
#[case::random_one(include_str!("../../../formal/quint/traces/session_selection_1.itf.json"))]
#[case::random_two(include_str!("../../../formal/quint/traces/session_selection_2.itf.json"))]
#[case::random_three(include_str!("../../../formal/quint/traces/session_selection_3.itf.json"))]
#[case::latest(include_str!("../../../formal/quint/traces/session_selection_latestStep_0.itf.json"))]
#[case::fallback(include_str!("../../../formal/quint/traces/session_selection_fallbackStep_0.itf.json"))]
#[case::fresh_priority(include_str!("../../../formal/quint/traces/session_selection_freshPriorityStep_0.itf.json"))]
fn quint_resume_selection_matches_sqlite(#[case] trace: &str) {
    // Arrange.
    let history = trace;
    // Act: the driver checks each operation after closing and reopening SQLite.
    let checked_states = replay_selection(history);
    // Assert.
    assert!(checked_states > 1);
}
#[rstest::rstest]
#[case::latest(0, 1, false, 2, 1)]
#[case::other_fingerprint(1, 1, false, 2, 0)]
#[case::budget_too_large(0, 2, false, 1, 0)]
#[case::already_complete(0, 1, true, 2, 0)]
fn literal_selection_chooses_latest_eligible_run(
    #[case] fp: u8,
    #[case] budget: usize,
    #[case] complete: bool,
    #[case] requested: usize,
    #[case] expected: usize,
) {
    // Arrange.
    let commands = [
        SelectionCommand::Create(0, 1, false),
        SelectionCommand::Create(fp, budget, complete),
        SelectionCommand::Load(0, requested),
    ];
    // Act.
    let selected = check_selection_history(&commands);
    // Assert.
    assert_eq!(
        selected,
        [
            SelectionOutcome::Created,
            SelectionOutcome::Created,
            SelectionOutcome::Resumed(expected),
        ]
    );
}
#[test]
fn resumed_budget_is_durable_and_rejects_later_decrease() {
    // Arrange.
    let commands = [
        SelectionCommand::Create(0, 1, false),
        SelectionCommand::Load(0, 2),
        SelectionCommand::Load(0, 1),
        SelectionCommand::Load(0, 2),
    ];
    // Act.
    let selected = check_selection_history(&commands);
    // Assert.
    assert_eq!(
        selected,
        [
            SelectionOutcome::Created,
            SelectionOutcome::Resumed(0),
            SelectionOutcome::Fresh(ResumeFreshReason::BudgetDecreased),
            SelectionOutcome::Resumed(0),
        ]
    );
}
fn budget_strategy() -> impl Strategy<Value = usize> {
    prop_oneof![1..5usize, Just(255), Just(256), Just(usize::MAX)]
}
fn selection_commands(rows: &[(u8, usize, bool)], loads: &[(u8, usize)]) -> Vec<SelectionCommand> {
    rows.iter()
        .map(|&(f, b, c)| SelectionCommand::Create(f, b, c))
        .chain(loads.iter().map(|&(f, b)| SelectionCommand::Load(f, b)))
        .collect()
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn generated_resume_selection_matches_durable_history(
        rows in prop::collection::vec((0..3u8,budget_strategy(),any::<bool>()),0..5),
        loads in prop::collection::vec((0..3u8,budget_strategy()),0..20),
    ) {
        // Arrange: PBT extends to four runs, byte boundaries and usize::MAX.
        let commands=selection_commands(&rows,&loads);
        // Act.
        let selected=check_selection_history(&commands);
        // Assert.
        prop_assert_eq!(selected.len(),commands.len());
    }
}

fn invalid_replacement_recovered_status(slot: u8, previous: u8, replacement: u8) -> u8 {
    let mut database = Database::new();
    database.handler().begin(begin(0, 0, 2)).unwrap();
    database
        .handler()
        .persist(&persist(slot, previous))
        .unwrap();
    let before = database.snapshot();
    let mut request = persist(slot, replacement);
    request.result.diagnostics[0].mutant_id = "another-mutant".into();
    let result = database.handler().persist(&request);
    assert!(!accepted(result, &["session.persist.diagnostic"]));
    assert_eq!(
        database.snapshot(),
        before,
        "failed replacement must roll back all tables"
    );
    database.reopen();
    database.handler().load(&load(0, 2)).unwrap();
    let (owned, status) = apply(&mut database, Command::Lookup(slot));
    assert!(owned);
    status
}
#[rstest::rstest]
#[case::timeout(3)]
#[case::memory(4)]
#[case::process_limit(5)]
#[case::error(6)]
#[case::not_run(7)]
fn invalid_diagnostic_rolls_back_inconclusive_replacement(#[case] previous: u8) {
    // Arrange.
    let replacement = 1;
    // Act: the driver checks rollback and reopens the DB.
    let recovered = invalid_replacement_recovered_status(0, previous, replacement);
    // Assert.
    assert_eq!(recovered, previous);
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn generated_invalid_replacement_is_atomic(slot in 0..2u8, previous in 3..8u8, replacement in 1..8u8) {
        // Arrange: generated previous and replacement outcomes for one mutant.
        // Act: the driver also checks the complete durable snapshot.
        let recovered = invalid_replacement_recovered_status(slot, previous, replacement);
        // Assert.
        prop_assert_eq!(recovered, previous);
    }
}
