# Task 5 Report: Deterministic parent-replacement races

## Status

Complete.

## TDD evidence

- Added a deterministic, two-barrier seam which pauses after a parent capability is opened
  and before the final entry operation.
- Added race coverage for read, write, remove, mutation, and reset restore. Every test renames
  the opened parent, creates an alternate directory at the old pathname, then resumes the
  operation without probabilistic retries.
- RED: reset restore removed through the opened parent but reopened the logical pathname for
  its write, replacing the alternate directory's sentinel with snapshot bytes.
- GREEN: reset restore now removes, writes, and restores permissions relative to one retained
  parent handle.

## Implementation

- The race hook trait, thread-local storage, guard, and call sites are all `cfg(test)`.
  `WorkerRoot` has no hook field and normal builds contain no hook function or static.
- Public read/write/remove and mutation pause immediately after `open_parent`.
- Reset restoration pauses after opening its parent and keeps that handle across removal,
  recreation, content write, and permission restoration.
- Extracted `WorkerRoot::write_entry` so normal writes and reset restoration share the same
  no-follow final-entry implementation without reopening an ambient pathname.

## Platform behavior

- Tests use an alternate directory rather than requiring symlink/junction privileges, so the
  same deterministic setup runs on Unix and Windows.
- Each replacement sentinel is checked for content and existence; mutating operations also
  retain its permission fingerprint.
- Platform-specific final operations remain capability-relative through the existing Unix
  and Windows backends.

## Verification

- `cargo test -p hoimin-cli parent_replacement_ -- --nocapture`: 5 passed.
- `cargo check -p hoimin-cli --all-targets`: passed.
- `cargo test -p hoimin-cli --all-features`: passed; 86 library tests passed, one existing
  subprocess fixture ignored, and every integration suite passed.
- `cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.

## Self-review

- Hooks are thread-local, one-shot in each test, and cannot pause unrelated parallel tests.
- The tests do not silently skip on Windows or depend on elevated link privileges.
- Reset's previously vulnerable multi-step reopen was fixed at the capability boundary rather
  than weakened in the test.
- `.serena` and the unrelated modified Task 1 report are excluded from the commit.
