# Progress Candidate-Set Eligibility Design

## Context

`hoimin progress` compares adjacent complete reports and may publish
`latest.state = saturated`. The public contract requires every report in a
history to cover the identical candidate-ID set, but the implementation
currently derives a state from the common semantic subset even when candidates
were added, removed, or rotated. That lets an ineligible history contribute
stalls and eventually produce a terminal agent decision.

This design resolves GitHub Issue #25 and audit finding
`RUST-AUDIT-007`. It does not change the separate stall-retention policy tracked
by Issue #29.

## Decision

Each adjacent pair of usable reports must pass an eligibility gate before
status transitions are compared:

1. Build the candidate-ID set for each report.
2. Treat duplicate candidate IDs within either report as ambiguous and
   ineligible.
3. Require the two unique candidate-ID sets to be exactly equal.
4. Only after eligibility succeeds, compare mutants using the existing
   five-field semantic key: path, original text, replacement text, operator,
   and symbol.

Candidate IDs establish that both reports cover the same planned work. They do
not replace the semantic key, which intentionally preserves comparison across
source-position or candidate-hash changes.

## Comparison and History Behavior

An ineligible adjacent pair still produces a `Comparison` entry so callers can
inspect the pair, but its state is always `Indeterminate`. It must not increment
the stall counter. It also breaks the comparable stall chain by resetting
`consecutive_stalls` to zero, preventing stalls on opposite sides of a
candidate-set mismatch from producing `Saturated`.

The comparison continues to expose the existing semantic `common`, `added`, and
`removed` counts. A comparison-level eligibility flag is retained internally
so stderr rendering can explain why an otherwise usable pair is
`Indeterminate`. The public JSON schema and human stdout fields do not change;
`latest.state` remains the machine decision field.

Duplicate candidate IDs make `candidate_set_match` false even if the deduplicated
sets appear equal. This keeps the eligibility claim unambiguous rather than
silently collapsing multiple records into one set member.

## Diagnostics

Every ineligible comparison emits one stderr warning identifying its
one-based comparison index. The warning distinguishes:

- different candidate-ID sets; and
- duplicate candidate IDs in either input.

The command remains successful with exit code zero. Ineligibility is a valid,
machine-readable progress result rather than malformed input; agents continue
to use `latest.state` to make decisions.

## Testing

Tests follow red-green order:

- a pair with one shared semantic mutant plus added/removed IDs is
  `Indeterminate` and resets stalls;
- a rotating-ID history cannot reach `Saturated`;
- duplicate candidate IDs are ineligible;
- changing IDs while retaining a common semantic mutant is not allowed—the
  exact ID gate wins;
- an identical-ID-set control preserves the existing semantic status
  comparison;
- JSON and human stdout remain schema-compatible, while stderr explains the
  mismatch.

The focused integration suite is
`cargo test -p hoimin-cli --test progress`. Final verification also runs
formatting, Clippy, and the full workspace test suite. To check test quality
without paying for a workspace-wide mutation run, `cargo-mutants` targets only
the new candidate-set eligibility classifier, state classifier, and stall-chain
gate. Every surviving or timed-out mutant in that focused scope must be
resolved or justified before the branch is complete.

## Non-goals

- Do not change whether same-set regression or status-induced indeterminate
  comparisons reset retained stalls; Issue #29 owns that policy.
- Do not replace semantic transition matching with candidate IDs.
- Do not reject the entire command or change its exit code for a set mismatch.
- Do not combine or summarize changing batches into a synthetic whole-plan
  result.
