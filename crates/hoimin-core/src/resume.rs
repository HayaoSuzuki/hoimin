use std::collections::BTreeSet;
use std::time::Duration;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::{
    CommandArg, MutantTimeout, MutationCandidate, MutationStatus, OutputSpoolRef, ResourceMode,
    RunLimits, TargetSlice,
};

const FINGERPRINT_SCHEMA: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceHash {
    pub path: Utf8PathBuf,
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FingerprintInput {
    pub sources: Vec<SourceHash>,
    pub targets: Vec<TargetSlice>,
    pub operators: Vec<String>,
    pub test_argv: Vec<CommandArg>,
    pub limits: RunLimits,
    pub python_version: String,
    pub libcst_version: String,
    pub resource_mode: ResourceMode,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RunFingerprint([u8; 32]);

impl RunFingerprint {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub fn fingerprint(input: &FingerprintInput) -> RunFingerprint {
    let mut encoder = Encoder::new();
    encoder.raw(&[FINGERPRINT_SCHEMA]);
    encoder.field(1, &encode_sources(&input.sources));
    encoder.field(2, &encode_targets(&input.targets));
    encoder.field(3, &encode_operators(&input.operators));
    encoder.field(4, &encode_argv(&input.test_argv));
    encoder.field(5, &encode_limits(&input.limits));
    encoder.field(6, input.python_version.as_bytes());
    encoder.field(7, input.libcst_version.as_bytes());
    encoder.field(
        8,
        &[match input.resource_mode {
            ResourceMode::Hard => 0,
            ResourceMode::BestEffort => 1,
        }],
    );
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
pub struct StoredRun {
    pub run_id: String,
    pub fingerprint: RunFingerprint,
    pub ordinal: u64,
    pub complete: bool,
    pub results: Vec<StoredResult>,
    pub diagnostics: Vec<SessionDiagnostic>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResumeDecision {
    pub run_id: Option<String>,
    pub reusable_ids: BTreeSet<String>,
    pub rerun_ids: BTreeSet<String>,
}

pub fn resume_policy(results: &[StoredResult]) -> ResumeDecision {
    let mut decision = ResumeDecision::default();
    for result in results {
        match result.status {
            MutationStatus::Killed | MutationStatus::Survived => {
                decision.reusable_ids.insert(result.mutant_id.clone());
            }
            MutationStatus::Timeout
            | MutationStatus::OutOfMemory
            | MutationStatus::ProcessLimit
            | MutationStatus::Error
            | MutationStatus::NotRun => {
                decision.rerun_ids.insert(result.mutant_id.clone());
            }
        }
    }
    decision
}

pub fn select_resume_run(runs: &[StoredRun], wanted: &RunFingerprint) -> Option<ResumeDecision> {
    let run = runs
        .iter()
        .filter(|run| !run.complete && run.fingerprint == *wanted)
        .max_by_key(|run| run.ordinal)?;
    let mut decision = resume_policy(&run.results);
    decision.run_id = Some(run.run_id.clone());
    Some(decision)
}

fn encode_sources(sources: &[SourceHash]) -> Vec<u8> {
    let mut values = sources.to_vec();
    values.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
    let mut out = Encoder::new();
    out.count(values.len());
    for value in values {
        out.bytes(value.path.as_str().as_bytes());
        out.bytes(&value.hash);
    }
    out.bytes
}

fn encode_targets(targets: &[TargetSlice]) -> Vec<u8> {
    let mut values = targets.to_vec();
    values.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
    let mut out = Encoder::new();
    out.count(values.len());
    for value in values {
        out.bytes(value.path.as_str().as_bytes());
        let mut lines = value.lines;
        lines.sort_by_key(|line| (line.start, line.end));
        out.count(lines.len());
        for line in lines {
            out.raw(&line.start.to_le_bytes());
            out.raw(&line.end.to_le_bytes());
        }
        let mut symbols = value.symbols;
        symbols.sort();
        symbols.dedup();
        out.count(symbols.len());
        for symbol in symbols {
            out.bytes(symbol.as_bytes());
        }
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
