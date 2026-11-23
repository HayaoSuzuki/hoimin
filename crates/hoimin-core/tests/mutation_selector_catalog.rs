use hoimin_core::{MutationOperator, MutationOperatorSelection, MutationSelector};
use proptest::prelude::*;
use rstest::rstest;

#[rstest]
#[case::operator(
    "binary_add_sub",
    MutationSelector::Operator(MutationOperator::BinaryAddSub)
)]
#[case::family("exception_ops", MutationSelector::Family {
    name: "exception_ops",
    members: &[MutationOperator::ExceptionTypePair],
})]
fn advertised_selector_retains_its_kind(#[case] name: &str, #[case] expected: MutationSelector) {
    // Arrange
    let catalog = MutationOperatorSelection::selectors();

    // Act
    let actual = catalog
        .into_iter()
        .find(|selector| selector.as_str() == name);

    // Assert
    assert_eq!(actual, Some(expected));
}

#[test]
fn typed_catalog_preserves_unique_alphabetical_advertising() {
    // Arrange
    let catalog = MutationOperatorSelection::selectors();

    // Act
    let names: Vec<_> = catalog.into_iter().map(MutationSelector::as_str).collect();

    // Assert
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(names, MutationOperatorSelection::valid_names());
}

proptest! {
    #[test]
    fn typed_catalog_expansion_matches_the_cli_boundary(
        selector in proptest::sample::select(MutationOperatorSelection::selectors()),
    ) {
        // Arrange
        let expected = match selector {
            MutationSelector::Operator(operator) => vec![operator],
            MutationSelector::Family { members, .. } => members.to_vec(),
        };

        // Act
        let actual = MutationOperatorSelection::parse_selector(selector.as_str()).unwrap();

        // Assert
        prop_assert_eq!(actual, expected);
    }
}
