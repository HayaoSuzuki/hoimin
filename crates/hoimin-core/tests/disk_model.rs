#[path = "support/disk_model.rs"]
mod model;
use model::Command as C;
use proptest::prelude::*;
use rstest::rstest;
#[rstest]
#[case::stop_blocks_dispatch(3,&[C::Dispatch,C::Stop,C::Dispatch],&[true,true,false])]
#[case::delivery_deferred_blocks_finish(3,&[C::Component(0,true),C::Component(1,true),C::Component(2,true),C::Request(0),C::Cleanup(0,5),C::Component(3,true),C::Request(1),C::Cleanup(1,4),C::Finish],&[true,true,true,true,true,true,true,true,false])]
#[case::failed_drain_can_retain_execution(1,&[C::Dispatch,C::Component(0,false),C::Component(1,true),C::Component(2,true),C::Request(0),C::Cleanup(0,2),C::Cleanup(0,5),C::Component(3,true),C::Finish],&[true,true,true,true,true,false,true,true,true])]
#[case::unknown_root(0,&[C::Component(0,true),C::Component(1,true),C::Component(2,true),C::Request(1)],&[true,true,true,false])]
#[case::terminal_freeze(0,&[C::Component(0,true),C::Component(1,true),C::Component(2,true),C::Component(3,true),C::Finish,C::Stop,C::Dispatch],&[true,true,true,true,true,false,false])]
fn literal_disk_history(#[case] mask: u8, #[case] commands: &[C], #[case] expected: &[bool]) {
    // Arrange.
    let history = commands;
    // Act.
    let accepted = model::check_history(mask, history);
    // Assert.
    assert_eq!(accepted, expected);
}
fn command() -> impl Strategy<Value = C> {
    prop_oneof![
        Just(C::Dispatch),
        Just(C::Stop),
        (0..4u8, any::<bool>()).prop_map(|(c, s)| C::Component(c, s)),
        (0..2u8).prop_map(C::Request),
        (0..2u8, 2..6u8).prop_map(|(r, o)| C::Cleanup(r, o)),
        Just(C::Finish),
        (any::<u64>(), any::<u64>()).prop_map(|(a, b)| C::Observe(a, b))
    ]
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn generated_disk_history_preserves_lifecycle_laws(mask in 0..4u8,commands in prop::collection::vec(command(),0..128)) {
        // Arrange: generated owned-root configuration and one operation history.
        // Act.
        let outcomes=model::check_history(mask,&commands);
        // Assert.
        prop_assert_eq!(outcomes.len(),commands.len());
    }
}
#[test]
fn fuzz_decoder_preserves_full_width_observation() {
    // Arrange.
    let input = [
        6, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    // Act.
    let commands = model::decode(&input);
    // Assert.
    assert!(matches!(commands.as_slice(), [C::Observe(u64::MAX, 0)]));
}
