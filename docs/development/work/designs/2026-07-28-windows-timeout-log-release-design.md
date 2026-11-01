# Windows Timeout Log Release Design

## Context

GitHub Issue #47 tracks an intermittent Windows failure in
`RunnerTests.test_timeout_terminates_process_and_keeps_partial_logs`. The
timeout assertions completed, but `TemporaryDirectory` cleanup then failed
with `WinError 32` while deleting a command stderr log. Rerunning the unchanged
CI job succeeded.

`CommandRunner` currently terminates and waits for the timed-out child while
its parent-side stdout and stderr streams are still open. It raises
`CommandTimedOut` from inside the stream context, so the parent streams close
only while that exception unwinds. The runner does not verify that Windows can
delete the log files before returning control to its caller.

This design makes log-file cleanup readiness part of the Python
`CommandRunner` lifecycle. It does not change the Rust process supervisor.

## Decision

After the child has exited and the parent-side log streams have closed,
`CommandRunner` will verify on Windows that both log paths can be opened with
delete access. A sharing violation means an inherited or transient handle can
still prevent directory cleanup, so the runner retries the check with a short
polling interval up to a fixed deadline.

The check uses the Windows file API without deleting, renaming, truncating, or
otherwise modifying either log. Non-Windows platforms skip it.

The guarantee is bounded:

- when readiness succeeds, `CommandRunner` returns or raises its primary
  lifecycle exception only after both logs are deletable;
- when the deadline expires, it preserves the primary result and records an
  explicit cleanup failure instead of retrying indefinitely or suppressing the
  condition.

## Runner Lifecycle

`CommandRunner.run` will separate process execution from result delivery:

```text
open stdout and stderr
  -> spawn and wait
  -> on timeout or interruption, terminate/kill and reap
  -> complete the command record
  -> close parent stdout and stderr
  -> on Windows, wait boundedly for both logs to become deletable
  -> attach any cleanup failures to the command record
  -> return, raise CommandTimedOut, or raise CommandInterrupted
```

Deferring result delivery until after the stream context exits is required:
the runner's own open streams must not cause the readiness probe to fail.

The same post-close readiness step applies to normal completion, timeout, and
interruption. Existing timeout and interruption exception types remain the
primary outcomes. Partial stdout and stderr content remains available.

## Windows Readiness Probe

A small Windows-only helper attempts to acquire a handle to an existing log
with delete access and permissive sharing. Successful acquisition and closure
proves that the path is currently deletable. A Windows sharing violation is
retryable; other operating-system errors are reported immediately as cleanup
failures.

The runner checks stdout and stderr independently so diagnostics identify the
affected path. Waiting uses injected monotonic-clock and sleep dependencies in
tests. Production defaults use `time.monotonic` and `time.sleep`.

The polling interval and total readiness timeout are fixed internal lifecycle
constants. They are not CLI options: this is a narrow platform cleanup
invariant rather than a user-tunable execution policy.

## Error Precedence and Persistence

`CommandRecord` gains:

```python
cleanup_errors: list[str] = field(default_factory=list)
```

The field is always encoded in `run.json`; successful cleanup produces an
empty array.

If readiness fails after a command timeout, `CommandTimedOut` remains the
raised exception and its existing leading message remains unchanged. The
associated record contains path-specific cleanup diagnostics. The same rule
applies to `CommandInterrupted`.

For a normally completed command, a readiness failure does not rewrite the
child exit code. It is retained in `cleanup_errors` so callers and persisted
evidence can distinguish command behavior from lifecycle cleanup behavior.

This change does not broadly catch or suppress `PermissionError` during test
directory removal, and it does not add CI job retries.

## Deterministic Regression Coverage

Portable unit tests inject the readiness probe, monotonic clock, and sleeper:

- sharing violations followed by success prove the runner polls and does not
  deliver its result early;
- a deadline-expiry case proves `CommandTimedOut` remains primary and the
  command record receives stdout/stderr-specific cleanup failures;
- normal completion and interruption exercise the same post-close lifecycle;
- successful readiness leaves `cleanup_errors` empty.

A Windows-only lifecycle regression runs a helper that causes an inherited log
handle to remain live briefly after the timed-out root exits. The test proves:

1. timeout termination and reaping still occur;
2. partial stdout and stderr remain readable;
3. the runner waits until the inherited handle is released; and
4. immediate `TemporaryDirectory` cleanup succeeds without a test-level retry.

The helper uses explicit synchronization for handle acquisition and release.
Timing supplies only the bounded safety limit, not the condition asserted by
the test.

Existing runner tests continue to cover native arguments, nonzero exits,
working-directory recording, timeout termination, interruption, and the kill
fallback.

## Verification

Development follows red-green order:

1. add the deterministic lifecycle and injected-probe regressions;
2. run the focused runner tests and observe the new expectation fail;
3. implement post-close Windows readiness checking;
4. rerun the focused runner and reporting tests;
5. run the complete Python test suite on Windows.

The final PR also runs the repository's configured formatting, linting, typing,
and test checks. The Windows CI job is required evidence for the platform API
and lifecycle regression.

## Non-goals

- Do not modify the Rust process supervisor or resource backends.
- Do not introduce broad CI or test retries.
- Do not suppress `PermissionError` from `TemporaryDirectory`.
- Do not wait without a fixed upper bound.
- Do not delete, rename, or truncate logs as part of readiness checking.
- Do not change command timeout classification or child exit-code semantics.
