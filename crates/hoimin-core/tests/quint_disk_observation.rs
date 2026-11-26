#[path = "support/disk_observation_model.rs"]
mod model;
#[path = "support/quint.rs"]
#[allow(dead_code)]
mod quint;
use proptest::prelude::*;
use rstest::rstest;
fn replay(trace: &str) -> Result<(), String> {
    let mut actual = model::Driver::default();
    for state in quint::states(trace) {
        actual.apply(model::command(&state["dispatch"]));
        let expected = model::Snapshot::from_spec(&state["s"]);
        let actual = actual.snapshot();
        if actual != expected {
            return Err(format!("actual {actual:?}, expected {expected:?}"));
        }
    }
    Ok(())
}
#[rstest]
#[case::random_0(include_str!("../../../formal/quint/traces/disk_observation_0.itf.json"))]
#[case::random_1(include_str!("../../../formal/quint/traces/disk_observation_1.itf.json"))]
#[case::random_2(include_str!("../../../formal/quint/traces/disk_observation_2.itf.json"))]
#[case::random_3(include_str!("../../../formal/quint/traces/disk_observation_3.itf.json"))]
#[case::priority(include_str!("../../../formal/quint/traces/disk_observation_priorityStep_0.itf.json"))]
#[case::deduplicate(include_str!("../../../formal/quint/traces/disk_observation_duplicateStep_0.itf.json"))]
#[case::failure(include_str!("../../../formal/quint/traces/disk_observation_failureStep_0.itf.json"))]
fn disk_evidence_matches_fixed_oracle(#[case] trace: &str) {
    // Arrange.
    let oracle = trace;
    // Act.
    let result = replay(oracle);
    // Assert.
    assert_eq!(result, Ok(()));
}

fn check_bytes(bytes: &[u8]) -> model::Snapshot {
    let mut driver = model::Driver::default();
    for command in model::decode(bytes) {
        driver.apply(command);
    }
    driver.snapshot()
}
proptest! {
    #![proptest_config(ProptestConfig { cases:256, ..ProptestConfig::default() })]
    #[test]
    fn arbitrary_evidence_preserves_primary_and_deduplicates(bytes in prop::collection::vec(any::<u8>(),0..1600)) {
        // Arrange: byte records retain full-width owned/free/duration values.
        let input=bytes;
        // Act.
        let actual=check_bytes(&input);
        // Assert: intermediate states are also checked by the driver.
        prop_assert!(actual.evidence_is_unique());
    }
}
