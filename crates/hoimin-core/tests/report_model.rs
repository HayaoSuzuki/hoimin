#[path = "support/report_model.rs"]
mod model;
use model::{Command as C, Kind as K};
use proptest::prelude::*;
use rstest::rstest;

#[rstest]
#[case::active_finish(&[C::new(K::Start,0,0,0),C::new(K::MutantStart,1,0,0),C::new(K::Finish,2,0,0),C::new(K::MutantFinish,2,0,0),C::new(K::Finish,3,0,0)], &["ok","ok","unfinished","ok","ok"])]
#[case::interleaved(&[C::new(K::Start,0,0,0),C::new(K::MutantStart,1,0,0),C::new(K::MutantStart,2,1,1),C::new(K::MutantFinish,3,1,1),C::new(K::MutantFinish,4,0,0),C::new(K::Finish,5,0,0)], &["ok","ok","ok","ok","ok","ok"])]
#[case::stable_identity(&[C::new(K::Start,0,0,0),C::new(K::MutantStart,1,0,0),C::new(K::MutantFinish,2,0,0),C::new(K::MutantStart,3,0,0),C::new(K::MutantStart,4,0,1)], &["ok","ok","ok","identity_duplicate","identity_sequence"])]
#[case::priority(&[C::new(K::Start,0,0,0),C::new(K::Start,0,0,0),C::new(K::MutantFinish,0,1,0),C::new(K::Finish,1,0,0),C::new(K::Diagnostic,1,0,0)], &["ok","started","not_active","ok","finished"])]
#[case::full_counter(&[C::new(K::Start,u64::MAX,0,0),C::new(K::Diagnostic,u64::MAX,0,0),C::new(K::Diagnostic,0,0,0)], &["ok","sequence","sequence"])]
fn literal_report_history(#[case] commands: &[C], #[case] expected: &[&str]) {
    // Arrange: one event history per case.
    let history = commands;
    // Act.
    let actual = model::check_history(history);
    // Assert.
    assert_eq!(actual, expected);
}

fn counter() -> impl Strategy<Value = u64> {
    prop_oneof![8 => 0..32u64, 1 => Just(u64::MAX), 1 => any::<u64>()]
}
fn command() -> impl Strategy<Value = C> {
    (
        0..5usize,
        0..2u8,
        0..2u8,
        counter(),
        prop_oneof![8 => 0..2u64,1 => any::<u64>()],
    )
        .prop_map(|(kind, run, mutant, sequence, identity)| C {
            kind: [
                K::Start,
                K::MutantStart,
                K::MutantFinish,
                K::Diagnostic,
                K::Finish,
            ][kind],
            run,
            mutant,
            identity,
            sequence,
        })
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn generated_report_history_matches_journal(commands in prop::collection::vec(command(),0..128)) {
        // Arrange: one generated history, with independent identities and event counters.
        // Act.
        let results = model::check_history(&commands);
        // Assert.
        prop_assert_eq!(results.len(), commands.len());
    }
}
#[test]
fn fuzz_decoder_preserves_full_counter() {
    // Arrange.
    let bytes = [
        0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    // Act.
    let commands = model::decode(&bytes);
    // Assert.
    assert_eq!(commands[0].sequence, u64::MAX);
}
