use std::collections::HashSet;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    BudgetLedger, CreateWorker, EffectId, ResetWorker, RunBudgets, RunEvent, reserve_workspace_copy,
};
use serde::Deserialize;

use super::super::{CopyOptions, WorkspacePlan, WorkspaceTask, WorkspaceTaskCompletion};
use super::{
    WorkerSlot, WorkspaceHandler, WorkspaceOwnership, create_overdeep_tree, prepared_handler,
};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/worker-recovery.jsonl");
const CLEANUP_FAULT: &str = "worker.clear.open";
const COPY_FAULT: &str = "copy.worker.write";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    original: String,
    schedule: Vec<Operation>,
    expected: Vec<Observation>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Reset,
    RestoreFault,
    DiscardFault,
    DepthFault,
    Create,
    Foreign,
    ForeignWithCleanupFault,
    RetryCleanupFault,
    CopyFault,
    ImportFault,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct View {
    active: Option<usize>,
    pending: Option<usize>,
    slots: usize,
    bytes: u64,
    live_roots: Vec<usize>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Observation {
    outcome: String,
    diagnostics: Vec<String>,
    fault_consumed: Option<bool>,
    prepared: Option<View>,
    executed: Option<View>,
    accepted: View,
}

enum Request {
    Create(CreateWorker),
    Reset(ResetWorker),
}

struct Fixture {
    handler: WorkspaceHandler,
    retry: CreateWorker,
    foreign: CreateWorker,
    roots: Vec<Utf8PathBuf>,
}

impl Fixture {
    fn new(handler: WorkspaceHandler, retry: CreateWorker, project: &Utf8Path) -> Self {
        let completed = WorkspacePlan::preflight(project, EffectId(999), 1, CopyOptions::default())
            .unwrap()
            .completed();
        let mut ledger = BudgetLedger::new(RunBudgets {
            memory: 1,
            copy: completed.aggregate_logical_bytes,
            processes: 1,
        });
        let wrong_grant = reserve_workspace_copy(&mut ledger, &completed).unwrap();
        let foreign = wrong_grant.create_worker(EffectId(998), 0).unwrap();
        let root = handler.worker(0).unwrap().root().to_owned();
        Self {
            handler,
            retry,
            foreign,
            roots: vec![root],
        }
    }

    fn generation(roots: &mut Vec<Utf8PathBuf>, path: &Utf8Path) -> usize {
        if let Some(index) = roots.iter().position(|root| root == path) {
            return index;
        }
        let index = roots.len();
        roots.push(path.to_owned());
        index
    }

    fn view(&mut self) -> View {
        let (active, pending) = match self.handler.workers.get(&0) {
            Some(WorkerSlot::Active(worker)) => {
                (Some(Self::generation(&mut self.roots, worker.root())), None)
            }
            Some(WorkerSlot::PendingCleanup(worker)) => {
                (None, Some(Self::generation(&mut self.roots, worker.root())))
            }
            None => (None, None),
        };
        assert_eq!(self.handler.worker_count(), usize::from(active.is_some()));
        assert_eq!(
            self.handler.pending_cleanup_count(),
            usize::from(pending.is_some())
        );
        View {
            active,
            pending,
            slots: self.handler.materialized_worker_slots(),
            bytes: self.handler.observed_copy_bytes(),
            live_roots: self
                .roots
                .iter()
                .enumerate()
                .filter_map(|(index, root)| root.try_exists().unwrap().then_some(index))
                .collect(),
        }
    }

    fn request(&self, op: Operation) -> Request {
        match op {
            Operation::Reset
            | Operation::RestoreFault
            | Operation::DiscardFault
            | Operation::DepthFault => Request::Reset(ResetWorker {
                id: EffectId(60),
                worker: 0,
            }),
            Operation::Foreign | Operation::ForeignWithCleanupFault => {
                Request::Create(self.foreign.clone())
            }
            _ => Request::Create(self.retry.clone()),
        }
    }

    fn run_sync(&mut self, request: Request) -> RunEvent {
        match request {
            Request::Create(request) => self
                .handler
                .handle_create_worker(request)
                .map_or_else(RunEvent::EffectFailed, RunEvent::WorkerCreated),
            Request::Reset(request) => self
                .handler
                .handle_reset_worker(request)
                .map_or_else(RunEvent::EffectFailed, RunEvent::WorkerReset),
        }
    }

    fn prepare(&mut self, request: Request) -> Result<WorkspaceTask, hoimin_core::EffectFailed> {
        match request {
            Request::Create(request) => self.handler.prepare_create_task(request),
            Request::Reset(request) => self.handler.prepare_reset_task(request),
        }
    }

    fn run_owned(&mut self, request: Request) -> (RunEvent, Option<View>, Option<View>) {
        let task = match self.prepare(request) {
            Ok(task) => task,
            Err(failure) => return (RunEvent::EffectFailed(failure), None, None),
        };
        let prepared = self.view();
        let completion: WorkspaceTaskCompletion = task.execute();
        match &completion.ownership {
            WorkspaceOwnership::Active(_, worker)
            | WorkspaceOwnership::PendingCleanup(_, worker) => {
                Self::generation(&mut self.roots, worker.root());
            }
            WorkspaceOwnership::None => {}
        }
        let executed = self.view();
        let event = self.handler.accept_task_completion(completion).unwrap();
        (event, Some(prepared), Some(executed))
    }

    fn run(&mut self, op: Operation, original: &str, owned: bool) -> Observation {
        self.handler.import_roots = if op == Operation::ImportFault {
            vec!["missing-import-root".into()]
        } else {
            Vec::new()
        };
        let missing_snapshot = matches!(op, Operation::RestoreFault | Operation::DiscardFault)
            .then(|| {
                let path = self
                    .handler
                    .worker(0)
                    .unwrap()
                    .snapshot
                    .root
                    .join("pkg/a.py");
                fs::remove_file(&path).unwrap();
                path
            });
        let deepest = (op == Operation::DepthFault)
            .then(|| create_overdeep_tree(self.handler.worker(0).unwrap().root()));
        let fault = match op {
            Operation::DiscardFault
            | Operation::ForeignWithCleanupFault
            | Operation::RetryCleanupFault => Some(CLEANUP_FAULT),
            Operation::CopyFault => Some(COPY_FAULT),
            _ => None,
        }
        .map(crate::io_faults::arm);

        let request = self.request(op);
        let (event, prepared, executed) = if owned {
            self.run_owned(request)
        } else {
            (self.run_sync(request), None, None)
        };
        let fault_consumed = fault.map(|guard| {
            let triggered = guard.triggered();
            drop(guard);
            triggered
        });
        let accepted = self.view();
        if let Some(path) = missing_snapshot {
            fs::write(path, original).unwrap();
        }
        if let Some(path) = deepest
            && path.try_exists().unwrap()
        {
            fs::remove_dir(path).unwrap();
        }
        let (outcome, diagnostics) = summarize(&event);
        Observation {
            outcome,
            diagnostics,
            fault_consumed,
            prepared,
            executed,
            accepted,
        }
    }
}

fn summarize(event: &RunEvent) -> (String, Vec<String>) {
    let failure = match event {
        RunEvent::WorkerCreated(_) => return ("created".into(), Vec::new()),
        RunEvent::WorkerReset(_) => return ("reset".into(), Vec::new()),
        RunEvent::EffectFailed(failure) => &failure.failure,
        other => panic!("unexpected worker effect event: {other:?}"),
    };
    let message = failure.message();
    let mut found = [
        ("read shared snapshot", "restore"),
        ("discard cleanup failed", "discard"),
        ("workspace tree depth exceeds limit", "depth"),
        (CLEANUP_FAULT, "cleanup"),
        (COPY_FAULT, "copy"),
    ]
    .into_iter()
    .filter(|(_, diagnostic)| {
        *diagnostic != "cleanup" || !message.contains("discard cleanup failed")
    })
    .filter_map(|(marker, diagnostic)| message.find(marker).map(|index| (index, diagnostic)))
    .collect::<Vec<_>>();
    found.sort_unstable();
    (
        failure.code().to_owned(),
        found.into_iter().map(|(_, name)| name.to_owned()).collect(),
    )
}

#[rstest::rstest]
#[case::synchronous(false)]
#[case::owned(true)]
fn worker_recovery_matches_lean_oracle(#[case] owned: bool) {
    let mut cases = HashSet::new();
    let mut steps = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert_eq!(case.schedule.len(), case.expected.len());
        assert!(cases.insert(case.id.clone()));
        let (project, handler, retry) = prepared_handler();
        let project_root = Utf8Path::from_path(project.path()).unwrap();
        assert_eq!(
            fs::read_to_string(project_root.join("pkg/a.py")).unwrap(),
            case.original
        );
        let mut fixture = Fixture::new(handler, retry, project_root);
        for (index, (op, mut expected)) in case.schedule.into_iter().zip(case.expected).enumerate()
        {
            if !owned {
                expected.prepared = None;
                expected.executed = None;
            }
            let actual = fixture.run(op, &case.original, owned);
            assert_eq!(actual, expected, "{} step {} {op:?}", case.id, index + 1);
            steps += 1;
        }
        fixture.handler.close().unwrap();
        for root in fixture.roots {
            assert!(
                !root.try_exists().unwrap(),
                "retained root after close: {root}"
            );
        }
    }
    assert_eq!(cases.len(), 48);
    assert_eq!(steps, 100);
}
