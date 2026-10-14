use std::time::Duration;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: u64,
    pub length: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TargetSlice {
    pub path: Utf8PathBuf,
    pub lines: Vec<LineRange>,
    pub symbols: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationCandidate {
    pub id: String,
    pub sequence: u64,
    pub path: Utf8PathBuf,
    pub span: ByteSpan,
    pub original: String,
    pub replacement: String,
    pub operator: String,
    pub line: u32,
    pub column: u32,
    pub symbol: Option<String>,
    pub file_hash: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationStatus {
    Killed,
    Survived,
    Timeout,
    OutOfMemory,
    ProcessLimit,
    Error,
    NotRun,
}

impl MutationStatus {
    /// Returns the canonical name used in reports and persisted sessions.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Killed => "killed",
            Self::Survived => "survived",
            Self::Timeout => "timeout",
            Self::OutOfMemory => "out_of_memory",
            Self::ProcessLimit => "process_limit",
            Self::Error => "error",
            Self::NotRun => "not_run",
        }
    }

    /// Accepts only exact canonical names, without case or whitespace normalization.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "killed" => Self::Killed,
            "survived" => Self::Survived,
            "timeout" => Self::Timeout,
            "out_of_memory" => Self::OutOfMemory,
            "process_limit" => Self::ProcessLimit,
            "error" => Self::Error,
            "not_run" => Self::NotRun,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceMode {
    Hard,
    BestEffort,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandArg {
    Unix(Vec<u8>),
    Windows(Vec<u16>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateCursor {
    pub offset: u64,
    pub expected_sequence: u64,
}

impl CandidateCursor {
    pub const START: Self = Self {
        offset: 0,
        expected_sequence: 1,
    };
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateSpoolRef {
    pub token: String,
    pub records: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutputSpoolRef {
    pub token: String,
    pub retained: u64,
    pub observed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub max_output_bytes: u64,
    pub max_memory_bytes: u64,
    pub max_processes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProcessTermination {
    Exit(i32),
    Timeout,
    OutOfMemory,
    ProcessLimit,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityCheckpoint {
    PreAnalysis,
    PostMaterialization,
    Periodic,
    PreFinalReport,
    Cleanup,
}
