# RunFinished Terminal Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make final report emission terminal across the run state machine and report-sequence validator.

**Architecture:** Add an early terminal guard for stop events in `transition`, retaining the pending final-output acknowledgement. Extend `ReportSequence` with explicit terminal state and typed lifecycle errors while preserving transactional validation.

**Tech Stack:** Rust 2024, `thiserror`, existing `hoimin-core` integration tests.

## Global Constraints

- Do not change public JSON event shapes or schema versions.
- Do not retire or duplicate the pending final-output effect.
- Preserve the selected final exit code and completeness once `RunFinished` emission starts.
- Follow test-driven development: each behavior must fail before production code changes.

---

### Task 1: State-machine terminal stop semantics

**Files:**
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-core/src/machine.rs`

**Interfaces:**
- Consumes: `transition(RunState, RunEvent) -> Result<(RunState, Vec<RunEffect>), MachineError>`
- Produces: cancellation/deadline no-op behavior during pending final output and in `RunPhase::Finished`

- [ ] **Step 1: Add failing pending-final-output tests**

Add a helper that drives the existing no-candidate fixture through cleanup until the `EmitOutput(OutputEvent::RunFinished(_))` effect is pending. For both stop events, assert:

```rust
let original_exit = state.exit_code();
let (state, effects) = transition(state, stop).unwrap();
assert!(effects.is_empty());
assert_eq!(state.exit_code(), original_exit);
assert_eq!(state.phase(), RunPhase::Finalize);
assert!(!state.is_effect_retired(finished_id));
```

Then acknowledge `finished_id` and assert `RunPhase::Finished`.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```console
cargo test -p hoimin-core --test machine stop_signals_do_not_reopen_final_report -- --exact
```

Expected: failure because the stop event retires the final output and schedules cleanup.

- [ ] **Step 3: Add failing finished-state coverage**

After acknowledging the final output, inject cancellation and deadline independently and assert no effects, unchanged phase, and unchanged exit code.

- [ ] **Step 4: Implement the minimal early guard**

Before the existing stop-event arms mutate flags, return the unchanged state with no effects when:

```rust
matches!(event, RunEvent::DeadlineReached | RunEvent::CancellationRequested)
    && (state.run_finished_output_id.is_some() || state.phase == RunPhase::Finished)
```

Keep normal stop handling unchanged in every earlier phase.

- [ ] **Step 5: Run focused and core machine tests**

Run:

```console
cargo test -p hoimin-core --test machine
```

Expected: all tests pass.

### Task 2: ReportSequence terminal validation

**Files:**
- Modify: `crates/hoimin-core/tests/report_policy.rs`
- Modify: `crates/hoimin-core/src/report.rs`

**Interfaces:**
- Consumes: `ReportSequence::observe(&OutputEvent) -> Result<(), ReportSequenceError>`
- Produces: `RunAlreadyFinished` and `RunFinishedWithActiveMutants` errors

- [ ] **Step 1: Add failing report lifecycle tests**

Build a valid sequence ending in `RunFinished`, then assert a diagnostic and a second terminal event both return `RunAlreadyFinished`. Start a mutant without finishing it and assert the first terminal event returns `RunFinishedWithActiveMutants`.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```console
cargo test -p hoimin-core --test report_policy run_finished -- --nocapture
```

Expected: compilation/test failure because the terminal errors and validation do not exist.

- [ ] **Step 3: Implement typed terminal tracking**

Add:

```rust
RunAlreadyFinished { run_id: String },
RunFinishedWithActiveMutants { count: usize },
```

and `finished: bool` to `ReportSequence`. Reject every event when `finished` is already true. Reject the first `RunFinished` when `active_mutants` is nonempty. Set `finished = true` only after all validation succeeds.

- [ ] **Step 4: Run report tests in both contract modes**

Run:

```console
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-core --features contracts --test report_policy
```

Expected: all tests pass.

### Task 3: Full verification and delivery

**Files:**
- Verify all modified files and documentation.

**Interfaces:**
- Consumes: Tasks 1 and 2
- Produces: reviewable branch and PR closing issue #52

- [ ] **Step 1: Run formatting and lint gates**

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- [ ] **Step 2: Run the workspace and contract suites**

```console
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
```

- [ ] **Step 3: Review the diff against issue #52**

Confirm one terminal output, preserved outcome, active-mutant rejection, and no schema changes.

- [ ] **Step 4: Commit, push, and open the PR**

```console
git add docs/superpowers/specs/2026-07-29-run-finished-terminal-design.md \
  docs/superpowers/plans/2026-07-29-run-finished-terminal.md \
  crates/hoimin-core/src/machine.rs crates/hoimin-core/src/report.rs \
  crates/hoimin-core/tests/machine.rs crates/hoimin-core/tests/report_policy.rs
git commit -m "fix: make run finished terminal"
git push -u origin fix/issue-52-run-finished-terminal
gh pr create --base main --head fix/issue-52-run-finished-terminal
```
