# RunFinished Terminal Semantics Design

## Context

Issue #52 identifies two representations of the same missing invariant:

- the run state machine can process cancellation or deadline events while final output is pending or after `RunPhase::Finished`;
- `ReportSequence` validates ordering and mutant pairing but does not make `RunFinished` terminal.

The final report is a public machine-readable boundary. Once emission has started, later stop signals must not change its selected outcome or schedule another cleanup/report cycle.

## Design

### State machine

`transition` will treat `DeadlineReached` and `CancellationRequested` as idempotent no-ops when either:

- `run_finished_output_id` is present, meaning final report emission is already pending; or
- `phase == RunPhase::Finished`, meaning final output has been acknowledged.

The guard precedes outcome-flag mutation and pending-effect retirement. This preserves the already selected exit code and keeps the one pending final output registered until its acknowledgement. Other late completions retain their existing typed duplicate/retired behavior.

No new phase or public event is introduced. The state machine continues to move to `Finished` only when the matching final `OutputEmitted` arrives.

### Report sequence

`ReportSequence` will track whether `RunFinished` has been observed. It will add two typed errors:

- `RunAlreadyFinished` for every event after the terminal event;
- `RunFinishedWithActiveMutants` when termination is attempted while mutant starts remain unmatched.

Validation remains transactional: an invalid event does not update sequence, terminal, or active-mutant state. Successful `RunFinished` records the terminal state.

## Error precedence

For `ReportSequence`, terminal-state rejection precedes run ID, mutant lifecycle, and monotonicity checks because no event is legal after termination. On the first `RunFinished`, active-mutant rejection occurs before monotonicity state is committed.

For the state machine, late stop signals after final-report scheduling return the unchanged state and no effects. They do not set `incomplete`, `interrupted`, or `stop_requested`.

## Testing

- State-machine tests drive a no-candidate run through cleanup to pending final output, then inject cancellation and deadline independently. They assert no effects, unchanged exit code, a still-pending final output, and successful acknowledgement into `Finished`.
- A finished-state test injects both stop signals and asserts the phase/outcome remain terminal.
- Report tests assert rejection of duplicate terminal output, post-terminal diagnostics, and termination with an active mutant. A valid completed sequence remains accepted.

## Compatibility

The JSON schema and event shapes do not change. Only previously invalid lifecycle sequences are rejected, and stop signals that arrive after terminal output selection no longer rewrite the outcome.
