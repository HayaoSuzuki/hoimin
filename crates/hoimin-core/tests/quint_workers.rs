#[path = "support/workers_model.rs"]
#[allow(dead_code)] // PBT/fuzz own arbitrary histories and decoding.
mod model;
#[path = "support/quint.rs"]
#[allow(dead_code)] // This model contains lists/records, not ITF sets.
mod quint;
use model::{Command as C, Verdict};
fn replay(session: bool, trace: &str) -> (usize, bool) {
    let history = quint::states(trace);
    let mut driver = model::Driver::new(session, 2);
    for row in &history {
        let action = &row["dispatch"];
        let payload = &action["value"];
        let worker = || u8::try_from(quint::number(&payload["worker"])).unwrap();
        let (command, expected) = match action["tag"].as_str().unwrap() {
            "Init" => {
                driver.assert_model(row);
                continue;
            }
            "Ack" => (
                C::Ack(worker(), quint::number(&payload["outcome"]) == 2),
                Verdict::Accepted,
            ),
            "Drain" => (C::Ack(worker(), false), Verdict::Accepted),
            "Global" => (C::Global, Verdict::Accepted),
            "Stop" => (
                C::Stop(
                    u8::try_from(quint::number(&payload["cause"])).unwrap(),
                    1024,
                ),
                Verdict::Accepted,
            ),
            "Probe" => match payload["kind"].as_str().unwrap() {
                "identity" => (C::Identity(worker(), true), Verdict::Identity),
                "worker" => (C::Worker(worker()), Verdict::Worker),
                "wrong" => (C::Wrong(worker()), Verdict::Wrong),
                op => panic!("unknown probe {op}"),
            },
            op => panic!("unknown dispatch {op}"),
        };
        assert_eq!(driver.apply(command), expected);
        driver.assert_model(row);
    }
    (history.len(), driver.snapshot().mode == 6)
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../formal/quint/traces/workers_0.itf.json"), false)]
#[case::trace_1(include_str!("../../../formal/quint/traces/workers_1.itf.json"), false)]
#[case::trace_2(include_str!("../../../formal/quint/traces/workers_2.itf.json"), false)]
#[case::trace_3(include_str!("../../../formal/quint/traces/workers_3.itf.json"), false)]
#[case::trace_reverse_step_0(include_str!("../../../formal/quint/traces/workers_reverseStep_0.itf.json"), true)]
#[case::trace_completed_stop_step_0(include_str!("../../../formal/quint/traces/workers_completedStopStep_0.itf.json"), true)]
#[case::trace_start_stop_step_0(include_str!("../../../formal/quint/traces/workers_startStopStep_0.itf.json"), true)]
#[case::trace_running_stop_step_0(include_str!("../../../formal/quint/traces/workers_runningStopStep_0.itf.json"), true)]
fn quint_workers_matches_public_protocol(#[case] trace: &str, #[case] end_to_end: bool) {
    // Arrange.
    let session = false;
    // Act: compare each public observation with the model.
    let checked = replay(session, trace);
    // Assert.
    assert!(checked.0 > 1);
    assert!(checked.1 || !end_to_end);
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../formal/quint/traces/workers_session_0.itf.json"), false)]
#[case::trace_1(include_str!("../../../formal/quint/traces/workers_session_1.itf.json"), false)]
#[case::trace_2(include_str!("../../../formal/quint/traces/workers_session_2.itf.json"), false)]
#[case::trace_3(include_str!("../../../formal/quint/traces/workers_session_3.itf.json"), false)]
#[case::trace_reverse_step_0(include_str!("../../../formal/quint/traces/workers_session_reverseStep_0.itf.json"), true)]
#[case::trace_completed_stop_step_0(include_str!("../../../formal/quint/traces/workers_session_completedStopStep_0.itf.json"), true)]
#[case::trace_start_stop_step_0(include_str!("../../../formal/quint/traces/workers_session_startStopStep_0.itf.json"), true)]
#[case::trace_running_stop_step_0(include_str!("../../../formal/quint/traces/workers_session_runningStopStep_0.itf.json"), true)]
fn quint_workers_session_matches_public_protocol(#[case] trace: &str, #[case] end_to_end: bool) {
    // Arrange.
    let session = true;
    // Act: compare each public observation with the model.
    let checked = replay(session, trace);
    // Assert.
    assert!(checked.0 > 1);
    assert!(checked.1 || !end_to_end);
}
