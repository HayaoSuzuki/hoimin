//! Compare cooperative deletion slices against a real nested tree.
use super::super::{
    CleanupBudget, CleanupCapabilities, DELETING_PREFIX, HEARTBEAT_FILE, LEASE_FILE, LEASE_SCHEMA,
    LeaseMarker, ManagedRootCoordinator, OwnerKind, TEST_CLEANUP_SLICE_ENTRIES,
    create_owned_directory, directory_identity, open_owned_directory,
    remove_claimed_tree_slice_with_capabilities,
};
#[cfg(windows)]
use super::super::{
    inspect_marker_evidence, open_owned_directory_for_rename, open_regular_file_nofollow,
    reopen_locked_lease,
};
use crate::quint_itf::{number, set, states};
use camino::Utf8Path;
use serde_json::Value;
use std::{io::ErrorKind, time::Duration};

const PARKED: &str = "parked-original";
const PAYLOAD: &str = "branch/payload";
const SENTINEL: &[u8] = b"replacement must survive";

fn tag(value: &Value) -> &str {
    value["tag"].as_str().unwrap()
}

fn assert_tree(path: &Utf8Path, expected: &Value) {
    let entries = set(expected);
    for (entry, present) in [
        ("Root", path.is_dir()),
        ("Branch", path.join("branch").is_dir()),
        ("Payload", path.join(PAYLOAD).is_file()),
        ("Heartbeat", path.join(HEARTBEAT_FILE).is_file()),
        ("Lease", path.join(LEASE_FILE).is_file()),
    ] {
        assert_eq!(
            present,
            entries.iter().any(|value| tag(value) == entry),
            "{entry} under {path}",
        );
    }
}

fn run_slice(
    coordinator: &ManagedRootCoordinator,
    name: &str,
    identity: (u64, u64),
    capabilities: &mut CleanupCapabilities,
    limit: &str,
    expected: &Value,
) {
    let path = coordinator.path.join(name);
    let stopped = || match limit {
        "StopBefore" => true,
        "StopAfterPayload" => !path.join(PAYLOAD).exists(),
        "StopAfterBranch" => !path.join("branch").exists(),
        "StopAfterHeartbeat" => !path.join(HEARTBEAT_FILE).exists(),
        "FullSlice" | "OneExamined" => false,
        other => panic!("unexpected slice limit {other}"),
    };
    let before = capabilities.progress.examined_entries;
    let budget = CleanupBudget {
        duration: Duration::from_secs(5),
        stopped: Some(&stopped),
    };
    let result = TEST_CLEANUP_SLICE_ENTRIES.with(|cap| {
        let previous = cap.get();
        if limit == "OneExamined" {
            cap.set(1);
        }
        let result = remove_claimed_tree_slice_with_capabilities(
            &coordinator.dir,
            name,
            Some(identity),
            budget,
            capabilities,
        );
        cap.set(previous);
        result
    });
    let expected_result = tag(&expected["result"]);
    let observed_result = match result {
        Ok(slice) => {
            assert_eq!(slice.removed, number(&expected["sliceRemoved"]));
            if slice.complete {
                "Removed"
            } else {
                "Deferred"
            }
        }
        Err(error) if error.kind() == ErrorKind::TimedOut => "Exhausted",
        Err(error) if is_identity_refusal(&error) => "Refused",
        Err(error) => panic!("cleanup slice failed for {limit}: {error}"),
    };
    assert_eq!(observed_result, expected_result, "{limit}");
    let examined = capabilities.progress.examined_entries - before;
    if limit == "StopBefore" || expected_result == "Refused" {
        assert_eq!(examined, 0);
    } else if limit == "OneExamined" {
        assert_eq!(examined, 1);
    }
}

fn is_identity_refusal(error: &std::io::Error) -> bool {
    matches!(
        error.to_string().as_str(),
        "cleanup directory identity changed before reopening"
            | "cleanup directory identity changed while opening"
            | "claimed cleanup root remains after its exact deletion capability was consumed"
    )
}

#[cfg(windows)]
pub(super) fn pin_cleanup(coordinator: &ManagedRootCoordinator, name: &str) -> CleanupCapabilities {
    let path = coordinator.path.join(name);
    let root = open_owned_directory_for_rename(&coordinator.dir, name).unwrap();
    let lease = open_regular_file_nofollow(&root, LEASE_FILE).unwrap();
    let evidence = inspect_marker_evidence(&lease, &path.join(LEASE_FILE)).unwrap();
    drop(lease);
    let lease = reopen_locked_lease(&root, &evidence, &path).unwrap();
    CleanupCapabilities::claimed(root, lease)
}

fn replay(trace: &str, claim: impl FnOnce(&ManagedRootCoordinator, &str) -> CleanupCapabilities) {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let run_id = uuid::Uuid::new_v4().to_string();
    let name = format!("{DELETING_PREFIX}{run_id}");
    let path = coordinator.path.join(&name);
    let original = create_owned_directory(&coordinator.dir, &name).unwrap();
    let branch = create_owned_directory(&original, "branch").unwrap();
    branch.write("payload", b"nested entry").unwrap();
    let marker = serde_json::to_vec(&LeaseMarker {
        schema: LEASE_SCHEMA,
        run_id,
        created_unix_seconds: 0,
        owner: OwnerKind::PublicExecution,
    })
    .unwrap();
    original.write(HEARTBEAT_FILE, &marker).unwrap();
    original.write(LEASE_FILE, &marker).unwrap();
    let identity = directory_identity(&original).unwrap();
    drop(branch);
    drop(original);
    let mut capabilities = claim(&coordinator, &name);

    for row in states(trace).iter().skip(1) {
        let state = &row["s"];
        let previous = capabilities.progress.removed_entries;
        let command = &state["command"];
        let replacement = tag(&state["binding"]) == "Replacement";
        let original_removed = set(&state["remaining"]).is_empty();
        match tag(command) {
            "RunSlice" => {
                let expected_identity = if replacement && original_removed {
                    let entry = open_owned_directory(&coordinator.dir, &name).unwrap();
                    directory_identity(&entry).unwrap()
                } else {
                    identity
                };
                run_slice(
                    &coordinator,
                    &name,
                    expected_identity,
                    &mut capabilities,
                    tag(&command["value"]),
                    state,
                );
            }
            "ReplaceName" => {
                if !original_removed {
                    coordinator
                        .dir
                        .rename(&name, &coordinator.dir, PARKED)
                        .unwrap();
                }
                let replacement = create_owned_directory(&coordinator.dir, &name).unwrap();
                replacement.write("sentinel", SENTINEL).unwrap();
            }
            other => panic!("unexpected removal command {other}"),
        }
        assert_eq!(
            capabilities.progress.removed_entries - previous,
            number(&state["sliceRemoved"])
        );
        assert_eq!(
            capabilities.progress.removed_entries,
            number(&state["acknowledged"])
        );
        assert_eq!(capabilities.progress.root_removed, original_removed);
        if replacement {
            assert_tree(&coordinator.path.join(PARKED), &state["remaining"]);
            assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), SENTINEL);
            assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
        } else {
            assert_tree(&path, &state["remaining"]);
        }
    }
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_removal_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_removal_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_removal_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_removal_3.itf.json"))]
#[case::limits(include_str!("../../../../../../formal/quint/traces/janitor_removal_limitsStep_0.itf.json"))]
#[case::finalization(include_str!("../../../../../../formal/quint/traces/janitor_removal_finalizationStep_0.itf.json"))]
#[case::replacement(include_str!("../../../../../../formal/quint/traces/janitor_removal_replacementStep_0.itf.json"))]
fn quint_histories_match_claimed_tree_removal(#[case] trace: &str) {
    replay(trace, |_, _| CleanupCapabilities::default());
}

#[cfg(windows)]
#[rstest::rstest]
#[case::limits(include_str!("../../../../../../formal/quint/traces/janitor_removal_limitsStep_0.itf.json"))]
#[case::finalization(include_str!("../../../../../../formal/quint/traces/janitor_removal_finalizationStep_0.itf.json"))]
#[case::consumed(include_str!("../../../../../../formal/quint/traces/janitor_removal_consumedReplacementStep_0.itf.json"))]
fn quint_histories_match_windows_pinned_tree_removal(#[case] trace: &str) {
    replay(trace, pin_cleanup);
}
