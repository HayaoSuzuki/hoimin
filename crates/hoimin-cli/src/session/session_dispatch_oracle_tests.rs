use std::collections::BTreeSet;
use std::future::Future;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Arc, Mutex, TryLockError, Weak, mpsc};
use std::task::{Context, Waker};
use std::time::Duration;

use hoimin_core::ResumeFreshReason;
use serde::Deserialize;
use tokio::sync::oneshot;

use super::{
    BeginSession, EffectFailure, EffectId, LoadSession, OptionalExtension, SessionCommitPause,
    SessionDispatcher, SessionHandler, SessionLoadOutcome,
    coverage_tests::{begin_request, reject_commit},
    params,
};

const CORPUS: &str = include_str!("../../../../formal/HoiminOracle/corpus/session-dispatch.jsonl");
const RUN: &str = "dispatch-oracle";
const REQUEST_ID: EffectId = EffectId(20);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    command: Command,
    waiter: Waiter,
    event: Event,
    initial: Snapshot,
    pending: Option<Snapshot>,
    settled: Snapshot,
    after_dispatcher_drop: Snapshot,
    reply: Option<Reply>,
    requested_budget: NonZeroUsize,
    edited_budget: u64,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Command {
    Begin,
    Resume,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Waiter {
    Awaited,
    #[serde(rename = "waiter_cancelled")]
    CancelledAtGate,
    QueuedCancelled,
    DispatcherDropped,
    Unpolled,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Event {
    Normal,
    DatabaseError,
    BudgetChanged,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Reply {
    Started,
    Resumed,
    CommitError,
    RecheckError,
    CandidateChanged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    budget: Option<u64>,
    locked: bool,
}

fn start_request(budget: NonZeroUsize) -> BeginSession {
    BeginSession {
        id: REQUEST_ID,
        run_id: RUN.into(),
        max_mutants: budget,
        ..begin_request()
    }
}

fn snapshot(probe: &SessionHandler) -> Snapshot {
    let budget = probe
        .connection
        .query_row(
            "SELECT max_mutants, finished, complete FROM runs WHERE run_id=?1",
            [RUN],
            |row| {
                let bytes: Vec<u8> = row.get(0)?;
                assert!(!row.get::<_, bool>(1)?);
                assert!(!row.get::<_, bool>(2)?);
                Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
            },
        )
        .optional()
        .unwrap();
    let locked = match probe.acquire_ownership(EffectId(21), RUN, "probe dispatcher ownership") {
        Ok(ownership) => {
            drop(ownership);
            false
        }
        Err(error) => {
            assert_eq!(error.failure.code(), "session.resume.active");
            true
        }
    };
    Snapshot { budget, locked }
}

fn seed(path: &Path, case: &Case) {
    if let Some(budget) = case.initial.budget {
        assert!(matches!(case.command, Command::Resume));
        let mut seeder = SessionHandler::open(path).unwrap();
        let budget = NonZeroUsize::try_from(usize::try_from(budget).unwrap()).unwrap();
        seeder.begin(start_request(budget)).unwrap();
    }
}

struct Gate {
    entered: oneshot::Receiver<Weak<Mutex<SessionHandler>>>,
    release: mpsc::Sender<()>,
}

fn pause(dispatcher: &SessionDispatcher, case: &Case) -> Gate {
    let (entered, ready) = oneshot::channel();
    let (release, hold) = mpsc::channel();
    let gate = SessionCommitPause {
        entered,
        release: hold,
    };
    let hook = gate.into_hook(Arc::downgrade(&dispatcher.handler));
    let mut handler = dispatcher.handler.lock().unwrap();
    match case.command {
        Command::Begin => {
            if matches!(case.event, Event::DatabaseError) {
                reject_commit(&handler, "INSERT", "runs");
            }
            handler.begin_before_commit = Some(hook);
        }
        Command::Resume => {
            if matches!(case.event, Event::DatabaseError) {
                handler
                    .connection
                    .execute_batch(
                        "CREATE TEMP TRIGGER dispatch_refuse_update BEFORE UPDATE ON main.runs
                         BEGIN SELECT RAISE(FAIL, 'oracle refused resume'); END;",
                    )
                    .unwrap();
            }
            handler.resume_after_candidate = Some(Box::new(move |run_id| {
                assert_eq!(run_id, RUN);
                hook();
            }));
        }
    }
    Gate {
        entered: ready,
        release,
    }
}

async fn invoke(dispatcher: &SessionDispatcher, case: &Case) -> Reply {
    let rejected = match case.command {
        Command::Begin => match dispatcher.begin(start_request(case.requested_budget)).await {
            Ok(started) => {
                assert_eq!((started.id, started.run_id.as_str()), (REQUEST_ID, RUN));
                return Reply::Started;
            }
            Err(error) => error,
        },
        Command::Resume => {
            let reply = dispatcher
                .load(LoadSession {
                    id: REQUEST_ID,
                    fingerprint: begin_request().fingerprint,
                    max_mutants: case.requested_budget,
                })
                .await;
            match reply {
                Ok(loaded) => {
                    assert_eq!(loaded.id, REQUEST_ID);
                    return match loaded.outcome {
                        SessionLoadOutcome::Resumed(reference) => {
                            assert_eq!(reference.run_id, RUN);
                            Reply::Resumed
                        }
                        SessionLoadOutcome::Fresh(ResumeFreshReason::CandidateChanged) => {
                            Reply::CandidateChanged
                        }
                        SessionLoadOutcome::Fresh(reason) => {
                            panic!("unexpected fresh outcome: {reason:?}")
                        }
                    };
                }
                Err(error) => error,
            }
        }
    };
    assert_eq!(rejected.id, REQUEST_ID);
    let EffectFailure::SessionDatabase {
        code,
        operation,
        message,
    } = rejected.failure
    else {
        panic!("unexpected dispatch failure: {:?}", rejected.failure);
    };
    match (code.as_str(), operation.as_str()) {
        ("session.commit", "commit run") => Reply::CommitError,
        ("session.read", "re-read resume run") if message.contains("oracle refused resume") => {
            Reply::RecheckError
        }
        unexpected => panic!("unexpected dispatch failure: {unexpected:?}: {message}"),
    }
}

fn release_worker(
    probe: &SessionHandler,
    worker: &Weak<Mutex<SessionHandler>>,
    release: &mpsc::Sender<()>,
    case: &Case,
) {
    {
        let running = worker.upgrade().expect("blocked worker owns its handler");
        assert!(matches!(running.try_lock(), Err(TryLockError::WouldBlock)));
    }
    assert_eq!(Some(snapshot(probe)), case.pending, "{}: pending", case.id);
    if matches!(case.event, Event::BudgetChanged) {
        let changed = probe
            .connection
            .execute(
                "UPDATE runs SET max_mutants=?1 WHERE run_id=?2",
                params![case.edited_budget.to_be_bytes().as_slice(), RUN],
            )
            .unwrap();
        assert_eq!(changed, 1, "{}", case.id);
    }
    release.send(()).unwrap();
}

async fn replay_dispatched(
    dispatcher: SessionDispatcher,
    probe: &SessionHandler,
    gate: Gate,
    case: &Case,
) {
    let mut future = Box::pin(invoke(&dispatcher, case));
    let worker = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            reply = &mut future => panic!("{}: returned before pause: {reply:?}", case.id),
            entered = gate.entered => entered.unwrap(),
        }
    })
    .await
    .unwrap();
    let (reply, retained) = match case.waiter {
        Waiter::Awaited => {
            release_worker(probe, &worker, &gate.release, case);
            (Some(future.await), Some(dispatcher))
        }
        Waiter::CancelledAtGate => {
            drop(future);
            release_worker(probe, &worker, &gate.release, case);
            // A subsequent operation can take the handler mutex once the worker returns.
            dispatcher.call(EffectId(22), |_| Ok(())).await.unwrap();
            (None, Some(dispatcher))
        }
        Waiter::DispatcherDropped => {
            drop(future);
            drop(dispatcher);
            release_worker(probe, &worker, &gate.release, case);
            SessionDispatcher::wait_for_session_worker_drop(&worker).await;
            (None, None)
        }
        Waiter::QueuedCancelled | Waiter::Unpolled => unreachable!(),
    };
    assert_completion(probe, worker, retained, reply, case).await;
}

async fn assert_completion(
    probe: &SessionHandler,
    worker: Weak<Mutex<SessionHandler>>,
    retained: Option<SessionDispatcher>,
    reply: Option<Reply>,
    case: &Case,
) {
    assert_eq!(reply, case.reply, "{}", case.id);
    assert_eq!(snapshot(probe), case.settled, "{}: settled", case.id);
    drop(retained);
    SessionDispatcher::wait_for_session_worker_drop(&worker).await;
    assert_eq!(
        snapshot(probe),
        case.after_dispatcher_drop,
        "{}: dispatcher dropped",
        case.id,
    );
}

async fn replay_cancelled_in_queue(
    dispatcher: SessionDispatcher,
    probe: &SessionHandler,
    mut gate: Gate,
    case: &Case,
) {
    {
        // Poll once while the test holds the mutex, so the worker cannot enter the handler.
        let _guard = dispatcher.handler.lock().unwrap();
        let mut future = Box::pin(invoke(&dispatcher, case));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        drop(future);
        assert_eq!(Arc::strong_count(&dispatcher.handler), 2);
        assert!(matches!(
            gate.entered.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        assert_eq!(snapshot(probe), case.initial, "{}: queued", case.id);
    }
    let worker = tokio::time::timeout(Duration::from_secs(5), gate.entered)
        .await
        .unwrap()
        .unwrap();
    release_worker(probe, &worker, &gate.release, case);
    dispatcher.call(EffectId(22), |_| Ok(())).await.unwrap();
    assert_completion(probe, worker, Some(dispatcher), None, case).await;
}

async fn replay(case: &Case) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.sqlite3");
    seed(&path, case);
    let probe = SessionHandler::open(&path).unwrap();
    let dispatcher = SessionDispatcher::open(&path).await.unwrap();
    assert_eq!(snapshot(&probe), case.initial, "{}: initial", case.id);
    let mut gate = pause(&dispatcher, case);
    if matches!(case.waiter, Waiter::Unpolled) {
        drop(invoke(&dispatcher, case));
        assert_eq!(Arc::strong_count(&dispatcher.handler), 1);
        assert!(matches!(
            gate.entered.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        assert!(
            case.reply.is_none() && case.pending.is_none(),
            "{}",
            case.id
        );
        assert_eq!(snapshot(&probe), case.settled, "{}: unpolled", case.id);
        drop(dispatcher);
        assert_eq!(
            snapshot(&probe),
            case.after_dispatcher_drop,
            "{}: dispatcher dropped without work",
            case.id,
        );
    } else if matches!(case.waiter, Waiter::QueuedCancelled) {
        replay_cancelled_in_queue(dispatcher, &probe, gate, case).await;
    } else {
        replay_dispatched(dispatcher, &probe, gate, case).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_begin_and_resume_match_lean_oracle() {
    let mut ids = BTreeSet::new();
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        replay(&case).await;
        assert!(ids.insert(case.id));
    }
    assert_eq!(ids.len(), 25);
}
