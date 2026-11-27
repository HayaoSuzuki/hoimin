//! Replay candidate scans against real directory entries and coordinator slots.
use super::super::{
    COORDINATOR_FILE, CoordinatorLockGuard, ManagedRootCoordinator, ManagedRunRoot,
    decode_coordinator_slot, owned_directory_entries, read_coordinator_state,
    select_reclaim_candidates, select_reclaim_candidates_controlled_with_hooks,
    select_reclaim_candidates_locked_until_with_hooks,
};
use super::quint_cursor_name as name;
use super::quint_cursor_recovery::{slot_range, torn_cursor_write};
use crate::io_faults;
use crate::quint_itf::{number, set, states};
use camino::Utf8Path;
use hoimin_core::{JanitorReason, JanitorScan, JanitorSummary};
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
#[case::startup_stop(include_str!("../../../../../../formal/quint/traces/janitor_selection_startupStopsStep_0.itf.json"))]
#[case::enumeration_failure(include_str!("../../../../../../formal/quint/traces/janitor_selection_enumerationErrorStep_0.itf.json"))]
fn quint_history_matches_candidate_selection(#[case] trace: &str) {
    replay_selection(trace);
}

fn replay_selection(trace: &str) {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let mut coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let mut previous = BTreeSet::new();
    let mut finished = 0;
    for row in states(trace) {
        let scan = &row["scan"];
        let phase = scan["phase"]["tag"].as_str().unwrap();
        if !matches!(
            phase,
            "Published"
                | "PersistFailed"
                | "PersistTorn"
                | "PersistSyncFailed"
                | "ExpiredScan"
                | "ExpiredStartup"
                | "ExpiredDuringEnumeration"
                | "ExpiredEof"
                | "ExpiredPersist"
                | "CoordinatorBusy"
                | "SelectionReadFailed"
                | "EnumerationFailed"
                | "EnumerationInterrupted"
        ) {
            continue;
        }
        update_listing(&coordinator, &mut previous, &scan["listing"]);
        assert_cursor(&coordinator, &row["startCursor"]);
        match phase {
            "PersistFailed" | "PersistTorn" | "PersistSyncFailed" => {
                check_cursor_failure_handoff(&coordinator, scan, phase);
                drop(coordinator);
                coordinator = ManagedRootCoordinator::open(parent).unwrap();
            }
            "CoordinatorBusy"
            | "SelectionReadFailed"
            | "EnumerationFailed"
            | "EnumerationInterrupted" => {
                check_unavailable(&coordinator, phase, scan);
            }
            "ExpiredStartup" | "ExpiredDuringEnumeration" => {
                check_initial_deadline(&coordinator, phase);
            }
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

fn check_unavailable(coordinator: &ManagedRootCoordinator, phase: &str, expected: &Value) {
    let before = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let busy = phase == "CoordinatorBusy";
    let guard = busy.then(|| CoordinatorLockGuard::acquire(coordinator).unwrap());
    let fault = match phase {
        "CoordinatorBusy" => None,
        "SelectionReadFailed" => Some(io_faults::arm("cursor.read")),
        "EnumerationFailed" => Some(io_faults::arm("janitor.selection_entries")),
        "EnumerationInterrupted" => {
            let entries = owned_directory_entries(&coordinator.dir).unwrap().count();
            Some(io_faults::arm_after("janitor.selection_next", entries))
        }
        other => panic!("unexpected selection error {other}"),
    };
    let report = ManagedRunRoot::reclaim_abandoned_controlled(
        coordinator,
        SystemTime::now(),
        tokio::time::Instant::now() + Duration::from_secs(2),
        &|| false,
    );
    if let Some(fault) = fault {
        assert!(fault.triggered());
        assert_scan_stopped(
            &report,
            JanitorReason::SelectionFailed,
            &[JanitorReason::SelectionFailed],
            u64::try_from(set(&expected["seen"]).len()).unwrap(),
        );
    } else {
        assert_scan_stopped(&report, JanitorReason::CoordinatorBusy, &[], 0);
    }
    if phase == "EnumerationInterrupted" {
        super::quint_reclaim::assert_startup_report(
            &report,
            include_str!(
                "../../../../../../formal/quint/traces/janitor_startup_selectionFailureStep_0.itf.json"
            ),
        );
    }
    if let Some(guard) = guard {
        guard.unlock(&coordinator.path).unwrap();
    }
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), before);
}

fn check_initial_deadline(coordinator: &ManagedRootCoordinator, phase: &str) {
    let before = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let now = SystemTime::now();
    if phase == "ExpiredStartup" {
        let fault = io_faults::arm("cursor.read");
        let report = ManagedRunRoot::reclaim_abandoned_controlled(
            coordinator,
            now,
            tokio::time::Instant::now(),
            &|| false,
        );
        assert!(!fault.triggered());
        assert_scan_stopped(&report, JanitorReason::SelectionDeadline, &[], 0);
    } else {
        let yielded = Cell::new(0);
        let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
            coordinator,
            now,
            Instant::now(),
            &|_| Duration::ZERO,
            &|scan| {
                select_reclaim_candidates_controlled_with_hooks(
                    coordinator,
                    scan,
                    &|| {
                        if yielded.get() == 0 {
                            Duration::from_secs(1)
                        } else {
                            Duration::ZERO
                        }
                    },
                    &|| yielded.set(yielded.get() + 1),
                )
            },
        );
        assert_eq!(yielded.get(), 1);
        assert_scan_stopped(
            &report,
            JanitorReason::SelectionDeadline,
            &[JanitorReason::SelectionDeadline],
            0,
        );
    }
    assert_eq!(coordinator.dir.read(COORDINATOR_FILE).unwrap(), before);
}

fn assert_scan_stopped(
    report: &JanitorSummary,
    reason: JanitorReason,
    details: &[JanitorReason],
    observed: u64,
) {
    assert_eq!(
        report.scan,
        JanitorScan {
            stop_reason: Some(reason),
            observed_matching_entries: observed,
            selection_omitted_observed_entries: observed,
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
}

fn check_cursor_failure_handoff(
    coordinator: &ManagedRootCoordinator,
    expected: &Value,
    phase: &str,
) {
    let initial_bytes = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    let (active, state) =
        read_coordinator_state(&coordinator.file, &coordinator.path.join(COORDINATOR_FILE))
            .unwrap();
    let expected_page = expected_page(expected);
    let report = ManagedRunRoot::reclaim_abandoned_with_clock_and_selection(
        coordinator,
        SystemTime::now(),
        Instant::now(),
        &|_| Duration::ZERO,
        &|scan| {
            let fault = match phase {
                "PersistFailed" => io_faults::arm("cursor.write"),
                "PersistTorn" => io_faults::arm_action(
                    "cursor.write",
                    0,
                    torn_cursor_write(
                        &coordinator.file,
                        active,
                        &state,
                        expected_page.last().unwrap(),
                    ),
                ),
                "PersistSyncFailed" => io_faults::arm("cursor.flush"),
                other => panic!("unexpected cursor fault {other}"),
            };
            let (page, error) = select_reclaim_candidates(coordinator, scan).unwrap();
            assert!(fault.triggered());
            assert_eq!(page, expected_page);
            let operation = if phase == "PersistSyncFailed" {
                "flush coordinator cursor"
            } else {
                "write coordinator cursor"
            };
            assert!(error.as_ref().unwrap().contains(operation));
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
    let bytes = coordinator.dir.read(COORDINATOR_FILE).unwrap();
    assert_eq!(bytes[slot_range(active)], initial_bytes[slot_range(active)]);
    if phase == "PersistFailed" {
        assert_eq!(bytes, initial_bytes);
    } else {
        assert_ne!(bytes, initial_bytes);
        assert_eq!(
            decode_coordinator_slot(&bytes[slot_range(1 - active)]).is_some(),
            phase == "PersistSyncFailed",
        );
    }
}

#[rstest::rstest]
#[case::before_write("PersistFailed", include_str!("../../../../../../formal/quint/traces/janitor_selection_persistFailureStep_0.itf.json"))]
#[case::torn("PersistTorn", include_str!("../../../../../../formal/quint/traces/janitor_selection_tornPersistStep_0.itf.json"))]
#[case::sync("PersistSyncFailed", include_str!("../../../../../../formal/quint/traces/janitor_selection_syncFailureStep_0.itf.json"))]
fn quint_cursor_failure_preserves_the_page_and_reports_the_failure(
    #[case] phase: &str,
    #[case] trace: &str,
) {
    assert!(
        states(trace)
            .iter()
            .any(|row| row["scan"]["phase"]["tag"] == phase)
    );
    replay_selection(trace);
}
