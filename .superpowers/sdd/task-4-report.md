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

## Independent-review follow-up: Windows entry deletion

- Removed the remaining Windows reset deletion calls through `cap_primitives`.
- Added `WindowsFinalOperation::RemoveEntry`, which opens the exact final entry relative to the
  stable parent `HANDLE` using `OBJECT_ATTRIBUTES::RootDirectory`, `FILE_OPEN_REPARSE_POINT`,
  `FILE_OPEN`, and `DELETE` access.
- Unlike regular-file operations, this operation deliberately omits
  `FILE_NON_DIRECTORY_FILE`, allowing empty directories and directory reparse points to be
  opened as entries.
- Deletion uses only `SetFileInformationByHandle`, first with `FileDispositionInfoEx` and then
  the legacy handle-only disposition. There is no ambient-path fallback.
- A name replacement race can therefore delete only the independently opened replacement entry
  or fail; it cannot traverse a symlink/junction into its external target.
- Extended the cross-platform operation-policy test to require delete access, non-destructive
  open disposition, reparse-point opening, and directory acceptance for `RemoveEntry`.
- Re-ran the Windows minimal native-backend harness successfully for
  `x86_64-pc-windows-msvc`.

### Read-only entry follow-up

- After the reparse-safe entry open, `remove_entry` now clears a read-only attribute through
  that same validated handle before requesting deletion.
- This preserves handle-only behavior while allowing the legacy `FileDispositionInfo` fallback
  to remove read-only files, directory links, or other reparse entries when
  `FileDispositionInfoEx` is unavailable.
- The Windows access-policy regression now explicitly requires `FILE_WRITE_ATTRIBUTES` as well
  as `DELETE` for `RemoveEntry`.
