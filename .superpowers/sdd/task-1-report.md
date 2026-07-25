# Task 1 Report: Stable no-follow worker-root capability

Status: DONE

## Implementation

- Added `cap-primitives` and `cap-fs-ext` 4.0 dependencies.
- Added private `WorkerRoot`, which opens the materialized worker root once and retains its directory handle.
- Added normalized relative-path validation and component-by-component no-follow parent traversal.
- Added capability-relative creation and reopening of missing parent directories.
- Rejects symbolic links and Windows reparse points as `WorkspaceError::InvalidPath`; ordinary parent I/O failures remain `WorkspaceError::Io`.
- Changed `WorkerWorkspace::from_materialized` to fail closed while establishing the capability and retained the pathname only through `WorkerRoot::path()`.

## TDD Evidence

- Initial focused test build failed because `WorkerRoot` was undefined.
- The missing-parent creation test failed with `No such file or directory` after creation behavior was removed, then passed after capability-relative creation was restored.
- The regular-file-parent test failed because `ENOTDIR` was over-classified as `InvalidPath`, then passed after link/reparse classification was narrowed.

## Verification

- `cargo test -p hoimin-cli workspace::root::tests -- --nocapture`: 5 passed.
- `cargo test -p hoimin-cli --test workspace_handler --test workspace_recovery`: 30 passed.
- `cargo clippy -p hoimin-cli --all-targets -- -D warnings`: passed.
- `cargo fmt --all`: passed.
- `git diff --check`: passed.

## Review

Independent review identified and prompted fixes for racy failure classification, untested `create=true`, and regular-file-parent over-classification. Final re-review found no blocking findings.

## Concerns

- Windows-specific code was compile-reviewed and uses `FILE_ATTRIBUTE_REPARSE_POINT`, but this macOS host does not have a Windows Rust target installed for local cross-compilation.
