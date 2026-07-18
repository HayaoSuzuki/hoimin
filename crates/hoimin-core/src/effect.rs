use serde::{Deserialize, Serialize};

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
    ResolveTargets,
    Preflight,
    CreateWorker,
    RunProcess,
    AnalyzeFile,
    ReadCandidate,
    ApplyMutation,
    ResetWorker,
    LoadSession,
    BeginSession,
    PersistResult,
    FinishSession,
    EmitOutput,
    Cleanup,
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    LoadSession(LoadSession),
    BeginSession(BeginSession),
    PersistResult(PersistResult),
    FinishSession(FinishSession),
    EmitOutput(EmitOutput),
    Cleanup(Cleanup),
}
