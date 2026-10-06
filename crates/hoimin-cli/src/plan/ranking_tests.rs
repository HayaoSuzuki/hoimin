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
        changed: true,
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
    validate_ranking_against(&Selection::default(), &targets, &ranked).unwrap();
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

#[test]
fn ranking_keeps_explicit_symbol_paths_exact() {
    let dot_path = rank_candidates(
        &Selection::default(),
        &[TargetSlice {
            path: Utf8PathBuf::from("src/./a.py"),
            lines: Vec::new(),
            symbols: vec!["selected".to_owned()],
        }],
        vec![candidate(
            "candidate",
            "src/a.py",
            1,
            0,
            "binary_add_sub",
            Some("selected"),
        )],
    );
    assert_eq!(dot_path[0].score, 320);
    assert_eq!(
        dot_path[0].ranking_reasons,
        vec![
            reason(RankingReasonCode::ExplicitSymbol, 250),
            reason(RankingReasonCode::Arithmetic, 70),
        ]
    );

    let case_distinct = rank_candidates(
        &Selection::default(),
        &[TargetSlice {
            path: Utf8PathBuf::from("src/A.py"),
            lines: Vec::new(),
            symbols: vec!["selected".to_owned()],
        }],
        vec![candidate(
            "candidate",
            "src/a.py",
            1,
            0,
            "binary_add_sub",
            Some("selected"),
        )],
    );
    assert_eq!(case_distinct[0].score, 70);
    assert_eq!(
        case_distinct[0].ranking_reasons,
        vec![reason(RankingReasonCode::Arithmetic, 70)]
    );
}

#[test]
fn ranking_normalizes_explicit_line_paths_before_matching_candidates() {
    let (absolute_root, absolute_selection) = if cfg!(windows) {
        ("C:/workspace", "C:/workspace/src/calc.py")
    } else {
        ("/workspace", "/workspace/src/calc.py")
    };
    for (root, selected_path) in [("", "./src/calc.py"), (absolute_root, absolute_selection)] {
        let selection = Selection {
            root: Utf8PathBuf::from(root),
            lines: vec![LineSelection {
                path: Utf8PathBuf::from(selected_path),
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

        assert_eq!(
            ranked[0].ranking_reasons,
            [
                reason(RankingReasonCode::ExplicitLine, 300),
                reason(RankingReasonCode::Arithmetic, 70),
            ],
            "selected path {selected_path}"
        );
    }
}

#[test]
fn ranking_preserves_mixed_selector_scores_reasons_and_ordering() {
    let (selection, targets, ranked) = mixed_selector_ranking();

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
    validate_ranking_against(&selection, &targets, &ranked).unwrap();
}

fn mixed_selector_ranking() -> (Selection, Vec<TargetSlice>, Vec<RankedPlanCandidate>) {
    let selection = Selection {
        files: vec![Utf8PathBuf::from("src/file_only.py")],
        lines: vec![
            selection("src/line.py", 2, 4),
            selection("src/line.py", 9, 9),
            selection("src/other.py", 1, 1),
        ],
        changed: true,
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
    "condition_constant",
    "compare_eq_ne",
    "compare_order",
    "membership",
    "identity",
    "boolean_and_or",
    "remove_not",
    "boolean_literal",
    "break_continue",
];
const EXCEPTION_HANDLING_OPERATORS: &[&str] = &[
    "exception_hierarchy",
    "exception_type_pair",
    "exception_bare_to_exception",
    "exception_exception_to_bare",
    "exception_base_boundary",
    "exception_tuple_add_pair",
    "exception_tuple_remove_member",
];
const BEHAVIORAL_OPERATORS: &[&str] = &[
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
];
const ARITHMETIC_OPERATORS: &[&str] = &[
    "integer_literal_neighbor",
    "binary_add_sub",
    "augmented_add_sub",
    "binary_mul_div",
    "augmented_mul_div",
    "binary_floor_mod",
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
fn ranking_assigns_every_operator_to_its_fixed_category() {
    let categories = [
        (
            HIGH_VALUE_CONTROL_OPERATORS,
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        (
            EXCEPTION_HANDLING_OPERATORS,
            reason(RankingReasonCode::ExceptionHandling, 90),
        ),
        (
            BEHAVIORAL_OPERATORS,
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            ARITHMETIC_OPERATORS,
            reason(RankingReasonCode::Arithmetic, 70),
        ),
        (
            TYPE_ANNOTATION_OPERATORS,
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
    ];
    let tested = categories
        .iter()
        .flat_map(|(operators, _)| operators.iter())
        .map(|name| MutationOperator::from_name(name).expect("canonical operator name"))
        .collect::<BTreeSet<_>>();
    let canonical = MutationOperatorSelection::valid_names()
        .into_iter()
        .filter_map(MutationOperator::from_name)
        .collect::<BTreeSet<_>>();
    assert_eq!(tested.len(), 59);
    assert_eq!(tested, canonical);

    for (operators, expected) in categories {
        for operator in operators {
            let ranked = rank_candidates(
                &Selection::default(),
                &[],
                vec![candidate("candidate", "src/calc.py", 1, 0, operator, None)],
            );
            assert_eq!(
                ranked[0].ranking_reasons.as_slice(),
                std::slice::from_ref(&expected),
                "{operator}"
            );
            validate_ranking(&ranked).unwrap_or_else(|error| panic!("{operator}: {error}"));
        }
    }
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

#[test]
fn ranking_validation_rejects_tampered_entries() {
    let valid = rank_candidates(
        &Selection::default(),
        &[],
        vec![
            candidate("a", "src/a.py", 1, 0, "binary_add_sub", None),
            candidate("b", "src/b.py", 1, 0, "binary_add_sub", None),
        ],
    );

    for (name, mutate, expected) in [
        (
            "zero rank",
            set_zero_rank as fn(&mut [RankedPlanCandidate]),
            "rank",
        ),
        ("duplicate rank", duplicate_rank, "rank"),
        ("wrong score", increment_score, "score"),
        ("wrong reason score", increment_reason_score, "reason"),
        ("duplicate reason", duplicate_reason, "reason"),
    ] {
        let mut tampered = valid.clone();
        mutate(&mut tampered);
        let error = validate_ranking(&tampered).unwrap_err();
        assert!(error.contains(expected), "{name}: {error}");
    }
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

    assert!(validate_ranking(&ranked).is_ok());
    assert!(validate_ranking_against(&selection, &[], &ranked).is_err());
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
) {
    let expected = rank_candidates(
        selection,
        targets,
        candidates.iter().map(|row| row.candidate.clone()).collect(),
    ) == candidates;
    let actual = validate_ranking_against(selection, targets, candidates);
    assert_eq!(actual.is_ok(), expected);
    if !expected {
        assert_eq!(
            actual.unwrap_err(),
            "candidate ranking differs from the deterministic ranking rules"
        );
    }
}

#[test]
fn ranking_semantic_validation_preserves_metadata_and_order_contract() {
    let (selection, targets, valid) = mixed_selector_ranking();
    assert_previous_ranking_contract(&selection, &targets, &[]);
    assert_previous_ranking_contract(&selection, &targets, &valid);
    for mutate in [
        set_zero_rank,
        duplicate_rank,
        increment_score,
        increment_reason_score,
        duplicate_reason,
    ] {
        let mut tampered = valid.clone();
        mutate(&mut tampered);
        assert_previous_ranking_contract(&selection, &targets, &tampered);
    }
    for position in 0..valid.len() {
        let mut tampered = valid.clone();
        tampered[position].ranking_reasons = vec![reason(RankingReasonCode::TypeAnnotation, 50)];
        tampered[position].score = 50;
        assert_previous_ranking_contract(&selection, &targets, &tampered);
    }
    let tied = rank_candidates(
        &Selection::default(),
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
    let default = Selection::default();
    for (context, resolved, sorted) in [
        (&selection, targets.as_slice(), valid.as_slice()),
        (&default, &[][..], tied.as_slice()),
    ] {
        for position in 1..sorted.len() {
            let mut reordered = sorted.to_vec();
            reordered.swap(position - 1, position);
            for (index, row) in reordered.iter_mut().enumerate() {
                row.rank = index + 1;
            }
            assert_previous_ranking_contract(context, resolved, &reordered);
            assert!(validate_ranking_against(context, resolved, &reordered).is_err());
        }
    }
}

#[test]
fn ranking_semantic_validation_preserves_stable_equal_key_ties() {
    for operator in ["binary_add_sub", "unknown_operator"] {
        let first = candidate("same", "same.py", 1, 0, operator, None);
        let mut second = first.clone();
        second.original = "different body".into();
        second.replacement = "different replacement".into();
        second.sequence = 2;
        let ranked = rank_candidates(&Selection::default(), &[], vec![first, second]);
        assert_previous_ranking_contract(&Selection::default(), &[], &ranked);
        assert!(validate_ranking_against(&Selection::default(), &[], &ranked).is_ok());
        let mut reversed = ranked;
        reversed.swap(0, 1);
        reversed[0].rank = 1;
        reversed[1].rank = 2;
        assert_previous_ranking_contract(&Selection::default(), &[], &reversed);
        assert!(validate_ranking_against(&Selection::default(), &[], &reversed).is_ok());
    }
}
