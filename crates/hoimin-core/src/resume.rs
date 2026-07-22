use std::time::Duration;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::{
    CommandArg, FingerprintInputFile, MutantTimeout, MutationCandidate, MutationProfile,
    MutationStatus, OutputSpoolRef, ResourceMode, RunConfig, RunLimits, TargetSlice,
};

pub const FINGERPRINT_SCHEMA_VERSION: u8 = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceHash {
    pub path: Utf8PathBuf,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FingerprintInput {
    pub sources: Vec<SourceHash>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub targets: Vec<TargetSlice>,
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
            sources,
            fingerprint_inputs: config.fingerprint_inputs.clone(),
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
    encoder.field(5, &encode_limits(&input.limits));
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
    RunFingerprint(*blake3::hash(&encoder.bytes).as_bytes())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredResult {
    pub mutant_id: String,
    pub status: MutationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutantResult {
    pub run_id: String,
    pub candidate: MutationCandidate,
    pub status: MutationStatus,
    pub elapsed: Duration,
    pub resource_mode: ResourceMode,
    pub output: Option<OutputSpoolRef>,
    pub diagnostics: Vec<SessionDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    Reuse,
    Rerun,
}

#[must_use]
pub fn resume_policy(result: Option<&StoredResult>) -> ResumeDecision {
    match result.map(|result| result.status) {
        Some(MutationStatus::Killed | MutationStatus::Survived) => ResumeDecision::Reuse,
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

fn encode_limits(limits: &RunLimits) -> Vec<u8> {
    let mut out = Encoder::new();
    out.u64(limits.jobs.get() as u64);
    out.u64(limits.max_mutants.get() as u64);
    out.u64(limits.max_candidates.get() as u64);
    out.duration(limits.analyzer_timeout.get());
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
    out.u64(limits.max_output.get());
    out.u64(limits.max_copy_size.get());
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
