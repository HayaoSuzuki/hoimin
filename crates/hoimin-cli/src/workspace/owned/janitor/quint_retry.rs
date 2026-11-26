//! Compare production retry order with model histories of injected slice results.
use super::retry_deferred;
use crate::quint_itf::{number, states};
use serde_json::Value;
use std::cell::Cell;
use std::collections::VecDeque;

fn roots(value: &Value) -> impl Iterator<Item = u64> {
    value.as_array().unwrap().iter().map(number)
}

fn replay(trace: &str) {
    let history = states(trace);
    let initial = &history.first().unwrap()["s"];
    let terminal = &history.last().unwrap()["s"];
    assert_eq!(
        terminal["phase"]["tag"], "Halted",
        "trace must reach a stop"
    );
    let deadline = terminal["phase"]["value"]["tag"] == "Deadline";
    let visits = terminal["visits"].as_array().unwrap();
    let mut pending: VecDeque<_> = roots(&initial["pending"]).collect();
    let attempted = Cell::new(0);
    let mut actual_order = Vec::new();

    retry_deferred(
        &mut pending,
        || !deadline || attempted.get() < visits.len(),
        |root| {
            let index = attempted.get();
            attempted.set(index + 1);
            actual_order.push(*root);
            let result = &visits.get(index)?["result"];
            match result["tag"].as_str().unwrap() {
                "Deferred" => Some(number(&result["value"])),
                "Complete" | "Failed" => None,
                other => panic!("unknown cleanup result {other}"),
            }
        },
    );

    let expected_order: Vec<_> = visits.iter().map(|visit| number(&visit["root"])).collect();
    let expected_pending: VecDeque<_> = roots(&terminal["pending"]).collect();
    assert_eq!(actual_order, expected_order, "visited roots");
    assert_eq!(pending, expected_pending, "pending roots at stop");
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_retry_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_retry_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_retry_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_retry_3.itf.json"))]
#[case::stalled(include_str!("../../../../../../formal/quint/traces/janitor_retry_stalledStep_0.itf.json"))]
#[case::progress(include_str!("../../../../../../formal/quint/traces/janitor_retry_progressStep_0.itf.json"))]
#[case::deadline(include_str!("../../../../../../formal/quint/traces/janitor_retry_deadlineStep_0.itf.json"))]
fn quint_history_matches_deferred_retry_scheduler(#[case] trace: &str) {
    replay(trace);
}
