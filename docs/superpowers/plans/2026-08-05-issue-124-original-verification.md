# Issue #124 Original Verification Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove two full original-workspace manifest rebuilds from every mutation/reset cycle while preserving worker integrity checks and the explicit final original-integrity checkpoint.

**Architecture:** `WorkerWorkspace` operates only on its immutable private snapshot and worker tree during a mutant cycle. Original-source drift remains the responsibility of `WorkspacePlan::verify_originals`, which is already invoked by the state machine at the pre-final-report checkpoint; mutation application continues to validate the worker target hash, exact span, and original bytes against the preflight manifest.

**Tech Stack:** Rust 2024, Cargo workspace tests, `ignore`, BLAKE3, existing `WorkspacePlan`/`WorkerWorkspace` APIs.

## Global Constraints

- Keep this PR scoped to issue #124; dirty-set reset, async offloading, replay parsing, worker materialization, and line indexing remain in #125–#129.
- Do not introduce mtime/size sampling as an integrity guarantee.
- Preserve the explicit `VerifyOriginals` typed failure at the state-machine checkpoint.
- Record deterministic before/after work metrics and targeted test results in a committed report.
- Keep implementation-private tests beside the implementation under `#[cfg(test)]`; retain public handler behavior tests under `crates/hoimin-cli/tests/`.

---

### Task 1: Add a deterministic manifest-work regression probe

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/manifest.rs`
- Modify: `crates/hoimin-cli/src/workspace/mutation.rs`

**Interfaces:**
- Consumes: existing synchronous `build_manifest(root, options)` and the `worker_and_candidate()` test fixture.
- Produces: test-only `reset_build_metrics()` and `build_metrics() -> (u64, u64)` helpers that report manifest builds and bytes hashed on the current test thread.

- [ ] **Step 1: Add test-only thread-local counters around manifest construction**

```rust
#[cfg(test)]
thread_local! {
    static BUILD_METRICS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

#[cfg(test)]
pub(crate) fn reset_build_metrics() {
    BUILD_METRICS.with(|metrics| metrics.set((0, 0)));
}

#[cfg(test)]
pub(crate) fn build_metrics() -> (u64, u64) {
    BUILD_METRICS.with(Cell::get)
}
```

Increment the build count once at `build_manifest` entry and the byte count after each successful manifest file read. Thread-local storage prevents unrelated parallel tests from contaminating the measurement.

- [ ] **Step 2: Write the failing cycle-cost test**

```rust
#[test]
fn mutation_and_reset_do_not_rebuild_the_original_manifest() {
    let (_project, mut worker, candidate) = worker_and_candidate();
    reset_build_metrics();

    worker.apply_mutation(&candidate).unwrap();
    worker.reset().unwrap();

    assert_eq!(build_metrics(), (0, 0));
}
```

- [ ] **Step 3: Run the test to establish the baseline failure**

Run: `cargo test -p hoimin-cli mutation_and_reset_do_not_rebuild_the_original_manifest -- --nocapture`

Expected: FAIL, reporting two manifest builds and twice the selected original bytes.

- [ ] **Step 4: Commit the red regression test**

```bash
git add crates/hoimin-cli/src/workspace/manifest.rs crates/hoimin-cli/src/workspace/mutation.rs
git commit -m "test(perf): expose repeated original manifest scans"
```

### Task 2: Remove per-mutant original-tree verification

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/mutation.rs`
- Modify: `crates/hoimin-cli/src/workspace/reset.rs`

**Interfaces:**
- Consumes: preflight `WorkspaceManifest`, private `WorkerSnapshot`, and existing mutation target validation.
- Produces: `apply_mutation` and `reset` that perform no original-root scan; no public signature changes.

- [ ] **Step 1: Remove `self.verify_originals()?` from mutation application and reset**

Delete the two calls and remove `WorkerWorkspace::verify_originals`, since the plan-level method is the sole supported original-integrity boundary.

- [ ] **Step 2: Correct API documentation**

Document that `apply_mutation` rejects a worker target that differs from the preflight manifest, and that `reset` restores from the private snapshot. Remove claims that these per-worker methods scan the original workspace.

- [ ] **Step 3: Run the performance regression test**

Run: `cargo test -p hoimin-cli mutation_and_reset_do_not_rebuild_the_original_manifest -- --nocapture`

Expected: PASS with `(0, 0)` manifest work.

- [ ] **Step 4: Commit the implementation**

```bash
git add crates/hoimin-cli/src/workspace/mutation.rs crates/hoimin-cli/src/workspace/reset.rs
git commit -m "perf(workspace): avoid original scans per mutant"
```

### Task 3: Align handler-level integrity and recovery coverage

**Files:**
- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`
- Modify: `crates/hoimin-cli/tests/workspace_recovery.rs`

**Interfaces:**
- Consumes: `WorkerWorkspace::reset`, `WorkspaceHandler::handle_reset_worker`, and `WorkspaceHandler::handle_verify_originals`.
- Produces: regression coverage that separates worker snapshot isolation from explicit source-integrity verification.

- [ ] **Step 1: Replace the reset-time source-drift expectation**

Change `detects_original_change` into a test that mutates the original after worker creation, mutates the worker, resets it successfully, and asserts that the worker contains the preflight bytes while the original retains its external change.

- [ ] **Step 2: Update recovery coverage**

Change `reset_failure_discards_worker_and_allows_recreate_under_same_reservation` to assert that reset succeeds and retains the worker after original drift. Then invoke `handle_verify_originals` with `IntegrityCheckpoint::PreFinalReport` and assert `EffectFailure::OriginalChanged` with the original effect ID.

- [ ] **Step 3: Run focused workspace tests**

Run: `cargo test -p hoimin-cli --test workspace_handler && cargo test -p hoimin-cli --test workspace_recovery`

Expected: PASS.

- [ ] **Step 4: Commit contract updates**

```bash
git add crates/hoimin-cli/tests/workspace_handler.rs crates/hoimin-cli/tests/workspace_recovery.rs
git commit -m "test(workspace): separate reset from source verification"
```

### Task 4: Document results and complete verification

**Files:**
- Create: `docs/superpowers/reports/2026-08-05-issue-124-original-verification.md`

**Interfaces:**
- Consumes: baseline and optimized manifest-build metrics plus verification command output.
- Produces: reviewable evidence for issue #124 and the input boundary for #125.

- [ ] **Step 1: Record scope, correctness argument, and before/after metrics**

The report must state the exact synthetic fixture size, cycle count, manifest builds, bytes hashed, elapsed-time command/results, and why the explicit `VerifyOriginals` checkpoint remains authoritative.

- [ ] **Step 2: Run formatting and full verification**

Run: `cargo fmt --check`

Run: `cargo test --workspace`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: all commands exit 0.

- [ ] **Step 3: Check documentation and diff hygiene**

Run: `rg -n "original workspace changed|verify_originals" crates/hoimin-cli/src crates/hoimin-cli/tests docs/superpowers`

Run: `git diff --check origin/main...HEAD`

Expected: no stale per-worker contract language and no whitespace errors.

- [ ] **Step 4: Commit the report**

```bash
git add docs/superpowers/reports/2026-08-05-issue-124-original-verification.md
git commit -m "docs: report issue 124 performance results"
```

- [ ] **Step 5: Push, create the issue-linked PR, review checks, and merge**

```bash
git push -u origin perf/issue-124-original-verification
gh pr create --base main --head perf/issue-124-original-verification --title "perf(workspace): avoid original scans per mutant" --body-file /private/tmp/hoimin-issue-124-pr-body.md
gh pr checks <pr-number> --watch
gh pr merge <pr-number> --squash --delete-branch
```

The PR body must include `Closes #124`, the before/after metrics, correctness boundary, and verification commands.
