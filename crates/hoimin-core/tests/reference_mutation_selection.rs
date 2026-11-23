use hoimin_core::MutationOperatorSelection;
use rstest::rstest;

#[rstest]
#[case("float_literal_neighbor")]
#[case("bytes_literal_empty")]
#[case("fstring_format_spec_remove")]
#[case("fstring_format_value_neighbor")]
#[case("conditional_branch_swap")]
#[case("await_remove")]
#[case("dict_unpack_order_swap")]
#[case("dict_unpack_delete")]
#[case("del_remove")]
#[case("exception_handler_swap")]
#[case("exception_cause_omit")]
#[case("match_case_swap")]
#[case("match_pattern_wildcard")]
#[case("match_class_position_swap")]
fn reference_mutation_is_enabled_by_default_and_can_be_excluded(#[case] name: &str) {
    // Arrange
    let mut selected = MutationOperatorSelection::default();
    let original = selected.names();

    // Act
    let operators = MutationOperatorSelection::parse_selector(name).unwrap();
    for operator in &operators {
        selected.exclude(*operator);
    }
    let remaining = selected.names();

    // Assert
    assert_eq!(operators.len(), 1);
    assert!(original.iter().any(|item| item == name));
    assert!(!remaining.iter().any(|item| item == name));
    assert_eq!(remaining.len() + 1, original.len());
}
