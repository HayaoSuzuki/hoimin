# Windows Portable Containment Design

## Context

On Windows, the portable resource backend currently creates a Job Object
during `prepare`, but starts the target normally and assigns the already-running
process during `attach`. Target code can therefore execute and create a
descendant between `Command::spawn` and `AssignProcessToJobObject`. A descendant
created in that interval is not retroactively added to the Job Object.

This design resolves GitHub Issue #23 and audit finding `RUST-AUDIT-001`.
It preserves Unix portable behavior and does not change the hard Windows
backend's resource policy.

## Decision

Windows portable startup will use the same suspended-startup primitive as the
hard Windows backend:

```text
configure CREATE_SUSPENDED
  -> spawn root with its primary thread suspended
  -> open the root process
  -> assign the root to every required Job Object
  -> resume the primary thread
```

The primitive will be extracted into a Windows-only resource module rather
than duplicated in `portable.rs`. The hard backend will use it for its
run-wide and nested root Job Objects; the portable backend will use it for its
single kill-on-close Job Object.

Tokio exposes Windows creation flags but does not expose the primary thread
handle returned by `CreateProcessW`. The shared primitive will retain the
existing hard-backend technique: identify the sole thread of the newly created
`CREATE_SUSPENDED` process with a Toolhelp thread snapshot, open it with
`THREAD_SUSPEND_RESUME`, and call `ResumeThread` only after all assignments
succeed.

## Shared Primitive

A Windows-only module under `resource` owns the mechanics that are independent
of resource policy:

- add `CREATE_SUSPENDED` to a `tokio::process::Command`;
- obtain and validate the spawned root PID;
- open and own the process handle required for Job Object assignment;
- assign that process handle to a caller-supplied Job Object with a
  caller-supplied operation label; and
- find and resume the suspended primary thread.

The primitive does not create, configure, terminate, or close Job Objects.
Those remain backend responsibilities. It also does not decide how many jobs a
root joins: portable assigns one job, while the hard backend assigns its
run-wide job and nested root job before calling resume.

This boundary keeps the ordering invariant explicit: no backend can resume a
child until its complete assignment sequence has succeeded.

## Ownership and Failure Semantics

The `tokio::process::Child` remains owned by `ProcessHandler`; the shared
primitive owns only temporary process, snapshot, and thread handles. Temporary
handles close on every return path.

Before `attach`, the root is suspended and cannot execute user code or create a
descendant. Failure behavior is:

- opening the root fails: return the attach error while the root is still
  suspended;
- any Job Object assignment fails: return the attach error without resuming;
- locating, opening, or resuming the primary thread fails: return the attach
  error without claiming successful startup;
- all assignments and resume succeed: normal process handling begins.

On any attach failure, the existing `ProcessHandler` cleanup remains
responsible for directly killing and waiting for the root. Dropping the
portable supervisor also closes its kill-on-close job. Since user code never
ran before a failed assignment or resume, there can be no pre-assignment
descendant to escape cleanup.

If assignment succeeded but resume failed, closing or terminating the Job
Object provides an additional containment cleanup path. The first attach error
continues to be the returned failure; this issue does not change error
precedence or the post-termination cleanup policy tracked separately by
Issue #24.

## Backend Changes

### Portable Windows

`PortableBackend::prepare` configures `CREATE_SUSPENDED` on Windows before
returning its supervisor. `PortableSupervisor::attach` opens the suspended
root, assigns it to the portable kill-on-close job, then resumes it.

Unix `setpgid` and rlimit configuration are unchanged. Non-Windows,
non-Unix behavior is unchanged.

### Hard Windows

The hard backend replaces its private copies of suspended command setup,
process opening, assignment, and primary-thread resume with the shared
primitive. Its locking, completion-port accounting, resource-limit jobs,
classification, and active-root bookkeeping remain unchanged.

The hard backend still records a root as active before resume and removes it
when resume fails. Extraction must preserve those state transitions exactly.

## Deterministic Windows Regression

A Windows-only test fault delays portable attachment and then returns an
injected assignment error. It exists only in test builds and is not exposed
through CLI configuration.

The regression child immediately:

1. writes a root marker;
2. starts a detached descendant that writes a second marker; and
3. waits.

The test repeats the scenario enough times to exercise rapid startup. Against
the old spawn-then-attach path, the injected pre-attach delay lets root code and
the detached descendant run, producing the markers and demonstrating the
containment failure. Against the suspended path, neither marker can appear:
the injected attach failure occurs before resume, and cleanup reaps the
suspended root.

Separate success coverage proves that a normally attached portable child is
resumed and retains its existing exit and output behavior. Existing hard
backend assignment/resume-failure tests must continue to prove that user code
does not run on either failure.

The test delay is a deterministic race amplifier, not a production sleep. The
assertion is based on marker absence and bounded cleanup rather than relying
only on timing.

## Verification

Development follows red-green order on a Windows runner:

- first run the regression against spawn-then-attach and record marker
  creation;
- enable suspended portable startup and prove both markers remain absent;
- prove successful portable children resume and complete normally;
- rerun the hard Windows attach/resume failure contracts;
- run all Windows portable process tests and the workspace test suite.

Formatting, all-target/all-feature Clippy, and the full local workspace suite
run on the development host. Windows compilation and behavior are verified by
the repository's code-change CI and are not canceled.

Focused mutation testing is limited to the shared suspended-startup operations
and the portable attach sequence. Because those paths are `cfg(windows)`,
behavioral mutation results must come from a Windows execution environment;
the implementation plan must not report a macOS inventory that excludes the
changed code as meaningful coverage. If the available Windows runner cannot
install or execute `cargo-mutants`, the limitation is recorded explicitly and
the deterministic fault tests remain the required test-quality evidence.

## Non-goals

- Do not redesign hard Windows resource limits, accounting, or completion-port
  handling.
- Do not change Unix portable process groups or rlimits.
- Do not change cancellation, timeout, output, or error precedence.
- Do not add a custom replacement for Tokio process spawning.
- Do not address cleanup after a supervisor termination error; Issue #24 owns
  that behavior.
- Do not claim that portable resource limits become equivalent to hard mode.
