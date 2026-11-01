# Report Stable Mutant Identity Design

## Context

`ReportSequence` tracks only active `(mutant ID, candidate sequence)` pairs.
Finishing a mutant removes that pair, so the same stable ID can later start with
a different candidate sequence and appear twice in an otherwise valid report.

## Decision

Retain a run-lifetime `stable ID -> candidate sequence` map in addition to the
active lifecycle set.

- The first `MutantStarted` event reserves the stable identity and sequence.
- Starting an already-seen ID with the same sequence is a duplicate stable
  identity.
- Starting or finishing an already-seen ID with another sequence is an identity
  sequence mismatch.
- Finishing removes only the active lifecycle pair; it never removes the stable
  identity mapping.
- Distinct IDs may remain active concurrently and finish in any order.

The two new cases are represented by separate `ReportSequenceError` variants so
callers can distinguish duplicate logical mutants from inconsistent identity
metadata.

## Reader Boundary

The persisted JSON report contains only `MutantFinished` events, not
`MutantStarted` events. The progress reader therefore cannot replay the complete
`ReportSequence`. It will validate the same stable identity rule directly while
checking document structure:

- repeated ID with the same candidate sequence is invalid duplicate identity;
- repeated ID with a different candidate sequence is an identity/sequence
  mismatch.

Both become `ProgressError::InvalidStructure`, which the progress command already
maps to its documented infrastructure-error path.

## Tests

Core report-policy tests cover:

- sequential reuse after completion;
- concurrent reuse while the original identity is active;
- same-sequence duplicate identity;
- finish-time ID/sequence mismatch; and
- distinct IDs executing concurrently and finishing successfully.

Progress integration tests construct summary-consistent duplicate documents so
summary validation cannot mask the stable-identity failure.

## Documentation Scope

This is a versioned core report-integrity invariant rather than a user workflow
or CLI option. Public Rust error messages and tests document it; the README does
not need an update.
