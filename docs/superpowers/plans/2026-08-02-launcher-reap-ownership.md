# Launcher Reap Ownership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Observe the Linux cgroup launcher's stop or premature exit without consuming child status owned by Tokio or `std::process::Child`.

**Architecture:** Replace the consuming `waitpid` poll with an exact-PID `waitid` observation using `WNOWAIT`. Preserve the existing polling deadline and errors, and prove ownership with a real child that remains reapable after the observer reports premature exit.

**Tech Stack:** Rust, `libc::waitid`, Linux process control, Cargo, Docker.

## Global Constraints

- Keep all production and regression-test changes inside the Linux cgroup backend.
- Do not raise the minimum Linux kernel version or add dependencies.
- Leave consuming waits exclusively to the owning `Child`.
- Preserve the existing one-second deadline, five-millisecond interval, and public error text.

---

### Task 1: Prove premature launcher exit remains reapable

**Files:**
- Modify: `crates/hoimin-cli/src/resource/linux.rs:1280-1314`
- Test: `crates/hoimin-cli/src/resource/linux.rs` inside `platform::tests`

**Interfaces:**
- Consumes: private `fn wait_for_launcher_stop(pid: i32) -> Result<(), ResourceError>`
- Produces: Linux regression coverage for non-consuming launcher observation

- [ ] **Step 1: Add the real-process regression test**

Add a Linux-only test module inside `mod platform` and spawn a child that exits
before raising `SIGSTOP`:

```rust
#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::wait_for_launcher_stop;

    #[test]
    fn premature_launcher_exit_remains_reapable_by_its_child_owner() {
        let mut child = Command::new("sh")
            .args(["-c", "exit 23"])
            .spawn()
            .expect("spawn premature launcher exit fixture");
        let pid = i32::try_from(child.id()).expect("fixture pid fits i32");

        let error = wait_for_launcher_stop(pid).unwrap_err();
        let status = child
            .wait()
            .expect("launcher status remains owned by Child");

        assert_eq!(status.code(), Some(23));
        assert!(error
            .to_string()
            .contains("cgroup launcher did not stop before target execution"));
    }
}
```

- [ ] **Step 2: Run the regression test on Linux and verify RED**

Run in a Linux container:

```bash
docker run --rm --network none \
  -v "$PWD:/code" \
  -v "$HOME/.cargo/registry:/usr/local/cargo/registry:ro" \
  -v "$HOME/.cargo/git:/usr/local/cargo/git:ro" \
  -w /code \
  -e CARGO_TARGET_DIR=/tmp/hoimin-issue100-target \
  rust:latest \
  cargo test --offline -p hoimin-cli premature_launcher_exit_remains_reapable_by_its_child_owner -- --nocapture
```

Expected: FAIL at `child.wait()` with `ECHILD`, proving the current `waitpid`
poll consumed the exit status.

- [ ] **Step 3: Commit the failing regression test**

```bash
git add crates/hoimin-cli/src/resource/linux.rs
git commit -m "test: expose cgroup launcher reap ownership"
```

---

### Task 2: Observe launcher state without reaping

**Files:**
- Modify: `crates/hoimin-cli/src/resource/linux.rs:1280-1314`
- Test: `crates/hoimin-cli/src/resource/linux.rs` inside `platform::tests`

**Interfaces:**
- Consumes: `libc::{waitid, siginfo_t, P_PID, WSTOPPED, WEXITED, WNOHANG, WNOWAIT}`
- Produces: unchanged `fn wait_for_launcher_stop(pid: i32) -> Result<(), ResourceError>` behavior with non-consuming observation

- [ ] **Step 1: Replace the consuming wait with `waitid`**

In each poll, zero-initialize `libc::siginfo_t`, then observe the exact child:

```rust
let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
// SAFETY: `info` points to writable siginfo storage, and WNOWAIT leaves
// child status owned by the caller's Child handle.
let result = unsafe {
    libc::waitid(
        libc::P_PID,
        libc::id_t::try_from(pid).map_err(|_| {
            ResourceError::InvalidCgroupData("invalid cgroup launcher pid".into())
        })?,
        info.as_mut_ptr(),
        libc::WSTOPPED | libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
    )
};
```

After `result == 0`, initialize `info` and interpret it as follows:

```rust
let info = unsafe { info.assume_init() };
let observed_pid = unsafe { info.si_pid() };
if observed_pid != 0 {
    let status = unsafe { info.si_status() };
    if info.si_code == libc::CLD_STOPPED && status == libc::SIGSTOP {
        return Ok(());
    }
    return Err(ResourceError::InvalidCgroupData(
        "cgroup launcher did not stop before target execution".into(),
    ));
}
```

Keep the existing syscall-error mapping, deadline check, and sleep unchanged.

- [ ] **Step 2: Verify GREEN on Linux**

Run the focused Docker command from Task 1.

Expected: PASS; `Child::wait()` returns exit code 23.

- [ ] **Step 3: Add the stopped-child success regression**

Add a second test that spawns `sh -c 'kill -STOP $$; exit 0'`, calls
`wait_for_launcher_stop`, sends `SIGCONT` to the owned PID, and verifies that
`child.wait()` succeeds with exit code zero:

```rust
#[test]
fn stopped_launcher_is_observed_and_reaped_only_by_its_child_owner() {
    let mut child = Command::new("sh")
        .args(["-c", "kill -STOP $$; exit 0"])
        .spawn()
        .expect("spawn stopped launcher fixture");
    let pid = i32::try_from(child.id()).expect("fixture pid fits i32");

    wait_for_launcher_stop(pid).expect("observe SIGSTOP");
    assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
    let status = child.wait().expect("reap continued launcher");

    assert_eq!(status.code(), Some(0));
}
```

- [ ] **Step 4: Run both focused Linux tests**

Run:

```bash
cargo test --offline -p hoimin-cli launcher_is_observed -- --nocapture
```

inside the same Linux container configuration, plus the premature-exit test by
its full name if the filter does not match both names.

Expected: both tests PASS.

- [ ] **Step 5: Commit the implementation**

```bash
git add crates/hoimin-cli/src/resource/linux.rs
git commit -m "fix: preserve cgroup launcher reap ownership"
```

---

### Task 3: Verify and prepare the pull request

**Files:**
- Verify: `crates/hoimin-cli/src/resource/linux.rs`
- Verify: `docs/superpowers/specs/2026-08-02-launcher-reap-ownership-design.md`
- Verify: `docs/superpowers/plans/2026-08-02-launcher-reap-ownership.md`

**Interfaces:**
- Consumes: completed Issue #100 implementation and tests
- Produces: reviewed branch ready for a PR against `main`

- [ ] **Step 1: Run repository verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
git diff --check main...HEAD
```

- [ ] **Step 2: Run Linux verification**

In the Linux container, run:

```bash
cargo test --offline -p hoimin-cli resource::linux -- --nocapture
cargo test --offline --workspace --all-targets --all-features
```

- [ ] **Step 3: Request independent code review**

Review the complete diff from `main` to `HEAD` for ownership violations,
incorrect `siginfo_t` interpretation, PID conversions, test cleanup, and scope.
Resolve every Critical or Important finding before proceeding.

- [ ] **Step 4: Push and create the PR**

Push `fix/issue-100-launcher-reap-ownership` and create a PR against `main` with
`Closes #100`. Require every PR check, including Linux and randomized Rust tests,
to pass before merge.
