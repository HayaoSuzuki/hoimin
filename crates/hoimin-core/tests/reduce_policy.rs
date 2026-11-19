use hoimin_core::reduce::{Span, median, pair_pass};

#[rstest::rstest]
#[case(100, 95, true)]
#[case(100, 96, false)]
#[case(1, 1, false)]
#[case(0, 0, false)]
#[case(u128::MAX, u128::MAX - u128::MAX / 10, true)]
fn reduction_threshold_is_exact(
    #[case] original: u128,
    #[case] reduced: u128,
    #[case] expected: bool,
) {
    // Arrange
    let rate = (1, 20);
    // Act
    let actual = pair_pass(rate.0, rate.1, original, reduced);
    // Assert
    assert_eq!(actual, expected);
}

#[rstest::rstest]
#[case([1,3,2], 2)]
#[case([u128::MAX; 3], u128::MAX)]
fn median_does_not_overflow(#[case] ticks: [u128; 3], #[case] expected: u128) {
    // Act
    let actual = median(ticks);
    // Assert
    assert_eq!(actual, expected);
}

proptest::proptest! {
    #[test]
    fn intersection_is_symmetric(a in proptest::array::uniform2(0u64..u64::MAX), b in proptest::array::uniform2(0u64..u64::MAX)) {
        // Arrange
        let left = Span { id: 0, lo: a[0], hi: a[1] };
        let right = Span { id: 1, lo: b[0], hi: b[1] };
        // Act
        let actual = left.overlaps(&right);
        // Assert
        proptest::prop_assert_eq!(actual, right.overlaps(&left));
    }
}

proptest::proptest! {
    #[test]
    fn accepted_reduction_is_strict(original in 1u128..u128::MAX,reduced in 1u128..u128::MAX) {
        // Arrange
        let rate=(1,20);
        // Act
        let accepted=pair_pass(rate.0,rate.1,original,reduced);
        // Assert
        proptest::prop_assert!(!accepted || reduced<original);
    }
    #[test]
    fn complete_result_order_does_not_change_preservation(outcomes in proptest::collection::vec(proptest::bool::ANY,1..32)) {
        // Arrange
        let selected:Vec<_>=(0..outcomes.len() as u64).collect();
        let before:Vec<_>=outcomes.iter().enumerate().map(|(i,killed)|hoimin_core::reduce::ResultEntry {id:i as u64,outcome:[hoimin_core::reduce::Outcome::Survived,hoimin_core::reduce::Outcome::Killed][usize::from(*killed)]}).collect();
        let after:Vec<_>=before.iter().rev().cloned().collect();
        // Act
        let actual=hoimin_core::reduce::exact_results(&selected,&before,&after);
        // Assert
        proptest::prop_assert!(actual);
    }
}
