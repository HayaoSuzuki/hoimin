use serde::{Deserialize, Serialize};

use camino::Utf8PathBuf;

use crate::{
    CandidateSpoolRef, CommandArg, IntegrityCheckpoint, MutantResult, MutationCandidate,
    OutputEvent, ProcessLimits, ReservationId, RunFingerprint, Selection, TargetSlice,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct EffectId(pub u64);

macro_rules! effect_request {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
            pub struct $name {
                pub id: EffectId,
            }
        )+
    };
}

effect_request!(Preflight, ObserveRemainingBudget);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeFile {
    pub id: EffectId,
    pub target: TargetSlice,
    pub final_target: bool,
    pub max_candidates: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadCandidate {
    pub id: EffectId,
    pub worker: u32,
    pub spool: CandidateSpoolRef,
    pub offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoadSession {
    pub id: EffectId,
    pub fingerprint: RunFingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LookupStoredResult {
    pub id: EffectId,
    pub worker: u32,
    pub run_id: String,
    pub mutant_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BeginSession {
    pub id: EffectId,
    pub run_id: String,
    pub fingerprint: RunFingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PersistResult {
    pub id: EffectId,
    pub worker: u32,
    pub result: MutantResult,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FinishSession {
    pub id: EffectId,
    pub run_id: String,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmitOutput {
    pub id: EffectId,
    pub event: OutputEvent,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunProcess {
    pub id: EffectId,
    /// Workspace worker for baseline/mutant processes. Runtime probes use None.
    pub worker: Option<u32>,
    pub run_id: Option<String>,
    pub mutant_id: Option<String>,
    pub argv: Vec<CommandArg>,
    pub cwd: Utf8PathBuf,
    pub limits: ProcessLimits,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateWorker {
    id: EffectId,
    preflight_id: EffectId,
    reservation_id: ReservationId,
    granted_allowance: u64,
    worker: u32,
}

impl CreateWorker {
    pub(crate) fn from_workspace_grant(
        id: EffectId,
        preflight_id: EffectId,
        reservation_id: ReservationId,
        granted_allowance: u64,
        worker: u32,
    ) -> Self {
        Self {
            id,
            preflight_id,
            reservation_id,
            granted_allowance,
            worker,
        }
    }

    #[must_use]
    pub fn id(&self) -> EffectId {
        self.id
    }

    #[must_use]
    pub fn preflight_id(&self) -> EffectId {
        self.preflight_id
    }

    #[must_use]
    pub fn reservation_id(&self) -> ReservationId {
        self.reservation_id
    }

    #[must_use]
    pub fn granted_allowance(&self) -> u64 {
        self.granted_allowance
    }

    #[must_use]
    pub fn worker(&self) -> u32 {
        self.worker
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyMutation {
    pub id: EffectId,
    pub worker: u32,
    pub candidate: MutationCandidate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResetWorker {
    pub id: EffectId,
    pub worker: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerifyOriginals {
    pub id: EffectId,
    pub checkpoint: IntegrityCheckpoint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Cleanup {
    pub id: EffectId,
    pub reservations: Vec<ReservationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolveTargets {
    pub id: EffectId,
    pub selection: Selection,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum RunEffect {
    ResolveTargets(ResolveTargets),
    Preflight(Preflight),
    #[serde(skip_deserializing)]
    CreateWorker(CreateWorker),
    RunBaseline(RunProcess),
    ObserveRemainingBudget(ObserveRemainingBudget),
    AnalyzeFile(AnalyzeFile),
    ReadCandidate(ReadCandidate),
    ApplyMutation(ApplyMutation),
    RunMutant(RunProcess),
    ResetWorker(ResetWorker),
    VerifyOriginals(VerifyOriginals),
    LoadSession(LoadSession),
    LookupStoredResult(LookupStoredResult),
    BeginSession(BeginSession),
    PersistResult(PersistResult),
    FinishSession(FinishSession),
    EmitOutput(EmitOutput),
    Cleanup(Cleanup),
}

impl RunEffect {
    #[must_use]
    pub fn id(&self) -> EffectId {
        match self {
            Self::ResolveTargets(value) => value.id,
            Self::Preflight(value) => value.id,
            Self::CreateWorker(value) => value.id(),
            Self::RunBaseline(value) | Self::RunMutant(value) => value.id,
            Self::ObserveRemainingBudget(value) => value.id,
            Self::AnalyzeFile(value) => value.id,
            Self::ReadCandidate(value) => value.id,
            Self::ApplyMutation(value) => value.id,
            Self::ResetWorker(value) => value.id,
            Self::VerifyOriginals(value) => value.id,
            Self::LoadSession(value) => value.id,
            Self::LookupStoredResult(value) => value.id,
            Self::BeginSession(value) => value.id,
            Self::PersistResult(value) => value.id,
            Self::FinishSession(value) => value.id,
            Self::EmitOutput(value) => value.id,
            Self::Cleanup(value) => value.id,
        }
    }
}
