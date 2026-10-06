use std::collections::BTreeSet;
use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Duration;

use camino::{Utf8Component, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CommandArg, LineSelection, Selection};

pub const MAX_JOBS: usize = 256;
pub const MAX_TIMEOUT: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);

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
    pub max_workspace_size: u64,
    pub min_free_space: u64,
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
            max_workspace_size: 8 * 1024 * 1024 * 1024,
            min_free_space: 10 * 1024 * 1024 * 1024,
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
    AugmentedMulDiv,
    BinaryFloorMod,
    AugmentedFloorMod,
    UnarySign,
    RemoveNot,
    BooleanLiteral,
    BreakContinue,
    CollectionAnyAll,
    CollectionListTuple,
    CollectionSetFrozenset,
    CollectionAppendInsert,
    CollectionMinMax,
    CollectionSetAddDiscard,
    CollectionSetRemoveDiscard,
    CollectionStringStartsEnds,
    CollectionStringSplitRsplit,
    BitwiseAndOr,
    BitwiseShift,
    BinaryPower,
    BinaryMatmul,
    AugmentedPower,
    AugmentedMatmul,
    BitwiseXor,
    BitwiseInvert,
    AugmentedBitwiseAndOr,
    AugmentedBitwiseXor,
    AugmentedBitwiseShift,
    OperatorFunction,
    StructureAppendExtend,
    StructureMappingGetSubscript,
    StructureSortReverse,
    StructureSortedReversed,
    StructureIndexNeighbor,
    StructureSliceNeighbor,
    ExceptionTypePair,
    ExceptionHierarchy,
    ExceptionBareToException,
    ExceptionExceptionToBare,
    ExceptionBaseBoundary,
    ExceptionTupleAddPair,
    ExceptionTupleRemoveMember,
    TypeNullableRemove,
    TypeNullableAdd,
    TypeListSequence,
    TypeSetAbstractSet,
    #[serde(rename = "type_dict_mapping", alias = "type_mapping")]
    TypeMapping,
    TypeIterableIterator,
    TypeSequenceIterable,
    StatementDelete,
    IntegerLiteralNeighbor,
    ConditionConstant,
    FunctionBodyErase,
    EnumMemberReplace,
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
            Self::FunctionBodyErase => "function_body_erase",
            Self::EnumMemberReplace => "enum_member_replace",
            Self::ConditionConstant => "condition_constant",
            Self::IntegerLiteralNeighbor => "integer_literal_neighbor",
            Self::StatementDelete => "statement_delete",
            Self::CompareEqNe => "compare_eq_ne",
            Self::CompareOrder => "compare_order",
            Self::Membership => "membership",
            Self::Identity => "identity",
            Self::BooleanAndOr => "boolean_and_or",
            Self::BinaryAddSub => "binary_add_sub",
            Self::AugmentedAddSub => "augmented_add_sub",
            Self::BinaryMulDiv => "binary_mul_div",
            Self::AugmentedMulDiv => "augmented_mul_div",
            Self::BinaryFloorMod => "binary_floor_mod",
            Self::AugmentedFloorMod => "augmented_floor_mod",
            Self::UnarySign => "unary_sign",
            Self::RemoveNot => "remove_not",
            Self::BooleanLiteral => "boolean_literal",
            Self::BreakContinue => "break_continue",
            Self::CollectionAnyAll => "collection_any_all",
            Self::CollectionListTuple => "collection_list_tuple",
            Self::CollectionSetFrozenset => "collection_set_frozenset",
            Self::CollectionAppendInsert => "collection_append_insert",
            Self::CollectionMinMax => "collection_min_max",
            Self::CollectionSetAddDiscard => "collection_set_add_discard",
            Self::CollectionSetRemoveDiscard => "collection_set_remove_discard",
            Self::CollectionStringStartsEnds => "collection_string_starts_ends",
            Self::CollectionStringSplitRsplit => "collection_string_split_rsplit",
            Self::BitwiseAndOr => "bitwise_and_or",
            Self::BitwiseShift => "bitwise_shift",
            Self::BinaryPower => "binary_power",
            Self::BinaryMatmul => "binary_matmul",
            Self::AugmentedPower => "augmented_power",
            Self::AugmentedMatmul => "augmented_matmul",
            Self::BitwiseXor => "bitwise_xor",
            Self::BitwiseInvert => "bitwise_invert",
            Self::AugmentedBitwiseAndOr => "augmented_bitwise_and_or",
            Self::AugmentedBitwiseXor => "augmented_bitwise_xor",
            Self::AugmentedBitwiseShift => "augmented_bitwise_shift",
            Self::OperatorFunction => "operator_function",
            Self::StructureAppendExtend => "structure_append_extend",
            Self::StructureMappingGetSubscript => "structure_mapping_get_subscript",
            Self::StructureSortReverse => "structure_sort_reverse",
            Self::StructureSortedReversed => "structure_sorted_reversed",
            Self::StructureIndexNeighbor => "structure_index_neighbor",
            Self::StructureSliceNeighbor => "structure_slice_neighbor",
            Self::ExceptionTypePair => "exception_type_pair",
            Self::ExceptionHierarchy => "exception_hierarchy",
            Self::ExceptionBareToException => "exception_bare_to_exception",
            Self::ExceptionExceptionToBare => "exception_exception_to_bare",
            Self::ExceptionBaseBoundary => "exception_base_boundary",
            Self::ExceptionTupleAddPair => "exception_tuple_add_pair",
            Self::ExceptionTupleRemoveMember => "exception_tuple_remove_member",
            Self::TypeNullableRemove => "type_nullable_remove",
            Self::TypeNullableAdd => "type_nullable_add",
            Self::TypeListSequence => "type_list_sequence",
            Self::TypeSetAbstractSet => "type_set_abstract_set",
            Self::TypeMapping => "type_dict_mapping",
            Self::TypeIterableIterator => "type_iterable_iterator",
            Self::TypeSequenceIterable => "type_sequence_iterable",
        }
    }
    fn all() -> [Self; 61] {
        [
            Self::CompareEqNe,
            Self::CompareOrder,
            Self::Membership,
            Self::Identity,
            Self::BooleanAndOr,
            Self::BinaryAddSub,
            Self::AugmentedAddSub,
            Self::BinaryMulDiv,
            Self::AugmentedMulDiv,
            Self::BinaryFloorMod,
            Self::AugmentedFloorMod,
            Self::UnarySign,
            Self::RemoveNot,
            Self::BooleanLiteral,
            Self::BreakContinue,
            Self::CollectionAnyAll,
            Self::CollectionListTuple,
            Self::CollectionSetFrozenset,
            Self::CollectionAppendInsert,
            Self::CollectionMinMax,
            Self::CollectionSetAddDiscard,
            Self::CollectionSetRemoveDiscard,
            Self::CollectionStringStartsEnds,
            Self::CollectionStringSplitRsplit,
            Self::BitwiseAndOr,
            Self::BitwiseShift,
            Self::BinaryPower,
            Self::BinaryMatmul,
            Self::AugmentedPower,
            Self::AugmentedMatmul,
            Self::BitwiseXor,
            Self::BitwiseInvert,
            Self::AugmentedBitwiseAndOr,
            Self::AugmentedBitwiseXor,
            Self::AugmentedBitwiseShift,
            Self::OperatorFunction,
            Self::StructureAppendExtend,
            Self::StructureMappingGetSubscript,
            Self::StructureSortReverse,
            Self::StructureSortedReversed,
            Self::StructureIndexNeighbor,
            Self::StructureSliceNeighbor,
            Self::ExceptionTypePair,
            Self::ExceptionHierarchy,
            Self::ExceptionBareToException,
            Self::ExceptionExceptionToBare,
            Self::ExceptionBaseBoundary,
            Self::ExceptionTupleAddPair,
            Self::ExceptionTupleRemoveMember,
            Self::TypeNullableRemove,
            Self::TypeNullableAdd,
            Self::TypeListSequence,
            Self::TypeSetAbstractSet,
            Self::TypeMapping,
            Self::TypeIterableIterator,
            Self::TypeSequenceIterable,
            Self::StatementDelete,
            Self::IntegerLiteralNeighbor,
            Self::ConditionConstant,
            Self::FunctionBodyErase,
            Self::EnumMemberReplace,
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
        names.extend([
            "type_nullable",
            "type_collections",
            "type_iterables",
            "collection_ops",
            "structure_ops",
            "bitwise_ops",
            "exception_ops",
            "exception_risky",
        ]);
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
                MutationOperator::AugmentedMulDiv,
                MutationOperator::BinaryFloorMod,
                MutationOperator::AugmentedFloorMod,
                MutationOperator::UnarySign,
                MutationOperator::RemoveNot,
                MutationOperator::BooleanLiteral,
                MutationOperator::BreakContinue,
                MutationOperator::CollectionAnyAll,
                MutationOperator::CollectionListTuple,
                MutationOperator::CollectionSetFrozenset,
                MutationOperator::CollectionAppendInsert,
                MutationOperator::CollectionMinMax,
                MutationOperator::CollectionSetAddDiscard,
                MutationOperator::CollectionSetRemoveDiscard,
                MutationOperator::CollectionStringStartsEnds,
                MutationOperator::CollectionStringSplitRsplit,
                MutationOperator::BitwiseAndOr,
                MutationOperator::BitwiseShift,
                MutationOperator::BinaryPower,
                MutationOperator::BinaryMatmul,
                MutationOperator::AugmentedPower,
                MutationOperator::AugmentedMatmul,
                MutationOperator::BitwiseXor,
                MutationOperator::BitwiseInvert,
                MutationOperator::AugmentedBitwiseAndOr,
                MutationOperator::AugmentedBitwiseXor,
                MutationOperator::AugmentedBitwiseShift,
                MutationOperator::OperatorFunction,
                MutationOperator::StructureAppendExtend,
                MutationOperator::StructureMappingGetSubscript,
                MutationOperator::StructureSortReverse,
                MutationOperator::StructureSortedReversed,
                MutationOperator::StructureIndexNeighbor,
                MutationOperator::StructureSliceNeighbor,
                MutationOperator::ExceptionTypePair,
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
            "collection_ops" => Ok(vec![
                MutationOperator::CollectionAnyAll,
                MutationOperator::CollectionListTuple,
                MutationOperator::CollectionSetFrozenset,
                MutationOperator::CollectionAppendInsert,
                MutationOperator::CollectionMinMax,
                MutationOperator::CollectionSetAddDiscard,
                MutationOperator::CollectionSetRemoveDiscard,
                MutationOperator::CollectionStringStartsEnds,
                MutationOperator::CollectionStringSplitRsplit,
            ]),
            "structure_ops" => Ok(vec![
                MutationOperator::StructureAppendExtend,
                MutationOperator::StructureMappingGetSubscript,
                MutationOperator::StructureSortReverse,
                MutationOperator::StructureSortedReversed,
                MutationOperator::StructureIndexNeighbor,
                MutationOperator::StructureSliceNeighbor,
            ]),
            "bitwise_ops" => Ok(vec![
                MutationOperator::BitwiseAndOr,
                MutationOperator::BitwiseShift,
                MutationOperator::BitwiseXor,
                MutationOperator::BitwiseInvert,
                MutationOperator::AugmentedBitwiseAndOr,
                MutationOperator::AugmentedBitwiseXor,
                MutationOperator::AugmentedBitwiseShift,
            ]),
            "exception_ops" => Ok(vec![MutationOperator::ExceptionTypePair]),
            "exception_risky" => Ok(vec![
                MutationOperator::ExceptionBareToException,
                MutationOperator::ExceptionExceptionToBare,
                MutationOperator::ExceptionBaseBoundary,
                MutationOperator::ExceptionTupleAddPair,
                MutationOperator::ExceptionTupleRemoveMember,
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
    #[serde(default)]
    pub import_roots: Vec<Utf8PathBuf>,
    pub files: Vec<Utf8PathBuf>,
    pub lines: Vec<LineSelection>,
    pub symbols: Vec<crate::SymbolSelection>,
    pub changed: bool,
    pub changed_context: u32,
    pub diff_base: Option<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fingerprint_env: Vec<String>,
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
#[serde(rename_all = "snake_case")]
pub enum MutantTimeout {
    #[serde(alias = "Auto")]
    Auto,
    #[serde(alias = "Fixed")]
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
    pub max_workspace_size: NonZeroU64,
    pub min_free_space: NonZeroU64,
    pub max_processes: NonZeroUsize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunConfig {
    pub root: Utf8PathBuf,
    pub selection: Selection,
    #[serde(default)]
    pub import_roots: Vec<Utf8PathBuf>,
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fingerprint_env: Vec<String>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_env_hash: Option<String>,
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
    pub import_roots: Vec<Utf8PathBuf>,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub operators: MutationOperatorSelection,
    pub allow_best_effort_memory: bool,
    pub profile: MutationProfile,
    pub fingerprint_includes: Vec<String>,
    #[serde(default)]
    pub fingerprint_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fingerprint_env: Vec<String>,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_env_hash: Option<String>,
}

impl RunConfig {
    /// Validates normalized run configuration semantics.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when selection, mutation operators, runtime resume state, the test
    /// command, or limits are invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        crate::fingerprint_env::validate_fingerprint_env(
            &self.fingerprint_env,
            self.fingerprint_env_hash.as_deref(),
            false,
        )?;
        validate_selection(&self.selection)?;
        validate_import_roots(&self.import_roots)?;
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
            import_roots: self.import_roots,
            limits: self.limits,
            test_argv: self.test_argv,
            output: self.output,
            operators: self.operators,
            allow_best_effort_memory: self.allow_best_effort_memory,
            profile: self.profile,
            fingerprint_includes: self.fingerprint_includes,
            fingerprint_files: self.fingerprint_files,
            fingerprint_env: self.fingerprint_env,
            fingerprint_env_hash: self.fingerprint_env_hash,
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
        crate::fingerprint_env::validate_fingerprint_env(
            &self.fingerprint_env,
            self.fingerprint_env_hash.as_deref(),
            true,
        )?;
        validate_selection(&self.selection)?;
        validate_import_roots(&self.import_roots)?;
        validate_operators(&self.operators)?;
        validate_test_argv(&self.test_argv)?;
        validate_limits(&self.limits)
    }

    #[must_use]
    pub fn into_run_config(self, output: OutputConfig) -> RunConfig {
        RunConfig {
            root: self.root,
            selection: self.selection,
            import_roots: self.import_roots,
            fingerprint_includes: self.fingerprint_includes,
            fingerprint_files: self.fingerprint_files,
            fingerprint_env: self.fingerprint_env,
            fingerprint_env_hash: self.fingerprint_env_hash,
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
    #[error("fingerprint.env.invalid_name: --fingerprint-env requires [A-Za-z_][A-Za-z0-9_]*")]
    InvalidFingerprintEnvName,
    #[error("fingerprint.env.non_normalized: names must be canonical, sorted, and unique")]
    NonNormalizedFingerprintEnv,
    #[error("fingerprint.env.invalid_hash: selected names require a lowercase 64-hex digest")]
    InvalidFingerprintEnvHash,
    #[error(
        "invalid --import-root {path}: expected a project-root-relative directory without escaping parent components"
    )]
    InvalidImportRoot { path: Utf8PathBuf },
    #[error("import_roots must be normalized and contain no duplicates")]
    NonNormalizedImportRoots,
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
    #[error("--changed-context requires --changed")]
    ChangedContextRequiresChanged,
    #[error("--changed-context {context} exceeds the supported maximum {maximum}")]
    ChangedContextTooLarge { context: u32, maximum: u32 },
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
        "max_workspace_size" => "--max-workspace-size",
        "min_free_space" => "--min-free-space",
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
        let limits = Self {
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
            max_workspace_size: NonZeroU64::new(raw.max_workspace_size)
                .ok_or(ConfigError::InvalidLimit("max_workspace_size"))?,
            min_free_space: NonZeroU64::new(raw.min_free_space)
                .ok_or(ConfigError::InvalidLimit("min_free_space"))?,
            max_processes: nonzero_usize(raw.max_processes, "max_processes")?,
        };
        validate_limits(&limits)?;
        Ok(limits)
    }
}

fn nonzero_usize(value: usize, name: &'static str) -> Result<NonZeroUsize, ConfigError> {
    NonZeroUsize::new(value).ok_or(ConfigError::InvalidLimit(name))
}

// Pure path validation: copied-directory availability belongs to workspace execution.
fn normalize_import_roots(roots: &[Utf8PathBuf]) -> Result<Vec<Utf8PathBuf>, ConfigError> {
    let mut normalized = Vec::new();
    let mut seen = BTreeSet::new();
    for path in roots {
        let invalid = || ConfigError::InvalidImportRoot { path: path.clone() };
        if path.as_str().is_empty() {
            return Err(invalid());
        }
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Utf8Component::CurDir => {}
                Utf8Component::Normal(part) => parts.push(part),
                Utf8Component::ParentDir => {
                    parts.pop().ok_or_else(invalid)?;
                }
                Utf8Component::Prefix(_) | Utf8Component::RootDir => return Err(invalid()),
            }
        }
        let path = Utf8PathBuf::from(if parts.is_empty() {
            ".".to_owned()
        } else {
            parts.join("/")
        });
        if seen.insert(path.clone()) {
            normalized.push(path);
        }
    }
    Ok(normalized)
}

fn validate_import_roots(roots: &[Utf8PathBuf]) -> Result<(), ConfigError> {
    let normalized = normalize_import_roots(roots)?;
    // Compare strings: Path equality itself ignores some harmless components.
    if normalized
        .iter()
        .map(|path| path.as_str())
        .eq(roots.iter().map(|path| path.as_str()))
    {
        Ok(())
    } else {
        Err(ConfigError::NonNormalizedImportRoots)
    }
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
    if selection.changed_context > crate::MAX_CHANGED_CONTEXT {
        return Err(ConfigError::ChangedContextTooLarge {
            context: selection.changed_context,
            maximum: crate::MAX_CHANGED_CONTEXT,
        });
    }
    if selection.changed_context != 0 && !selection.changed {
        return Err(ConfigError::ChangedContextRequiresChanged);
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
        validate_timeout(name, duration)?;
    }
    match limits.mutant_timeout {
        MutantTimeout::Auto => validate_timeout(
            "baseline_timeout",
            auto_mutant_timeout(limits.baseline_timeout.get()),
        )?,
        MutantTimeout::Fixed(duration) => {
            validate_timeout("mutant_timeout", duration.get())?;
        }
    }
    Ok(())
}

fn validate_timeout(name: &'static str, duration: Duration) -> Result<(), ConfigError> {
    if duration.is_zero() || duration > MAX_TIMEOUT {
        Err(ConfigError::InvalidLimit(name))
    } else {
        Ok(())
    }
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
            changed_context: raw.changed_context,
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
            import_roots: normalize_import_roots(&raw.import_roots)?,
            fingerprint_includes: raw.fingerprint_includes,
            fingerprint_files: raw.fingerprint_files,
            fingerprint_env: crate::normalize_fingerprint_env_names(&raw.fingerprint_env)?,
            fingerprint_env_hash: None,
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
