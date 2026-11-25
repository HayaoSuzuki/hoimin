#[path = "support/budget_model.rs"]
mod model;

use model::{Command, check_history};
use proptest::prelude::*;
use rstest::rstest;

#[rstest]
#[case::failed_reserve_preserves_id(&[Command::Reserve(0, 3), Command::Reserve(0, 2)], [2, 0, 0])]
#[case::double_release(&[Command::Reserve(0, 2), Command::Release(0), Command::Release(0)], [0, 0, 0])]
#[case::unknown_release(&[Command::Release(99), Command::Reserve(1, 2)], [0, 2, 0])]
#[case::zero_reservation_has_identity(&[Command::Reserve(0, 0), Command::Release(0)], [0, 0, 0])]
#[case::independent_kinds(&[Command::Reserve(0, 2), Command::Reserve(1, 2), Command::Reserve(2, 2), Command::Release(1)], [2, 0, 2])]
fn literal_history(#[case] commands: &[Command], #[case] expected: [u64; 3]) {
    // Arrange: one operation history per parameterized case.
    let limits = [2; 3];
    // Act: every transition is compared with the independent journal model.
    let totals = check_history(limits, commands);
    // Assert.
    assert_eq!(totals, expected);
}

fn amount() -> impl Strategy<Value = u64> {
    prop_oneof![0..=4u64, Just(u64::MAX), Just(u64::MAX - 1), any::<u64>()]
}

fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        (0..3u8, amount()).prop_map(|(kind, n)| Command::Reserve(kind, n)),
        prop_oneof![0..16u64, Just(u64::MAX)].prop_map(Command::Release),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn generated_history_matches_journal(
        limits in [amount(), amount(), amount()],
        commands in prop::collection::vec(command(), 0..128),
    ) {
        // Arrange: proptest supplies one generated history and its limits.
        // Act.
        let totals = check_history(limits, &commands);
        // Assert: also check the public aggregate against each original limit.
        prop_assert!(totals[0] <= limits[0] && totals[1] <= limits[1] && totals[2] <= limits[2]);
    }
}

#[test]
fn fuzz_bytes_preserve_full_width_values() {
    // Arrange.
    let input = [0, 2, 255, 255, 255, 255, 255, 255, 255, 255];
    // Act.
    let totals = check_history([u64::MAX; 3], &model::decode(&input));
    // Assert.
    assert_eq!(totals, [0, 0, u64::MAX]);
}
