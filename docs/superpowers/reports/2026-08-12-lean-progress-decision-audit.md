# Lean progress decision audit

## Result

The `hoimin progress` decision core matched every same-premise public case in
the Lean-generated strict corpus. The audit found no confirmed Rust mismatch,
so it made no production Rust change.

The durable audited claim is:

> Progress compares only adjacent usable reports. A comparable pair is
> regressing when any conclusive mutant regresses, improving when at least one
> improves and none regress, stalled when conclusive common mutants exist and
> none change, and indeterminate otherwise. Saturation occurs exactly when the
> latest comparison ends a stalled suffix whose length reaches patience.

This result has three deliberately separate strengths:

- kernel-checked theorems establish general properties of the Lean model;
- bounded execution checked the declared reduced semantic domain;
- 23 `strict` generated cases exercised the public Rust CLI and matched their
  complete stable observations.

Lean did not prove the Rust implementation. The public adapter supplied the
implementation-correspondence evidence.

## Boundary

Included:

- usable and unusable adjacent reports;
- candidate-ID eligibility and join selection;
- duplicate content-key ambiguity;
- killed, survived, and five concrete inconclusive statuses;
- common, added, removed, ambiguous, inconclusive, improvement, regression,
  and carried-survivor counts;
- exact small scores and score deltas;
- pair-state precedence, stalled-suffix resets, and patience saturation.

Excluded:

- JSON parsing mechanics except as adapter infrastructure;
- mutation execution, filesystem timing, and human prose rendering;
- floating-point correspondence outside `null`, `0`, `0.5`, and `1`, with
  score deltas restricted to `-1`, `-0.5`, `0`, `0.5`, and `1`;
- cross-platform path normalization and case folding;
- public correspondence for duplicate candidate IDs, which the owned parser
  rejects before comparison.

## Declared and implicit behavior

The state precedence and patience behavior are explicit in
`crates/hoimin-cli/src/progress/compare.rs`. The audit also retained behavior
that is easy to miss when reading only the state labels:

- matching unique ID sets select candidate-ID keys even when content keys
  repeat;
- changed ID sets may produce content-key counts, but remain ineligible for a
  directional progress decision;
- ambiguity removes the duplicated content key from common/added/removed and
  score accounting;
- any inconclusive endpoint removes that key from both score denominators and
  directional counts;
- an unusable adjacency emits no comparison and resets the latest decision;
- individual pair observations cannot be saturated;
- saturation uses `consecutive_stalls >= patience`, including patience one.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public observation | Evidence | Mode/result |
| --- | --- | --- | --- | --- | --- |
| usable report | `OracleReport.usable` → `Report.usable` | owned complete schema-v2 report | public progress JSON | generated temporary report | `strict`, match |
| missing baseline | `.unusable .missingBaseline` | `baseline: null` | input disposition and latest decision | public CLI | `strict`, match |
| failed baseline | `.unusable .baselineFailed` | nonzero baseline exit | input disposition and latest decision | public CLI | `strict`, match |
| incomplete run | `.unusable .incomplete` | `summary.complete: false` | input disposition and latest decision | public CLI | `strict`, match |
| matching unique IDs | `Eligibility.matching` | same unique candidate-ID set | counts/state | public CLI | `strict`, match |
| changed unique IDs | `Eligibility.different` | different candidate-ID sets | counts/state | public CLI | `strict`, match |
| duplicate candidate ID | `Eligibility.duplicate` | parser rejects repeated stable identity | no comparison observation | parser contract | `model-only`, checked only in Lean |
| duplicate content | repeated `contentKey` | distinct IDs with equal content fields | ambiguity/counts/state | public CLI | `strict`, match |
| conclusive status | `.killed` / `.survived` | same public status strings | counts/scores/state | public CLI | `strict`, match |
| inconclusive status | one Lean class plus `StatusTag` | timeout, OOM, process-limit, error, not-run | counts/scores/state | five public cases | `strict`, match |
| positive patience | `patience : Nat` | `--patience 1..3` | suffix/latest/saturated | public CLI | `strict`, match |
| exact score | killed/decidable fraction | at most two conclusive mutants | JSON `f64` | exact small literals | `strict`, match |

The Rust adapter creates an isolated directory per case, writes owned
schema-v2 reports, invokes the built `hoimin progress --format json` binary,
and compares the public output. It recomputes only report summary fields that
the parser requires for fixture validity. It does not compute expected pair
states, comparison counts, scores, suffix length, or saturation.

Process spawn failures, timeouts, nonzero parser exits, and malformed stdout
are classified as `infrastructure-error`; none occurred in the final strict
run.

## Lean model and proofs

The imported modules are:

- `ProgressDecisionModel.lean`: pure pair/history semantics;
- `ProgressDecisionProofs.lean`: general theorems;
- `ProgressDecisionCases.lean`: fixed cases and broken witnesses.

The non-imported `ProgressDecisionAuditMain.lean` owns enumeration, statistics,
JSON serialization, generation, and freshness checks.

The proof module establishes:

| Theorem | Meaning and premises |
| --- | --- |
| `fold_consecutive_stalls_eq_trailing` | the fold counter equals the model's trailing-stall computation for every step list and patience |
| `unusable_resets` | appending an unusable adjacency leaves zero consecutive stalls |
| `nonstalled_resets` | appending any comparison whose state is not stalled leaves zero consecutive stalls |
| `saturated_implies_latest_stalled` | for positive patience, saturation implies the final step is a stalled comparison |
| `saturated_implies_patience_le_trailing` | for positive patience, saturation implies the suffix reaches patience |
| `patience_le_trailing_implies_saturated` | for positive patience, a suffix reaching patience produces saturation |
| `regression_precedes_improvement` | a positive regression count wins when a matching pair has comparable common mutants |
| `pair_never_saturated` | pair states contain no saturated constructor |
| `matching_uses_candidate_id` | matching eligibility selects the stable-ID join |
| `candidate_id_key_ignores_content` | candidate-ID keys do not depend on content roles |
| `inconclusive_transition_has_no_directional_or_score_counts` | an inconclusive endpoint contributes no direction or score denominator |

Nontrivial theorems retain local `maxHeartbeats 100000`. No module uses
unlimited heartbeats.

## Bounded refutation

The declared domain is:

- candidate-ID roles `0..1`;
- content-key roles `0..1`;
- statuses killed, survived, and inconclusive;
- zero, one, or two mutants per usable report, with unique ID roles;
- an unusable report class;
- history length at most four;
- patience one through three.

The executable generated 50 report states and checked all 2,500 adjacent
report pairs. For histories, it reduced each pair to one of five
decision-relevant steps: unusable adjacency, improving, regressing, stalled,
or indeterminate. It checked 2,343 `(step history, patience)` combinations
through length four. This is a state-equivalence reduction, not an unbounded
proof of report histories.

The independent executable specifications checked classifier precedence and
trailing suffix behavior. Fixed witnesses remain outside the reduction.

## Sensitivity

Corpus generation and freshness checking refuse to proceed unless all seven
broken variants are distinguished:

| Risk | Broken variant | Fixed witness | Result |
| --- | --- | --- | --- |
| precedence | improvement wins over simultaneous regression | one killed→survived and one survived→killed | detected |
| reset | indeterminate retains prior stall count | stalled then indeterminate | detected |
| reset | unusable adjacency retains prior stall count | stalled then unusable | detected |
| boundary | saturation uses `>` instead of `>=` | one stall, patience one | detected |
| identity | matching ID sets join by content | distinct IDs sharing content | detected |
| uniqueness | duplicate content is treated as unique | changed ID set with repeated content | detected |
| exclusion | inconclusive contributes direction/score | survived→inconclusive | detected |

Duplicate-key identity and ambiguity cover the applicable
uniqueness/idempotency family. Atomicity and transactionality do not apply:
the audited decision is a pure computation with no partial persistence or
effect commit.

## Corpus

`formal/HoiminOracle/corpus/progress-decision.jsonl` has schema version 1:

- 24 rows and 18,648 bytes;
- 23 `strict` cases;
- one `model-only` duplicate-candidate-ID case;
- five distinct public inconclusive status rows;
- Lean-owned pair counts, score fractions, signed score deltas, pair states,
  latest state, suffix count, and saturation.

The corpus was generated twice and compared byte for byte. The final
`--check` command exited zero. The corpus is generated output and must not be
edited by hand.

## Counterexample ledger

### Strict mismatches: none

All 23 same-premise public CLI cases matched every compared field. Therefore:

- no Lean witness required promotion to a Rust regression test;
- Task 4's conditional Rust repair was skipped;
- `crates/hoimin-cli/src/progress/compare.rs` is unchanged.

### Model-only boundary

`duplicate_candidate_id_model_only` exercises duplicate-ID eligibility in the
Lean model. The public schema-v2 parser rejects the same document before the
decision function, so it is not a production match or mismatch.

### Infrastructure boundary

The clean aggregate `lake -Kjobs=1 build` baseline was stopped by the fixed RSS
guard after 586 ms at 950,352 KiB. Its classification is
`infrastructure-error`; it supports no semantic conclusion. It was not retried
with a larger cap.

## Resource evidence

Every final Lean command used a 20-second deadline, 768 MiB (786,432 KiB)
process-tree RSS ceiling, 250 ms samples, and one command at a time.

| Focused command | Exit | Elapsed | Peak RSS |
| --- | ---: | ---: | ---: |
| model | 0 | 2,440 ms | 680,784 KiB |
| proofs | 0 | 587 ms | 678,592 KiB |
| proof consumer | 0 | 283 ms | 2,784 KiB |
| sensitivity | 0 | 288 ms | 2,400 KiB |
| fixed cases | 0 | 292 ms | 2,624 KiB |
| bounded stats | 0 | 289 ms | 2,800 KiB |
| corpus freshness | 0 | 293 ms | 2,752 KiB |

The first executable build plus sensitivity run peaked at 736,288 KiB, still
below the retained ceiling. Bounds were not increased.

## Verification

Focused Lean commands, each wrapped by `tools/lean_resource_guard.py`:

```text
lake env lean HoiminOracle/ProgressDecisionModel.lean
lake env lean HoiminOracle/ProgressDecisionProofs.lean
lake env lean /tmp/hoimin-progress-decision-proof-consumer.lean
lake exe generate_progress_decision -- --sensitivity
lake exe generate_progress_decision -- --cases
lake exe generate_progress_decision -- --stats
lake exe generate_progress_decision -- --check corpus/progress-decision.jsonl
```

All seven exited zero. The executable reported:

```text
report_domain=50
pair_checks=2500
history_checks=2343
fixed_cases=24
strict_cases=23
model_only_cases=1
```

Rust and repository checks:

```text
cargo test -p hoimin-cli --test lean_progress_decision_oracle -- --nocapture
cargo test -p hoimin-cli --test progress
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Every command exited zero. The focused oracle ran 2 tests with zero failures;
the existing progress suite ran 57 tests with zero failures; workspace suites
reported zero failures, with only explicitly ignored benchmark/subprocess
fixtures.

## Files and ownership

The audit adds the Lean model/proofs/cases/executable, deterministic corpus,
public CLI adapter, design, plan, and this report on branch
`audit/lean-progress-decision` in the isolated worktree. Production Rust source
is unchanged. Future progress-decision changes should update the Lean cases,
regenerate the corpus, retain all sensitivity witnesses, and rerun the public
adapter before changing this conclusion.
