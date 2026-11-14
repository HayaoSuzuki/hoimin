use std::collections::BTreeSet;

use camino::Utf8PathBuf;
use hoimin_core::{
    ByteSpan, LineRange, LineSelection, MutationCandidate, MutationOperator,
    MutationOperatorSelection, Selection, TargetSlice,
};

use super::ranking::{
    RankedPlanCandidate, RankingReason, RankingReasonCode, rank_candidates, validate_ranking,
    validate_ranking_against,
};

fn candidate(
    id: &str,
    path: &str,
    line: u32,
    column: u32,
    operator: &str,
    symbol: Option<&str>,
) -> MutationCandidate {
    MutationCandidate {
        id: id.to_owned(),
        sequence: 1,
        path: Utf8PathBuf::from(path),
        span: ByteSpan {
            start: u64::from(column),
            length: 1,
        },
        original: "x".to_owned(),
        replacement: "y".to_owned(),
        operator: operator.to_owned(),
        line,
        column,
        symbol: symbol.map(str::to_owned),
        file_hash: "0".repeat(64),
    }
}

fn reason(code: RankingReasonCode, score: u32) -> RankingReason {
    RankingReason { code, score }
}

#[test]
fn ranking_accumulates_selector_and_operator_reasons() {
    let selection = Selection {
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("src/calc.py"),
            range: LineRange { start: 5, end: 5 },
        }],
        changed: Some(hoimin_core::ChangedSelection::default()),
        ..Selection::default()
    };
    let targets = vec![TargetSlice {
        path: Utf8PathBuf::from("src/calc.py"),
        lines: vec![LineRange { start: 5, end: 5 }],
        symbols: vec!["calculate".to_owned()],
    }];

    let ranked = rank_candidates(
        &selection,
        &targets,
        vec![candidate(
            "candidate",
            "src/calc.py",
            5,
            3,
            "compare_eq_ne",
            Some("calculate"),
        )],
    );

    assert_eq!(
        ranked[0].ranking_reasons,
        vec![
            reason(RankingReasonCode::ExplicitLine, 300),
            reason(RankingReasonCode::ExplicitSymbol, 250),
            reason(RankingReasonCode::ChangedLine, 200),
            reason(RankingReasonCode::HighValueControl, 100),
        ]
    );
    assert_eq!(ranked[0].score, 850);
    assert_eq!(ranked[0].rank, 1);
}

#[test]
fn ranking_uses_all_resolved_target_symbols_without_changing_output_order() {
    let targets = vec![
        TargetSlice {
            path: Utf8PathBuf::from("src/a.py"),
            lines: Vec::new(),
            symbols: vec!["first".to_owned(), "shared".to_owned()],
        },
        TargetSlice {
            path: Utf8PathBuf::from("src/a.py"),
            lines: Vec::new(),
            symbols: vec!["second".to_owned(), "shared".to_owned()],
        },
        TargetSlice {
            path: Utf8PathBuf::from("src/b.py"),
            lines: Vec::new(),
            symbols: vec!["other".to_owned()],
        },
        TargetSlice {
            path: Utf8PathBuf::from("src/empty.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
    ];

    let ranked = rank_candidates(
        &Selection::default(),
        &targets,
        vec![
            candidate("first", "src/a.py", 1, 0, "binary_add_sub", Some("first")),
            candidate("second", "src/a.py", 2, 0, "binary_add_sub", Some("second")),
            candidate("shared", "src/a.py", 4, 0, "binary_add_sub", Some("shared")),
            candidate("other", "src/b.py", 1, 0, "binary_add_sub", Some("other")),
            candidate(
                "wrong-file",
                "src/b.py",
                2,
                0,
                "binary_add_sub",
                Some("first"),
            ),
            candidate("none", "src/a.py", 3, 0, "binary_add_sub", None),
            candidate(
                "empty",
                "src/empty.py",
                1,
                0,
                "binary_add_sub",
                Some("empty"),
            ),
            candidate(
                "missing-path",
                "src/missing.py",
                1,
                0,
                "binary_add_sub",
                Some("missing"),
            ),
        ],
    );

    let validated = validate_ranking_against(&Selection::default(), &targets, &ranked);

    assert_eq!(
        ranked
            .iter()
            .map(|entry| (entry.candidate.id.as_str(), entry.rank, entry.score))
            .collect::<Vec<_>>(),
        vec![
            ("first", 1, 320),
            ("second", 2, 320),
            ("shared", 3, 320),
            ("other", 4, 320),
            ("none", 5, 70),
            ("wrong-file", 6, 70),
            ("empty", 7, 70),
            ("missing-path", 8, 70),
        ]
    );
    for entry in ranked.iter().take(4) {
        assert_eq!(
            entry.ranking_reasons,
            vec![
                reason(RankingReasonCode::ExplicitSymbol, 250),
                reason(RankingReasonCode::Arithmetic, 70),
            ]
        );
    }
    for entry in ranked.iter().skip(4) {
        assert_eq!(
            entry.ranking_reasons,
            vec![reason(RankingReasonCode::Arithmetic, 70)]
        );
    }
    assert_eq!(validated, Ok(()));
}

#[test]
fn ranking_matches_explicit_symbol_ancestors_at_dot_boundaries_once() {
    let targets = vec![
        TargetSlice {
            path: Utf8PathBuf::from("src/calc.py"),
            lines: Vec::new(),
            symbols: vec!["Box".to_owned(), "Box.check".to_owned(), "outer".to_owned()],
        },
        TargetSlice {
            path: Utf8PathBuf::from("src/other.py"),
            lines: Vec::new(),
            symbols: vec!["Other".to_owned()],
        },
    ];
    let ranked = rank_candidates(
        &Selection::default(),
        &targets,
        vec![
            candidate("exact", "src/calc.py", 1, 0, "boolean_literal", Some("Box")),
            candidate(
                "method",
                "src/calc.py",
                2,
                0,
                "boolean_literal",
                Some("Box.check"),
            ),
            candidate(
                "nested-class",
                "src/calc.py",
                3,
                0,
                "boolean_literal",
                Some("Box.Inner.check"),
            ),
            candidate(
                "nested-function",
                "src/calc.py",
                4,
                0,
                "boolean_literal",
                Some("outer.inner"),
            ),
            candidate(
                "dot-prefix",
                "src/calc.py",
                5,
                0,
                "boolean_literal",
                Some("BoxOther.check"),
            ),
            candidate(
                "cross-file",
                "src/other.py",
                6,
                0,
                "boolean_literal",
                Some("Box.check"),
            ),
            candidate(
                "missing-symbol",
                "src/calc.py",
                7,
                0,
                "boolean_literal",
                None,
            ),
        ],
    );

    assert_eq!(
        ranked
            .iter()
            .map(|entry| (entry.id.as_str(), entry.rank, entry.score))
            .collect::<Vec<_>>(),
        vec![
            ("exact", 1, 350),
            ("method", 2, 350),
            ("nested-class", 3, 350),
            ("nested-function", 4, 350),
            ("dot-prefix", 5, 100),
            ("missing-symbol", 6, 100),
            ("cross-file", 7, 100),
        ]
    );
    for entry in ranked.iter().take(4) {
        assert_explicit_symbol_reason_once(entry);
    }
    for entry in ranked.iter().skip(4) {
        assert_eq!(
            entry.ranking_reasons,
            vec![reason(RankingReasonCode::HighValueControl, 100)],
            "{}",
            entry.id
        );
    }
}

fn assert_explicit_symbol_reason_once(entry: &RankedPlanCandidate) {
    assert_eq!(
        entry.ranking_reasons,
        vec![
            reason(RankingReasonCode::ExplicitSymbol, 250),
            reason(RankingReasonCode::HighValueControl, 100),
        ],
        "{} must receive one symbol reason even when parent and child selectors match",
        entry.id
    );
}

#[rstest::rstest]
#[case::dot_component("src/./a.py", 320, vec![reason(RankingReasonCode::ExplicitSymbol, 250), reason(RankingReasonCode::Arithmetic, 70)])]
#[case::case_distinct("src/A.py", 70, vec![reason(RankingReasonCode::Arithmetic, 70)])]
fn ranking_keeps_explicit_symbol_paths_exact(
    #[case] selected_path: &str,
    #[case] expected_score: u32,
    #[case]
    #[by_ref]
    expected_reasons: &[RankingReason],
) {
    let selection = Selection::default();
    let targets = [TargetSlice {
        path: Utf8PathBuf::from(selected_path),
        lines: Vec::new(),
        symbols: vec!["selected".to_owned()],
    }];
    let candidates = vec![candidate(
        "candidate",
        "src/a.py",
        1,
        0,
        "binary_add_sub",
        Some("selected"),
    )];

    let ranked = rank_candidates(&selection, &targets, candidates);

    assert_eq!(ranked[0].score, expected_score);
    assert_eq!(ranked[0].ranking_reasons, expected_reasons);
}

#[rstest::rstest]
#[case::relative("", "./src/calc.py")]
#[cfg_attr(windows, case::absolute("C:/workspace", "C:/workspace/src/calc.py"))]
#[cfg_attr(not(windows), case::absolute("/workspace", "/workspace/src/calc.py"))]
fn ranking_normalizes_explicit_line_paths_before_matching_candidates(
    #[case] root: &str,
    #[case] selected_path: &str,
) {
    let selection = Selection {
        root: Utf8PathBuf::from(root),
        lines: vec![LineSelection {
            path: Utf8PathBuf::from(selected_path),
            range: LineRange { start: 5, end: 5 },
        }],
        ..Selection::default()
    };
    let candidates = vec![candidate(
        "candidate",
        "src/calc.py",
        5,
        3,
        "binary_add_sub",
        None,
    )];

    let ranked = rank_candidates(&selection, &[], candidates);

    assert_eq!(
        ranked[0].ranking_reasons,
        [
            reason(RankingReasonCode::ExplicitLine, 300),
            reason(RankingReasonCode::Arithmetic, 70)
        ],
        "selected path {selected_path}"
    );
}

#[test]
fn ranking_preserves_mixed_selector_scores_reasons_and_ordering() {
    let (selection, targets, ranked) = mixed_selector_ranking();

    let validated = validate_ranking_against(&selection, &targets, &ranked);

    assert_eq!(
        ranked
            .iter()
            .map(|entry| (
                entry.candidate.id.as_str(),
                entry.rank,
                entry.score,
                entry
                    .ranking_reasons
                    .iter()
                    .map(|reason| reason.code)
                    .collect::<Vec<_>>(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "line-symbol",
                1,
                850,
                vec![
                    RankingReasonCode::ExplicitLine,
                    RankingReasonCode::ExplicitSymbol,
                    RankingReasonCode::ChangedLine,
                    RankingReasonCode::HighValueControl,
                ],
            ),
            (
                "other-line",
                2,
                570,
                vec![
                    RankingReasonCode::ExplicitLine,
                    RankingReasonCode::ChangedLine,
                    RankingReasonCode::Arithmetic,
                ],
            ),
            (
                "line-gap-symbol",
                3,
                520,
                vec![
                    RankingReasonCode::ExplicitSymbol,
                    RankingReasonCode::ChangedLine,
                    RankingReasonCode::Arithmetic,
                ],
            ),
            (
                "unselected",
                4,
                280,
                vec![
                    RankingReasonCode::ChangedLine,
                    RankingReasonCode::Behavioral,
                ],
            ),
            (
                "file-only",
                5,
                270,
                vec![
                    RankingReasonCode::ChangedLine,
                    RankingReasonCode::Arithmetic,
                ],
            ),
        ]
    );
    assert_eq!(validated, Ok(()));
}

fn mixed_selector_ranking() -> (Selection, Vec<TargetSlice>, Vec<RankedPlanCandidate>) {
    let selection = Selection {
        files: vec![Utf8PathBuf::from("src/file_only.py")],
        lines: vec![
            selection("src/line.py", 2, 4),
            selection("src/line.py", 9, 9),
            selection("src/other.py", 1, 1),
        ],
        changed: Some(hoimin_core::ChangedSelection::default()),
        ..Selection::default()
    };
    let targets = vec![
        TargetSlice {
            path: Utf8PathBuf::from("src/line.py"),
            lines: vec![LineRange { start: 2, end: 4 }],
            symbols: vec!["chosen".to_owned()],
        },
        TargetSlice {
            path: Utf8PathBuf::from("src/file_only.py"),
            lines: Vec::new(),
            symbols: Vec::new(),
        },
    ];
    let candidates = vec![
        candidate(
            "line-symbol",
            "src/line.py",
            2,
            0,
            "compare_eq_ne",
            Some("chosen"),
        ),
        candidate(
            "line-gap-symbol",
            "src/line.py",
            6,
            0,
            "binary_add_sub",
            Some("chosen"),
        ),
        candidate("other-line", "src/other.py", 1, 0, "binary_add_sub", None),
        candidate(
            "file-only",
            "src/file_only.py",
            1,
            0,
            "binary_add_sub",
            None,
        ),
        candidate(
            "unselected",
            "src/unselected.py",
            1,
            0,
            "collection_any_all",
            None,
        ),
    ];
    let ranked = rank_candidates(&selection, &targets, candidates);
    (selection, targets, ranked)
}

fn selection(path: &str, start: u32, end: u32) -> LineSelection {
    LineSelection {
        path: Utf8PathBuf::from(path),
        range: LineRange { start, end },
    }
}

#[cfg(windows)]
#[test]
fn ranking_compares_explicit_line_paths_with_windows_case_rules() {
    let selection = Selection {
        root: Utf8PathBuf::from("C:/Workspace"),
        lines: vec![LineSelection {
            path: Utf8PathBuf::from("c:/workspace/SRC/CALC.py"),
            range: LineRange { start: 5, end: 5 },
        }],
        ..Selection::default()
    };

    let ranked = rank_candidates(
        &selection,
        &[],
        vec![candidate(
            "candidate",
            "src/calc.py",
            5,
            3,
            "binary_add_sub",
            None,
        )],
    );

    assert!(
        ranked[0]
            .ranking_reasons
            .iter()
            .any(|reason| reason.code == RankingReasonCode::ExplicitLine)
    );
}

const HIGH_VALUE_CONTROL_OPERATORS: &[&str] = &[
    "lambda_capture_late",
    "continue_remove",
    "comparison_not_implemented",
    "condition_constant",
    "condition_clause_delete",
    "while_condition_false",
    "compare_eq_ne",
    "compare_order",
    "membership",
    "identity",
    "boolean_and_or",
    "remove_not",
    "boolean_literal",
    "break_continue",
    "loop_break_insert",
    "iterator_stop_insert",
    "async_iterator_stop_insert",
    "generator_return_insert",
    "generator_skip_first",
    "generator_empty",
    "none_truthiness",
    "nonlocal_remove",
    "yield_from_to_yield",
];
const EXCEPTION_HANDLING_OPERATORS: &[&str] = &[
    "exception_cause_remove",
    "gather_return_exceptions",
    "exception_hierarchy",
    "exception_type_pair",
    "exception_bare_to_exception",
    "exception_exception_to_bare",
    "exception_base_boundary",
    "exception_tuple_add_pair",
    "exception_tuple_remove_member",
];
const BEHAVIORAL_OPERATORS: &[&str] = &[
    "base_order_shuffle",
    "override_method_delete",
    "super_call_move",
    "dataclass_factory_share",
    "sort_key_swap",
    "encoding_errors_ignore",
    "itertools_take_drop_swap",
    "fstring_conversion_swap",
    "mutable_default_share",
    "mapping_setdefault_to_get",
    "bisect_boundary_swap",
    "deepcopy_to_copy",
    "deque_popleft_to_pop",
    "list_comprehension_to_generator",
    "method_call_remove",
    "function_body_return_constant",
    "function_body_erase",
    "enum_member_replace",
    "return_tuple_swap",
    "string_literal_empty",
    "string_segment_empty",
    "optional_keyword_delete",
    "conversion_call_remove",
    "stdlib_conversion_call_remove",
    "container_element_delete",
    "statement_delete",
    "operator_function",
    "collection_any_all",
    "collection_list_tuple",
    "collection_set_frozenset",
    "collection_append_insert",
    "collection_min_max",
    "collection_set_add_discard",
    "collection_set_remove_discard",
    "collection_string_starts_ends",
    "collection_string_split_rsplit",
    "structure_append_extend",
    "structure_mapping_get_subscript",
    "structure_sort_reverse",
    "structure_sorted_reversed",
    "structure_index_neighbor",
    "structure_slice_neighbor",
    "structure_slice_bound_omit",
    "structure_slice_bound_move",
];
const ARITHMETIC_OPERATORS: &[&str] = &[
    "integer_literal_neighbor",
    "augmented_to_assignment",
    "binary_add_sub",
    "augmented_add_sub",
    "binary_mul_div",
    "augmented_mul_div",
    "binary_floor_mod",
    "binary_div_floor",
    "augmented_floor_mod",
    "unary_sign",
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
];
const TYPE_ANNOTATION_OPERATORS: &[&str] = &[
    "type_nullable_remove",
    "type_nullable_add",
    "type_list_sequence",
    "type_set_abstract_set",
    "type_dict_mapping",
    "type_iterable_iterator",
    "type_sequence_iterable",
];

#[test]
fn ranking_category_inventory_covers_every_canonical_operator() {
    let categories = [
        HIGH_VALUE_CONTROL_OPERATORS,
        EXCEPTION_HANDLING_OPERATORS,
        BEHAVIORAL_OPERATORS,
        ARITHMETIC_OPERATORS,
        TYPE_ANNOTATION_OPERATORS,
    ];
    let tested = categories
        .iter()
        .flat_map(|operators| operators.iter())
        .map(|name| MutationOperator::from_name(name).expect("canonical operator name"))
        .collect::<BTreeSet<_>>();
    let canonical = MutationOperatorSelection::valid_names()
        .into_iter()
        .filter_map(MutationOperator::from_name)
        .collect::<BTreeSet<_>>();

    assert_eq!(tested.len(), 104);
    assert_eq!(tested, canonical);
}

#[rstest::rstest]
#[case::lambda_capture_late("lambda_capture_late", RankingReasonCode::HighValueControl, 100)]
#[case::continue_remove("continue_remove", RankingReasonCode::HighValueControl, 100)]
#[case::comparison_not_implemented(
    "comparison_not_implemented",
    RankingReasonCode::HighValueControl,
    100
)]
#[case::condition_constant("condition_constant", RankingReasonCode::HighValueControl, 100)]
#[case::condition_clause_delete(
    "condition_clause_delete",
    RankingReasonCode::HighValueControl,
    100
)]
#[case::while_condition_false("while_condition_false", RankingReasonCode::HighValueControl, 100)]
#[case::compare_eq_ne("compare_eq_ne", RankingReasonCode::HighValueControl, 100)]
#[case::compare_order("compare_order", RankingReasonCode::HighValueControl, 100)]
#[case::membership("membership", RankingReasonCode::HighValueControl, 100)]
#[case::identity("identity", RankingReasonCode::HighValueControl, 100)]
#[case::boolean_and_or("boolean_and_or", RankingReasonCode::HighValueControl, 100)]
#[case::remove_not("remove_not", RankingReasonCode::HighValueControl, 100)]
#[case::boolean_literal("boolean_literal", RankingReasonCode::HighValueControl, 100)]
#[case::break_continue("break_continue", RankingReasonCode::HighValueControl, 100)]
#[case::loop_break_insert("loop_break_insert", RankingReasonCode::HighValueControl, 100)]
#[case::iterator_stop_insert("iterator_stop_insert", RankingReasonCode::HighValueControl, 100)]
#[case::async_iterator_stop_insert(
    "async_iterator_stop_insert",
    RankingReasonCode::HighValueControl,
    100
)]
#[case::generator_return_insert(
    "generator_return_insert",
    RankingReasonCode::HighValueControl,
    100
)]
#[case::generator_skip_first("generator_skip_first", RankingReasonCode::HighValueControl, 100)]
#[case::generator_empty("generator_empty", RankingReasonCode::HighValueControl, 100)]
#[case::none_truthiness("none_truthiness", RankingReasonCode::HighValueControl, 100)]
#[case::nonlocal_remove("nonlocal_remove", RankingReasonCode::HighValueControl, 100)]
#[case::yield_from_to_yield("yield_from_to_yield", RankingReasonCode::HighValueControl, 100)]
#[case::exception_cause_remove("exception_cause_remove", RankingReasonCode::ExceptionHandling, 90)]
#[case::gather_return_exceptions(
    "gather_return_exceptions",
    RankingReasonCode::ExceptionHandling,
    90
)]
#[case::exception_hierarchy("exception_hierarchy", RankingReasonCode::ExceptionHandling, 90)]
#[case::exception_type_pair("exception_type_pair", RankingReasonCode::ExceptionHandling, 90)]
#[case::exception_bare_to_exception(
    "exception_bare_to_exception",
    RankingReasonCode::ExceptionHandling,
    90
)]
#[case::exception_exception_to_bare(
    "exception_exception_to_bare",
    RankingReasonCode::ExceptionHandling,
    90
)]
#[case::exception_base_boundary(
    "exception_base_boundary",
    RankingReasonCode::ExceptionHandling,
    90
)]
#[case::exception_tuple_add_pair(
    "exception_tuple_add_pair",
    RankingReasonCode::ExceptionHandling,
    90
)]
#[case::exception_tuple_remove_member(
    "exception_tuple_remove_member",
    RankingReasonCode::ExceptionHandling,
    90
)]
#[case::base_order_shuffle("base_order_shuffle", RankingReasonCode::Behavioral, 80)]
#[case::override_method_delete("override_method_delete", RankingReasonCode::Behavioral, 80)]
#[case::super_call_move("super_call_move", RankingReasonCode::Behavioral, 80)]
#[case::dataclass_factory_share("dataclass_factory_share", RankingReasonCode::Behavioral, 80)]
#[case::sort_key_swap("sort_key_swap", RankingReasonCode::Behavioral, 80)]
#[case::encoding_errors_ignore("encoding_errors_ignore", RankingReasonCode::Behavioral, 80)]
#[case::itertools_take_drop_swap("itertools_take_drop_swap", RankingReasonCode::Behavioral, 80)]
#[case::fstring_conversion_swap("fstring_conversion_swap", RankingReasonCode::Behavioral, 80)]
#[case::mutable_default_share("mutable_default_share", RankingReasonCode::Behavioral, 80)]
#[case::mapping_setdefault_to_get("mapping_setdefault_to_get", RankingReasonCode::Behavioral, 80)]
#[case::bisect_boundary_swap("bisect_boundary_swap", RankingReasonCode::Behavioral, 80)]
#[case::deepcopy_to_copy("deepcopy_to_copy", RankingReasonCode::Behavioral, 80)]
#[case::deque_popleft_to_pop("deque_popleft_to_pop", RankingReasonCode::Behavioral, 80)]
#[case::list_comprehension_to_generator(
    "list_comprehension_to_generator",
    RankingReasonCode::Behavioral,
    80
)]
#[case::method_call_remove("method_call_remove", RankingReasonCode::Behavioral, 80)]
#[case::function_body_return_constant(
    "function_body_return_constant",
    RankingReasonCode::Behavioral,
    80
)]
#[case::function_body_erase("function_body_erase", RankingReasonCode::Behavioral, 80)]
#[case::enum_member_replace("enum_member_replace", RankingReasonCode::Behavioral, 80)]
#[case::return_tuple_swap("return_tuple_swap", RankingReasonCode::Behavioral, 80)]
#[case::string_literal_empty("string_literal_empty", RankingReasonCode::Behavioral, 80)]
#[case::string_segment_empty("string_segment_empty", RankingReasonCode::Behavioral, 80)]
#[case::optional_keyword_delete("optional_keyword_delete", RankingReasonCode::Behavioral, 80)]
#[case::conversion_call_remove("conversion_call_remove", RankingReasonCode::Behavioral, 80)]
#[case::stdlib_conversion_call_remove(
    "stdlib_conversion_call_remove",
    RankingReasonCode::Behavioral,
    80
)]
#[case::container_element_delete("container_element_delete", RankingReasonCode::Behavioral, 80)]
#[case::statement_delete("statement_delete", RankingReasonCode::Behavioral, 80)]
#[case::operator_function("operator_function", RankingReasonCode::Behavioral, 80)]
#[case::collection_any_all("collection_any_all", RankingReasonCode::Behavioral, 80)]
#[case::collection_list_tuple("collection_list_tuple", RankingReasonCode::Behavioral, 80)]
#[case::collection_set_frozenset("collection_set_frozenset", RankingReasonCode::Behavioral, 80)]
#[case::collection_append_insert("collection_append_insert", RankingReasonCode::Behavioral, 80)]
#[case::collection_min_max("collection_min_max", RankingReasonCode::Behavioral, 80)]
#[case::collection_set_add_discard("collection_set_add_discard", RankingReasonCode::Behavioral, 80)]
#[case::collection_set_remove_discard(
    "collection_set_remove_discard",
    RankingReasonCode::Behavioral,
    80
)]
#[case::collection_string_starts_ends(
    "collection_string_starts_ends",
    RankingReasonCode::Behavioral,
    80
)]
#[case::collection_string_split_rsplit(
    "collection_string_split_rsplit",
    RankingReasonCode::Behavioral,
    80
)]
#[case::structure_append_extend("structure_append_extend", RankingReasonCode::Behavioral, 80)]
#[case::structure_mapping_get_subscript(
    "structure_mapping_get_subscript",
    RankingReasonCode::Behavioral,
    80
)]
#[case::structure_sort_reverse("structure_sort_reverse", RankingReasonCode::Behavioral, 80)]
#[case::structure_sorted_reversed("structure_sorted_reversed", RankingReasonCode::Behavioral, 80)]
#[case::structure_index_neighbor("structure_index_neighbor", RankingReasonCode::Behavioral, 80)]
#[case::structure_slice_neighbor("structure_slice_neighbor", RankingReasonCode::Behavioral, 80)]
#[case::structure_slice_bound_omit("structure_slice_bound_omit", RankingReasonCode::Behavioral, 80)]
#[case::structure_slice_bound_move("structure_slice_bound_move", RankingReasonCode::Behavioral, 80)]
#[case::integer_literal_neighbor("integer_literal_neighbor", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_to_assignment("augmented_to_assignment", RankingReasonCode::Arithmetic, 70)]
#[case::binary_add_sub("binary_add_sub", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_add_sub("augmented_add_sub", RankingReasonCode::Arithmetic, 70)]
#[case::binary_mul_div("binary_mul_div", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_mul_div("augmented_mul_div", RankingReasonCode::Arithmetic, 70)]
#[case::binary_floor_mod("binary_floor_mod", RankingReasonCode::Arithmetic, 70)]
#[case::binary_div_floor("binary_div_floor", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_floor_mod("augmented_floor_mod", RankingReasonCode::Arithmetic, 70)]
#[case::unary_sign("unary_sign", RankingReasonCode::Arithmetic, 70)]
#[case::bitwise_and_or("bitwise_and_or", RankingReasonCode::Arithmetic, 70)]
#[case::bitwise_shift("bitwise_shift", RankingReasonCode::Arithmetic, 70)]
#[case::binary_power("binary_power", RankingReasonCode::Arithmetic, 70)]
#[case::binary_matmul("binary_matmul", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_power("augmented_power", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_matmul("augmented_matmul", RankingReasonCode::Arithmetic, 70)]
#[case::bitwise_xor("bitwise_xor", RankingReasonCode::Arithmetic, 70)]
#[case::bitwise_invert("bitwise_invert", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_bitwise_and_or("augmented_bitwise_and_or", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_bitwise_xor("augmented_bitwise_xor", RankingReasonCode::Arithmetic, 70)]
#[case::augmented_bitwise_shift("augmented_bitwise_shift", RankingReasonCode::Arithmetic, 70)]
#[case::type_nullable_remove("type_nullable_remove", RankingReasonCode::TypeAnnotation, 50)]
#[case::type_nullable_add("type_nullable_add", RankingReasonCode::TypeAnnotation, 50)]
#[case::type_list_sequence("type_list_sequence", RankingReasonCode::TypeAnnotation, 50)]
#[case::type_set_abstract_set("type_set_abstract_set", RankingReasonCode::TypeAnnotation, 50)]
#[case::type_dict_mapping("type_dict_mapping", RankingReasonCode::TypeAnnotation, 50)]
#[case::type_iterable_iterator("type_iterable_iterator", RankingReasonCode::TypeAnnotation, 50)]
#[case::type_sequence_iterable("type_sequence_iterable", RankingReasonCode::TypeAnnotation, 50)]
fn ranking_assigns_every_operator_to_its_fixed_category(
    #[case] operator: &str,
    #[case] code: RankingReasonCode,
    #[case] score: u32,
) {
    let expected = vec![reason(code, score)];
    let candidates = vec![candidate("candidate", "src/calc.py", 1, 0, operator, None)];

    let ranked = rank_candidates(&Selection::default(), &[], candidates);
    let validated = validate_ranking(&ranked);

    assert_eq!(ranked[0].ranking_reasons, expected, "{operator}");
    assert_eq!(validated, Ok(()), "{operator}");
}

#[test]
fn ranking_breaks_score_ties_by_stable_candidate_fields() {
    let ranked = rank_candidates(
        &Selection::default(),
        &[],
        vec![
            candidate("z", "src/b.py", 1, 0, "binary_add_sub", None),
            candidate("z", "src/a.py", 2, 0, "binary_add_sub", None),
            candidate("z", "src/a.py", 1, 2, "binary_add_sub", None),
            candidate("z", "src/a.py", 1, 1, "unary_sign", None),
            candidate("b", "src/a.py", 1, 1, "binary_add_sub", None),
            candidate("a", "src/a.py", 1, 1, "binary_add_sub", None),
        ],
    );

    assert_eq!(
        ranked
            .iter()
            .map(|entry| (
                entry.candidate.path.as_str(),
                entry.candidate.line,
                entry.candidate.column,
                entry.candidate.operator.as_str(),
                entry.candidate.id.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            ("src/a.py", 1, 1, "binary_add_sub", "a"),
            ("src/a.py", 1, 1, "binary_add_sub", "b"),
            ("src/a.py", 1, 1, "unary_sign", "z"),
            ("src/a.py", 1, 2, "binary_add_sub", "z"),
            ("src/a.py", 2, 0, "binary_add_sub", "z"),
            ("src/b.py", 1, 0, "binary_add_sub", "z"),
        ]
    );
    assert_eq!(
        ranked.iter().map(|entry| entry.rank).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
}

#[rstest::rstest]
#[case::zero_rank(set_zero_rank, "rank")]
#[case::duplicate_rank(duplicate_rank, "rank")]
#[case::wrong_score(increment_score, "score")]
#[case::wrong_reason_score(increment_reason_score, "reason")]
#[case::duplicate_reason(duplicate_reason, "reason")]
#[case::reordered_rows(reverse_rows_preserving_ranks, "deterministic order")]
fn ranking_validation_rejects_tampered_entries(
    #[case] mutate: fn(&mut [RankedPlanCandidate]),
    #[case] expected: &str,
) {
    let mut tampered = rank_candidates(
        &Selection::default(),
        &[],
        vec![
            candidate("a", "src/a.py", 1, 0, "binary_add_sub", None),
            candidate("b", "src/b.py", 1, 0, "binary_add_sub", None),
        ],
    );
    mutate(&mut tampered);

    let error = validate_ranking(&tampered).unwrap_err();

    assert!(error.contains(expected), "{error}");
}

#[test]
fn ranking_semantic_validation_rejects_a_consistently_rescored_wrong_reason() {
    let selection = Selection::default();
    let mut ranked = rank_candidates(
        &selection,
        &[],
        vec![candidate("a", "src/a.py", 1, 0, "binary_add_sub", None)],
    );
    ranked[0].ranking_reasons = vec![reason(RankingReasonCode::HighValueControl, 100)];
    ranked[0].score = 100;

    let structural = validate_ranking(&ranked);
    let semantic = validate_ranking_against(&selection, &[], &ranked);

    assert!(structural.is_ok());
    assert!(semantic.is_err());
}

fn set_zero_rank(entries: &mut [RankedPlanCandidate]) {
    entries[0].rank = 0;
}

fn duplicate_rank(entries: &mut [RankedPlanCandidate]) {
    entries[1].rank = entries[0].rank;
}

fn increment_score(entries: &mut [RankedPlanCandidate]) {
    entries[0].score += 1;
}

fn reverse_rows_preserving_ranks(entries: &mut [RankedPlanCandidate]) {
    entries.swap(0, 1);
    entries[0].rank = 1;
    entries[1].rank = 2;
}

#[test]
fn ranking_validation_rejects_multiple_operator_reasons_after_bounded_score_accumulation() {
    let ranked = RankedPlanCandidate {
        candidate: candidate("candidate", "src/calc.py", 1, 0, "binary_add_sub", None),
        rank: 1,
        score: 1140,
        ranking_reasons: vec![
            reason(RankingReasonCode::ExplicitLine, 300),
            reason(RankingReasonCode::ExplicitSymbol, 250),
            reason(RankingReasonCode::ChangedLine, 200),
            reason(RankingReasonCode::HighValueControl, 100),
            reason(RankingReasonCode::ExceptionHandling, 90),
            reason(RankingReasonCode::Behavioral, 80),
            reason(RankingReasonCode::Arithmetic, 70),
            reason(RankingReasonCode::TypeAnnotation, 50),
        ],
    };

    let actual_0 = validate_ranking(&[ranked]);

    assert_eq!(
        actual_0,
        Err("candidate ranking must contain exactly one operator reason, got 5".to_owned())
    );
}

#[test]
fn ranking_validation_rejects_an_unbounded_external_reason_score_before_accumulation() {
    let ranked = RankedPlanCandidate {
        candidate: candidate("candidate", "src/calc.py", 1, 0, "binary_add_sub", None),
        rank: 1,
        score: u32::MAX,
        ranking_reasons: vec![reason(RankingReasonCode::Arithmetic, u32::MAX)],
    };

    let actual_0 = validate_ranking(&[ranked]);

    assert_eq!(
        actual_0,
        Err("ranking reason Arithmetic has score 4294967295, expected 70".to_owned())
    );
}

fn increment_reason_score(entries: &mut [RankedPlanCandidate]) {
    entries[0].ranking_reasons[0].score += 1;
}

fn duplicate_reason(entries: &mut [RankedPlanCandidate]) {
    let reason = entries[0].ranking_reasons[0].clone();
    entries[0].ranking_reasons.push(reason);
}

fn assert_previous_ranking_contract(
    selection: &Selection,
    targets: &[TargetSlice],
    candidates: &[RankedPlanCandidate],
) -> Result<(), String> {
    let expected = (rank_candidates(
        selection,
        targets,
        candidates.iter().map(|row| row.candidate.clone()).collect(),
    ) == candidates)
        .then_some(())
        .ok_or_else(|| "candidate ranking differs from the deterministic ranking rules".to_owned());

    let actual = validate_ranking_against(selection, targets, candidates);

    assert_eq!(actual, expected);
    actual
}

#[rstest::rstest]
#[case::empty(0)]
#[case::valid(5)]
fn ranking_semantic_validation_accepts_untampered_metadata(#[case] count: usize) {
    let (selection, targets, mut ranked) = mixed_selector_ranking();
    ranked.truncate(count);

    assert_previous_ranking_contract(&selection, &targets, &ranked).unwrap();
}

#[rstest::rstest]
#[case::zero_rank(set_zero_rank)]
#[case::duplicate_rank(duplicate_rank)]
#[case::wrong_score(increment_score)]
#[case::wrong_reason_score(increment_reason_score)]
#[case::duplicate_reason(duplicate_reason)]
fn ranking_semantic_validation_preserves_metadata_contract(
    #[case] mutate: fn(&mut [RankedPlanCandidate]),
) {
    let (selection, targets, mut tampered) = mixed_selector_ranking();
    mutate(&mut tampered);

    assert_previous_ranking_contract(&selection, &targets, &tampered).unwrap_err();
}

#[rstest::rstest]
#[case::line_symbol(0)]
#[case::other_line(1)]
#[case::line_gap_symbol(2)]
#[case::unselected(3)]
#[case::file_only(4)]
fn ranking_semantic_validation_rejects_rescored_rows(#[case] position: usize) {
    let (selection, targets, mut tampered) = mixed_selector_ranking();
    tampered[position].ranking_reasons = vec![reason(RankingReasonCode::TypeAnnotation, 50)];
    tampered[position].score = 50;

    assert_previous_ranking_contract(&selection, &targets, &tampered).unwrap_err();
}

type RankingFixture = fn() -> (Selection, Vec<TargetSlice>, Vec<RankedPlanCandidate>);

#[rstest::rstest]
#[case::mixed_combined_selectors(mixed_selector_ranking, 1)]
#[case::mixed_explicit_line_vs_symbol(mixed_selector_ranking, 2)]
#[case::mixed_explicit_symbol_vs_behavioral(mixed_selector_ranking, 3)]
#[case::mixed_behavioral_vs_arithmetic(mixed_selector_ranking, 4)]
#[case::tied_candidate_id(tied_ranking, 1)]
#[case::tied_operator(tied_ranking, 2)]
#[case::tied_column(tied_ranking, 3)]
#[case::tied_line(tied_ranking, 4)]
#[case::tied_path(tied_ranking, 5)]
fn ranking_semantic_validation_rejects_reordered_adjacent_rows(
    #[case] fixture: RankingFixture,
    #[case] position: usize,
) {
    let (selection, targets, mut reordered) = fixture();
    reordered.swap(position - 1, position);
    for (index, row) in reordered.iter_mut().enumerate() {
        row.rank = index + 1;
    }

    assert_previous_ranking_contract(&selection, &targets, &reordered).unwrap_err();
}

fn tied_ranking() -> (Selection, Vec<TargetSlice>, Vec<RankedPlanCandidate>) {
    let selection = Selection::default();
    let ranked = rank_candidates(
        &selection,
        &[],
        vec![
            candidate("a", "a.py", 1, 0, "binary_add_sub", None),
            candidate("b", "a.py", 1, 0, "binary_add_sub", None),
            candidate("b", "a.py", 1, 0, "unary_sign", None),
            candidate("b", "a.py", 1, 1, "unary_sign", None),
            candidate("b", "a.py", 2, 1, "unary_sign", None),
            candidate("b", "b.py", 2, 1, "unary_sign", None),
        ],
    );
    (selection, Vec::new(), ranked)
}

#[rstest::rstest]
#[case::known_original_order("binary_add_sub", [0, 1])]
#[case::known_reversed_order("binary_add_sub", [1, 0])]
#[case::unknown_original_order("unknown_operator", [0, 1])]
#[case::unknown_reversed_order("unknown_operator", [1, 0])]
fn ranking_semantic_validation_preserves_stable_equal_key_ties(
    #[case] operator: &str,
    #[case] order: [usize; 2],
) {
    let selection = Selection::default();
    let first = candidate("same", "same.py", 1, 0, operator, None);
    let mut second = first.clone();
    second.original = "different body".into();
    second.replacement = "different replacement".into();
    second.sequence = 2;
    let ranked = rank_candidates(&selection, &[], vec![first, second]);
    let reordered = order
        .into_iter()
        .enumerate()
        .map(|(position, source)| {
            let mut row = ranked[source].clone();
            row.rank = position + 1;
            row
        })
        .collect::<Vec<_>>();

    assert_previous_ranking_contract(&selection, &[], &reordered).unwrap();
}
