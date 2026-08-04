# Issue #222 Windows Root-Before-Descendant Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic Windows regression coverage proving that a real root can exit before its assigned descendant and that production `ProcessHandler` cleanup remains bounded and empties the Job Object.

**Architecture:** Extend the private `resource::windows::tests` module so the test can drive the public `ProcessHandler` and query the existing private run Job Object. A Python fixture atomically publishes both PIDs and waits on a release file; owned Win32 handles bind assertions to those exact process generations while the pinned handler future controls when production cleanup may advance.

**Tech Stack:** Rust 2024, Tokio, `windows-sys` 0.60, Python process fixture, Cargo test/fmt/clippy.

## Global Constraints

- Run only on Windows under the existing `#[cfg(test)]` Windows resource module.
- Assign a real runtime root and its real long-lived descendant through the production Job Object path.
- Atomically publish both process identities before a coordinated zero exit.
- Observe root-exited/descendant-active through owned real process handles, not raw PID liveness or synthetic state.
- Require `ProcessTermination::Exit(0)`.
- Bound `ProcessHandler::handle`, descendant cleanup, Job Object accounting convergence, and `ProcessHandler::close` together to six seconds.
- Require the descendant handle to become inactive and the production run Job Object to report zero active processes before close.
- Do not mutate `RunState` or call `record_notification` in the new test.
- Do not change production APIs, dependencies, or cleanup semantics.

---

### Task 1: Add real process-handle test utilities and the Windows lifecycle regression

**Files:**
- Modify: `crates/hoimin-cli/src/resource/windows.rs:634-821`
- Test: `crates/hoimin-cli/src/resource/windows.rs` private `tests` module

**Interfaces:**
- Consumes: `ProcessHandler::handle(RunProcess)`, `ProcessHandler::close()`, `WindowsBackend::new(&RunLimits)`, private `active_process_count(HANDLE)`, private `OwnedHandle`, Win32 `OpenProcess` and `WaitForSingleObject`.
- Produces: test-only `FixtureProcessHandle::{open,is_active,wait_until_exits}`, `published_process_identities`, `wait_until_job_is_empty`, and `exited_root_is_observed_before_assigned_descendant_cleanup`.

- [ ] **Step 1: Name the break the regression catches**

Record beside the test implementation that removing `terminate_job(self.root_job.raw(), ...)` from `WindowsSupervisor::terminate` must leave the published descendant active and the run Job Object nonempty after `ProcessHandler::handle` returns. The assertion values are independently derived: root wait is `WAIT_OBJECT_0`, descendant pre-cleanup wait is `WAIT_TIMEOUT`, result is literal `ProcessTermination::Exit(0)`, and accounting is literal zero.

- [ ] **Step 2: Add owned real-process-handle helpers**

Import `WAIT_OBJECT_0`, `WAIT_TIMEOUT`, `OpenProcess`, `WaitForSingleObject`, `PROCESS_QUERY_LIMITED_INFORMATION`, and `SYNCHRONIZE`. Add this test-only shape inside `resource::windows::tests`:

```rust
struct FixtureProcessHandle(OwnedHandle);

impl FixtureProcessHandle {
    fn open(pid: u32) -> Self {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                0,
                pid,
            )
        };
        Self(OwnedHandle::new(handle, "open fixture process").unwrap())
    }

    fn wait_result(&self) -> u32 {
        unsafe { WaitForSingleObject(self.0.raw(), 0) }
    }

    fn is_active(&self) -> bool {
        self.wait_result() == WAIT_TIMEOUT
    }

    async fn wait_until_exits(&self) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            match self.wait_result() {
                WAIT_OBJECT_0 => return true,
                WAIT_TIMEOUT if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                WAIT_TIMEOUT => return false,
                result => panic!("unexpected fixture process wait result: {result}"),
            }
        }
    }
}
```

Add a parser that returns `Some((root_pid, descendant_pid))` only for exactly two valid `u32` tokens in the atomically renamed identity file. Add a bounded condition poll that returns true only when `active_process_count(backend.inner.job.raw())` reaches zero.

- [ ] **Step 3: Write the regression test without polling the handler across the observation boundary**

Create a `RunProcess` whose Python source performs these exact operations:

```python
child = subprocess.Popen(
    [sys.executable, "-c", "import time; time.sleep(30)"],
    stdin=subprocess.DEVNULL,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
pending.write_text(f"{os.getpid()} {child.pid}")
os.replace(pending, identities)
while not release.exists():
    time.sleep(0.005)
```

Pin `handler.handle(request)`. Poll it only until the atomic identity file appears, open both process handles before writing the release file, then stop polling the handler. Condition-wait for the root handle to signal and assert the descendant handle still returns `WAIT_TIMEOUT`. Resume the handler and assert:

```rust
assert_eq!(result.termination, ProcessTermination::Exit(0));
assert!(descendant.wait_until_exits().await);
assert!(wait_until_job_is_empty(&backend).await);
handler.close().unwrap();
```

Wrap spawn, observation, handler completion, cleanup convergence, and close in `tokio::time::timeout(Duration::from_secs(6), ...)`, then assert the wall-clock elapsed time is below six seconds. Do not access `RunState` or `record_notification`.

- [ ] **Step 4: Verify RED with the named cleanup regression**

After the test is written, temporarily replace only this production statement in `WindowsSupervisor::terminate`:

```rust
terminate_job(self.root_job.raw(), "terminate nested root process job")?;
```

with a no-op success so the nested assigned descendant survives `handle`. Run:

```console
cargo test -p hoimin-cli resource::windows::tests::exited_root_is_observed_before_assigned_descendant_cleanup -- --exact --nocapture
```

Expected: FAIL within six seconds because the descendant handle remains active or the production run Job Object remains nonempty. Restore the exact production statement immediately after recording the failure.

- [ ] **Step 5: Verify GREEN against unmodified production cleanup**

Run the same focused command again.

Expected: PASS; the root handle is observed signaled while the descendant handle is active, the handler returns `Exit(0)`, the descendant becomes signaled, Job Object accounting reaches zero, and close succeeds under the outer bound.

### Task 2: Verify scope and repository quality gates

**Files:**
- Verify: `crates/hoimin-cli/src/resource/windows.rs`
- Verify: `docs/superpowers/specs/2026-08-04-issue-222-root-before-descendant-design.md`
- Verify: `docs/superpowers/plans/2026-08-04-issue-222-root-before-descendant.md`

**Interfaces:**
- Consumes: Cargo workspace configuration and the repository's Rust mutation policy.
- Produces: fresh focused/full test, formatting, lint, diff, and mutation-applicability evidence.

- [ ] **Step 1: Format and rerun the focused Windows regression**

```console
cargo fmt --all -- --check
cargo test -p hoimin-cli resource::windows::tests::exited_root_is_observed_before_assigned_descendant_cleanup -- --exact --nocapture
```

Expected: both commands exit zero.

- [ ] **Step 2: Run the full relevant Rust tests**

```console
cargo test -p hoimin-cli
cargo test --workspace
```

Expected: all tests pass, including the Windows-only resource test and integration suite.

- [ ] **Step 3: Run lint and source checks**

```console
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
rg -n "RunState|record_notification" crates/hoimin-cli/src/resource/windows.rs
```

Expected: clippy and diff checks exit zero. The text search may find older direct-state unit tests but must not find either symbol inside the new real-handle regression.

- [ ] **Step 4: Record Rust mutation non-applicability with evidence**

```console
git diff --unified=0 origin/main -- crates/hoimin-cli/src/resource/windows.rs
git diff --numstat origin/main -- crates/hoimin-cli/src/resource/windows.rs
```

Inspect every changed hunk. Expected: all Rust additions are inside the existing `#[cfg(test)] mod tests`; no production Rust expression changed. Therefore `cargo mutants --workspace` has no changed production target for this issue and is not applicable. Preserve the RED run from Task 1 as mutation-style evidence that omission of nested Job Object termination is detected.

- [ ] **Step 5: Review and commit the implementation**

```console
git status --short
git diff --stat origin/main
git diff origin/main -- crates/hoimin-cli/src/resource/windows.rs
git log --oneline origin/main..HEAD
git add crates/hoimin-cli/src/resource/windows.rs docs/superpowers/plans/2026-08-04-issue-222-root-before-descendant.md
git commit -m "test(windows): cover root-before-descendant cleanup"
```

Expected: the final commit contains only the plan and Windows test changes; the earlier design commit remains in branch history. Do not push, open a pull request, merge, or remove the worktree.
