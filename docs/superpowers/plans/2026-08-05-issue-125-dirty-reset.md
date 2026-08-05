# Issue #125 Single-Pass Workspace Reset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce each worker reset from two tree walks and four workspace-sized reads to one tree walk, one worker-content pass, and snapshot reads only for files that require restoration.

**Architecture:** Build the expected directory set once, classify the worker tree in one capability-relative traversal, and compare each surviving regular file's BLAKE3 hash and permissions with immutable snapshot metadata. Restore missing or mismatched files from the private disk snapshot; keep the expensive complete postcondition only in test/contracts builds, where invariant verification is explicitly requested.

**Tech Stack:** Rust 2024, BLAKE3, capability-relative filesystem APIs, Cargo workspace tests, existing `contracts` feature.

## Global Constraints

- Keep arbitrary subprocess writes correct; do not assume the mutation target is the only dirty path.
- Do not rely on mtime/size alone, because same-size and timestamp-preserving writes must be detected.
- Preserve link/reparse-point rejection, capability-relative traversal, permission restoration, depth limits, and typed reset failures.
- Keep this PR scoped to #125; async offloading, candidate replay, worker materialization, and line indexing remain #126–#129.
- Record deterministic I/O work metrics and a same-fixture release benchmark before and after.

---

### Task 1: Add deterministic reset I/O metrics and a failing regression

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`

**Interfaces:**
- Consumes: `DiskSnapshot::read`, `WorkerRoot::entries`, and worker-content reads performed by reset comparison.
- Produces: test-only thread-local `ResetIoMetrics { tree_walks, snapshot_bytes, worker_bytes }`, reset/read helpers, a zero-cross-test-contamination probe, and an ignored release benchmark.

- [ ] **Step 1: Add test-only thread-local counters at the actual I/O boundaries**

Count one walk at `WorkerRoot::entries`, snapshot bytes after each successful `DiskSnapshot::read`, and worker bytes after each successful reset-comparison read. Keep the helpers under `#[cfg(test)]` and thread-local so parallel tests cannot affect the result.

- [ ] **Step 2: Write a failing one-cycle work test**

Create a worker containing a 1 MiB unchanged padding file and a 9-byte target, mutate only the target, reset the metrics, call `reset`, and assert these exact optimized bounds:

```rust
assert_eq!(metrics.tree_walks, 1);
assert_eq!(metrics.worker_bytes, 1_048_585);
assert_eq!(metrics.snapshot_bytes, 9);
```

The production change that makes this test pass is removal of the second walk/read pass and avoidance of snapshot reads for unchanged files.

- [ ] **Step 3: Run the regression to establish RED**

Run: `cargo test -p hoimin-cli reset_reads_worker_once_and_snapshot_only_for_dirty_files -- --nocapture`

Expected baseline failure: two tree walks, twice the worker bytes, and twice the complete snapshot bytes.

- [ ] **Step 4: Add an ignored release benchmark using 8 MiB padding and ten mutation/reset cycles**

Print fixture bytes, cycles, tree walks, worker bytes, snapshot bytes, and elapsed milliseconds. Run the identical test-only harness on the baseline and optimized revisions.

- [ ] **Step 5: Commit the RED test and benchmark harness**

```bash
git add crates/hoimin-cli/src/workspace/mod.rs crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/src/workspace/reset.rs
git commit -m "test(perf): expose repeated workspace reset reads"
```

### Task 2: Store immutable content hashes with snapshot metadata

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/copy.rs`

**Interfaces:**
- Consumes: `ManifestEntry::blake3` while `create_disk_snapshot` validates and copies each source file.
- Produces: `SnapshotFile::new(permissions, blake3)` and a private `blake3: blake3::Hash` field used by reset without reading snapshot contents.

- [ ] **Step 1: Extend `SnapshotFile` with its validated BLAKE3 hash**

Pass `entry.blake3` into `SnapshotFile::new` only after the copied bytes have already matched the manifest hash.

- [ ] **Step 2: Run snapshot and copy tests**

Run: `cargo test -p hoimin-cli workspace::copy --lib`

Expected: PASS with no public API changes.

- [ ] **Step 3: Commit snapshot metadata**

```bash
git add crates/hoimin-cli/src/workspace/mod.rs crates/hoimin-cli/src/workspace/copy.rs
git commit -m "refactor(workspace): retain snapshot content hashes"
```

### Task 3: Implement single-pass reset classification and restoration

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`

**Interfaces:**
- Consumes: `SnapshotFile::blake3`, permission fingerprint, `WorkerRoot::entries`, and capability-relative file opening.
- Produces: `WorkerRoot::snapshot_hash_matches(path, expected_hash, expected_permissions) -> Result<bool, WorkspaceError>` and a one-pass `reset_from_snapshot`.

- [ ] **Step 1: Add capability-relative hash and permission comparison**

Open a regular worker file without following links, read it once, account for the bytes in test metrics, and compare `blake3::hash(&bytes)` plus the permission fingerprint to the snapshot metadata. Preserve Windows reparse handling and Unix `O_NONBLOCK` behavior from `snapshot_matches`.

- [ ] **Step 2: Precompute required directories**

Build a `BTreeSet<Utf8PathBuf>` containing every non-empty parent of every snapshot file. Replace `required_directory(path, snapshot.files.keys())` with O(log dirs) membership checks.

- [ ] **Step 3: Classify and clean the worker from one `entries` result**

In reverse depth order, remove every link/reparse entry, file absent from the snapshot, and directory absent from the required-directory set. Retain a set of existing regular snapshot paths for the restoration loop.

- [ ] **Step 4: Read snapshots only for missing or mismatched files**

Call `snapshot_hash_matches` first. If it returns true, retain the existing file object. Otherwise read that one snapshot and restore its bytes and permissions.

- [ ] **Step 5: Gate the complete postcondition behind test/contracts builds**

Compile `matches_snapshot` under `#[cfg(any(test, feature = "contracts"))]`, but invoke it automatically only under `#[cfg(feature = "contracts")]`. Return success directly after checked restoration in normal builds.

- [ ] **Step 6: Run GREEN and focused reset coverage**

Run: `cargo test -p hoimin-cli reset_reads_worker_once_and_snapshot_only_for_dirty_files -- --nocapture`

Run: `cargo test -p hoimin-cli --test workspace_handler && cargo test -p hoimin-cli --test workspace_recovery && cargo test -p hoimin-cli workspace::reset --lib`

Expected: all PASS; the metrics test reports one walk, one worker pass, and only dirty snapshot bytes.

- [ ] **Step 7: Commit the implementation**

```bash
git add crates/hoimin-cli/src/workspace/root.rs crates/hoimin-cli/src/workspace/reset.rs
git commit -m "perf(workspace): reset workers in one content pass"
```

### Task 4: Verify adversarial writes and contracts postcondition

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`
- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`
- Modify: `crates/hoimin-cli/tests/workspace_recovery.rs`

**Interfaces:**
- Consumes: public worker write/remove operations, direct fixture filesystem writes that model subprocess behavior, and contracts-enabled reset.
- Produces: explicit coverage for same-size changes, permission-only changes, added/deleted/type-changed paths, and full postcondition verification.

- [ ] **Step 1: Add same-size arbitrary-write coverage**

Modify a non-candidate file to different bytes of the same length through the filesystem, reset, and assert exact snapshot bytes. This prevents metadata-only shortcuts.

- [ ] **Step 2: Retain structural and permission coverage**

Confirm existing tests still cover extra nested paths, deleted files, file/directory replacement, links, read-only files, and permissions. Add only a missing boundary test if the existing assertion does not exercise the optimized branch.

- [ ] **Step 3: Run contracts-enabled reset tests**

Run: `cargo test -p hoimin-cli --features contracts workspace::reset --lib`

Expected: PASS with the full `matches_snapshot` postcondition enabled.

- [ ] **Step 4: Commit adversarial coverage**

```bash
git add crates/hoimin-cli/src/workspace/reset.rs crates/hoimin-cli/tests/workspace_handler.rs crates/hoimin-cli/tests/workspace_recovery.rs
git commit -m "test(workspace): preserve arbitrary reset recovery"
```

### Task 5: Document, verify, review, and integrate

**Files:**
- Create: `docs/superpowers/reports/2026-08-05-issue-125-dirty-reset.md`

**Interfaces:**
- Consumes: deterministic baseline/optimized I/O counts, release timings, and verification output.
- Produces: issue #125 performance/correctness report and a `Closes #125` PR.

- [ ] **Step 1: Record the design deviation and benchmark**

Explain why mutation-target-only tracking is unsafe for arbitrary subprocess writes. Record identical fixture/cycles, tree walks, worker bytes, snapshot bytes, elapsed time, and exact baseline harness procedure.

- [ ] **Step 2: Run full verification**

Run: `cargo fmt --check`

Run: `cargo test --workspace`

Run: `cargo test --workspace --all-features`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Run: `git diff --check origin/main...HEAD`

Expected: all exit 0.

- [ ] **Step 3: Request read-only code review and resolve Critical/Important findings**

Review the complete `origin/main..HEAD` range for reset correctness, capability safety, platform behavior, metrics accuracy, and plan alignment.

- [ ] **Step 4: Commit the report, push, create PR, watch CI, and merge**

```bash
git add docs/superpowers/reports/2026-08-05-issue-125-dirty-reset.md
git commit -m "docs: report issue 125 performance results"
git push -u origin perf/issue-125-dirty-reset
gh pr create --base main --head perf/issue-125-dirty-reset --title "perf(workspace): reset workers in one content pass" --body-file /private/tmp/hoimin-issue-125-pr-body.md
gh pr checks --watch
gh pr merge --squash --delete-branch
```

After confirming the merge commit and issue closure, fast-forward local `main`, remove the merged worktree, and delete remaining local/remote branches.
