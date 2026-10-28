use hoimin_core::{MutationOperator, MutationOperatorSelection};

// Fixed wire-name and declaration-order contract at v0.5.0. This fixture is
// deliberately independent of the production declaration that generates names.
const OPERATORS: &[(MutationOperator, &str)] = &[
    (MutationOperator::CompareEqNe, "compare_eq_ne"),
    (MutationOperator::CompareOrder, "compare_order"),
    (MutationOperator::Membership, "membership"),
    (MutationOperator::Identity, "identity"),
    (MutationOperator::BooleanAndOr, "boolean_and_or"),
    (MutationOperator::BinaryAddSub, "binary_add_sub"),
    (MutationOperator::AugmentedAddSub, "augmented_add_sub"),
    (MutationOperator::BinaryMulDiv, "binary_mul_div"),
    (MutationOperator::AugmentedMulDiv, "augmented_mul_div"),
    (MutationOperator::BinaryFloorMod, "binary_floor_mod"),
    (MutationOperator::AugmentedFloorMod, "augmented_floor_mod"),
    (MutationOperator::UnarySign, "unary_sign"),
    (MutationOperator::RemoveNot, "remove_not"),
    (MutationOperator::BooleanLiteral, "boolean_literal"),
    (MutationOperator::BreakContinue, "break_continue"),
    (MutationOperator::CollectionAnyAll, "collection_any_all"),
    (
        MutationOperator::CollectionListTuple,
        "collection_list_tuple",
    ),
    (
        MutationOperator::CollectionSetFrozenset,
        "collection_set_frozenset",
    ),
    (
        MutationOperator::CollectionAppendInsert,
        "collection_append_insert",
    ),
    (MutationOperator::CollectionMinMax, "collection_min_max"),
    (
        MutationOperator::CollectionSetAddDiscard,
        "collection_set_add_discard",
    ),
    (
        MutationOperator::CollectionSetRemoveDiscard,
        "collection_set_remove_discard",
    ),
    (
        MutationOperator::CollectionStringStartsEnds,
        "collection_string_starts_ends",
    ),
    (
        MutationOperator::CollectionStringSplitRsplit,
        "collection_string_split_rsplit",
    ),
    (MutationOperator::BitwiseAndOr, "bitwise_and_or"),
    (MutationOperator::BitwiseShift, "bitwise_shift"),
    (MutationOperator::BinaryPower, "binary_power"),
    (MutationOperator::BinaryMatmul, "binary_matmul"),
    (MutationOperator::AugmentedPower, "augmented_power"),
    (MutationOperator::AugmentedMatmul, "augmented_matmul"),
    (MutationOperator::BitwiseXor, "bitwise_xor"),
    (MutationOperator::BitwiseInvert, "bitwise_invert"),
    (
        MutationOperator::AugmentedBitwiseAndOr,
        "augmented_bitwise_and_or",
    ),
    (
        MutationOperator::AugmentedBitwiseXor,
        "augmented_bitwise_xor",
    ),
    (
        MutationOperator::AugmentedBitwiseShift,
        "augmented_bitwise_shift",
    ),
    (MutationOperator::OperatorFunction, "operator_function"),
    (
        MutationOperator::StructureAppendExtend,
        "structure_append_extend",
    ),
    (
        MutationOperator::StructureMappingGetSubscript,
        "structure_mapping_get_subscript",
    ),
    (
        MutationOperator::StructureSortReverse,
        "structure_sort_reverse",
    ),
    (
        MutationOperator::StructureSortedReversed,
        "structure_sorted_reversed",
    ),
    (
        MutationOperator::StructureIndexNeighbor,
        "structure_index_neighbor",
    ),
    (
        MutationOperator::StructureSliceNeighbor,
        "structure_slice_neighbor",
    ),
    (MutationOperator::ExceptionTypePair, "exception_type_pair"),
    (MutationOperator::ExceptionHierarchy, "exception_hierarchy"),
    (
        MutationOperator::ExceptionBareToException,
        "exception_bare_to_exception",
    ),
    (
        MutationOperator::ExceptionExceptionToBare,
        "exception_exception_to_bare",
    ),
    (
        MutationOperator::ExceptionBaseBoundary,
        "exception_base_boundary",
    ),
    (
        MutationOperator::ExceptionTupleAddPair,
        "exception_tuple_add_pair",
    ),
    (
        MutationOperator::ExceptionTupleRemoveMember,
        "exception_tuple_remove_member",
    ),
    (MutationOperator::TypeNullableRemove, "type_nullable_remove"),
    (MutationOperator::TypeNullableAdd, "type_nullable_add"),
    (MutationOperator::TypeListSequence, "type_list_sequence"),
    (
        MutationOperator::TypeSetAbstractSet,
        "type_set_abstract_set",
    ),
    (MutationOperator::TypeMapping, "type_dict_mapping"),
    (
        MutationOperator::TypeIterableIterator,
        "type_iterable_iterator",
    ),
    (
        MutationOperator::TypeSequenceIterable,
        "type_sequence_iterable",
    ),
    (MutationOperator::StatementDelete, "statement_delete"),
    (
        MutationOperator::IntegerLiteralNeighbor,
        "integer_literal_neighbor",
    ),
    (MutationOperator::ConditionConstant, "condition_constant"),
    (MutationOperator::FunctionBodyErase, "function_body_erase"),
    (MutationOperator::EnumMemberReplace, "enum_member_replace"),
    (
        MutationOperator::AugmentedToAssignment,
        "augmented_to_assignment",
    ),
    (MutationOperator::ReturnTupleSwap, "return_tuple_swap"),
    (MutationOperator::StringLiteralEmpty, "string_literal_empty"),
    (
        MutationOperator::WhileConditionFalse,
        "while_condition_false",
    ),
    (
        MutationOperator::ConditionClauseDelete,
        "condition_clause_delete",
    ),
    (
        MutationOperator::ContainerElementDelete,
        "container_element_delete",
    ),
    (
        MutationOperator::ConversionCallRemove,
        "conversion_call_remove",
    ),
    (
        MutationOperator::OptionalKeywordDelete,
        "optional_keyword_delete",
    ),
    (MutationOperator::MethodCallRemove, "method_call_remove"),
    (
        MutationOperator::FunctionBodyReturnConstant,
        "function_body_return_constant",
    ),
    (MutationOperator::StringSegmentEmpty, "string_segment_empty"),
    (
        MutationOperator::StdlibConversionCallRemove,
        "stdlib_conversion_call_remove",
    ),
    (MutationOperator::LoopBreakInsert, "loop_break_insert"),
    (MutationOperator::IteratorStopInsert, "iterator_stop_insert"),
    (
        MutationOperator::AsyncIteratorStopInsert,
        "async_iterator_stop_insert",
    ),
    (
        MutationOperator::ListComprehensionToGenerator,
        "list_comprehension_to_generator",
    ),
    (
        MutationOperator::GeneratorReturnInsert,
        "generator_return_insert",
    ),
    (MutationOperator::GeneratorSkipFirst, "generator_skip_first"),
    (MutationOperator::GeneratorEmpty, "generator_empty"),
    (MutationOperator::NoneTruthiness, "none_truthiness"),
    (
        MutationOperator::MutableDefaultShare,
        "mutable_default_share",
    ),
    (MutationOperator::NonlocalRemove, "nonlocal_remove"),
    (MutationOperator::YieldFromToYield, "yield_from_to_yield"),
    (
        MutationOperator::MappingSetdefaultToGet,
        "mapping_setdefault_to_get",
    ),
    (MutationOperator::BisectBoundarySwap, "bisect_boundary_swap"),
    (MutationOperator::DeepcopyToCopy, "deepcopy_to_copy"),
    (MutationOperator::DequePopleftToPop, "deque_popleft_to_pop"),
    (
        MutationOperator::GatherReturnExceptions,
        "gather_return_exceptions",
    ),
    (
        MutationOperator::ItertoolsTakeDropSwap,
        "itertools_take_drop_swap",
    ),
    (
        MutationOperator::FStringConversionSwap,
        "fstring_conversion_swap",
    ),
    (MutationOperator::BinaryDivFloor, "binary_div_floor"),
    (
        MutationOperator::StructureSliceBoundOmit,
        "structure_slice_bound_omit",
    ),
    (
        MutationOperator::StructureSliceBoundMove,
        "structure_slice_bound_move",
    ),
    (MutationOperator::ContinueRemove, "continue_remove"),
    (
        MutationOperator::ComparisonNotImplemented,
        "comparison_not_implemented",
    ),
    (
        MutationOperator::ExceptionCauseRemove,
        "exception_cause_remove",
    ),
];

const FAMILIES: [&str; 8] = [
    "type_nullable",
    "type_collections",
    "type_iterables",
    "collection_ops",
    "structure_ops",
    "bitwise_ops",
    "exception_ops",
    "exception_risky",
];

#[test]
fn canonical_names_preserve_wire_encoding_and_exact_lookup() {
    for &(operator, name) in OPERATORS {
        assert_eq!(operator.as_str(), name);
        assert_eq!(MutationOperator::from_name(name), Some(operator));
        assert_eq!(
            serde_json::to_value(operator).unwrap(),
            serde_json::json!(name)
        );
        assert_eq!(
            serde_json::from_value::<MutationOperator>(serde_json::json!(name)).unwrap(),
            operator
        );
        assert_eq!(
            MutationOperatorSelection::parse_selector(name).unwrap(),
            vec![operator]
        );
    }
}

#[test]
fn operator_selection_retains_declaration_order_in_saved_inputs() {
    let names: Vec<_> = OPERATORS.iter().map(|(_, name)| *name).collect();
    let mut reversed = names.clone();
    reversed.reverse();
    let selection: MutationOperatorSelection =
        serde_json::from_value(serde_json::json!(reversed)).unwrap();

    assert_eq!(selection.names(), names);
    assert_eq!(
        serde_json::to_value(selection).unwrap(),
        serde_json::json!(names)
    );
    assert!(OPERATORS.windows(2).all(|pair| pair[0].0 < pair[1].0));
}

#[test]
fn advertised_names_are_exactly_the_canonical_operators_and_families() {
    let mut expected: Vec<_> = OPERATORS
        .iter()
        .map(|(_, name)| *name)
        .chain(FAMILIES)
        .collect();
    expected.sort_unstable();

    assert_eq!(MutationOperatorSelection::valid_names(), expected);
}

#[test]
fn legacy_mapping_alias_is_rejected_in_saved_inputs_and_selectors() {
    assert!(serde_json::from_value::<MutationOperator>(serde_json::json!("type_mapping")).is_err());
    assert_eq!(
        serde_json::to_value(MutationOperator::TypeMapping).unwrap(),
        serde_json::json!("type_dict_mapping")
    );
    assert_eq!(MutationOperator::from_name("type_mapping"), None);
    assert!(MutationOperatorSelection::parse_selector("type_mapping").is_err());
}

#[test]
fn operator_and_family_names_reject_unknown_case_and_whitespace() {
    for name in [
        "",
        "unknown",
        "CompareEqNe",
        "COMPARE_EQ_NE",
        " compare_eq_ne",
        "compare_eq_ne ",
        "TypeNullable",
        "type_nullable ",
    ] {
        assert_eq!(MutationOperator::from_name(name), None, "{name:?}");
        assert!(
            MutationOperatorSelection::parse_selector(name).is_err(),
            "{name:?}"
        );
        assert!(
            serde_json::from_value::<MutationOperator>(serde_json::json!(name)).is_err(),
            "{name:?}"
        );
    }
    for family in FAMILIES {
        assert_eq!(MutationOperator::from_name(family), None);
        assert!(serde_json::from_value::<MutationOperator>(serde_json::json!(family)).is_err());
    }
}

#[test]
fn type_families_expand_in_the_existing_order() {
    for (name, expected) in [
        (
            "type_nullable",
            vec![
                MutationOperator::TypeNullableRemove,
                MutationOperator::TypeNullableAdd,
            ],
        ),
        (
            "type_collections",
            vec![
                MutationOperator::TypeListSequence,
                MutationOperator::TypeSetAbstractSet,
                MutationOperator::TypeMapping,
            ],
        ),
        (
            "type_iterables",
            vec![
                MutationOperator::TypeIterableIterator,
                MutationOperator::TypeSequenceIterable,
            ],
        ),
    ] {
        assert_eq!(
            MutationOperatorSelection::parse_selector(name).unwrap(),
            expected
        );
    }
}
