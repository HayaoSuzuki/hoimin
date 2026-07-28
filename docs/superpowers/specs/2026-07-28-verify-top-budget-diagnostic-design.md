# Verify Top Budget Diagnostic Design

## Goal

Warn before mutation execution when the immutable limits in a saved plan are
unlikely to cover a `verify --top N` selection. The warning is advisory: it
must not alter the manifest, stop the run, or change exit-code and incomplete
result semantics.

## Scope

The diagnostic applies only to ranked selections created by `verify --top N`.
Explicit `--candidate` selections and ordinary `run` invocations retain their
current behavior and do not request a budget observation.

## Architecture

The existing boundary remains explicit:

- `hoimin-cli` owns the absolute runtime deadline and observes monotonic time.
- `hoimin-core` owns deterministic policy, state transitions, and output
  ordering.

After a successful baseline for a top-N verification, the state machine emits
an `ObserveRemainingBudget` effect. The shell calculates the duration remaining
until its existing absolute deadline and returns a typed
`RemainingBudgetObserved` event. The state machine evaluates the observation,
optionally emits a typed warning diagnostic, and then continues with analysis
and mutation execution.

The observation is a dedicated effect/event pair rather than an extra field on
the process-completion event. Process results therefore remain about the
process itself, while wall-clock budget observation remains an explicit runtime
capability.

## Projection policy

The policy uses the configured timeout capacity, not an assumed average mutant
duration:

```text
effective_mutant_timeout =
  fixed mutant timeout
  or max(5 seconds, baseline_duration * 2 + 1 second)

waves = ceil(selected_count / jobs)
projected_capacity = waves * effective_mutant_timeout
shortfall = projected_capacity > remaining_total_timeout
```

All duration arithmetic saturates instead of overflowing. Equality is treated
as sufficient for this advisory check; only a strict excess emits a warning.

This is deliberately conservative. Mutants may finish earlier than their
timeout, so the projection is not a prediction of actual elapsed time and must
not be described as a guaranteed failure.

## Diagnostic contract

The warning code is `budget.projected_shortfall`. Its message includes:

- selected candidate count;
- configured jobs;
- planned total timeout;
- fresh baseline duration;
- effective mutant timeout;
- remaining total timeout at observation;
- projected timeout capacity;
- an explicit statement that the estimate is not a guaranteed failure;
- an instruction to create a new plan with different `--jobs` and/or
  `--total-timeout` limits because verify does not modify manifest settings.

The warning uses the existing typed `Diagnostic` output path:

- human output writes it to stderr;
- JSON retains one valid document on stdout and writes the diagnostic to
  stderr;
- JSONL retains a machine-readable event stream on stdout and writes the
  diagnostic to stderr.

No diagnostic is emitted when the projection fits in the remaining budget.
Diagnostic output failures follow the existing report-failure policy.

## State and execution semantics

The observation occurs after the fresh baseline succeeds and before analysis
starts. A warning does not:

- modify jobs, timeouts, or any other normalized plan setting;
- reduce the selected candidate set;
- delay or suppress normal analysis after the diagnostic is emitted;
- change exit-code precedence;
- mark the run incomplete.

Baseline failure follows the existing finalization path without observing the
remaining budget.

## Testing

Pure policy tests cover:

- a 30-candidate serial projection that exceeds the remaining budget;
- parallel wave ceiling division;
- automatic and fixed mutant timeouts;
- equality without a warning;
- saturating duration arithmetic.

State-machine tests cover:

- top-N baseline success requesting a budget observation;
- explicit candidate and ordinary run paths skipping the observation;
- shortfall diagnostic ordering before analysis;
- a sufficient observation proceeding without a diagnostic;
- unchanged failure, exit-code, and incomplete semantics.

Shell and integration tests cover:

- translating the absolute deadline into a typed remaining-duration event;
- valid JSON/JSONL stdout with the warning confined to stderr;
- all required diagnostic fields and the concrete replan instruction;
- immutable manifest limits.

Completion verification runs formatting, Clippy, the stable workspace tests,
all Python contract tests, and the pinned-nightly randomized Rust suite.
