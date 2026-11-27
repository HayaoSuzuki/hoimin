//! Compare namespace movement and the held lease with the claim model.
#[cfg(windows)]
use super::super::open_owned_directory_for_rename;
use super::super::{
    ACTIVE_PREFIX, ClaimResult, CoordinatorLockGuard, DELETING_PREFIX, LEASE_FILE, LEASE_SCHEMA,
    LeaseMarker, ManagedRootCoordinator, OwnerKind, WorkspaceError, claim_managed_child,
    create_owned_directory, directory_identity, file_identity, open_owned_directory,
    open_regular_file_nofollow,
};
use crate::{io_faults, quint_itf::states};
use camino::Utf8Path;
use cap_std::fs::Dir;
use serde_json::Value;
use std::{cell::Cell, fs::File, io::ErrorKind};
#[cfg(windows)]
use std::{cell::RefCell, rc::Rc};

fn tag(value: &Value) -> &str {
    value["tag"].as_str().unwrap()
}

fn expected_directory(binding: &Value) -> bool {
    tag(binding) == "Present" && tag(&binding["value"]["directory"]) == "ExpectedDirectory"
}

fn make_directory(parent: &Dir, name: &str, marker: &[u8]) -> Dir {
    let dir = create_owned_directory(parent, name).unwrap();
    dir.write(LEASE_FILE, marker).unwrap();
    dir.write("payload", name.as_bytes()).unwrap();
    dir
}

fn outcome(result: Result<ClaimResult, WorkspaceError>) -> &'static str {
    match result {
        Ok(ClaimResult::Claimed) => "Claimed",
        Ok(ClaimResult::Absent) => "Absent",
        Err(WorkspaceError::InvalidPath { .. }) => "Rejected",
        Err(WorkspaceError::Io { operation, .. }) => match operation {
            "claim deadline" => "Stopped",
            "claim managed workspace" => "RenameFailed",
            "restore workspace lease after failed claim" => "RestoreFailed",
            "reopen workspace lease" => "ReopenFailed",
            other => panic!("unexpected claim operation {other}"),
        },
        Err(other) => panic!("unexpected claim error {other}"),
    }
}

fn assert_slot(
    parent: &Dir,
    name: &str,
    expected: &Value,
    root_identity: (u64, u64),
    lease_identity: (u64, u64),
    payload_from: &str,
) {
    match open_owned_directory(parent, name) {
        Ok(dir) => {
            assert_eq!(tag(expected), "Present", "{name}");
            let wanted = &expected["value"];
            let lease = open_regular_file_nofollow(&dir, LEASE_FILE).unwrap();
            assert_eq!(
                directory_identity(&dir).unwrap() == root_identity,
                tag(&wanted["directory"]) == "ExpectedDirectory",
                "{name}",
            );
            assert_eq!(
                file_identity(&lease).unwrap() == lease_identity,
                tag(&wanted["lease"]) == "ExpectedLease",
                "{name}",
            );
            assert_eq!(dir.read("payload").unwrap(), payload_from.as_bytes());
        }
        Err(error) => {
            assert_eq!(error.kind(), ErrorKind::NotFound, "{name}: {error}");
            assert_eq!(tag(expected), "Missing", "{name}");
        }
    }
}

fn assert_lease(dir: &Dir, name: &str, file: Option<&File>, identity: (u64, u64), expected: &str) {
    match expected {
        "HeldExpectedLease" => {
            assert_eq!(file_identity(file.unwrap()).unwrap(), identity);
            let competitor = open_regular_file_nofollow(dir, name).unwrap();
            assert!(matches!(
                File::try_lock(&competitor),
                Err(std::fs::TryLockError::WouldBlock)
            ));
        }
        "Released" => assert!(file.is_none()),
        other => panic!("unexpected lease guard {other}"),
    }
}

#[cfg(windows)]
fn prevent_lease_reopening(root: &Utf8Path) -> File {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    // Reacquisition requests DELETE access. The other reader permits ordinary
    // reads and holds the file open without FILE_SHARE_DELETE.
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(root.join(LEASE_FILE))
        .unwrap()
}

fn prepare_claim<'a>(
    parent: &Dir,
    current: &'a str,
    deleting: &'a str,
    input: &Value,
    run_id: &str,
) -> (&'a str, &'static str, Dir, File) {
    let marker = serde_json::to_vec(&LeaseMarker {
        schema: LEASE_SCHEMA,
        run_id: run_id.to_owned(),
        created_unix_seconds: 5,
        owner: match tag(&input["marker"]) {
            "MatchingMarker" => OwnerKind::PublicExecution,
            "MarkerMismatch" => OwnerKind::PublicDelivery,
            other => panic!("unexpected claim marker {other}"),
        },
    })
    .unwrap();
    let (original_name, original_binding) = if expected_directory(&input["current"]) {
        (current, Some(&input["current"]))
    } else if expected_directory(&input["deleting"]) {
        (deleting, Some(&input["deleting"]))
    } else {
        ("parked-original", None)
    };
    let original_dir = make_directory(parent, original_name, &marker);
    let lease_name = if original_binding
        .is_some_and(|binding| tag(&binding["value"]["lease"]) == "OtherLease")
    {
        let name = "prior-lease";
        original_dir.write(name, &marker).unwrap();
        name
    } else {
        LEASE_FILE
    };
    let lease = open_regular_file_nofollow(&original_dir, lease_name).unwrap();
    File::try_lock(&lease).unwrap();
    drop(original_dir);
    for (name, slot) in [(current, &input["current"]), (deleting, &input["deleting"])] {
        if tag(slot) == "Present" && !expected_directory(slot) {
            drop(make_directory(parent, name, &marker));
        }
    }
    #[cfg(unix)]
    let pin = open_owned_directory(parent, original_name).unwrap();
    #[cfg(windows)]
    let pin = open_owned_directory_for_rename(parent, original_name).unwrap();
    (original_name, lease_name, pin, lease)
}

fn replay(value: &Value) {
    let input = &value["input"];
    let result = &value["claim"];
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let parent = &coordinator.dir;
    let run_id = uuid::Uuid::new_v4().to_string();
    let current = format!("{ACTIVE_PREFIX}{run_id}");
    let deleting = format!("{DELETING_PREFIX}{run_id}");
    let (original_name, lease_name, pin, original_lease) =
        prepare_claim(parent, &current, &deleting, input, &run_id);
    let root_identity = directory_identity(&pin).unwrap();
    let lease_identity = file_identity(&original_lease).unwrap();
    let mut lease = Some(original_lease);
    let interruption = tag(&input["interruption"]);
    #[cfg(windows)]
    let blocker = Rc::new(RefCell::new(None));
    let fault = match interruption {
        "RenameError" => Some(io_faults::arm("claim.rename")),
        #[cfg(windows)]
        "RestoreError" => {
            let path = coordinator.path.join(&current);
            let blocker = Rc::clone(&blocker);
            Some(io_faults::arm_action("claim.rename", 0, move || {
                *blocker.borrow_mut() = Some(prevent_lease_reopening(&path));
                Err(std::io::Error::other(
                    "injected rename error before lease restoration",
                ))
            }))
        }
        _ => None,
    };
    let notifications = Cell::new(0);
    let coordinator_lock = CoordinatorLockGuard::acquire(&coordinator).unwrap();
    let actual = claim_managed_child(
        parent,
        &current,
        &deleting,
        &run_id,
        OwnerKind::PublicExecution,
        &pin,
        #[cfg(windows)]
        &pin,
        &mut lease,
        &|| {
            notifications.set(notifications.get() + 1);
            #[cfg(windows)]
            if interruption == "ReopenError" {
                *blocker.borrow_mut() =
                    Some(prevent_lease_reopening(&coordinator.path.join(&deleting)));
            }
        },
        &|| {
            if interruption == "Deadline" {
                return Err(WorkspaceError::io(
                    "claim deadline",
                    &current,
                    "injected deadline",
                ));
            }
            Ok(())
        },
    );
    coordinator_lock.unlock(&coordinator.path).unwrap();
    if let Some(fault) = fault {
        assert!(fault.triggered(), "{input}");
    }
    assert_eq!(outcome(actual), tag(&result["outcome"]), "{input}");
    let notified = result["notified"].as_bool().unwrap();
    assert_eq!(notifications.get(), u8::from(notified), "{input}");
    assert_lease(
        &pin,
        lease_name,
        lease.as_ref(),
        lease_identity,
        tag(&result["guard"]),
    );
    assert_slot(
        parent,
        &current,
        &result["current"],
        root_identity,
        lease_identity,
        &current,
    );
    assert_slot(
        parent,
        &deleting,
        &result["deleting"],
        root_identity,
        lease_identity,
        if notified { original_name } else { &deleting },
    );
}

fn replay_trace(trace: &str, platform: &str) -> usize {
    let mut replayed = 0;
    let history = states(trace);
    assert_eq!(history.len(), 5);
    for row in history.iter().skip(1) {
        let last = &row["s"]["last"];
        assert_eq!(tag(last), "Evaluated");
        if tag(&last["value"]["input"]["platform"]) == platform {
            replay(&last["value"]);
            replayed += 1;
        }
    }
    replayed
}

#[cfg(unix)]
const PLATFORM: &str = "Unix";
#[cfg(windows)]
const PLATFORM: &str = "Windows";

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_claim_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_claim_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_claim_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_claim_3.itf.json"))]
fn quint_sampled_janitor_claims(#[case] trace: &str) {
    replay_trace(trace, PLATFORM);
}

#[cfg(unix)]
#[rstest::rstest]
#[case::identity(include_str!("../../../../../../formal/quint/traces/janitor_claim_unixIdentityStep_0.itf.json"))]
#[case::handoff(include_str!("../../../../../../formal/quint/traces/janitor_claim_unixHandoffStep_0.itf.json"))]
#[case::fault(include_str!("../../../../../../formal/quint/traces/janitor_claim_unixFaultStep_0.itf.json"))]
fn quint_scheduled_unix_janitor_claims(#[case] trace: &str) {
    assert_eq!(replay_trace(trace, PLATFORM), 4);
}

#[cfg(windows)]
#[rstest::rstest]
#[case::identity(include_str!("../../../../../../formal/quint/traces/janitor_claim_windowsIdentityStep_0.itf.json"))]
#[case::handoff(include_str!("../../../../../../formal/quint/traces/janitor_claim_windowsHandoffStep_0.itf.json"))]
#[case::fault(include_str!("../../../../../../formal/quint/traces/janitor_claim_windowsFaultStep_0.itf.json"))]
fn quint_scheduled_windows_janitor_claims(#[case] trace: &str) {
    assert_eq!(replay_trace(trace, PLATFORM), 4);
}
