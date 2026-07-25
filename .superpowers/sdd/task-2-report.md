# Task 2 Report: Capability-relative public file operations

## Status

Complete.

## TDD evidence

- Characterized nested creation, read-only replacement, missing existence, and removal.
- Added final-component link write/remove rejection with an unchanged external sentinel.
- Added an open-root binding regression: after the root pathname is renamed and replaced,
  writes remain attached to the originally opened worker directory.
- RED: `root_relative_file_apis_remain_bound_to_open_worker_root` failed because the
  pathname implementation wrote through the replacement root; the expected file under
  the moved, originally opened root was absent.
- GREEN: capability-relative implementation passes the new tests and the existing suites.

## Implementation

- Added `WorkerRoot::{read, write, remove_file, try_exists}`.
- Each operation validates normalized logical paths, opens the parent relative to the
  stable root handle, rejects final links/reparse points, and performs the final operation
  relative to that parent.
- Read and write use `cap_primitives::fs::OpenOptions` with
  `OpenOptionsFollowExt::follow(FollowSymlinks::No)`.
- Writable permission changes use opened file/directory handles.
- `WorkerWorkspace` public file APIs delegate to `WorkerRoot`.
- Ambient `make_tree_writable` remains limited to owned temporary-directory cleanup.

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy -p hoimin-cli --tests -- -D warnings`
- `cargo test -p hoimin-cli`
- Result: all commands exited 0; crate tests passed with one existing ignored subprocess
  fixture and no failures.

## Self-review

- Scope is limited to Task 2 production files and the requested integration tests.
- Final-entry checks map links/reparse points to `WorkspaceError::InvalidPath`.
- No-follow opens protect the use after the metadata check; capability-relative parent
  handles avoid root pathname replacement.
- Existing `resolve_worker_path` and pathname `make_writable` remain temporarily because
  `mutation.rs` consumes them. Removing or migrating those would be Task 3 scope; public
  file operations no longer use them.
