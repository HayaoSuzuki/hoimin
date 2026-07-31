# Nonblocking Worker FIFO Handling Implementation Plan

> **Issue:** #144 — avoid blocking forever on FIFOs planted in worker trees

## Goal

Ensure Unix worker workspace operations never block while opening a FIFO or
other hostile special entry, and ensure reset can remove such entries.

## Design

Use `cap_primitives::fs::OpenOptionsExt::custom_flags(libc::O_NONBLOCK)` for
Unix file opens that can observe worker-controlled entries. `O_NONBLOCK` has no
behavioral effect on regular files, but closes the stat/open race where an entry
is replaced by a FIFO immediately before `open`.

Validate the opened metadata before treating an entry as a regular file.
During recursive reset removal, unlink non-directory, non-link, non-regular
special entries directly instead of opening them for permission changes.

## Task 1: Add timeout regressions

**Files:**

- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`

Under `cfg(unix)`, create FIFOs with `libc::mkfifo` and exercise worker read,
write, explicit remove, and reset paths from bounded helper threads. Verify:

1. Operations return before a short timeout rather than waiting for a peer.
2. Read/write/remove reject an invalid regular-file path as appropriate.
3. Reset successfully removes a planted FIFO and restores the worker.

Run focused tests against the existing implementation and record the timeout
failure before changing production code.

## Task 2: Make Unix opens nonblocking

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/root.rs`

Add a small Unix-only helper or consistently configure worker-controlled file
opens with `O_NONBLOCK`. Preserve no-follow behavior and capability-relative
resolution. Validate regular-file expectations after opening.

For reset cleanup, directly unlink special non-directory entries without
opening them. Keep Windows behavior unchanged.

Run focused FIFO tests, the workspace handler suite, formatting, Clippy, full
workspace tests, contract-feature tests, and `git diff --check`. Request an
independent review before creating the PR.
