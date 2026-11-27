//! Compare janitor observations from the ledger and real selected directories.
use super::super::{
    COORDINATOR_FILE, DELETING_PREFIX, JANITOR_CLEANUP_BUDGET, ManagedRootCoordinator,
    ManagedRunRoot, OwnerKind, STAGING_PREFIX, STALE_AFTER, TEST_CLEANUP_SLICE_ENTRIES,
    create_owned_directory, read_coordinator_state, select_reclaim_candidates,
};
use super::{DeletionProgress, ReclaimLedger};
use crate::quint_itf::{number, states};
use camino::Utf8Path;
use hoimin_core::{JanitorReason, JanitorScan, JanitorSummary};
use serde_json::Value;
use std::time::{Duration, Instant, SystemTime};

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
const SELECTED: &str = include_str!(
    "../../../../../../formal/quint/traces/janitor_reclaim_selectedRootsStep_0.itf.json"
);
const DEFERRED: &str = include_str!(
    "../../../../../../formal/quint/traces/janitor_reclaim_deferredReportStep_0.itf.json"
);
const SHELL: &str = include_str!(
    "../../../../../../formal/quint/traces/janitor_reclaim_shellReportStep_0.itf.json"
);

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_3.itf.json"))]
#[case::acknowledged(ACKNOWLEDGED)]
#[case::partial(include_str!("../../../../../../formal/quint/traces/janitor_reclaim_partialStep_0.itf.json"))]
#[case::selected(SELECTED)]
#[case::shell(SHELL)]
#[case::deferred(DEFERRED)]
fn quint_history_matches_reclaim_summary(#[case] trace: &str) {
    assert_eq!(replay(trace, false), Ok(()));
}

#[test]
fn missing_deletion_acknowledgement_changes_observed_summary() {
    let mismatch = replay(ACKNOWLEDGED, true).expect_err("deletion acknowledgement was lost");
    assert!(mismatch.starts_with("step 3"), "{mismatch}");
}

fn seed_cursor(coordinator: &ManagedRootCoordinator, name: &str) {
    drop(create_owned_directory(&coordinator.dir, name).unwrap());
    let (seed, error) =
        select_reclaim_candidates(coordinator, &mut JanitorScan::default()).unwrap();
    assert_eq!(seed, [name.to_owned()]);
    assert!(error.is_none());
}

#[rstest::rstest]
#[case::deadline(true)]
#[case::vanished(false)]
fn quint_selected_roots_reach_the_report(#[case] stop_after_first: bool) {
    let directory = tempfile::tempdir().unwrap();
    let coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(directory.path()).unwrap()).unwrap();
    let names = [1_u128, 2, 3].map(|id| format!("{DELETING_PREFIX}{}", uuid::Uuid::from_u128(id)));
    let [behind_cursor, first, second] = &names;

    // Start the live scan after the first matching name.
    seed_cursor(&coordinator, behind_cursor);
    drop(create_owned_directory(&coordinator.dir, first).unwrap());
    drop(create_owned_directory(&coordinator.dir, second).unwrap());

    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        &coordinator,
        SystemTime::now(),
        Instant::now(),
        &|_| {
            if stop_after_first && !coordinator.dir.exists(first) {
                JANITOR_CLEANUP_BUDGET
            } else {
                Duration::ZERO
            }
        },
        &|scan| {
            let selection = select_reclaim_candidates(&coordinator, scan).unwrap();
            assert_eq!(selection.0, names[1..]);
            assert!(selection.1.is_none());
            if !stop_after_first {
                coordinator.dir.remove_dir(second).unwrap();
            }
            Ok(selection)
        },
    );
    let history = states(SELECTED);
    let terminal_command = if stop_after_first {
        "Remove"
    } else {
        "NoticeAbsence"
    };
    let expected = history
        .iter()
        .find(|row| row["command"]["tag"] == terminal_command)
        .unwrap();
    assert_eq!(
        actual_counts(&report),
        expected_counts(&expected["summary"])
    );
    assert_eq!(
        report.scan.stop_reason,
        stop_after_first.then_some(JanitorReason::CleanupIncomplete)
    );
    assert!(report.scan.directory_scan_complete);
    assert!(report.scan.cursor_deferred);
    assert_eq!(report.details, Vec::new());
    assert!(coordinator.dir.exists(behind_cursor));
    assert!(!coordinator.dir.exists(first));
    assert_eq!(coordinator.dir.exists(second), stop_after_first);
}

#[test]
fn quint_deferred_cleanup_progress_reaches_the_report() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(directory.path()).unwrap()).unwrap();
    let behind = format!("{DELETING_PREFIX}{}", uuid::Uuid::from_u128(1));
    seed_cursor(&coordinator, &behind);
    let [first, second] =
        [(); 2].map(|()| ManagedRunRoot::create(&coordinator, OwnerKind::PublicExecution).unwrap());
    let (small, deep) = if first.path() < second.path() {
        (first, second)
    } else {
        (second, first)
    };
    small
        .dir
        .write("payload", b"admitted for deletion")
        .unwrap();
    {
        let branch = create_owned_directory(&deep.dir, "branch").unwrap();
        let nested = create_owned_directory(&branch, "nested").unwrap();
        let leaf = create_owned_directory(&nested, "leaf").unwrap();
        leaf.write("payload", b"beyond the traversal cap").unwrap();
    }
    let small_path = small.path().to_owned();
    let deep_path = deep.path().to_owned();
    let deep_deleting = coordinator
        .path
        .join(format!("{DELETING_PREFIX}{}", deep.run_id));
    drop(small);
    drop(deep);
    let now = SystemTime::now() + STALE_AFTER;
    let report = TEST_CLEANUP_SLICE_ENTRIES.with(|cap| {
        let previous = cap.replace(3);
        let report = ManagedRunRoot::reclaim_abandoned(&coordinator, now);
        cap.set(previous);
        report
    });
    let history = states(DEFERRED);
    let terminal = history.last().unwrap();
    assert_eq!(terminal["command"]["tag"], "Remove");
    assert_eq!(
        actual_counts(&report),
        expected_counts(&terminal["summary"])
    );
    assert_eq!(
        report
            .details
            .iter()
            .map(|detail| detail.reason)
            .collect::<Vec<_>>(),
        [JanitorReason::CleanupIncomplete],
    );
    assert!(report.scan.directory_scan_complete);
    assert_eq!(report.scan.stop_reason, None);
    assert!(coordinator.dir.exists(behind));
    assert!(!small_path.exists());
    assert!(!deep_path.exists());
    assert_eq!(
        std::fs::read(deep_deleting.join("branch/nested/leaf/payload")).unwrap(),
        b"beyond the traversal cap",
    );
}

#[test]
fn quint_mixed_namespaces_wrap_after_reopening_the_coordinator() {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let behind = format!("{DELETING_PREFIX}{}", uuid::Uuid::from_u128(1));
    let deleting = format!("{DELETING_PREFIX}{}", uuid::Uuid::from_u128(2));
    let staging = format!("{STAGING_PREFIX}{}", uuid::Uuid::from_u128(3));
    seed_cursor(&coordinator, &behind);
    drop(create_owned_directory(&coordinator.dir, &deleting).unwrap());
    let stage = create_owned_directory(&coordinator.dir, &staging).unwrap();
    let threshold = stage.metadata(".").unwrap().modified().unwrap().into_std() + STALE_AFTER;
    drop(stage);

    let first =
        ManagedRunRoot::reclaim_abandoned(&coordinator, threshold - Duration::from_nanos(1));
    let first_trace = states(SHELL);
    let expected = &first_trace.last().unwrap()["summary"];
    assert_eq!(actual_counts(&first), expected_counts(expected));
    assert_eq!(first.details.len(), 1);
    assert_eq!(first.details[0].reason, JanitorReason::Young);
    assert!(first.scan.cursor_deferred);
    assert!(coordinator.dir.exists(&behind));
    assert!(!coordinator.dir.exists(&deleting));
    assert!(coordinator.dir.exists(&staging));
    drop(coordinator);

    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let path = coordinator.path.join(COORDINATOR_FILE);
    let (_, saved) = read_coordinator_state(&coordinator.file, &path).unwrap();
    assert_eq!(saved.cursor, staging);
    assert_eq!(saved.generation, 2);
    let second = ManagedRunRoot::reclaim_abandoned(&coordinator, threshold);

    let namespace_trace = states(include_str!(
        "../../../../../../formal/quint/traces/janitor_namespace_recovery_unmarkedStep_0.itf.json"
    ));
    let reports: Vec<_> = namespace_trace
        .iter()
        .skip(1)
        .map(|row| &row["s"]["last"]["value"]["report"])
        .filter(|report| report["outcome"]["tag"] == "Reclaimed")
        .collect();
    let reclaimed = u64::try_from(reports.len()).unwrap();
    assert_eq!(reclaimed, 2);
    assert_eq!(second.inspected_roots, reclaimed);
    assert_eq!(second.reclaimed_roots, reclaimed);
    assert_eq!(
        second.removed_entries,
        reports
            .iter()
            .map(|report| number(&report["removed"]))
            .sum::<u64>()
    );
    assert_eq!(second.scan.selected_roots, 2);
    assert_eq!(second.scan.uninspected_selected_roots, 0);
    assert_eq!(second.scan.selection_omitted_observed_entries, 0);
    assert!(second.scan.directory_scan_complete);
    assert!(!second.scan.cursor_deferred);
    assert_eq!(second.details, Vec::new());
    assert!(!coordinator.dir.exists(&behind));
    assert!(!coordinator.dir.exists(&staging));
    let (_, advanced) = read_coordinator_state(&coordinator.file, &path).unwrap();
    assert_eq!(advanced.cursor, staging);
    assert_eq!(advanced.generation, 3);
}
