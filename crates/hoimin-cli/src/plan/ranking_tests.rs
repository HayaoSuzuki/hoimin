use camino::Utf8PathBuf;
use hoimin_core::{ByteSpan, LineRange, LineSelection, MutationCandidate, Selection, TargetSlice};

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
fn ranking_assigns_every_operator_to_its_fixed_category() {
    for (operator, expected) in [
        (
            "boolean_and_or",
            reason(RankingReasonCode::HighValueControl, 100),
        ),
        ("binary_add_sub", reason(RankingReasonCode::Arithmetic, 70)),
        (
            "type_nullable_remove",
            reason(RankingReasonCode::TypeAnnotation, 50),
        ),
    ] {
        let ranked = rank_candidates(
            &Selection::default(),
            &[],
            vec![candidate("candidate", "src/calc.py", 1, 0, operator, None)],
        );
        assert_eq!(ranked[0].ranking_reasons, [expected], "{operator}");
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
