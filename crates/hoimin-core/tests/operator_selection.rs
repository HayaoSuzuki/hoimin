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
            MutationOperator::BitwiseShift
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
    let config = RunConfig::try_from(raw.clone()).unwrap();

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
    let config = RunConfig::try_from(raw).unwrap();
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
    let mut config = RunConfig::try_from(raw_config()).unwrap();
    config.operators.exclude(MutationOperator::CompareEqNe);

    assert_eq!(
        config.validate(),
        Err(ConfigError::EmptyMutationOperatorSelection)
    );
}

#[test]
fn persisted_plan_config_rejects_an_empty_operator_selection() {
    let plan = RunConfig::try_from(raw_config())
        .unwrap()
        .into_plan_config();
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
