# Task 4 Report: Capability-relative reset and restoration

## Status

Complete.

## Coverage

- Added a directory-link regression that verifies reset removes the worker entry without
  traversing to or changing an external sentinel.
- Added a stable-root regression: after the worker pathname is renamed and replaced, reset
  restores the originally opened worker and leaves the replacement tree unchanged.
- Characterized recursive removal of extra nested entries and restoration of a file replaced
  by a directory.
- Characterized read-only permission restoration and exact Unix mode restoration.

The link test alone could pass under the former non-following ambient walker. The stable-root
test distinguishes the new contract because the former `root.join(...)` reset implementation
would address the replacement pathname rather than the opened worker root.

## Implementation

- Added `WorkerEntry` and `WorkerEntryKind` and capability-relative recursive enumeration
  through `read_base_dir` and `open_dir_nofollow`.
- Reset now obtains every worker entry from the stable root capability. It recurses only into
  opened non-link directories and classifies links/reparse points without following them.
- Recursive removal keeps parent directory handles open, makes real files/directories writable
  through handles, and removes only the final component relative to its opened parent.
- Snapshot comparison reads content and permissions through an opened regular-file handle.
- Restoration removes conflicting entry types, creates/writes through `WorkerRoot`, and restores
  permissions through an opened handle.
- Removed reset's ambient `WalkBuilder`, `root.join(...)`, `remove_dir_all`, and pathname
  permission mutation helpers.
- Windows snapshot and permission restoration reuse the handle-relative native final-open
  backend; no ambient fallback was added.
- Replaced Task 3's unstable Windows metadata identity methods with
  `GetFileInformationByHandle`; mutation reopen identity now compares the stable volume serial
  number and 64-bit file index returned for each handle.

## Verification

- `cargo test -p hoimin-cli --test workspace_recovery`: 21 passed.
- `cargo test -p hoimin-cli --test workspace_handler`: 19 passed.
- `cargo test -p hoimin-cli --all-features`: all tests passed; 1 existing subprocess fixture
  remained intentionally ignored.
- `cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.

- Minimal native-backend harness:
  `cargo check --offline --target x86_64-pc-windows-msvc --tests`: passed. This also verifies
  the Task 3 stable Windows handle-identity follow-up.

## Self-review

- Enumeration never derives authority from the display pathname.
- Directory recursion retains opened handles and never traverses links/reparse points.
- Extra files, nested directories, links, and file/directory type changes are removed relative
  to stable parent capabilities.
- Snapshot bytes and permissions are checked on one opened file object per comparison.
- Restoration and permission changes are capability-relative on Unix and Windows.
- Changes are limited to Task 4 production files, recovery tests, this report, and the required
  stable-API repair for Task 3's Windows identity comparison.
