use serde::{Deserialize, Serialize};

use crate::{IntegrityCheckpoint, ReservationId, Selection};

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
    RunProcess,
    AnalyzeFile,
    ReadCandidate,
    LoadSession,
    BeginSession,
    PersistResult,
    FinishSession,
    EmitOutput,
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateWorker {
    pub id: EffectId,
    pub preflight_id: EffectId,
    pub reservation_id: ReservationId,
    pub granted_allowance: u64,
    pub worker: u32,
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
