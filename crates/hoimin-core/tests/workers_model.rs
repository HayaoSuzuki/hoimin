#[path = "support/workers_model.rs"]
#[allow(dead_code)] // ITF observation and byte decoding have sibling harnesses.
mod model;
use model::Command as C;
use proptest::prelude::*;

#[rstest::rstest]
#[case::reverse_finish(&[C::Ack(1,true),C::Ack(1,true),C::Ack(1,true),C::Ack(1,true),C::Ack(1,true),C::Ack(1,true),C::Ack(1,true),C::Ack(0,false),C::Ack(0,false),C::Ack(0,false)],[1,1,0,0])]
#[case::stop_before_start_ack(&[C::Ack(0,false),C::Stop(1,1024),C::Retired,C::Unknown],[0,0,2,0])]
#[case::stop_running(&[C::Ack(0,false),C::Ack(0,false),C::Ack(1,false),C::Ack(1,false),C::Stop(2,1024),C::Global,C::Retired],[0,0,2,0])]
#[case::stop_after_result(&[C::Ack(1,false),C::Ack(1,false),C::Ack(1,false),C::Stop(0,1024),C::Duplicate],[1,0,1,0])]
#[case::stop_after_persistence(&[C::Ack(1,true),C::Ack(1,true),C::Ack(1,true),C::Ack(1,true),C::Stop(0,1024)],[0,1,1,0])]
#[case::duplicate_stop(&[C::Stop(1,1024),C::Ack(0,false),C::Stop(2,0),C::Global,C::Stop(0,u64::MAX),C::Retired],[0,0,2,0])]
#[case::timeout(&[C::Ack(1,false),C::Ack(1,false),C::Timeout(1)],[1,0,0,1])]
#[case::stop_after_timeout(&[C::Ack(1,false),C::Ack(1,false),C::Timeout(1),C::Stop(0,1024),C::Retired],[0,0,1,1])]
fn literal_parallel_history_preserves_results_and_counts(
    #[values(false, true)] session: bool,
    #[case] commands: &[C],
    #[case] expected: [u64; 4],
) {
    // Arrange.
    let jobs = 2;
    // Act: complete the injected history, then deliver only successful replies.
    let actual = model::check_history(session, jobs, commands);
    // Assert.
    assert_eq!(actual, expected);
}
#[rstest::rstest]
#[case::run_id(true)]
#[case::mutant_id(false)]
fn invalid_persistence_identity_does_not_consume_the_valid_result(#[case] run: bool) {
    // Arrange.
    let commands = [
        C::Ack(0, false),
        C::Ack(0, false),
        C::Ack(0, false),
        C::Identity(0, run),
        C::Worker(0),
        C::Wrong(0),
        C::Ack(0, false),
        C::Duplicate,
    ];
    // Act.
    let actual = model::check_history(true, 2, &commands);
    // Assert.
    assert_eq!(actual, [2, 0, 0, 0]);
}
fn command(jobs: usize) -> impl Strategy<Value = C> {
    prop_oneof![
        (0..u8::try_from(jobs).unwrap(), any::<bool>()).prop_map(|(w, s)| C::Ack(w, s)),
        (0..u8::try_from(jobs).unwrap()).prop_map(C::Timeout),
        Just(C::Global),
        (0..3u8, any::<u64>()).prop_map(|(c, b)| C::Stop(c, b)),
        (0..u8::try_from(jobs).unwrap()).prop_map(C::Wrong),
        (0..u8::try_from(jobs).unwrap()).prop_map(C::Worker),
        (0..u8::try_from(jobs).unwrap(), any::<bool>()).prop_map(|(w, r)| C::Identity(w, r)),
        Just(C::Retired),
        Just(C::Duplicate),
        Just(C::Unknown)
    ]
}
fn history() -> impl Strategy<Value = (usize, Vec<C>)> {
    (1..5usize).prop_flat_map(|jobs| (Just(jobs), prop::collection::vec(command(jobs), 0..96)))
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn generated_parallel_history_has_one_result_per_candidate(session in any::<bool>(),(jobs,commands) in history()) {
        // Arrange: one generated worker configuration and adversarial completion history.
        // Act: the shared journal checks every intermediate state and successful drain.
        let counts=model::check_history(session,jobs,&commands);
        // Assert.
        prop_assert_eq!(counts.into_iter().sum::<u64>(),u64::try_from(jobs).unwrap());
    }
}
#[test]
fn fuzz_decoder_preserves_full_width_disk_reading() {
    // Arrange.
    let bytes = [1, 1, 4, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255];
    // Act.
    let (_, _, commands) = model::decode(&bytes);
    // Assert.
    assert!(matches!(commands.as_slice(), [C::Stop(2, u64::MAX)]));
}

#[test]
fn fuzz_decoder_selects_a_process_timeout() {
    let bytes = [1, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0];
    let (_, _, commands) = model::decode(&bytes);
    assert!(matches!(commands.as_slice(), [C::Timeout(0)]));
}
