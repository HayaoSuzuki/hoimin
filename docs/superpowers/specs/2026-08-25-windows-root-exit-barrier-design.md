# Windows Root Exit Barrier Design

## Problem

The Windows resource backend treats a run-wide Job Object completion-port exit message as the only proof that a root process has exited. `ProcessHandler` calls this synchronous barrier after `tokio::process::Child::wait` has already returned. Ordinary Job Object completion-port messages are not a delivery guarantee, so the backend can wait for a message even though it owns a signaled process handle for the same root generation.

The wait calls `GetQueuedCompletionStatus` for as long as one second while holding the run state mutex. It also runs on the async executor thread. Parallel roots can therefore prevent unrelated stdout and stderr drain tasks from being polled until their independent one-second post-termination deadline expires. The Windows CI failure in run 32796856115 demonstrates this: the root termination was known as `Exit(1)`, but output remained at zero bytes and was classified as `process.output.close.timeout` after 1300 ms.

The notification timeout and output timeout are symptoms. Increasing either timeout, retrying the test, or serializing it would preserve the incorrect ownership model.

## Decision

Introduce an internal `RootExitBarrier` result for Windows root classification. The barrier has two successful outcomes:

- the exit notification for the generation was consumed;
- the owned process handle for the generation is signaled before that notification is observed.

The barrier first consumes already queued notifications and preserves all existing resource-limit notification handling. If no matching exit notification is available, it checks the owned generation process handle. A signaled handle is authoritative proof that the exact process object exited; it does not depend on PID spelling or PID lifetime.

When the notification was consumed, classification removes the completed generation as before. When only the process handle is signaled, classification detaches the generation's signal but retains its UUID, PID, and owned handle in run state. A later PID-specific exit notification consumes that detached generation and closes the handle. Retaining the handle prevents Windows from recycling the PID while the delayed notification can still arrive, so the notification cannot be applied to a newer generation. If the notification never arrives, the detached generation remains bounded by the number of roots in the run and is released when the backend is dropped.

The process handle is only a fallback for exit completion. Job Object notifications remain the source of memory-limit and active-process-limit classification, and the nested root completion port is drained through the existing classification path. The run-wide `ACTIVE_PROCESS_ZERO` accounting check and generation ordering remain unchanged.

## State Transitions

For a live generation awaiting classification:

1. A matching exit or confirmed `ACTIVE_PROCESS_ZERO` notification produces `NotificationConsumed`; classification reads violations and removes its exited-generation marker.
2. A signaled owned process handle without a matching notification produces `ProcessSignaled`; classification reads violations and converts the active generation to a detached tombstone.
3. Neither condition retains the existing bounded notification wait and timeout error. This is defensive for callers that violate the normal `Child::wait`-before-classify ordering.

For a detached generation:

1. A delayed PID-specific notification removes the oldest matching generation and drops its handle.
2. A reused PID cannot be registered by Windows while the old process object handle remains open.
3. Run close or backend drop releases any notification that was never delivered.

## Compatibility

No public API, configuration, dependency, timeout value, resource-limit status, output-close contract, or Job Object close behavior changes. The change is confined to the Windows resource backend, its tests, and this design record.

## Verification

TDD begins with a real ended process handle and an empty completion port. On the current implementation the classification barrier reaches `root exit notification timed out`. The corrected implementation returns the original termination without waiting for an unavailable notification and retains the generation with its handle detached.

The regression must fail if any of these changes are introduced:

- the process-handle fallback is removed;
- a notification-consumed generation is retained;
- a handle-signaled but unnotified generation is removed;
- a detached generation loses its owned process handle;
- a delayed notification is applied to a newer reused-PID generation.

Existing memory-limit, process-limit, abnormal-root, root-before-descendant, timeout cleanup, and close tests remain green. The failing E2E is repeated on Windows without adding repetition or retries to CI. Formatting, Clippy, the workspace suite, the independent `run_e2e` suite, and focused mutation testing cover the final production diff.
