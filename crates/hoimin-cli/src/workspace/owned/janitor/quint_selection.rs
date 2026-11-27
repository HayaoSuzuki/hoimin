//! Replay candidate scans against real directory entries and coordinator slots.
use super::super::{
    COORDINATOR_FILE, CoordinatorLockGuard, ManagedRootCoordinator, ManagedRunRoot,
    read_coordinator_state, select_reclaim_candidates,
    select_reclaim_candidates_locked_until_with_hooks,
};
use super::quint_cursor_name as name;
use crate::io_faults;
use crate::quint_itf::{number, set, states};
use camino::Utf8Path;
use hoimin_core::{JanitorReason, JanitorScan};
use serde_json::Value;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::time::{Duration, Instant, SystemTime};

pub(super) fn update_listing(
    coordinator: &ManagedRootCoordinator,
    previous: &mut BTreeSet<u64>,
    listing: &Value,
) {
    let current: BTreeSet<_> = set(listing).iter().map(number).collect();
    for &rank in previous.difference(&current) {
        coordinator.dir.remove_dir(name(rank)).unwrap();
    }
    let additions: Vec<_> = current.difference(previous).copied().collect();
    for rank in additions.into_iter().rev() {
        coordinator.dir.create_dir(name(rank)).unwrap();
    }
    *previous = current;
}

pub(super) fn assert_cursor(coordinator: &ManagedRootCoordinator, expected: &Value) {
    let (_, actual) =
        read_coordinator_state(&coordinator.file, &coordinator.path.join(COORDINATOR_FILE))
            .unwrap();
    assert_eq!(actual.cursor, name(number(&expected["name"])));
    assert_eq!(actual.generation, number(&expected["generation"]));
}

fn expected_page(expected: &Value) -> Vec<String> {
    expected["chosen"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rank| name(number(rank)))
        .collect()
}

pub(super) fn check_scan(coordinator: &ManagedRootCoordinator, expected: &Value) {
    let phase = expected["phase"]["tag"].as_str().unwrap();
    let expire_on = match phase {
        "Published" => None,
        "ExpiredScan" => Some(0),
        "ExpiredEof" => Some(1),
        "ExpiredPersist" => Some(2),
        other => panic!("unexpected terminal phase {other}"),
    };
    let before = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let start = Instant::now();
    let deadline = start + Duration::from_secs(1);
    let after_eof = Cell::new(None);
    let clock = || {
        let mut checkpoint = 0;
        if let Some(checks) = after_eof.get() {
            checkpoint = checks + 1;
            after_eof.set(Some(checkpoint));
        }
        if expire_on.is_some_and(|expiry| checkpoint >= expiry) {
            deadline
        } else {
            start
        }
    };
    let mut scan = JanitorScan::default();
    let guard = CoordinatorLockGuard::acquire(coordinator).unwrap();
    let result = select_reclaim_candidates_locked_until_with_hooks(
        coordinator,
        &mut scan,
        deadline,
        &clock,
        &|| after_eof.set(Some(0)),
        &|| {},
    );
    guard.unlock(&coordinator.path).unwrap();
    assert_eq!(
        scan.directory_scan_complete,
        expected["complete"].as_bool().unwrap()
    );
    assert_eq!(
        scan.cursor_deferred,
        expected["cursorDeferred"].as_bool().unwrap()
    );
    assert_eq!(
        usize::try_from(scan.observed_matching_entries).unwrap(),
        set(&expected["seen"]).len(),
    );
    assert_eq!(
        scan.selection_limited,
        expected["limited"].as_bool().unwrap()
    );
    if let Some(checkpoint) = expire_on {
        let error = result.expect_err("deadline should prevent cursor persistence");
        assert!(
            error.to_string().contains("selection deadline exceeded"),
            "{error}"
        );
        assert_eq!(scan.stop_reason, Some(JanitorReason::SelectionDeadline));
        assert_eq!(after_eof.get().unwrap_or(0), checkpoint);
        assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), before);
    } else {
        let (page, persistence_error) = result.unwrap();
        let expected_page = expected_page(expected);
        assert_eq!(page, expected_page);
        assert_eq!(persistence_error, None);
        assert_eq!(scan.stop_reason, None);
        if expected_page.is_empty() {
            assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), before);
        }
    }
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_selection_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_selection_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_selection_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_selection_3.itf.json"))]
#[case::wrap(include_str!("../../../../../../formal/quint/traces/janitor_selection_wrapStep_0.itf.json"))]
#[case::empty_page(include_str!("../../../../../../formal/quint/traces/janitor_selection_emptyPageStep_0.itf.json"))]
#[case::eof_deadline(include_str!("../../../../../../formal/quint/traces/janitor_selection_eofDeadlineStep_0.itf.json"))]
#[case::persist_deadline(include_str!("../../../../../../formal/quint/traces/janitor_selection_persistDeadlineStep_0.itf.json"))]
#[case::capacity(include_str!("../../../../../../formal/quint/traces/janitor_selection_capacityStep_0.itf.json"))]
#[case::unavailable(include_str!("../../../../../../formal/quint/traces/janitor_selection_unavailableStep_0.itf.json"))]
fn quint_history_matches_candidate_selection(#[case] trace: &str) {
    replay_selection(trace);
}

fn replay_selection(trace: &str) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
    let mut previous = BTreeSet::new();
    let mut finished = 0;
    for row in states(trace) {
        let scan = &row["scan"];
        let phase = scan["phase"]["tag"].as_str().unwrap();
        if !matches!(
            phase,
            "Published"
                | "PersistFailed"
                | "ExpiredScan"
                | "ExpiredEof"
                | "ExpiredPersist"
                | "CoordinatorBusy"
                | "SelectionReadFailed"
        ) {
            continue;
        }
        update_listing(&coordinator, &mut previous, &scan["listing"]);
        assert_cursor(&coordinator, &row["startCursor"]);
        match phase {
            "PersistFailed" => check_cursor_failure_handoff(&coordinator, scan),
            "CoordinatorBusy" | "SelectionReadFailed" => check_unavailable(&coordinator, phase),
            _ => check_scan(&coordinator, scan),
        }
        assert_cursor(&coordinator, &row["saved"]);
        finished += 1;
    }
    assert_eq!(
        finished, 3,
        "trace must include three complete scan outcomes"
    );
}

fn check_unavailable(coordinator: &ManagedRootCoordinator, phase: &str) {
    let before = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let busy = phase == "CoordinatorBusy";
    let guard = busy.then(|| CoordinatorLockGuard::acquire(coordinator).unwrap());
    let fault = (!busy).then(|| io_faults::arm("cursor.read"));
    let report = ManagedRunRoot::reclaim_abandoned_controlled(
        coordinator,
        SystemTime::now(),
        tokio::time::Instant::now() + Duration::from_secs(2),
        &|| false,
    );
    let (reason, details) = if busy {
        (JanitorReason::CoordinatorBusy, vec![])
    } else {
        assert!(fault.unwrap().triggered());
        (
            JanitorReason::SelectionFailed,
            vec![JanitorReason::SelectionFailed],
        )
    };
    if let Some(guard) = guard {
        guard.unlock(&coordinator.path).unwrap();
    }
    assert_eq!(
        report.scan,
        JanitorScan {
            stop_reason: Some(reason),
            ..JanitorScan::default()
        },
    );
    assert_eq!(report.inspected_roots, 0);
    assert_eq!(report.removed_entries, 0);
    assert_eq!(
        report
            .details
            .iter()
            .map(|detail| detail.reason)
            .collect::<Vec<_>>(),
        details,
    );
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), before);
}

fn check_cursor_failure_handoff(coordinator: &ManagedRootCoordinator, expected: &Value) {
    let initial_bytes = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let expected_page = expected_page(expected);
    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        coordinator,
        SystemTime::now(),
        Instant::now(),
        &|_| Duration::ZERO,
        &|scan| {
            let fault = io_faults::arm("cursor.write");
            let (page, error) = select_reclaim_candidates(coordinator, scan).unwrap();
            assert!(fault.triggered());
            assert_eq!(page, expected_page);
            assert!(error.is_some());
            Ok((page, error))
        },
    );
    let selected = u64::try_from(expected_page.len()).unwrap();
    let observed = u64::try_from(set(&expected["seen"]).len()).unwrap();
    assert_eq!(
        report.scan.directory_scan_complete,
        expected["complete"].as_bool().unwrap()
    );
    assert_eq!(
        report.scan.cursor_deferred,
        expected["cursorDeferred"].as_bool().unwrap()
    );
    assert_eq!(report.scan.stop_reason, None);
    assert_eq!(
        report.scan.selection_limited,
        expected["limited"].as_bool().unwrap(),
    );
    assert_eq!(report.scan.observed_matching_entries, observed);
    assert_eq!(
        report.scan.selection_omitted_observed_entries,
        observed - selected
    );
    assert_eq!(report.scan.selected_roots, selected);
    assert_eq!(report.scan.uninspected_selected_roots, 0);
    assert_eq!(report.inspected_roots, selected);
    assert_eq!(
        report
            .unresolved_by_reason
            .get(&JanitorReason::LeaseInvalid),
        Some(&selected)
    );
    assert_eq!(
        report.details[0].reason,
        JanitorReason::CursorPersistenceFailed
    );
    assert_eq!(report.details.len(), expected_page.len() + 1);
    assert_eq!(
        coordinator.dir.read(COORDINATOR_FILE).unwrap(),
        initial_bytes
    );
}

#[test]
fn cursor_write_failure_preserves_the_page_and_reports_the_failure() {
    let trace = include_str!(
        "../../../../../../formal/quint/traces/janitor_selection_persistFailureStep_0.itf.json"
    );
    assert!(
        states(trace)
            .iter()
            .any(|row| row["scan"]["phase"]["tag"] == "PersistFailed")
    );
    replay_selection(trace);
}
