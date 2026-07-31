# Non-UTF-8 Worker Cleanup Implementation Plan

> **Issue:** #146 — remove non-UTF-8 entries during reset and cleanup

## Goal

Allow reset and worker cleanup to remove hostile native filesystem names that
cannot be represented as UTF-8. Such entries are never manifest members and
must be treated as untracked worker output without leaking the temporary tree,
worker slot, or copy allowance.

## Design

Keep native entry identity separate from manifest identity. Worker traversal
must retain raw `OsString` names (or raw root-relative native paths) for every
filesystem operation. A UTF-8 logical path is optional and is used only for
manifest comparison and ordinary diagnostics. An entry without a UTF-8 path is
always untracked and therefore scheduled for removal.

Never use lossy text to reopen or remove an entry because distinct native names
can collapse to the same display string. Lossy rendering is permitted only in
error messages. Preserve capability-relative, no-follow, nonblocking, bounded
depth, and post-order deletion semantics from Issues #144 and #145.

Cleanup permission preparation must operate on native `Path` values throughout.
I/O errors can remain attributed to the known UTF-8 worker root while including
the native child's lossy display form only as context.

## Task 1: Add platform-native regressions

**Files:**

- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`
- Modify: relevant workspace unit tests

On Linux, create invalid UTF-8 names with
`std::os::unix::ffi::OsStringExt::from_vec`. Verify:

1. Reset removes an untracked non-UTF-8 regular file and restores manifest
   content.
2. Reset removes a non-UTF-8 directory tree, including nested native names.
3. `try_cleanup` removes read-only non-UTF-8 files/directories instead of
   leaving pending cleanup.
4. Handler-level reset/cleanup releases the worker slot and allowance without
   a persistent cleanup error.

On Windows, attempt an equivalent native-name fixture with
`OsStringExt::from_wide` and a lone surrogate. Keep this test separate from the
Linux fixture so platform creation restrictions produce an explicit,
well-scoped result rather than weakening Linux coverage.

Record the existing reset failure and, where supported, the persistent cleanup
failure before changing production code.

## Task 2: Preserve native identity during enumeration

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`

Extend worker entries and iterative collection frames to retain native names or
components. Build `Option<Utf8PathBuf>` only for manifest lookup and stable
ordering. Classify entries with no UTF-8 representation as untracked removal
targets.

Resolve raw components capability-relatively from the worker root and pass the
final `(parent File, name OsString)` into the bounded post-order removal state
machine. Do not follow links or reparse points.

## Task 3: Make removal and cleanup UTF-8-independent

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`

Remove `to_str`/`Utf8Path::from_path` requirements from successful deletion and
permission-preparation paths. Keep `OsString` in removal frames and use native
`Path` values in the iterative cleanup stack. Attribute errors without using a
lossy string as an operational path.

Run focused native-name tests on supported hosts, workspace root/reset/handler
suites, formatting, Clippy, full workspace tests, contract-feature tests, and
`git diff --check`. Request an independent review before creating the PR.
