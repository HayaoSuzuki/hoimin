# Progress Stall Adjacency Design

## Context

`hoimin progress` documents saturation as the result of consecutive comparable
stalls, and its public JSON field is named `consecutive_stalls`. The current
implementation resets that counter after improvement or candidate-set
ineligibility, but retains it across a same-candidate-set regression and an
indeterminate comparison caused by inconclusive mutation statuses.

As a result, a history such as `Stalled -> Regressing -> Stalled` can become
`Saturated` at patience two even though its stalled comparisons are not
adjacent. The same problem occurs when a status-induced `Indeterminate`
comparison separates two stalls.

## Decision

Treat `consecutive_stalls` as a strict suffix count of adjacent `Stalled`
comparisons. Every comparison state other than `Stalled` breaks the chain.
Only `Stalled` increments the counter, and saturation is published only when
that increment reaches `patience`.

This preserves the existing public names and aligns them with their documented
meaning:

- `Improving` resets the counter to zero.
- `Regressing` resets the counter to zero.
- `Indeterminate` resets the counter to zero, regardless of whether it was
  caused by candidate-set eligibility, an empty comparable common set, or an
  inconclusive mutation status.
- `Stalled` increments the counter.
- `Saturated` remains a derived latest decision, not an individual comparison
  state.

The latest state still describes the most recent comparison. A regression
remains `Regressing`, and an unknown comparison remains `Indeterminate`; the
change affects only whether older stalled evidence can contribute to a later
saturation decision.

## Public Contract

No output field or schema is renamed. The following existing fields retain
their shapes and gain one unambiguous interpretation:

- `consecutive_stalls` is the number of immediately adjacent stalled
  comparisons ending at the latest comparison.
- Human output `stalls` reports that same count.
- `latest.saturated` is true only when the latest comparison is stalled and the
  suffix count is at least `patience`.

The README will explicitly state that improvement, regression, and
indeterminate comparisons break the chain. Existing candidate-ID-set guidance
continues to apply.

## Implementation Boundary

The state machine in `compare_reports` will centralize counter maintenance
around the computed comparison state:

1. Compute the comparison exactly as today.
2. Increment the counter only for `Stalled`.
3. Reset it for `Improving`, `Regressing`, and `Indeterminate`.
4. Derive `Saturated` only from the incremented stalled suffix.

The comparison classifier, mutation status classification, scores, counts,
input usability, output schema, and candidate-set eligibility rules do not
change.

## Verification

Characterization tests will first demonstrate the current policy mismatch at
patience two:

- `stall -> same-set regression -> stall`
- `stall -> status-induced indeterminate -> stall`

Both final comparisons must be `Stalled`, with `consecutive_stalls == 1`.
Existing tests will be updated where they intentionally encoded retained stall
evidence. Ordinary adjacent stalls must still saturate, and improvement must
still reset the chain.

Focused mutation testing will target the progress comparison state machine
only. It is intended to verify that the new reset branches and saturation
threshold are behaviorally covered without paying the cost of mutating the
whole Rust workspace.

Required local verification:

```console
cargo test -p hoimin-cli --test progress
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
cargo build --workspace
git diff --check
```

## Non-goals

- Changing candidate-ID-set mismatch or empty-common eligibility.
- Changing mutation result classification.
- Renaming or redesigning the progress report schema.
- Combining histories from different candidate subsets.
- Refactoring unrelated progress rendering or input parsing.
