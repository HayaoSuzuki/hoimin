use serde::{Deserialize, Serialize};

use camino::Utf8PathBuf;

use crate::{
    CommandArg, IntegrityCheckpoint, MutantResult, OutputEvent, ProcessLimits, ReservationId,
    RunFingerprint, Selection,
};

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

effect_request!(Preflight, AnalyzeFile, ReadCandidate,);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoadSession {
    pub id: EffectId,
    pub fingerprint: RunFingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LookupStoredResult {
    pub id: EffectId,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    LookupStoredResult(LookupStoredResult),
    BeginSession(BeginSession),
    PersistResult(PersistResult),
    FinishSession(FinishSession),
    EmitOutput(EmitOutput),
    Cleanup(Cleanup),
}
