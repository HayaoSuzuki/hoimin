# Issue #333: Preserve mutant results when output pipes stay open

## Problem

After the supervised root process has exited or has been terminated, hoimin
waits one second for its stdout and stderr drain tasks to observe EOF. An
escaped descendant can keep either write end open, especially with the portable
resource backend. The process termination is already known at that point, but
`combine_process_and_output` replaces it with
`process.output.close.timeout`. The shell then reports an `EffectFailed`, the
machine cancels every remaining mutant, and the run becomes an infrastructure
failure dominated by `NotRun` results.

## Goals

- Preserve an already-classified mutant termination when only pipe closure
  times out.
- Produce one durable `MutationStatus::Error` result for that mutant and
  continue scheduling remaining mutants.
- Record a stable diagnostic explaining that captured output may be incomplete.
- Preserve fatal behavior for process failures, ordinary output I/O failures,
  and baseline output failures.
- Keep output retention bounds and the existing primary-error precedence.

## Non-goals

- Finding or killing descendants that have escaped the selected resource
  backend.
- Extending the one-second post-termination grace period.
- Treating an incomplete output capture as a killed, survived, or timed-out
  mutant.
- Changing the final exit policy for a run containing an error result.
- Recovering an exact retained/observed byte count after collector abortion.

## Options considered

### Add an explicit output state to `ProcessFinished`

Keep `ProcessTermination` unchanged and add a defaulted `ProcessOutputState`
with `Complete` and `CloseTimedOut`. When a mutant has a known termination and
only output closure times out, the process handler returns the initial valid
spool reference plus `CloseTimedOut`. The machine converts that combination to
`MutationStatus::Error` and a session diagnostic.

This is the recommended design. It preserves both facts instead of overloading
termination, makes the live producer of `MutationStatus::Error` explicit, and
keeps the durable result schema unchanged because it already stores status,
termination, output, and diagnostics independently.

### Add an output-error termination variant

A `ProcessTermination::OutputError` variant would make classification easy,
but discard the known exit/timeout result and broaden report and session schema
semantics. It is rejected because output capture and process termination are
independent observations.

### Keep the classified termination and emit only a warning

Returning `Killed`, `Survived`, or `Timeout` with incomplete output would let
the run continue, but claim a normal test outcome even though the capture
contract failed. It is rejected because `MutationStatus::Error` already denotes
an executed mutant whose result is not usable.

## Detailed design

`ProcessOutputState` is a serializable core event value. `Complete` is its
default so older serialized `ProcessFinished` fixtures remain compatible. A
`ProcessFinished` carries that state alongside the existing termination and
spool reference.

The process handler keeps the empty, bounded `OutputSpoolRef` created before
drain tasks start. Combining process and output outcomes follows this order:

1. A process failure always wins, preserving current cleanup-error precedence.
2. Complete output and a known termination produce a normal completion.
3. `process.output.close.timeout`, a known termination, and a non-null mutant
   identity produce a completion with the fallback spool reference and
   `CloseTimedOut`.
4. The same timeout for a baseline, or any other output failure, remains an
   `EffectFailed`.

The fallback reference has `retained = observed = 0`, so it satisfies the
existing output postcondition without claiming that aborted collector state is
complete. The spool token still identifies the partially written file, but
consumers must rely on the diagnostic rather than byte counters for its
completeness.

On `MutantFinished`, the machine maps `CloseTimedOut` to
`MutationStatus::Error`, keeps the known termination, and adds an error
diagnostic with code `process.output.close.timeout`. Normal completions continue
to use `classify_mutant`. Result persistence, reporting, summary accounting,
and exit policy already support error results, so the scheduler proceeds to
reset the worker and run later candidates.

The worker identifier cannot distinguish a baseline from a mutant: both use
worker zero in normal runs. The process boundary therefore uses the presence of
`RunProcess.mutant_id` as the execution identity. The public
`MutantFinished` event carries the output state and its diagnostics. Both fields
have serde defaults, and empty/complete values are omitted, so existing report
documents remain compatible. Report-sequence validation classifies a known
termination together with its output state, accepting only the new
`Error`/`CloseTimedOut` combination. It also requires exactly one matching
`process.output.close.timeout` error diagnostic for that mutant and rejects the
same diagnostic on complete output. A `CloseTimedOut` event is valid only with
a known termination, `MutationStatus::Error`, and a fallback output reference.
Human reports render nested mutant diagnostics to stderr; JSON and JSONL retain
them in the mutant event.

## Formal contract

A Lean oracle closes the decision table over:

- baseline versus mutant execution;
- every known process termination versus a failed process outcome;
- complete output, close timeout, and other output failure.

It asserts that only the known-mutant/close-timeout combination yields a
non-fatal error result, while all primary and unrelated output failures remain
fatal. Broken-family checks ensure the corpus detects accidentally degrading
baseline failures, swallowing process failures, classifying incomplete output
from the process termination, or stopping subsequent mutants.

## Test design

- Generate and check the closed Lean decision corpus.
- Add a Rust adapter at the real CLI combiner that checks all 42 strict rows,
  including baseline, process-failure, unrelated-output-failure, fallback, and
  status cases.
- Change the process combiner unit tests first so known mutant timeout degrades
  while baseline and other failures stay fatal.
- Add a machine regression with two candidates: the first has
  `CloseTimedOut`, persists/reports as `Error` with a diagnostic, and the second
  is still scheduled.
- Assert that the public no-session event retains `CloseTimedOut` and its stable
  diagnostic, that human output exposes it on stderr, and that report-sequence
  validation accepts only a matching diagnostic.
- Deserialize a legacy `MutantFinished` without the new fields and verify that
  complete/empty defaults remain omitted when it is serialized again.
- Run focused mutation testing over the combiner and machine classification,
  then the full Rust, Python, lint, and wheel suites.
