#[path = "support/stop_model.rs"]
#[allow(dead_code)] // ITF observations and byte decoding are exercised by sibling harnesses.
mod model;
use model::{Command as C, Entry as E};
use proptest::prelude::*;

#[rstest::rstest]
#[case::cancel(0, 130)]
#[case::deadline(1, 4)]
#[case::disk(2, 2)]
fn stopped_bootstrap_drains_in_protocol_order(
    #[case] cause: u8,
    #[case] code: i32,
    #[values(E::Targets, E::Preflight, E::Report, E::Session)] entry: E,
) {
    // Arrange.
    let commands = [
        C::Retired,
        C::Unknown,
        C::Wrong,
        C::Ack,
        C::Ack,
        C::Reservation,
        C::Ack,
        C::Ack,
        C::Ack,
        C::Ack,
        C::Ack,
        C::Stop(0, u64::MAX),
    ];
    // Act.
    let outcome = model::check_history(entry, cause, &commands);
    // Assert.
    assert_eq!(outcome, (code, true));
}
#[rstest::rstest]
#[case::preserve_disk_diagnostic(2,&[C::Stop(1,0),C::Stop(2,u64::MAX),C::Ack,C::Ack,C::Ack,C::Ack,C::Ack,C::Ack],2)]
#[case::cancel_overrides_disk(2,&[C::Stop(0,0),C::Ack,C::Ack,C::Ack,C::Ack,C::Ack,C::Ack],130)]
#[case::start_delivery_failure(1,&[C::Fail(true),C::Ack,C::Ack],2)]
#[case::verification_failure(1,&[C::Ack,C::Fail(false),C::Ack,C::Ack,C::Ack,C::Ack],2)]
#[case::cleanup_failure(1,&[C::Ack,C::Ack,C::Fail(false),C::Fail(true),C::Ack,C::Ack],2)]
#[case::final_report_freezes_cause(1,&[C::Ack,C::Ack,C::Ack,C::Stop(0,0),C::Ack,C::Stop(0,0),C::Ack],4)]
fn literal_stop_history_checks_failure_and_freeze_rules(
    #[case] cause: u8,
    #[case] commands: &[C],
    #[case] code: i32,
) {
    // Arrange.
    let entry = E::Session;
    // Act.
    let outcome = model::check_history(entry, cause, commands);
    // Assert.
    assert_eq!(outcome, (code, true));
}
fn command() -> impl Strategy<Value = C> {
    prop_oneof![
        (0..3u8, any::<u64>()).prop_map(|(cause, bytes)| C::Stop(cause, bytes)),
        Just(C::Ack),
        any::<bool>().prop_map(C::Fail),
        Just(C::Retired),
        Just(C::Duplicate),
        Just(C::Unknown),
        Just(C::Wrong),
        Just(C::Reservation)
    ]
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn generated_public_stop_history_preserves_protocol_laws(entry in 0..4u8,cause in 0..3u8,commands in prop::collection::vec(command(),0..128)) {
        // Arrange.
        let start=E::decode(entry);
        // Act: the shared driver checks the public protocol after each operation.
        let (code,finished)=model::check_history(start,cause,&commands);
        // Assert.
        prop_assert!(matches!(code,2|4|130));
        prop_assert!(finished);
    }
}
#[test]
fn fuzz_decoder_preserves_full_width_disk_observation() {
    // Arrange.
    let input = [3, 2, 2, 255, 255, 255, 255, 255, 255, 255, 255];
    // Act.
    let (_, _, commands) = model::decode(&input);
    // Assert.
    assert!(matches!(commands.as_slice(), [C::Stop(2, u64::MAX)]));
}

#[rstest::rstest]
#[case::cancel(0, 130)]
#[case::deadline(1, 2)]
#[case::disk(2, 2)]
fn failed_final_report_after_unacknowledged_start_does_not_restart(
    #[case] cause: u8,
    #[case] code: i32,
    #[values(E::Targets, E::Preflight, E::Report, E::Session)] entry: E,
) {
    // Arrange: run-start fails, cleanup succeeds, then final output fails.
    let commands = [C::Fail(false), C::Ack, C::Fail(true)];
    // Act.
    let outcome = model::check_history(entry, cause, &commands);
    // Assert.
    assert_eq!(outcome, (code, true));
}
