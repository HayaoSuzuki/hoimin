# Issue #331: Preserve queued effects across stop transitions design

## Problem

The shell checks cancellation and the total deadline immediately after popping
an effect from its ready queue. When a stop is observed, it discards that
effect and sends the stop event to the machine.

Most stop transitions retire ordinary pending effects, so discarding them is
correct. A pending final-report output is different: the machine intentionally
treats a late stop as a no-op and leaves the original `EmitOutput(RunFinished)`
pending. The shell has already lost the only copy of that effect. With no queue
entry or in-flight task, the next iteration reports `run stalled in Finalize`
and never writes the final report.

The same ownership rule now applies to cleanup after Issue #330: a stop during
`Cleaning` preserves its pending cleanup obligation.

## Goals

- Restore an effect popped by the pre-dispatch stop check until the machine has
  decided whether it remains pending.
- After a stop or failure transition, retain exactly the queued effects the
  resulting `RunState` still identifies as pending.
- Preserve FIFO order and existing machine effect IDs.
- Cancel queued-mutant metrics only when the corresponding effect is actually
  retired.
- Produce a final report rather than stalling when cancellation wins before a
  queued `RunFinished` output is dispatched.

## Non-goals

- Reopening or re-emitting effects the machine retired.
- Changing the core late-stop semantics or exit-code policy.
- Prioritizing ordinary completion over a stop that arrives before dispatch.
- Changing in-flight process or blocking-I/O drain behavior.
- Making synchronous report writes preemptible.

## Options considered

### Requeue, transition the stop, then filter by machine pending state

Put the just-popped effect back at the front of the queue before injecting the
stop. After the machine accepts the stop, remove queued effects for which
`state.is_effect_pending(effect.id())` is false and retain the rest.

This is the recommended design. The machine remains the sole authority on
effect retirement, retained IDs are unchanged, and existing metrics cleanup
runs only for effects actually discarded.

### Dispatch the popped effect before the stop

This would avoid losing the final report, but for ordinary effects it could
start new worker or mutation work after cancellation was observed. It reverses
the scheduler's stop priority and is rejected.

### Always requeue the popped effect

Unconditionally dispatching it on the next loop would feed completions for
retired effects back to the machine, producing `machine.effect.retired` errors.
It would also leave queued process metrics open. This option is rejected.

### Reconstruct only `RunFinished` after the transition

Special-casing the output variant would duplicate the pending-state rule in
the shell and miss preserved cleanup or future terminal obligations. It is
rejected in favor of the generic public pending query.

## Detailed design

Immediately after `effects.pop_front()`, a test-only observation hook may
request cancellation to reproduce the scheduling boundary deterministically.
Production behavior is unchanged by the hook.

If the normal pre-dispatch stop check fires, push the owned effect back with
`push_front` instead of calling `cancel_queued_effect`. Establish the shutdown
budget and process the priority stop as today.

Replace unconditional queue draining after an external stop or failed effect
with a helper that applies this rule to every queued effect:

- retain it when the post-transition state reports its ID pending;
- otherwise call `cancel_queued_effect` and remove it.

The stop transition's newly produced effects are appended afterward. A late
stop with a pending final output produces no new effects, so the retained
`RunFinished` remains the next ready effect. Because `stop_signalled` is true,
the next iteration dispatches it under the established shutdown budget and its
`OutputEmitted` completion can finish the run.

For an ordinary stop, the machine retires the old queue. The helper removes
those effects and the stop transition's finalize or cleanup effects replace
them exactly as before.

## Serial execution boundary

`EmitOutput` is synchronous inside the first poll of the biased serial select.
Once dispatch begins, its completion wins before cancellation can be selected;
the existing serial-output cancellation regression covers that boundary. The
reported failure is therefore the earlier pop/pre-dispatch window. Async serial
effects can lose their drained completion only after a stop transition that
retires them, so they remain benign under the current transition contract.

## Tests

Add a test-only `RunControl` hook that cancels exactly when a queued
`EmitOutput(RunFinished)` is popped. Run a successful zero-candidate project
under a bounded timeout and assert:

- the run returns normally rather than `run stalled in Finalize`;
- a parseable final report is written exactly once;
- the test hook fired at the intended effect;
- stderr has no stall or infrastructure diagnostic.

Keep the existing serial-output cancellation test as coverage for cancellation
triggered during output execution. Add a queue-filter unit test if needed to
pin retained order and mutant-metric cleanup independently.
