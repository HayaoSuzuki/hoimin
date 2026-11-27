//! Replay diagnostics at the production detail cap with a fixed issue prefix.
use super::super::{MAX_DIAGNOSTIC_DETAIL_BYTES, MAX_DIAGNOSTIC_DETAILS};
use super::ReclaimLedger;
use crate::quint_itf::{number, states};
use hoimin_core::{JanitorDetail, JanitorReason};
use serde_json::Value;

const PREFIX: usize = MAX_DIAGNOSTIC_DETAILS - 2;
const MULTIBYTE: &str = "界";

fn detail(input: &Value, truncate: bool) -> JanitorDetail {
    let reason = match input["kind"]["tag"].as_str().unwrap() {
        "Normal" => JanitorReason::Young,
        "Issue" => JanitorReason::DeleteFailed,
        other => panic!("unknown diagnostic kind: {other}"),
    };
    let id = number(&input["id"]);
    let mut message = format!("detail #{id}: ");
    if input["long"].as_bool().unwrap() {
        let fits = (MAX_DIAGNOSTIC_DETAIL_BYTES - message.len()) / MULTIBYTE.len();
        let repetitions = if truncate { fits } else { fits + 1 };
        message.push_str(&MULTIBYTE.repeat(repetitions));
    }
    JanitorDetail { reason, message }
}

fn replay(trace: &str, skip_first_issue: bool) -> Result<(), String> {
    let history = states(trace);
    assert!(history.len() > 1);
    let prefix = JanitorDetail {
        reason: JanitorReason::MarkerInvalid,
        message: "earlier issue".into(),
    };
    let mut ledger = ReclaimLedger::default();
    for _ in 0..PREFIX {
        ledger.detail(prefix.reason, prefix.message.clone());
    }
    for (index, row) in history.iter().enumerate() {
        let arrivals = row["arrivals"].as_array().unwrap();
        assert_eq!(arrivals.len(), index);
        if let Some(input) = arrivals.last() {
            let entry = detail(input, false);
            if !(skip_first_issue && index == 3 && entry.reason == JanitorReason::DeleteFailed) {
                ledger.detail(entry.reason, entry.message);
            }
        }
        let summary = &ledger.report;
        let expected = &row["summary"];
        let kept = expected["retained"]
            .as_array()
            .unwrap()
            .iter()
            .map(|input| detail(input, true))
            .collect::<Vec<_>>();
        let omissions = &summary.detail_omissions;
        let actual_counts = [
            omissions.normal_details,
            omissions.issue_details,
            omissions.truncated_details,
        ];
        let expected_counts =
            ["omittedNormal", "omittedIssue", "truncated"].map(|field| number(&expected[field]));
        let (earlier, observed) = summary
            .details
            .split_at_checked(PREFIX)
            .ok_or_else(|| format!("step {index}: lost earlier issue details"))?;
        if earlier.iter().any(|entry| entry != &prefix)
            || observed != kept
            || actual_counts != expected_counts
        {
            return Err(format!(
                "step {index}: retained {} details, expected {}; counts {actual_counts:?}, expected {expected_counts:?}",
                summary.details.len(),
                PREFIX + kept.len(),
            ));
        }
    }
    Ok(())
}

const PRIORITY: &str = include_str!(
    "../../../../../../formal/quint/traces/janitor_diagnostics_priorityStep_0.itf.json"
);

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_diagnostics_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_diagnostics_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_diagnostics_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_diagnostics_3.itf.json"))]
#[case::priority(PRIORITY)]
#[case::issue_only(include_str!("../../../../../../formal/quint/traces/janitor_diagnostics_issueOnlyStep_0.itf.json"))]
fn quint_history_matches_diagnostic_summary(#[case] trace: &str) {
    assert_eq!(replay(trace, false), Ok(()));
}

#[test]
fn missing_priority_issue_changes_observed_summary() {
    let mismatch = replay(PRIORITY, true).expect_err("priority issue was lost");
    assert!(mismatch.starts_with("step 3"), "{mismatch}");
}
