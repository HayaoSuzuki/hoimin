# Issue 260 Bounded Shutdown Design

## Goal

Make `--total-timeout` a reliable upper bound for the normal run plus a fixed,
documented shutdown grace. Cancellation and fatal-stop drains receive the same
bound. Preserve the current orderly report, session, workspace, and descendant
cleanup whenever it finishes inside that grace.

## Current failure

`run_loop_prepared` notices the total deadline or cancellation, cancels process
work, and then enters orderly shutdown. Several awaits after that decision are
unbounded:

- the serial effect already executing when the stop wins the scheduler select;
- the stopped-loop receive from the completion channel;
- `drain_processes`, which joins process tasks and non-abortable blocking-I/O
  wrapper tasks;
- later serial cleanup effects, especially `FinishSession`.

Session calls run through Tokio `spawn_blocking`. SQLite connections use a
five-second busy timeout, so a competing write transaction can hold
`FinishSession` beyond a short total timeout. Moving that work off a Tokio
worker fixed runtime responsiveness but did not bound the subsequent await.

The CLI binary calls `process::exit` immediately after `run_from` returns.
Consequently a detached blocking call cannot keep the CLI process alive, but
the shell must first stop awaiting its Tokio wrapper. Library callers must also
receive an error rather than an indefinitely pending run future.

## Chosen design

### One fixed shutdown budget

Add a private two-second `SHUTDOWN_GRACE`. It is intentionally fixed rather
than another CLI limit:

- total timeout: absolute shutdown deadline is the configured run deadline plus
  two seconds, even if the scheduler observes the deadline late;
- `RunControl` cancellation or first interrupt: deadline is two seconds after
  the stop is observed;
- fatal effect/transition/signal failure: deadline is two seconds after the
  failure starts shutdown.

The first shutdown cause and deadline are immutable. Later failures cannot
restart or extend the budget.

### Bound every post-stop await

Represent the active shutdown with a small private value containing its cause
and absolute deadline. Route these waits through that value:

- an in-progress serial effect when deadline/cancellation/first interrupt wins;
- serial effects produced after the state machine accepts the stop;
- stopped completion-channel receives;
- process and blocking-I/O task draining;
- error-path drains after signal, transition, task, or effect failure.

Normal execution before a stop retains its existing scheduler ordering and
does not pay a timeout wrapper on each ready operation.

### Grace expiry

When the shared deadline expires:

1. keep the original stop cause;
2. cancel process work as today;
3. abort Tokio wrapper tasks and stop awaiting their joins or completion sends;
4. accept any already-buffered blocking completion so returned workspace state
   is not discarded unnecessarily;
5. return a stable shutdown error that names the original cause, the two-second
   grace, and the remaining process/blocking-I/O task counts.

`spawn_blocking` operations that have already started cannot be forcibly
cancelled safely. Dropping their Tokio join wrappers detaches them. The CLI's
immediate `process::exit` terminates those threads after the error is rendered;
library callers may observe a detached operation finish later, but the run
future itself is bounded and never claims successful cleanup.

No cleanup operation is reported as complete unless its completion was
actually accepted before the grace expired. In particular, a blocked session
finish leaves the run resumable (`complete = 0`).

## Result semantics

- A total timeout whose orderly shutdown completes within two seconds keeps
  the existing incomplete report and exit code 4.
- Cancellation whose orderly shutdown completes keeps exit code 130.
- If the shutdown grace itself expires, the command returns infrastructure exit
  code 2 and stderr identifies both the initiating cause (for example,
  `total timeout`) and `shutdown grace expired`.
- A blocked terminal effect may prevent `run_finished` from being emitted. JSONL
  already permits a parseable incomplete prefix; stderr is the authoritative
  cleanup-failure diagnostic in that case.
- A second real Ctrl+C keeps unconditional process-level precedence and forces
  exit 130 immediately, without waiting for the grace.

This separates an ordinary timeout from failure to complete its teardown.

## Testing

### Deterministic shell tests

Add paused serial/blocking operations and prove:

- a deadline/cancellation drain remains orderly when released within grace;
- an unreleased operation returns at the shared deadline;
- process and blocking-I/O task counts are included in the expiry error;
- ready completions are accepted before wrappers are aborted;
- a second stop cannot extend the first budget.

### SQLite E2E

Run the real CLI with JSONL output, a session, and a short total timeout. After a
mutant starts, hold `BEGIN IMMEDIATE` from the test process so `FinishSession`
blocks. Assert that:

- the child exits within total timeout + two-second grace + a small CI tolerance;
- exit code is 2, not the ordinary timeout code 4;
- stderr contains both total-timeout and shutdown-grace-expired identities;
- every emitted stdout line is parseable JSONL and no false `run_finished`
  appears;
- the session remains incomplete and descendants are reaped.

Keep existing normal total-timeout, first interrupt, and second interrupt E2E
coverage unchanged.

## Documentation

Update the README timeout contract to state that `--total-timeout` stops new
work and permits at most a fixed two-second orderly shutdown grace. Explain the
exit-code distinction between an ordinary timeout and grace expiry. Add the
same invariant to development documentation.

## Rejected alternatives

- **Add `--shutdown-grace`:** expands config, plans, fingerprints, schemas, and
  user error modes without a current need. An arbitrarily large value would
  also weaken the timeout promise.
- **Reuse SQLite busy timeout:** covers only session locks, is currently five
  seconds, and cannot bound process/workspace drains.
- **Hard-exit exactly at total timeout:** loses ordinary terminal reports,
  resumable session finalization, metrics, and descendant cleanup even when
  they need only a few milliseconds.
- **Only abort `JoinSet` tasks:** does not bound the serial effect await or the
  stopped completion receive and would lose already-ready workspace ownership.

## Non-goals

- Making already-running OS blocking calls cancellable.
- Changing report schemas, state-machine events, candidate outcomes, timeout
  configuration, or session compatibility.
- Bounding arbitrary caller-provided synchronous `Write` implementations; the
  CLI's report paths and second-interrupt escape hatch remain as currently
  specified.
