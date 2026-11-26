//! Replay recorded janitor observations through the production report ledger.
use super::{DeletionProgress, ReclaimLedger};
use crate::quint_itf::{number, states};
use hoimin_core::{JanitorReason, JanitorSummary};
use serde_json::Value;

type Counts = [u64; 11];

fn apply(ledger: &mut ReclaimLedger, command: &Value, skip_acknowledgement: bool) {
    let input = &command["value"];
    match command["tag"].as_str().unwrap() {
        "Init" => {}
        "Inspect" => assert_eq!(ledger.begin(), usize::try_from(number(input)).unwrap()),
        "Hold" => ledger.hold(
            usize::try_from(number(input)).unwrap(),
            JanitorReason::Young,
            "young root".into(),
        ),
        "Fail" => ledger.hold(
            usize::try_from(number(input)).unwrap(),
            JanitorReason::DeleteFailed,
            "removal failed".into(),
        ),
        "NoticeAbsence" => ledger.absent(usize::try_from(number(input)).unwrap()),
        "Remove" => {
            let acknowledged = input["deleted"].as_bool().unwrap();
            ledger.removal(
                usize::try_from(number(&input["root"])).unwrap(),
                DeletionProgress {
                    examined_entries: number(&input["examined"]),
                    removed_entries: number(&input["removed"]),
                    root_removed: acknowledged && !skip_acknowledgement,
                },
                input["absent"].as_bool().unwrap(),
            );
        }
        other => panic!("unknown reclaim operation: {other}"),
    }
}

fn actual_counts(actual: &JanitorSummary) -> Counts {
    [
        actual.inspected_roots,
        actual.reclaimed_roots,
        actual.absent_roots,
        actual.preserved_roots,
        actual.unresolved_roots,
        actual
            .unresolved_by_reason
            .get(&JanitorReason::DeleteFailed)
            .copied()
            .unwrap_or_default(),
        actual
            .unresolved_by_reason
            .get(&JanitorReason::CleanupIncomplete)
            .copied()
            .unwrap_or_default(),
        actual.examined_entries,
        actual.removed_entries,
        actual.scan.uninspected_selected_roots,
        actual.scan.selection_omitted_observed_entries,
    ]
}

fn expected_counts(expected: &Value) -> Counts {
    [
        "inspected",
        "reclaimed",
        "absent",
        "preserved",
        "unresolved",
        "deleteFailed",
        "cleanupIncomplete",
        "examined",
        "removed",
        "uninspected",
        "omitted",
    ]
    .map(|field| number(&expected[field]))
}

fn replay(trace: &str, skip_acknowledgement: bool) -> Result<(), String> {
    let history = states(trace);
    assert!(history.len() > 1);
    for (index, row) in history.iter().enumerate() {
        // Finishing consumes the ledger. Replay each bounded prefix to observe its real summary.
        let mut ledger = ReclaimLedger::default();
        ledger.report.scan.observed_matching_entries = 3;
        ledger.report.scan.selected_roots = 2;
        for step in &history[..=index] {
            apply(&mut ledger, &step["command"], skip_acknowledgement);
        }
        let actual = ledger.finish();
        let expected = expected_counts(&row["summary"]);
        if actual_counts(&actual) != expected {
            return Err(format!(
                "step {index} {}: got {:?}, expected {expected:?}",
                row["command"],
                actual_counts(&actual)
            ));
        }
    }
    Ok(())
}

const ACKNOWLEDGED: &str = include_str!(
    "../../../../../../formal/quint/traces/janitor_reclaim_acknowledgedStep_0.itf.json"
);

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_3.itf.json"))]
#[case::acknowledged(ACKNOWLEDGED)]
#[case::partial(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_partialStep_0.itf.json"))]
fn quint_history_matches_reclaim_summary(#[case] trace: &str) {
    assert_eq!(replay(trace, false), Ok(()));
}

#[test]
fn missing_deletion_acknowledgement_changes_observed_summary() {
    let mismatch = replay(ACKNOWLEDGED, true).expect_err("deletion acknowledgement was lost");
    assert!(mismatch.starts_with("step 3"), "{mismatch}");
}
