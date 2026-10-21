use std::time::Duration;

use camino::Utf8PathBuf;
use hoimin_budget_math::MutantTimeout;
use serde::{Deserialize, Serialize};

use crate::{
    CommandArg, FingerprintInputFile, MutationCandidate, MutationProfile, MutationStatus,
    OutputSpoolRef, ProcessOutputState, ProcessTermination, ResourceMode, RunConfig, RunLimits,
    TargetSlice,
};

// Includes execution semantics: schema 13 requires baseline workspace restoration.
pub const FINGERPRINT_SCHEMA_VERSION: u8 = 13;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceHash {
    pub path: Utf8PathBuf,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FingerprintInput {
    pub sources: Vec<SourceHash>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub environment: crate::EnvironmentSnapshot,
    pub targets: Vec<TargetSlice>,
    pub import_roots: Vec<Utf8PathBuf>,
    pub source_roots: Vec<Utf8PathBuf>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub operators: Vec<String>,
    pub profile: MutationProfile,
    pub test_argv: Vec<CommandArg>,
    pub limits: RunLimits,
    pub resource_mode: ResourceMode,
}

impl FingerprintInput {
    #[must_use]
    pub fn from_config(
        config: &RunConfig,
        sources: Vec<SourceHash>,
        targets: Vec<TargetSlice>,
        resource_mode: ResourceMode,
    ) -> Self {
        Self {
            import_roots: config.import_roots.clone(),
            source_roots: config.selection.sources.clone(),
            includes: config.selection.includes.clone(),
            excludes: config.selection.excludes.clone(),
            sources,
            fingerprint_inputs: config.fingerprint_inputs.clone(),
            environment: config.environment.clone(),
            targets,
            operators: config.operators.names(),
            profile: config.profile,
            test_argv: config.test_argv.clone(),
            limits: config.limits.clone(),
            resource_mode,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RunFingerprint([u8; 32]);

impl RunFingerprint {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[must_use]
pub fn fingerprint(input: &FingerprintInput) -> RunFingerprint {
    let mut encoder = Encoder::new();
    encoder.raw(&[FINGERPRINT_SCHEMA_VERSION]);
    encoder.field(1, &encode_sources(&input.sources));
    encoder.field(2, &encode_targets(&input.targets));
    encoder.field(3, &encode_operators(&input.operators));
    encoder.field(4, &encode_argv(&input.test_argv));
    encoder.field(5, &encode_compatibility_limits(&input.limits));
    encoder.field(
        6,
        &[match input.resource_mode {
            ResourceMode::Hard => 0,
            ResourceMode::BestEffort => 1,
        }],
    );
    encoder.field(
        7,
        &[match input.profile {
            MutationProfile::Full => 0,
            MutationProfile::Focused => 1,
        }],
    );
    encoder.field(8, &encode_fingerprint_inputs(&input.fingerprint_inputs));
    let mut roots = Encoder::new();
    roots.count(input.import_roots.len());
    for root in &input.import_roots {
        roots.bytes(root.as_str().as_bytes());
    }
    encoder.field(9, &roots.bytes);
    // Override patterns can contain negations: preserve matching precedence.
    encoder.field(10, &encode_patterns(&input.includes));
    encoder.field(11, &encode_patterns(&input.excludes));
    // Source roots also define Python import precedence; file hashes are a set.
    let mut source_roots = Encoder::new();
    source_roots.count(input.source_roots.len());
    for root in &input.source_roots {
        source_roots.bytes(root.as_str().as_bytes());
    }
    encoder.field(12, &source_roots.bytes);
    let mut environment = Encoder::new();
    environment.bytes(&encode_patterns(input.environment.names()));
    match input.environment.digest() {
        None => environment.raw(&[0]),
        Some(hash) => {
            environment.raw(&[1]);
            environment.bytes(hash.as_bytes());
        }
    }
    encoder.field(13, &environment.bytes);
    RunFingerprint(*blake3::hash(&encoder.bytes).as_bytes())
}

fn encode_patterns(patterns: &[String]) -> Vec<u8> {
    let mut out = Encoder::new();
    out.count(patterns.len());
    for pattern in patterns {
        out.bytes(pattern.as_bytes());
    }
    out.bytes
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredResult {
    pub mutant_id: String,
    pub status: MutationStatus,
}

/// A completed execution, a reusable conclusive result, or no classified completion.
/// Diagnostics are independent evidence and never determine this outcome.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MutationOutcome {
    Executed {
        termination: ProcessTermination,
        output_state: ProcessOutputState,
        output: OutputSpoolRef,
        #[serde(deserialize_with = "deserialize_report_duration")]
        elapsed: Duration,
    },
    Reused {
        status: ReusedMutationStatus,
    },
    /// No classified completed result is available; the process may have started.
    NotRun,
}

// Keep report durations equal to the public wire schema. Serde's standard Duration
// decoder also accepts sequences and normalizes oversized nanoseconds.
pub(crate) fn deserialize_report_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct DurationVisitor;

    impl<'de> serde::de::Visitor<'de> for DurationVisitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a duration object with u64 secs and nanos below 1000000000")
        }

        fn visit_map<M>(self, map: M) -> Result<Duration, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Elapsed {
                secs: u64,
                nanos: u32,
            }

            let elapsed = Elapsed::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
            if elapsed.nanos >= 1_000_000_000 {
                return Err(serde::de::Error::custom(
                    "duration nanos must be below 1000000000",
                ));
            }
            Ok(Duration::new(elapsed.secs, elapsed.nanos))
        }
    }

    deserializer.deserialize_map(DurationVisitor)
}

/// Only conclusive outcomes qualify for reuse under the resume policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReusedMutationStatus {
    Killed,
    Survived,
}

impl MutationOutcome {
    /// Derives the domain status from owned execution evidence or reuse provenance.
    #[must_use]
    pub fn status(&self) -> MutationStatus {
        match self {
            Self::Executed {
                termination,
                output_state,
                ..
            } => crate::classify_mutant_result(*termination, *output_state),
            Self::Reused {
                status: ReusedMutationStatus::Killed,
            } => MutationStatus::Killed,
            Self::Reused {
                status: ReusedMutationStatus::Survived,
            } => MutationStatus::Survived,
            Self::NotRun => MutationStatus::NotRun,
        }
    }

    /// Synthetic and reused results contribute no execution time to this run.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        match self {
            Self::Executed { elapsed, .. } => *elapsed,
            Self::Reused { .. } | Self::NotRun => Duration::ZERO,
        }
    }

    /// Only a completed execution owns an output spool reference.
    #[must_use]
    pub const fn output(&self) -> Option<&OutputSpoolRef> {
        match self {
            Self::Executed { output, .. } => Some(output),
            Self::Reused { .. } | Self::NotRun => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutantResult {
    pub run_id: String,
    pub candidate: MutationCandidate,
    pub outcome: MutationOutcome,
    pub resource_mode: ResourceMode,
    pub diagnostics: Vec<SessionDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionDiagnostic {
    pub mutant_id: String,
    pub level: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionResumeRef {
    pub run_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ResumeDecision {
    Reuse(ReusedMutationStatus),
    Rerun,
}

/// Selects only conclusive stored results and carries their reusable status.
#[must_use]
pub fn resume_policy(result: Option<&StoredResult>) -> ResumeDecision {
    match result.map(|result| result.status) {
        Some(MutationStatus::Killed) => ResumeDecision::Reuse(ReusedMutationStatus::Killed),
        Some(MutationStatus::Survived) => ResumeDecision::Reuse(ReusedMutationStatus::Survived),
        Some(
            MutationStatus::Timeout
            | MutationStatus::OutOfMemory
            | MutationStatus::ProcessLimit
            | MutationStatus::Error
            | MutationStatus::NotRun,
        )
        | None => ResumeDecision::Rerun,
    }
}

fn encode_sources(sources: &[SourceHash]) -> Vec<u8> {
    let mut values = sources
        .iter()
        .map(|value| {
            let mut element = Encoder::new();
            element.bytes(value.path.as_str().as_bytes());
            element.bytes(&value.hash);
            element.bytes
        })
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    let mut out = Encoder::new();
    out.count(values.len());
    for value in values {
        out.bytes(&value);
    }
    out.bytes
}

fn encode_fingerprint_inputs(inputs: &[FingerprintInputFile]) -> Vec<u8> {
    let mut values = inputs.to_vec();
    values.sort();
    values.dedup();
    let mut out = Encoder::new();
    out.count(values.len());
    for value in values {
        out.bytes(value.path.as_str().as_bytes());
        out.bytes(value.hash.as_bytes());
    }
    out.bytes
}

fn encode_targets(targets: &[TargetSlice]) -> Vec<u8> {
    let mut values = targets
        .iter()
        .cloned()
        .map(|value| {
            let mut element = Encoder::new();
            element.bytes(value.path.as_str().as_bytes());
            let mut lines = value.lines;
            lines.sort_by_key(|line| (line.start, line.end));
            element.count(lines.len());
            for line in lines {
                element.raw(&line.start.to_le_bytes());
                element.raw(&line.end.to_le_bytes());
            }
            let mut symbols = value.symbols;
            symbols.sort();
            symbols.dedup();
            element.count(symbols.len());
            for symbol in symbols {
                element.bytes(symbol.as_bytes());
            }
            element.bytes
        })
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    let mut out = Encoder::new();
    out.count(values.len());
    for value in values {
        out.bytes(&value);
    }
    out.bytes
}

fn encode_operators(operators: &[String]) -> Vec<u8> {
    let mut values = operators.to_vec();
    values.sort();
    values.dedup();
    let mut out = Encoder::new();
    out.count(values.len());
    for value in values {
        out.bytes(value.as_bytes());
    }
    out.bytes
}

fn encode_argv(argv: &[CommandArg]) -> Vec<u8> {
    let mut out = Encoder::new();
    out.count(argv.len());
    for arg in argv {
        match arg {
            CommandArg::Unix(bytes) => {
                out.raw(&[0]);
                out.bytes(bytes);
            }
            CommandArg::Windows(units) => {
                out.raw(&[1]);
                out.count(units.len());
                for unit in units {
                    out.raw(&unit.to_le_bytes());
                }
            }
        }
    }
    out.bytes
}

fn encode_compatibility_limits(limits: &RunLimits) -> Vec<u8> {
    let mut out = Encoder::new();
    // The session owner separately checks and records monotone max-mutants changes.
    out.u64(limits.max_candidates.get() as u64);
    out.duration(limits.baseline_timeout.get());
    match limits.mutant_timeout {
        MutantTimeout::Auto => out.raw(&[0]),
        MutantTimeout::Fixed(value) => {
            out.raw(&[1]);
            out.duration(value.get());
        }
    }
    out.duration(limits.total_timeout.get());
    out.u64(limits.max_memory.get());
    out.u64(limits.max_copy_size.get());
    out.u64(limits.max_workspace_size.get());
    out.u64(limits.min_free_space.get());
    out.u64(limits.max_processes.get() as u64);
    out.bytes
}

struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn field(&mut self, tag: u8, value: &[u8]) {
        self.raw(&[tag]);
        self.bytes(value);
    }

    fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.raw(value);
    }

    fn count(&mut self, value: usize) {
        self.u64(value as u64);
    }

    fn u64(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    fn duration(&mut self, value: Duration) {
        self.u64(value.as_secs());
        self.raw(&value.subsec_nanos().to_le_bytes());
    }

    fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
}
