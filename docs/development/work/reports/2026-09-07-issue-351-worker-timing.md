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

The model uses integer milliseconds and a monotonic clock. Each transition
preserves the bound on a worker's accounted time. While that worker is queued
or running, the bound is the start of the open interval. This lets the proof
bound the sum after accounting for the interval. The model also proves that
rejected operations preserve worker state and cancelling a queued process
preserves its accumulated counters.

The corpus replays all 125 sequences of three operations for one worker at
fixed timestamps. Eight additional cases cover overlapping work on two
workers, rejection followed by successful completion, cancellation and reuse,
uncounted work, outstanding-process error precedence, empty output, a count
above discovery, and an elapsed time of `u64::MAX`. The Rust adapter uses the
collector's test clock and checks each operation's typed result, the final
validation status, and the ordered worker counters. The comparison covers
integer-millisecond inputs. Submillisecond truncation and stage timing remain
outside this model.

The sensitivity check finds differences when a duplicate queue resets its
timestamp, an uncounted process increments the process count, or cancelling
a queued process charges elapsed queue time.

```sh
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --check corpus/worker-metrics.jsonl)
(cd formal/HoiminOracle && lake exe generate_worker_metrics -- --sensitivity)
CARGO_BUILD_JOBS=1 cargo test --locked -p hoimin-cli --lib worker_metrics_accounting_matches_lean
```
