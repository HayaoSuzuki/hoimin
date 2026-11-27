use std::collections::HashSet;
use std::fs;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{
    ByteSpan, CandidateDescriptor, CandidateIdentity, MutationCandidate, stable_mutant_id,
};
use serde::Deserialize;

use super::super::{WorkerWorkspace, WorkspaceError};
use super::{make_read_only, source_worker};
use crate::source_file::{WorkspaceRaceHook, install_workspace_race_hook};

const CORPUS: &str =
    include_str!("../../../../../../formal/HoiminOracle/corpus/workspace-mutation.jsonl");
const TARGET: &str = "source.py";
const UNTRACKED: &str = "untracked.py";
const RESERVE: &str = "worker.mutation.reserve";
const LINKS: &str = "worker.mutation.links";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    snapshot: Vec<u8>,
    request: Request,
    worker: Option<FileState>,
    fault: Option<Fault>,
    resize: Option<u64>,
    shared: bool,
    unix_only: bool,
    first: Observation,
    retry: Observation,
}

#[derive(Deserialize)]
struct Request {
    tracked: bool,
    current_hash: bool,
    current_id: bool,
    start: u64,
    length: u64,
    original: String,
    replacement: String,
    operator: String,
    line: u32,
    column: u32,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct FileState {
    bytes: Vec<u8>,
    read_only: bool,
}

fn observe_file(path: &Utf8Path) -> Option<FileState> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            assert!(metadata.is_file(), "{path}");
            Some(FileState {
                bytes: fs::read(path).unwrap(),
                read_only: metadata.permissions().readonly(),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => panic!("inspect {path}: {error}"),
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Fault {
    Reserve,
    BeforeRepair,
    AfterRepair,
}

impl Fault {
    const fn boundary(self) -> (&'static str, usize) {
        match self {
            Self::Reserve => (RESERVE, 0),
            Self::BeforeRepair => (LINKS, 0),
            Self::AfterRepair => (LINKS, 1),
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Applied,
    NotSelected,
    Missing,
    Hash,
    Original,
    Span,
    Shared,
    ReserveFault,
    BeforeRepairFault,
    AfterRepairFault,
}

fn outcome(
    result: Result<(), WorkspaceError>,
    path: &Utf8Path,
    missing: bool,
    fault: Option<Fault>,
) -> Outcome {
    match result {
        Ok(()) => Outcome::Applied,
        Err(WorkspaceError::MutationTargetMissing { path: observed }) if observed == path => {
            Outcome::NotSelected
        }
        Err(WorkspaceError::MutationHashMismatch { path: observed }) if observed == path => {
            Outcome::Hash
        }
        Err(WorkspaceError::MutationOriginalMismatch { path: observed }) if observed == path => {
            Outcome::Original
        }
        Err(WorkspaceError::MutationSpanInvalid { path: observed }) if observed == path => {
            Outcome::Span
        }
        Err(WorkspaceError::MutationTargetShared { path: observed }) if observed == path => {
            Outcome::Shared
        }
        Err(WorkspaceError::Io {
            operation: "open mutation target",
            path: observed,
            ..
        }) if observed == path && missing => Outcome::Missing,
        Err(WorkspaceError::Io {
            operation: "read mutation target",
            path: observed,
            message,
        }) if observed == path && message.contains(RESERVE) => Outcome::ReserveFault,
        Err(WorkspaceError::Io {
            operation: "inspect mutation target links",
            path: observed,
            message,
        }) if observed == path && message.contains(LINKS) => match fault {
            Some(Fault::BeforeRepair) => Outcome::BeforeRepairFault,
            Some(Fault::AfterRepair) => Outcome::AfterRepairFault,
            _ => panic!("unexpected link fault: {message}"),
        },
        Err(other) => panic!("unexpected mutation result: {other:?}"),
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Observation {
    outcome: Outcome,
    file: Option<FileState>,
    bytes_read: Option<u64>,
    fault_triggered: bool,
}

struct ReadObservation {
    target: Utf8PathBuf,
    resize: Option<u64>,
    bytes_read: AtomicU64,
}

impl WorkspaceRaceHook for ReadObservation {
    fn parent_opened(&self, operation: &'static str, _: &Utf8Path) {
        if operation == "mutation-budget"
            && let Some(size) = self.resize
        {
            fs::OpenOptions::new()
                .write(true)
                .open(&self.target)
                .unwrap()
                .set_len(size)
                .unwrap();
        }
    }

    fn mutation_bytes_read(&self, bytes: u64) {
        self.bytes_read.store(bytes, Ordering::Relaxed);
    }
}

fn apply(
    worker: &mut WorkerWorkspace,
    candidate: &MutationCandidate,
    fault: Option<Fault>,
    resize: Option<u64>,
) -> Observation {
    let target = worker.root().join(&candidate.path);
    let observation = Arc::new(ReadObservation {
        target: target.clone(),
        resize,
        bytes_read: AtomicU64::new(u64::MAX),
    });
    let observe_read = install_workspace_race_hook(observation.clone());
    let fault_guard = fault.map(|choice| {
        let (point, skip) = choice.boundary();
        crate::io_faults::arm_after(point, skip)
    });
    let result = worker.apply_mutation(candidate);
    let fault_triggered = if let Some(guard) = fault_guard {
        guard.triggered()
    } else {
        false
    };
    drop(observe_read);
    let read = observation.bytes_read.load(Ordering::Relaxed);
    let file = observe_file(&target);
    Observation {
        outcome: outcome(result, &candidate.path, file.is_none(), fault),
        file,
        bytes_read: (read != u64::MAX).then_some(read),
        fault_triggered,
    }
}

fn set_up_candidate(case: &Case) -> (tempfile::TempDir, WorkerWorkspace, MutationCandidate) {
    let input = &case.request;
    let (project, worker, mut candidate) = source_worker(
        &case.snapshot,
        ByteSpan {
            start: input.start,
            length: input.length,
        },
        &input.original,
        &input.replacement,
        (input.line, input.column),
    );
    if !input.tracked {
        candidate.path = UNTRACKED.into();
    }
    if !input.current_hash {
        candidate.file_hash = "0".repeat(64);
    }
    candidate.operator.clone_from(&input.operator);
    candidate.id = if input.current_id {
        stable_mutant_id(&CandidateIdentity::from(&CandidateDescriptor::from(
            &candidate,
        )))
        .to_string()
    } else {
        "stale-id".into()
    };
    (project, worker, candidate)
}

fn set_up_file(target: &Utf8Path, sentinel: &Utf8Path, state: Option<&FileState>, shared: bool) {
    if let Some(state) = state {
        let file = if shared { sentinel } else { target };
        fs::write(file, &state.bytes).unwrap();
        if state.read_only {
            make_read_only(file);
        }
        if shared {
            fs::remove_file(target).unwrap();
            fs::hard_link(sentinel, target).unwrap();
        }
    } else if target.try_exists().unwrap() {
        fs::remove_file(target).unwrap();
    }
}

#[test]
fn workspace_mutation_matches_lean_oracle() {
    let mut ids = HashSet::new();
    let mut checked = 0;
    for line in CORPUS.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        assert_eq!(case.schema, 1);
        assert_eq!(case.mode, "internal-fixture");
        assert!(ids.insert(case.id.clone()));
        if case.unix_only && !cfg!(unix) {
            continue;
        }
        let (project, mut worker, candidate) = set_up_candidate(&case);
        let outside = tempfile::tempdir().unwrap();
        let sentinel = Utf8Path::from_path(outside.path())
            .unwrap()
            .join("external.py");
        let target = worker.root().join(&candidate.path);
        set_up_file(&target, &sentinel, case.worker.as_ref(), case.shared);
        assert_eq!(observe_file(&target), case.worker, "{} setup", case.id);

        let first = apply(&mut worker, &candidate, case.fault, case.resize);
        assert_eq!(first, case.first, "{} first mutation", case.id);
        let retry = apply(&mut worker, &candidate, None, None);
        assert_eq!(retry, case.retry, "{} retry", case.id);
        if !case.request.tracked {
            assert_eq!(
                worker.read(TARGET).unwrap(),
                case.snapshot,
                "{} tracked file",
                case.id
            );
        }
        if case.shared {
            assert_eq!(
                observe_file(&sentinel),
                case.worker,
                "{} shared referent",
                case.id
            );
        }
        worker.reset().unwrap();
        assert_eq!(
            worker.read(TARGET).unwrap(),
            case.snapshot,
            "{} reset",
            case.id
        );
        assert!(
            !worker.root().join(UNTRACKED).try_exists().unwrap(),
            "{} reset untracked",
            case.id
        );
        assert_eq!(
            fs::read(project.path().join(TARGET)).unwrap(),
            case.snapshot,
            "{} original source",
            case.id
        );
        if case.shared {
            assert_eq!(
                observe_file(&sentinel),
                case.worker,
                "{} shared referent after reset",
                case.id
            );
        }
        checked += 1;
    }
    assert_eq!(ids.len(), 111);
    assert_eq!(checked, if cfg!(unix) { 111 } else { 110 });
}
