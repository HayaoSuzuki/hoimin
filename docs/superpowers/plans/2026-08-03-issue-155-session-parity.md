# Real CLI Session Ownership Contention Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add regression evidence that two overlapping real `hoimin run` processes cannot own and execute the same resumable session run.

**Architecture:** Extend `run_e2e.rs` with one Unix-only integration test that launches the built `CARGO_BIN_EXE_hoimin` twice against one SQLite session. The first mutation command atomically publishes a readiness marker and blocks; the test observes both that marker and exactly one incomplete run before launching the resumer, then reaps all children before asserting refusal and database invariants.

**Tech Stack:** Rust, Tokio process APIs, `rusqlite`, Python fixture command, Unix signals.

## Global Constraints

- Retain `fresh_session_and_sessionless_results_preserve_the_same_termination` and its exact `termination == {"Exit":1}` assertion; do not duplicate it.
- Use two genuinely overlapping `env!("CARGO_BIN_EXE_hoimin")` processes with the same fingerprint and `--session`; only the second process uses `--resume`.
- Synchronize through an atomically renamed readiness file and the committed SQLite run row, not a fixed-duration sleep.
- Require exact second-process exit code `2`, parseable JSON stdout with `summary.complete == false`, and stderr containing both `session.resume.active` and `active in another process`.
- Send the first process `SIGINT` and require exit code `130`.
- Verify SQLite integrity, foreign keys, one run, unique mutant results, and exactly one candidate join per result after both processes exit.
- Keep production code free of test-only APIs and do not commit the temporary ownership-refusal mutation.
- Preserve the untracked `.venv`; do not push or modify another worktree.

---

### Task 1: Real-process ownership regression

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Create: `docs/superpowers/plans/2026-08-03-issue-155-session-parity.md`

**Interfaces:**
- Consumes: `env!("CARGO_BIN_EXE_hoimin")`, `python_executable()`, `write_parallel_project()`, `reap_test_child()`, and `kill_fixture_processes()`.
- Produces: `concurrent_real_cli_runs_refuse_live_session_ownership`, plus test-local process-output, readiness, argument-building, and signal helpers.

- [ ] **Step 1: Add the test-first real CLI scenario**

  Add a `#[cfg(unix)]` Tokio test that creates a project and coordinator, binds `let session = coordinator.path().join("session.sqlite3")`, and builds one shared command. In the mutated-source branch, the Python command writes a PID to a temporary readiness path, atomically replaces the final readiness path, records a duplicate-execution marker if the final marker already exists, and blocks only the first execution. Spawn the first real CLI with JSON format and `kill_on_drop(true)`.

- [ ] **Step 2: Establish deterministic overlap before resumption**

  Poll with a deadline until the atomic readiness marker exists and a read-only SQLite query returns exactly one `runs` row with `complete=0`. Only then spawn the second real CLI with the same arguments plus `--resume`, capture its output, signal the first process, and wait for code `130`.

- [ ] **Step 3: Reap before asserting behavior**

  Store fallible scenario results, always call `reap_test_child(&mut first)` and `kill_fixture_processes(&active, &ready)`, and only then assert:

  ```rust
  assert_eq!(second.exit_code, 2);
  let second_document: serde_json::Value = serde_json::from_str(&second.stdout)?;
  assert_eq!(second_document["summary"]["complete"], false);
  assert!(second.stderr.contains("session.resume.active"), "{}", second.stderr);
  assert!(second.stderr.contains("active in another process"), "{}", second.stderr);
  assert_eq!(first_status.code(), Some(130));
  assert!(!duplicate_execution.exists());
  ```

- [ ] **Step 4: Assert database integrity and uniqueness**

  Open the session after teardown and assert literal values: `PRAGMA integrity_check == "ok"`, no row from `PRAGMA foreign_key_check`, `COUNT(*) FROM runs == 1`, result count equals `COUNT(DISTINCT mutant_id)` for the sole run, and zero results have a candidate-join count other than one.

- [ ] **Step 5: Prove RED through an uncommitted production mutation**

  Temporarily replace resume-time ownership acquisition in `SessionHandler::load` with no ownership. Run:

  ```bash
  cargo test -p hoimin-cli --test run_e2e concurrent_real_cli_runs_refuse_live_session_ownership
  ```

  Record the expected failure because the second process executes instead of exiting with refusal code `2`, then restore `crates/hoimin-cli/src/session/mod.rs` without committing the mutation.

- [ ] **Step 6: Prove focused GREEN**

  Run:

  ```bash
  cargo test -p hoimin-cli --test run_e2e fresh_session_and_sessionless_results_preserve_the_same_termination
  cargo test -p hoimin-cli --test run_e2e concurrent_real_cli_runs_refuse_live_session_ownership
  ```

  Expected: both commands pass and the new test leaves no child processes running.

- [ ] **Step 7: Verify related suites and repository hygiene**

  Run the full `run_e2e` and `session_handler` integration-test targets, `cargo fmt --check`, focused Clippy for `hoimin-cli`, `git diff --check`, and inspect `git diff` plus `git status --short` to confirm the production mutation is absent and `.venv` remains untracked.

- [ ] **Step 8: Commit the verified test and plan**

  ```bash
  git add crates/hoimin-cli/tests/run_e2e.rs docs/superpowers/plans/2026-08-03-issue-155-session-parity.md docs/superpowers/plans/2026-08-03-test-issue-consolidation.md
  git commit -m "test: cover real CLI session ownership contention"
  ```
