use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile, MutationSelector};
use hoimin_python::{
    AnalyzeRequest, AnalyzerOutput, analyze_source_cancellable, payload::PayloadBudget,
};
use proptest::prelude::*;

const SITES: &[(MutationOperator, &str)] = &[
    (MutationOperator::FloatLiteralNeighbor, "value = 1.5\n"),
    (MutationOperator::BytesLiteralEmpty, "value = b'payload'\n"),
    (
        MutationOperator::FStringFormatSpecRemove,
        "value = f'{number:.2f}'\n",
    ),
    (
        MutationOperator::FStringFormatValueNeighbor,
        "value = f'{number:08.2f}'\n",
    ),
    (
        MutationOperator::ConditionalBranchSwap,
        "value = first if ready else second\n",
    ),
    (
        MutationOperator::AwaitRemove,
        "async def f():\n    return await work()\n",
    ),
    (
        MutationOperator::DictUnpackOrderSwap,
        "value = {**base, **overrides}\n",
    ),
    (
        MutationOperator::DictUnpackDelete,
        "value = {**base, **overrides}\n",
    ),
    (MutationOperator::DelRemove, "def f():\n    del value\n"),
    (
        MutationOperator::ExceptionHandlerSwap,
        "try:\n    work()\nexcept ValueError:\n    first()\nexcept TypeError:\n    second()\n",
    ),
    (
        MutationOperator::ExceptionCauseOmit,
        "raise ValueError() from None\n",
    ),
    (
        MutationOperator::MatchCaseSwap,
        "match value:\n    case 1:\n        first()\n    case 2:\n        second()\n",
    ),
    (
        MutationOperator::MatchPatternWildcard,
        "match value:\n    case [1, name]:\n        work(name)\n",
    ),
    (
        MutationOperator::MatchClassPositionSwap,
        "match value:\n    case Point(1, 2):\n        work()\n",
    ),
];

fn analyze(source: &str, operators: &MutationOperatorSelection, limit: usize) -> AnalyzerOutput {
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("example.py"),
        lines: &[],
        symbols: &[],
        operators,
        profile: MutationProfile::Full,
        max_candidates: limit,
    };
    analyze_source_cancellable(&request, source, || false).unwrap()
}

#[rstest::rstest]
#[case::float(0)]
#[case::bytes(1)]
#[case::format_spec(2)]
#[case::format_value(3)]
#[case::conditional(4)]
#[case::await_expression(5)]
#[case::unpack_order(6)]
#[case::unpack_delete(7)]
#[case::del_statement(8)]
#[case::handler_order(9)]
#[case::exception_cause(10)]
#[case::case_order(11)]
#[case::pattern_constraint(12)]
#[case::class_position(13)]
fn default_analysis_discovers_reference_mutation(#[case] site: usize) {
    // Arrange
    let (operator, source) = SITES[site];
    let operators = MutationOperatorSelection::default();

    // Act
    let output = analyze(source, &operators, 100);

    // Assert
    assert!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.operator == operator),
        "{operator:?}: {:?}",
        output.candidates
    );
}

#[rstest::rstest]
#[case::float("1.5", MutationOperator::FloatLiteralNeighbor)]
#[case::integer("0xff", MutationOperator::IntegerLiteralNeighbor)]
#[case::bytes("b'payload'", MutationOperator::BytesLiteralEmpty)]
#[case::formatted("f'{value:.2f}'", MutationOperator::FStringFormatSpecRemove)]
#[case::conditional("first if ready else second", MutationOperator::ConditionalBranchSwap)]
#[case::unpack("{**first, **second}", MutationOperator::DictUnpackDelete)]
fn explicit_type_alias_rhs_is_not_a_runtime_site(
    #[case] expression: &str,
    #[case] operator: MutationOperator,
) {
    // Arrange
    let source = format!("from typing import TypeAlias\nAlias: TypeAlias = {expression}\n");
    let operators = MutationOperatorSelection::default();

    // Act
    let output = analyze(&source, &operators, 100);

    // Assert
    assert!(
        output
            .candidates
            .iter()
            .all(|candidate| candidate.operator != operator)
    );
}

#[rstest::rstest]
#[case::constant(MutationOperator::ConditionConstant, 2)]
#[case::clause(MutationOperator::ConditionClauseDelete, 2)]
fn conditional_expression_reuses_condition_operators(
    #[case] operator: MutationOperator,
    #[case] count: usize,
) {
    // Arrange
    let source = "result = first if enabled and ready else second\n";
    let mut operators = MutationOperatorSelection::default();
    operators.include(operator);

    // Act
    let output = analyze(source, &operators, 100);

    // Assert
    assert_eq!(
        output
            .candidates
            .iter()
            .filter(|candidate| candidate.operator == operator)
            .count(),
        count
    );
}

#[rstest::rstest]
#[case::conditional(
    "conditional_branch_swap",
    "value = ",
    "first if ready else second",
    199
)]
#[case::constant("condition_constant", "value = ", "first if ready else second", 199)]
#[case::clause(
    "condition_clause_delete",
    "value = ",
    "first if ready and enabled else second",
    199
)]
#[case::await_expression("await_remove", "async def f():\n    value = ", "await task", 200)]
#[case::unpack_order("dict_unpack_order_swap", "value = ", "{**first, **second}", 198)]
#[case::unpack_delete("dict_unpack_delete", "value = ", "{**first}", 198)]
#[case::delete("del_remove", "del ", "value", 199)]
#[case::format_spec("fstring_format_spec_remove", "value = ", "f'{value:{width}}'", 197)]
fn selected_operator_builds_delimiter_admission_index(
    #[case] operator: &str,
    #[case] prefix: &str,
    #[case] expression: &str,
    #[case] boundary: usize,
    #[values(false, true)] at_boundary: bool,
) {
    // Arrange
    let depth = if at_boundary { boundary } else { 0 };
    let source = format!(
        "{prefix}{}{expression}{}\n",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    let operators = serde_json::from_value(serde_json::json!([operator])).unwrap();

    // Act
    let output = analyze(&source, &operators, 100);

    // Assert
    assert_eq!(output.candidates.is_empty(), at_boundary);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn excluding_an_operator_suppresses_its_candidates(site in 0..SITES.len()) {
        // Arrange
        let (operator, source) = SITES[site];
        let mut operators = MutationOperatorSelection::default();
        operators.exclude(operator);

        // Act
        let output = analyze(source, &operators, 100);

        // Assert
        prop_assert!(output.candidates.iter().all(|candidate| candidate.operator != operator));
    }

    #[test]
    fn reference_candidate_prefix_obeys_limit(site in 0..SITES.len(), limit in 0usize..5) {
        // Arrange
        let (operator, source) = SITES[site];
        let mut operators = MutationOperatorSelection::default();
        for selector in MutationOperatorSelection::selectors() {
            if let MutationSelector::Operator(known) = selector {
                operators.exclude(known);
            }
        }
        operators.include(operator);
        let complete = analyze(source, &operators, 100);

        // Act
        let bounded = analyze(source, &operators, limit);

        // Assert
        prop_assert!(bounded.candidates.len() <= limit);
        prop_assert_eq!(&bounded.candidates, &complete.candidates[..complete.candidates.len().min(limit)]);
    }
}
