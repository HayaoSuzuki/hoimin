# Diversity-Aware Ranked Selection Design

## Context

A version-2 plan stores candidates in deterministic global rank order. Today,
`verify --top N` selects the first `N` retained candidates. This is correct for
strict ranking, but equal-score candidates can be concentrated in one
production file and consume a small verification budget without covering other
available defect surfaces.

Issue #42 adds an explicit, opt-in diversity policy. Existing strict selection
must remain the default and retain its exact saved-rank-prefix semantics.

## Goals

- Let users request deterministic file diversity for `verify --top N`.
- Distribute candidates across files within an equal-score tier.
- Never let a lower-score candidate displace an unselected higher-score
  candidate.
- Record the effective selection policy in every verification report format.
- Keep the plan manifest's ranking contract and stored candidates unchanged.

## Non-Goals

- Reweighting files, scores, operators, or ranking reasons.
- Diversifying across symbols, operators, directories, or packages.
- Automatically choosing diversity based on the plan.
- Changing execution limits, timeouts, cancellation, exit codes, or mutation
  execution behavior.
- Adding an alternate precomputed rank to the plan manifest.

## Command-Line Contract

Top-N verification accepts:

```text
hoimin verify PLAN.json --top N --selection-policy strict
hoimin verify PLAN.json --top N --selection-policy diverse
```

For `--top`, an omitted `--selection-policy` resolves to `strict`. The option
is parsed as optional so existing `--candidate` commands remain valid.
Explicitly combining `--selection-policy` with one or more `--candidate`
values is a CLI validation error. Unknown values are rejected by the CLI
parser.

The user-facing value `diverse` maps to the versioned report policy
`file_round_robin_v1`. The user-facing value `strict` maps to `strict`.

## Contract Boundaries

Ranking and verification selection remain separate:

- Plan creation continues to compute and store one deterministic global rank.
- `ranking_rule_version`, candidate `rank`, candidate `score`, ranking reasons,
  and manifest candidate order do not change.
- Manifest validation completes before verification selection.
- A pure selection-policy function consumes the validated manifest candidate
  slice, requested count, and policy, and returns candidate IDs in execution
  order.
- The existing state machine consumes the selected ordered IDs without
  knowing how that order was produced.

This keeps ranking reproducible, makes the selection policy independently
testable, and prevents diversity concerns from leaking into mutation execution.

## Report Contract

`VerificationSelection` gains a `policy` field with these serialized values:

- `explicit_candidates`
- `strict`
- `file_round_robin_v1`

The existing selection `mode` remains `candidate_ids` or `top`. An explicit-ID
verification reports `explicit_candidates`; a top-N verification reports
either `strict` or `file_round_robin_v1`.

JSON and JSONL expose the typed field on the existing run-started verification
selection object. Human output includes the same policy alongside mode,
requested count, selected count, scope, and plan truncation.

Because the plan does not choose a verification policy, no policy field is
added to the plan manifest.

## `file_round_robin_v1` Algorithm

The validated manifest order is the only ordering input. Candidates are already
ordered by descending score and deterministic strict tie-breakers.

1. Process score tiers from highest score to lowest score.
2. Within one score tier, group candidates by production source path.
3. Order file groups by the strict rank of each file's first candidate in that
   tier.
4. Preserve strict rank order within each file group.
5. Select one candidate from each file group in file-group order.
6. Repeat rounds over non-empty file groups until the tier is exhausted or
   `N` candidates have been selected.
7. Advance to the next lower score tier only after exhausting the current tier.
8. Stop after `N` candidates, or after every retained candidate has been
   selected when `N` exceeds the retained count.

Paths are not lexically re-sorted. First occurrence in validated strict order
defines file order, avoiding a second platform-sensitive ordering rule.

For equal-score strict order:

```text
A1, A2, B1, C1, B2
```

the diverse order is:

```text
A1, B1, C1, A2, B2
```

The algorithm emits every selected candidate exactly once. It uses candidate
source path as the grouping key and candidate ID as the output identity.

## Strict Policy

`strict` returns the first `min(N, retained candidate count)` IDs from the
validated manifest. This is intentionally the current implementation's exact
behavior. Both the omitted flag and explicit `--selection-policy strict` use
this path.

## Validation and Errors

- Manifest structure, rank, score, candidate identity, source path, and ranking
  semantics are validated before either policy runs.
- Selection above normalized `max_mutants` returns the existing typed candidate
  error for both policies.
- An oversized requested `N` is allowed when the number of retained candidates
  selected does not exceed `max_mutants`, matching current top-N behavior.
- Policy selection never modifies the manifest or normalized plan limits.
- Selection does not introduce diagnostics or alter complete/incomplete status.

## Data Flow

```text
CLI parse
  -> manifest read and validation
  -> strict or file_round_robin_v1 ID selection
  -> VerificationSelection attached to RunConfig
  -> existing ordered state-machine execution
  -> JSON, JSONL, or human report
```

The selected ID order remains observable through the existing mutation events.
Timeout and cancellation drain candidates in that selected order using the
existing state-machine behavior.

## Testing Strategy

### Pure policy tests

- concentrated equal-score candidates across several files;
- multiple score tiers, proving no lower score crosses a higher tier;
- fewer files than `N`, proving subsequent rounds are deterministic;
- `N` larger than retained candidates;
- repeated selection from identical input produces identical IDs;
- strict default and explicit strict equal the saved rank prefix;
- no duplicate or missing candidate IDs;
- extreme counts do not panic or overflow.

### CLI tests

- omitted policy parses as strict;
- explicit `strict` and `diverse` parse successfully with `--top`;
- policy with `--candidate` is rejected;
- unknown policy is rejected;
- help text explains strict default and equal-score file round-robin behavior.

### Integration and report tests

- a synthetic concentrated manifest executes candidates in diverse order;
- unequal score tiers retain score priority;
- JSON and JSONL report `file_round_robin_v1`;
- human output states the selected policy;
- explicit candidate reports state `explicit_candidates`;
- existing strict top-N tests continue to assert the saved rank prefix;
- the manifest is byte-for-byte unchanged by verification.

### Repository verification

- stable Rust workspace tests;
- Clippy with all targets and features and warnings denied;
- Python contract tests;
- pinned-nightly Rust tests with shuffled test order.

## Compatibility

Strict selection is behaviorally compatible and remains the default. The report
schema gains a required typed policy field. The project has not broadly
published this contract, so no legacy report deserialization fallback is
required for the new field.
