# Issue #332: Move preflight I/O off the async dispatch loop

## Problem

`RunEffect::Preflight` currently executes `WorkspacePlan::preflight_validated`
inside `execute_effect_with_cancellation`. That operation canonicalizes the
root, builds and hashes a manifest, validates fingerprint inputs, snapshots the
selected files, and builds the manifest again. All of those filesystem passes
are synchronous.

Because the future performs that work in one poll, the surrounding
`tokio::select!` cannot observe cancellation, the total-timeout deadline, or an
interrupt until preflight returns. On a large tree this makes the first Ctrl+C
appear ineffective and lets the total timeout overrun by the entire preflight
duration.

## Goals

- Execute all synchronous preflight filesystem and hashing work on Tokio's
  blocking pool.
- Keep cancellation, deadline, and interrupt polling responsive while
  preflight is running.
- Preserve validated-preflight ABA checks and the exact run fingerprint.
- Return `WorkspaceHandler` ownership on success, failure, cancellation drain,
  and stale completion drain.
- Preserve effect IDs and existing machine transition semantics.

## Non-goals

- Making a filesystem walk cooperatively cancellable once it has started.
- Changing shutdown-grace duration or forced-interrupt behavior.
- Moving target discovery in this issue.
- Changing the workspace manifest, snapshot, or fingerprint algorithms.
- Allowing another workspace effect to overlap preflight.

## Options considered

### Transfer the workspace into a dedicated blocking preflight effect

Take the `WorkspaceHandler` from `ShellContext`, move it together with owned
copies of the config, resolved targets, copied-input set, and resource mode into
a `BlockingEffect::Preflight`, and return the workspace with its completion.
Perform both validated preflight and source fingerprint reads inside that
blocking task.

This is the recommended design. It follows the existing cleanup ownership
pattern, keeps the plan installation inside `WorkspaceHandler`, and ensures the
shell cannot access the workspace concurrently while preflight mutates it.

### Add preflight to `WorkspaceTask`

`WorkspaceTask` could gain a preflight variant and return an optional plan in
`WorkspaceTaskCompletion`. The shell-specific fingerprint validation and
resource-mode inputs would then have to cross into the workspace module through
callbacks or a second completion phase. This expands the module boundary for no
additional safety and is rejected.

### Split only the manifest walks into blocking calls

Each pass could be spawned separately while `WorkspaceHandler` remains in the
shell. That would require exposing intermediate manifests and snapshots,
reconstructing the ABA ordering in the shell, and adding cancellation points
inside an operation that is currently atomic from the handler's perspective.
It is rejected as a larger semantic change.

## Detailed design

Classify `RunEffect::Preflight` as blocking I/O. During preparation:

1. Require resolved targets and capture owned copies of all data needed for
   fingerprint validation and construction.
2. Take the sole `WorkspaceHandler` from `ShellContext`.
3. Build a `BlockingEffect::Preflight` containing the original request and the
   captured state.

The blocking task calls `handle_preflight_validated` with the existing
fingerprint-input recheck. If it succeeds, it synchronously reads the resolved
source files on the same blocking thread, computes the same BLAKE3 source
hashes, constructs the same `FingerprintInput`, and attaches the resulting
fingerprint to `PreflightCompleted`. Fingerprint errors retain the preflight
effect ID exactly as today.

Both success and failure return an owned-workspace completion. Acceptance first
restores the workspace to `ShellContext`, rejecting duplicate ownership, and
then exposes the event to the machine. Shutdown drain uses the same acceptance
path, so a completion retired by an earlier stop still restores shell resource
ownership without sending the stale event through the machine.

The scheduler already counts and drains blocking I/O tasks and enforces the
shutdown budget around them. No new cancellation protocol is required. A stop
does not abort an in-progress filesystem syscall; it becomes observable
immediately and then waits for the owned task under the existing shutdown
grace.

## Test design

Add a test-only preflight gate at the beginning of validated preflight. A
controller thread waits a bounded interval for a runtime heartbeat and then
releases the gate. On a current-thread Tokio runtime, run
`execute_effect(Preflight)` concurrently with that heartbeat:

- before the fix, the synchronous preflight poll blocks the runtime and the
  heartbeat is delayed until the gate is released;
- after the fix, preflight waits on the blocking pool and the heartbeat fires
  before release.

Also assert that the final event is `PreflightCompleted`, the fingerprint is
present, and the workspace is restored. Keep focused tests for ABA input
validation, blocking completion drain, and preflight classification. Run
focused mutation testing against the new classification and owned completion
path.
