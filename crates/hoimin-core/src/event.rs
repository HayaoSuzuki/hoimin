use serde::{Deserialize, Serialize};

use camino::Utf8PathBuf;

use std::time::Duration;

use crate::{
    CandidateCursor, CandidateSpoolRef, DiskFailure, EffectId, IntegrityCheckpoint,
    MutationCandidate, OutputSpoolRef, ProcessTermination, ReservationId, ResourceMode,
    RunFingerprint, SessionResumeRef, StoredResult, TargetSlice,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartRequested;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskStopRequested {
    pub failure: DiskFailure,
}

macro_rules! completion_event {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
            pub struct $name {
                pub id: EffectId,
            }
        )+
    };
}

completion_event!(OutputEmitted,);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemainingBudgetObserved {
    pub id: EffectId,
    pub remaining: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisFinished {
    pub id: EffectId,
    pub spool: Option<CandidateSpoolRef>,
    pub truncated: bool,
    #[serde(default)]
    pub diagnostics: Vec<AnalysisDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateLoaded {
    pub id: EffectId,
    pub worker: u32,
    pub candidate: Option<MutationCandidate>,
    pub next_cursor: CandidateCursor,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionLoaded {
    pub id: EffectId,
    pub resume: Option<SessionResumeRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredResultLoaded {
    pub id: EffectId,
    pub worker: u32,
    pub result: Option<StoredResult>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionStarted {
    pub id: EffectId,
    pub run_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResultPersisted {
    pub id: EffectId,
    pub worker: u32,
    pub run_id: String,
    pub mutant_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionFinished {
    pub id: EffectId,
    pub run_id: String,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOutputState {
    #[default]
    Complete,
    CloseTimedOut,
}

pub const PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE: &str = "process.output.close.timeout";

impl ProcessOutputState {
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessFinished {
    pub id: EffectId,
    pub worker: Option<u32>,
    pub termination: ProcessTermination,
    #[serde(default)]
    pub output_state: ProcessOutputState,
    pub output: OutputSpoolRef,
    pub elapsed: Duration,
    pub resource_mode: ResourceMode,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkerCreated {
    pub id: EffectId,
    pub worker: u32,
    pub reservation_id: ReservationId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationApplied {
    pub id: EffectId,
    pub worker: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkerReset {
    pub id: EffectId,
    pub worker: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OriginalsVerified {
    pub id: EffectId,
    pub checkpoint: IntegrityCheckpoint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreflightCompleted {
    pub id: EffectId,
    pub per_worker_logical_bytes: u64,
    pub requested_workers: u32,
    pub aggregate_logical_bytes: u64,
    #[serde(default)]
    pub fingerprint: Option<RunFingerprint>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CleanupFinished {
    pub id: EffectId,
    pub released_reservations: Vec<ReservationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TargetsResolved {
    pub id: EffectId,
    pub targets: Vec<TargetSlice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EffectFailure {
    WorkspaceRestore {
        path: Utf8PathBuf,
        message: String,
    },
    OriginalChanged {
        path: Utf8PathBuf,
    },
    CopyLimit {
        requested: u64,
        allowance: u64,
    },
    WorkspacePreflightMismatch {
        expected: EffectId,
        received: EffectId,
    },
    WorkspaceWorkerOutOfRange {
        worker: u32,
        requested_workers: u32,
    },
    WorkspaceAllowanceMismatch {
        expected: u64,
        received: u64,
    },
    InvalidWorkspaceGrant {
        expected: ReservationId,
        received: ReservationId,
    },
    InvalidWorkspacePath {
        path: Utf8PathBuf,
    },
    InvalidMutation {
        code: String,
        path: Utf8PathBuf,
        message: String,
    },
    WorkerMissing {
        worker: u32,
    },
    Io {
        code: String,
        operation: String,
        path: Option<Utf8PathBuf>,
        message: String,
    },
    ReportIo {
        operation: String,
        message: String,
    },
    ReportSerialization {
        message: String,
    },
    ReportState {
        message: String,
    },
    SessionDatabase {
        code: String,
        operation: String,
        message: String,
    },
    Other {
        code: String,
        message: String,
    },
}

impl EffectFailure {
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::WorkspaceRestore { .. } => "workspace.restore",
            Self::OriginalChanged { .. } => "workspace.original.changed",
            Self::CopyLimit { .. } => "workspace.copy.limit",
            Self::WorkspacePreflightMismatch { .. } => "workspace.preflight.mismatch",
            Self::WorkspaceWorkerOutOfRange { .. } => "workspace.worker.out_of_range",
            Self::WorkspaceAllowanceMismatch { .. } => "workspace.allowance.mismatch",
            Self::InvalidWorkspaceGrant { .. } => "workspace.grant.invalid",
            Self::InvalidWorkspacePath { .. } => "workspace.path.invalid",
            Self::InvalidMutation { code, .. }
            | Self::Io { code, .. }
            | Self::SessionDatabase { code, .. }
            | Self::Other { code, .. } => code,
            Self::WorkerMissing { .. } => "workspace.worker.missing",
            Self::ReportIo { .. } => "report.io",
            Self::ReportSerialization { .. } => "report.serialization",
            Self::ReportState { .. } => "report.state",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::WorkspaceRestore { path, message } => {
                format!("restore {path}: {message}")
            }
            Self::OriginalChanged { path } => format!("original changed: {path}"),
            Self::CopyLimit {
                requested,
                allowance,
            } => format!("copy requires {requested} bytes, allowance is {allowance}"),
            Self::WorkspacePreflightMismatch { expected, received } => {
                format!("preflight effect mismatch: expected {expected:?}, received {received:?}")
            }
            Self::WorkspaceWorkerOutOfRange {
                worker,
                requested_workers,
            } => format!("worker {worker} is outside requested count {requested_workers}"),
            Self::WorkspaceAllowanceMismatch { expected, received } => {
                format!("workspace allowance mismatch: expected {expected}, received {received}")
            }
            Self::InvalidWorkspaceGrant { expected, received } => {
                format!("workspace grant mismatch: expected {expected:?}, received {received:?}")
            }
            Self::InvalidWorkspacePath { path } => format!("invalid workspace path: {path}"),
            Self::InvalidMutation { path, message, .. } => {
                format!("invalid mutation for {path}: {message}")
            }
            Self::WorkerMissing { worker } => format!("worker {worker} is missing"),
            Self::Io {
                operation,
                path,
                message,
                ..
            } => match path {
                Some(path) => format!("{operation} {path}: {message}"),
                None => format!("{operation}: {message}"),
            },
            Self::ReportIo { operation, message }
            | Self::SessionDatabase {
                operation, message, ..
            } => format!("{operation}: {message}"),
            Self::ReportSerialization { message }
            | Self::ReportState { message }
            | Self::Other { message, .. } => message.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectFailed {
    pub id: EffectId,
    pub failure: EffectFailure,
}

impl EffectFailed {
    pub fn other(id: EffectId, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            failure: EffectFailure::Other {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RunEvent {
    StartRequested(StartRequested),
    TargetsResolved(TargetsResolved),
    PreflightCompleted(PreflightCompleted),
    WorkerCreated(WorkerCreated),
    BaselineFinished(ProcessFinished),
    RemainingBudgetObserved(RemainingBudgetObserved),
    AnalysisFinished(AnalysisFinished),
    CandidateLoaded(CandidateLoaded),
    MutationApplied(MutationApplied),
    MutantFinished(ProcessFinished),
    WorkerReset(WorkerReset),
    OriginalsVerified(OriginalsVerified),
    SessionLoaded(SessionLoaded),
    StoredResultLoaded(StoredResultLoaded),
    SessionStarted(SessionStarted),
    ResultPersisted(ResultPersisted),
    SessionFinished(SessionFinished),
    OutputEmitted(OutputEmitted),
    CleanupFinished(CleanupFinished),
    EffectFailed(EffectFailed),
    DiskStopRequested(DiskStopRequested),
    DeadlineReached,
    CancellationRequested,
}
