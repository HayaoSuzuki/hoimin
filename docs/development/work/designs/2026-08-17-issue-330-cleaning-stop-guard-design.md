# Issue #330: Stop events during normal cleanup design

## Problem

The machine ignores deadline and cancellation events during `Cleaning` only
after `stop_requested` is already set. A successful run enters `Cleaning` with
that flag false, so its first stop event retires the pending `Cleanup` effect
and starts finalization again when a copy grant exists.

The shell must still drain the in-flight blocking cleanup. Its completion is
discarded as retired, while cleanup consumes the workspace plan. The newly
issued `VerifyOriginals` then observes a missing worker and turns an otherwise
successful or timed-out run into an infrastructure failure.

## Goals

- Keep the active cleanup effect pending when a deadline or cancellation first
  arrives during any `Cleaning` phase.
- Avoid reissuing finalization or cleanup effects after terminal teardown has
  begun.
- Preserve the already determined outcome and exit code.
- Continue accepting the original `CleanupFinished` event normally.
- Cover both deadline and cancellation events on a successful cleanup path.

## Non-goals

- Making filesystem cleanup interruptible.
- Changing stop handling before the machine reaches `Cleaning`.
- Changing repeated-stop handling after final output or in `Finished`.
- Altering shell drain behavior or workspace-plan ownership.

## Options considered

### Ignore stop events throughout `Cleaning`

Extend the transition precondition so `DeadlineReached` and
`CancellationRequested` return the current state with no effects whenever the
phase is `Cleaning`, regardless of `stop_requested`.

This is the recommended design. Cleanup is already terminal teardown, the
shell drains it before exit, and the machine cannot safely replace it after
the workspace handler has begun consuming its plan.

### Record the stop outcome but keep cleanup pending

The transition could update `incomplete` or `interrupted` without retiring the
cleanup. This would make a signal that arrives after the run outcome is already
finalized retroactively change an exit status even though it cannot shorten
teardown. It also complicates an otherwise idempotent terminal guard and is
rejected.

### Re-run finalization after cleanup completes

Retaining the current stop transition and teaching the shell to preserve or
reconstruct the workspace plan would duplicate verification and cleanup work.
It expands both layers to support a transition that has no useful effect on
shutdown latency. This option is rejected.

## Detailed design

At the top of `transition`, the existing terminal stop-event guard already
returns the unchanged state and no effects for a pending final report, a
finished run, and a stopped cleaning run. Replace the conditional cleaning arm
with an unconditional `state.phase == RunPhase::Cleaning` check.

Because this check runs before `accept_completion`, the original cleanup effect
remains pending and is not retired. No scheduling or outcome flags change. The
subsequent `CleanupFinished` event therefore completes the same effect and the
machine proceeds to its final report using the outcome established before
cleanup.

## Formal oracle and implementation correspondence

The durable claim is: once terminal cleanup is pending, either stop event is
an exact no-op until that cleanup completes. The model includes phase, pending
effect identity and kind, stop cause, emitted effects, and cleanup-to-final
ordering. It excludes filesystem I/O, wall-clock delivery, process supervision,
and the shell's blocking-task drain.

The existing Lean state-machine model treats `Cleaning` with an unset stop
cause like `Running`, and its prior report explicitly excluded the first stop
during normal cleanup. Issue #330 adds the missing real-system premise: the
shell cannot preempt cleanup and the handler consumes the workspace plan. The
contract therefore changes deliberately rather than weakening the model to
match an implementation accident.

Update `Model.stop` so every cleaning stop is unchanged. Add a theorem that
the state and emitted effects are preserved, plus a deliberately broken
old-behavior witness. Generate strict deadline and cancellation cases from a
normal-cleaning state.

The Rust adapter adds a `normal_cleaning_with_copy` scenario. It reaches the
state through the public `RunState` and `transition` API: target resolution,
preflight, worker creation, materialization verification, successful baseline,
empty analysis, and pre-final verification. It does not construct private
machine state or encode expected observations. The Lean-generated corpus
remains the sole source of expected phase, emission, and pending counts.

## Tests

Construct the ordinary successful empty-candidate path through analysis and
pre-final verification until its cleanup effect is pending. For both deadline
and cancellation:

- assert the stop transition returns no new effects;
- assert the phase and exit code remain `Cleaning` and success;
- assert the original cleanup remains pending and is not retired;
- complete that cleanup and assert final reporting proceeds normally.

The existing stopped-cleanup regression remains as coverage for the prior
guarded case.
