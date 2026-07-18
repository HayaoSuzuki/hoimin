use serde::{Deserialize, Serialize};

use crate::EffectId;

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
    TargetsResolved,
    PreflightCompleted,
    WorkerCreated,
    ProcessFinished,
    AnalysisFinished,
    CandidateLoaded,
    MutationApplied,
    WorkerReset,
    SessionLoaded,
    SessionStarted,
    ResultPersisted,
    SessionFinished,
    OutputEmitted,
    CleanupFinished,
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectFailed {
    pub id: EffectId,
    pub message: String,
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
