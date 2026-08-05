# Issue #128 Worker Materialization I/O Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Materialize each worker with one trusted snapshot read and verify the original workspace exactly once after the complete worker-creation wave.

**Architecture:** The core state machine inserts a typed `PostMaterialization` integrity checkpoint between concurrent worker creation and the baseline. `WorkspacePlan` stops reading originals and hashing snapshot bytes per worker in normal builds; the existing `VerifyOriginals` task performs the single wave-level source check, while contracts builds retain snapshot hash assertions.

**Tech Stack:** Rust 2024, BLAKE3, Tokio owned blocking tasks, atomic test instrumentation, Cargo workspace tests, existing `contracts` feature.

## Global Constraints

- The baseline must not start until every worker succeeds and `VerifyOriginals(PostMaterialization)` succeeds.
- Preserve effect IDs, pending-effect validation, typed workspace failures, cancellation, cleanup, capability-relative file operations, permissions, and copy allowance rollback.
- The process-private snapshot is trusted only in normal builds; contracts builds compare each read snapshot file's size and BLAKE3 hash with the manifest before writing it.
- Copy allowance counts bytes written into workers, not integrity-verification reads.
- Keep this PR scoped to Issue #128; do not optimize preflight, change scheduler concurrency, change CLI behavior, or implement Issue #129 line indexing.
- Record deterministic I/O work counts and same-harness release timings; elapsed time is supporting evidence, not a CI threshold.

---

### Task 1: Insert the post-materialization integrity checkpoint

**Files:**
- Modify: `crates/hoimin-core/src/model.rs:108-115`
- Modify: `crates/hoimin-core/src/machine.rs:20-31,604-626,1365-1392,1699-1701`
- Test: `crates/hoimin-core/tests/machine.rs`

**Interfaces:**
- Consumes: existing `VerifyOriginals { id, checkpoint }`, `OriginalsVerified { id, checkpoint }`, pending-effect tracking, and worker completion accounting.
- Produces: `IntegrityCheckpoint::PostMaterialization`, `RunPhase::MaterializationVerification`, and the transition `last WorkerCreated -> VerifyOriginals -> RunBaseline`.

- [ ] **Step 1: Write failing state-machine tests**

Add `IntegrityCheckpoint` and `VerifyOriginals` to the test imports. Complete all emitted `CreateWorker` effects and assert the new boundary:

```rust
#[test]
fn all_workers_are_verified_once_before_baseline() {
    let mut raw = fixture_raw_config();
    raw.limits.jobs = 2;
    let (state, effects) = waiting_for_materialization_verification_with(
        RunConfig::try_from(raw).unwrap(),
    );

    assert_eq!(state.phase(), RunPhase::MaterializationVerification);
    let [RunEffect::VerifyOriginals(verify)] = effects.as_slice() else {
        panic!("expected one original verification, got {effects:?}");
    };
    assert_eq!(verify.checkpoint, IntegrityCheckpoint::PostMaterialization);

    let (state, effects) = transition(
        state,
        RunEvent::OriginalsVerified(OriginalsVerified {
            id: verify.id,
            checkpoint: IntegrityCheckpoint::PostMaterialization,
        }),
    )
    .unwrap();

    assert_eq!(state.phase(), RunPhase::Baseline);
    assert!(matches!(effects.as_slice(), [RunEffect::RunBaseline(_)]));
}
```

Add a second test that sends `OriginalsVerified` with the correct pending effect ID but `IntegrityCheckpoint::PreFinalReport` and asserts `transition(...).is_err()`. Extract the current start-through-worker-completion body of `waiting_for_baseline_with(config)` into `waiting_for_materialization_verification_with(config: RunConfig) -> (RunState, Vec<RunEffect>)`. Make `waiting_for_baseline_with` call that helper, extract the sole `VerifyOriginals`, and transition with `OriginalsVerified { id: verify.id, checkpoint: verify.checkpoint }` so unrelated tests retain their intended baseline starting point. Make `waiting_for_baseline_from` cross the same verification after its single worker completion.

- [ ] **Step 2: Run focused tests and confirm RED**

Run: `cargo test -p hoimin-core all_workers_are_verified_once_before_baseline -- --nocapture`

Expected: FAIL because the new checkpoint and phase do not exist and the last worker emits `RunBaseline` directly.

- [ ] **Step 3: Implement the minimal checkpoint transition**

Add the enum variants:

```rust
pub enum IntegrityCheckpoint {
    PreAnalysis,
    PostMaterialization,
    Periodic,
    PreFinalReport,
    Cleanup,
}

pub enum RunPhase {
    Validate,
    Preflight,
    Copy,
    MaterializationVerification,
    Baseline,
    BudgetCheck,
    Analyze,
    Mutants,
    Finalize,
    Cleaning,
    Finished,
}
```

Extract baseline effect construction into `fn baseline_effects(&mut self) -> Result<Vec<RunEffect>, MachineError>`. After the last worker completes, enter `MaterializationVerification` and emit exactly:

```rust
RunEffect::VerifyOriginals(VerifyOriginals {
    id: state.allocate_id()?,
    checkpoint: IntegrityCheckpoint::PostMaterialization,
})
```

Accept only a matching completion payload:

```rust
RunEvent::OriginalsVerified(value)
    if state.phase == RunPhase::MaterializationVerification
        && value.checkpoint == IntegrityCheckpoint::PostMaterialization =>
{
    state.phase = RunPhase::Baseline;
    state.baseline_effects()?
}
```

Let the existing unexpected-event path reject a mismatched checkpoint. Keep final-report verification restricted to `RunPhase::Finalize`.

- [ ] **Step 4: Run the core machine suite**

Run: `cargo test -p hoimin-core --test machine`

Expected: PASS for single-worker, multi-worker, mismatched checkpoint, and all existing transitions.

- [ ] **Step 5: Commit the state-machine boundary**

```bash
git add crates/hoimin-core/src/model.rs crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs
git commit -m "perf(core): verify originals once after worker creation"
```

---

### Task 2: Add deterministic materialization I/O evidence

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/copy.rs:16-31,280-455,517-720`

**Interfaces:**
- Consumes: `WorkspacePlan::create_worker`, `WorkspacePlan::verify_originals`, manifest logical byte counts, and concurrent `Arc<WorkspacePlan>` access.
- Produces: test-only `MaterializationIoSnapshot { original_manifest_builds, original_bytes, original_hash_bytes, snapshot_bytes, snapshot_hash_bytes }`, plan-scoped atomic counters, a baseline-count test, and `benchmark_worker_materialization_io`.

- [ ] **Step 1: Add plan-scoped atomic test instrumentation**

Under `#[cfg(test)]`, define counters owned by each `WorkspacePlan` and a copyable snapshot:

```rust
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MaterializationIoSnapshot {
    original_manifest_builds: u64,
    original_bytes: u64,
    original_hash_bytes: u64,
    snapshot_bytes: u64,
    snapshot_hash_bytes: u64,
}
```

Use a `MaterializationIoMetrics` struct with five `AtomicU64` fields and methods `record_original_manifest(bytes: u64)`, `record_original_read(bytes: usize)`, `record_original_hash(bytes: usize)`, `record_snapshot_read(bytes: usize)`, `record_snapshot_hash(bytes: usize)`, and `snapshot() -> MaterializationIoSnapshot`. Store it as `Arc<MaterializationIoMetrics>` in `WorkspacePlan` so concurrent workers contribute to one plan without cross-test contamination. Record actual byte lengths after successful reads and hash bytes immediately before BLAKE3 calls. In `verify_originals`, call `record_original_manifest(current.logical_bytes())` after a successful build.

- [ ] **Step 2: Add a passing baseline-count test and ignored benchmark**

Use two files totaling `fixture_bytes` and two workers. After preflight, create both workers, call `plan.verify_originals()` once to model the future wave boundary, and assert the current counts:

```rust
assert_eq!(
    plan.materialization_io_snapshot(),
    MaterializationIoSnapshot {
        original_manifest_builds: 5,
        original_bytes: 7 * fixture_bytes,
        original_hash_bytes: 7 * fixture_bytes,
        snapshot_bytes: 2 * fixture_bytes,
        snapshot_hash_bytes: 2 * fixture_bytes,
    }
);
```

Add `#[ignore = "manual before/after performance evidence"] fn benchmark_worker_materialization_io()` with an 8 MiB padding file, eight workers, one final `verify_originals`, and elapsed timing around only worker creation plus final verification. Print all counters, copied bytes, workers, fixture bytes, and elapsed milliseconds.

- [ ] **Step 3: Run the evidence and baseline benchmark**

Run: `cargo test -p hoimin-cli materialization_io_counts_full_tree_work -- --nocapture`

Expected: PASS with 5 manifest builds, 7 original byte/hash passes, 2 snapshot reads/hashes, and 2 worker copies.

Run five times: `cargo test --release -p hoimin-cli benchmark_worker_materialization_io -- --ignored --nocapture`

Expected: PASS. Save every elapsed value and deterministic counter for the report.

- [ ] **Step 4: Commit the reusable evidence harness**

```bash
git add crates/hoimin-cli/src/workspace/copy.rs
git commit -m "test(workspace): measure materialization tree work"
```

---

### Task 3: Remove redundant worker-level reads and hashes

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/copy.rs:352-455,517-720`
- Test: `crates/hoimin-cli/src/workspace/copy.rs`
- Test: `crates/hoimin-cli/tests/workspace_handler.rs`

**Interfaces:**
- Consumes: Task 2's plan-scoped metrics and Task 1's `IntegrityCheckpoint::PostMaterialization`.
- Produces: production `materialize_worker` with one snapshot read per entry, contracts-only size/hash validation, and wave-level changed-original coverage.

- [ ] **Step 1: Change the deterministic expectation to the optimized contract**

```rust
let expected_snapshot_hash_bytes = if cfg!(feature = "contracts") {
    2 * fixture_bytes
} else {
    0
};
assert_eq!(
    plan.materialization_io_snapshot(),
    MaterializationIoSnapshot {
        original_manifest_builds: 1,
        original_bytes: fixture_bytes,
        original_hash_bytes: fixture_bytes,
        snapshot_bytes: 2 * fixture_bytes,
        snapshot_hash_bytes: expected_snapshot_hash_bytes,
    }
);
assert_eq!(plan.observed_copy_bytes(), 2 * fixture_bytes);
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p hoimin-cli materialization_io_counts_full_tree_work -- --nocapture`

Expected: FAIL with Task 2's baseline counts, proving the regression detects each redundant pass.

- [ ] **Step 3: Implement the minimal materialization path**

Delete `verify_originals_for_materialization` and both calls to `verify_originals` from `materialize_worker_with_root_opener`. Read the snapshot metadata before the bytes. Keep size/hash validation only under contracts:

```rust
#[cfg(feature = "contracts")]
{
    #[cfg(test)]
    self.materialization_metrics.record_snapshot_hash(bytes.len());
    let amount = u64::try_from(bytes.len()).map_err(|_| WorkspaceError::CopySizeOverflow)?;
    if amount != entry.size || blake3::hash(&bytes) != entry.blake3 {
        return Err(WorkspaceError::WorkspaceRestore {
            path: entry.path.clone(),
            message: "shared snapshot does not match its manifest".to_owned(),
        });
    }
}
```

Normal builds execute no BLAKE3 during materialization. Preserve allowance charging, `charged` accumulation and rollback, directories, writes, and permissions.

- [ ] **Step 4: Add handler-level wave change detection**

In `workspace_handler.rs`, preflight and create every worker, mutate an original file, then call:

```rust
handler.handle_verify_originals(VerifyOriginals {
    id: EffectId(40),
    checkpoint: IntegrityCheckpoint::PostMaterialization,
})
```

Assert the failure code is `workspace.original.changed`. Together with Task 1, this proves the single scheduled boundary uses the existing typed verifier.

- [ ] **Step 5: Run focused normal and contracts tests**

Run: `cargo test -p hoimin-cli materialization_io_counts_full_tree_work -- --nocapture`

Run: `cargo test -p hoimin-cli --features contracts materialization_io_counts_full_tree_work -- --nocapture`

Run: `cargo test -p hoimin-cli --test workspace_handler post_materialization_verification_rejects_changed_original -- --nocapture`

Expected: all PASS. Normal metrics show zero snapshot hash bytes; contracts metrics show one snapshot hash pass per worker.

- [ ] **Step 6: Run the optimized benchmark five times and commit**

Run five times: `cargo test --release -p hoimin-cli benchmark_worker_materialization_io -- --ignored --nocapture`

Expected deterministic counts for eight workers: one original manifest/hash pass, eight snapshot reads, zero normal snapshot hash bytes, and eight worker copies. Record all elapsed values.

```bash
git add crates/hoimin-cli/src/workspace/copy.rs crates/hoimin-cli/tests/workspace_handler.rs
git commit -m "perf(workspace): remove redundant materialization hashes"
```

---

### Task 4: Document evidence and verify the complete change

**Files:**
- Create: `docs/superpowers/reports/2026-08-05-issue-128-worker-materialization.md`

**Interfaces:**
- Consumes: Task 2 baseline metrics/timings and Task 3 optimized metrics/timings.
- Produces: reproducible performance report and final verification evidence for the PR.

- [ ] **Step 1: Write the performance report**

Document the outcome, safety boundary, production versus contracts behavior, identical benchmark command and fixture, five elapsed runs per revision, median comparison, deterministic before/after table, scope, and verification commands. Note that current main performed three original full-data passes plus one snapshot read/hash per worker, exceeding Issue #128's original estimate.

- [ ] **Step 2: Run focused and full verification**

```bash
cargo fmt --check
cargo test -p hoimin-core --test machine
cargo test -p hoimin-cli materialization -- --nocapture
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

Expected: every command exits 0. The worktree-local `.venv` symlink remains untracked.

- [ ] **Step 3: Commit the report**

```bash
git add docs/superpowers/reports/2026-08-05-issue-128-worker-materialization.md
git commit -m "docs: report issue 128 performance results"
```

- [ ] **Step 4: Obtain independent review and resolve every finding**

Use `superpowers:requesting-code-review` against `origin/main...HEAD`. Verify each finding technically, apply accepted fixes with focused regression tests, and repeat review until no actionable findings remain.

- [ ] **Step 5: Push, create the linked PR, monitor CI, and merge**

```bash
git push -u origin perf/issue-128-worker-materialization-hash
gh pr create --base main --head perf/issue-128-worker-materialization-hash --title "perf(workspace): eliminate redundant materialization hashes" --body-file /private/tmp/hoimin-issue-128-pr-body.md
gh pr checks --watch
gh pr merge --squash --delete-branch
```

The PR body must include `Closes #128`, deterministic before/after counts, benchmark medians, safety boundary, contracts behavior, and verification commands. Merge only after every CI check passes and independent review has no actionable findings.
