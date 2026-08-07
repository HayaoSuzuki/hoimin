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

#[test]
fn ranking_assigns_every_operator_to_its_fixed_category() {
    let cases = [
        (
            "compare_eq_ne",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        (
            "compare_order",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        (
            "membership",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        ("identity", reason(RankingReasonCode::HighValueControl, 100)),
        (
            "boolean_and_or",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        ("binary_add_sub", reason(RankingReasonCode::Arithmetic, 70)),
        (
            "augmented_add_sub",
            reason(RankingReasonCode::Arithmetic, 70),
        ),
        ("binary_mul_div", reason(RankingReasonCode::Arithmetic, 70)),
        (
            "binary_floor_mod",
            reason(RankingReasonCode::Arithmetic, 70),
        ),
        ("unary_sign", reason(RankingReasonCode::Arithmetic, 70)),
        (
            "remove_not",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        (
            "boolean_literal",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        (
            "break_continue",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        (
            "collection_any_all",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_list_tuple",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_set_frozenset",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_append_insert",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_min_max",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_set_add_discard",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_set_remove_discard",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_string_starts_ends",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "collection_string_split_rsplit",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        ("bitwise_and_or", reason(RankingReasonCode::Arithmetic, 70)),
        ("bitwise_shift", reason(RankingReasonCode::Arithmetic, 70)),
        (
            "structure_append_extend",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "structure_mapping_get_subscript",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "structure_sort_reverse",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "structure_sorted_reversed",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "structure_index_neighbor",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "structure_slice_neighbor",
            reason(RankingReasonCode::Behavioral, 80),
        ),
        (
            "exception_type_pair",
            reason(RankingReasonCode::ExceptionHandling, 90),
        ),
        (
            "exception_bare_to_exception",
            reason(RankingReasonCode::ExceptionHandling, 90),
        ),
        (
            "exception_exception_to_bare",
            reason(RankingReasonCode::ExceptionHandling, 90),
        ),
        (
            "exception_base_boundary",
            reason(RankingReasonCode::ExceptionHandling, 90),
        ),
        (
            "exception_tuple_add_pair",
            reason(RankingReasonCode::ExceptionHandling, 90),
        ),
        (
            "exception_tuple_remove_member",
            reason(RankingReasonCode::ExceptionHandling, 90),
        ),
        (
            "type_nullable_remove",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
        (
            "type_nullable_add",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
        (
            "type_list_sequence",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
        (
            "type_set_abstract_set",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
        (
            "type_dict_mapping",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
        (
            "type_iterable_iterator",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
        (
            "type_sequence_iterable",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
    ];

    let tested = cases
        .iter()
        .map(|(name, _)| MutationOperator::from_name(name).expect("canonical operator name"))
        .collect::<BTreeSet<_>>();
    let canonical = MutationOperatorSelection::valid_names()
        .into_iter()
        .filter_map(MutationOperator::from_name)
        .collect::<BTreeSet<_>>();
    assert_eq!(tested.len(), 43);
    assert_eq!(tested, canonical);

    for (operator, expected) in cases {
        let ranked = rank_candidates(
            &Selection::default(),
            &[],
            vec![candidate("candidate", "src/calc.py", 1, 0, operator, None)],
        );
        assert_eq!(ranked[0].ranking_reasons, [expected], "{operator}");
        validate_ranking(&ranked).unwrap_or_else(|error| panic!("{operator}: {error}"));
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
