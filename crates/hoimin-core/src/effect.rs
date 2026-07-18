use serde::{Deserialize, Serialize};

use camino::Utf8PathBuf;

use crate::{CommandArg, IntegrityCheckpoint, ProcessLimits, ReservationId, Selection};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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

effect_request!(
    Preflight,
    AnalyzeFile,
    ReadCandidate,
    LoadSession,
    BeginSession,
    PersistResult,
    FinishSession,
    EmitOutput,
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunProcess {
    pub id: EffectId,
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

    pub fn id(&self) -> EffectId {
        self.id
    }

    pub fn preflight_id(&self) -> EffectId {
        self.preflight_id
    }

    pub fn reservation_id(&self) -> ReservationId {
        self.reservation_id
    }

    pub fn granted_allowance(&self) -> u64 {
        self.granted_allowance
    }

    pub fn worker(&self) -> u32 {
        self.worker
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyMutation {
    pub id: EffectId,
    pub worker: u32,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum RunEffect {
    ResolveTargets(ResolveTargets),
    Preflight(Preflight),
    #[serde(skip_deserializing)]
    CreateWorker(CreateWorker),
    RunBaseline(RunProcess),
    AnalyzeFile(AnalyzeFile),
    ReadCandidate(ReadCandidate),
    ApplyMutation(ApplyMutation),
    RunMutant(RunProcess),
    ResetWorker(ResetWorker),
    VerifyOriginals(VerifyOriginals),
    LoadSession(LoadSession),
    BeginSession(BeginSession),
    PersistResult(PersistResult),
    FinishSession(FinishSession),
    EmitOutput(EmitOutput),
    Cleanup(Cleanup),
}
