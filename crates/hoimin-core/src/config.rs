use std::collections::BTreeSet;
use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Duration;

use camino::{Utf8Component, Utf8PathBuf};
use hoimin_budget_math::{MutantTimeout, NonZeroDuration, auto_mutant_timeout};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CommandArg, LineSelection, Selection};

pub const MAX_JOBS: usize = 256;
pub const MAX_TIMEOUT: Duration = Duration::from_hours(100 * 365 * 24);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRunLimits {
    pub jobs: usize,
    pub max_mutants: usize,
    pub max_candidates: usize,
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
            baseline_timeout: Duration::from_secs(60),
            mutant_timeout: None,
            total_timeout: Duration::from_mins(5),
            max_memory: 1024 * 1024 * 1024,
            max_output: 1024 * 1024,
            max_copy_size: 1024 * 1024 * 1024,
            max_workspace_size: 8 * 1024 * 1024 * 1024,
            min_free_space: 10 * 1024 * 1024 * 1024,
            max_processes: 64,
        }
    }
}

// Keep declaration order stable: derived Ord determines saved selection ordering.
// This local declaration owns operator identity, not default or family membership.
macro_rules! mutation_operators {
    ($( $(#[$attribute:meta])* $variant:ident => $name:literal, )+) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        pub enum MutationOperator {
            $(
                $(#[$attribute])*
                #[serde(rename = $name)]
                $variant,
            )+
        }

        impl MutationOperator {
            /// Looks up an operator by its canonical ID.
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// Returns the canonical ID used by the CLI and serialized plans.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            fn all() -> &'static [Self] {
                &[$(Self::$variant,)+]
            }
        }
    };
}

mutation_operators! {
    CompareEqNe => "compare_eq_ne",
    CompareOrder => "compare_order",
    Membership => "membership",
    Identity => "identity",
    BooleanAndOr => "boolean_and_or",
    BinaryAddSub => "binary_add_sub",
    AugmentedAddSub => "augmented_add_sub",
    BinaryMulDiv => "binary_mul_div",
    AugmentedMulDiv => "augmented_mul_div",
    BinaryFloorMod => "binary_floor_mod",
    AugmentedFloorMod => "augmented_floor_mod",
    UnarySign => "unary_sign",
    RemoveNot => "remove_not",
    BooleanLiteral => "boolean_literal",
    BreakContinue => "break_continue",
    CollectionAnyAll => "collection_any_all",
    CollectionListTuple => "collection_list_tuple",
    CollectionSetFrozenset => "collection_set_frozenset",
    CollectionAppendInsert => "collection_append_insert",
    CollectionMinMax => "collection_min_max",
    CollectionSetAddDiscard => "collection_set_add_discard",
    CollectionSetRemoveDiscard => "collection_set_remove_discard",
    CollectionStringStartsEnds => "collection_string_starts_ends",
    CollectionStringSplitRsplit => "collection_string_split_rsplit",
    BitwiseAndOr => "bitwise_and_or",
    BitwiseShift => "bitwise_shift",
    BinaryPower => "binary_power",
    BinaryMatmul => "binary_matmul",
    AugmentedPower => "augmented_power",
    AugmentedMatmul => "augmented_matmul",
    BitwiseXor => "bitwise_xor",
    BitwiseInvert => "bitwise_invert",
    AugmentedBitwiseAndOr => "augmented_bitwise_and_or",
    AugmentedBitwiseXor => "augmented_bitwise_xor",
    AugmentedBitwiseShift => "augmented_bitwise_shift",
    OperatorFunction => "operator_function",
    StructureAppendExtend => "structure_append_extend",
    StructureMappingGetSubscript => "structure_mapping_get_subscript",
    StructureSortReverse => "structure_sort_reverse",
    StructureSortedReversed => "structure_sorted_reversed",
    StructureIndexNeighbor => "structure_index_neighbor",
    StructureSliceNeighbor => "structure_slice_neighbor",
    ExceptionTypePair => "exception_type_pair",
    ExceptionHierarchy => "exception_hierarchy",
    ExceptionBareToException => "exception_bare_to_exception",
    ExceptionExceptionToBare => "exception_exception_to_bare",
    ExceptionBaseBoundary => "exception_base_boundary",
    ExceptionTupleAddPair => "exception_tuple_add_pair",
    ExceptionTupleRemoveMember => "exception_tuple_remove_member",
    TypeNullableRemove => "type_nullable_remove",
    TypeNullableAdd => "type_nullable_add",
    TypeListSequence => "type_list_sequence",
    TypeSetAbstractSet => "type_set_abstract_set",
    TypeMapping => "type_dict_mapping",
    TypeIterableIterator => "type_iterable_iterator",
    TypeSequenceIterable => "type_sequence_iterable",
    StatementDelete => "statement_delete",
    IntegerLiteralNeighbor => "integer_literal_neighbor",
    ConditionConstant => "condition_constant",
    FunctionBodyErase => "function_body_erase",
    EnumMemberReplace => "enum_member_replace",
    AugmentedToAssignment => "augmented_to_assignment",
    ReturnTupleSwap => "return_tuple_swap",
    StringLiteralEmpty => "string_literal_empty",
    WhileConditionFalse => "while_condition_false",
    ConditionClauseDelete => "condition_clause_delete",
    ContainerElementDelete => "container_element_delete",
    ConversionCallRemove => "conversion_call_remove",
    OptionalKeywordDelete => "optional_keyword_delete",
    MethodCallRemove => "method_call_remove",
    FunctionBodyReturnConstant => "function_body_return_constant",
    StringSegmentEmpty => "string_segment_empty",
    StdlibConversionCallRemove => "stdlib_conversion_call_remove",
    LoopBreakInsert => "loop_break_insert",
    IteratorStopInsert => "iterator_stop_insert",
    AsyncIteratorStopInsert => "async_iterator_stop_insert",
    ListComprehensionToGenerator => "list_comprehension_to_generator",
    GeneratorReturnInsert => "generator_return_insert",
    GeneratorSkipFirst => "generator_skip_first",
    GeneratorEmpty => "generator_empty",
    NoneTruthiness => "none_truthiness",
    MutableDefaultShare => "mutable_default_share",
    NonlocalRemove => "nonlocal_remove",
    YieldFromToYield => "yield_from_to_yield",
    MappingSetdefaultToGet => "mapping_setdefault_to_get",
    BisectBoundarySwap => "bisect_boundary_swap",
    DeepcopyToCopy => "deepcopy_to_copy",
    DequePopleftToPop => "deque_popleft_to_pop",
    GatherReturnExceptions => "gather_return_exceptions",
    ItertoolsTakeDropSwap => "itertools_take_drop_swap",
    FStringConversionSwap => "fstring_conversion_swap",
    BinaryDivFloor => "binary_div_floor",
    StructureSliceBoundOmit => "structure_slice_bound_omit",
    StructureSliceBoundMove => "structure_slice_bound_move",
    ContinueRemove => "continue_remove",
    ComparisonNotImplemented => "comparison_not_implemented",
    ExceptionCauseRemove => "exception_cause_remove",
    DataclassFactoryShare => "dataclass_factory_share",
    SortKeySwap => "sort_key_swap",
    LambdaCaptureLate => "lambda_capture_late",
    EncodingErrorsIgnore => "encoding_errors_ignore",
    BaseOrderShuffle => "base_order_shuffle",
}

// Family membership is an explicit selection policy, separate from the full
// catalog and current defaults. Both CLI advertising and expansion use these
// names and preserve each family's existing member order.
const SELECTOR_FAMILIES: &[(&str, &[MutationOperator])] = &[
    (
        "type_nullable",
        &[
            MutationOperator::TypeNullableRemove,
            MutationOperator::TypeNullableAdd,
        ],
    ),
    (
        "type_collections",
        &[
            MutationOperator::TypeListSequence,
            MutationOperator::TypeSetAbstractSet,
            MutationOperator::TypeMapping,
        ],
    ),
    (
        "type_iterables",
        &[
            MutationOperator::TypeIterableIterator,
            MutationOperator::TypeSequenceIterable,
        ],
    ),
    (
        "collection_ops",
        &[
            MutationOperator::CollectionAnyAll,
            MutationOperator::CollectionListTuple,
            MutationOperator::CollectionSetFrozenset,
            MutationOperator::CollectionAppendInsert,
            MutationOperator::CollectionMinMax,
            MutationOperator::CollectionSetAddDiscard,
            MutationOperator::CollectionSetRemoveDiscard,
            MutationOperator::CollectionStringStartsEnds,
            MutationOperator::CollectionStringSplitRsplit,
        ],
    ),
    (
        "structure_ops",
        &[
            MutationOperator::StructureAppendExtend,
            MutationOperator::StructureMappingGetSubscript,
            MutationOperator::StructureSortReverse,
            MutationOperator::StructureSortedReversed,
            MutationOperator::StructureIndexNeighbor,
            MutationOperator::StructureSliceNeighbor,
            MutationOperator::StructureSliceBoundOmit,
            MutationOperator::StructureSliceBoundMove,
        ],
    ),
    (
        "bitwise_ops",
        &[
            MutationOperator::BitwiseAndOr,
            MutationOperator::BitwiseShift,
            MutationOperator::BitwiseXor,
            MutationOperator::BitwiseInvert,
            MutationOperator::AugmentedBitwiseAndOr,
            MutationOperator::AugmentedBitwiseXor,
            MutationOperator::AugmentedBitwiseShift,
        ],
    ),
    ("exception_ops", &[MutationOperator::ExceptionTypePair]),
    (
        "exception_risky",
        &[
            MutationOperator::ExceptionBareToException,
            MutationOperator::ExceptionExceptionToBare,
            MutationOperator::ExceptionBaseBoundary,
            MutationOperator::ExceptionTupleAddPair,
            MutationOperator::ExceptionTupleRemoveMember,
        ],
    ),
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationOperatorSelection(BTreeSet<MutationOperator>);

impl MutationOperatorSelection {
    /// Returns canonical operator IDs and selector-family names accepted by the CLI.
    #[must_use]
    pub fn valid_names() -> Vec<&'static str> {
        let mut names: Vec<_> = MutationOperator::all()
            .iter()
            .copied()
            .map(MutationOperator::as_str)
            .collect();
        names.extend(SELECTOR_FAMILIES.iter().map(|(name, _)| *name));
        names.sort_unstable();
        names
    }

    /// Expands a selector name into its constituent mutation operators.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UnknownMutationOperator`] for an unrecognized selector.
    pub fn parse_selector(selector: &str) -> Result<Vec<MutationOperator>, ConfigError> {
        if let Some((_, operators)) = SELECTOR_FAMILIES.iter().find(|(name, _)| *name == selector) {
            return Ok(operators.to_vec());
        }
        MutationOperator::from_name(selector).map_or_else(
            || {
                Err(ConfigError::UnknownMutationOperator {
                    value: selector.to_owned(),
                })
            },
            |operator| Ok(vec![operator]),
        )
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
        // Default membership is a policy independent of the catalog and selector families.
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
                MutationOperator::BinaryDivFloor,
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
                MutationOperator::StructureSliceBoundOmit,
                MutationOperator::StructureSliceBoundMove,
                MutationOperator::ExceptionTypePair,
                MutationOperator::StatementDelete,
                MutationOperator::IntegerLiteralNeighbor,
                MutationOperator::AugmentedToAssignment,
                MutationOperator::ReturnTupleSwap,
                MutationOperator::StringLiteralEmpty,
                MutationOperator::WhileConditionFalse,
                MutationOperator::ConversionCallRemove,
                MutationOperator::MethodCallRemove,
                MutationOperator::FunctionBodyReturnConstant,
                MutationOperator::StringSegmentEmpty,
                MutationOperator::StdlibConversionCallRemove,
                MutationOperator::LoopBreakInsert,
                MutationOperator::IteratorStopInsert,
                MutationOperator::AsyncIteratorStopInsert,
                MutationOperator::ListComprehensionToGenerator,
                MutationOperator::GeneratorReturnInsert,
                MutationOperator::GeneratorSkipFirst,
                MutationOperator::GeneratorEmpty,
                MutationOperator::NoneTruthiness,
                MutationOperator::MutableDefaultShare,
                MutationOperator::NonlocalRemove,
                MutationOperator::YieldFromToYield,
                MutationOperator::MappingSetdefaultToGet,
                MutationOperator::BisectBoundarySwap,
                MutationOperator::DeepcopyToCopy,
                MutationOperator::DequePopleftToPop,
                MutationOperator::GatherReturnExceptions,
                MutationOperator::ItertoolsTakeDropSwap,
                MutationOperator::FStringConversionSwap,
                MutationOperator::ContinueRemove,
                MutationOperator::ComparisonNotImplemented,
                MutationOperator::ExceptionCauseRemove,
                MutationOperator::DataclassFactoryShare,
                MutationOperator::SortKeySwap,
                MutationOperator::LambdaCaptureLate,
                MutationOperator::EncodingErrorsIgnore,
                MutationOperator::BaseOrderShuffle,
            ]
            .into_iter()
            .collect(),
        )
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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    #[serde(default)]
    pub metrics: Option<Utf8PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    pub path: Utf8PathBuf,
}

/// Session intent after CLI validation; enabled variants always own their path.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "config",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SessionMode {
    #[default]
    Disabled,
    Fresh(SessionConfig),
    Resume(SessionConfig),
}

impl SessionMode {
    /// Returns the session configuration for either enabled mode.
    #[must_use]
    pub fn config(&self) -> Option<&SessionConfig> {
        match self {
            Self::Disabled => None,
            Self::Fresh(config) | Self::Resume(config) => Some(config),
        }
    }

    /// Exposes the enabled path for filesystem normalization without changing intent.
    pub fn config_mut(&mut self) -> Option<&mut SessionConfig> {
        match self {
            Self::Disabled => None,
            Self::Fresh(config) | Self::Resume(config) => Some(config),
        }
    }

    #[must_use]
    pub const fn is_resume(&self) -> bool {
        matches!(self, Self::Resume(_))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunLimits {
    pub jobs: NonZeroUsize,
    pub max_mutants: NonZeroUsize,
    pub max_candidates: NonZeroUsize,
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

/// Semantically checked execution settings, immutable after acceptance.
/// Construction and deserialization check selection, normalized import roots,
/// operators, argv, and limits without filesystem access. Environment request and
/// snapshot types own their metadata invariants. This does not establish input
/// freshness or output-destination safety; preparation owns those checks.
/// Read fields through the shared view. Consume into construction data and use
/// `TryFrom` for arbitrary edits.
///
/// Accepted configuration cannot be invalidated by a field write:
/// ```compile_fail
/// fn change_jobs(mut config: hoimin_core::RunConfig) {
///     config.limits.jobs = std::num::NonZeroUsize::new(257).unwrap();
/// }
/// ```
/// Nested collection mutation also requires checked reconstruction:
/// ```compile_fail
/// fn remove_command(mut config: hoimin_core::RunConfig) {
///     config.test_argv.clear();
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RunConfig<E = crate::EnvironmentSnapshot>(RunConfigData<E>);

/// Owned normalized construction data, not accepted execution settings.
/// Fields may be edited before `RunConfig::try_from` checks their semantics.
/// The project root belongs to `selection`. Saved arrays remain required;
/// an explicit empty array means no configured entries where the field permits it.
///
/// Execution does not accept construction data:
/// ```compile_fail
/// fn execute(data: hoimin_core::RunConfigData, control: hoimin_core::ResourceControl) {
///     hoimin_core::RunState::new("run", data, control);
/// }
/// ```
/// Fingerprint generation also requires the checked owner:
/// ```compile_fail
/// fn fingerprint(data: hoimin_core::RunConfigData) {
///     hoimin_core::FingerprintInput::from_config(
///         &data, vec![], vec![], hoimin_core::ResourceMode::Hard,
///     );
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfigData<E = crate::EnvironmentSnapshot> {
    pub selection: Selection,
    pub import_roots: Vec<Utf8PathBuf>,
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_files: Vec<String>,
    /// Canonical capture request before preparation, validated metadata afterward.
    pub environment: E,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub session: SessionMode,
    pub operators: MutationOperatorSelection,
    pub allow_best_effort_memory: bool,
    pub profile: MutationProfile,
}

/// Persisted plan data; `validate` checks header semantics and
/// `into_run_config` accepts execution settings through checked construction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanConfig {
    pub discovery: DiscoveryConfig,
    pub selection: Selection,
    pub import_roots: Vec<Utf8PathBuf>,
    pub limits: RunLimits,
    pub test_argv: Vec<CommandArg>,
    pub output: OutputConfig,
    pub operators: MutationOperatorSelection,
    pub allow_best_effort_memory: bool,
    pub profile: MutationProfile,
    pub fingerprint_includes: Vec<String>,
    pub fingerprint_files: Vec<String>,
    /// Validated inherited-environment metadata.
    pub environment: crate::EnvironmentSnapshot,
    pub fingerprint_inputs: Vec<FingerprintInputFile>,
}

impl<E> RunConfigData<E> {
    /// Validates common normalized run fields.
    /// Environment constructors and deserialization own their state invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when selection, mutation operators, the test command,
    /// or limits are invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_selection(&self.selection)?;
        validate_import_roots(&self.import_roots)?;
        validate_operators(&self.operators)?;
        validate_test_argv(&self.test_argv)?;
        validate_limits(&self.limits)
    }
}

impl<E> TryFrom<RunConfigData<E>> for RunConfig<E> {
    type Error = ConfigError;

    fn try_from(data: RunConfigData<E>) -> Result<Self, Self::Error> {
        data.validate()?;
        Ok(Self(data))
    }
}

impl<'de, E: Deserialize<'de>> Deserialize<'de> for RunConfig<E> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Decode the original map so duplicate known keys are still visible.
        let data = RunConfigData::<E>::deserialize(deserializer)?;
        Self::try_from(data).map_err(serde::de::Error::custom)
    }
}

impl<E> std::ops::Deref for RunConfig<E> {
    type Target = RunConfigData<E>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<E> RunConfig<E> {
    /// Gives up the checked owner for editing; execution requires checking again.
    #[must_use]
    pub fn into_data(self) -> RunConfigData<E> {
        self.0
    }

    /// Replaces presentation settings while preserving normalized semantics.
    /// Execution still checks metrics destination safety.
    #[must_use]
    pub fn with_output(mut self, output: OutputConfig) -> Self {
        self.0.output = output;
        self
    }

    /// Attaches resolved records without certifying their filesystem freshness.
    /// Preparation and verification retain ownership of record checks.
    #[must_use]
    pub fn with_fingerprint_inputs(mut self, inputs: Vec<FingerprintInputFile>) -> Self {
        self.0.fingerprint_inputs = inputs;
        self
    }
}

impl RunConfig {
    #[must_use]
    pub fn into_plan_config(self, discovery: DiscoveryConfig) -> PlanConfig {
        let data = self.into_data();
        PlanConfig {
            discovery,
            selection: data.selection,
            import_roots: data.import_roots,
            limits: data.limits,
            test_argv: data.test_argv,
            output: data.output,
            operators: data.operators,
            allow_best_effort_memory: data.allow_best_effort_memory,
            profile: data.profile,
            fingerprint_includes: data.fingerprint_includes,
            fingerprint_files: data.fingerprint_files,
            environment: data.environment,
            fingerprint_inputs: data.fingerprint_inputs,
        }
    }
}

impl RunConfig<crate::EnvironmentRequest> {
    /// Finishes environment preparation with a snapshot of exactly the requested names.
    ///
    /// Execution and fingerprint generation accept only the returned snapshot configuration.
    ///
    /// ```compile_fail
    /// use hoimin_core::{FingerprintInput, RawRunConfig, ResourceMode, RunConfig};
    /// let request = RunConfig::try_from(RawRunConfig::default()).unwrap();
    /// FingerprintInput::from_config(&request, vec![], vec![], ResourceMode::Hard);
    /// ```
    ///
    /// # Errors
    /// Returns an error when the snapshot's name set differs from the request.
    pub fn with_environment(
        self,
        environment: crate::EnvironmentSnapshot,
    ) -> Result<RunConfig, ConfigError> {
        if self.environment.names() != environment.names() {
            return Err(ConfigError::FingerprintEnvSnapshotMismatch);
        }
        let data = self.into_data();
        // Only the typed environment changes; the shared normalized fields keep
        // the validity established when this request configuration was accepted.
        Ok(RunConfig(RunConfigData {
            selection: data.selection,
            import_roots: data.import_roots,
            fingerprint_includes: data.fingerprint_includes,
            fingerprint_files: data.fingerprint_files,
            environment,
            fingerprint_inputs: data.fingerprint_inputs,
            limits: data.limits,
            test_argv: data.test_argv,
            output: data.output,
            session: data.session,
            operators: data.operators,
            allow_best_effort_memory: data.allow_best_effort_memory,
            profile: data.profile,
        }))
    }
}

/// Settings for cooperative discovery across the complete selected target set.
/// These are saved in plans and discarded before execution or fingerprinting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryConfig {
    pub analyzer_timeout: NonZeroDuration,
}

impl DiscoveryConfig {
    /// Creates a positive discovery deadline bounded by [`MAX_TIMEOUT`].
    ///
    /// # Errors
    /// Returns [`ConfigError`] for zero or oversized timeouts.
    pub fn new(analyzer_timeout: Duration) -> Result<Self, ConfigError> {
        let config = Self {
            analyzer_timeout: NonZeroDuration::new(analyzer_timeout)
                .ok_or(ConfigError::InvalidLimit("analyzer_timeout"))?,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validates the discovery deadline after deserialization.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when the timeout exceeds [`MAX_TIMEOUT`].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_timeout("analyzer_timeout", self.analyzer_timeout.get())
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
        self.discovery.validate()?;
        validate_selection(&self.selection)?;
        validate_import_roots(&self.import_roots)?;
        validate_operators(&self.operators)?;
        validate_test_argv(&self.test_argv)?;
        validate_limits(&self.limits)
    }

    /// Accepts persisted settings for execution, with sessions disabled.
    ///
    /// # Errors
    /// Rejects invalid discovery or normalized execution semantics even when the
    /// caller has not validated a plan header.
    pub fn into_run_config(self, output: OutputConfig) -> Result<RunConfig, ConfigError> {
        self.discovery.validate()?;
        RunConfig::try_from(RunConfigData {
            selection: self.selection,
            import_roots: self.import_roots,
            fingerprint_includes: self.fingerprint_includes,
            fingerprint_files: self.fingerprint_files,
            environment: self.environment,
            fingerprint_inputs: self.fingerprint_inputs,
            limits: self.limits,
            test_argv: self.test_argv,
            output,
            session: SessionMode::Disabled,
            operators: self.operators,
            allow_best_effort_memory: self.allow_best_effort_memory,
            profile: self.profile,
        })
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
    #[error("fingerprint.env.snapshot_mismatch: captured names differ from requested names")]
    FingerprintEnvSnapshotMismatch,
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
        let limits = Self {
            jobs: nonzero_usize(raw.jobs, "jobs")?,
            max_mutants: nonzero_usize(raw.max_mutants, "max_mutants")?,
            max_candidates: nonzero_usize(raw.max_candidates, "max_candidates")?,
            baseline_timeout: NonZeroDuration::new(raw.baseline_timeout)
                .ok_or(ConfigError::InvalidLimit("baseline_timeout"))?,
            mutant_timeout: match raw.mutant_timeout {
                Some(value) => MutantTimeout::Fixed(
                    NonZeroDuration::new(value)
                        .ok_or(ConfigError::InvalidLimit("mutant_timeout"))?,
                ),
                None => MutantTimeout::Auto,
            },
            total_timeout: NonZeroDuration::new(raw.total_timeout)
                .ok_or(ConfigError::InvalidLimit("total_timeout"))?,
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
        || selection.changed.is_some();
    if !has_selector {
        return Err(ConfigError::MissingSelector);
    }
    if selection.changed.is_some() && selection.sources.is_empty() {
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
    for (name, duration) in [
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
    if duration > MAX_TIMEOUT {
        Err(ConfigError::InvalidLimit(name))
    } else {
        Ok(())
    }
}

/// Consume CLI-only relationships before passing a coherent value to consumers.
fn normalize_changed_selection(
    enabled: bool,
    context: u32,
    diff_base: Option<String>,
) -> Result<Option<crate::ChangedSelection>, ConfigError> {
    let changed = crate::ChangedSelection::new(context, diff_base)?;
    if enabled {
        return Ok(Some(changed));
    }
    if context != 0 {
        return Err(ConfigError::ChangedContextRequiresChanged);
    }
    if changed.diff_base().is_some() {
        return Err(ConfigError::DiffBaseRequiresChanged);
    }
    Ok(None)
}

impl TryFrom<RawRunConfig> for RunConfig<crate::EnvironmentRequest> {
    type Error = ConfigError;

    fn try_from(raw: RawRunConfig) -> Result<Self, Self::Error> {
        let selection = Selection {
            root: raw.root,
            sources: raw.sources,
            files: raw.files,
            lines: raw.lines,
            symbols: raw.symbols,
            changed: normalize_changed_selection(raw.changed, raw.changed_context, raw.diff_base)?,
            includes: raw.includes,
            excludes: raw.excludes,
        };
        validate_selection(&selection)?;
        let session = match (raw.session, raw.resume) {
            (None, false) => SessionMode::Disabled,
            (Some(config), false) => SessionMode::Fresh(config),
            (Some(config), true) => SessionMode::Resume(config),
            (None, true) => return Err(ConfigError::ResumeRequiresSession),
        };
        validate_test_argv(&raw.test_argv)?;
        let mut operators = if raw.operators.is_empty() {
            MutationOperatorSelection::default()
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
        Self::try_from(RunConfigData {
            selection,
            import_roots: normalize_import_roots(&raw.import_roots)?,
            fingerprint_includes: raw.fingerprint_includes,
            fingerprint_files: raw.fingerprint_files,
            environment: crate::EnvironmentRequest::new(&raw.fingerprint_env)?,
            fingerprint_inputs: Vec::new(),
            limits,
            test_argv: raw.test_argv,
            output: raw.output,
            session,
            operators,
            allow_best_effort_memory: raw.allow_best_effort_memory,
            profile: raw.profile,
        })
    }
}
