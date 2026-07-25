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
- On Unix, read and write use `cap_primitives::fs::OpenOptions` with
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

## Independent-review follow-up: Windows final entries

- Added a `cfg(windows)` backend isolated in `workspace/root/windows.rs`.
- Final names are opened relative to the already-open parent `HANDLE` with `NtCreateFile`,
  `OBJECT_ATTRIBUTES::RootDirectory`, and `FILE_OPEN_REPARSE_POINT`.
- The opened handle is rejected before read, truncate, write, or delete when it has
  `FILE_ATTRIBUTE_REPARSE_POINT` or is not a regular file.
- Writes use the non-destructive `FILE_OPEN_IF` disposition, validate first, then make the
  opened handle writable and truncate through that handle.
- Removal requests `DELETE` access and marks the opened handle with
  `FileDispositionInfoEx`; systems without that information class use only the legacy
  handle-based `FileDispositionInfo`. Failure of both paths is returned without any
  ambient-path fallback.
- Raw FFI is confined to the Windows module and each unsafe call/ownership transfer has a
  safety comment.

### Follow-up TDD and verification evidence

- RED: the cross-platform operation-policy test failed to compile before
  `WindowsFinalOperation` and its non-destructive create dispositions existed.
- GREEN: both operation-policy and single-final-component tests pass on macOS.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy -p hoimin-cli --tests -- -D warnings`: pass.
- `cargo test -p hoimin-cli --test workspace_handler --test workspace_recovery`: pass,
  33 passed and 0 failed.
- `cargo check --all-features`: pass.
- Installed `x86_64-pc-windows-msvc`. Full-crate cross-check reached native dependency
  builds but could not complete because this macOS host lacks the MSVC C toolchain/SDK
  required by bundled SQLite and BLAKE3 (`stdlib.h` and `ml64.exe` unavailable).
- A minimal Windows-target harness including the production `root/windows.rs` compiled
  successfully offline against windows-sys 0.60.2, validating the native backend's FFI
  types, constants, and Rust syntax.
