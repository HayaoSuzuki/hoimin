use std::collections::BTreeSet;
use std::convert::Infallible;
use std::fs;
use std::io;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use camino::Utf8Path;
use hoimin_core::{
    BudgetLedger, CreateWorker, DiskCleanupStatus, EffectId, PreflightCompleted, ReservationId,
    RunBudgets, WorkspaceCopyGrant, reserve_workspace_copy,
};
use serde::Deserialize;

use super::{
    CopyOptions, InitialGrantHook, ManagedRunRoot, PreflightLimits, WorkerWorkspace,
    WorkspaceError, WorkspacePlan,
};
use crate::workspace::{ManagedRootCoordinator, OwnerKind};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/worker-grant-publication.jsonl");
const AWAIT: Duration = Duration::from_secs(3);
const INJECTED_COPY: &str = "injected copy failure after reaching the copy boundary";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    per_worker: u64,
    actions: Vec<Action>,
    expected: Vec<Observation>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Grant {
    A,
    B,
    WrongPreflight,
    WrongAllowance,
}

impl Grant {
    const ALL: [Self; 4] = [Self::A, Self::B, Self::WrongPreflight, Self::WrongAllowance];
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct Request {
    grant: Grant,
    worker: u32,
    fail: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Action {
    Start(Request),
    Queue(Request),
    Publish,
    Finish { worker: u32 },
    Release { worker: u32 },
    Retain { worker: u32 },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    HoldingLock,
    Queued,
    Published,
    Copying,
    Ready,
    CopyFailed,
    Released,
    Retained,
    PreflightMismatch,
    OutOfRange,
    AllowanceMismatch,
    ReservationMismatch,
    Duplicate,
}

impl From<WorkspaceError> for Outcome {
    fn from(error: WorkspaceError) -> Self {
        match error {
            WorkspaceError::PreflightMismatch { .. } => Self::PreflightMismatch,
            WorkspaceError::WorkerOutOfRange { .. } => Self::OutOfRange,
            WorkspaceError::AllowanceMismatch { .. } => Self::AllowanceMismatch,
            WorkspaceError::ReservationMismatch { .. } => Self::ReservationMismatch,
            WorkspaceError::WorkerAlreadyExists { .. } => Self::Duplicate,
            WorkspaceError::Io {
                operation: "copy worker file" | "copy worker permissions",
                message,
                ..
            } if message == INJECTED_COPY => Self::CopyFailed,
            other => panic!("unexpected workspace outcome: {other:?}"),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Observation {
    outcome: Outcome,
    queued_outcome: Option<Outcome>,
    locked: bool,
    reservation: Option<Grant>,
    registered: Option<Vec<u32>>,
    copying: Vec<u32>,
    ready: Vec<u32>,
    allowance: u64,
    charged: u64,
    worker_roots: usize,
    disk_bytes: u64,
}

struct Gate(Option<SyncSender<()>>);

impl Gate {
    fn open(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.try_send(());
        }
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.open();
    }
}

enum Update {
    LockProbe(bool),
    Copying,
    Finished(Box<Result<WorkerWorkspace, WorkspaceError>>),
}

struct Attempt {
    worker: u32,
    copy_gate: Gate,
    updates: Receiver<Update>,
    thread: Option<JoinHandle<()>>,
}

impl Attempt {
    fn spawn(plan: Arc<WorkspacePlan>, request: CreateWorker, fail: bool, probe: bool) -> Self {
        let worker = request.worker();
        let (release, copy_go) = mpsc::sync_channel(1);
        let (signals, updates) = mpsc::channel();
        let copy_signals = signals.clone();
        let thread = thread::spawn(move || {
            if probe {
                let held = matches!(plan.state.try_lock(), Err(TryLockError::WouldBlock));
                signals.send(Update::LockProbe(held)).unwrap();
            }
            let per_worker = plan.completed().per_worker_logical_bytes;
            let boundary = if per_worker == 0 {
                "copy.worker.permissions"
            } else {
                "copy.worker.write"
            };
            let skip = usize::from(per_worker > crate::workspace::stream::BUFFER_BYTES as u64);
            let _copy_hook = crate::io_faults::arm_action(boundary, skip, move || {
                copy_signals
                    .send(Update::Copying)
                    .map_err(|_| io::Error::other("copy observer disconnected"))?;
                copy_go.recv().map_err(io::Error::other)?;
                if fail {
                    Err(io::Error::other(INJECTED_COPY))
                } else {
                    Ok(())
                }
            });
            let result = plan.create_worker(&request);
            let _ = signals.send(Update::Finished(Box::new(result)));
        });
        Self {
            worker,
            copy_gate: Gate(Some(release)),
            updates,
            thread: Some(thread),
        }
    }

    fn receive(&self) -> Update {
        self.updates
            .recv_timeout(AWAIT)
            .expect("worker boundary response")
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        self.copy_gate.open();
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !thread::panicking() {
                result.expect("worker attempt thread did not panic");
            }
        }
    }
}

struct Fixture {
    // The publication gate opens before any worker join during unwinding.
    grant_gate: Gate,
    leader: Option<Attempt>,
    queued: Option<Attempt>,
    copying: [Option<Attempt>; 2],
    ready: [Option<WorkerWorkspace>; 2],
    grant_entered: Receiver<()>,
    grants: [WorkspaceCopyGrant; 4],
    plan: Arc<WorkspacePlan>,
    source: Vec<u8>,
    _project: tempfile::TempDir,
}

impl Fixture {
    fn new(per_worker: u64, managed: &Arc<ManagedRunRoot>) -> Self {
        let project = tempfile::tempdir().unwrap();
        let source = vec![b'x'; usize::try_from(per_worker).unwrap()];
        fs::write(project.path().join("target.py"), &source).unwrap();
        let mut plan = WorkspacePlan::preflight_validated_in(
            Utf8Path::from_path(project.path()).unwrap(),
            EffectId(7),
            2,
            CopyOptions::default(),
            Some(Arc::clone(managed)),
            PreflightLimits::default(),
            |_, _| Ok::<_, Infallible>(()),
        )
        .unwrap();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: u64::MAX,
            processes: 1,
        });
        let grants = Grant::ALL.map(|kind| {
            let wrong = kind == Grant::WrongPreflight || kind == Grant::WrongAllowance;
            let bytes = per_worker / 2 + u64::from(wrong);
            let preflight = if kind == Grant::WrongPreflight { 8 } else { 7 };
            reserve_workspace_copy(
                &mut ledger,
                &PreflightCompleted {
                    id: EffectId(preflight),
                    per_worker_logical_bytes: bytes,
                    requested_workers: 4,
                    aggregate_logical_bytes: bytes * 4,
                    fingerprint: None,
                },
            )
            .unwrap()
        });
        let (entered, grant_entered) = mpsc::sync_channel(1);
        let (release, grant_go) = mpsc::sync_channel(1);
        plan.initial_grant_hook = Some(Arc::new(InitialGrantHook {
            entered,
            release: Mutex::new(grant_go),
        }));
        Self {
            grant_gate: Gate(Some(release)),
            leader: None,
            queued: None,
            copying: [None, None],
            ready: [None, None],
            grant_entered,
            grants,
            plan: Arc::new(plan),
            source,
            _project: project,
        }
    }

    fn reservation_name(&self, id: ReservationId) -> Grant {
        Grant::ALL
            .into_iter()
            .zip(self.grants)
            .find_map(|(name, grant)| (grant.reservation_id() == id).then_some(name))
            .unwrap()
    }

    fn launch(&self, request: Request, probe: bool) -> Attempt {
        let index = Grant::ALL
            .iter()
            .position(|grant| *grant == request.grant)
            .unwrap();
        let command = self.grants[index]
            .create_worker(EffectId(9), request.worker)
            .unwrap();
        Attempt::spawn(Arc::clone(&self.plan), command, request.fail, probe)
    }

    fn after_admission(&mut self, attempt: Attempt) -> Outcome {
        match attempt.receive() {
            Update::Copying => {
                let index = usize::try_from(attempt.worker).unwrap();
                assert!(self.copying[index].replace(attempt).is_none());
                Outcome::Copying
            }
            Update::Finished(result) => result.expect_err("copy boundary was skipped").into(),
            Update::LockProbe(_) => panic!("unexpected second lock probe"),
        }
    }

    fn act(&mut self, action: Action) -> (Outcome, Option<Outcome>) {
        let outcome = match action {
            Action::Start(request) => {
                assert!(self.leader.is_none());
                let attempt = self.launch(request, false);
                if self.grant_gate.0.is_some() {
                    self.leader = Some(attempt);
                    self.grant_entered
                        .recv_timeout(AWAIT)
                        .expect("first binding holds the plan lock");
                    Outcome::HoldingLock
                } else {
                    self.after_admission(attempt)
                }
            }
            Action::Queue(request) => {
                assert!(self.leader.is_some() && self.queued.is_none());
                self.queued = Some(self.launch(request, true));
                assert!(matches!(
                    self.queued.as_ref().unwrap().receive(),
                    Update::LockProbe(true)
                ));
                Outcome::Queued
            }
            Action::Publish => {
                self.grant_gate.open();
                let leader = self.leader.take().unwrap();
                assert_eq!(self.after_admission(leader), Outcome::Copying);
                let queued = self
                    .queued
                    .take()
                    .map(|attempt| self.after_admission(attempt));
                return (Outcome::Published, queued);
            }
            Action::Finish { worker } => {
                let index = usize::try_from(worker).unwrap();
                let mut attempt = self.copying[index].take().unwrap();
                attempt.copy_gate.open();
                let Update::Finished(result) = attempt.receive() else {
                    panic!("worker did not finish")
                };
                match *result {
                    Ok(worker) => {
                        assert_eq!(worker.read("target.py").unwrap(), self.source);
                        assert!(self.ready[index].replace(worker).is_none());
                        Outcome::Ready
                    }
                    Err(error) => error.into(),
                }
            }
            Action::Release { worker } | Action::Retain { worker } => {
                let mut worker = self.ready[usize::try_from(worker).unwrap()].take().unwrap();
                let root = worker.root().to_owned();
                let retain = matches!(action, Action::Retain { .. });
                if !retain {
                    worker.try_cleanup().unwrap();
                }
                drop(worker);
                assert_eq!(root.try_exists().unwrap(), retain, "{root}");
                if retain {
                    Outcome::Retained
                } else {
                    Outcome::Released
                }
            }
        };
        (outcome, None)
    }

    fn observe(&self, outcome: Outcome, queued_outcome: Option<Outcome>) -> Observation {
        let (locked, reservation, registered) = match self.plan.state.try_lock() {
            Ok(state) => (
                false,
                state.reservation.map(|id| self.reservation_name(id)),
                Some(state.workers.iter().copied().collect()),
            ),
            Err(TryLockError::WouldBlock) => (true, None, None),
            Err(TryLockError::Poisoned(_)) => panic!("workspace plan lock was poisoned"),
        };
        let (worker_roots, disk_bytes) =
            fs::read_dir(self.plan.managed_root.as_ref().unwrap().path())
                .unwrap()
                .map(|entry| entry.unwrap())
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("worker-"))
                .fold((0, 0), |(count, bytes), entry| {
                    let size = fs::metadata(entry.path().join("workspace/target.py"))
                        .unwrap()
                        .len();
                    (count + 1, bytes + size)
                });
        Observation {
            outcome,
            queued_outcome,
            locked,
            reservation,
            registered,
            copying: (0_u32..2)
                .zip(&self.copying)
                .filter_map(|(worker, slot)| slot.as_ref().map(|_| worker))
                .collect(),
            ready: (0_u32..2)
                .zip(&self.ready)
                .filter_map(|(worker, slot)| slot.as_ref().map(|_| worker))
                .collect(),
            allowance: self.plan.allowance.granted.load(Ordering::Acquire),
            charged: self.plan.observed_copy_bytes(),
            worker_roots,
            disk_bytes,
        }
    }
}

#[test]
fn worker_grant_publication_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    let mut transitions = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert_eq!(case.actions.len(), case.expected.len());
        assert!(ids.insert(case.id.clone()));
        let parent = tempfile::tempdir().unwrap();
        let coordinator =
            ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
        let managed =
            Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
        let mut fixture = Fixture::new(case.per_worker, &managed);
        for (index, (action, expected)) in case.actions.into_iter().zip(case.expected).enumerate() {
            let (outcome, queued) = fixture.act(action);
            let observed = fixture.observe(outcome, queued);
            assert_eq!(
                observed,
                expected,
                "{} step {} {action:?}",
                case.id,
                index + 1
            );
            transitions += 1;
        }
        drop(fixture);
        assert_eq!(
            managed.cleanup(AWAIT).disposition.report_status(),
            DiskCleanupStatus::Clean,
            "{}",
            case.id
        );
    }
    assert_eq!((ids.len(), transitions), (80, 724));
}
