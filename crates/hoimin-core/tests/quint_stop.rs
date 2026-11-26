#[path = "support/stop_model.rs"]
#[allow(dead_code)] // PBT/fuzz own byte decoding and arbitrary command histories.
mod model;
#[path = "support/quint.rs"]
#[allow(dead_code)] // This model contains records rather than ITF sets.
mod quint;
use model::{Command as C, Driver, Entry as E, Verdict};
fn replay(entry: E, trace: &str) -> usize {
    let history = quint::states(trace);
    let mut driver = Driver::new(entry);
    for state in &history {
        let arg = u8::try_from(quint::number(&state["arg"])).unwrap();
        let (command, expected) = match state["op"].as_str().unwrap() {
            "init" => {
                driver.assert_model(state);
                continue;
            }
            "stop" => (
                match arg {
                    0..=2 => C::Stop(arg, 1024),
                    3 => C::DiskMeasurementFailed,
                    4 => C::Stop(2, 0),
                    other => panic!("unknown model stop input {other}"),
                },
                Verdict::Accepted,
            ),
            "ack" => (C::Ack, Verdict::Accepted),
            "fail" => (C::Fail(arg == 1), Verdict::Accepted),
            "retired" => (C::Retired, Verdict::Retired),
            "duplicate" => (C::Duplicate, Verdict::Duplicate),
            "unknown" => (C::Unknown, Verdict::Unknown),
            "wrong" => (C::Wrong, Verdict::Wrong),
            "reservation" => (C::Reservation, Verdict::Reservation),
            op => panic!("unknown op {op}"),
        };
        assert_eq!(driver.apply(command), expected, "{command:?}");
        driver.assert_model(state);
    }
    history.len()
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../formal/quint/traces/stop_early_0.itf.json"))]
#[case::trace_1(include_str!("../../../formal/quint/traces/stop_early_1.itf.json"))]
#[case::trace_2(include_str!("../../../formal/quint/traces/stop_early_2.itf.json"))]
#[case::trace_3(include_str!("../../../formal/quint/traces/stop_early_3.itf.json"))]
#[case::trace_clean_step_0(include_str!("../../../formal/quint/traces/stop_early_cleanStep_0.itf.json"))]
#[case::trace_disk_step_0(include_str!("../../../formal/quint/traces/stop_early_diskStep_0.itf.json"))]
#[case::trace_failure_step_0(include_str!("../../../formal/quint/traces/stop_early_failureStep_0.itf.json"))]
#[case::terminal_stop(include_str!("../../../formal/quint/traces/stop_early_terminalStep_0.itf.json"))]
#[case::unacknowledged_start_final_failure(include_str!("../../../formal/quint/traces/stop_early_finalFailureStep_0.itf.json"))]
fn quint_stop_early_matches_public_transition(
    #[values(E::Targets, E::Preflight)] entry: E,
    #[case] trace: &str,
) {
    // Arrange.
    let history = trace;
    // Act: compare each public outcome with the finite model.
    let checked = replay(entry, history);
    // Assert.
    assert!(checked > 1);
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../formal/quint/traces/stop_copy_0.itf.json"))]
#[case::trace_1(include_str!("../../../formal/quint/traces/stop_copy_1.itf.json"))]
#[case::trace_2(include_str!("../../../formal/quint/traces/stop_copy_2.itf.json"))]
#[case::trace_3(include_str!("../../../formal/quint/traces/stop_copy_3.itf.json"))]
#[case::trace_clean_step_0(include_str!("../../../formal/quint/traces/stop_copy_cleanStep_0.itf.json"))]
#[case::trace_disk_step_0(include_str!("../../../formal/quint/traces/stop_copy_diskStep_0.itf.json"))]
#[case::trace_failure_step_0(include_str!("../../../formal/quint/traces/stop_copy_failureStep_0.itf.json"))]
#[case::terminal_stop(include_str!("../../../formal/quint/traces/stop_copy_terminalStep_0.itf.json"))]
#[case::unacknowledged_start_final_failure(include_str!("../../../formal/quint/traces/stop_copy_finalFailureStep_0.itf.json"))]
fn quint_stop_copy_matches_public_transition(#[case] trace: &str) {
    // Arrange.
    let entry = E::Report;
    let history = trace;
    // Act: compare each public outcome with the finite model.
    let checked = replay(entry, history);
    // Assert.
    assert!(checked > 1);
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../formal/quint/traces/stop_session_0.itf.json"))]
#[case::trace_1(include_str!("../../../formal/quint/traces/stop_session_1.itf.json"))]
#[case::trace_2(include_str!("../../../formal/quint/traces/stop_session_2.itf.json"))]
#[case::trace_3(include_str!("../../../formal/quint/traces/stop_session_3.itf.json"))]
#[case::trace_clean_step_0(include_str!("../../../formal/quint/traces/stop_session_cleanStep_0.itf.json"))]
#[case::trace_disk_step_0(include_str!("../../../formal/quint/traces/stop_session_diskStep_0.itf.json"))]
#[case::trace_failure_step_0(include_str!("../../../formal/quint/traces/stop_session_failureStep_0.itf.json"))]
#[case::terminal_stop(include_str!("../../../formal/quint/traces/stop_session_terminalStep_0.itf.json"))]
#[case::unacknowledged_start_final_failure(include_str!("../../../formal/quint/traces/stop_session_finalFailureStep_0.itf.json"))]
#[case::reserve_evidence(include_str!("../../../formal/quint/traces/stop_session_reserveEvidenceStep_0.itf.json"))]
#[case::measurement_evidence(include_str!("../../../formal/quint/traces/stop_session_measurementEvidenceStep_0.itf.json"))]
#[case::ordered_diagnostics(include_str!("../../../formal/quint/traces/stop_session_diagnosticEvidenceStep_0.itf.json"))]
fn quint_stop_session_matches_public_transition(#[case] trace: &str) {
    // Arrange.
    let entry = E::Session;
    let history = trace;
    // Act: compare each public outcome with the finite model.
    let checked = replay(entry, history);
    // Assert.
    assert!(checked > 1);
}
