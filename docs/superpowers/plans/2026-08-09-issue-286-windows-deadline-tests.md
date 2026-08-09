# Issue #286 Windows Deadline Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop Windows CI from charging teardown work to shutdown deadlines while preserving every production timeout and behavioral assertion.

**Architecture:** Measure the subprocess completion milestone before descendant observation, use the existing Tokio timeout result before controller teardown, and establish blocking-task ownership before starting the preemption test's budget. A private helper extraction reuses the unchanged production wait/classification logic for that deterministic test.

**Tech Stack:** Rust 2024, Tokio, Cargo integration tests, GitHub Actions Windows runners

## Global Constraints

- Do not change production behavior, timeout values, shutdown grace, public APIs, or report formats.
- Keep descendant cleanup and pause-controller teardown assertions.
- Preserve exit-code, diagnostic, session, and incomplete-report assertions.
- Keep the fix in the Issue #286 worktree and squash merge its PR.

---

### Task 1: Measure locked-session child exit before descendant observation

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:1610-1690`

**Interfaces:**
- Consumes: `child.wait() -> std::process::ExitStatus` and `started_at: Instant`
- Produces: `child_exit_elapsed: Duration`, returned with the scenario observation

- [ ] **Step 1: Preserve the CI failure as Red evidence**

Record that run `31313554038`, attempt 1, failed with
`elapsed=10.1697482s` after `child.wait()` had already returned. The existing
test cannot be made deterministically Red locally because the defect is runner
scheduling dependent; this captured CI execution is the failing test run.

- [ ] **Step 2: Capture the semantic milestone**

Immediately after the bounded wait, capture elapsed time:

```rust
let status = tokio::time::timeout(Duration::from_secs(9), child.wait())
    .await
    .map_err(|_| "hoimin exceeded total timeout plus shutdown grace".to_owned())?
    .map_err(|error| error.to_string())?;
let child_exit_elapsed = started_at.elapsed();
let descendant_stopped = fixture_processes
    .as_ref()
    .expect("assigned above")
    .descendant
    .wait_until_stops(Duration::from_secs(2))
    .await;
```

Return and destructure `child_exit_elapsed`, then assert:

```rust
assert!(
    child_exit_elapsed < Duration::from_secs(9),
    "child_exit_elapsed={child_exit_elapsed:?}"
);
```

- [ ] **Step 3: Run the focused E2E repeatedly**

Run five times:

```bash
cargo test -p hoimin-cli --test run_e2e total_timeout_exits_after_grace_when_session_finish_is_locked -- --exact
```

Expected: all five invocations pass, including descendant and session checks.

### Task 2: Keep controller teardown outside the paused-run deadline

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs:3590-3635`

**Interfaces:**
- Consumes: `tokio::time::timeout(Duration::from_millis(500), &mut run)`
- Produces: the same bounded `Result`, with controller teardown performed after observation

- [ ] **Step 1: Preserve the CI failure as Red evidence**

Record that run `31313554038`, attempt 2, entered the timeout `Ok` branch and
then failed `observed_at.elapsed() < Duration::from_millis(500)` after
controller teardown. This proves the redundant assertion measured a wider
interval than the operation under test.

- [ ] **Step 2: Remove only the wider redundant measurement**

Delete `let observed_at = Instant::now();` and the later elapsed assertion.
Keep the explicit timeout, release, join, timeout error branch, and semantic
error assertions unchanged:

```rust
let result = tokio::time::timeout(Duration::from_millis(500), &mut run).await;
release_tx.send(()).unwrap();
controller.await.unwrap();
let Ok(result) = result else {
    let _ = tokio::time::timeout(Duration::from_secs(3), &mut run).await;
    panic!("paused materialization outlived the first shutdown deadline");
};
let error = result.unwrap_err();
```

- [ ] **Step 3: Run the focused unit test repeatedly**

Run five times:

```bash
cargo test -p hoimin-cli --lib shell::tests::total_timeout_bounds_paused_materialization_at_first_shutdown_deadline -- --exact
```

Expected: all five invocations pass while checking the 500 ms Tokio timeout.

### Task 3: Verify and deliver the isolated fix

Before delivery, make the already-owned blocking-close premise deterministic.

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs:552-575`
- Modify: `crates/hoimin-cli/src/shell.rs:2948-2975`

**Interfaces:**
- Consumes: `&ShutdownBudget` and `&mut JoinHandle<Option<T>>`
- Produces: `await_owned_blocking_until<T> -> Result<T, OwnedBlockingError>`

- [ ] **Step 1: Preserve the additional CI failure as Red evidence**

Record that PR #287 run `31315274590` failed because the 20 ms budget expired
before `spawn_blocking` entered the operation, dropping `entered_tx` and
producing `RecvError`. The test's claimed "owned" premise was not established.

- [ ] **Step 2: Extract the existing wait logic without changing it**

Move the `budget.wait`, join-error, and expiry branches to:

```rust
async fn await_owned_blocking_until<T>(
    budget: &ShutdownBudget,
    task: &mut tokio::task::JoinHandle<Option<T>>,
) -> Result<T, OwnedBlockingError>
where
    T: Send + 'static,
```

Keep `run_owned_blocking_until` responsible for spawning the operation and
delegate its join handle to this helper.

- [ ] **Step 3: Establish ownership before starting the test budget**

Spawn the shaped blocking task directly, await its readiness signal, construct
the same 20 ms cancellation budget, and call `await_owned_blocking_until`.
Release the blocking task after expiry and retain the exact expiry diagnostics.

- [ ] **Step 4: Run the focused unit test repeatedly**

Run five times:

```bash
cargo test -p hoimin-cli --lib shell::tests::shutdown_budget_preempts_an_owned_blocking_close -- --exact
```

Expected: all five invocations pass with a pending blocking-I/O count of one.

### Task 4: Verify and deliver the isolated fix

**Files:**
- Verify: `crates/hoimin-cli/src/shell.rs`
- Verify: `crates/hoimin-cli/tests/run_e2e.rs`
- Verify: `docs/superpowers/specs/2026-08-09-issue-286-windows-deadline-tests-design.md`
- Verify: `docs/superpowers/plans/2026-08-09-issue-286-windows-deadline-tests.md`

**Interfaces:**
- Consumes: the two corrected test observations
- Produces: a test-only PR that closes Issue #286

- [ ] **Step 1: Run formatting and static analysis**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: both commands exit zero.

- [ ] **Step 2: Run full Rust verification**

```bash
cargo test --workspace
cargo test -p hoimin-cli --test run_e2e
```

Expected: both commands exit zero with only documented ignored tests.

- [ ] **Step 3: Check the patch and commit**

```bash
git diff --check
git status --short
git add crates/hoimin-cli/src/shell.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "test(windows): measure semantic shutdown deadlines"
```

Expected: the implementation commits contain the three test corrections and
the behavior-preserving private helper extraction; design and plan remain in
documentation-only commits.

- [ ] **Step 4: Push, open, verify, and squash merge the PR**

Push `fix/issue-286-windows-deadline-tests`, open a PR with `Fixes #286`, wait
for all required checks including Windows Rust, then squash merge and delete
the remote branch. Finally fast-forward main and remove this worktree.
