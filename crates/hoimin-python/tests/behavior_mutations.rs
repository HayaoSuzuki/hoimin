use camino::Utf8Path;
use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationProfile};
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};

#[rstest::rstest]
#[case::augmented_to_binary("augmented_to_binary", "xs += ys\n")]
#[case::assignment_target_swap("assignment_target_swap", "left, right = pair\n")]
#[case::return_value_none("return_value_none", "def f():\n    return compute()\n")]
#[case::identity_equality_swap("identity_equality_swap", "value = left is right\n")]
#[case::else_suite_remove(
    "else_suite_remove",
    "for value in values:\n    work(value)\nelse:\n    done()\n"
)]
#[case::zip_strict_toggle("zip_strict_toggle", "value = zip(first, second)\n")]
#[case::range_bound_neighbor("range_bound_neighbor", "value = range(limit)\n")]
#[case::compare_direction("compare_direction", "value = left < right\n")]
#[case::regex_quantifier_neighbor(
    "regex_quantifier_neighbor",
    "import re\nvalue = re.fullmatch(r'a{2,4}', text)\n"
)]
#[case::regex_greediness_toggle(
    "regex_greediness_toggle",
    "import re\nvalue = re.search(r'a+', text)\n"
)]
#[case::regex_alternative_delete(
    "regex_alternative_delete",
    "import re\nvalue = re.fullmatch(r'(?:one|two)', text)\n"
)]
#[case::fstring_format_option_swap("fstring_format_option_swap", "value = f'{number:>10}'\n")]
#[case::context_exit_omit("context_exit_omit", "with manager as value:\n    work(value)\n")]
#[case::context_exit_result_negate(
    "context_exit_result_negate",
    "class Manager:\n    def __exit__(self, kind, error, trace):\n        return flag\n"
)]
#[case::yield_statement_remove("yield_statement_remove", "def f():\n    yield value\n")]
fn proposed_operator_supports_default_and_opt_out(
    #[case] id: &str,
    #[case] source: &str,
    #[values(true, false)] enabled: bool,
) {
    // Arrange
    let mut operators = MutationOperatorSelection::default();
    if !enabled {
        operators.exclude(MutationOperator::from_name(id).unwrap());
    }
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };

    // Act
    let output = analyze_source_cancellable(&request, source, || false).unwrap();

    // Assert
    assert_eq!(
        output
            .candidates
            .iter()
            .any(|candidate| candidate.operator.as_str() == id),
        enabled,
        "{id}"
    );
}
