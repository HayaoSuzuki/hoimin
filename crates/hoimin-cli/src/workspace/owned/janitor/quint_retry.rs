//! Compare production retry order with model histories of injected slice results.
#[cfg(unix)]
use super::super::{CleanupCapabilities, open_regular_file_nofollow};
use super::super::{
    DELETING_PREFIX, LEASE_FILE, LEASE_SCHEMA, LeaseMarker, ManagedRootCoordinator, OwnerKind,
    PendingCleanup, ReclaimRemoval, TEST_CLEANUP_SLICE_ENTRIES, create_owned_directory,
    directory_identity, reclaim_removal, write_owned_fixture,
};
#[cfg(windows)]
use super::quint_removal::pin_cleanup;
use super::retry_deferred;
use crate::quint_itf::{number, states};
use camino::Utf8Path;
use serde_json::Value;
use std::cell::Cell;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

fn roots(value: &Value) -> impl Iterator<Item = u64> {
    value.as_array().unwrap().iter().map(number)
}

fn replay(trace: &str, allow_round_start: bool) {
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
    let round_start = Cell::new(allow_round_start);
    let mut actual_order = Vec::new();

    retry_deferred(
        &mut pending,
        || !deadline || attempted.get() < visits.len() || round_start.replace(false),
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
    assert!(
        !round_start.get(),
        "expected a final round-entry clock check"
    );
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
    replay(trace, false);
}

#[test]
fn deadline_after_round_entry_prevents_its_first_visit() {
    let trace = include_str!(
        "../../../../../../formal/quint/traces/janitor_retry_nextRoundDeadlineStep_0.itf.json"
    );
    replay(trace, true);
}

fn make_pending(coordinator: &ManagedRootCoordinator, index: usize) -> PendingCleanup {
    let run_id = uuid::Uuid::new_v4().to_string();
    let deleting_name = format!("{DELETING_PREFIX}{run_id}");
    let root = create_owned_directory(&coordinator.dir, &deleting_name).unwrap();
    let marker = LeaseMarker {
        schema: LEASE_SCHEMA,
        run_id,
        created_unix_seconds: 5,
        owner: OwnerKind::PublicExecution,
    };
    write_owned_fixture(&root, LEASE_FILE, &serde_json::to_vec(&marker).unwrap());
    if index == 0 {
        root.write("payload", b"reclaimable").unwrap();
    } else {
        let branch = create_owned_directory(&root, "branch").unwrap();
        let nested = create_owned_directory(&branch, "nested").unwrap();
        nested.write("payload", b"beyond this slice").unwrap();
    }
    let expected_identity = directory_identity(&root).unwrap();
    #[cfg(unix)]
    let capabilities = {
        let lease = open_regular_file_nofollow(&root, LEASE_FILE).unwrap();
        lease.try_lock().unwrap();
        CleanupCapabilities::with_lease(Some(lease))
    };
    #[cfg(windows)]
    let capabilities = {
        drop(root);
        pin_cleanup(coordinator, &deleting_name)
    };
    PendingCleanup {
        index,
        deleting_name,
        expected_identity,
        capabilities,
    }
}

#[test]
fn quint_retry_rounds_follow_real_deletion_progress() {
    let trace = include_str!(
        "../../../../../../formal/quint/traces/janitor_retry_filesystemProgressStep_0.itf.json"
    );
    let history = states(trace);
    let initial = &history.first().unwrap()["s"];
    let terminal = &history.last().unwrap()["s"];
    assert_eq!(terminal["phase"]["value"]["tag"], "Stalled");
    let visits = terminal["visits"].as_array().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let coordinator =
        ManagedRootCoordinator::open(Utf8Path::from_path(directory.path()).unwrap()).unwrap();
    let mut pending: VecDeque<_> = roots(&initial["pending"])
        .map(|rank| make_pending(&coordinator, usize::try_from(rank).unwrap()))
        .collect();
    let cleared_name = pending.front().unwrap().deleting_name.clone();

    let mut attempted = 0;
    TEST_CLEANUP_SLICE_ENTRIES.with(|limit| {
        let previous = limit.replace(2);
        retry_deferred(
            &mut pending,
            || true,
            |root| {
                let visit = &visits[attempted];
                attempted += 1;
                assert_eq!(root.index, usize::try_from(number(&visit["root"])).unwrap());
                let result = reclaim_removal(
                    &coordinator,
                    &root.deleting_name,
                    root.expected_identity,
                    Instant::now(),
                    &mut root.capabilities,
                    &|_| Duration::ZERO,
                );
                let expected = &visit["result"];
                match result {
                    ReclaimRemoval::Complete => {
                        assert_eq!(expected["tag"], "Complete");
                        assert_eq!(root.capabilities.progress.removed_entries, 3);
                        assert!(root.capabilities.progress.root_removed);
                        None
                    }
                    ReclaimRemoval::Deferred { removed } => {
                        assert_eq!(expected["tag"], "Deferred");
                        assert_eq!(removed, number(&expected["value"]));
                        assert!(!root.capabilities.progress.root_removed);
                        Some(removed)
                    }
                    ReclaimRemoval::Failed(error) => panic!("unexpected slice failure: {error}"),
                }
            },
        );
        limit.set(previous);
    });
    assert_eq!(attempted, visits.len());
    let actual_pending: Vec<_> = pending
        .iter()
        .map(|root| u64::try_from(root.index).unwrap())
        .collect();
    let expected_pending: Vec<_> = roots(&terminal["pending"]).collect();
    assert_eq!(actual_pending, expected_pending);
    assert!(!coordinator.path.join(cleared_name).exists());
    let remaining = pending.front().unwrap();
    let path = coordinator.path.join(&remaining.deleting_name);
    assert_eq!(remaining.capabilities.progress.examined_entries, 4);
    assert_eq!(remaining.capabilities.progress.removed_entries, 0);
    assert_eq!(
        std::fs::read(path.join("branch/nested/payload")).unwrap(),
        b"beyond this slice"
    );
    assert!(path.join(LEASE_FILE).is_file());
}
