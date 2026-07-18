use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Duration;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CommandArg, LineSelection, Selection};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawRunLimits {
    pub jobs: usize,
    pub max_mutants: usize,
    pub max_candidates: usize,
    pub analyzer_timeout: Duration,
    pub baseline_timeout: Duration,
    pub mutant_timeout: Option<Duration>,
    pub total_timeout: Duration,
    pub max_memory: u64,
    pub max_output: u64,
    pub max_copy_size: u64,
    pub max_processes: usize,
}

impl Default for RawRunLimits {
    fn default() -> Self {
        Self {
            jobs: 1,
            max_mutants: 100,
            max_candidates: 10_000,
            analyzer_timeout: Duration::from_secs(30),
            baseline_timeout: Duration::from_secs(60),
            mutant_timeout: None,
            total_timeout: Duration::from_secs(5 * 60),
            max_memory: 1024 * 1024 * 1024,
            max_output: 1024 * 1024,
            max_copy_size: 1024 * 1024 * 1024,
            max_processes: 64,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawRunConfig {
    pub root: Utf8PathBuf,
    pub sources: Vec<Utf8PathBuf>,
    pub files: Vec<Utf8PathBuf>,
    pub lines: Vec<LineSelection>,
    pub symbols: Vec<crate::SymbolSelection>,
    pub changed: bool,
    pub diff_base: Option<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub python: Option<Utf8PathBuf>,
    pub allow_best_effort_memory: bool,
    pub limits: RawRunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub session: Option<SessionConfig>,
    pub resume: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Json,
    Jsonl,
    Human,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: OutputFormat,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Json,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionConfig {
    pub path: Utf8PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NonZeroDuration(Duration);

impl NonZeroDuration {
    pub fn get(self) -> Duration {
        self.0
    }

    fn new(value: Duration, name: &'static str) -> Result<Self, ConfigError> {
        if value.is_zero() {
            Err(ConfigError::InvalidLimit(name))
        } else {
            Ok(Self(value))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MutantTimeout {
    Auto,
    Fixed(NonZeroDuration),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunLimits {
    pub jobs: NonZeroUsize,
    pub max_mutants: NonZeroUsize,
    pub max_candidates: NonZeroUsize,
    pub analyzer_timeout: NonZeroDuration,
    pub baseline_timeout: NonZeroDuration,
    pub mutant_timeout: MutantTimeout,
    pub total_timeout: NonZeroDuration,
    pub max_memory: NonZeroU64,
    pub max_output: NonZeroU64,
    pub max_copy_size: NonZeroU64,
    pub max_processes: NonZeroUsize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunConfig {
    pub root: Utf8PathBuf,
    pub selection: Selection,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub session: Option<SessionConfig>,
    pub python: Option<Utf8PathBuf>,
    pub allow_best_effort_memory: bool,
    pub resume: bool,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error("at least one target selector is required")]
    MissingSelector,
    #[error("--diff-base requires --changed")]
    DiffBaseRequiresChanged,
    #[error("--changed requires --source")]
    ChangedRequiresSource,
    #[error("--symbol requires --source")]
    SymbolRequiresSource,
    #[error("--resume requires --session")]
    ResumeRequiresSession,
    #[error("at least one test argv element is required")]
    MissingTestArgv,
    #[error("invalid zero or overflowing limit: {0}")]
    InvalidLimit(&'static str),
}

impl TryFrom<&RawRunLimits> for RunLimits {
    type Error = ConfigError;

    fn try_from(raw: &RawRunLimits) -> Result<Self, Self::Error> {
        if raw
            .baseline_timeout
            .checked_mul(2)
            .and_then(|value| value.checked_add(Duration::from_secs(1)))
            .is_none()
        {
            return Err(ConfigError::InvalidLimit("baseline_timeout"));
        }
        Ok(Self {
            jobs: nonzero_usize(raw.jobs, "jobs")?,
            max_mutants: nonzero_usize(raw.max_mutants, "max_mutants")?,
            max_candidates: nonzero_usize(raw.max_candidates, "max_candidates")?,
            analyzer_timeout: NonZeroDuration::new(raw.analyzer_timeout, "analyzer_timeout")?,
            baseline_timeout: NonZeroDuration::new(raw.baseline_timeout, "baseline_timeout")?,
            mutant_timeout: match raw.mutant_timeout {
                Some(value) => MutantTimeout::Fixed(NonZeroDuration::new(value, "mutant_timeout")?),
                None => MutantTimeout::Auto,
            },
            total_timeout: NonZeroDuration::new(raw.total_timeout, "total_timeout")?,
            max_memory: NonZeroU64::new(raw.max_memory)
                .ok_or(ConfigError::InvalidLimit("max_memory"))?,
            max_output: NonZeroU64::new(raw.max_output)
                .ok_or(ConfigError::InvalidLimit("max_output"))?,
            max_copy_size: NonZeroU64::new(raw.max_copy_size)
                .ok_or(ConfigError::InvalidLimit("max_copy_size"))?,
            max_processes: nonzero_usize(raw.max_processes, "max_processes")?,
        })
    }
}

fn nonzero_usize(value: usize, name: &'static str) -> Result<NonZeroUsize, ConfigError> {
    NonZeroUsize::new(value).ok_or(ConfigError::InvalidLimit(name))
}

impl TryFrom<RawRunConfig> for RunConfig {
    type Error = ConfigError;

    fn try_from(raw: RawRunConfig) -> Result<Self, Self::Error> {
        let has_selector = !raw.sources.is_empty()
            || !raw.files.is_empty()
            || !raw.lines.is_empty()
            || !raw.symbols.is_empty()
            || raw.changed;
        if !has_selector {
            return Err(ConfigError::MissingSelector);
        }
        if raw.diff_base.is_some() && !raw.changed {
            return Err(ConfigError::DiffBaseRequiresChanged);
        }
        if raw.changed && raw.sources.is_empty() {
            return Err(ConfigError::ChangedRequiresSource);
        }
        if !raw.symbols.is_empty() && raw.sources.is_empty() {
            return Err(ConfigError::SymbolRequiresSource);
        }
        if raw.resume && raw.session.is_none() {
            return Err(ConfigError::ResumeRequiresSession);
        }
        if raw.test_argv.is_empty() {
            return Err(ConfigError::MissingTestArgv);
        }
        let limits = RunLimits::try_from(&raw.limits)?;
        let selection = Selection {
            root: raw.root.clone(),
            sources: raw.sources,
            files: raw.files,
            lines: raw.lines,
            symbols: raw.symbols,
            changed: raw.changed,
            diff_base: raw.diff_base,
            includes: raw.includes,
            excludes: raw.excludes,
        };
        Ok(Self {
            root: raw.root,
            selection,
            limits,
            test_argv: raw.test_argv,
            output: raw.output,
            session: raw.session,
            python: raw.python,
            allow_best_effort_memory: raw.allow_best_effort_memory,
            resume: raw.resume,
        })
    }
}

pub fn auto_mutant_timeout(baseline: Duration) -> Duration {
    baseline
        .checked_mul(2)
        .and_then(|value| value.checked_add(Duration::from_secs(1)))
        .unwrap_or(Duration::MAX)
        .max(Duration::from_secs(5))
}
