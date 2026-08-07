# Issue 260 Bounded Shutdown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound every await after total-timeout, cancellation, or fatal shutdown begins, while retaining orderly cleanup inside a fixed two-second grace.

**Architecture:** Introduce one immutable monotonic shutdown budget in the shell. Reuse it for interrupted serial effects, stopped completion receives, and every process/blocking-I/O drain; on expiry, accept buffered ownership returns, abort Tokio wrappers, and return a cause-preserving infrastructure error. Prove the real CLI bound with a lock-held SQLite JSONL E2E and document the public timeout-plus-grace contract.

**Tech Stack:** Rust 2024, Tokio `select!`/`timeout_at`/`JoinSet`, rusqlite, cross-platform CLI E2E tests, Markdown user/developer documentation.

## Global Constraints

- Production shutdown grace is fixed at exactly two seconds and is not a new CLI/config/schema/fingerprint field.
- Total-timeout shutdown ends no later than the original total deadline plus grace; observing the deadline late never extends it.
- Cancellation/first interrupt and fatal shutdown get one grace starting at first observation; later events never restart it.
- Normal total timeout remains exit 4 with an incomplete report when cleanup succeeds; grace expiry is infrastructure exit 2 with the initiating cause and `shutdown grace expired` in stderr.
- Second real Ctrl+C retains unconditional immediate exit 130.
- Buffered workspace completions are accepted before wrapper tasks are abandoned; no unobserved cleanup is reported complete.
- Report/event/session schemas, state-machine events, candidate outcomes, timeout configuration, and session compatibility remain unchanged.
- Behavior-changing commits do not use `[skip ci]`; only pure documentation commits may use it.

## File Map

- `crates/hoimin-cli/src/shell.rs`: shutdown budget, bounded waits/drains, task abandonment, deterministic unit tests.
- `crates/hoimin-cli/tests/run_e2e.rs`: real lock-held SQLite total-timeout regression and existing interrupt coverage.
- `README.md`: public timeout-plus-grace and exit semantics.
- `docs/development.md`: shutdown invariant and maintainer guidance.
- `docs/superpowers/specs/2026-08-07-issue-260-bounded-shutdown-design.md`: approved design committed before this plan.

---

### Task 1: Give shutdown drains one immutable deadline

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Adds private `SHUTDOWN_GRACE: Duration = Duration::from_secs(2)`.
- Adds private shutdown cause/budget types with absolute deadlines and stable expiry formatting.
- Changes private `drain_processes` to require the active shutdown budget.

- [ ] **Step 1: Write failing shutdown-budget and drain tests**

Add focused unit tests that construct short test budgets without changing the production constant:

- total-timeout budget uses `run_deadline + grace`, including when created after that deadline;
- a second cause/budget initialization cannot extend an existing deadline;
- an unreleased process task and blocking-I/O wrapper cause `drain_processes` to return at the shared deadline with both task counts in the error;
- a completion already buffered at expiry is accepted and decrements metrics/in-flight accounting before task wrappers are aborted;
- update `shutdown_drain_accepts_owned_workspace_state_without_process_metrics` and `drain_closes_failed_and_cancelled_process_metrics` to pass a generous test budget and retain their exact successful behavior.

- [ ] **Step 2: Run focused tests and verify RED**

Run: `cargo test -p hoimin-cli --lib shell::tests::shutdown_budget -- --show-output`

Run: `cargo test -p hoimin-cli --lib shell::tests::shutdown_drain -- --show-output`

Expected: compile/test failure because the shutdown budget and bounded drain interface do not exist.

- [ ] **Step 3: Implement the budget and bounded drain**

The budget must carry a stable cause label (`total timeout`, `cancellation`, or `failure`) and an absolute Tokio instant. Its wait helper uses `timeout_at` and never constructs a fresh relative timeout.

On drain expiry:

1. record process and blocking-I/O wrapper counts;
2. consume all `receiver.try_recv()` completions and accept returned blocking ownership;
3. abort both `JoinSet`s without awaiting them;
4. set in-flight accounting to zero;
5. return a stable error containing cause, `shutdown grace expired`, `2s`, and both counts.

Preserve the first join failure when drain completes inside the deadline.

- [ ] **Step 4: Run shell tests and verify GREEN**

Run: `cargo test -p hoimin-cli --lib shell::tests -- --show-output`

Expected: all shell tests pass; released owned workspace state still returns to the handler.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/shell.rs
git commit -m "fix: bound shutdown task draining"
```

---

### Task 2: Apply the shared budget across the run loop

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- `run_loop_prepared` stores at most one active shutdown budget.
- Test-only `RunControl` support may override grace duration so deterministic tests do not sleep for two seconds; production remains fixed.

- [ ] **Step 1: Write failing run-loop regressions**

Use the existing controlled materialization pause to hold a real blocking workspace operation. With a short test-only grace:

- configure a total deadline, let the paused operation remain unreleased, and assert the run future returns a `total timeout: shutdown grace expired` error within a bounded test timeout;
- assert the error reports a blocking-I/O wrapper and the first absolute deadline;
- release the paused OS operation after the run future returns so the Tokio test runtime can shut down cleanly;
- add a cancellation variant proving a second cancellation observation does not extend the first grace;
- retain an orderly release-inside-grace case that returns the normal timeout/cancellation exit rather than infrastructure failure.

Also extend the existing interrupt fixture spawn helper to accept extra run
arguments and add
`total_timeout_exits_after_grace_when_session_finish_is_locked`. Start a JSONL
session run with a short total timeout, continuously drain stdout/stderr, wait
for `mutant_started`, retain the descendant handle, and acquire a
zero-busy-timeout `BEGIN IMMEDIATE` lock from the test process before the run
deadline. Without sending an interrupt, require bounded exit 2, cause-specific
stderr, a parseable JSONL prefix without false `run_finished`, an incomplete
session row, and a stopped descendant. Always reap/kill fixtures on failure.

- [ ] **Step 2: Run focused tests and verify RED**

Run the new exact tests with `cargo test -p hoimin-cli --lib shell::tests::<name> -- --exact --nocapture`.

Run: `cargo test -p hoimin-cli --test run_e2e total_timeout_exits_after_grace_when_session_finish_is_locked -- --exact --nocapture`

Expected: the unit run remains pending, and the real child waits for SQLite's
longer busy timeout or exceeds the total-timeout-plus-grace envelope.

- [ ] **Step 3: Integrate the budget into every shutdown path**

Establish the budget exactly once when deadline, `RunControl`, first interrupt, failed effect, task join failure, signal failure, or transition failure first starts shutdown.

- When deadline/cancellation/interrupt wins against an in-progress serial effect, cancel process work and await that effect only until the shared deadline before producing the stop event.
- Execute later serial cleanup effects through the same budget.
- Bound stopped `completion_rx.recv()` with the same deadline.
- Pass the same budget to every `drain_processes` call, including error-combination paths.
- Keep biased select ordering and second-interrupt monitor ownership unchanged.
- Combine a primary failure with grace expiry using the existing shutdown-error precedence; never replace the primary failure silently.

- [ ] **Step 4: Run shell and existing signal/timeout tests**

Run: `cargo test -p hoimin-cli --lib shell::tests -- --show-output`

Run: `cargo test -p hoimin-cli --test run_e2e total_timeout_cancels_and_reaps_descendants_before_cleanup -- --exact --nocapture`

Run platform-applicable first/second interrupt tests by exact name.

Expected: the unit and locked-session E2E pass; ordinary timeout remains exit
4; first and second interrupt semantics are unchanged; no fixture process leaks.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/shell.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "fix: enforce one shutdown grace"
```

---

### Task 3: Document the contract and run merge verification

**Files:**
- Modify: `README.md`
- Modify: `docs/development.md`

**Interfaces:**
- Documents fixed two-second grace and exit 2 versus exit 4 semantics.

- [ ] **Step 1: Re-run behavioral acceptance tests**

Run the new E2E plus:

- `total_timeout_cancels_and_reaps_descendants_before_cleanup`;
- platform-applicable first interrupt test;
- platform-applicable second interrupt test.

Expected: all pass without leaked fixture processes or invalid JSON prefixes.

- [ ] **Step 2: Update documentation**

In `README.md`, state that `--total-timeout` stops normal work at its deadline and permits at most two additional seconds for orderly shutdown. Clarify ordinary timeout exit 4 versus shutdown-grace-expiry exit 2. In `docs/development.md`, record the immutable shared-deadline invariant, buffered ownership acceptance, and detached blocking-call limitation.

- [ ] **Step 3: Run full verification**

Run: `cargo fmt --all -- --check`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Run: `cargo test --workspace`

Run: `.venv/bin/python -m unittest discover -s tests -p 'test_*.py'`

Run: `git diff --check origin/main...HEAD`

Expected: all non-platform-skipped tests pass; only the test-environment `.venv` symlink remains untracked before cleanup.

- [ ] **Step 4: Commit pure documentation**

```bash
git add README.md docs/development.md
git commit -m "docs: document bounded runtime shutdown [skip ci]"
```

The PR closes #260 and includes this design and implementation plan. Because the
last functional diff is followed by a `[skip ci]` docs commit, add a diff-empty
CI trigger commit before the first push if GitHub would otherwise skip the PR
workflow. Merge only after all repository CI passes, with the hard-cgroup job
allowed to remain at its configured skip state.
