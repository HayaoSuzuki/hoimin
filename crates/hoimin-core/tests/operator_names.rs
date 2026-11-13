#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
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
    (
        MutationOperator::DataclassFactoryShare,
        "dataclass_factory_share",
    ),
    (MutationOperator::SortKeySwap, "sort_key_swap"),
    (MutationOperator::LambdaCaptureLate, "lambda_capture_late"),
    (
        MutationOperator::EncodingErrorsIgnore,
        "encoding_errors_ignore",
    ),
    (MutationOperator::BaseOrderShuffle, "base_order_shuffle"),
    (
        MutationOperator::OverrideMethodDelete,
        "override_method_delete",
    ),
    (MutationOperator::SuperCallMove, "super_call_move"),
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

#[rstest::rstest]
#[case::mutationoperator_compareeqne_1(0)]
#[case::mutationoperator_compareorder_2(1)]
#[case::mutationoperator_membership_3(2)]
#[case::mutationoperator_identity_4(3)]
#[case::mutationoperator_booleanandor_5(4)]
#[case::mutationoperator_binaryaddsub_6(5)]
#[case::mutationoperator_augmentedaddsub_7(6)]
#[case::mutationoperator_binarymuldiv_8(7)]
#[case::mutationoperator_augmentedmuldiv_9(8)]
#[case::mutationoperator_binaryfloormod_10(9)]
#[case::mutationoperator_augmentedfloormod_11(10)]
#[case::mutationoperator_unarysign_12(11)]
#[case::mutationoperator_removenot_13(12)]
#[case::mutationoperator_booleanliteral_14(13)]
#[case::mutationoperator_breakcontinue_15(14)]
#[case::mutationoperator_collectionanyall_16(15)]
#[case::mutationoperator_collectionlisttuple_17(16)]
#[case::mutationoperator_collectionsetfrozenset_18(17)]
#[case::mutationoperator_collectionappendinsert_19(18)]
#[case::mutationoperator_collectionminmax_20(19)]
#[case::mutationoperator_collectionsetadddiscard_21(20)]
#[case::mutationoperator_collectionsetremovediscard_22(21)]
#[case::mutationoperator_collectionstringstartsends_23(22)]
#[case::mutationoperator_collectionstringsplitrsplit_24(23)]
#[case::mutationoperator_bitwiseandor_25(24)]
#[case::mutationoperator_bitwiseshift_26(25)]
#[case::mutationoperator_binarypower_27(26)]
#[case::mutationoperator_binarymatmul_28(27)]
#[case::mutationoperator_augmentedpower_29(28)]
#[case::mutationoperator_augmentedmatmul_30(29)]
#[case::mutationoperator_bitwisexor_31(30)]
#[case::mutationoperator_bitwiseinvert_32(31)]
#[case::mutationoperator_augmentedbitwiseandor_33(32)]
#[case::mutationoperator_augmentedbitwisexor_34(33)]
#[case::mutationoperator_augmentedbitwiseshift_35(34)]
#[case::mutationoperator_operatorfunction_36(35)]
#[case::mutationoperator_structureappendextend_37(36)]
#[case::mutationoperator_structuremappinggetsubscript_38(37)]
#[case::mutationoperator_structuresortreverse_39(38)]
#[case::mutationoperator_structuresortedreversed_40(39)]
#[case::mutationoperator_structureindexneighbor_41(40)]
#[case::mutationoperator_structuresliceneighbor_42(41)]
#[case::mutationoperator_exceptiontypepair_43(42)]
#[case::mutationoperator_exceptionhierarchy_44(43)]
#[case::mutationoperator_exceptionbaretoexception_45(44)]
#[case::mutationoperator_exceptionexceptiontobare_46(45)]
#[case::mutationoperator_exceptionbaseboundary_47(46)]
#[case::mutationoperator_exceptiontupleaddpair_48(47)]
#[case::mutationoperator_exceptiontupleremovemember_49(48)]
#[case::mutationoperator_typenullableremove_50(49)]
#[case::mutationoperator_typenullableadd_51(50)]
#[case::mutationoperator_typelistsequence_52(51)]
#[case::mutationoperator_typesetabstractset_53(52)]
#[case::mutationoperator_typemapping_54(53)]
#[case::mutationoperator_typeiterableiterator_55(54)]
#[case::mutationoperator_typesequenceiterable_56(55)]
#[case::mutationoperator_statementdelete_57(56)]
#[case::mutationoperator_integerliteralneighbor_58(57)]
#[case::mutationoperator_conditionconstant_59(58)]
#[case::mutationoperator_functionbodyerase_60(59)]
#[case::mutationoperator_enummemberreplace_61(60)]
#[case::mutationoperator_augmentedtoassignment_62(61)]
#[case::mutationoperator_returntupleswap_63(62)]
#[case::mutationoperator_stringliteralempty_64(63)]
#[case::mutationoperator_whileconditionfalse_65(64)]
#[case::mutationoperator_conditionclausedelete_66(65)]
#[case::mutationoperator_containerelementdelete_67(66)]
#[case::mutationoperator_conversioncallremove_68(67)]
#[case::mutationoperator_optionalkeyworddelete_69(68)]
#[case::mutationoperator_methodcallremove_70(69)]
#[case::mutationoperator_functionbodyreturnconstant_71(70)]
#[case::mutationoperator_stringsegmentempty_72(71)]
#[case::mutationoperator_stdlibconversioncallremove_73(72)]
#[case::mutationoperator_loopbreakinsert_74(73)]
#[case::mutationoperator_iteratorstopinsert_75(74)]
#[case::mutationoperator_asynciteratorstopinsert_76(75)]
#[case::mutationoperator_listcomprehensiontogenerator_77(76)]
#[case::mutationoperator_generatorreturninsert_78(77)]
#[case::mutationoperator_generatorskipfirst_79(78)]
#[case::mutationoperator_generatorempty_80(79)]
#[case::mutationoperator_nonetruthiness_81(80)]
#[case::mutationoperator_mutabledefaultshare_82(81)]
#[case::mutationoperator_nonlocalremove_83(82)]
#[case::mutationoperator_yieldfromtoyield_84(83)]
#[case::mutationoperator_mappingsetdefaulttoget_85(84)]
#[case::mutationoperator_bisectboundaryswap_86(85)]
#[case::mutationoperator_deepcopytocopy_87(86)]
#[case::mutationoperator_dequepoplefttopop_88(87)]
#[case::mutationoperator_gatherreturnexceptions_89(88)]
#[case::mutationoperator_itertoolstakedropswap_90(89)]
#[case::mutationoperator_fstringconversionswap_91(90)]
#[case::mutationoperator_binarydivfloor_92(91)]
#[case::mutationoperator_structuresliceboundomit_93(92)]
#[case::mutationoperator_structuresliceboundmove_94(93)]
#[case::mutationoperator_continueremove_95(94)]
#[case::mutationoperator_comparisonnotimplemented_96(95)]
#[case::mutationoperator_exceptioncauseremove_97(96)]
#[case::mutationoperator_dataclassfactoryshare_98(97)]
#[case::mutationoperator_sortkeyswap_99(98)]
#[case::mutationoperator_lambdacapturelate_100(99)]
#[case::mutationoperator_encodingerrorsignore_101(100)]
#[case::mutationoperator_baseordershuffle_102(101)]
#[case::mutationoperator_overridemethoddelete_103(102)]
#[case::mutationoperator_supercallmove_104(103)]
fn canonical_names_preserve_wire_encoding_and_exact_lookup(#[case] case_index: usize) {
    let &(operator, name) = &OPERATORS[case_index];

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

#[rstest::rstest]
#[case::value("")]
#[case::unknown("unknown")]
#[case::compareeqne("CompareEqNe")]
#[case::compare_eq_ne("COMPARE_EQ_NE")]
#[case::compare_eq_ne_5(" compare_eq_ne")]
#[case::compare_eq_ne_6("compare_eq_ne ")]
#[case::typenullable("TypeNullable")]
#[case::type_nullable("type_nullable ")]
fn operator_and_family_names_reject_unknown_case_and_whitespace_table_1(#[case] name: &str) {
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

#[rstest::rstest]
#[case::type_nullable_1(0)]
#[case::type_collections_2(1)]
#[case::type_iterables_3(2)]
#[case::collection_ops_4(3)]
#[case::structure_ops_5(4)]
#[case::bitwise_ops_6(5)]
#[case::exception_ops_7(6)]
#[case::exception_risky_8(7)]
fn operator_and_family_names_reject_unknown_case_and_whitespace(#[case] case_index: usize) {
    let family = FAMILIES[case_index];

    assert_eq!(MutationOperator::from_name(family), None);
    assert!(serde_json::from_value::<MutationOperator>(serde_json::json!(family)).is_err());
}

#[rstest::rstest]
#[case::type_nullable("type_nullable", &[
                MutationOperator::TypeNullableRemove,
                MutationOperator::TypeNullableAdd,
            ])]
#[case::type_collections("type_collections", &[
                MutationOperator::TypeListSequence,
                MutationOperator::TypeSetAbstractSet,
                MutationOperator::TypeMapping,
            ])]
#[case::type_iterables("type_iterables", &[
                MutationOperator::TypeIterableIterator,
                MutationOperator::TypeSequenceIterable,
            ])]
fn type_families_expand_in_the_existing_order(
    #[case] name: &str,
    #[case] expected: &[MutationOperator],
) {
    assert_eq!(
        MutationOperatorSelection::parse_selector(name).unwrap(),
        expected
    );
}
