# Persisting Cancellation Result Implementation Plan

> **For Codex:** Use the `superpowers:executing-plans` workflow to implement this plan task by task, and preserve the red/green evidence for the regression.

**Goal:** When cancellation or a deadline interrupts a session worker whose classified result is waiting to be persisted, report that real result instead of replacing it with `NotRun`.

**Architecture:** The state machine already stores the complete `MutantResult` before issuing `PersistResult`. Stopped-run draining should therefore treat `WorkerPhase::Persisting` like `WorkerPhase::Finishing`, while retaining the existing synthetic `NotRun` behavior for a process that is still in `WorkerPhase::Running`. Pending persistence effects remain retired by the normal stop transition.

**Tech Stack:** Rust, `hoimin-core` state-machine tests, Cargo.

---

### Task 1: Reproduce interrupted persistence

**Files:**
- Modify: `crates/hoimin-core/tests/machine.rs`

1. Add a focused driver that advances a session-backed candidate through `MutantFinished` and leaves its `PersistResult` in flight.
2. Stop the machine once with `CancellationRequested` and once with `DeadlineReached`.
3. Assert that the emitted `MutantFinished` retains the classified status, process termination, elapsed time, resource mode, output, and candidate identity rather than reporting `NotRun`.
4. Complete the output lifecycle and assert the final summary counts the classified result, the run is incomplete, and the expected exit code is 130 for cancellation or 4 for deadline.
5. Submit the late `ResultPersisted` completion and assert `MachineError::RetiredEffect`.
6. Run the focused test and record that it fails against the current `Persisting => StartedNotRun` behavior.

### Task 2: Preserve the classified result during stopped drain

**Files:**
- Modify: `crates/hoimin-core/src/machine.rs`
- Test: `crates/hoimin-core/tests/machine.rs`

1. Change `begin_stopped_mutant_drain` so `WorkerPhase::Persisting` drains `worker.result` as `StoppedCandidate::Finished`, alongside `WorkerPhase::Finishing`.
2. Leave `WorkerPhase::Running` mapped to `StoppedCandidate::StartedNotRun`.
3. Run the new focused tests and confirm they pass.

### Task 3: Guard the running boundary and verify

**Files:**
- Modify: `crates/hoimin-core/tests/machine.rs`

1. Add or strengthen a regression assertion showing cancellation during `WorkerPhase::Running` still reports `NotRun`, with no fabricated process termination.
2. Run `cargo test -p hoimin-core --test machine -- --nocapture`.
3. Run `cargo fmt --all -- --check`.
4. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
5. Run `cargo test --workspace --all-targets --all-features`.
6. Run `git diff --check origin/main...HEAD` and request an independent review before opening the PR.
