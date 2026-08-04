# Issue #222 Windows Root-Before-Descendant Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Completed steps are marked with checkbox (`- [x]`) syntax.

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

- [x] **Step 1: Name the break the regression catches**

Record beside the test implementation that discarding the assigned `root_job` without terminating it or closing its kill-on-close handle must leave the published descendant active and the run Job Object nonempty after `ProcessHandler::handle` returns. The assertion values are independently derived: root wait is `WAIT_OBJECT_0`, descendant pre-cleanup wait is `WAIT_TIMEOUT`, result is literal `ProcessTermination::Exit(0)`, and accounting is literal zero.

- [x] **Step 2: Add owned real-process-handle helpers**

Import `WAIT_OBJECT_0`, `WAIT_TIMEOUT`, `OpenProcess`, `WaitForSingleObject`, `PROCESS_QUERY_LIMITED_INFORMATION`, and `PROCESS_SYNCHRONIZE`. Add this test-only shape inside `resource::windows::tests`:

```rust
struct FixtureProcessHandle(OwnedHandle);

impl FixtureProcessHandle {
    fn open(pid: u32) -> Self {
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
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

- [x] **Step 3: Write the regression test without polling the handler across the observation boundary**

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

- [x] **Step 4: Verify RED with the named cleanup regression**

After the test is written, temporarily replace this production statement in `WindowsSupervisor::terminate`:

```rust
terminate_job(self.root_job.raw(), "terminate nested root process job")?;
```

with the following fault, which discards the assigned Job Object without exercising explicit termination or kill-on-close:

```rust
let assigned_job = std::mem::replace(&mut self.root_job, create_job()?);
std::mem::forget(assigned_job);
```

Run:

```console
cargo test -p hoimin-cli resource::windows::tests::exited_root_is_observed_before_assigned_descendant_cleanup -- --exact --nocapture
```

Recorded result: exit `1`; the test failed in 2.62 seconds with `assigned descendant remained active after normal root cleanup`. The exact production statement was restored immediately afterward. An earlier trial that omitted only `TerminateJobObject` passed because the still-owned nested job is configured kill-on-close and supplies equivalent cleanup when the supervisor drops; it did not represent complete loss of cleanup ownership.

- [x] **Step 5: Verify GREEN against unmodified production cleanup**

Run the same focused command again.

Recorded result after restoring production cleanup: exit `0`; one focused test passed in 0.25 seconds. The post-commit rerun also exited `0`, with one test passed in 0.43 seconds. The root handle was observed signaled while the descendant handle remained active, the handler returned `Exit(0)`, the descendant became signaled, Job Object accounting reached zero, and close succeeded under the outer bound.

### Task 2: Verify scope and repository quality gates

**Files:**
- Verify: `crates/hoimin-cli/src/resource/windows.rs`
- Verify: `docs/superpowers/specs/2026-08-04-issue-222-root-before-descendant-design.md`
- Verify: `docs/superpowers/plans/2026-08-04-issue-222-root-before-descendant.md`

**Interfaces:**
- Consumes: Cargo workspace configuration and the repository's Rust mutation policy.
- Produces: fresh focused/full test, formatting, lint, diff, and mutation-applicability evidence.

- [x] **Step 1: Format and rerun the focused Windows regression**

```console
cargo fmt --all -- --check
cargo test -p hoimin-cli resource::windows::tests::exited_root_is_observed_before_assigned_descendant_cleanup -- --exact --nocapture
```

Recorded result: both commands exited `0`; the post-commit focused test passed once in 0.43 seconds.

- [x] **Step 2: Run the full relevant Rust tests**

```console
cargo test -p hoimin-cli
cargo test --workspace
```

Recorded results:

- Unfiltered `cargo test -p hoimin-cli` reached 176 passed and 1 ignored library tests, then failed only `workspace::root::tests::rejects_non_normal_and_linked_parent_components` with Windows error 1314 because the host lacks symlink-creation privilege. An outside-sandbox retry produced the same privilege failure.
- After creating the worktree's locked `.venv`, `cargo test -p hoimin-cli -- --skip workspace::root::tests::rejects_non_normal_and_linked_parent_components` exited `0` across the package targets.
- The initial parallel workspace run exposed existing PID-publication timing flakes. `cargo test --workspace -- --skip workspace::root::tests::rejects_non_normal_and_linked_parent_components --test-threads=1` then exited `0` in 130.7 seconds.
- The privilege-dependent exclusion is unrelated to issue #222; the new Windows Job Object test passed in every package and workspace run.

- [x] **Step 3: Run lint and source checks**

```console
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
rg -n "RunState|record_notification" crates/hoimin-cli/src/resource/windows.rs
```

Recorded result: clippy and diff checks exited `0`. A source-range check over the new regression found neither `RunState` nor `record_notification`; matches elsewhere belong to pre-existing direct-state unit tests.

- [x] **Step 4: Record Rust mutation non-applicability with evidence**

```console
git diff --unified=0 origin/main -- crates/hoimin-cli/src/resource/windows.rs
git diff --numstat origin/main -- crates/hoimin-cli/src/resource/windows.rs
```

Recorded result: every Rust hunk is inside the existing `#[cfg(test)] mod tests`; no production Rust expression changed. Therefore `cargo mutants --workspace` has no changed production target for this issue and is not applicable. The deliberate discarded-Job-Object fault and its 2.62-second RED failure provide mutation-style evidence that the new test detects complete loss of nested cleanup ownership.

- [x] **Step 5: Review and commit the implementation**

```console
git status --short
git diff --stat origin/main
git diff origin/main -- crates/hoimin-cli/src/resource/windows.rs
git log --oneline origin/main..HEAD
git add crates/hoimin-cli/src/resource/windows.rs docs/superpowers/plans/2026-08-04-issue-222-root-before-descendant.md
git commit -m "test(windows): cover root-before-descendant cleanup"
```

Recorded result: implementation commit `5c734b19fcf1fac981a8f39dc53d1c06a475c010` contains the Windows test. The earlier design and plan commits remain in branch history, and this execution record is a documentation-only review follow-up. Nothing was pushed, merged, or removed.
