# Issue #332: Blocking preflight implementation plan

**Goal:** Make cancellation, deadline, and interrupt polling remain responsive
while preflight performs synchronous filesystem work.

**Architecture:** Treat preflight as an owned blocking effect. Move the
workspace plus immutable fingerprint inputs into the blocking task and restore
the workspace through the existing completion/drain boundary.

**Tech stack:** Rust, Tokio `spawn_blocking`, hoimin shell/workspace state
machines, Cargo tests.

## Task 1: Add a deterministic responsiveness regression

**Files:**

- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`

1. Add a test-only gate at the start of validated preflight.
2. Run preflight with `execute_effect` on a current-thread runtime while a
   short heartbeat future runs beside it.
3. Have a standard thread wait a bounded interval for the heartbeat, then
   release the gate.
4. Assert the heartbeat fires before release, the final event succeeds with a
   fingerprint, and workspace ownership is restored.
5. Run the exact test and confirm it fails against synchronous dispatch.

## Task 2: Move preflight to owned blocking dispatch

**Files:**

- Modify: `crates/hoimin-cli/src/shell.rs`

1. Add `Preflight` to `is_blocking_io_effect`.
2. Add an owned `BlockingEffect::Preflight` carrying the workspace, request,
   config, resolved targets, copied-input set, and resource mode.
3. Run validated preflight and source fingerprint construction inside
   `BlockingEffect::execute`.
4. Return the workspace and event together and restore ownership before the
   event reaches the machine.
5. Remove the synchronous preflight arm from serial effect execution.

## Task 3: Verify behavior and coverage

1. Run the new exact responsiveness test.
2. Run existing fingerprint ABA, shutdown drain, preflight, and end-to-end
   regressions.
3. Run focused mutation testing for blocking classification and owned
   completion restoration.
4. Run formatting, clippy, the Rust workspace, Python tests, and wheel smoke.
5. Inspect the full diff and request independent review.

## Task 4: Deliver and clean up

1. Commit the design, plan, implementation, and tests.
2. Push the issue branch and open a PR that closes #332.
3. Merge after review and confirm every CI job succeeds.
4. Fast-forward local main, rerun the focused regression, then remove the
   remote/local branch and issue worktree.
