# Worker timing validation (#351)

## Timing contract

Each worker accounts for disjoint queue-wait and busy intervals. The collector
rejects queueing while that worker is queued or running. Starting a process
ends the queued interval and begins the busy interval at the same timestamp;
finishing it ends the busy interval before another process can queue.

The collector truncates each duration to milliseconds before accumulating it.
This can undercount elapsed time but cannot make disjoint intervals exceed the
run duration. Different workers can execute in parallel, so the bound applies
to each worker rather than the sum across workers.

## Change

`RunMetrics::validate` now rejects a worker whose combined `busy_ms` and
`queue_wait_ms` exceeds `elapsed_ms`. Checked addition also rejects totals that
overflow `u64`, including when the run elapsed time is `u64::MAX`.

The existing individual-duration errors retain their precedence. Metrics JSON
fields and schema version remain unchanged. The schema description now states
the per-worker invariant; Rust validation enforces the cross-field arithmetic.

## Verification

The regression test first accepted an invalid worker with 6 ms busy time and
5 ms queue wait in a 10 ms run. It passes after the combined-duration check.

`cargo test -p hoimin-core --test telemetry` passed all 14 tests on macOS arm64.
New cases cover excess combined time, arithmetic overflow, exact equality at
zero and `u64::MAX`, and overlapping intervals across distinct workers.

`cargo clippy --workspace --all-targets --all-features -- -D warnings` and
`cargo fmt --all -- --check` passed.

`cargo test --workspace --all-features --quiet -- --test-threads=1` passed on
macOS arm64, including the collector's existing overlapping-worker accounting
test and the metrics sidecar integration tests.

## Lean model and Rust comparison

The model uses a monotonic clock with nanosecond resolution. It subtracts the
timestamps before truncating each closed interval to milliseconds. Time
conversion and accumulation saturate at `u64::MAX`. Each transition
preserves the bound on the worker's accounted time. While the worker is
queued or running, the bound is the start of the open interval. When elapsed
milliseconds are representable, the invariant implies that the published
per-worker total fits the elapsed counter. The model also proves that
rejected operations preserve worker state and that cancelling a queued
process preserves its accumulated counters.

The corpus replays all 125 sequences of three operations for one worker at
fixed timestamps. Fourteen additional cases cover overlapping work on two
workers, rejection followed by successful completion, cancellation and reuse,
uncounted work, outstanding-process error precedence, empty output, a count
above discovery, submillisecond boundaries, and saturation. Beyond the elapsed
counter's range, one counter can saturate and still be valid, while combined
wait and busy counters above that range cause validation to fail. The Rust
adapter uses the collector's test clock and checks each operation's typed
result, the final validation status, and the ordered worker counters. Stage
timing and process-count saturation remain outside this model.

The sensitivity check finds differences when a duplicate queue resets its
timestamp, an uncounted process increments the process count, a cancelled
queue interval is charged, timestamps are rounded before subtraction,
fractional intervals are rounded only after accumulation, or accumulated time
is allowed to exceed the counter's range.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --check corpus/worker-metrics.jsonl)
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib worker_metrics_accounting_matches_lean
```

## CLI stage timing

The stage model takes the previous and resulting phases of an accepted
transition together with its completion signal. The Lean proofs keep one
timer across copy and materialization verification, and one across baseline
execution and restoration. Cleanup completion closes its timer once. After
a timer is closed, subsequent timer operations preserve its saved duration.

The corpus contains 13 internal fixtures and 62 prefix observations. It
covers session startup between preflight and copy, the budget check between
baseline and analysis, early cleanup, submillisecond timing, and warnings
from inconsistent timer state. The model omits open timers from the
published stages. Its warning cases check that the first failed operation
ends the transition's timer updates while preserving earlier successful
updates. The Rust adapter calls the shell's stage observer and the real
collector with a controlled clock. It compares the ordered completed stages
and the accumulated warnings after each prefix.

The sensitivity check detects closing the copy timer at verification entry,
closing the baseline timer at restoration entry, deferring preflight close
while the phase stays the same, and closing cleanup twice. The fixtures
exercise the observer's inputs directly. Machine acceptance of the input
events and filesystem publication are outside this comparison.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --check-stages corpus/stage-metrics.jsonl)
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib stage_metrics_follow_accepted_cli_transitions
```

### Public CLI correspondence

Five fixtures run seven CLI invocations with real Python processes,
workspaces, and metrics files. The Lean corpus projects the names of completed
stages from the same timer model. It also predicts the exit code, report
completion status, counts, and session resume decision. The cases cover
normal execution, an empty candidate set, baseline failure, and resume
requests against a complete and an incomplete SQLite session.

The two-mutant budget leaves a three-candidate run incomplete. On resume,
the run publishes the mutants stage, reuses both stored outcomes, reports
the third candidate as not run, and retains the session run ID with zero
executions. A resume request against the completed two-candidate session
starts a fresh run and executes both mutants again. The no-candidate run
publishes analysis, while a failed baseline ends before analysis. The
adapter reads the sidecar after CLI return, validates its invariants and run
ID, and compares its stage names in output order. Actual wall-clock durations
are checked by the sidecar validator. The controlled-clock fixtures above
provide the exact duration comparison.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --check-stages-cli corpus/stage-metrics-cli.jsonl)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --test run_e2e stage_metrics_public_cli_matches_lean
```

## Validation of saved metrics

The validator model checks headers, stage rows in input order, worker rows
in input order, worker ordering, and process counts. The first violation
determines the diagnostic. In particular, a duplicate in a row takes
precedence over a duration violation in that row, while an invalid duration
in an earlier row takes precedence over a later duplicate.

Lean proves that accepted metrics bound each worker's combined wait and busy
time, and that the worker process total equals the executed count. The
executed count cannot exceed discoveries. With a representable elapsed
counter, checked-add overflow for a worker's times gives the same rejection
as a mathematical sum above elapsed.

The corpus contains 420 combinations of schema version, run ID, stage rows,
worker rows, and counts, plus 10 boundary cases. It compares exact messages
and the payloads of typed errors with the public Rust validator. The corpus
supplies typed metrics input through JSON. Malformed JSON and JSON Schema
validation are outside the comparison. The sensitivity check detects
checking workers before stages, checking worker ordering before row
validation, wrapping process counts, and clamping combined worker time.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --check-validation corpus/metrics-validation.jsonl)
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-core --test telemetry
```
