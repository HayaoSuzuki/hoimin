# Lean candidate ranking and top-selection audit design

## Objective

Audit Hoimin's claim that candidate ranking and `verify --top` selection are
deterministic, score-preserving, and boundary-safe. The audit covers ranking
reason construction, fixed scores, stable tie breaking, strict-prefix
selection, diverse round-robin selection inside equal-score tiers, and
retained-plan truncation. It adds formal evidence and correspondence tests;
production behavior changes only if the audit first produces a reproducible
implementation counterexample.

## Owned contract

For a retained candidate manifest:

- ranking reasons are ordered, unique, and contain exactly one operator reason;
- every reason has its version-3 fixed score and the candidate score is their sum;
- ranks are one-based and candidates are ordered by descending score, then
  path, line, column, operator, and ID;
- strict top selection returns exactly the retained rank prefix up to the
  requested limit;
- diverse selection never selects a lower score while an unselected higher
  score remains, and round-robins path groups in first-seen order within a tier;
- either selection returns no duplicates and at most the retained candidate
  count, including requests larger than a truncated plan;
- validating saved ranking data recomputes the complete deterministic ranking
  rather than trusting stored ranks, scores, or reasons.

Candidate discovery, mutation semantics, Git changed-line discovery, resource
accounting, execution results, and plan storage durability are outside this
audit. Path normalization itself is reused as an implementation premise; this
audit observes its effect on explicit-line reasons but does not re-prove it.

## Approach

Add a small finite `CandidateRanking` model to the pinned Lean project. It uses
semantic candidate IDs, two paths, stable scalar keys, five operator score
classes, and the three selector bonuses. The correct functions construct and
sort ranked candidates, validate saved data, and implement strict and diverse
selection. General list theorems cover score construction, strict-prefix
behavior, selected-length bounds, and membership conservation. Small finite
cases cover the complete stable-order key, tier precedence, round-robin order,
and truncation boundaries.

The executable performs only a finite manifest enumeration up to four
candidates. It starts at depth/size three and may extend to four only after the
smaller run succeeds within the resource envelope. There is no unbounded trace
search. Expensive evaluation is excluded from imported library modules.

## Refutation and sensitivity

The audit must reject fixed broken variants for three risk families:

- **atomicity/validation:** accept a saved score or reason list without full
  deterministic recomputation;
- **uniqueness/idempotency:** emit the same candidate twice during diverse
  selection;
- **boundary/precedence:** cross into a lower score tier early or use an
  off-by-one strict limit.

Each family has a deterministic minimal witness. Failure to distinguish a
broken variant makes the audit fail even when the correct model passes.

## Lean/Rust correspondence

Lean owns a versioned JSONL corpus at
`formal/HoiminOracle/corpus/candidate-ranking.jsonl`. Strict cases describe
semantic source roles, selector configuration, expected reasons, scores,
ranks, and strict/diverse selected role order. The Rust adapter invokes the
real public `hoimin plan` and `hoimin verify` commands against isolated Python
fixtures, maps candidates to roles using stable source fields, and compares
only the Lean-owned projection. It never recomputes expected ordering.

Cases not constructible through the public analyzer remain explicitly
`model-only`. Harness failures, timeouts, malformed output, and unavailable
controlled interpreters are `infrastructure-error`, never semantic matches.
Every public process has a short timeout and process-tree cleanup.

## Resource envelope

- run only one potentially expensive Lean process at a time;
- use local `maxHeartbeats` values no higher than 100,000 and never set zero;
- place a 20-second external deadline around each Lean build or audit run;
- keep exhaustive data out of `HoiminOracle.lean` imports;
- record elapsed time and finite sizes, and stop or reduce the model if memory
  pressure, swapping, or unexpected growth appears;
- do not increase depth merely to overcome a timeout.

## Files

The audit adds `CandidateRankingModel.lean`, `CandidateRankingProofs.lean`,
`CandidateRankingCases.lean`, `CandidateRankingAuditMain.lean`, the generated
corpus, a Rust integration adapter, and a self-contained report. It updates the
Lean library imports and lake executable declarations. The worktree-local
`.venv` link is setup only and is never committed.

## Completion criteria

The pinned Lean build, theorem checks, sensitivity checks, corpus freshness,
public correspondence adapter, focused existing ranking/selection tests, Rust
formatting and Clippy, and the workspace test suite must pass. A confirmed
mismatch is documented with the shortest witness and repaired test-first in
this branch before the final audit is claimed complete.
