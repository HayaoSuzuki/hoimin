# Wait Error Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure every post-attachment wait error explicitly terminates and reaps the supervised process tree.

**Architecture:** Reuse `terminate_and_reap` for the wait-error branch. Preserve the wait failure through a generic async cleanup-composition helper so cleanup success and failure can be tested deterministically.

**Tech Stack:** Rust, Tokio, platform resource supervisors, Cargo test

## Global Constraints

- The `process.wait` error remains the primary failure.
- Tree termination and bounded root reaping occur before output grace waiting.
- Cleanup failures are appended, never substituted for the wait error.
- Cancellation, timeout, and successful-exit behavior remain unchanged.

---

### Task 1: Specify wait-error cleanup composition

**Files:**
- Modify: `crates/hoimin-cli/src/process/mod.rs`

**Interfaces:**
- Produces: `wait_failure_after_cleanup(id, wait_error, cleanup_future)`

- [x] **Step 1: Add failing unit tests**

Use an atomic flag set inside an injected cleanup future passed to the same
wait-error handler used by the process-selection branch. Cover cleanup success
and cleanup failure. Assert the future runs, the original wait error code and
operation remain, and failure detail is appended only on cleanup failure.

- [x] **Step 2: Verify RED**

Run: `cargo test -p hoimin-cli process::tests::wait_error_cleanup`

Expected: compilation fails because the composition helper does not exist.

### Task 2: Clean up the real wait-error branch

**Files:**
- Modify: `crates/hoimin-cli/src/process/mod.rs`
- Test: `crates/hoimin-cli/src/process/mod.rs`

**Interfaces:**
- Consumes: `terminate_and_reap(id, supervisor, child)`
- Produces: `wait_failure_after_cleanup(...) -> EffectFailed`

- [x] **Step 1: Implement cleanup composition**

Await the supplied cleanup future and append failure detail to the primary
error only when it returns `Err`.

- [x] **Step 2: Implement wait-error cleanup**

Construct the existing `process.wait` I/O failure, pass
`terminate_and_reap(...)` to the composition helper, and await it in the
`ProcessSelection::Exited(Err(...))` branch.

- [x] **Step 3: Verify focused process tests**

Run:

```console
cargo test -p hoimin-cli process::tests::wait_error_cleanup
cargo test -p hoimin-cli --test process_handler --test run_e2e
```

Expected: all tests pass.

- [x] **Step 4: Run full verification**

Run:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all commands exit zero.
