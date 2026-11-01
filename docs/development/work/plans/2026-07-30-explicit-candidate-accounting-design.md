# Explicit Candidate Accounting Design

## Scope

Fix #82 so explicit verification cannot finish successfully unless every
requested candidate ID appears in the analyzer spool and is accounted for.

## Root cause

`RunState::with_candidate_filter` stores the requested IDs only as a membership
filter. During replay, non-selected candidates are skipped and selected
candidates execute, but EOF immediately marks candidate discovery exhausted.
The state does not retain which requested IDs were observed. Ordered
verification has a separate collection phase that rejects missing IDs, so the
two selected-verification paths currently provide different completeness
guarantees.

## Design

Keep the requested `candidate_filter` unchanged and add a private
`matched_candidate_ids` set used only by explicit, unordered selection.

When replay yields a candidate:

1. It is selected only if its ID is requested.
2. Its ID is inserted into `matched_candidate_ids` before execution.
3. A duplicate occurrence is skipped because it is already matched.

When replay reaches EOF, explicit selection computes the first requested ID not
present in `matched_candidate_ids`. If one exists, transition returns the
existing typed `MachineError::SelectedCandidateMissing`. No final-report effect
is emitted and the caller treats the state-machine failure as infrastructure
failure. If none is missing, existing finalization proceeds unchanged.

Ordered selection continues using its ordered collection and existing presence
validation. Runs without a filter continue executing every replayed candidate.

## Alternatives considered

- Removing IDs directly from `candidate_filter` would avoid a second set, but
  would overload the field as both immutable selection policy and mutable
  progress, making ordered-selection membership fragile.
- Converting missing IDs to synthetic `NotRun` results would preserve a final
  report, but would require candidate descriptors that are unavailable when an
  ID is absent from the spool.
- Sharing ordered collection for explicit selection would unnecessarily buffer
  every selected candidate and remove explicit selection's streaming behavior.

## Error and reporting semantics

The existing `machine.candidate.missing` error code is reused. A missing
selected candidate is an analyzer/spool consistency failure, not a mutation
result; therefore the run must not emit a complete final report or claim an
executed/not-run candidate count for a descriptor it never received.

## Testing

- A machine regression requests `{m2}`, replays only `m1`, then EOF and expects
  `SelectedCandidateMissing(m2)`.
- A partial multi-ID regression requests `{m1, m2}`, executes `m1`, then reaches
  EOF and expects `SelectedCandidateMissing(m2)` without a complete report.
- A CLI integration test calls the selected-run boundary with a real analyzer
  and a nonexistent requested ID, asserting an error/non-success path containing
  the typed missing-candidate diagnostic.
- Existing explicit-present, ordered, unfiltered, cancellation, and report
  tests remain unchanged.

## Documentation impact

This restores the existing user-facing promise that explicit verification
executes exactly the requested candidates. It adds no option or workflow, so
README changes are not required.
