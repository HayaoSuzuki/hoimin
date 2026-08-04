# Windows Console Interrupt Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Exercise the first and second real Windows Ctrl+C events against the hoimin binary and assert the complete cancellation and forced-exit contracts from issue #215.

**Architecture:** Reuse the existing Unix process/session scenarios through platform-neutral scenario functions. Windows starts hoimin in a dedicated console and re-enters the integration-test binary as a sender helper that attaches to that console and invokes `GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)`.

**Tech Stack:** Rust 2024/MSRV 1.88, Tokio process and I/O APIs, rusqlite, windows-sys 0.60 Win32 console and process APIs, cargo-mutants 27.1.0.

## Global Constraints

- Use the real `CARGO_BIN_EXE_hoimin` and `GenerateConsoleCtrlEvent(CTRL_C_EVENT, ...)` boundary.
- Put the Windows child in a dedicated console.
- Add no production fault-injection API, environment behavior, or Unix emulation.
- Synchronize through atomic readiness markers, SQLite state, JSONL events, and durable process handles.
- First event: exit 130, parseable JSON, `complete:false`, session `complete=0`, descendant reaped.
- Second event: retain a SQLite write lock, deliver the second event, exit 130 within one second, and emit no `run_finished`.
- Keep teardown bounded and preserve the existing Unix tests and public behavior.

---

### Task 1: Add failing Windows console-boundary tests

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: existing signal fixture, session readiness, JSONL readiness, descendant handles, and cleanup helpers.
- Produces: `first_ctrl_c_event_finishes_a_parseable_incomplete_session` and `second_ctrl_c_event_forces_130_while_session_finish_is_blocked`.

- [ ] **Step 1: Write Windows wrappers and wished-for helper calls**

Add Windows-only Tokio tests that invoke shared scenario functions. Refactor the existing Unix bodies without changing assertions, and make the shared body call platform-specific `spawn_interrupt_fixture` and `send_platform_interrupt` helpers that do not yet exist on Windows.

```rust
#[cfg(windows)]
#[tokio::test]
async fn first_ctrl_c_event_finishes_a_parseable_incomplete_session() {
    first_interrupt_scenario().await;
}

#[cfg(windows)]
#[tokio::test]
async fn second_ctrl_c_event_forces_130_while_session_finish_is_blocked() {
    second_interrupt_scenario().await;
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p hoimin-cli --test run_e2e first_ctrl_c_event_finishes_a_parseable_incomplete_session -- --exact --nocapture`

Expected: compilation fails because the Windows console fixture/sender is not implemented. The failure names that missing boundary rather than a typo in the assertions.

### Task 2: Implement the test-only console sender and bounded cleanup

**Files:**
- Modify: `crates/hoimin-cli/Cargo.toml`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: `std::env::current_exe`, hoimin child PID, Win32 Console APIs, Tokio `Command`.
- Produces: ignored `console_ctrl_sender_helper`, async `send_platform_interrupt(pid) -> Result<(), String>`, and Windows fixture creation flags.

- [ ] **Step 1: Enable the console API feature**

Add `Win32_System_Console` to the existing Windows-only `windows-sys` feature list. Add no dependency.

- [ ] **Step 2: Implement the ignored sender helper**

```rust
#[cfg(windows)]
#[test]
#[ignore = "subprocess helper for Windows console-control delivery"]
fn console_ctrl_sender_helper() {
    // Parse the target PID, attach to its dedicated console, ignore Ctrl+C in
    // this helper, generate CTRL_C_EVENT for console group zero, and detach.
}
```

The parent invokes the current test executable with `--ignored --exact console_ctrl_sender_helper`, checks its exit status, and includes captured stdout/stderr in failures.

- [ ] **Step 3: Start hoimin in the dedicated Windows console**

On Windows, use `std::os::windows::process::CommandExt::creation_flags` with `CREATE_NEW_CONSOLE` before spawning the real binary. Do not also request `CREATE_NEW_PROCESS_GROUP`: Windows ignores that flag when `CREATE_NEW_CONSOLE` is present, and group-zero `CTRL_C_EVENT` delivery is isolated by the dedicated console itself. Unix supplies no creation flags.

- [ ] **Step 4: Generalize cleanup without weakening handle identity**

Compile session/readiness helpers on Unix and Windows. Make both Python fixture processes ignore `SIGINT` before publishing readiness or sleeping. Open Windows handles for both the active mutant and its descendant with `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE`, retain them through the scenario, and use `TerminateProcess` only on those retained handles during teardown. Never reopen a raw Windows PID during cleanup; preserve the existing Unix PID-based cleanup.

- [ ] **Step 5: Verify GREEN for both rows**

Run:

```console
cargo test -p hoimin-cli --test run_e2e first_ctrl_c_event_finishes_a_parseable_incomplete_session -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e second_ctrl_c_event_forces_130_while_session_finish_is_blocked -- --exact --nocapture
```

Expected: both pass on Windows. The first reports 130/incomplete and reaps the descendant. The second reports bounded 130 without `run_finished` while the lock is retained.

### Task 3: Regression and mutation verification

**Files:**
- Verify: `crates/hoimin-cli/tests/run_e2e.rs`
- Verify: `crates/hoimin-cli/src/interrupt.rs`

**Interfaces:**
- Consumes: Tasks 1-2.
- Produces: review-ready evidence for #215.

- [ ] **Step 1: Run the complete owning target**

Run: `cargo test -p hoimin-cli --test run_e2e -- --nocapture`

Expected: all tests pass on Windows, including both console-control rows and existing platform-neutral E2E behavior.

- [ ] **Step 2: Run quality gates**

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check
```

Expected: formatting and Clippy pass. Workspace tests pass except only if the same recorded Windows symlink privilege error 1314 recurs; no new failure is accepted.

- [ ] **Step 3: Run focused Rust mutation testing**

Run: `cargo mutants --workspace --jobs 2 --file crates/hoimin-cli/src/interrupt.rs`

Expected: clean baseline, no timeout/error, and no surviving behavioral mutant for first/second interrupt delivery. Inspect `mutants.out/missed.txt`, `timeout.txt`, and `unviable.txt`; do not hide outcomes with `--iterate` or new exclusions.

- [ ] **Step 4: Commit**

```console
git add crates/hoimin-cli/Cargo.toml crates/hoimin-cli/tests/run_e2e.rs docs/superpowers/specs/2026-08-04-issue-215-windows-console-interrupts-design.md docs/superpowers/plans/2026-08-04-issue-215-windows-console-interrupts.md
git commit -m "test: exercise Windows console interrupts"
```
