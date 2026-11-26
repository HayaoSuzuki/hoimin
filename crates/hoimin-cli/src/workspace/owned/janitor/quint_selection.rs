//! Replay candidate scans against real directory entries and coordinator slots.
use super::super::{
    COORDINATOR_FILE, CoordinatorLockGuard, ManagedRootCoordinator, read_coordinator_state,
    select_reclaim_candidates_locked_until_with_hooks,
};
use crate::quint_itf::{number, set, states};
use camino::Utf8Path;
use hoimin_core::{JanitorReason, JanitorScan};
use serde_json::Value;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

fn name(rank: u64) -> String {
    if rank == 0 {
        String::new()
    } else {
        format!("run-00000000-0000-0000-0000-{rank:012x}")
    }
}

fn update_listing(
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

fn assert_cursor(coordinator: &ManagedRootCoordinator, expected: &Value) {
    let (_, actual) =
        read_coordinator_state(&coordinator.file, &coordinator.path.join(COORDINATOR_FILE))
            .unwrap();
    assert_eq!(actual.cursor, name(number(&expected["name"])));
    assert_eq!(actual.generation, number(&expected["generation"]));
}

fn check_scan(coordinator: &ManagedRootCoordinator, expected: &Value) {
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
    assert!(!scan.selection_limited);
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
        let expected_page: Vec<_> = expected["chosen"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rank| name(number(rank)))
            .collect();
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
fn quint_history_matches_candidate_selection(#[case] trace: &str) {
    let parent = tempfile::tempdir().unwrap();
    let coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(parent.path()).unwrap()).unwrap();
    let mut previous = BTreeSet::new();
    let mut finished = 0;
    for row in states(trace) {
        let scan = &row["scan"];
        if !matches!(
            scan["phase"]["tag"].as_str().unwrap(),
            "Published" | "ExpiredScan" | "ExpiredEof" | "ExpiredPersist"
        ) {
            continue;
        }
        update_listing(&coordinator, &mut previous, &scan["listing"]);
        assert_cursor(&coordinator, &row["startCursor"]);
        check_scan(&coordinator, scan);
        assert_cursor(&coordinator, &row["saved"]);
        finished += 1;
    }
    assert_eq!(
        finished, 3,
        "trace must include three complete scan outcomes"
    );
}
