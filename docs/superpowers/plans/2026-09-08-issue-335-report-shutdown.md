# Owned Report Delivery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep stalled CLI report writes inside the run's shutdown contract.

**Architecture:** Preserve inline reporting for borrowed writers and add an
owned asynchronous driver for CLI standard streams. Share the existing shell
state machine and retain the handler and managed roots while writes run.

**Tech Stack:** Rust, Tokio, existing report serializers and shutdown oracle.

**Spec:** `docs/superpowers/specs/2026-09-08-issue-335-report-shutdown-design.md`

## Global Constraints

- Keep public borrowed and non-`Send` writer support.
- Acknowledge successful physical writes only.
- Never remove delivery roots while a report operation may still use them.
- Reuse the established absolute shutdown deadline, without extending it.
- Do not alter report schemas or oracle fixtures.

## Task 1: Reproduce the blocked consumer

**Files:** `crates/hoimin-cli/tests/run_e2e.rs`

- [x] Add `stalled_report_consumer_cannot_outlive_total_timeout_and_grace`:
  fill a nonblocking Unix stream until `WouldBlock`, restore blocking mode,
  pass its owned producer to the real CLI, and leave its consumer unread.
- [x] Run `cargo test -p hoimin-cli --test run_e2e stalled_report_consumer_cannot_outlive_total_timeout_and_grace -- --nocapture`.
  Confirm JSONL exceeds the six-second watchdog with a one-second run limit.

## Task 2: Own asynchronous report operations

**Files:** `crates/hoimin-cli/src/report/delivery.rs`,
`crates/hoimin-cli/src/report/mod.rs`, `crates/hoimin-cli/src/shell.rs`,
`crates/hoimin-cli/src/lib.rs`

**Interfaces:** Report delivery provides asynchronous `handle` and
`flush_and_release_spool`, plus a quiescence query. Inline delivery owns the
existing generic handler; owned delivery type-erases `Write + Send + 'static`
streams and retains its blocking task between polls.

- [x] Introduce the owned driver with one in-flight operation; restore the
  handler only after joining its task. Keep an `Arc` root owner in each task.
- [x] Select the owned driver for real CLI run/verify commands without adding
  bounds to existing borrowed APIs.
- [x] Await every shell report write and flush, including final JSON output.
  Include first interruption in the final-write wait and reuse its budget.
- [x] On expiration retain the in-flight task and defer delivery cleanup.
  Do not start new warning/flush work once the deadline has expired.
- [x] Ensure final CLI diagnostics cannot block on stalled stderr.
- [x] Rerun Task 1 and existing report-failure tests.

## Task 3: Validate lifetime and failure contracts

**Files:** report delivery unit tests and `crates/hoimin-cli/tests/run_e2e.rs`.

- [x] Exercise a controlled writer whose first write waits for release from
  another thread. Assert no acknowledgement before release and that dropping
  the waiting future does not drop its resource owner.
- [x] Repeat with a blocking flush and a write failure; verify no false
  acknowledgement and no overlapping operations.
- [x] Add first-interrupt and stalled-stderr CLI scenarios with watchdog
  teardown that always kills and reaps the child on failure.
- [x] Run `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  and `cargo test --workspace --all-features -- --test-threads=1` with this
  worktree's dedicated Cargo target directory.
- [x] Obtain independent review, fix findings, and record actual results.

## Delivery

Source, tests, and documents form one branch delivery. The pull request records
CI results for the final pushed SHA.
