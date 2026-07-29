# Progress Summary Consistency Design

## Decision

`read_report` will validate the embedded `RunFinished.counts` against
`hoimin_core::summarize` applied to every `MutantFinished.status` in document order. The complete
`MutationSummary` values will be compared with `PartialEq`, including every status count,
`inconclusive`, and `score`.

This is preferred over manual field-by-field checks, which can omit fields as the schema evolves,
and over silently replacing the producer's summary, which would hide corrupted input. Canonical
score generation and deserialization both use the same `f64` value represented in JSON; exact
equality is therefore intentional. `None` must equal `None` when there are no killed or survived
mutants.

## Validation boundary

The consistency check belongs in structural validation after event kinds are confirmed and before
baseline or completeness usability decisions. Consequently, even an incomplete or failed-baseline
document cannot conceal a contradictory summary. A mismatch returns
`ProgressError::InvalidStructure` with a stable summary-consistency message, which the existing CLI
error path maps to infrastructure exit code 2.

## Tests

Table-driven input tests will cover changed status/counts, missing and extra mutant records,
incorrect `inconclusive`, incorrect numeric score, incorrect non-null score for no decidable
mutants, and valid controls for ordinary and canonical null-score summaries. The CLI test will
assert exit code 2 and an `invalid structure` diagnostic.
