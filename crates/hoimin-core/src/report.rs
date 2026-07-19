use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    MutationCandidate, MutationStatus, OutputSpoolRef, ProcessTermination, ResourceMode, RunConfig,
    contract_ensure,
};

pub const REPORT_SCHEMA_VERSION: u32 = 2;

#[must_use]
pub fn classify_mutant(termination: ProcessTermination) -> MutationStatus {
    match termination {
        ProcessTermination::Exit(0) => MutationStatus::Survived,
        ProcessTermination::Exit(_) => MutationStatus::Killed,
        ProcessTermination::Timeout => MutationStatus::Timeout,
        ProcessTermination::OutOfMemory => MutationStatus::OutOfMemory,
        ProcessTermination::ProcessLimit => MutationStatus::ProcessLimit,
        ProcessTermination::Cancelled => MutationStatus::NotRun,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MutationSummary {
    pub killed: u64,
    pub survived: u64,
    pub timeout: u64,
    pub out_of_memory: u64,
    pub process_limit: u64,
    pub error: u64,
    pub not_run: u64,
    pub inconclusive: u64,
    pub score: Option<f64>,
}

impl MutationSummary {
    // The public score protocol is f64; mutation counts remain exact u64 values.
    #[allow(clippy::cast_precision_loss)]
    pub fn record(&mut self, status: MutationStatus) {
        match status {
            MutationStatus::Killed => self.killed += 1,
            MutationStatus::Survived => self.survived += 1,
            MutationStatus::Timeout => self.timeout += 1,
            MutationStatus::OutOfMemory => self.out_of_memory += 1,
            MutationStatus::ProcessLimit => self.process_limit += 1,
            MutationStatus::Error => self.error += 1,
            MutationStatus::NotRun => self.not_run += 1,
        }
        self.inconclusive =
            self.timeout + self.out_of_memory + self.process_limit + self.error + self.not_run;
        let decidable = self.killed + self.survived;
        self.score = (decidable != 0).then(|| self.killed as f64 / decidable as f64);
    }
}

#[must_use]
pub fn summarize(statuses: &[MutationStatus]) -> MutationSummary {
    let mut result = MutationSummary::default();
    for status in statuses {
        result.record(*status);
    }
    result
}

// Each field is an independent, public exit-condition input. Keeping the flags
// directly addressable preserves the exit-status protocol used by callers.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExitPolicy {
    pub infrastructure_error: bool,
    pub baseline_failed: bool,
    pub incomplete: bool,
    pub survivors: bool,
    pub interrupted: bool,
}

impl ExitPolicy {
    #[must_use]
    pub fn from_summary(summary: &MutationSummary) -> Self {
        Self {
            infrastructure_error: summary.error > 0,
            incomplete: summary.timeout > 0
                || summary.out_of_memory > 0
                || summary.process_limit > 0
                || summary.not_run > 0,
            survivors: summary.survived > 0,
            ..Self::default()
        }
    }
}

#[must_use]
pub fn exit_code(incomplete: bool, survivors: bool, interrupted: bool) -> i32 {
    exit_code_for(ExitPolicy {
        incomplete,
        survivors,
        interrupted,
        ..ExitPolicy::default()
    })
}

#[must_use]
pub fn exit_code_for(policy: ExitPolicy) -> i32 {
    if policy.interrupted {
        130
    } else if policy.infrastructure_error {
        2
    } else if policy.baseline_failed {
        3
    } else if policy.incomplete {
        4
    } else {
        i32::from(policy.survivors)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReportVersions {
    pub os: String,
    pub hoimin: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceControl {
    pub mode: ResourceMode,
    pub mechanism: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunStarted {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub normalized_config: Option<RunConfig>,
    pub versions: ReportVersions,
    pub resource_control: ResourceControl,
}

impl RunStarted {
    pub fn minimal(run_id: impl Into<String>, sequence: u64) -> Self {
        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence,
            run_id: run_id.into(),
            normalized_config: None,
            versions: ReportVersions::default(),
            resource_control: ResourceControl {
                mode: ResourceMode::Hard,
                mechanism: String::new(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaselineFinished {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub termination: ProcessTermination,
    pub elapsed_ms: u64,
    pub resource_mode: ResourceMode,
    pub output: OutputSpoolRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutantStarted {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub mutant_id: String,
    pub mutant_sequence: u64,
}

impl MutantStarted {
    pub fn new(
        run_id: impl Into<String>,
        sequence: u64,
        mutant_id: impl Into<String>,
        mutant_sequence: u64,
    ) -> Self {
        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence,
            run_id: run_id.into(),
            mutant_id: mutant_id.into(),
            mutant_sequence,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutantFinished {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub candidate: MutationCandidate,
    pub status: MutationStatus,
    pub termination: Option<ProcessTermination>,
    pub elapsed_ms: u64,
    pub resource_mode: ResourceMode,
    pub output: Option<OutputSpoolRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub level: String,
    pub code: String,
    pub message: String,
}

impl Diagnostic {
    pub fn new(
        run_id: impl Into<String>,
        sequence: u64,
        level: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            sequence,
            run_id: run_id.into(),
            level: level.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub counts: MutationSummary,
    pub complete: bool,
    pub exit_code: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum OutputEvent {
    RunStarted(RunStarted),
    BaselineFinished(BaselineFinished),
    MutantStarted(MutantStarted),
    MutantFinished(MutantFinished),
    Diagnostic(Diagnostic),
    RunFinished(RunSummary),
}

impl OutputEvent {
    #[must_use]
    pub fn sequence(&self) -> u64 {
        match self {
            Self::RunStarted(value) => value.sequence,
            Self::BaselineFinished(value) => value.sequence,
            Self::MutantStarted(value) => value.sequence,
            Self::MutantFinished(value) => value.sequence,
            Self::Diagnostic(value) => value.sequence,
            Self::RunFinished(value) => value.sequence,
        }
    }

    #[must_use]
    pub fn schema_version(&self) -> u32 {
        match self {
            Self::RunStarted(value) => value.schema_version,
            Self::BaselineFinished(value) => value.schema_version,
            Self::MutantStarted(value) => value.schema_version,
            Self::MutantFinished(value) => value.schema_version,
            Self::Diagnostic(value) => value.schema_version,
            Self::RunFinished(value) => value.schema_version,
        }
    }

    #[must_use]
    pub fn run_id(&self) -> &str {
        match self {
            Self::RunStarted(value) => &value.run_id,
            Self::BaselineFinished(value) => &value.run_id,
            Self::MutantStarted(value) => &value.run_id,
            Self::MutantFinished(value) => &value.run_id,
            Self::Diagnostic(value) => &value.run_id,
            Self::RunFinished(value) => &value.run_id,
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ReportSequenceError {
    #[error("report event preceded run_started")]
    RunNotStarted,
    #[error("run_started was emitted more than once for {run_id}")]
    RunAlreadyStarted { run_id: String },
    #[error("report event belongs to run {received}, expected {expected}")]
    RunIdMismatch { expected: String, received: String },
    #[error("report sequence {received} does not follow {previous}")]
    NotMonotonic { previous: u64, received: u64 },
    #[error("mutant {mutant_id} sequence {mutant_sequence} started more than once")]
    MutantAlreadyStarted {
        mutant_id: String,
        mutant_sequence: u64,
    },
    #[error("mutant {mutant_id} sequence {mutant_sequence} finished before it started")]
    MutantNotStarted {
        mutant_id: String,
        mutant_sequence: u64,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ReportSequence {
    last: Option<u64>,
    run_id: Option<String>,
    active_mutants: BTreeSet<(String, u64)>,
}

impl ReportSequence {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// # Errors
    ///
    /// Returns [`ReportSequenceError`] when the event violates the run,
    /// mutant-lifecycle, or strictly increasing sequence invariants.
    pub fn observe(&mut self, event: &OutputEvent) -> Result<(), ReportSequenceError> {
        let lifecycle_error = match (event, self.run_id.as_deref()) {
            (OutputEvent::RunStarted(value), Some(_)) => {
                Some(ReportSequenceError::RunAlreadyStarted {
                    run_id: value.run_id.clone(),
                })
            }
            (OutputEvent::RunStarted(_), None) => None,
            (_, None) => Some(ReportSequenceError::RunNotStarted),
            (_, Some(expected)) if event.run_id() != expected => {
                Some(ReportSequenceError::RunIdMismatch {
                    expected: expected.to_owned(),
                    received: event.run_id().to_owned(),
                })
            }
            _ => None,
        };
        let mutant_error = lifecycle_error.or_else(|| match event {
            OutputEvent::MutantStarted(value) => {
                let key = (value.mutant_id.clone(), value.mutant_sequence);
                self.active_mutants.contains(&key).then_some(
                    ReportSequenceError::MutantAlreadyStarted {
                        mutant_id: value.mutant_id.clone(),
                        mutant_sequence: value.mutant_sequence,
                    },
                )
            }
            OutputEvent::MutantFinished(value) => {
                let key = (value.candidate.id.clone(), value.candidate.sequence);
                (!self.active_mutants.contains(&key)).then_some(
                    ReportSequenceError::MutantNotStarted {
                        mutant_id: value.candidate.id.clone(),
                        mutant_sequence: value.candidate.sequence,
                    },
                )
            }
            _ => None,
        });
        let error = mutant_error.or_else(|| {
            self.last.and_then(|previous| {
                (event.sequence() <= previous).then_some(ReportSequenceError::NotMonotonic {
                    previous,
                    received: event.sequence(),
                })
            })
        });
        contract_ensure!(
            "report.sequence.invariant",
            error.is_none(),
            (
                &self.run_id,
                &self.active_mutants,
                self.last,
                event.run_id(),
                event.sequence()
            )
        );
        if let Some(error) = error {
            return Err(error);
        }
        self.last = Some(event.sequence());
        match event {
            OutputEvent::RunStarted(value) => self.run_id = Some(value.run_id.clone()),
            OutputEvent::MutantStarted(value) => {
                self.active_mutants
                    .insert((value.mutant_id.clone(), value.mutant_sequence));
            }
            OutputEvent::MutantFinished(value) => {
                self.active_mutants
                    .remove(&(value.candidate.id.clone(), value.candidate.sequence));
            }
            _ => {}
        }
        Ok(())
    }
}
