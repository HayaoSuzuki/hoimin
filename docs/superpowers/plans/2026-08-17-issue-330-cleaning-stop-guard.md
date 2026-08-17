# Issue #330: Stop events during normal cleanup implementation plan

**Goal:** Prevent a first deadline or cancellation during normal cleanup from
retiring cleanup and re-running finalization against a consumed workspace plan.

**Architecture:** Treat `Cleaning` as terminal teardown in the machine's
existing early stop-event guard. Keep shell and workspace behavior unchanged.

**Tech stack:** Rust state machine, Lean 4 oracle, Cargo tests, focused mutation
testing.

## Task 1: Add the successful-cleanup regression

**Files:**

- Modify: `crates/hoimin-core/tests/machine.rs`

1. Add a helper that advances an empty-candidate successful run through
   analysis and pre-final verification until `Cleanup` is pending.
2. Add a table-driven test for `DeadlineReached` and
   `CancellationRequested`.
3. Assert no replacement effects, no exit-code change, and preservation of the
   original pending cleanup.
4. Complete the original cleanup and assert normal final reporting continues.
5. Run the exact test and confirm the current guard fails these assertions.

## Task 2: Generalize the cleaning guard

**Files:**

- Modify: `crates/hoimin-core/src/machine.rs`

1. Change the terminal stop-event guard to ignore stop events in every
   `Cleaning` state.
2. Keep the other guarded states unchanged.
3. Run the new regression and the existing
   `lean_oracle_regression_cleanup_is_emitted_once` test.

## Task 3: Extend the Lean state-machine oracle

**Files:**

- Modify: `formal/HoiminOracle/HoiminOracle/Model.lean`
- Modify: `formal/HoiminOracle/HoiminOracle/Proofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle/Cases.lean`
- Regenerate: `formal/HoiminOracle/corpus/state-machine.jsonl`
- Modify: `crates/hoimin-core/tests/lean_oracle.rs`
- Create: `docs/superpowers/reports/2026-08-17-issue-330-cleaning-stop-guard-oracle.md`

1. State the cleaning no-op theorem first and confirm it fails under the old
   model.
2. Make cleaning stops unconditional no-ops in the model and prove the claim.
3. Add a broken old-behavior witness and strict deadline/cancellation cases.
4. Regenerate the corpus from Lean; do not edit JSONL by hand.
5. Drive a successful normal-cleaning scenario through the public Rust API and
   require strict correspondence.
6. Record proof scope, adapter observations, resource limits, and reproduction
   commands in a self-contained report.

## Task 4: Verify and review

1. Run formatting, clippy, Rust workspace tests, and Python tests.
2. Run focused mutation testing for the changed cleaning condition.
3. Check the diff and request an independent review against Issue #330.
4. Address all important findings and rerun affected verification.

## Task 5: Deliver and clean up

1. Commit the design, plan, implementation, and tests.
2. Push the issue branch and open a PR that closes #330.
3. Merge after review and confirm all CI succeeds.
4. Fast-forward local main, rerun the focused regression, and remove the issue
   branch and worktree.
