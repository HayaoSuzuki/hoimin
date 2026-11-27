use std::collections::BTreeSet;
use std::convert::Infallible;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use hoimin_core::{DiskCleanupStatus, EffectId};
use serde::Deserialize;

use super::{
    PreflightLimits, ValidatedPreflightError, WorkspacePlan, reset_snapshot_write_metrics,
    snapshot_write_metrics,
};
use crate::source_file::{WorkspaceRaceHook, install_workspace_race_hook};
use crate::workspace::{
    CopyOptions, ManagedRootCoordinator, ManagedRunRoot, OwnerKind, WorkspaceError,
};

const CORPUS: &str =
    include_str!("../../../../../formal/HoiminOracle/corpus/workspace-capacity.jsonl");
const TARGET: &str = "target.py";
const ADDED: &str = "added.py";

#[derive(Deserialize)]
struct Case {
    schema: u64,
    mode: String,
    id: String,
    #[serde(flatten)]
    phase: Phase,
}

#[derive(Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
enum Phase {
    Arithmetic {
        logical: u64,
        workers: u32,
        limits: Limits,
        expected: Admission,
    },
    Live {
        initial: u64,
        workers: u32,
        limits: Limits,
        duplicate: bool,
        edit: Option<Edit>,
        expected: Observation,
    },
}

#[derive(Deserialize)]
struct Limits {
    copy: Option<u64>,
    owned: Option<u64>,
}

impl From<Limits> for PreflightLimits {
    fn from(limits: Limits) -> Self {
        Self {
            copy: limits.copy,
            owned: limits.owned,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Admission {
    Admit { aggregate: u64 },
    ZeroWorkers,
    Overflow,
    CopyLimit { requested: u64, limit: u64 },
    OwnedLimit { requested: u64, limit: u64 },
    Grew,
    Shrank,
    OriginalChanged,
}

impl From<WorkspaceError> for Admission {
    fn from(error: WorkspaceError) -> Self {
        match error {
            WorkspaceError::ZeroWorkers => Self::ZeroWorkers,
            WorkspaceError::CopySizeOverflow => Self::Overflow,
            WorkspaceError::CopyAllowanceExceeded {
                observed,
                allowance,
            } => Self::CopyLimit {
                requested: observed,
                limit: allowance,
            },
            WorkspaceError::OwnedWorkspaceLimit { planned, limit } => Self::OwnedLimit {
                requested: planned,
                limit,
            },
            WorkspaceError::OriginalChanged { path } => {
                assert_eq!(path, TARGET);
                Self::OriginalChanged
            }
            WorkspaceError::Io {
                operation: "read manifest file",
                path,
                message,
            } => {
                assert_eq!(path, TARGET);
                if message.contains("file grew") {
                    Self::Grew
                } else if message.contains("file shrank") {
                    Self::Shrank
                } else {
                    panic!("unexpected descriptor error: {message}");
                }
            }
            other => panic!("unexpected workspace error: {other}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Edit {
    BeforeSnapshot { bytes: u64 },
    AddFile,
    BeforeDescriptor { bytes: u64 },
    AfterDescriptor { bytes: u64 },
    Validation { bytes: u64 },
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Observation {
    admission: Admission,
    snapshot_started: bool,
    writes: u64,
    written_bytes: u64,
    validated_bytes: Option<u64>,
    source_bytes: u64,
    added: bool,
}

fn resize(path: &Utf8Path, bytes: u64) -> io::Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .set_len(bytes)
}

struct AfterDescriptor {
    file: Utf8PathBuf,
    bytes: u64,
}

impl WorkspaceRaceHook for AfterDescriptor {
    fn parent_opened(&self, operation: &'static str, path: &Utf8Path) {
        if operation == "snapshot-budget" && path == TARGET {
            resize(&self.file, self.bytes).unwrap();
        }
    }
}

fn inventory(root: &ManagedRunRoot) -> BTreeSet<OsString> {
    fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}

fn observe_published(
    case: &str,
    plan: &WorkspacePlan,
    source: &Utf8Path,
    added: bool,
    validated_bytes: Option<u64>,
) -> Admission {
    let files: &[&str] = if added { &[TARGET, ADDED] } else { &[TARGET] };
    for file in files {
        assert_eq!(
            fs::read(plan.snapshot.root.join(file)).unwrap(),
            fs::read(source.join(file)).unwrap(),
            "{case}: {file}"
        );
    }
    let completed = plan.completed();
    assert_eq!(
        Some(completed.per_worker_logical_bytes),
        validated_bytes,
        "{case}"
    );
    Admission::Admit {
        aggregate: completed.aggregate_logical_bytes,
    }
}

fn observe_live(
    case: &str,
    initial: u64,
    workers: u32,
    limits: PreflightLimits,
    duplicate: bool,
    edit: Option<Edit>,
) -> Observation {
    let project = tempfile::tempdir().unwrap();
    let source = Utf8Path::from_path(project.path()).unwrap();
    let file = source.join(TARGET);
    let added_file = source.join(ADDED);
    fs::write(&file, vec![b'x'; usize::try_from(initial).unwrap()]).unwrap();
    let parent = tempfile::tempdir().unwrap();
    let coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
    let managed =
        Arc::new(ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
    let before = inventory(&managed);

    let boundary = if matches!(edit, Some(Edit::BeforeDescriptor { .. })) {
        "snapshot.metadata"
    } else {
        "copy.snapshot.root"
    };
    let action_file = file.clone();
    let action_added = added_file.clone();
    let observe_start = crate::io_faults::arm_action(boundary, 0, move || match edit {
        Some(Edit::BeforeSnapshot { bytes } | Edit::BeforeDescriptor { bytes }) => {
            resize(&action_file, bytes)
        }
        Some(Edit::AddFile) => fs::copy(&action_file, action_added).map(|_| ()),
        _ => Ok(()),
    });
    let _after_descriptor = if let Some(Edit::AfterDescriptor { bytes }) = edit {
        Some(install_workspace_race_hook(Arc::new(AfterDescriptor {
            file: file.clone(),
            bytes,
        })))
    } else {
        None
    };
    let mut validated_bytes = None;
    reset_snapshot_write_metrics();
    let result = WorkspacePlan::preflight_validated_in(
        source,
        EffectId(1),
        workers,
        CopyOptions {
            includes: if duplicate {
                vec![TARGET.into(), TARGET.into()]
            } else {
                Vec::new()
            },
            ..CopyOptions::default()
        },
        Some(Arc::clone(&managed)),
        limits,
        |_, manifest| -> Result<(), Infallible> {
            assert!(
                validated_bytes.replace(manifest.logical_bytes()).is_none(),
                "{case}"
            );
            if let Some(Edit::Validation { bytes }) = edit {
                resize(&file, bytes).unwrap();
            }
            Ok(())
        },
    );
    let snapshot_started = observe_start.triggered();
    drop(observe_start);
    let (writes, written_bytes) = snapshot_write_metrics();
    let live = inventory(&managed);
    assert!(before.is_subset(&live), "{case}: removed an existing child");
    assert_eq!(
        live.len() - before.len(),
        usize::from(result.is_ok()),
        "{case}: {result:?}"
    );

    let added = added_file.try_exists().unwrap();
    let admission = match result {
        Ok(plan) => observe_published(case, &plan, source, added, validated_bytes),
        Err(ValidatedPreflightError::Workspace(error)) => error.into(),
        Err(ValidatedPreflightError::Validation(never)) => match never {},
    };
    assert_eq!(
        managed
            .cleanup(Duration::from_secs(1))
            .disposition
            .report_status(),
        DiskCleanupStatus::Clean,
        "{case}: run cleanup failed"
    );
    assert!(
        !managed.path().try_exists().unwrap(),
        "{case}: run root leaked"
    );
    Observation {
        admission,
        snapshot_started,
        writes,
        written_bytes,
        validated_bytes,
        source_bytes: fs::metadata(&file).unwrap().len(),
        added,
    }
}

#[test]
fn workspace_capacity_matches_lean_oracle() {
    let mut ids = BTreeSet::new();
    let mut arithmetic = 0;
    let mut live = 0;
    for line in CORPUS.lines() {
        let Case {
            schema,
            mode,
            id,
            phase,
        } = serde_json::from_str(line).unwrap();
        assert_eq!(schema, 1);
        assert_eq!(mode, "internal-fixture");
        match phase {
            Phase::Arithmetic {
                logical,
                workers,
                limits,
                expected,
            } => {
                let actual = PreflightLimits::from(limits)
                    .check(logical, workers)
                    .map_or_else(Admission::from, |aggregate| Admission::Admit { aggregate });
                assert_eq!(actual, expected, "{id}");
                arithmetic += 1;
            }
            Phase::Live {
                initial,
                workers,
                limits,
                duplicate,
                edit,
                expected,
            } => {
                let actual = observe_live(&id, initial, workers, limits.into(), duplicate, edit);
                assert_eq!(actual, expected, "{id}");
                live += 1;
            }
        }
        assert!(ids.insert(id));
    }
    assert_eq!((arithmetic, live), (72, 30));
}
