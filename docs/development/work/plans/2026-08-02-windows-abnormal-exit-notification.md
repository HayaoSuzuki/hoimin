# Windows Abnormal Exit Notification Implementation Plan

> **For Codex:** Use the `superpowers:executing-plans` workflow to implement this plan task by task, preserving test-first evidence where the host platform permits it.

**Goal:** Treat Windows Job Object abnormal-exit notifications as process exits so crashed mutant roots do not wait for the notification barrier and become infrastructure failures.

**Architecture:** Microsoft documents both `JOB_OBJECT_MSG_EXIT_PROCESS` and `JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS` as process-exit messages whose completion-port payload is the exiting PID. Route both identifiers through the existing active-root removal and `exited_roots` insertion branch; do not change resource-limit classification or the aggregate `ACTIVE_PROCESS_ZERO` fallback.

**Tech Stack:** Rust, `windows-sys`, Windows Job Objects, Cargo cross-target checks, GitHub Actions Windows tests.

---

### Task 1: Add a Windows regression test

**Files:**
- Modify: `crates/hoimin-cli/src/resource/windows.rs`

1. Add a unit test that registers an active root, records `JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS` for its PID, and asserts that the root leaves `active` and enters `exited_roots`.
2. Keep another active root in the fixture to prove the fix does not depend on `ACTIVE_PROCESS_ZERO`.
3. On the non-Windows development host, run `cargo check -p hoimin-cli --tests --target x86_64-pc-windows-msvc` to compile the Windows-only test. Record that the pre-fix branch in `record_notification` ignores the abnormal message; runtime RED is delegated to the Windows CI runner because Windows binaries cannot execute on macOS.

### Task 2: Handle the abnormal exit message

**Files:**
- Modify: `crates/hoimin-cli/src/resource/windows.rs`
- Test: `crates/hoimin-cli/src/resource/windows.rs`

1. Import `JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS` from `windows-sys`.
2. Match it alongside `JOB_OBJECT_MSG_EXIT_PROCESS` in the existing PID-specific exit branch.
3. Re-run the Windows cross-target test check.

### Task 3: Verify and review

1. Run `cargo fmt --all -- --check`.
2. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` on the host.
3. Run `cargo test --workspace --all-targets --all-features` on the host.
4. Run `cargo check -p hoimin-cli --tests --target x86_64-pc-windows-msvc`.
5. Run `git diff --check origin/main...HEAD` and request an independent review.
6. Open the PR and require the Windows Rust and Quality jobs to pass before merge.
