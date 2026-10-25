use hoimin_core::{
    CommandArg, ConfigError, MutationOperator, MutationOperatorSelection, PlanConfig, RawRunConfig,
    RunConfig,
};

#[test]
fn unknown_operator_error_lists_valid_operators_and_selectors() {
    let error = MutationOperatorSelection::parse_selector("not_a_real_operator").unwrap_err();
    let message = error.to_string();

    for valid in ["compare_eq_ne", "type_nullable", "type_dict_mapping"] {
        assert!(message.contains(valid), "missing {valid} in {message}");
    }
}

#[test]
fn augmented_arithmetic_operator_ids_are_default_and_valid() {
    let selected = MutationOperatorSelection::default();
    let selected_names = selected.names();
    let valid_names = MutationOperatorSelection::valid_names();

    for name in ["augmented_mul_div", "augmented_floor_mod"] {
        assert!(selected_names.iter().any(|selected| selected == name));
        assert!(valid_names.contains(&name));
        assert_eq!(
            MutationOperatorSelection::parse_selector(name)
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn native_python_operator_ids_are_default_valid_and_selectable() {
    let selected = MutationOperatorSelection::default();
    let selected_names = selected.names();
    let valid_names = MutationOperatorSelection::valid_names();

    for name in [
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
    ] {
        assert!(selected_names.iter().any(|selected| selected == name));
        assert!(valid_names.contains(&name));
        assert_eq!(
            MutationOperatorSelection::parse_selector(name)
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn default_runtime_selection_contains_collection_structure_and_bitwise_operators() {
    let selected = MutationOperatorSelection::default();

    for operator in [
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
        MutationOperator::StructureAppendExtend,
        MutationOperator::StructureMappingGetSubscript,
        MutationOperator::StructureSortReverse,
        MutationOperator::StructureSortedReversed,
        MutationOperator::StructureIndexNeighbor,
        MutationOperator::StructureSliceNeighbor,
        MutationOperator::ExceptionTypePair,
    ] {
        assert!(selected.contains(operator), "missing {operator:?}");
    }

    for type_operator in [
        MutationOperator::TypeNullableRemove,
        MutationOperator::TypeNullableAdd,
        MutationOperator::TypeListSequence,
        MutationOperator::TypeSetAbstractSet,
        MutationOperator::TypeMapping,
        MutationOperator::TypeIterableIterator,
        MutationOperator::TypeSequenceIterable,
    ] {
        assert!(
            !selected.contains(type_operator),
            "included {type_operator:?}"
        );
    }
    for risky_operator in [
        MutationOperator::ExceptionBareToException,
        MutationOperator::ExceptionExceptionToBare,
        MutationOperator::ExceptionBaseBoundary,
        MutationOperator::ExceptionTupleAddPair,
        MutationOperator::ExceptionTupleRemoveMember,
    ] {
        assert!(
            !selected.contains(risky_operator),
            "included {risky_operator:?}"
        );
    }
}

#[test]
fn collection_structure_and_bitwise_operator_ids_are_valid_names() {
    let valid_names = MutationOperatorSelection::valid_names();

    for name in [
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
        "structure_append_extend",
        "structure_mapping_get_subscript",
        "structure_sort_reverse",
        "structure_sorted_reversed",
        "structure_index_neighbor",
        "structure_slice_neighbor",
        "exception_type_pair",
        "exception_bare_to_exception",
        "exception_exception_to_bare",
        "exception_base_boundary",
        "exception_tuple_add_pair",
        "exception_tuple_remove_member",
        "collection_ops",
        "structure_ops",
        "bitwise_ops",
        "exception_ops",
        "exception_risky",
    ] {
        assert!(valid_names.contains(&name), "missing {name}");
    }

    assert!(valid_names.windows(2).all(|names| names[0] < names[1]));
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
    let mut raw = RawRunConfig {
        root: ".".into(),
        files: vec!["src/lib.py".into()],
        exclude_operators: vec!["collection_ops".to_owned()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    };
    let config = RunConfig::try_from(raw.clone())
        .unwrap()
        .with_environment(hoimin_core::EnvironmentSnapshot::Untracked)
        .unwrap();

    for operator in MutationOperatorSelection::parse_selector("collection_ops").unwrap() {
        assert!(
            !config.operators.contains(operator),
            "included {operator:?}"
        );
    }
    for operator in MutationOperatorSelection::parse_selector("structure_ops")
        .unwrap()
        .into_iter()
        .chain(MutationOperatorSelection::parse_selector("bitwise_ops").unwrap())
    {
        assert!(config.operators.contains(operator), "missing {operator:?}");
    }

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

#[test]
fn exception_hierarchy_is_explicit_only() {
    let operators = MutationOperatorSelection::parse_selector("exception_hierarchy").unwrap();
    assert_eq!(operators.len(), 1);
    assert_eq!(operators[0].as_str(), "exception_hierarchy");
    assert!(!MutationOperatorSelection::default().contains(operators[0]));
    for family in ["exception_ops", "exception_risky"] {
        assert!(
            !MutationOperatorSelection::parse_selector(family)
                .unwrap()
                .contains(&operators[0])
        );
    }
}

// Fixed current policy, independent of catalog and production default builders.
const CURRENT_DEFAULT_NAMES: [&str; 72] = [
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

#[test]
fn effective_python_mutations_are_default_and_individually_selectable() {
    let defaults = MutationOperatorSelection::default();
    for name in [
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
    ] {
        assert!(defaults.names().iter().any(|item| item == name), "{name}");
        assert_eq!(
            MutationOperatorSelection::parse_selector(name)
                .unwrap()
                .len(),
            1
        );
    }
}
