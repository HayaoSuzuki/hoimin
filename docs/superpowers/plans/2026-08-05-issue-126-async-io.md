# Issue #126 Async Filesystem Dispatch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dispatch worker workspace and candidate-spool filesystem operations through owned blocking tasks so independent workers overlap without blocking the async run loop.

**Architecture:** The main task transfers each affected `WorkerWorkspace` into a `WorkspaceTask` and restores its returned state before applying the event transition. The run loop schedules those tasks and candidate reads through `spawn_blocking`, tracks them in a separate I/O `JoinSet`, and keeps process metrics and state transitions on the main task.

**Tech Stack:** Rust 2024, Tokio `spawn_blocking`, Tokio `JoinSet` and bounded MPSC, capability-relative workspace APIs, Cargo tests.

## Global Constraints

- Only the main async task may mutate `RunState`, `WorkspaceHandler` maps, or `active_candidates`.
- Preserve typed effect IDs, workspace failures, reset discard cleanup, capability boundaries, candidate ordering, and process-only concurrency metrics.
- A running blocking OS operation is drained rather than force-cancelled during shutdown.
- Keep preflight and final cleanup serial as global workspace lifetime boundaries.
- Keep this PR scoped to #126; replay format, materialization, and line indexing remain #127–#129.
- Use deterministic concurrency assertions as regression gates and elapsed timing only as supporting evidence.

---

### Task 1: Add a tested owned blocking-task primitive

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Consumes: an `EffectId` and `FnOnce() -> T + Send + 'static`.
- Produces: `run_blocking_io<T>(id, operation) -> Result<T, EffectFailed>` where `T: Send + 'static`; join failures use code `shell.blocking_io` and retain `id`.

- [ ] **Step 1: Write RED tests for runtime responsiveness and join identity**

Add Tokio tests that hold a blocking closure on a standard channel, assert a short Tokio timer completes before releasing it, and assert a panicking closure returns an `EffectFailed` with the supplied ID and `shell.blocking_io` code.

- [ ] **Step 2: Run RED**

Run: `cargo test -p hoimin-cli shell::tests::blocking_io --lib -- --nocapture`

Expected: FAIL because `run_blocking_io` does not exist.

- [ ] **Step 3: Implement the minimal wrapper**

Implement:

```rust
async fn run_blocking_io<T>(
    id: EffectId,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, EffectFailed>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| EffectFailed::other(id, "shell.blocking_io", error.to_string()))
}
```

- [ ] **Step 4: Run GREEN and commit**

Run: `cargo test -p hoimin-cli shell::tests::blocking_io --lib -- --nocapture`

```bash
git add crates/hoimin-cli/src/shell.rs
git commit -m "test(shell): define blocking I/O task boundary"
```

### Task 2: Transfer workspace ownership through task completions

**Files:**
- Modify: `crates/hoimin-cli/src/workspace/copy.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Test: `crates/hoimin-cli/tests/workspace_handler.rs`
- Test: `crates/hoimin-cli/tests/workspace_recovery.rs`

**Interfaces:**
- Consumes: `CreateWorker`, `ApplyMutation`, `ResetWorker`, `VerifyOriginals`, and an optional active worker removed from `WorkspaceHandler`.
- Produces: `WorkspaceTask`, `WorkspaceTaskCompletion { event: RunEvent, update: WorkspaceTaskUpdate }`, `WorkspaceHandler::prepare_task`, and `WorkspaceHandler::accept_task_completion`.

- [ ] **Step 1: Write RED ownership round-trip tests**

Add tests that prepare an apply task, observe that the worker is unavailable while owned, execute and accept the completion, then observe the worker and mutated bytes again. Add the same cycle for successful reset and for reset failure with cleanup failure, asserting the existing pending-cleanup count and combined error behavior.

- [ ] **Step 2: Run RED**

Run: `cargo test -p hoimin-cli --test workspace_handler workspace_task -- --nocapture`

Run: `cargo test -p hoimin-cli --test workspace_recovery workspace_task -- --nocapture`

Expected: FAIL because the task preparation and acceptance APIs do not exist.

- [ ] **Step 3: Make `WorkspacePlan` cheaply shareable**

Derive `Clone` for `WorkspacePlan` and store `WorkspaceHandler::plan` as `Option<Arc<WorkspacePlan>>`. Keep all public query behavior unchanged by dereferencing the `Arc`.

- [ ] **Step 4: Implement owned task variants**

Define shell-internal variants for create, apply, reset, and verify. `prepare_task` performs map validation and removes worker ownership. `WorkspaceTask::execute` calls the existing workspace operation and constructs the same typed event or failure. Reset performs the existing discard cleanup and returns either active, pending-cleanup, or no worker state.

- [ ] **Step 5: Accept returned state on the main task**

`accept_task_completion` inserts the returned worker into exactly one map and returns the completion's `RunEvent`. Reject duplicate returned state as a stable `shell.blocking_io` failure instead of overwriting a live worker.

- [ ] **Step 6: Run GREEN and focused workspace suites**

Run: `cargo test -p hoimin-cli --test workspace_handler`

Run: `cargo test -p hoimin-cli --test workspace_recovery`

Run: `cargo test -p hoimin-cli workspace::copy --lib`

- [ ] **Step 7: Commit**

```bash
git add crates/hoimin-cli/src/workspace/copy.rs crates/hoimin-cli/src/workspace/mod.rs crates/hoimin-cli/tests/workspace_handler.rs crates/hoimin-cli/tests/workspace_recovery.rs
git commit -m "refactor(workspace): transfer workers through I/O tasks"
```

### Task 3: Prepare all five blocking effects through one shell boundary

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Consumes: `RunEffect::{CreateWorker, ReadCandidate, ApplyMutation, ResetWorker, VerifyOriginals}`.
- Produces: `BlockingEffect`, `BlockingEffectCompletion`, `prepare_blocking_effect`, `execute_direct_io_effect`, and `accept_blocking_completion`.

- [ ] **Step 1: Write RED classification and state-acceptance tests**

Assert that exactly the five issue variants return true from `is_blocking_io_effect`. Exercise candidate replay completion and assert `active_candidates` changes only after `accept_blocking_completion`, not during task execution.

- [ ] **Step 2: Run RED**

Run: `cargo test -p hoimin-cli shell::tests::blocking_effect --lib -- --nocapture`

Expected: FAIL because the blocking-effect boundary does not exist.

- [ ] **Step 3: Implement preparation and execution**

Create an owned enum whose workspace variants contain `WorkspaceTask` and whose candidate variant contains the request. Execution calls `WorkspaceTask::execute` or `CandidateStore::replay_one` and returns a typed event plus optional workspace update.

- [ ] **Step 4: Implement main-task acceptance**

Accept workspace state first. Then update `active_candidates` for `CandidateLoaded`, remove it only for successful `WorkerReset`, and preserve the existing apply insertion timing.

- [ ] **Step 5: Keep direct `execute_effect` calls cancellation-safe**

Keep the five direct-call match arms synchronous against the borrowed context, with candidate-map updates matching production acceptance. Do not remove a worker and then await a detached blocking task: callers may drop the direct-effect future. The production run loop uses the owned blocking path; existing direct callers retain one-event and worker-ownership semantics.

- [ ] **Step 6: Run GREEN and shell tests**

Run: `cargo test -p hoimin-cli shell::tests --lib`

Run: `cargo test -p hoimin-cli --test run_e2e`

- [ ] **Step 7: Commit**

```bash
git add crates/hoimin-cli/src/shell.rs
git commit -m "refactor(shell): own blocking filesystem effects"
```

### Task 4: Dispatch blocking I/O concurrently in the production loop

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Consumes: prepared `BlockingEffect`, the existing completion channel, and cancellation/deadline branches.
- Produces: a separate `JoinSet<()>` for I/O tasks, `spawn_blocking_effect`, independent process/I/O completion markers, and shutdown draining for both task sets.

- [ ] **Step 1: Write a failing overlap regression**

Use two controlled blocking operations that each signal entry and wait on separate release receivers. Schedule both through `spawn_blocking_effect` and require both entry signals before releasing either. Assert the tracked maximum I/O completions in flight is two while process-task metrics remain zero.

- [ ] **Step 2: Run RED**

Run: `cargo test -p hoimin-cli shell::tests::blocking_io_effects_overlap --lib -- --nocapture`

Expected: FAIL because the production scheduler still awaits non-process effects serially.

- [x] **Step 3: Track process and I/O completion origins independently**

Add independent internal process-task and I/O-task markers. A serial completion sets neither marker, a process completion sets only the process marker, and a blocking-I/O completion sets only the I/O marker. Keep process metrics conditional on the process marker and process metadata.

- [ ] **Step 4: Schedule ready blocking effects without awaiting**

Prepare each recognized effect on the main task, spawn its owned synchronous execution through `spawn_blocking`, send its completion to the bounded channel, increment total in-flight and I/O-in-flight counts, and continue dispatching ready effects.

- [x] **Step 5: Accept and join by task marker**

For I/O completions, restore workspace/candidate state, decrement I/O and total counts, and join one I/O wrapper task before transition. For process completions, preserve the current metrics and process `JoinSet` behavior.

- [ ] **Step 6: Drain both task sets on every stop path**

Extend shutdown draining to await process tasks and non-abortable blocking tasks, accept all returned workspace state, discard stale events after metrics accounting, and then allow `WorkspaceHandler::close` to run.

- [ ] **Step 7: Run GREEN and concurrency-sensitive suites**

Run: `cargo test -p hoimin-cli shell::tests --lib`

Run: `cargo test -p hoimin-cli --test run_e2e`

Run: `cargo test -p hoimin-cli --test workspace_recovery`

- [ ] **Step 8: Commit**

```bash
git add crates/hoimin-cli/src/shell.rs
git commit -m "perf(shell): dispatch filesystem effects concurrently"
```

### Task 5: Benchmark, document, verify, review, and integrate

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`
- Create: `docs/superpowers/reports/2026-08-05-issue-126-async-io.md`

**Interfaces:**
- Consumes: deterministic overlap counters, four fixed-duration controlled operations, and full verification output.
- Produces: an ignored benchmark, issue report, and a `Closes #126` PR.

- [ ] **Step 1: Add and run the ignored scheduler benchmark**

Compare four 50 ms controlled blocking operations executed serially and through the production blocking-I/O scheduler. Print operation count, serial elapsed, concurrent elapsed, and maximum I/O in flight. Run in release mode five times; do not assert elapsed thresholds.

Run: `cargo test --release -p hoimin-cli benchmark_blocking_io_dispatch -- --ignored --nocapture`

- [ ] **Step 2: Write the report**

Document the rejected awaited/global-lock designs, ownership-transfer correctness boundary, exact deterministic overlap result, five timing runs, and shutdown limitation for already-running OS calls.

- [ ] **Step 3: Run full verification**

Run: `cargo fmt --check`

Run: `cargo test --workspace`

Run: `cargo test --workspace --all-features`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Run: `git diff --check origin/main...HEAD`

- [ ] **Step 4: Request independent review**

Review `origin/main..HEAD` for worker ownership loss, duplicate completion handling, cancellation draining, process metrics, typed IDs, candidate ordering, and platform behavior. Resolve every Critical or Important finding.

- [ ] **Step 5: Commit, push, create PR, monitor CI, and merge**

```bash
git add crates/hoimin-cli/src/shell.rs docs/superpowers/reports/2026-08-05-issue-126-async-io.md
git commit -m "docs: report issue 126 performance results"
git push -u origin perf/issue-126-async-io
gh pr create --base main --head perf/issue-126-async-io --title "perf(shell): dispatch filesystem effects concurrently" --body-file /private/tmp/hoimin-issue-126-pr-body.md
gh pr checks --watch
gh pr merge --squash --delete-branch
```

After confirming the merge commit and issue closure, fast-forward local `main`, remove the merged worktree, and delete remaining local and remote branches.
