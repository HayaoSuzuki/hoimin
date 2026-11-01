# Mutation score and exit policy Lean audit

## Verdict

Issue #310 found no same-premise Rust mismatch. The seven status counts,
inconclusive total, optional mutation score, summary-derived flags, composed
run flags, completeness, and exit-code precedence agree with the Lean model.
No production Rust correction was required.

## Audited contract

Each status increments exactly one canonical count. `inconclusive` is the sum
of timeout, out-of-memory, process-limit, error, and not-run counts. The score
is absent when there are no killed or survived results. Otherwise it is the
exact reduced fraction `killed / (killed + survived)` in Lean and the documented
`f64` projection in Rust.

Exit precedence is interruption 130, infrastructure error 2, baseline failure
3, incomplete run 4, survivors 1, then success 0. Survivors do not make a run
incomplete. A run is complete exactly when interruption, infrastructure,
baseline, and incomplete conditions are all absent.

## Kernel-checked claims

Lean checks the following claims for arbitrary natural-number counts, status
lists, policies, and run flags:

- `summarize_permutation_invariant`;
- `record_increments_exactly_one`;
- `summarize_counts_each_status`;
- `summarize_total`;
- `inconclusive_eq_five_status_sum`;
- `exactScore_eq_none_iff`;
- `reduceFraction_is_reduced` and `reduceFraction_preserves_ratio`;
- `exactScore_denominator_positive`, `exactScore_is_reduced`, and
  `exactScore_preserves_ratio`;
- `inconclusive_record_preserves_score`;
- `killed_update` and `survived_update`;
- the four named exit-precedence theorems;
- `complete_iff_no_failure_flags` and `composed_complete_iff`;
- `survivors_do_not_change_completeness`;
- `observation_order_invariant`.

The external consumer imports only the model and proof modules. Cases,
bounded enumeration, corpus generation, and mutation sensitivity remain in the
non-imported executable path.

## Corpus and Rust correspondence

The closed JSONL corpus contains 48 rows:

| Mode and scenario | Rows | Rust observation |
| --- | ---: | --- |
| strict summary | 10 | complete `summarize`, score bits, summary policy, completeness, exit |
| strict exit policy | 32 | every five-Boolean assignment through `exit_code_for` |
| internal composed policy | 5 | representative summary/run combinations |
| model-only exact fraction | 1 | exact ratio beyond lossless binary64 integer representation |

The Rust parser rejects unknown fields, unknown statuses, zero denominators,
crossed mode/scenario assignments, and scenario-irrelevant premises. It also
requires the exact 48-ID set and 32 distinct direct-policy assignments. Strict
score rows compare the bit pattern of `(numerator as f64) / (denominator as
f64)` with the public Rust score. Counts are small, their sum fits `u64`, and
every integer is at most `2^53`; the audit does not claim arbitrary exact
rational equality after an `f64` conversion.

An owned `RunState` fixture directly consumes all five Lean-generated composed
rows. It compares the private composed policy, completeness, and exit code,
then serializes and deserializes the actual public `RunFinished` event and
checks its counts, score, `complete`, and `exit_code` fields against the Lean
expectations. A separate exhaustive fixture checks four summary shapes across
all 16 run-level flag assignments as additional wiring coverage.

## Refutation sensitivity

The executable retains a minimized witness for each broken family:

| Broken family | Small witness |
| --- | --- |
| wrong or doubled status count | one killed result |
| inconclusive result in score denominator | killed + timeout |
| zero decidable results produce a score | empty summary |
| survived numerator replaces killed | killed + two survived |
| omit an incomplete class | one not-run result |
| survivors override incomplete | survivor and incomplete flags |
| survivors override baseline | survivor and baseline flags |
| survivors override infrastructure | survivor and infrastructure flags |
| survivors override interruption | survivor and interruption flags |
| baseline overrides infrastructure | both flags |
| infrastructure overrides interruption | both flags |
| survivor-only run marked incomplete | survivor flag |
| final completeness ignores composed summary | one not-run result |

All thirteen variants are detected. The bounded Boolean policy domain has 32
assignments; the status sensitivity alphabet has seven constructors. These
finite checks are refutation evidence, not substitutes for the universal
proofs.

## Overlap and exclusions

Issue #55 owns progress-report summary consistency. Result Lifecycle owns
identity/status conservation and lifecycle finalization. Progress Decision
owns comparisons between accepted scores. Report Sequence owns event ordering.
This audit uses their public types but does not repeat those contracts.

It does not redefine the score protocol, prove exact rational equality for
arbitrary `u64` values after `f64` conversion, or prove that an impossible
in-memory `MutationSummary` cannot be manually constructed with overflowing
field sums. Canonical summaries produced from representable status sequences
are the Rust arithmetic premise.

## Resource measurements

Each retained Lean command ran alone under a 20,000 ms deadline, a 786,432 KiB
root-plus-descendant RSS ceiling, and 250 ms sampling.

| Command | Elapsed ms | Peak RSS KiB | Exit / reason |
| --- | ---: | ---: | --- |
| proof module build | 285 | 2,016 | 0 / `child_exit` |
| external proof consumer | 2,433 | 657,504 | 0 / `child_exit` |
| sensitivity | 287 | 2,800 | 0 / `child_exit` |
| fixed cases | 286 | 2,848 | 0 / `child_exit` |
| corpus freshness | 288 | 2,912 | 0 / `child_exit` |

No retained command reached either limit. The largest sample remains 128,928
KiB below the RSS ceiling.

## Verification commands

```text
lake build
lake env lean /tmp/hoimin-mutation-score-exit-policy-proof-consumer.lean
lake exe generate_mutation_score_exit_policy -- --cases
lake exe generate_mutation_score_exit_policy -- --sensitivity
lake exe generate_mutation_score_exit_policy -- --check corpus/mutation-score-exit-policy.jsonl
cargo test -p hoimin-core --test lean_mutation_score_exit_policy_oracle --all-features
cargo test -p hoimin-core --all-features
cargo test --workspace --all-features
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```
