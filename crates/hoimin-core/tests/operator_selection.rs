#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
use hoimin_core::{
    CommandArg, ConfigError, MutationOperator, MutationOperatorSelection, PlanConfig, RawRunConfig,
    RunConfig,
};

#[rstest::rstest]
#[case::compare_eq_ne("compare_eq_ne")]
#[case::type_nullable("type_nullable")]
#[case::type_dict_mapping("type_dict_mapping")]
fn unknown_operator_error_lists_valid_operators_and_selectors(#[case] valid: &str) {
    let error = MutationOperatorSelection::parse_selector("not_a_real_operator").unwrap_err();

    let message = error.to_string();

    assert!(message.contains(valid), "missing {valid} in {message}");
}

#[rstest::rstest]
#[case::augmented_mul_div("augmented_mul_div")]
#[case::augmented_floor_mod("augmented_floor_mod")]
fn augmented_arithmetic_operator_ids_are_default_and_valid(#[case] name: &str) {
    let selected = MutationOperatorSelection::default();
    let selected_names = selected.names();
    let valid_names = MutationOperatorSelection::valid_names();

    assert!(selected_names.iter().any(|selected| selected == name));
    assert!(valid_names.contains(&name));
    assert_eq!(
        MutationOperatorSelection::parse_selector(name)
            .unwrap()
            .len(),
        1
    );
}

#[rstest::rstest]
#[case::binary_power("binary_power")]
#[case::binary_matmul("binary_matmul")]
#[case::augmented_power("augmented_power")]
#[case::augmented_matmul("augmented_matmul")]
#[case::bitwise_xor("bitwise_xor")]
#[case::bitwise_invert("bitwise_invert")]
#[case::augmented_bitwise_and_or("augmented_bitwise_and_or")]
#[case::augmented_bitwise_xor("augmented_bitwise_xor")]
#[case::augmented_bitwise_shift("augmented_bitwise_shift")]
#[case::operator_function("operator_function")]
fn native_python_operator_ids_are_default_valid_and_selectable(#[case] name: &str) {
    let selected = MutationOperatorSelection::default();
    let selected_names = selected.names();
    let valid_names = MutationOperatorSelection::valid_names();

    assert!(selected_names.iter().any(|selected| selected == name));
    assert!(valid_names.contains(&name));
    assert_eq!(
        MutationOperatorSelection::parse_selector(name)
            .unwrap()
            .len(),
        1
    );
}

#[rstest::rstest]
#[case::mutationoperator_collectionanyall(MutationOperator::CollectionAnyAll)]
#[case::mutationoperator_collectionlisttuple(MutationOperator::CollectionListTuple)]
#[case::mutationoperator_collectionsetfrozenset(MutationOperator::CollectionSetFrozenset)]
#[case::mutationoperator_collectionappendinsert(MutationOperator::CollectionAppendInsert)]
#[case::mutationoperator_collectionminmax(MutationOperator::CollectionMinMax)]
#[case::mutationoperator_collectionsetadddiscard(MutationOperator::CollectionSetAddDiscard)]
#[case::mutationoperator_collectionsetremovediscard(MutationOperator::CollectionSetRemoveDiscard)]
#[case::mutationoperator_collectionstringstartsends(MutationOperator::CollectionStringStartsEnds)]
#[case::mutationoperator_collectionstringsplitrsplit(MutationOperator::CollectionStringSplitRsplit)]
#[case::mutationoperator_bitwiseandor(MutationOperator::BitwiseAndOr)]
#[case::mutationoperator_bitwiseshift(MutationOperator::BitwiseShift)]
#[case::mutationoperator_structureappendextend(MutationOperator::StructureAppendExtend)]
#[case::mutationoperator_structuremappinggetsubscript(
    MutationOperator::StructureMappingGetSubscript
)]
#[case::mutationoperator_structuresortreverse(MutationOperator::StructureSortReverse)]
#[case::mutationoperator_structuresortedreversed(MutationOperator::StructureSortedReversed)]
#[case::mutationoperator_structureindexneighbor(MutationOperator::StructureIndexNeighbor)]
#[case::mutationoperator_structuresliceneighbor(MutationOperator::StructureSliceNeighbor)]
#[case::mutationoperator_exceptiontypepair(MutationOperator::ExceptionTypePair)]
fn default_runtime_selection_included(#[case] operator: MutationOperator) {
    let selected = MutationOperatorSelection::default();

    assert!(selected.contains(operator), "missing {operator:?}");
}

#[rstest::rstest]
#[case::mutationoperator_typenullableremove(MutationOperator::TypeNullableRemove)]
#[case::mutationoperator_typenullableadd(MutationOperator::TypeNullableAdd)]
#[case::mutationoperator_typelistsequence(MutationOperator::TypeListSequence)]
#[case::mutationoperator_typesetabstractset(MutationOperator::TypeSetAbstractSet)]
#[case::mutationoperator_typemapping(MutationOperator::TypeMapping)]
#[case::mutationoperator_typeiterableiterator(MutationOperator::TypeIterableIterator)]
#[case::mutationoperator_typesequenceiterable(MutationOperator::TypeSequenceIterable)]
fn default_runtime_selection_type_excluded(#[case] type_operator: MutationOperator) {
    let selected = MutationOperatorSelection::default();

    assert!(
        !selected.contains(type_operator),
        "included {type_operator:?}"
    );
}

#[rstest::rstest]
#[case::mutationoperator_exceptionbaretoexception(MutationOperator::ExceptionBareToException)]
#[case::mutationoperator_exceptionexceptiontobare(MutationOperator::ExceptionExceptionToBare)]
#[case::mutationoperator_exceptionbaseboundary(MutationOperator::ExceptionBaseBoundary)]
#[case::mutationoperator_exceptiontupleaddpair(MutationOperator::ExceptionTupleAddPair)]
#[case::mutationoperator_exceptiontupleremovemember(MutationOperator::ExceptionTupleRemoveMember)]
fn default_runtime_selection_risky_excluded(#[case] risky_operator: MutationOperator) {
    let selected = MutationOperatorSelection::default();

    assert!(
        !selected.contains(risky_operator),
        "included {risky_operator:?}"
    );
}

#[rstest::rstest]
#[case::collection_any_all("collection_any_all")]
#[case::collection_list_tuple("collection_list_tuple")]
#[case::collection_set_frozenset("collection_set_frozenset")]
#[case::collection_append_insert("collection_append_insert")]
#[case::collection_min_max("collection_min_max")]
#[case::collection_set_add_discard("collection_set_add_discard")]
#[case::collection_set_remove_discard("collection_set_remove_discard")]
#[case::collection_string_starts_ends("collection_string_starts_ends")]
#[case::collection_string_split_rsplit("collection_string_split_rsplit")]
#[case::bitwise_and_or("bitwise_and_or")]
#[case::bitwise_shift("bitwise_shift")]
#[case::structure_append_extend("structure_append_extend")]
#[case::structure_mapping_get_subscript("structure_mapping_get_subscript")]
#[case::structure_sort_reverse("structure_sort_reverse")]
#[case::structure_sorted_reversed("structure_sorted_reversed")]
#[case::structure_index_neighbor("structure_index_neighbor")]
#[case::structure_slice_neighbor("structure_slice_neighbor")]
#[case::exception_type_pair("exception_type_pair")]
#[case::exception_bare_to_exception("exception_bare_to_exception")]
#[case::exception_exception_to_bare("exception_exception_to_bare")]
#[case::exception_base_boundary("exception_base_boundary")]
#[case::exception_tuple_add_pair("exception_tuple_add_pair")]
#[case::exception_tuple_remove_member("exception_tuple_remove_member")]
#[case::collection_ops("collection_ops")]
#[case::structure_ops("structure_ops")]
#[case::bitwise_ops("bitwise_ops")]
#[case::exception_ops("exception_ops")]
#[case::exception_risky("exception_risky")]
fn collection_structure_and_bitwise_operator_ids_are_valid_names(#[case] name: &str) {
    let valid_names = MutationOperatorSelection::valid_names();

    assert!(valid_names.contains(&name), "missing {name}");
}

#[test]
fn valid_operator_names_are_sorted() {
    let names = MutationOperatorSelection::valid_names();

    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn collection_structure_and_bitwise_selector_families_expand_exactly() {
    assert_eq!(
        MutationOperatorSelection::parse_selector("collection_ops").unwrap(),
        vec![
            MutationOperator::CollectionAnyAll,
            MutationOperator::CollectionListTuple,
            MutationOperator::CollectionSetFrozenset,
            MutationOperator::CollectionAppendInsert,
            MutationOperator::CollectionMinMax,
            MutationOperator::CollectionSetAddDiscard,
            MutationOperator::CollectionSetRemoveDiscard,
            MutationOperator::CollectionStringStartsEnds,
            MutationOperator::CollectionStringSplitRsplit,
        ]
    );
    assert_eq!(
        MutationOperatorSelection::parse_selector("structure_ops").unwrap(),
        vec![
            MutationOperator::StructureAppendExtend,
            MutationOperator::StructureMappingGetSubscript,
            MutationOperator::StructureSortReverse,
            MutationOperator::StructureSortedReversed,
            MutationOperator::StructureIndexNeighbor,
            MutationOperator::StructureSliceNeighbor,
            MutationOperator::StructureSliceBoundOmit,
            MutationOperator::StructureSliceBoundMove,
        ]
    );
    assert_eq!(
        MutationOperatorSelection::parse_selector("bitwise_ops").unwrap(),
        vec![
            MutationOperator::BitwiseAndOr,
            MutationOperator::BitwiseShift,
            MutationOperator::BitwiseXor,
            MutationOperator::BitwiseInvert,
            MutationOperator::AugmentedBitwiseAndOr,
            MutationOperator::AugmentedBitwiseXor,
            MutationOperator::AugmentedBitwiseShift,
        ]
    );
}

#[test]
fn exception_selector_families_expand_safe_and_risky_operators() {
    assert_eq!(
        MutationOperatorSelection::parse_selector("exception_ops").unwrap(),
        vec![MutationOperator::ExceptionTypePair]
    );
    assert_eq!(
        MutationOperatorSelection::parse_selector("exception_risky").unwrap(),
        vec![
            MutationOperator::ExceptionBareToException,
            MutationOperator::ExceptionExceptionToBare,
            MutationOperator::ExceptionBaseBoundary,
            MutationOperator::ExceptionTupleAddPair,
            MutationOperator::ExceptionTupleRemoveMember,
        ]
    );
}

#[test]
fn excluding_collection_family_preserves_other_runtime_families() {
    let raw = RawRunConfig {
        root: ".".into(),
        files: vec!["src/lib.py".into()],
        exclude_operators: vec!["collection_ops".to_owned()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    };
    let excluded = MutationOperatorSelection::parse_selector("collection_ops").unwrap();
    let retained = MutationOperatorSelection::parse_selector("structure_ops")
        .unwrap()
        .into_iter()
        .chain(MutationOperatorSelection::parse_selector("bitwise_ops").unwrap())
        .collect::<Vec<_>>();

    let config = RunConfig::try_from(raw)
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();

    assert!(
        excluded
            .into_iter()
            .all(|operator| !config.operators.contains(operator))
    );
    assert!(
        retained
            .into_iter()
            .all(|operator| config.operators.contains(operator))
    );
}

#[test]
fn excluding_structure_family_preserves_collection_operators() {
    let mut raw = RawRunConfig {
        root: ".".into(),
        files: vec!["src/lib.py".into()],
        exclude_operators: vec!["collection_ops".to_owned()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    };
    raw.exclude_operators = vec!["structure_ops".to_owned()];
    let config = RunConfig::try_from(raw)
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();
    assert!(
        config
            .operators
            .contains(MutationOperator::CollectionAnyAll)
    );
    assert!(
        !config
            .operators
            .contains(MutationOperator::StructureAppendExtend)
    );
}

#[test]
fn raw_operator_includes_and_excludes_cannot_normalize_to_empty() {
    let mut raw = raw_config();
    raw.exclude_operators = vec!["compare_eq_ne".to_owned()];

    let error = RunConfig::try_from(raw).unwrap_err();

    assert_eq!(error, ConfigError::EmptyMutationOperatorSelection);
    assert!(error.to_string().contains("--operators"));
    assert!(error.to_string().contains("--exclude-operators"));
}

#[test]
fn normalized_run_config_rejects_an_empty_operator_selection() {
    let mut config = RunConfig::try_from(raw_config())
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap()
        .into_data();
    config.operators.exclude(MutationOperator::CompareEqNe);

    assert_eq!(
        RunConfig::try_from(config),
        Err(ConfigError::EmptyMutationOperatorSelection)
    );
}

#[test]
fn persisted_plan_config_rejects_an_empty_operator_selection() {
    let plan = RunConfig::try_from(raw_config())
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap()
        .into_plan_config(
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
        );
    let mut value = serde_json::to_value(plan).unwrap();
    value["operators"] = serde_json::json!([]);
    let plan: PlanConfig = serde_json::from_value(value).unwrap();

    assert_eq!(
        plan.validate(),
        Err(ConfigError::EmptyMutationOperatorSelection)
    );
}

fn raw_config() -> RawRunConfig {
    RawRunConfig {
        root: ".".into(),
        files: vec!["src/lib.py".into()],
        operators: vec!["compare_eq_ne".to_owned()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    }
}

#[rstest::rstest]
#[case::exception_ops("exception_ops")]
#[case::exception_risky("exception_risky")]
fn exception_hierarchy_is_explicit_only(#[case] family: &str) {
    let operators = MutationOperatorSelection::parse_selector("exception_hierarchy").unwrap();

    assert_eq!(operators.len(), 1);
    assert_eq!(operators[0].as_str(), "exception_hierarchy");
    assert!(!MutationOperatorSelection::default().contains(operators[0]));

    assert!(
        !MutationOperatorSelection::parse_selector(family)
            .unwrap()
            .contains(&operators[0])
    );
}

// Fixed current policy, independent of catalog and production default builders.
const CURRENT_DEFAULT_NAMES: [&str; 85] = [
    "compare_eq_ne",
    "compare_order",
    "membership",
    "identity",
    "boolean_and_or",
    "binary_add_sub",
    "augmented_add_sub",
    "binary_mul_div",
    "augmented_mul_div",
    "binary_floor_mod",
    "augmented_floor_mod",
    "unary_sign",
    "remove_not",
    "boolean_literal",
    "break_continue",
    "collection_any_all",
    "collection_list_tuple",
    "collection_set_frozenset",
    "collection_append_insert",
    "collection_min_max",
    "collection_set_add_discard",
    "collection_set_remove_discard",
    "collection_string_starts_ends",
    "collection_string_split_rsplit",
    "bitwise_and_or",
    "bitwise_shift",
    "binary_power",
    "binary_matmul",
    "augmented_power",
    "augmented_matmul",
    "bitwise_xor",
    "bitwise_invert",
    "augmented_bitwise_and_or",
    "augmented_bitwise_xor",
    "augmented_bitwise_shift",
    "operator_function",
    "structure_append_extend",
    "structure_mapping_get_subscript",
    "structure_sort_reverse",
    "structure_sorted_reversed",
    "structure_index_neighbor",
    "structure_slice_neighbor",
    "exception_type_pair",
    "statement_delete",
    "integer_literal_neighbor",
    "augmented_to_assignment",
    "return_tuple_swap",
    "string_literal_empty",
    "while_condition_false",
    "conversion_call_remove",
    "method_call_remove",
    "function_body_return_constant",
    "string_segment_empty",
    "stdlib_conversion_call_remove",
    "loop_break_insert",
    "iterator_stop_insert",
    "async_iterator_stop_insert",
    "list_comprehension_to_generator",
    "generator_return_insert",
    "generator_skip_first",
    "generator_empty",
    "none_truthiness",
    "mutable_default_share",
    "nonlocal_remove",
    "yield_from_to_yield",
    "mapping_setdefault_to_get",
    "bisect_boundary_swap",
    "deepcopy_to_copy",
    "deque_popleft_to_pop",
    "gather_return_exceptions",
    "itertools_take_drop_swap",
    "fstring_conversion_swap",
    "binary_div_floor",
    "structure_slice_bound_omit",
    "structure_slice_bound_move",
    "continue_remove",
    "comparison_not_implemented",
    "exception_cause_remove",
    "dataclass_factory_share",
    "sort_key_swap",
    "lambda_capture_late",
    "encoding_errors_ignore",
    "base_order_shuffle",
    "override_method_delete",
    "super_call_move",
];

#[test]
fn current_default_selection_has_exact_names_in_saved_order() {
    let selection = MutationOperatorSelection::default();

    assert_eq!(selection.names(), CURRENT_DEFAULT_NAMES);
    assert_eq!(
        serde_json::to_value(selection).unwrap(),
        serde_json::json!(CURRENT_DEFAULT_NAMES.as_slice())
    );
}

#[test]
fn omitted_raw_operators_use_exact_current_defaults() {
    let mut raw = raw_config();
    raw.operators.clear();

    let config = RunConfig::try_from(raw).unwrap();

    assert_eq!(config.operators.names(), CURRENT_DEFAULT_NAMES);
}

#[test]
fn default_exclusions_remove_only_selected_current_operators() {
    let mut raw = raw_config();
    raw.operators.clear();
    raw.exclude_operators = vec!["statement_delete".into(), "method_call_remove".into()];

    let config = RunConfig::try_from(raw).unwrap();

    let expected: Vec<_> = CURRENT_DEFAULT_NAMES
        .into_iter()
        .filter(|name| !matches!(*name, "statement_delete" | "method_call_remove"))
        .collect();

    assert_eq!(config.operators.names(), expected);
}

#[test]
fn explicit_selection_exclusion_does_not_expand_to_defaults() {
    let mut raw = raw_config();
    raw.operators = vec!["string_literal_empty".into(), "compare_eq_ne".into()];
    raw.exclude_operators = vec!["string_literal_empty".into()];

    let config = RunConfig::try_from(raw).unwrap();

    assert_eq!(config.operators.names(), vec!["compare_eq_ne"]);
}

#[test]
fn stored_explicit_selection_roundtrips_without_default_expansion() {
    let mut raw = raw_config();
    raw.operators = vec![
        "method_call_remove".into(),
        "binary_add_sub".into(),
        "type_list_sequence".into(),
    ];
    let plan = RunConfig::try_from(raw)
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap()
        .into_plan_config(
            hoimin_core::DiscoveryConfig::new(std::time::Duration::from_secs(30)).unwrap(),
        );
    let json = serde_json::to_vec(&plan).unwrap();

    let restored: PlanConfig = serde_json::from_slice(&json).unwrap();

    restored.validate().unwrap();

    assert_eq!(
        restored.operators.names(),
        ["binary_add_sub", "type_list_sequence", "method_call_remove"]
    );
    assert_eq!(serde_json::to_vec(&restored).unwrap(), json);
}

#[rstest::rstest]
#[case::none_truthiness("none_truthiness")]
#[case::mutable_default_share("mutable_default_share")]
#[case::nonlocal_remove("nonlocal_remove")]
#[case::yield_from_to_yield("yield_from_to_yield")]
#[case::mapping_setdefault_to_get("mapping_setdefault_to_get")]
#[case::bisect_boundary_swap("bisect_boundary_swap")]
#[case::deepcopy_to_copy("deepcopy_to_copy")]
#[case::deque_popleft_to_pop("deque_popleft_to_pop")]
#[case::gather_return_exceptions("gather_return_exceptions")]
#[case::itertools_take_drop_swap("itertools_take_drop_swap")]
#[case::fstring_conversion_swap("fstring_conversion_swap")]
fn effective_python_mutations_are_default_and_individually_selectable(#[case] name: &str) {
    let defaults = MutationOperatorSelection::default();

    assert!(defaults.names().iter().any(|item| item == name), "{name}");
    assert_eq!(
        MutationOperatorSelection::parse_selector(name)
            .unwrap()
            .len(),
        1
    );
}
