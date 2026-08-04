# Windows Real PID Reuse Feasibility Design

## Status

Issue [#223](https://github.com/tokyogas-tech/hoimin/issues/223) cannot satisfy its
acceptance criteria on a supported Windows CI runner. It remains the explicit
Windows platform remainder for
[#162](https://github.com/tokyogas-tech/hoimin/issues/162). This decision does
not close either issue and does not weaken the acceptance criteria.

## Required boundary

The requested fixture must establish all of these facts in one bounded run:

1. A real process is assigned through the production Windows Job Object path.
2. The process exits while its generation and durable process handle remain in
   delayed cleanup state.
3. A second real process receives the same numeric PID as the exited process.
4. Production cleanup retires only the old generation after crossing the real
   completion port.
5. Real handles prove the new generation remains active and was not terminated.

Synthetic PIDs, direct `RunState` mutation, direct `record_notification` calls,
unbounded PID churn, and timing-dependent retries are excluded.

## Production invariant

`SuspendedChild::open` opens the process handle used for Job Object assignment.
After both assignments and resume, `register_root` moves that handle into
`ActiveRoot::process`. A root that has exited but whose completion message has
not been consumed therefore remains represented by its UUID generation, PID,
weak signal, and owned process handle. `drain_pending` or
`drain_until_root_exit` removes the matching `ActiveRoot`; dropping that entry
closes the handle. Only then can cleanup forget or classify that generation.

The relevant implementation is in
`crates/hoimin-cli/src/resource/suspended.rs` and
`crates/hoimin-cli/src/resource/windows.rs`:

- `SuspendedChild::open` obtains a real process handle.
- `WindowsRunJob::attach_root` assigns that handle to the run-wide and nested
  jobs before registering it.
- `register_root` retains the handle in delayed cleanup state.
- `record_notification` matches the oldest registered generation for an exit
  PID and removes exactly that entry.
- `ActiveRoot::drop` closes the retained handle.

This is the production safeguard introduced by `8a774fc` (`fix: track Windows
root processes by generation identity (#208)`). Removing the handle before the
notification is consumed would remove the safeguard the requested test is
supposed to validate.

## Windows API constraints

Microsoft's
[`PROCESS_INFORMATION`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/ns-processthreadsapi-process_information)
contract states that a PID remains valid until all process handles are closed
and the process object is freed; only then may the identifier be reused.
Microsoft's
[`JOBOBJECT_ASSOCIATE_COMPLETION_PORT`](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_associate_completion_port)
documentation gives the corresponding notification rule: a completion-port
PID cannot be guaranteed unrecycled unless the consumer maintains an open
process handle.

Consequently, the required state is contradictory:

```text
old delayed generation owns a durable handle
    => old process object is not freed
    => old PID is not eligible for reuse
    => no second real process can have the old PID
```

Closing the old handle makes PID reuse possible, not enforceable. The supported
[`CreateProcessW`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)
API has no requested-PID input: the system assigns the PID and returns it in the
output `PROCESS_INFORMATION`. Windows exposes no supported user-mode API that
reserves a PID or directs the allocator to reuse a selected PID.

Completion-port delivery adds a separate determinism limit. Microsoft documents
ordinary Job Object messages, including process-exit messages, as notifications
whose delivery is not guaranteed. Hoimin associates the port before assigning
processes and bounds the notification barrier, which avoids the documented
association race, but it cannot turn notification delivery into a platform
guarantee.

## Considered approaches

### Retain the production handle barrier

Keep the current UUID-plus-handle generation tracking. This uses supported APIs,
matches Microsoft's guidance, and prevents stale PID cleanup from targeting a
new process. It cannot manufacture the forbidden simultaneous state. This is
the selected disposition: preserve the implementation and retain #223 as a
platform remainder.

### Close the handle and churn PIDs

Close `ActiveRoot::process` after exit, then create processes until Windows
happens to return the old PID. This violates the durable-handle and production-
path requirements, reintroduces the stale-PID window, and supplies no fixed
upper bound. A nominal maximum iteration count only converts the test into a
probabilistic skip or failure and remains the PID-churn fixture the issue
explicitly rejects.

### Control or simulate the allocator

Hook `CreateProcessW`, inject completion packets, use synthetic PIDs, mutate
private state, or patch kernel process tables. The first three are expressly
excluded and do not exercise the operating-system boundary. Kernel or
hypervisor instrumentation is unsupported, unavailable on `windows-latest`,
and would exercise a modified allocator rather than the production CI
environment.

## Bounded evidence that remains valid

The existing Windows tests
`attach_failures_kill_suspended_root_and_retain_assigned_identity_until_exit`
and `timed_out_root_retains_identity_until_its_exit_notification` start real
processes, retain the production `OwnedHandle`, and drain the real completion
port within fixed two-second and one-second bounds. They demonstrate the
reachable handle-retention path but not real PID reuse, so they do not satisfy
or close #223.

A bounded local probe retained an exited real process handle, verified its PID
and exit state through Win32 handle APIs, then created exactly 64 additional
real processes. None received the retained PID. That observation corroborates
the documented contract but is not proposed as a regression test: the API
contract and production ownership graph are the proof, and a finite churn test
would not add issue-closing evidence.

## Decision and reopening condition

Do not change Rust code or add a test that claims the issue is covered. Keep
#223 open and linked from #162. Reconsider implementation only if a supported
Windows environment available to CI can deterministically allocate a requested
real PID while preserving an independent durable handle to the retired process
object, or if the issue owner explicitly revises the acceptance boundary to test
the reachable invariant that the retained handle prevents reuse.

## Verification

Verify the disposition with the Windows-focused tests, workspace formatting,
strict Clippy, and the workspace test suite. Rust mutation testing is not
applicable because this disposition changes no Rust production code or tests.
