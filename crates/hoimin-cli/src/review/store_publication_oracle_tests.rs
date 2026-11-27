use super::{ReviewStore, draft, key, with_initialization_hook};
use crate::review::{ReviewError, model::SourceState};
use camino::Utf8Path;
use std::{
    cell::Cell,
    collections::BTreeSet,
    fs,
    io::{ErrorKind, Write},
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

const FOREIGN: &[u8] = b"foreign file at the review destination";
const OTHER_PROJECT: &str = "/other-review-project";
const INJECTED_FAILURE: &str = "injected private initialization failure";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(serde::Deserialize)]
struct Case {
    schema: u32,
    mode: String,
    id: String,
    shared_project: bool,
    actions: Vec<Action>,
    observations: Vec<Observation>,
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Action {
    Advance { worker: WorkerId },
    Fail { worker: WorkerId, unwind: bool },
    InstallForeign,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkerId {
    First,
    Second,
}

impl WorkerId {
    fn index(self) -> usize {
        usize::from(self == Self::Second)
    }

    fn label(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Second => "second",
        }
    }
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Created,
    TransactionFirst,
    TransactionSecond,
    TransactionThird,
    Publish,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Reply {
    Reached { phase: Phase },
    Opened { revision: u64 },
    RejectedForeign,
    RejectedProject,
    Failed,
    Unwound,
    InstalledForeign,
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Published {
    Absent,
    ForeignFile,
    Ready {
        owner: WorkerId,
        history: Vec<WorkerId>,
    },
}

#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
struct Observation {
    reply: Reply,
    published: Published,
    private_count: usize,
}

enum Permit {
    Continue,
    Fail,
    Unwind,
}

struct InjectedUnwind;

struct Worker {
    permit: Sender<Permit>,
    reply: Receiver<Reply>,
}

fn open_and_mark(path: &Path, actor: WorkerId, shared: bool) -> Result<u64, ReviewError> {
    let mut key = key();
    if actor == WorkerId::Second && !shared {
        key.project = OTHER_PROJECT.into();
    }
    let mut store = ReviewStore::open_write(path, &key.project)?;
    let expected = store
        .candidate(&key.candidate_id)?
        .and_then(|(_, state)| state.history.last().map(|event| event.revision))
        .unwrap_or(0);
    let mut review = draft();
    review.reason = actor.label().into();
    let saved = store.append(&key, SourceState::Fresh, expected, review)?;
    Ok(saved.history.last().unwrap().revision)
}

fn wrong_project(error: &ReviewError) -> bool {
    matches!(error, ReviewError::Invalid(message) if message == "store belongs to another project")
}

fn foreign_file(error: &ReviewError) -> bool {
    matches!(error, ReviewError::Database(rusqlite::Error::SqliteFailure(failure, _))
        if failure.code == rusqlite::ErrorCode::NotADatabase)
}

fn run_worker(
    path: &Path,
    actor: WorkerId,
    shared: bool,
    permissions: Receiver<Permit>,
    replies: &Sender<Reply>,
) -> Reply {
    let reports = replies.clone();
    let progress = Cell::new(0);
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        with_initialization_hook(
            Box::new(move |stage| {
                let index = progress.get();
                let phase = match (stage, index) {
                    ("created", 0) => Phase::Created,
                    ("transaction", 1) => Phase::TransactionFirst,
                    ("transaction", 2) => Phase::TransactionSecond,
                    ("transaction", 3) => Phase::TransactionThird,
                    ("publish", 4) => Phase::Publish,
                    _ => panic!("unexpected initialization stage {stage:?} at {index}"),
                };
                progress.set(index + 1);
                if reports.send(Reply::Reached { phase }).is_err() {
                    return Err(ReviewError::Invalid(INJECTED_FAILURE.into()));
                }
                match permissions.recv_timeout(TIMEOUT) {
                    Ok(Permit::Continue) => Ok(()),
                    Ok(Permit::Fail) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        Err(ReviewError::Invalid(INJECTED_FAILURE.into()))
                    }
                    Ok(Permit::Unwind) => panic::panic_any(InjectedUnwind),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        panic!("initialization permit timed out")
                    }
                }
            }),
            || open_and_mark(path, actor, shared),
        )
    }));
    match result {
        Ok(Ok(revision)) => Reply::Opened { revision },
        Ok(Err(error)) if wrong_project(&error) => Reply::RejectedProject,
        Ok(Err(error)) if foreign_file(&error) => Reply::RejectedForeign,
        Ok(Err(ReviewError::Invalid(message))) if message == INJECTED_FAILURE => Reply::Failed,
        Ok(Err(error)) => panic!("unexpected initialization error {error:?}"),
        Err(payload) if payload.is::<InjectedUnwind>() => Reply::Unwound,
        Err(payload) => panic::resume_unwind(payload),
    }
}

fn start_worker<'scope>(
    scope: &'scope thread::Scope<'scope, '_>,
    path: PathBuf,
    actor: WorkerId,
    shared: bool,
) -> Worker {
    let (permit, permissions) = mpsc::channel();
    let (replies, reply) = mpsc::channel();
    scope.spawn(move || {
        let result = run_worker(&path, actor, shared, permissions, &replies);
        let _ = replies.send(result);
    });
    Worker { permit, reply }
}

fn receive(slot: &mut Option<Worker>, context: &str) -> Reply {
    let reply = slot
        .as_ref()
        .expect(context)
        .reply
        .recv_timeout(TIMEOUT)
        .expect(context);
    if !matches!(reply, Reply::Reached { .. }) {
        *slot = None;
    }
    reply
}

fn act<'scope>(
    scope: &'scope thread::Scope<'scope, '_>,
    workers: &mut [Option<Worker>; 2],
    path: &Path,
    shared: bool,
    action: &Action,
    context: &str,
) -> Reply {
    match *action {
        Action::Advance { worker } => {
            let slot = &mut workers[worker.index()];
            if let Some(active) = slot {
                active.permit.send(Permit::Continue).expect(context);
            } else {
                *slot = Some(start_worker(scope, path.to_owned(), worker, shared));
            }
            receive(slot, context)
        }
        Action::Fail { worker, unwind } => {
            let slot = &mut workers[worker.index()];
            let permission = if unwind { Permit::Unwind } else { Permit::Fail };
            slot.as_ref()
                .expect(context)
                .permit
                .send(permission)
                .expect(context);
            receive(slot, context)
        }
        Action::InstallForeign => {
            fs::File::options()
                .write(true)
                .create_new(true)
                .open(path)
                .expect(context)
                .write_all(FOREIGN)
                .expect(context);
            Reply::InstalledForeign
        }
    }
}

fn published(path: &Path, context: &str) -> Published {
    let project = key().project;
    match fs::read(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            assert!(
                ReviewStore::open_read(path, &project)
                    .expect(context)
                    .is_none(),
                "{context}"
            );
            return Published::Absent;
        }
        Ok(bytes) if bytes == FOREIGN => return Published::ForeignFile,
        Ok(_) => {}
        Err(error) => panic!("{context}: cannot inspect public store: {error}"),
    }
    let (owner, mut store) = match ReviewStore::open_read(path, &project) {
        Ok(Some(store)) => (WorkerId::First, store),
        Err(error) if wrong_project(&error) => (
            WorkerId::Second,
            ReviewStore::open_read(path, Utf8Path::new(OTHER_PROJECT))
                .expect(context)
                .expect(context),
        ),
        _ => panic!("{context}: public database must be complete and readable"),
    };
    let rows = store.snapshot().expect(context);
    assert_eq!(rows.len(), 1, "{context}");
    let history = rows[0]
        .1
        .history
        .iter()
        .enumerate()
        .map(|(index, event)| {
            assert_eq!(
                event.revision,
                u64::try_from(index).unwrap() + 1,
                "{context}"
            );
            match event.draft.reason.as_str() {
                "first" => WorkerId::First,
                "second" => WorkerId::Second,
                reason => panic!("{context}: unexpected review marker {reason:?}"),
            }
        })
        .collect();
    Published::Ready { owner, history }
}

fn private_directories(directory: &Path, public: &Path, context: &str) -> usize {
    fs::read_dir(directory)
        .expect(context)
        .map(|entry| entry.expect(context))
        .filter(|entry| {
            if entry.path() == public {
                return false;
            }
            assert!(
                entry.file_type().expect(context).is_dir(),
                "{context}: {:?}",
                entry.path()
            );
            assert!(
                entry
                    .file_name()
                    .to_str()
                    .unwrap()
                    .starts_with(".hoimin-review-"),
                "{context}"
            );
            true
        })
        .count()
}

#[test]
fn review_store_publication_matches_lean_schedules() {
    let corpus = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../formal/HoiminOracle/corpus/review-store-publication.jsonl"
    ));
    let mut ids = BTreeSet::new();
    for line in corpus.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1, "{}", case.id);
        assert_eq!(case.mode, "internal-fixture", "{}", case.id);
        assert!(ids.insert(case.id.clone()), "{}", case.id);
        assert_eq!(case.actions.len(), case.observations.len(), "{}", case.id);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reviews.sqlite3");
        thread::scope(|scope| {
            let mut workers = [None, None];
            for (index, (action, expected)) in
                case.actions.iter().zip(case.observations).enumerate()
            {
                let context = format!("{} step {index}", case.id);

                let reply = act(
                    scope,
                    &mut workers,
                    &path,
                    case.shared_project,
                    action,
                    &context,
                );
                let actual = Observation {
                    reply,
                    published: published(&path, &context),
                    private_count: private_directories(directory.path(), &path, &context),
                };

                assert_eq!(actual, expected, "{context}");
            }
            assert!(workers.iter().all(Option::is_none), "{}", case.id);
        });
    }
    assert_eq!(ids.len(), 52);
}
