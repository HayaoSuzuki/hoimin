# Bounded Worker Tree Walks Implementation Plan

> **Issue:** #145 — bound recursion over hostile directory depth in worker tree walks

## Goal

Prevent worker-created directory trees from overflowing the process stack or
exhausting unbounded traversal resources during reset and cleanup. Trees that
exceed the supported depth must fail with a clear `WorkspaceError` instead of
aborting the process.

## Design

Replace recursive traversal in worker enumeration, post-order removal, and
temporary-tree permission cleanup with explicit heap-backed work stacks.
Apply one shared maximum tree depth before descending so that converting stack
frames to file descriptors, paths, and heap allocations does not merely move
the denial-of-service boundary.

Add a typed depth-limit error carrying the offending path and configured
limit. Preserve capability-relative and no-follow behavior in `WorkerRoot`.
Removal frames retain entry names as `OsString`; this avoids introducing a
UTF-8 dependency and prepares the path for Issue #146.

## Task 1: Add bounded-depth regressions

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`

Create nested directories capability-relatively so the fixture does not rely
on long ambient paths. Add regressions that verify:

1. A tree at the supported depth can be enumerated and reset.
2. A tree one level beyond the limit returns the typed depth error without
   terminating the process.
3. Post-order removal handles a deep, supported extra tree.
4. Cleanup traversal reports the same limit while retaining existing
   symlink/reparse and permission behavior.

Run the focused tests against the existing recursive implementation and record
the expected RED result for the new depth-limit contract before changing
production code.

## Task 2: Replace recursive worker walks

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/root.rs`

Convert entry collection to an explicit DFS stack while preserving the final
depth/path sort contract.

Convert recursive removal to an explicit post-order state machine. Process one
child at a time so broad directories do not accumulate one open handle per
child, and drop directory handles before the platform-specific directory
removal call.

Check the shared depth limit before opening or scheduling a child directory.
Keep link/reparse handling and the Unix nonblocking special-entry behavior
introduced by Issue #144 unchanged.

## Task 3: Replace recursive cleanup walk

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/mod.rs`

Convert `make_tree_writable` to an explicit `(PathBuf, depth)` work stack.
Continue to avoid following symlinks and preserve platform permission rules.
Apply the same typed depth limit before descending.

Run focused depth tests, workspace root and handler suites, formatting,
Clippy, full workspace tests, contract-feature tests, and `git diff --check`.
Request an independent review before creating the PR.
