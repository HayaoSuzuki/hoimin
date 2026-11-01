# Windows Portable Containment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure a Windows portable child cannot execute target code or create a descendant before its kill-on-close Job Object contains it.

**Architecture:** Extract the hard backend's Windows suspended-process mechanics into a Windows-only `suspended` module. Both hard and portable backends configure `CREATE_SUSPENDED`, assign every required Job Object through one `SuspendedChild`, and resume only after the complete assignment sequence succeeds.

**Tech Stack:** Rust 2024, Tokio process APIs, `windows-sys` 0.60 Job Object and Toolhelp APIs, Cargo tests, Clippy, cargo-mutants

## Global Constraints

- Work only in `.worktrees/issue-23-windows-portable-containment` on branch `fix/issue-23-windows-portable-containment`.
- Keep the design and implementation plan in the same branch and worktree as the implementation.
- Preserve Unix portable process groups and rlimits exactly.
- Preserve hard Windows locking, completion-port accounting, limit configuration, classification, and active-root bookkeeping.
- Do not change cancellation, timeout, output, or error precedence.
- Do not add a custom replacement for Tokio process spawning.
- A failed assignment or resume must leave the root suspended until existing attach-failure cleanup kills and waits for it.
- Windows code-change CI must run; do not cancel it.
- Mutation testing is limited to the shared suspended-startup operations and portable attach sequence and is meaningful only on a Windows execution environment.

---

## File Structure

- Create `crates/hoimin-cli/src/resource/suspended.rs`: Windows-only command suspension, root process handle ownership, Job Object assignment, and primary-thread resume.
- Modify `crates/hoimin-cli/src/resource/mod.rs`: register the private Windows-only shared module.
- Modify `crates/hoimin-cli/src/resource/windows.rs`: consume the shared primitive and remove duplicated startup mechanics.
- Modify `crates/hoimin-cli/src/resource/portable.rs`: configure suspended Windows startup, assign before resume, and provide deterministic test-only attach faults.
- Test `crates/hoimin-cli/src/resource/portable.rs`: Windows-only regression and success contracts using private test fault injection.
- Test `crates/hoimin-cli/src/resource/windows.rs`: retain and rerun hard-backend assignment/resume failure contracts.
- Test `crates/hoimin-cli/tests/process_handler.rs`: retain public portable process behavior coverage.

### Task 1: Extract the Shared Suspended-Startup Primitive

**Files:**
- Create: `crates/hoimin-cli/src/resource/suspended.rs`
- Modify: `crates/hoimin-cli/src/resource/mod.rs`
- Modify: `crates/hoimin-cli/src/resource/windows.rs`
- Test: `crates/hoimin-cli/src/resource/windows.rs`

**Interfaces:**
- Consumes: `tokio::process::{Child, Command}`, live Windows Job Object `HANDLE` values, and `ResourceError`.
- Produces:
  - `pub(super) fn configure(command: &mut Command)`
  - `pub(super) struct SuspendedChild`
  - `pub(super) fn SuspendedChild::open(child: &Child) -> Result<Self, ResourceError>`
  - `pub(super) fn SuspendedChild::pid(&self) -> u32`
  - `pub(super) fn SuspendedChild::assign(&self, job: HANDLE, operation: &'static str) -> Result<(), ResourceError>`
  - `pub(super) fn SuspendedChild::resume(self) -> Result<(), ResourceError>`

- [ ] **Step 1: Run the hard-backend characterization test before extraction**

Run on Windows:

```powershell
cargo test -p hoimin-cli resource::windows::tests::assign_and_resume_failures_kill_suspended_root_before_user_code -- --exact
```

Expected: PASS for both injected assignment and resume failures, with no marker created.

On the development host, establish the platform-neutral baseline:

```bash
cargo test -p hoimin-cli --lib
```

Expected: PASS.

- [ ] **Step 2: Create the Windows-only shared module**

Create `crates/hoimin-cli/src/resource/suspended.rs` with the existing hard-backend behavior moved behind this interface:

```rust
use std::io;
use std::mem::size_of;
use std::os::windows::process::CommandExt;

use tokio::process::{Child, Command};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_TERMINATE, ResumeThread, THREAD_SUSPEND_RESUME,
};

use super::{ResourceError, windows::OwnedHandle};

pub(super) fn configure(command: &mut Command) {
    command.as_std_mut().creation_flags(CREATE_SUSPENDED);
}

pub(super) struct SuspendedChild {
    pid: u32,
    process: OwnedHandle,
}

impl SuspendedChild {
    pub(super) fn open(child: &Child) -> Result<Self, ResourceError> {
        let pid = child.id().ok_or(ResourceError::MissingProcessId)?;
        let process = OwnedHandle::new(
            unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
                    0,
                    pid,
                )
            },
            "open suspended root process",
        )?;
        Ok(Self { pid, process })
    }

    pub(super) fn pid(&self) -> u32 {
        self.pid
    }

    pub(super) fn assign(
        &self,
        job: HANDLE,
        operation: &'static str,
    ) -> Result<(), ResourceError> {
        if unsafe { AssignProcessToJobObject(job, self.process.raw()) } == 0 {
            Err(ResourceError::io(operation, io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    pub(super) fn resume(self) -> Result<(), ResourceError> {
        resume_primary_thread(self.pid)
    }
}
```

Move `resume_primary_thread` from `windows.rs` without changing its Toolhelp enumeration, access rights, operation labels, or `ResumeThread == u32::MAX` error check. Make `OwnedHandle` and only the `new`/`raw` methods needed by the module `pub(super)`; do not broaden them outside `resource`.

The unsafe blocks must retain precise safety comments for `OpenProcess`, snapshot enumeration, `OpenThread`, and `ResumeThread`.

- [ ] **Step 3: Register the private module**

Add to `crates/hoimin-cli/src/resource/mod.rs`:

```rust
#[cfg(windows)]
mod suspended;
```

Keep it private; do not re-export `SuspendedChild`.

- [ ] **Step 4: Replace hard-backend duplication**

In `WindowsBackend::prepare`, replace the direct creation flag call with:

```rust
super::suspended::configure(command);
```

In `WindowsRunJob::attach_root`, preserve the existing lock and state ordering while replacing open/assign/resume calls:

```rust
let child = super::suspended::SuspendedChild::open(child)?;
let pid = child.pid();
child.assign(self.job.raw(), "assign process to run-wide job")?;
child.assign(root_job, "assign process to nested root job")?;
state.active.push(ActiveRoot {
    pid,
    signal: Arc::downgrade(signal),
});
if attach_fault == AttachFault::Resume {
    state.active.retain(|root| root.pid != pid);
    return Err(ResourceError::io(
        "resume suspended primary thread",
        io::Error::other("injected resume failure"),
    ));
}
if let Err(error) = child.resume() {
    state.active.retain(|root| root.pid != pid);
    return Err(error);
}
```

Remove only imports and helper functions made obsolete by the extraction.

- [ ] **Step 5: Verify behavior and compilation**

Run locally:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p hoimin-cli --lib
```

Expected: all commands PASS.

Run on Windows:

```powershell
cargo test -p hoimin-cli resource::windows::tests::assign_and_resume_failures_kill_suspended_root_before_user_code -- --exact
cargo test -p hoimin-cli --lib
```

Expected: both commands PASS; the extracted primitive has no behavior change for hard mode.

- [ ] **Step 6: Commit**

```bash
git add crates/hoimin-cli/src/resource/mod.rs crates/hoimin-cli/src/resource/suspended.rs crates/hoimin-cli/src/resource/windows.rs
git commit -m "refactor: share Windows suspended startup"
```

### Task 2: Contain Portable Children Before Resume

**Files:**
- Modify: `crates/hoimin-cli/src/resource/portable.rs`
- Test: `crates/hoimin-cli/src/resource/portable.rs`
- Test: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Consumes: Task 1 `suspended::configure` and `SuspendedChild`.
- Produces: Windows portable `prepare` and `attach` ordering of suspend, assign, resume; private `AttachFault::{None, AssignAfterDelay}` test policy.

- [ ] **Step 1: Add the deterministic failing Windows regression**

Extend `PortableBackend` and `PortableSupervisor` with a private, copyable
test fault:

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AttachFault {
    #[default]
    None,
    #[cfg(test)]
    AssignAfterDelay,
}
```

Under `#[cfg(all(test, windows))]`, add a constructor that selects
`AssignAfterDelay`. Propagate the value from `PortableBackend::prepare` to
`PortableSupervisor`; ordinary constructors always use `None`.

Before changing production startup ordering, add a Windows-only async unit test
whose command immediately writes a root marker, starts a detached Python
descendant that writes a descendant marker, and then waits. The injected attach
fault is applied inside the portable assignment helper: it sleeps for 200 ms,
then calls the real `SuspendedChild::assign` operation with an invalid Job
Object handle so Win32 returns an assignment error. It must not return before
the assignment call:

```rust
suspended.assign(std::ptr::null_mut(), "assign spawned process to job")
```

Use `subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP` for the
descendant. Repeat the scenario four times with unique marker paths. Bound each
handler call to two seconds. Before interpreting or asserting the handler
result, poll briefly for any detached fixture PID, open it with
`PROCESS_TERMINATE | SYNCHRONIZE`, require `TerminateProcess` to succeed, and
require `WaitForSingleObject` to report termination within one second. This
cleanup order must also run when the handler times out or unexpectedly
succeeds, so assertion failure cannot leave the 30-second fixture alive.

Assertions after each run:

```rust
assert!(matches!(
    error.failure,
    EffectFailure::Io { ref code, .. } if code == "process.resource.attach"
));
assert!(!root_marker.exists(), "suspended root code must not execute");
assert!(
    !descendant_marker.exists(),
    "a pre-assignment descendant must not escape the portable job"
);
```

- [ ] **Step 2: Run the test against spawn-then-attach to verify RED**

Run on Windows:

```powershell
cargo test -p hoimin-cli resource::portable::tests::attach_failure_prevents_immediate_detached_descendant -- --exact --nocapture
```

Expected: FAIL because at least the root marker, and normally the descendant
marker, is created during the injected 200 ms pre-attach window. Record the
exact RED result in the task report before implementing the fix.

- [ ] **Step 3: Configure Windows portable commands as suspended**

At the end of the Windows branch of `configure_command`, call the shared
primitive:

```rust
#[cfg(windows)]
fn configure_command(
    command: &mut Command,
    _limits: ProcessLimits,
) -> Result<(), ResourceError> {
    super::suspended::configure(command);
    Ok(())
}
```

Keep the non-Windows no-op in its own `cfg(not(any(unix, windows)))` function so
the Windows behavior cannot silently fall through to it.

- [ ] **Step 4: Assign before resuming**

Replace the Windows PID reopen/assignment path in
`PortableSupervisor::attach` with:

```rust
#[cfg(windows)]
{
    let suspended = super::suspended::SuspendedChild::open(child)?;
    self.assign_suspended(&suspended)?;
    suspended.resume()?;
}
```

Add a Windows-only `assign_suspended` helper. Ordinary execution passes
`self.job`; the test fault sleeps inside this helper and passes a null handle
to the real assignment operation:

```rust
#[cfg(windows)]
fn assign_suspended(
    &self,
    suspended: &super::suspended::SuspendedChild,
) -> Result<(), ResourceError> {
    #[cfg(test)]
    if self.attach_fault == AttachFault::AssignAfterDelay {
        std::thread::sleep(std::time::Duration::from_millis(200));
        return suspended.assign(std::ptr::null_mut(), "assign spawned process to job");
    }
    suspended.assign(self.job as _, "assign spawned process to job")
}
```

Set the Unix process group from the PID as before. Remove the obsolete
portable-local `assign_to_job` function and Windows `OpenProcess` imports.

The injected delay occurs at the actual assignment boundary while the child is
already suspended. Do not resume on assignment failure and do not mark the
supervisor terminated. The test must also be demonstrated RED with both of
these temporary mutations:

- delete/bypass `assign_suspended` so the child resumes without containment;
- call `suspended.resume()` before `assign_suspended`, so the 200 ms delay runs
  after user code is released.

- [ ] **Step 5: Run the regression to verify GREEN**

Run on Windows:

```powershell
cargo test -p hoimin-cli resource::portable::tests::attach_failure_prevents_immediate_detached_descendant -- --exact --nocapture
```

Expected: PASS in all four iterations; neither marker exists and attach-failure cleanup is bounded.

Temporarily bypassing assignment or reversing assignment/resume must FAIL by
creating the root marker (and normally the descendant marker). Restore the
correct implementation before continuing and record both RED results in the
task report.

- [ ] **Step 6: Prove successful portable children resume**

Run:

```powershell
cargo test -p hoimin-cli --test process_handler portable::returns_zero_and_nonzero_exit -- --exact
cargo test -p hoimin-cli --test process_handler portable::passes_native_arguments_without_a_shell -- --exact
cargo test -p hoimin-cli --test process_handler portable::drains_stdout_and_stderr_under_combined_cap -- --exact
```

Expected: all PASS. A forgotten resume would timeout these existing success paths.

- [ ] **Step 7: Run local non-Windows regression checks**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p hoimin-cli --lib --test process_handler
git diff --check
```

Expected: all PASS and Unix portable behavior remains unchanged.

- [ ] **Step 8: Commit**

```bash
git add crates/hoimin-cli/src/resource/portable.rs
git commit -m "fix: contain Windows portable children before resume"
```

### Task 3: Focused Mutation and Windows Contract Review

**Files:**
- Modify only if a survivor exposes missing coverage:
  `crates/hoimin-cli/src/resource/portable.rs`
  `crates/hoimin-cli/src/resource/suspended.rs`
- Create ignored local report:
  `.superpowers/sdd/issue-23-task-3-report.md`

**Interfaces:**
- Consumes: final Task 1 and Task 2 Windows startup functions.
- Produces: evidence that viable mutations in the important startup sequence are caught, or an explicit environment limitation with no false coverage claim.

- [ ] **Step 1: Enumerate only the important Windows functions**

On a Windows execution environment with `cargo-mutants` installed, run:

```powershell
cargo mutants --workspace --file crates/hoimin-cli/src/resource/suspended.rs --file crates/hoimin-cli/src/resource/portable.rs --re "configure|SuspendedChild::open|SuspendedChild::assign|SuspendedChild::resume|PortableSupervisor::attach" --list
```

Expected: inventory contains only the shared suspended-startup operations and
portable attach sequence. Record the exact count before execution.

- [ ] **Step 2: Run the focused mutants**

Run on Windows:

```powershell
cargo mutants --workspace --jobs 2 --file crates/hoimin-cli/src/resource/suspended.rs --file crates/hoimin-cli/src/resource/portable.rs --re "configure|SuspendedChild::open|SuspendedChild::assign|SuspendedChild::resume|PortableSupervisor::attach" -- --lib --test process_handler
```

Expected: every viable mutant is caught; no timeout is accepted without
investigation. Inspect every outcome, diff, and log rather than relying only on
the summary.

If no Windows environment can execute `cargo-mutants`, do not run or cite a
macOS inventory that compiles out the target functions. Record:

- the attempted command and environment;
- why behavioral mutation could not execute;
- the Windows RED/GREEN fault evidence used instead; and
- that no mutation score is claimed.

- [ ] **Step 3: Strengthen only missing behavioral contracts**

For each genuine survivor, first add the smallest Windows test that fails with
that mutant. Typical required observations are:

```rust
assert!(!root_marker.exists());
assert!(!descendant_marker.exists());
assert!(started.elapsed() < Duration::from_secs(2));
```

Do not exclude a mutant until its equivalence is demonstrated from Win32
semantics and the exact diff. Re-run only the surviving mutant, then run the
complete focused set fresh.

- [ ] **Step 4: Re-run focused normal tests**

Run on Windows:

```powershell
cargo test -p hoimin-cli resource::portable::tests::attach_failure_prevents_immediate_detached_descendant -- --exact
cargo test -p hoimin-cli resource::windows::tests::assign_and_resume_failures_kill_suspended_root_before_user_code -- --exact
cargo test -p hoimin-cli --test process_handler portable::
```

Expected: all selected tests PASS.

- [ ] **Step 5: Commit test improvements if any**

If Task 3 changed tracked tests:

```bash
git add crates/hoimin-cli/src/resource/portable.rs crates/hoimin-cli/src/resource/suspended.rs
git commit -m "test: strengthen Windows startup containment coverage"
```

If no tracked change was needed, do not create an empty commit.

### Task 4: Final Verification and Pull Request

**Files:**
- Verify all files in `origin/main..HEAD`.
- Do not add branch-specific CI configuration.

**Interfaces:**
- Consumes: Tasks 1–3 and both committed documents.
- Produces: independently reviewed branch and one PR that closes Issue #23.

- [ ] **Step 1: Run fresh local verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
git diff --check origin/main..HEAD
git status -sb
```

Expected: all commands PASS and the worktree is clean.

- [ ] **Step 2: Obtain independent review**

The reviewer compares `origin/main..HEAD` with Issue #23, the design, and this
plan. Required review points:

- target code cannot run before portable Job Object assignment;
- every assignment completes before resume;
- attach/resume failure leaves the root suspended for cleanup;
- hard Windows state ordering is unchanged;
- temporary native handles close on all paths;
- Unix code is unchanged;
- tests cannot pass through timing alone or leave a detached fixture alive.

Expected: no Critical or Important findings. Fix findings with TDD and repeat
the relevant verification before continuing.

- [ ] **Step 3: Push and open the dedicated PR**

Write `.superpowers/sdd/issue-23-pr-body.md`:

```markdown
## Summary

- start Windows portable roots suspended and assign the kill-on-close Job Object before resume
- share the suspended-process primitive with the hard Windows backend without changing its resource policy
- cover assignment failure with an immediate detached-descendant regression

## Verification

- Windows containment RED/GREEN evidence: see implementation report
- focused Windows mutation evidence: see implementation report
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features`
- independent final review: approved

Closes #23
```

Run:

```bash
git push -u origin fix/issue-23-windows-portable-containment
gh pr create \
  --base main \
  --head fix/issue-23-windows-portable-containment \
  --title "fix: contain Windows portable children before execution" \
  --body-file .superpowers/sdd/issue-23-pr-body.md
```

The PR body lists the suspended-startup ordering, Windows RED/GREEN evidence,
focused mutation result or explicit environment limitation, local verification,
and `Closes #23`.

- [ ] **Step 4: Let code-change CI complete**

Run:

```bash
gh pr checks <pr-number> --watch
```

Expected: Windows Quality, Rust, portable process behavior, and all other
required jobs PASS. Do not cancel this code-change CI. If Windows fails, use
`superpowers:systematic-debugging`, fix on the same branch, and repeat fresh
verification and review.
