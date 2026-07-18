use serde::{Deserialize, Serialize};

use camino::Utf8PathBuf;

use crate::{EffectId, IntegrityCheckpoint, ReservationId, TargetSlice};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartRequested;

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

completion_event!(
    ProcessFinished,
    AnalysisFinished,
    CandidateLoaded,
    SessionLoaded,
    SessionStarted,
    ResultPersisted,
    SessionFinished,
    OutputEmitted,
);

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
    Other {
        code: String,
        message: String,
    },
}

impl EffectFailure {
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
            | Self::Other { code, .. } => code,
            Self::WorkerMissing { .. } => "workspace.worker.missing",
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
    AnalysisFinished(AnalysisFinished),
    CandidateLoaded(CandidateLoaded),
    MutationApplied(MutationApplied),
    MutantFinished(ProcessFinished),
    WorkerReset(WorkerReset),
    OriginalsVerified(OriginalsVerified),
    SessionLoaded(SessionLoaded),
    SessionStarted(SessionStarted),
    ResultPersisted(ResultPersisted),
    SessionFinished(SessionFinished),
    OutputEmitted(OutputEmitted),
    CleanupFinished(CleanupFinished),
    EffectFailed(EffectFailed),
    DeadlineReached,
    CancellationRequested,
}
