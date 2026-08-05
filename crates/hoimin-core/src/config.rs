use std::collections::BTreeSet;
use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Duration;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CommandArg, LineSelection, Selection};

pub const MAX_JOBS: usize = 256;

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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOperator {
    CompareEqNe,
    CompareOrder,
    Membership,
    Identity,
    BooleanAndOr,
    BinaryAddSub,
    AugmentedAddSub,
    BinaryMulDiv,
    BinaryFloorMod,
    UnarySign,
    RemoveNot,
    BooleanLiteral,
    BreakContinue,
    TypeNullableRemove,
    TypeNullableAdd,
    TypeListSequence,
    TypeSetAbstractSet,
    #[serde(rename = "type_dict_mapping", alias = "type_mapping")]
    TypeMapping,
    TypeIterableIterator,
    TypeSequenceIterable,
}

impl MutationOperator {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|operator| operator.as_str() == name)
    }
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompareEqNe => "compare_eq_ne",
            Self::CompareOrder => "compare_order",
            Self::Membership => "membership",
            Self::Identity => "identity",
            Self::BooleanAndOr => "boolean_and_or",
            Self::BinaryAddSub => "binary_add_sub",
            Self::AugmentedAddSub => "augmented_add_sub",
            Self::BinaryMulDiv => "binary_mul_div",
            Self::BinaryFloorMod => "binary_floor_mod",
            Self::UnarySign => "unary_sign",
            Self::RemoveNot => "remove_not",
            Self::BooleanLiteral => "boolean_literal",
            Self::BreakContinue => "break_continue",
            Self::TypeNullableRemove => "type_nullable_remove",
            Self::TypeNullableAdd => "type_nullable_add",
            Self::TypeListSequence => "type_list_sequence",
            Self::TypeSetAbstractSet => "type_set_abstract_set",
            Self::TypeMapping => "type_dict_mapping",
            Self::TypeIterableIterator => "type_iterable_iterator",
            Self::TypeSequenceIterable => "type_sequence_iterable",
        }
    }
    fn all() -> [Self; 20] {
        [
            Self::CompareEqNe,
            Self::CompareOrder,
            Self::Membership,
            Self::Identity,
            Self::BooleanAndOr,
            Self::BinaryAddSub,
            Self::AugmentedAddSub,
            Self::BinaryMulDiv,
            Self::BinaryFloorMod,
            Self::UnarySign,
            Self::RemoveNot,
            Self::BooleanLiteral,
            Self::BreakContinue,
            Self::TypeNullableRemove,
            Self::TypeNullableAdd,
            Self::TypeListSequence,
            Self::TypeSetAbstractSet,
            Self::TypeMapping,
            Self::TypeIterableIterator,
            Self::TypeSequenceIterable,
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationOperatorSelection(BTreeSet<MutationOperator>);

impl MutationOperatorSelection {
    /// Returns canonical operator IDs and selector-family names accepted by the CLI.
    #[must_use]
    pub fn valid_names() -> Vec<&'static str> {
        let mut names: Vec<_> = MutationOperator::all()
            .into_iter()
            .map(MutationOperator::as_str)
            .collect();
        names.extend(["type_nullable", "type_collections", "type_iterables"]);
        names.sort_unstable();
        names
    }

    #[must_use]
    pub fn all_legacy() -> Self {
        Self(
            [
                MutationOperator::CompareEqNe,
                MutationOperator::CompareOrder,
                MutationOperator::Membership,
                MutationOperator::Identity,
                MutationOperator::BooleanAndOr,
                MutationOperator::BinaryAddSub,
                MutationOperator::AugmentedAddSub,
                MutationOperator::BinaryMulDiv,
                MutationOperator::BinaryFloorMod,
                MutationOperator::UnarySign,
                MutationOperator::RemoveNot,
                MutationOperator::BooleanLiteral,
                MutationOperator::BreakContinue,
            ]
            .into_iter()
            .collect(),
        )
    }
    /// Expands a selector name into its constituent mutation operators.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnknownMutationOperator`] for an unrecognized selector.
    pub fn parse_selector(selector: &str) -> Result<Vec<MutationOperator>, ConfigError> {
        match selector {
            "type_nullable" => Ok(vec![
                MutationOperator::TypeNullableRemove,
                MutationOperator::TypeNullableAdd,
            ]),
            "type_collections" => Ok(vec![
                MutationOperator::TypeListSequence,
                MutationOperator::TypeSetAbstractSet,
                MutationOperator::TypeMapping,
            ]),
            "type_iterables" => Ok(vec![
                MutationOperator::TypeIterableIterator,
                MutationOperator::TypeSequenceIterable,
            ]),
            value => MutationOperator::from_name(value).map_or_else(
                || {
                    Err(ConfigError::UnknownMutationOperator {
                        value: value.to_owned(),
                    })
                },
                |operator| Ok(vec![operator]),
            ),
        }
    }
    pub fn include(&mut self, operator: MutationOperator) {
        self.0.insert(operator);
    }
    pub fn exclude(&mut self, operator: MutationOperator) {
        self.0.remove(&operator);
    }
    #[must_use]
    pub fn contains(&self, operator: MutationOperator) -> bool {
        self.0.contains(&operator)
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|operator| operator.as_str().to_owned())
            .collect()
    }
}

impl Default for MutationOperatorSelection {
    fn default() -> Self {
        Self::all_legacy()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationProfile {
    #[default]
    Full,
    Focused,
}

impl MutationProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Focused => "focused",
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
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_files: Vec<String>,
    pub operators: Vec<String>,
    pub exclude_operators: Vec<String>,
    pub allow_best_effort_memory: bool,
    pub profile: MutationProfile,
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
    #[serde(default)]
    pub metrics: Option<Utf8PathBuf>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Json,
            metrics: None,
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
    #[must_use]
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
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_files: Vec<String>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub session: Option<SessionConfig>,
    pub operators: MutationOperatorSelection,
    pub allow_best_effort_memory: bool,
    pub profile: MutationProfile,
    pub resume: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    pub root: Utf8PathBuf,
    pub selection: Selection,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub operators: MutationOperatorSelection,
    pub allow_best_effort_memory: bool,
    pub profile: MutationProfile,
    pub fingerprint_includes: Vec<String>,
    #[serde(default)]
    pub fingerprint_files: Vec<String>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
}

impl RunConfig {
    /// Validates normalized run configuration semantics.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when selection, mutation operators, runtime resume state, the test
    /// command, or limits are invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_selection(&self.selection)?;
        validate_operators(&self.operators)?;
        if self.resume && self.session.is_none() {
            return Err(ConfigError::ResumeRequiresSession);
        }
        validate_test_argv(&self.test_argv)?;
        validate_limits(&self.limits)
    }

    #[must_use]
    pub fn into_plan_config(self) -> PlanConfig {
        PlanConfig {
            root: self.root,
            selection: self.selection,
            limits: self.limits,
            test_argv: self.test_argv,
            output: self.output,
            operators: self.operators,
            allow_best_effort_memory: self.allow_best_effort_memory,
            profile: self.profile,
            fingerprint_includes: self.fingerprint_includes,
            fingerprint_files: self.fingerprint_files,
            fingerprint_inputs: self.fingerprint_inputs,
        }
    }
}

impl PlanConfig {
    /// Validates normalized persisted plan semantics.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when selection, mutation operators, the test command, or limits are
    /// invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_selection(&self.selection)?;
        validate_operators(&self.operators)?;
        validate_test_argv(&self.test_argv)?;
        validate_limits(&self.limits)
    }

    #[must_use]
    pub fn into_run_config(self, output: OutputConfig) -> RunConfig {
        RunConfig {
            root: self.root,
            selection: self.selection,
            fingerprint_includes: self.fingerprint_includes,
            fingerprint_files: self.fingerprint_files,
            fingerprint_inputs: self.fingerprint_inputs,
            limits: self.limits,
            test_argv: self.test_argv,
            output,
            session: None,
            operators: self.operators,
            allow_best_effort_memory: self.allow_best_effort_memory,
            profile: self.profile,
            resume: false,
        }
    }
}

fn valid_operator_names() -> String {
    MutationOperatorSelection::valid_names().join(", ")
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FingerprintInputFile {
    pub path: Utf8PathBuf,
    pub hash: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error(
        "unknown mutation operator: {value}; valid operators/selectors: {valid}",
        valid = valid_operator_names()
    )]
    UnknownMutationOperator { value: String },
    #[error(
        "mutation operator selection is empty after applying --operators and --exclude-operators"
    )]
    EmptyMutationOperatorSelection,
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
    #[error("invalid zero or overflowing limit: {}", limit_flag(.0))]
    InvalidLimit(&'static str),
    #[error("--jobs {jobs} exceeds the supported maximum {maximum}")]
    JobsExceedsMaximum { jobs: usize, maximum: usize },
    #[error("--jobs {jobs} exceeds --max-processes {max_processes}")]
    JobsExceedsProcesses { jobs: usize, max_processes: usize },
}

fn limit_flag(name: &str) -> &str {
    match name {
        "jobs" => "--jobs",
        "max_mutants" => "--max-mutants",
        "max_candidates" => "--max-candidates",
        "analyzer_timeout" => "--analyzer-timeout",
        "baseline_timeout" => "--baseline-timeout",
        "mutant_timeout" => "--mutant-timeout",
        "total_timeout" => "--total-timeout",
        "max_memory" => "--max-memory",
        "max_output" => "--max-output",
        "max_copy_size" => "--max-copy-size",
        "max_processes" => "--max-processes",
        other => other,
    }
}

impl TryFrom<&RawRunLimits> for RunLimits {
    type Error = ConfigError;

    fn try_from(raw: &RawRunLimits) -> Result<Self, Self::Error> {
        if raw.jobs == 0 {
            return Err(ConfigError::InvalidLimit("jobs"));
        }
        if raw.max_processes == 0 {
            return Err(ConfigError::InvalidLimit("max_processes"));
        }
        if u32::try_from(raw.max_processes).is_err() {
            return Err(ConfigError::InvalidLimit("max_processes"));
        }
        if raw.jobs > MAX_JOBS {
            return Err(ConfigError::JobsExceedsMaximum {
                jobs: raw.jobs,
                maximum: MAX_JOBS,
            });
        }
        if raw.jobs > raw.max_processes {
            return Err(ConfigError::JobsExceedsProcesses {
                jobs: raw.jobs,
                max_processes: raw.max_processes,
            });
        }
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

fn validate_selection(selection: &Selection) -> Result<(), ConfigError> {
    let has_selector = !selection.sources.is_empty()
        || !selection.files.is_empty()
        || !selection.lines.is_empty()
        || !selection.symbols.is_empty()
        || selection.changed;
    if !has_selector {
        return Err(ConfigError::MissingSelector);
    }
    if selection.diff_base.is_some() && !selection.changed {
        return Err(ConfigError::DiffBaseRequiresChanged);
    }
    if selection.changed && selection.sources.is_empty() {
        return Err(ConfigError::ChangedRequiresSource);
    }
    if !selection.symbols.is_empty() && selection.sources.is_empty() {
        return Err(ConfigError::SymbolRequiresSource);
    }
    Ok(())
}

fn validate_test_argv(test_argv: &[CommandArg]) -> Result<(), ConfigError> {
    if test_argv.is_empty() {
        Err(ConfigError::MissingTestArgv)
    } else {
        Ok(())
    }
}

fn validate_operators(operators: &MutationOperatorSelection) -> Result<(), ConfigError> {
    if operators.is_empty() {
        Err(ConfigError::EmptyMutationOperatorSelection)
    } else {
        Ok(())
    }
}

fn validate_limits(limits: &RunLimits) -> Result<(), ConfigError> {
    if limits.jobs.get() > MAX_JOBS {
        return Err(ConfigError::JobsExceedsMaximum {
            jobs: limits.jobs.get(),
            maximum: MAX_JOBS,
        });
    }
    if u32::try_from(limits.max_processes.get()).is_err() {
        return Err(ConfigError::InvalidLimit("max_processes"));
    }
    if limits.jobs.get() > limits.max_processes.get() {
        return Err(ConfigError::JobsExceedsProcesses {
            jobs: limits.jobs.get(),
            max_processes: limits.max_processes.get(),
        });
    }
    for (name, duration) in [
        ("analyzer_timeout", limits.analyzer_timeout.get()),
        ("baseline_timeout", limits.baseline_timeout.get()),
        ("total_timeout", limits.total_timeout.get()),
    ] {
        if duration.is_zero() {
            return Err(ConfigError::InvalidLimit(name));
        }
    }
    if let MutantTimeout::Fixed(duration) = limits.mutant_timeout
        && duration.get().is_zero()
    {
        return Err(ConfigError::InvalidLimit("mutant_timeout"));
    }
    if limits
        .baseline_timeout
        .get()
        .checked_mul(2)
        .and_then(|value| value.checked_add(Duration::from_secs(1)))
        .is_none()
    {
        return Err(ConfigError::InvalidLimit("baseline_timeout"));
    }
    Ok(())
}

impl TryFrom<RawRunConfig> for RunConfig {
    type Error = ConfigError;

    fn try_from(raw: RawRunConfig) -> Result<Self, Self::Error> {
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
        validate_selection(&selection)?;
        if raw.resume && raw.session.is_none() {
            return Err(ConfigError::ResumeRequiresSession);
        }
        validate_test_argv(&raw.test_argv)?;
        let mut operators = if raw.operators.is_empty() {
            MutationOperatorSelection::all_legacy()
        } else {
            MutationOperatorSelection(BTreeSet::new())
        };
        for selector in &raw.operators {
            for operator in MutationOperatorSelection::parse_selector(selector)? {
                operators.include(operator);
            }
        }
        for selector in &raw.exclude_operators {
            for operator in MutationOperatorSelection::parse_selector(selector)? {
                operators.exclude(operator);
            }
        }
        validate_operators(&operators)?;
        let limits = RunLimits::try_from(&raw.limits)?;
        validate_limits(&limits)?;
        Ok(Self {
            root: raw.root,
            selection,
            fingerprint_includes: raw.fingerprint_includes,
            fingerprint_files: raw.fingerprint_files,
            fingerprint_inputs: Vec::new(),
            limits,
            test_argv: raw.test_argv,
            output: raw.output,
            session: raw.session,
            operators,
            allow_best_effort_memory: raw.allow_best_effort_memory,
            profile: raw.profile,
            resume: raw.resume,
        })
    }
}

#[must_use]
pub fn auto_mutant_timeout(baseline: Duration) -> Duration {
    baseline
        .checked_mul(2)
        .and_then(|value| value.checked_add(Duration::from_secs(1)))
        .unwrap_or(Duration::MAX)
        .max(Duration::from_secs(5))
}
