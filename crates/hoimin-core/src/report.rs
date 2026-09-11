use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    MutationCandidate, MutationStatus, OutputSpoolRef, PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE,
    ProcessOutputState, ProcessTermination, ResourceMode, RunConfig, SessionDiagnostic,
    contract_ensure,
};

pub const REPORT_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskRunSummary {
    pub configured_max_owned_bytes: u64,
    pub configured_min_free_bytes: u64,
    pub peak_owned_bytes: u64,
    pub minimum_available_bytes: Option<u64>,
    pub filesystems: Vec<DiskFilesystemReport>,
    pub sample_count: u64,
    pub maximum_measurement_ms: u64,
    pub enforcement: Vec<DiskEnforcementReport>,
    pub stop: Option<DiskStopReport>,
    pub cleanup: Vec<DiskCleanupReport>,
    pub removed_logical_bytes: Option<u64>,
    pub stale_roots_reclaimed: u64,
}

impl DiskRunSummary {
    #[must_use]
    pub fn unmeasured(configured_max_owned_bytes: u64, configured_min_free_bytes: u64) -> Self {
        Self {
            configured_max_owned_bytes,
            configured_min_free_bytes,
            peak_owned_bytes: 0,
            minimum_available_bytes: None,
            filesystems: Vec::new(),
            sample_count: 0,
            maximum_measurement_ms: 0,
            enforcement: Vec::new(),
            stop: None,
            cleanup: Vec::new(),
            removed_logical_bytes: None,
            stale_roots_reclaimed: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiskEnforcementReport {
    PortableGuard,
    CapacityOnly {
        root_kind: String,
        filesystem_key: String,
    },
    VerifiedAggregate {
        backend: String,
        probe: DiskCapabilityProbe,
    },
}

impl<'de> Deserialize<'de> for DiskEnforcementReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum Encoded {
            PortableGuard,
            CapacityOnly {
                root_kind: String,
                filesystem_key: String,
            },
            VerifiedAggregate {
                backend: String,
                probe: DiskCapabilityProbe,
            },
        }

        match Encoded::deserialize(deserializer)? {
            Encoded::PortableGuard => Ok(Self::PortableGuard),
            Encoded::CapacityOnly {
                root_kind,
                filesystem_key,
            } => Ok(Self::CapacityOnly {
                root_kind,
                filesystem_key,
            }),
            Encoded::VerifiedAggregate { backend, probe } => {
                Self::verified_aggregate(backend, probe).map_err(serde::de::Error::custom)
            }
        }
    }
}

impl DiskEnforcementReport {
    /// Creates a verified aggregate capability claim after validating its bounded evidence.
    ///
    /// # Errors
    ///
    /// Returns [`DiskEnforcementError`] when verification failed, the backend/capability pair is
    /// not registered, or the observation exceeds 4 KiB.
    pub fn verified_aggregate(
        backend: String,
        probe: DiskCapabilityProbe,
    ) -> Result<Self, DiskEnforcementError> {
        if !probe.verified {
            return Err(DiskEnforcementError::Unverified);
        }
        if !matches!(
            (backend.as_str(), probe.capability.as_str()),
            ("linux_project_quota", "project_quota")
        ) {
            return Err(DiskEnforcementError::Unregistered);
        }
        if probe.observation.is_empty() || probe.observation.len() > 4 * 1024 {
            return Err(DiskEnforcementError::InvalidObservation);
        }
        Ok(Self::VerifiedAggregate { backend, probe })
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DiskEnforcementError {
    #[error("aggregate disk capability probe did not verify")]
    Unverified,
    #[error("aggregate disk backend and capability are not registered")]
    Unregistered,
    #[error("aggregate disk capability observation must contain at most 4096 bytes")]
    InvalidObservation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskCapabilityProbe {
    pub capability: String,
    pub verified: bool,
    pub observation: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiskFilesystemReport {
    pub key: String,
    pub start_available_bytes: Option<u64>,
    pub minimum_available_bytes: Option<u64>,
    pub end_available_bytes: Option<u64>,
    pub available_bytes_change: Option<i128>,
}

impl<'de> Deserialize<'de> for DiskFilesystemReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            key: String,
            start_available_bytes: Box<serde_json::value::RawValue>,
            minimum_available_bytes: Box<serde_json::value::RawValue>,
            end_available_bytes: Box<serde_json::value::RawValue>,
            available_bytes_change: Box<serde_json::value::RawValue>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let start_available_bytes =
            serde_json::from_str::<Option<u64>>(wire.start_available_bytes.get())
                .map_err(serde::de::Error::custom)?;
        let minimum_available_bytes =
            serde_json::from_str::<Option<u64>>(wire.minimum_available_bytes.get())
                .map_err(serde::de::Error::custom)?;
        let end_available_bytes =
            serde_json::from_str::<Option<u64>>(wire.end_available_bytes.get())
                .map_err(serde::de::Error::custom)?;
        let available_bytes_change =
            serde_json::from_str::<Option<i128>>(wire.available_bytes_change.get())
                .map_err(serde::de::Error::custom)?;
        match (
            start_available_bytes,
            end_available_bytes,
            available_bytes_change,
        ) {
            (Some(start), Some(end), Some(reported)) => {
                let expected = i128::from(end) - i128::from(start);
                if reported != expected {
                    return Err(serde::de::Error::custom(format_args!(
                        "available_bytes_change {reported} does not match end-start {expected}"
                    )));
                }
            }
            (Some(_), Some(_), None) => {
                return Err(serde::de::Error::custom(
                    "available_bytes_change is required when both endpoints are present",
                ));
            }
            (_, _, Some(_)) => {
                return Err(serde::de::Error::custom(
                    "available_bytes_change requires both endpoints",
                ));
            }
            (_, _, None) => {}
        }
        Ok(Self {
            key: wire.key,
            start_available_bytes,
            minimum_available_bytes,
            end_available_bytes,
            available_bytes_change,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskCleanupStatus {
    Clean,
    Failed,
    Deferred,
    Retained,
    CleanupAfterDelivery,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskCleanupReport {
    pub root_id: String,
    pub owner: String,
    pub status: DiskCleanupStatus,
    pub examined_entries: u64,
    pub removed_entries: u64,
    pub details: Vec<String>,
    pub omitted_detail_count: u64,
    pub remaining_root: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiskStopReport {
    pub code: String,
    pub owned_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secondary: Vec<crate::DiskSecondary>,
}

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

#[must_use]
pub fn classify_mutant_result(
    termination: ProcessTermination,
    output_state: ProcessOutputState,
) -> MutationStatus {
    match output_state {
        ProcessOutputState::Complete => classify_mutant(termination),
        ProcessOutputState::CloseTimedOut => MutationStatus::Error,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSelectionMode {
    CandidateIds,
    Top,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSelectionPolicy {
    ExplicitCandidates,
    Strict,
    FileRoundRobinV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSelectionScope {
    ExplicitCandidates,
    RetainedCandidates,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerificationSelection {
    pub mode: VerificationSelectionMode,
    pub policy: VerificationSelectionPolicy,
    pub requested: usize,
    pub selected: usize,
    pub scope: VerificationSelectionScope,
    pub plan_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunStarted {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub normalized_config: Option<RunConfig>,
    pub versions: ReportVersions,
    pub resource_control: ResourceControl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_selection: Option<VerificationSelection>,
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
            verification_selection: None,
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
    #[serde(default, skip_serializing_if = "ProcessOutputState::is_complete")]
    pub output_state: ProcessOutputState,
    pub elapsed_ms: u64,
    pub resource_mode: ResourceMode,
    pub output: Option<OutputSpoolRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<SessionDiagnostic>,
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
    pub disk: DiskRunSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_selection: Option<VerificationSelection>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
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

impl<'de> Deserialize<'de> for OutputEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Kind {
            RunStarted,
            BaselineFinished,
            MutantStarted,
            MutantFinished,
            Diagnostic,
            RunFinished,
        }

        #[derive(Deserialize)]
        struct Envelope {
            kind: Kind,
        }

        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        let envelope =
            serde_json::from_str::<Envelope>(raw.get()).map_err(serde::de::Error::custom)?;
        match envelope.kind {
            Kind::RunStarted => serde_json::from_str(raw.get())
                .map(Self::RunStarted)
                .map_err(serde::de::Error::custom),
            Kind::BaselineFinished => serde_json::from_str(raw.get())
                .map(Self::BaselineFinished)
                .map_err(serde::de::Error::custom),
            Kind::MutantStarted => serde_json::from_str(raw.get())
                .map(Self::MutantStarted)
                .map_err(serde::de::Error::custom),
            Kind::MutantFinished => serde_json::from_str(raw.get())
                .map(Self::MutantFinished)
                .map_err(serde::de::Error::custom),
            Kind::Diagnostic => serde_json::from_str(raw.get())
                .map(Self::Diagnostic)
                .map_err(serde::de::Error::custom),
            Kind::RunFinished => serde_json::from_str(raw.get())
                .map(Self::RunFinished)
                .map_err(serde::de::Error::custom),
        }
    }
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
    #[error("report event followed run_finished for {run_id}")]
    RunAlreadyFinished { run_id: String },
    #[error("run_finished was emitted with {count} active mutants")]
    RunFinishedWithActiveMutants { count: usize },
    #[error("report event belongs to run {received}, expected {expected}")]
    RunIdMismatch { expected: String, received: String },
    #[error("report sequence {received} does not follow {previous}")]
    NotMonotonic { previous: u64, received: u64 },
    #[error("mutant {mutant_id} sequence {mutant_sequence} started more than once")]
    MutantAlreadyStarted {
        mutant_id: String,
        mutant_sequence: u64,
    },
    #[error("mutant {mutant_id} sequence {mutant_sequence} reused a stable identity")]
    DuplicateMutantIdentity {
        mutant_id: String,
        mutant_sequence: u64,
    },
    #[error(
        "mutant {mutant_id} identity maps to sequence {expected_sequence}, received {received_sequence}"
    )]
    MutantIdentitySequenceMismatch {
        mutant_id: String,
        expected_sequence: u64,
        received_sequence: u64,
    },
    #[error("mutant {mutant_id} sequence {mutant_sequence} finished before it started")]
    MutantNotStarted {
        mutant_id: String,
        mutant_sequence: u64,
    },
    #[error(
        "mutant {mutant_id} sequence {mutant_sequence} status {status:?} disagrees with termination {termination:?}; expected {expected_status:?}"
    )]
    MutantStatusTerminationMismatch {
        mutant_id: String,
        mutant_sequence: u64,
        status: MutationStatus,
        termination: ProcessTermination,
        expected_status: MutationStatus,
    },
    #[error(
        "mutant {mutant_id} sequence {mutant_sequence} output state {output_state:?} disagrees with its diagnostics"
    )]
    MutantOutputDiagnosticMismatch {
        mutant_id: String,
        mutant_sequence: u64,
        output_state: ProcessOutputState,
    },
    #[error(
        "mutant {mutant_id} sequence {mutant_sequence} output state {output_state:?} lacks its required result fields"
    )]
    MutantOutputStateMismatch {
        mutant_id: String,
        mutant_sequence: u64,
        output_state: ProcessOutputState,
    },
}

impl MutantFinished {
    /// Validates one result independently of report lifecycle events.
    ///
    /// Complete output with absent termination is accepted for legacy reports.
    ///
    /// # Errors
    ///
    /// Returns [`ReportSequenceError`] for missing output-close result fields,
    /// inconsistent output diagnostics, or a status that disagrees with the
    /// termination and output state, checked in that order.
    pub fn validate_result(&self) -> Result<(), ReportSequenceError> {
        if !output_state_matches_result(self) {
            return Err(ReportSequenceError::MutantOutputStateMismatch {
                mutant_id: self.candidate.id.clone(),
                mutant_sequence: self.candidate.sequence,
                output_state: self.output_state,
            });
        }
        if !output_diagnostics_match(self) {
            return Err(ReportSequenceError::MutantOutputDiagnosticMismatch {
                mutant_id: self.candidate.id.clone(),
                mutant_sequence: self.candidate.sequence,
                output_state: self.output_state,
            });
        }
        if let Some(termination) = self.termination {
            let expected_status = classify_mutant_result(termination, self.output_state);
            if self.status != expected_status {
                return Err(ReportSequenceError::MutantStatusTerminationMismatch {
                    mutant_id: self.candidate.id.clone(),
                    mutant_sequence: self.candidate.sequence,
                    status: self.status,
                    termination,
                    expected_status,
                });
            }
        }
        Ok(())
    }
}

fn output_state_matches_result(value: &MutantFinished) -> bool {
    match value.output_state {
        ProcessOutputState::Complete => true,
        ProcessOutputState::CloseTimedOut => {
            value.termination.is_some()
                && value.status == MutationStatus::Error
                && value.output.is_some()
        }
    }
}

fn output_diagnostics_match(value: &MutantFinished) -> bool {
    let mut close_timeout = value
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == PROCESS_OUTPUT_CLOSE_TIMEOUT_CODE);
    match value.output_state {
        ProcessOutputState::Complete => close_timeout.next().is_none(),
        ProcessOutputState::CloseTimedOut => {
            close_timeout.next().is_some_and(|diagnostic| {
                diagnostic.mutant_id == value.candidate.id
                    && diagnostic.level == "error"
                    && !diagnostic.message.trim().is_empty()
            }) && close_timeout.next().is_none()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReportSequence {
    last: Option<u64>,
    run_id: Option<String>,
    active_mutants: BTreeSet<(String, u64)>,
    seen_mutants: BTreeMap<String, u64>,
    finished: bool,
}

impl ReportSequence {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn mutant_error(&self, event: &OutputEvent) -> Option<ReportSequenceError> {
        match event {
            OutputEvent::MutantStarted(value) => {
                let key = (value.mutant_id.clone(), value.mutant_sequence);
                if self.active_mutants.contains(&key) {
                    Some(ReportSequenceError::MutantAlreadyStarted {
                        mutant_id: value.mutant_id.clone(),
                        mutant_sequence: value.mutant_sequence,
                    })
                } else {
                    match self.seen_mutants.get(&value.mutant_id) {
                        Some(expected_sequence) if *expected_sequence == value.mutant_sequence => {
                            Some(ReportSequenceError::DuplicateMutantIdentity {
                                mutant_id: value.mutant_id.clone(),
                                mutant_sequence: value.mutant_sequence,
                            })
                        }
                        Some(expected_sequence) => {
                            Some(ReportSequenceError::MutantIdentitySequenceMismatch {
                                mutant_id: value.mutant_id.clone(),
                                expected_sequence: *expected_sequence,
                                received_sequence: value.mutant_sequence,
                            })
                        }
                        None => None,
                    }
                }
            }
            OutputEvent::MutantFinished(value) => {
                match self.seen_mutants.get(&value.candidate.id) {
                    Some(expected_sequence) if *expected_sequence != value.candidate.sequence => {
                        Some(ReportSequenceError::MutantIdentitySequenceMismatch {
                            mutant_id: value.candidate.id.clone(),
                            expected_sequence: *expected_sequence,
                            received_sequence: value.candidate.sequence,
                        })
                    }
                    _ => {
                        let key = (value.candidate.id.clone(), value.candidate.sequence);
                        if self.active_mutants.contains(&key) {
                            value.validate_result().err()
                        } else {
                            Some(ReportSequenceError::MutantNotStarted {
                                mutant_id: value.candidate.id.clone(),
                                mutant_sequence: value.candidate.sequence,
                            })
                        }
                    }
                }
            }
            OutputEvent::RunFinished(_) if !self.active_mutants.is_empty() => {
                Some(ReportSequenceError::RunFinishedWithActiveMutants {
                    count: self.active_mutants.len(),
                })
            }
            _ => None,
        }
    }

    /// # Errors
    ///
    /// Returns [`ReportSequenceError`] when the event violates the run,
    /// mutant-lifecycle, or strictly increasing sequence invariants.
    pub fn observe(&mut self, event: &OutputEvent) -> Result<(), ReportSequenceError> {
        let lifecycle_error = if self.finished {
            Some(ReportSequenceError::RunAlreadyFinished {
                run_id: self.run_id.clone().unwrap_or_default(),
            })
        } else {
            match (event, self.run_id.as_deref()) {
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
            }
        };
        let mutant_error = lifecycle_error.or_else(|| self.mutant_error(event));
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
                &self.seen_mutants,
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
                self.seen_mutants
                    .insert(value.mutant_id.clone(), value.mutant_sequence);
                self.active_mutants
                    .insert((value.mutant_id.clone(), value.mutant_sequence));
            }
            OutputEvent::MutantFinished(value) => {
                self.active_mutants
                    .remove(&(value.candidate.id.clone(), value.candidate.sequence));
            }
            OutputEvent::RunFinished(_) => self.finished = true,
            _ => {}
        }
        Ok(())
    }
}
