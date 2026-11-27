//! Replay cursor writes and interrupted writes against the real coordinator file.
use super::super::{
    COORDINATOR_DIGEST_OFFSET, COORDINATOR_FILE, COORDINATOR_SLOT_BYTES, CoordinatorLockGuard,
    CoordinatorSlot, ManagedRootCoordinator, WorkspaceError, coordinator_slot_bytes,
    decode_coordinator_slot, persist_coordinator_cursor, read_coordinator_state,
};
use super::quint_cursor_name;
use crate::io_faults;
use crate::quint_itf::{number, states};
use camino::Utf8Path;
use serde_json::Value;
use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::ops::Range;

fn slot_range(index: usize) -> Range<usize> {
    assert!(index < 2);
    let start = 1 + index * COORDINATOR_SLOT_BYTES;
    start..start + COORDINATOR_SLOT_BYTES
}

fn record(expected: &Value) -> CoordinatorSlot {
    CoordinatorSlot {
        generation: number(&expected["generation"]),
        cursor: quint_cursor_name(number(&expected["cursor"])),
    }
}

fn damage(path: &Utf8Path, index: usize) {
    let mut bytes = fs::read(path).unwrap();
    bytes[slot_range(index).start + COORDINATOR_DIGEST_OFFSET] ^= 1;
    fs::write(path, bytes).unwrap();
}

fn save(parent: &Utf8Path, path: &Utf8Path, request: &Value) {
    let coordinator = ManagedRootCoordinator::open(parent).unwrap();
    let before = fs::read(path).unwrap();
    let guard = CoordinatorLockGuard::acquire(&coordinator).unwrap();
    let (active, state) = read_coordinator_state(&coordinator.file, path).unwrap();
    let cursor = quint_cursor_name(number(&request["cursor"]));
    let mode = request["mode"]["tag"].as_str().unwrap();
    let fault = match mode {
        "Commit" => None,
        "BeforeWrite" => Some(io_faults::arm("cursor.write")),
        "SyncError" => Some(io_faults::arm("cursor.flush")),
        "Torn" => {
            let mut writer = coordinator.file.try_clone().unwrap();
            let intended = coordinator_slot_bytes(state.generation + 1, &cursor).unwrap();
            let offset = u64::try_from(slot_range(1 - active).start).unwrap();
            Some(io_faults::arm_action("cursor.write", 0, move || {
                writer.seek(SeekFrom::Start(offset))?;
                // Leave the old digest in place after writing the new payload.
                writer.write_all(&intended[..COORDINATOR_DIGEST_OFFSET])?;
                Err(io::Error::other("injected torn cursor write"))
            }))
        }
        other => panic!("unknown cursor write mode {other}"),
    };
    let result = persist_coordinator_cursor(&coordinator.file, path, active, &state, &cursor);
    if let Some(fault) = fault {
        assert!(fault.triggered(), "the {mode} fault was triggered");
        let expected = if mode == "SyncError" {
            "flush coordinator cursor"
        } else {
            "write coordinator cursor"
        };
        assert!(
            matches!(result, Err(WorkspaceError::Io { operation, .. }) if operation == expected),
            "{mode}: {result:?}"
        );
    } else {
        result.unwrap();
    }
    guard.unlock(&coordinator.path).unwrap();
    let after = fs::read(path).unwrap();
    assert_eq!(after[slot_range(active)], before[slot_range(active)]);
    if mode == "BeforeWrite" {
        assert_eq!(after, before);
    }
}

fn assert_slots_and_reopen(parent: &Utf8Path, path: &Utf8Path, expected: &Value) {
    let bytes = fs::read(path).unwrap();
    let slots = expected["slots"]["#map"].as_array().unwrap();
    assert_eq!(slots.len(), 2);
    for pair in slots {
        let index = usize::try_from(number(&pair[0])).unwrap();
        let slot = &pair[1];
        let expected_slot = match slot["tag"].as_str().unwrap() {
            "Valid" => Some(record(&slot["value"])),
            "Invalid" => None,
            other => panic!("unknown cursor slot {other}"),
        };
        assert_eq!(
            decode_coordinator_slot(&bytes[slot_range(index)]),
            expected_slot,
            "slot {index}",
        );
    }

    let opened = ManagedRootCoordinator::open(parent);
    let observed = &expected["observed"];
    match observed["tag"].as_str().unwrap() {
        "Readable" => {
            let coordinator = opened.unwrap();
            let (index, actual) = read_coordinator_state(&coordinator.file, path).unwrap();
            let entry = &observed["value"];
            assert_eq!(u64::try_from(index).unwrap(), number(&entry["index"]));
            assert_eq!(actual, record(&entry["record"]));
        }
        "Unusable" => {
            assert!(
                matches!(opened, Err(WorkspaceError::InvalidPath { .. })),
                "{opened:?}"
            );
        }
        other => panic!("unknown coordinator reading {other}"),
    }
    assert_eq!(
        fs::read(path).unwrap(),
        bytes,
        "opening must preserve the file"
    );
}

#[rstest::rstest]
#[case::trace_0(include_str!("../../../../../../formal/quint/traces/janitor_cursor_recovery_0.itf.json"))]
#[case::trace_1(include_str!("../../../../../../formal/quint/traces/janitor_cursor_recovery_1.itf.json"))]
#[case::trace_2(include_str!("../../../../../../formal/quint/traces/janitor_cursor_recovery_2.itf.json"))]
#[case::trace_3(include_str!("../../../../../../formal/quint/traces/janitor_cursor_recovery_3.itf.json"))]
#[case::recover(include_str!("../../../../../../formal/quint/traces/janitor_cursor_recovery_recoverStep_0.itf.json"))]
#[case::interrupted_write(include_str!("../../../../../../formal/quint/traces/janitor_cursor_recovery_interruptedWriteStep_0.itf.json"))]
fn quint_history_matches_coordinator_cursor_recovery(#[case] trace: &str) {
    let directory = tempfile::tempdir().unwrap();
    let parent = Utf8Path::from_path(directory.path()).unwrap();
    let path = ManagedRootCoordinator::open(parent)
        .unwrap()
        .path
        .join(COORDINATOR_FILE);
    let rows = states(trace);
    assert!(rows.len() > 1);
    for row in rows {
        let expected = &row["s"];
        let command = &expected["command"];
        match command["tag"].as_str().unwrap() {
            "Init" => {}
            "Save" => save(parent, &path, &command["value"]),
            "Damage" => damage(
                &path,
                usize::try_from(number(&command["value"]["index"])).unwrap(),
            ),
            other => panic!("unknown cursor operation {other}"),
        }
        assert_slots_and_reopen(parent, &path, expected);
    }
}
