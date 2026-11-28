use std::collections::HashSet;
use std::fs;
use std::io;
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    ApplyMutation, CandidateDescriptor, CandidateIdentity, CreateWorker, EffectFailed,
    EffectFailure, EffectId, MutationCandidate, ResetWorker, RunEvent, stable_mutant_id,
};
use serde::Deserialize;

use super::super::WorkspaceTask;
use super::{WorkspaceHandler, WorkspaceOwnership, candidate, prepared_handler};
use crate::workspace::test_file_size_limit::FileSizeLimit;

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/workspace-mutation-write.jsonl");
const TARGET: &str = "pkg/a.py";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    snapshot: String,
    read_only: bool,
    candidate: Candidate,
    actions: Vec<Action>,
    expected: Vec<Observation>,
}

#[derive(Deserialize)]
struct Candidate {
    start: u64,
    original: String,
    replacement: String,
    line: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Action {
    Apply { limit: Option<libc::rlim_t> },
    Reset { limit: Option<libc::rlim_t> },
    Create,
}

impl Action {
    const fn limit(self) -> Option<libc::rlim_t> {
        match self {
            Self::Apply { limit } | Self::Reset { limit } => limit,
            Self::Create => None,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Applied,
    WriteFailed,
    HashMismatch,
    Reset,
    RestoreFailed,
    Created,
    Missing,
}

impl From<RunEvent> for Outcome {
    fn from(event: RunEvent) -> Self {
        let failure = match event {
            RunEvent::MutationApplied(_) => return Self::Applied,
            RunEvent::WorkerReset(_) => return Self::Reset,
            RunEvent::WorkerCreated(_) => return Self::Created,
            RunEvent::EffectFailed(event) => event.failure,
            other => panic!("unexpected effect result: {other:?}"),
        };
        let limit_error = io::Error::from_raw_os_error(libc::EFBIG).to_string();
        match failure {
            EffectFailure::WorkerMissing { worker: 0 } => Self::Missing,
            EffectFailure::InvalidMutation {
                code,
                path,
                message,
            } if code == "workspace.mutation.invalid"
                && path == TARGET
                && message.contains("hash does not match") =>
            {
                Self::HashMismatch
            }
            EffectFailure::Io {
                operation,
                path: Some(path),
                message,
                ..
            } if operation == "write mutation target"
                && path == TARGET
                && message.contains(&limit_error) =>
            {
                Self::WriteFailed
            }
            EffectFailure::WorkspaceRestore { message, .. }
                if message.contains("write worker file") && message.contains(&limit_error) =>
            {
                Self::RestoreFailed
            }
            other => panic!("unexpected worker failure: {other:?}"),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct FileState {
    bytes: Vec<u8>,
    read_only: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct View {
    registered: Option<usize>,
    live_root: Option<usize>,
    slots: usize,
    copy_bytes: u64,
    pending: usize,
    file: Option<FileState>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Observation {
    outcome: Outcome,
    prepared: Option<View>,
    executed: Option<View>,
    accepted: View,
}

enum Request {
    Apply(ApplyMutation),
    Reset(ResetWorker),
    Create(CreateWorker),
}

impl Request {
    fn execute(self, handler: &mut WorkspaceHandler) -> RunEvent {
        match self {
            Self::Apply(request) => handler
                .handle_apply_mutation(request)
                .map_or_else(RunEvent::EffectFailed, RunEvent::MutationApplied),
            Self::Reset(request) => handler
                .handle_reset_worker(request)
                .map_or_else(RunEvent::EffectFailed, RunEvent::WorkerReset),
            Self::Create(request) => handler
                .handle_create_worker(request)
                .map_or_else(RunEvent::EffectFailed, RunEvent::WorkerCreated),
        }
    }

    fn prepare(self, handler: &mut WorkspaceHandler) -> Result<WorkspaceTask, EffectFailed> {
        match self {
            Self::Apply(request) => handler.prepare_apply_task(request),
            Self::Reset(request) => handler.prepare_reset_task(request),
            Self::Create(request) => handler.prepare_create_task(request),
        }
    }
}

fn with_file_limit<T>(limit: Option<libc::rlim_t>, run: impl FnOnce() -> T) -> T {
    let _limit = limit.map(FileSizeLimit::start);
    run()
}

struct Fixture {
    handler: WorkspaceHandler,
    create: CreateWorker,
    candidate: MutationCandidate,
    roots: Vec<Utf8PathBuf>,
    project: tempfile::TempDir,
}

impl Fixture {
    fn new(case: &Case) -> Self {
        let (project, handler, create) = prepared_handler();
        assert_eq!(
            fs::read(project.path().join(TARGET)).unwrap(),
            case.snapshot.as_bytes()
        );
        let root = handler.worker(0).unwrap().root().to_owned();
        if case.read_only {
            let file = root.join(TARGET);
            let mut permissions = fs::metadata(&file).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(file, permissions).unwrap();
        }
        let mut candidate = candidate(&handler);
        let input = &case.candidate;
        candidate.span.start = input.start;
        candidate.span.length = u64::try_from(input.original.len()).unwrap();
        candidate.original.clone_from(&input.original);
        candidate.replacement.clone_from(&input.replacement);
        candidate.line = input.line;
        candidate.id = stable_mutant_id(&CandidateIdentity::from(&CandidateDescriptor::from(
            &candidate,
        )))
        .to_string();
        Self {
            handler,
            create,
            candidate,
            roots: vec![root],
            project,
        }
    }

    fn record_root(roots: &mut Vec<Utf8PathBuf>, path: &Utf8Path) -> usize {
        if let Some(index) = roots.iter().position(|known| known == path) {
            return index;
        }
        let index = roots.len();
        roots.push(path.to_owned());
        index
    }

    fn view(&mut self) -> View {
        let registered = self
            .handler
            .worker(0)
            .map(|worker| Self::record_root(&mut self.roots, worker.root()));
        assert_eq!(
            self.handler.worker_count(),
            usize::from(registered.is_some())
        );
        let mut roots = self
            .roots
            .iter()
            .enumerate()
            .filter(|(_, path)| path.try_exists().unwrap());
        let live = roots.next();
        assert!(roots.next().is_none(), "multiple worker roots survived");
        let file = live.map(|(_, root)| {
            let path = root.join(TARGET);
            FileState {
                bytes: fs::read(&path).unwrap(),
                read_only: fs::metadata(&path).unwrap().permissions().readonly(),
            }
        });
        View {
            registered,
            live_root: live.map(|(index, _)| index),
            slots: self.handler.materialized_worker_slots(),
            copy_bytes: self.handler.observed_copy_bytes(),
            pending: self.handler.pending_cleanup_count(),
            file,
        }
    }

    fn run_owned(
        &mut self,
        request: Request,
        limit: Option<libc::rlim_t>,
    ) -> (RunEvent, Option<View>, Option<View>) {
        let task = match request.prepare(&mut self.handler) {
            Ok(task) => task,
            Err(error) => return (RunEvent::EffectFailed(error), None, None),
        };
        let prepared = self.view();
        let completion = with_file_limit(limit, || task.execute());
        match &completion.ownership {
            WorkspaceOwnership::Active(_, worker)
            | WorkspaceOwnership::PendingCleanup(_, worker) => {
                Self::record_root(&mut self.roots, worker.root());
            }
            WorkspaceOwnership::None => {}
        }
        let executed = self.view();
        let event = self.handler.accept_task_completion(completion).unwrap();
        (event, Some(prepared), Some(executed))
    }

    fn run(&mut self, action: Action, id: EffectId, owned: bool) -> Observation {
        let request = match action {
            Action::Apply { .. } => Request::Apply(ApplyMutation {
                id,
                worker: 0,
                candidate: self.candidate.clone(),
            }),
            Action::Reset { .. } => Request::Reset(ResetWorker { id, worker: 0 }),
            Action::Create => Request::Create(self.create.clone()),
        };
        let (event, prepared, executed) = if owned {
            self.run_owned(request, action.limit())
        } else {
            (
                with_file_limit(action.limit(), || request.execute(&mut self.handler)),
                None,
                None,
            )
        };
        Observation {
            outcome: event.into(),
            accepted: self.view(),
            prepared,
            executed,
        }
    }
}

#[test]
fn mutation_write_limit_matches_lean() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "workspace::task_tests::mutation_write_oracle::replay_mutation_write_history_child",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("workspace_mutation_write_replayed=288 transitions=1160"),
        "{stdout}"
    );
}

#[test]
#[ignore = "process-wide file-size limit; invoked in an isolated subprocess"]
fn replay_mutation_write_history_child() {
    let mut ids = HashSet::new();
    let mut transitions = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert_eq!(case.actions.len(), case.expected.len());
        assert!(ids.insert(case.id.clone()));
        for owned in [false, true] {
            let mut fixture = Fixture::new(&case);
            for (index, (action, expected)) in
                case.actions.iter().copied().zip(&case.expected).enumerate()
            {
                let observed = fixture.run(action, EffectId(10 + index as u64), owned);
                let context = (&case.id, index + 1, action, owned);
                assert_eq!(observed.outcome, expected.outcome, "{context:?}");
                assert_eq!(observed.accepted, expected.accepted, "{context:?}");
                if owned {
                    assert_eq!(observed.prepared, expected.prepared, "{context:?}");
                    assert_eq!(observed.executed, expected.executed, "{context:?}");
                }
                transitions += 1;
            }
            assert_eq!(
                fs::read(fixture.project.path().join(TARGET)).unwrap(),
                case.snapshot.as_bytes(),
                "{} original source",
                case.id
            );
            fixture.handler.close().unwrap();
            assert!(
                fixture.roots.iter().all(|root| !root.try_exists().unwrap()),
                "{} cleanup",
                case.id
            );
        }
    }
    assert_eq!((ids.len(), transitions), (144, 1160));
    println!(
        "workspace_mutation_write_replayed={} transitions={transitions}",
        ids.len() * 2
    );
}
