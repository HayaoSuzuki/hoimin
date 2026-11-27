use std::cell::Cell;
use std::time::{Duration, Instant};

use rusqlite::{Connection, Error, ffi};
use serde::Deserialize;

use super::{SchemaError, enable_wal_until};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/session-wal-retry.jsonl");

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    timeout: u64,
    attempts: Vec<Attempt>,
    expected: Observation,
}

#[derive(Deserialize)]
struct Attempt {
    reply: Reply,
    query_time: u64,
    clock_advance: u64,
    oversleep: u64,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Reply {
    Mode { name: String },
    Fault { fault: Fault },
}

impl Reply {
    fn request(&self) -> rusqlite::Result<String> {
        match self {
            Self::Mode { name } => Ok(name.clone()),
            Self::Fault { fault } => Err(fault.to_error()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Fault {
    Busy,
    BusyRecovery,
    Locked,
    LockedSharedCache,
    Io,
    InvalidQuery,
}

impl Fault {
    fn to_error(self) -> Error {
        let code = match self {
            Self::Busy => ffi::SQLITE_BUSY,
            Self::BusyRecovery => ffi::SQLITE_BUSY_RECOVERY,
            Self::Locked => ffi::SQLITE_LOCKED,
            Self::LockedSharedCache => ffi::SQLITE_LOCKED_SHAREDCACHE,
            Self::Io => ffi::SQLITE_IOERR,
            Self::InvalidQuery => return Error::InvalidQuery,
        };
        Error::SqliteFailure(ffi::Error::new(code), None)
    }

    fn from_error(error: Error) -> Self {
        match error {
            Error::InvalidQuery => Self::InvalidQuery,
            Error::SqliteFailure(code, _) => match code.extended_code {
                ffi::SQLITE_BUSY => Self::Busy,
                ffi::SQLITE_BUSY_RECOVERY => Self::BusyRecovery,
                ffi::SQLITE_LOCKED => Self::Locked,
                ffi::SQLITE_LOCKED_SHAREDCACHE => Self::LockedSharedCache,
                ffi::SQLITE_IOERR => Self::Io,
                unexpected => panic!("unexpected SQLite extended code {unexpected}"),
            },
            unexpected => panic!("unexpected SQLite error {unexpected:?}"),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Outcome {
    Enabled,
    Refused { mode: String },
    Failed { fault: Fault },
}

impl Outcome {
    fn from_result(result: Result<(), SchemaError>) -> Self {
        match result {
            Ok(()) => Self::Enabled,
            Err(SchemaError::JournalMode(mode)) => Self::Refused { mode },
            Err(SchemaError::Database(error)) => Self::Failed {
                fault: Fault::from_error(error),
            },
            Err(unexpected) => panic!("unexpected WAL conversion outcome: {unexpected:?}"),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Observation {
    outcome: Outcome,
    query_starts: Vec<u64>,
    sleeps: Vec<u64>,
}

fn replay(case: &Case) -> Observation {
    let start = Instant::now();
    let current = Cell::new(start);
    let active = Cell::new(None::<&Attempt>);
    let reads = Cell::new(0);
    let mut attempts = case.attempts.iter();
    let mut query_starts = Vec::new();
    let mut sleeps = Vec::new();
    let result = enable_wal_until(
        start + Duration::from_millis(case.timeout),
        || {
            let attempt = attempts
                .next()
                .unwrap_or_else(|| panic!("{} unexpected additional query", case.id));
            query_starts.push(
                current
                    .get()
                    .duration_since(start)
                    .as_millis()
                    .try_into()
                    .unwrap(),
            );
            current.set(current.get() + Duration::from_millis(attempt.query_time));
            active.set(Some(attempt));
            reads.set(0);
            attempt.reply.request()
        },
        || {
            let read = reads.get() + 1;
            reads.set(read);
            assert!(read <= 2, "{} extra clock read", case.id);
            if read == 2 {
                let lag = active.get().unwrap().clock_advance;
                current.set(current.get() + Duration::from_millis(lag));
            }
            current.get()
        },
        |delay| {
            assert_eq!(reads.get(), 2, "{} sleep without clock resample", case.id);
            sleeps.push(delay.as_millis().try_into().unwrap());
            let oversleep = active.get().unwrap().oversleep;
            current.set(current.get() + delay + Duration::from_millis(oversleep));
        },
    );
    Observation {
        outcome: Outcome::from_result(result),
        query_starts,
        sleeps,
    }
}

#[test]
fn session_wal_retry_matches_lean_oracle() {
    let mut total = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert_eq!(replay(&case), case.expected, "{}", case.id);
        total += 1;
    }
    assert!(total > 0, "WAL retry corpus is empty");
}

#[test]
fn wal_retry_succeeds_after_peer_commits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.sqlite3");
    let owner = Connection::open(&path).unwrap();
    owner
        .execute_batch(
            "CREATE TABLE original(value); BEGIN EXCLUSIVE; INSERT INTO original VALUES(7);",
        )
        .unwrap();
    let contender = Connection::open(&path).unwrap();
    contender.busy_timeout(Duration::ZERO).unwrap();

    let start = Instant::now();
    let clock = Cell::new(start);
    let mut sleeps = Vec::new();
    enable_wal_until(
        start + Duration::from_millis(20),
        || contender.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0)),
        || clock.get(),
        |delay| {
            assert!(
                sleeps.is_empty(),
                "retry after releasing owning transaction"
            );
            sleeps.push(delay);
            owner.execute_batch("COMMIT").unwrap();
            clock.set(clock.get() + delay);
        },
    )
    .unwrap();
    assert_eq!(sleeps, [Duration::from_millis(5)]);
    let mode: String = contender
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let value: i64 = contender
        .query_row("SELECT value FROM original", [], |row| row.get(0))
        .unwrap();
    assert_eq!(value, 7);
}
