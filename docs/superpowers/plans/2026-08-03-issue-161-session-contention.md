# Session Operation Contention Matrix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prove every session operation returns its exact typed success while a second SQLite
connection holds either a write reservation or a read snapshot, without readiness sleeps.

**Architecture:** Add one table-driven `session_handler` integration test. Each independent cell
seeds only the valid run/result state it needs, starts a blocker thread, waits for an explicit
transaction-established signal, dispatches the real `SessionHandler` call on a worker thread,
and releases the blocker through a channel within one second. Result and cleanup channels bound
all waits. The three write operations must remain pending while `BEGIN IMMEDIATE` is held.

**Tech Stack:** Rust standard threads and channels, `rusqlite`, `SessionHandler`, `tempfile`.

## Production breaks caught

- `begin`, `persist`, or `finish` stops honoring SQLite's busy timeout and leaks an
  `EffectFailed`/`DatabaseBusy`/`DatabaseLocked` result instead of completing after a held writer
  releases.
- Any write operation incorrectly completes while the second connection still holds
  `BEGIN IMMEDIATE`.
- `lookup` or `load` fails under a concurrent write reservation, or any of the five operations
  fails under a retained WAL read snapshot.
- A successful operation echoes the wrong effect ID, run ID, mutant ID, worker, completeness, or
  seeded resume/result value.
- The table silently omits a required lock/operation combination.

## Exact matrix

```rust
enum ContendedOperation { Begin, Persist, Lookup, Finish, Load }
enum HeldLock { BeginImmediate, ReadTransaction }

const CELLS: [(HeldLock, ContendedOperation); 10] = [
    (HeldLock::BeginImmediate, ContendedOperation::Begin),
    (HeldLock::BeginImmediate, ContendedOperation::Persist),
    (HeldLock::BeginImmediate, ContendedOperation::Lookup),
    (HeldLock::BeginImmediate, ContendedOperation::Finish),
    (HeldLock::BeginImmediate, ContendedOperation::Load),
    (HeldLock::ReadTransaction, ContendedOperation::Begin),
    (HeldLock::ReadTransaction, ContendedOperation::Persist),
    (HeldLock::ReadTransaction, ContendedOperation::Lookup),
    (HeldLock::ReadTransaction, ContendedOperation::Finish),
    (HeldLock::ReadTransaction, ContendedOperation::Load),
];
```

## Task 1: Prove the completeness guard RED

**File:** `crates/hoimin-cli/tests/session_handler.rs`

- [ ] Add the exact enums and `CELLS` constant.
- [ ] Temporarily collect only `CELLS[..9]` into the visited set and assert its length equals ten.
- [ ] Run `cargo test -p hoimin-cli --test session_handler contention_matrix` and record the
      expected `left: 9`, `right: 10` failure.
- [ ] Replace the deliberately incomplete body; never commit it.

## Task 2: Exercise every cell deterministically

**File:** `crates/hoimin-cli/tests/session_handler.rs`

- [ ] Give every cell its own temporary database, handler, run ID, mutant ID, and effect IDs.
- [ ] Seed an incomplete run for persist/lookup/finish/load and a killed stored result for lookup.
- [ ] Have the blocker execute `BEGIN IMMEDIATE` or
      `BEGIN; SELECT count(*) FROM runs;`, then send an established signal.
- [ ] Dispatch the real operation only after establishment. Use a zero-capacity dispatch channel
      and no readiness sleep.
- [ ] For `BEGIN IMMEDIATE` plus begin/persist/finish, require the result channel to remain pending
      during the bounded pre-release observation window.
- [ ] Release through a channel no later than one second after dispatch and require an explicit
      released signal.
- [ ] Require completion within six seconds and join only after a bounded finished-thread check.
- [ ] On every failure path, release/drop blocker coordination, observe bounded cleanup, and report
      the `(lock, operation)` cell.

## Task 3: Assert exact typed results and matrix closure

**File:** `crates/hoimin-cli/tests/session_handler.rs`

- [ ] Compare begin with `SessionStarted` containing the requested effect and run IDs.
- [ ] Compare persist with `ResultPersisted` containing worker zero and the requested run/mutant IDs.
- [ ] Compare lookup with `StoredResultLoaded` containing the seeded killed `StoredResult`.
- [ ] Compare finish with `SessionFinished` containing the requested run and `complete: true`.
- [ ] Compare load with `SessionLoaded` containing the seeded `SessionResumeRef`.
- [ ] Treat every error variant, mismatched success variant, panic, disconnect, or timeout as failure.
- [ ] Insert every exercised cell into a visited set and assert its size and contents equal `CELLS`.

## Task 4: Verify and commit

- [ ] Run `cargo test -p hoimin-cli --test session_handler contention_matrix`.
- [ ] Run `cargo test -p hoimin-cli --test session_handler`.
- [ ] Run `cargo fmt --all --check`.
- [ ] Run targeted Clippy with warnings denied.
- [ ] Run `git diff --check`; inspect the full diff and status; preserve the untracked `.venv`.
- [ ] Commit only the test and this plan as `test: cover session operations under contention`.
- [ ] Do not push, create a PR, merge, close issues, or modify production APIs.
