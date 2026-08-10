# Lean candidate ranking and top-selection audit report

## Result

No Hoimin implementation mismatch was found in the audited candidate ranking
and top-selection contract. The Lean model, fixed refutation witnesses, finite
manifest audit, generated corpus, public `plan`/`verify` correspondence case,
and existing Rust ranking/selection tests agree.

This result is evidence for the scoped rules below, not a proof of the Rust
implementation. Lean proves properties of the independent model; the Rust
adapter establishes correspondence only for the stable projection of the
strict public case.

## Scope and implementation correspondence

The audit owns ranking rule version 3 reason construction and scoring,
deterministic stable ordering, strict retained-prefix selection, equal-score
tier/path round-robin selection, no lower-tier crossing, and requested limits
above the retained candidate count.

| Case | Mode | Evidence | Result |
| --- | --- | --- | --- |
| `selector_reason_scores` | `model-only` | all three selector bonuses plus one operator reason total 850 and outrank an arithmetic-only candidate | pass |
| `stable_tie_break` | `model-only` | six unique roles distinguish path, line, column, operator, and ID keys | pass |
| `diverse_equal_tier` | `strict` | real public plan over two Python files, then public strict and diverse `verify --top 3` | match |
| `truncated_limit_saturates` | `model-only` | request 10 against three retained high-tier candidates | pass |

The strict adapter generated four real candidates with semantic symbols
`alpha_1`, `alpha_2`, `beta_1`, and `low`. It observed version-3 reason codes,
scores, and one-based ranks from the plan manifest. Strict selected
`alpha_1, alpha_2, beta_1`; diverse selected
`alpha_1, beta_1, alpha_2`; neither crossed to the lower arithmetic tier.
The saved manifest remained byte-identical after both verifications.

The stable-key case remains model-only because two discovered candidates
cannot legitimately differ only by generated ID while sharing every source
field. The truncation case remains model-only because analyzer discovery
cutoff order is a different premise from ranking a supplied retained
manifest. Existing public plan integration tests independently cover retained
truncation saturation.

## Lean evidence

The imported library contains only pure semantics and inexpensive
kernel-checked proofs. With a local `maxHeartbeats 100000` limit, Lean proves:

- `rankOne_score_is_reason_sum`: stored score is the sum of constructed reasons;
- `rankOne_reasons_are_constructed`: stored reasons are exactly the constructed list;
- `strictSelect_is_saved_prefix`: strict selection is the saved prefix projection;
- `strictSelect_length`: strict selection length is `min limit ranked.length`;
- `strictSelect_member_of_saved`: every strict-selected ID comes from a saved candidate;
- `validation_recomputes_complete_ranking`: validation compares saved data with a
  complete deterministic reranking.

Sorting correctness and diverse-selection uniqueness/tier behavior are finite
checks rather than unbounded theorems. The executable enumerates all 16
subsets of a four-candidate universe, every limit from zero through two above
the manifest length, and checks complete reranking validation, candidate-ID
uniqueness, strict/diverse uniqueness, exact saturated lengths, and
non-increasing diverse score order. Fixed cases separately exercise the full
stable comparison key and expected round-robin order.

No trace search, unbounded `native_decide`, or imported exhaustive evaluator is
used. The candidate universe is four, the maximum manifest size is four, and
the fixed corpus contains four JSONL records (4,591 bytes at generation time).

## Refutation sensitivity

All applicable broken families were distinguished before the correct audit
was accepted:

| Family | Broken behavior | Minimal witness | Detected |
| --- | --- | --- | --- |
| atomicity/validation | trust saved ranking data without deterministic recomputation | one arithmetic candidate with stored score incremented by one | yes |
| uniqueness/idempotency | duplicate the first diverse selection result | three selected high-tier candidates | yes |
| boundary/precedence | move the lower arithmetic tier into the second position | three high-tier candidates plus one lower candidate | yes |

The sensitivity command reports
`validation_detected=true`, `uniqueness_detected=true`, and
`tier_boundary_detected=true`. A missing witness fails the executable before
corpus generation or freshness checking.

## Resource observations

Every Lean command ran as the only potentially expensive Lean process under a
20-second external alarm. Clean incremental library builds completed within
the limit; the audit stats and sensitivity commands completed in a few
seconds or less. No depth escalation, unlimited heartbeat setting, swapping,
or abnormal memory growth was observed. The audit intentionally stopped at
the four-candidate complete subset domain rather than increasing an unrelated
search depth.

## Verification record

Focused pre-report verification produced:

| Command | Result |
| --- | --- |
| `lake build` under 20-second alarm | pass, 28 jobs |
| `lake exe generate_candidate_ranking -- --check corpus/candidate-ranking.jsonl` under alarm | pass/fresh |
| `lake exe generate_candidate_ranking -- --stats` under alarm | universe 4, manifests 16, cases 4, all sensitivity flags true |
| `lake exe generate_candidate_ranking -- --sensitivity` under alarm | all three broken families detected |
| `cargo test -p hoimin-cli plan:: --lib` | 16 passed, 1 ignored benchmark |
| `cargo test -p hoimin-cli --test plan` | 34 passed |
| `cargo test -p hoimin-cli --test lean_candidate_ranking_oracle` | 2 passed |
| `cargo clippy -p hoimin-cli --test lean_candidate_ranking_oracle -- -D warnings` | pass |
| `cargo fmt --all -- --check` and `git diff --check` | pass |
| `cargo test --workspace --all-features` | pass; all targets completed with zero failures, including 52/52 `run_e2e` |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |

CI results are recorded separately in the PR and are not inferred from these
local commands.

## Limitations and next audit

- The model uses two abstract paths and ASCII comparison keys; it assumes the
  production path normalization and byte/string ordering premises already
  validated by their Rust tests.
- The five operator classes are modeled by their fixed scores. Exhaustive
  membership of all 43 concrete operators remains covered by the existing
  Rust unit test rather than duplicated in Lean.
- Public correspondence covers one deliberately dense four-candidate case;
  the other cases are independently model-checked for documented premise
  reasons.
- Candidate discovery order before a retained-plan cutoff, analyzer fact
  correctness, and execution result accounting are outside this contract.

The next independent formal audit should target schema migration concurrency,
followed by AST fact-flow joins. Those surfaces do not share ranking premises
and should use separate worktrees and correspondence ledgers.
