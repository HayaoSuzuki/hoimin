# Shared Workspace Snapshot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound retained CLI heap independently of workspace bytes multiplied by worker count.

**Architecture:** Store pristine bytes once in a private plan-owned temporary directory. Share immutable metadata and the snapshot handle with workers; stream one file into memory only when materializing or restoring it.

**Tech Stack:** Rust, `tempfile`, capability-relative worker filesystem APIs, allocator-tracking integration tests.

## Global Constraints

- Preserve source-change detection before worker creation and reset.
- Preserve file permissions and post-reset verification.
- Preserve existing per-worker copy allowance accounting.
- Do not redefine descendant `--max-memory` as a CLI heap limit.

---

### Task 1: Add retained-heap regression coverage

**Files:**
- Create: `crates/hoimin-cli/tests/workspace_heap.rs`

**Interfaces:**
- Consumes: `WorkspacePlan::preflight`, `WorkspacePlan::create_worker`.
- Produces: a multi-worker large-fixture bound on retained allocations.

- [ ] Write a global allocator tracker isolated in its own integration-test binary.
- [ ] Materialize four workers from a multi-megabyte fixture and retain them.
- [ ] Assert retained allocation remains below a fixed small bound rather than scaling with aggregate worker bytes.
- [ ] Run the test and confirm it fails with the current per-worker byte snapshots.

### Task 2: Introduce one shared disk snapshot

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/copy.rs`

**Interfaces:**
- Produces: `DiskSnapshot`, containing one private temp directory and immutable per-file permission metadata.
- Consumes: the existing manifest, source root, and copy allowance.

- [ ] Remove `Vec<u8>` from `SnapshotFile`.
- [ ] Build and verify one snapshot during preflight.
- [ ] Materialize workers from the shared snapshot while charging the same worker bytes.
- [ ] Share the snapshot with each worker through `Arc`.
- [ ] Run the heap test and worker-copy accounting tests to confirm GREEN.

### Task 3: Restore on demand and document memory semantics

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `DiskSnapshot::read(path)` plus stored permission metadata.
- Produces: unchanged reset behavior without retained pristine byte vectors.

- [ ] Read pristine bytes only for the file currently compared or restored.
- [ ] Preserve permission comparison and post-reset verification.
- [ ] Document shared disk snapshots separately from descendant `--max-memory`.
- [ ] Run focused reset/recovery tests, full workspace tests, fmt, clippy, and diff checks.
- [ ] Commit, push the issue branch, and create a PR closing Issue #68.
