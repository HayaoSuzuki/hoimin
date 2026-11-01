# Issue #331: Preserve queued effects across stop transitions implementation plan

**Goal:** Prevent a stop observed after queue pop from losing an effect the
machine still expects, especially the final report output.

**Architecture:** Temporarily restore the popped effect, then synchronize the
ready queue with the post-transition public pending-effect state. Keep the
machine authoritative for retirement and preserve existing shutdown drains.

**Tech stack:** Rust, Tokio scheduler loop, Cargo tests.

## Task 1: Add a deterministic final-report regression

**Files:**

- Modify: `crates/hoimin-cli/src/shell.rs`

1. Add a test-only `RunControl` hook that requests cancellation when
   `EmitOutput(RunFinished)` is popped before dispatch.
2. Run a successful zero-candidate project with that control under a bounded
   timeout.
3. Assert the hook fires, the call returns a normal exit rather than an error,
   and one parseable final report is written.
4. Run the exact test and confirm the current loop fails with
   `run stalled in Finalize`.

## Task 2: Preserve machine-pending queue entries

**Files:**

- Modify: `crates/hoimin-cli/src/shell.rs`

1. Push a just-popped effect back to the queue when the pre-dispatch stop check
   wins.
2. Replace unconditional post-stop queue draining with filtering based on
   `RunState::is_effect_pending`.
3. Keep the existing queued-mutant metric cancellation for removed effects.
4. Preserve queue order and append newly produced effects afterward.
5. Run the final-report regression and relevant shutdown/metrics tests.

## Task 3: Verify and review

1. Run formatting, clippy, Rust workspace tests, Python tests, and wheel smoke.
2. Run focused mutation testing for the pending-state filter when practical.
3. Check the complete diff and request independent review against Issue #331.
4. Address all important findings and rerun affected verification.

## Task 4: Deliver and clean up

1. Commit design, plan, implementation, and tests.
2. Push the issue branch and open a PR that closes #331.
3. Merge after review and confirm all CI succeeds.
4. Fast-forward local main, rerun the focused regression, then delete the
   branch and issue worktree.
