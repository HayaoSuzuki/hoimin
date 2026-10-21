use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    MutationCandidate, MutationOutcome, MutationStatus, OutputSpoolRef, ProcessOutputState,
    ProcessTermination, ResourceMode, RunConfig, SessionDiagnostic, contract_ensure,
};

mod janitor;
pub use janitor::*;

pub const REPORT_SCHEMA_VERSION: u32 = 10;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(deserialize_with = "required_janitor")]
    pub janitor: Option<JanitorSummary>,
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
            janitor: None,
        }
    }
}

// A custom deserializer makes absence an error while accepting explicit null.
fn required_janitor<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<JanitorSummary>, D::Error> {
    Option::<JanitorSummary>::deserialize(deserializer)
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
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Encoded {
            PortableGuard {},
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
            Encoded::PortableGuard {} => Ok(Self::PortableGuard),
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct ReportVersions {
    pub os: String,
    pub hoimin: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceControl {
    pub mode: ResourceMode,
    pub mechanism: String,
}

mod selection;
pub use selection::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeFreshReason {
    NoPriorRun,
    MatchingRunComplete,
    BudgetDecreased,
    FingerprintMismatch,
    NoIncompleteRun,
    CandidateChanged,
}

impl ResumeFreshReason {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NoPriorRun => "no_prior_run",
            Self::MatchingRunComplete => "matching_run_complete",
            Self::BudgetDecreased => "budget_decreased",
            Self::FingerprintMismatch => "fingerprint_mismatch",
            Self::NoIncompleteRun => "no_incomplete_run",
            Self::CandidateChanged => "candidate_changed",
        }
    }

    #[must_use]
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::NoPriorRun => "no saved runs",
            Self::MatchingRunComplete => "a matching run is already complete",
            Self::BudgetDecreased => "matching incomplete runs require a larger max-mutants budget",
            Self::FingerprintMismatch => "incomplete runs have a different fingerprint",
            Self::NoIncompleteRun => "saved runs are complete and none match this fingerprint",
            Self::CandidateChanged => "session history changed during resume selection",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", from = "ResumeOutcomeWire")]
pub enum ResumeOutcome {
    Resumed,
    Fresh { reason: ResumeFreshReason },
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum ResumeOutcomeWire {
    Resumed {},
    Fresh { reason: ResumeFreshReason },
}

impl From<ResumeOutcomeWire> for ResumeOutcome {
    fn from(value: ResumeOutcomeWire) -> Self {
        match value {
            ResumeOutcomeWire::Resumed {} => Self::Resumed,
            ResumeOutcomeWire::Fresh { reason } => Self::Fresh { reason },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunStarted {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumeOutcome>,
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
    /// Builds a header with an explicit description of the selected resource backend.
    pub fn minimal(
        run_id: impl Into<String>,
        sequence: u64,
        resource_control: ResourceControl,
    ) -> Self {
        Self {
            resume: None,
            schema_version: REPORT_SCHEMA_VERSION,
            sequence,
            run_id: run_id.into(),
            normalized_config: None,
            versions: ReportVersions::default(),
            resource_control,
            verification_selection: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutantFinished {
    pub schema_version: u32,
    pub sequence: u64,
    pub run_id: String,
    pub candidate: MutationCandidate,
    pub outcome: MutationOutcome,
    pub resource_mode: ResourceMode,
    pub diagnostics: Vec<SessionDiagnostic>,
}

impl Serialize for MutantFinished {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Wire<'a> {
            schema_version: u32,
            sequence: u64,
            run_id: &'a str,
            candidate: &'a MutationCandidate,
            outcome: &'a MutationOutcome,
            status: MutationStatus,
            resource_mode: ResourceMode,
            #[serde(skip_serializing_if = "<[SessionDiagnostic]>::is_empty")]
            diagnostics: &'a [SessionDiagnostic],
        }

        Wire {
            schema_version: self.schema_version,
            sequence: self.sequence,
            run_id: &self.run_id,
            candidate: &self.candidate,
            outcome: &self.outcome,
            status: self.outcome.status(),
            resource_mode: self.resource_mode,
            diagnostics: &self.diagnostics,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MutantFinished {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            sequence: u64,
            run_id: String,
            candidate: MutationCandidate,
            outcome: MutationOutcome,
            status: MutationStatus,
            resource_mode: ResourceMode,
            #[serde(default)]
            diagnostics: Vec<SessionDiagnostic>,
        }

        let wire = Wire::deserialize(deserializer)?;
        // Status is an untrusted redundant projection: check and discard it.
        // Execution, summary and resume continue to derive from the owned outcome.
        let expected = wire.outcome.status();
        if wire.status != expected {
            return Err(serde::de::Error::custom(format_args!(
                "status {} does not match outcome status {}",
                wire.status.as_str(),
                expected.as_str()
            )));
        }
        Ok(Self {
            schema_version: wire.schema_version,
            sequence: wire.sequence,
            run_id: wire.run_id,
            candidate: wire.candidate,
            outcome: wire.outcome,
            resource_mode: wire.resource_mode,
            diagnostics: wire.diagnostics,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct RunSummary {
    /// Individual cap passed to mutant requests; null if none was requested.
    /// The run-wide deadline can interrupt a process before this cap expires.
    #[serde(deserialize_with = "required_mutant_timeout")]
    pub resolved_mutant_timeout: Option<std::time::Duration>,
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

// Require an explicit null when no mutant process request was constructed.
fn required_mutant_timeout<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<std::time::Duration>, D::Error> {
    #[derive(Deserialize)]
    struct ReportDuration(
        #[serde(deserialize_with = "crate::resume::deserialize_report_duration")]
        std::time::Duration,
    );
    Option::<ReportDuration>::deserialize(deserializer)
        .map(|value| value.map(|ReportDuration(duration)| duration))
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
            Kind::RunStarted => decode_event_payload(raw.get()).map(Self::RunStarted),
            Kind::BaselineFinished => decode_event_payload(raw.get()).map(Self::BaselineFinished),
            Kind::MutantStarted => decode_event_payload(raw.get()).map(Self::MutantStarted),
            Kind::MutantFinished => decode_event_payload(raw.get()).map(Self::MutantFinished),
            Kind::Diagnostic => decode_event_payload(raw.get()).map(Self::Diagnostic),
            Kind::RunFinished => decode_event_payload(raw.get()).map(Self::RunFinished),
        }
        .map_err(serde::de::Error::custom)
    }
}

// Consume only the discriminator while the original JSON parser still owns the
// payload. Serde's buffered internally tagged enum representation cannot carry
// nested RawValue fields, and a Value map would erase duplicate-key evidence.
fn decode_event_payload<T: serde::de::DeserializeOwned>(raw: &str) -> serde_json::Result<T> {
    struct Payload<T>(T);

    impl<'de, T: Deserialize<'de>> Deserialize<'de> for Payload<T> {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor<T>(std::marker::PhantomData<T>);
            impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
                type Value = Payload<T>;

                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a tagged report event")
                }

                fn visit_map<M: serde::de::MapAccess<'de>>(
                    self,
                    map: M,
                ) -> Result<Self::Value, M::Error> {
                    T::deserialize(serde::de::value::MapAccessDeserializer::new(PayloadMap(
                        map,
                    )))
                    .map(Payload)
                }
            }
            deserializer.deserialize_map(Visitor(std::marker::PhantomData))
        }
    }

    struct PayloadMap<M>(M);
    impl<'de, M: serde::de::MapAccess<'de>> serde::de::MapAccess<'de> for PayloadMap<M> {
        type Error = M::Error;

        fn next_key_seed<K: serde::de::DeserializeSeed<'de>>(
            &mut self,
            seed: K,
        ) -> Result<Option<K::Value>, Self::Error> {
            while let Some(key) = self.0.next_key::<String>()? {
                if key == "kind" {
                    // The envelope already validates the discriminator, including duplicates.
                    self.0.next_value::<serde::de::IgnoredAny>()?;
                } else {
                    return seed
                        .deserialize(serde::de::value::StringDeserializer::new(key))
                        .map(Some);
                }
            }
            Ok(None)
        }

        fn next_value_seed<V: serde::de::DeserializeSeed<'de>>(
            &mut self,
            seed: V,
        ) -> Result<V::Value, Self::Error> {
            self.0.next_value_seed(seed)
        }
    }

    serde_json::from_str::<Payload<T>>(raw).map(|value| value.0)
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
}

#[derive(Clone, Debug, Default)]
pub struct ReportSequence {
    last: Option<u64>,
    lifecycle: ReportLifecycle,
    active_mutants: BTreeSet<(String, u64)>,
    seen_mutants: BTreeMap<String, u64>,
}

/// The identity is retained after finish so late events report the original run,
/// even when the rejected event carries a different identity.
#[derive(Clone, Debug, Default)]
enum ReportLifecycle {
    #[default]
    NotStarted,
    Running(String),
    Finished(String),
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
                            None
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

    /// Checks an event without mutating state or asserting producer contracts.
    ///
    /// # Errors
    ///
    /// Returns [`ReportSequenceError`] when the event violates the run,
    /// mutant-lifecycle, or strictly increasing sequence invariants.
    pub fn validate(&self, event: &OutputEvent) -> Result<(), ReportSequenceError> {
        let lifecycle_error = match (&self.lifecycle, event) {
            (ReportLifecycle::Finished(run_id), _) => {
                Some(ReportSequenceError::RunAlreadyFinished {
                    run_id: run_id.clone(),
                })
            }
            (ReportLifecycle::Running(_), OutputEvent::RunStarted(value)) => {
                Some(ReportSequenceError::RunAlreadyStarted {
                    run_id: value.run_id.clone(),
                })
            }
            (ReportLifecycle::NotStarted, OutputEvent::RunStarted(_)) => None,
            (ReportLifecycle::NotStarted, _) => Some(ReportSequenceError::RunNotStarted),
            (ReportLifecycle::Running(expected), _) if event.run_id() != expected => {
                Some(ReportSequenceError::RunIdMismatch {
                    expected: expected.to_owned(),
                    received: event.run_id().to_owned(),
                })
            }
            (ReportLifecycle::Running(_), _) => None,
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
        error.map_or(Ok(()), Err)
    }

    /// Records a producer event after validating the lifecycle invariants.
    /// Untrusted input can call [`Self::validate`] first to receive errors
    /// without triggering producer assertions in contracts builds.
    ///
    /// # Errors
    ///
    /// Returns [`ReportSequenceError`] when lifecycle or sequence rules fail.
    pub fn observe(&mut self, event: &OutputEvent) -> Result<(), ReportSequenceError> {
        let result = self.validate(event);
        contract_ensure!(
            "report.sequence.invariant",
            result.is_ok(),
            (
                &self.lifecycle,
                &self.active_mutants,
                &self.seen_mutants,
                self.last,
                event.run_id(),
                event.sequence()
            )
        );
        result?;
        self.last = Some(event.sequence());
        match event {
            OutputEvent::RunStarted(value) => {
                self.lifecycle = ReportLifecycle::Running(value.run_id.clone());
            }
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
            OutputEvent::RunFinished(_) => {
                self.lifecycle = match std::mem::take(&mut self.lifecycle) {
                    ReportLifecycle::Running(run_id) => ReportLifecycle::Finished(run_id),
                    ReportLifecycle::NotStarted | ReportLifecycle::Finished(_) => {
                        unreachable!("validated finish requires a running report")
                    }
                };
            }
            _ => {}
        }
        Ok(())
    }
}

impl From<crate::DiskFailure> for DiskStopReport {
    fn from(failure: crate::DiskFailure) -> Self {
        use crate::DiskFailureReason;
        let code = failure.code().to_owned();
        let (owned_bytes, available_bytes, message) = match failure.reason {
            DiskFailureReason::WorkspaceSizeExceeded(value)
            | DiskFailureReason::FilesystemReserveReached(value) => {
                (Some(value.owned_bytes), Some(value.available_bytes), None)
            }
            DiskFailureReason::MeasurementFailed(message)
            | DiskFailureReason::ProcessFailed(message) => (None, None, Some(message)),
        };
        Self {
            code,
            owned_bytes,
            available_bytes,
            message,
            secondary: failure.secondary,
        }
    }
}
