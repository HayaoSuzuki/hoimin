# Issue #221 Windows Abnormal Root Exit Design

## Context

The Windows resource backend already classifies
`JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS` as a root exit. Its regression coverage,
however, calls `record_notification` with synthetic state and PIDs. That proves
the state transition but does not prove that a real process assigned through
`ProcessHandler` produces and crosses the production completion-port path.

The existing Windows `job_object` integration tests exercise real memory,
process-limit, timeout, and close behavior. The analogous Linux hard-backend
test also covers an abnormal runtime root, but Windows has no equivalent real
fixture.

## Chosen Approach

Add one Windows-only test beside the backend's existing tests in
`crates/hoimin-cli/src/resource/windows.rs`. Keeping the test in this module
lets it retain a clone of the production `WindowsBackend` and query the real
Job Object's `ActiveProcesses` count without adding a public test hook to the
production API.

The test starts a long-lived sibling through the same `ProcessHandler` and
waits for the sibling to atomically publish its real PID. It then starts the
root under test. That root atomically publishes its own real PID and calls
`TerminateProcess` on itself with Windows fail-fast status `0xC0000409`.
Because the sibling remains assigned, the run Job Object cannot emit a valid
`ACTIVE_PROCESS_ZERO` fallback while the abnormal root is classified. A
successful result therefore depends on consuming the real PID-specific
abnormal-exit notification.

Alternatives rejected:

- A single crashing root could pass through the aggregate
  `ACTIVE_PROCESS_ZERO` fallback even if the abnormal-exit branch regressed.
- An integration test in `tests/process_handler.rs` cannot directly query the
  private production Job Object and would require either a weaker liveness
  assertion or a new public test-only API.
- Direct `RunState` mutation, `record_notification`, and synthetic PIDs repeat
  the existing coverage and do not satisfy issue #221.

## Fixture and Data Flow

Both fixture modes use the configured Python interpreter and receive a
readiness path as a native Windows argument. Each writes its actual PID to a
neighboring `.pending` file and uses `os.replace` to publish the requested
readiness path atomically.

The sibling sleeps long enough that it cannot finish during the six-second
test window. The abnormal root initializes the required `kernel32` signatures,
publishes readiness, and terminates itself with `0xC0000409`. The expected Rust
termination is the independently derived signed value
`ProcessTermination::Exit(-1_073_740_791)`.

## Bounds and Cleanup

A six-second `tokio::time::timeout` encloses sibling readiness, abnormal-root
handling, `ProcessHandler::close`, sibling completion, and Job Object
quiescence. Before close, the kernel accounting query must report exactly one
active process: the sibling. After close, the same query must reach zero. The
test also records and checks the elapsed time for the abnormal `handle` call
and synchronous `close` call individually.

A test guard closes the handler if an assertion unwinds after the sibling has
started. Production `KILL_ON_JOB_CLOSE` ownership remains the cleanup
mechanism; the test never signals a numeric PID.

## Verification

The focused Windows test must first be observed RED before fixture helpers are
implemented, then GREEN after the minimal fixture implementation. Final
verification runs the focused test, all `hoimin-cli` tests, the workspace test
suite, formatting, Clippy with warnings denied, and diff checks.

This change is expected to touch only Windows test code and documentation. If
the final diff contains no changed Rust production source, Rust production
mutation testing has no eligible candidate and will be reported as
non-applicable with the changed-file diff as evidence.
