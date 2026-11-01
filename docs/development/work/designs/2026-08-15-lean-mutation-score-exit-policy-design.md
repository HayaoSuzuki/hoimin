# Mutation score and exit policy Lean audit design

## Scope

Issue #310 owns the pure boundary between the seven canonical mutation
statuses, `MutationSummary`, `ExitPolicy`, `RunMachine::complete`, and the final
exit code. It does not re-audit result identity, report ordering, or progress
comparison.

## Recommended model

Use one small Lean model with:

- seven status constructors and a seven-field natural-number count;
- a fold that increments exactly one field;
- `inconclusive` as the sum of the five non-decidable fields;
- an exact optional fraction `killed / (killed + survived)`, represented by its
  numerator and denominator and compared by cross multiplication;
- five exit-policy flags and the precedence
  `interrupted > infrastructure > baseline > incomplete > survivors > success`;
- completeness as the absence of interruption, infrastructure failure,
  baseline failure, and incompleteness.

The imported proof modules contain definitions and kernel-checked theorems.
Finite tables, broken variants, corpus emission, and statistics live in a
separate executable.

## Correspondence worksheet

| Premise or observation | Lean representation | Rust observation | Mode |
| --- | --- | --- | --- |
| seven status counts and inconclusive total | `Counts` | `summarize` | `strict` |
| score absent/present and finite projection | `ExactScore` | `MutationSummary::score` bits | `strict` |
| exact fractions beyond the stated `f64` premise | natural-number ratio | none | `model-only` |
| summary-derived flags | `policyFromCounts` | `ExitPolicy::from_summary` | `strict` |
| all five-flag assignments | `ExitPolicy` | `exit_code_for` | `strict` |
| composed run flags and summary flags | `composePolicy` | owned `RunMachine` test fixture | `internal-fixture` |
| final count, score, complete, and exit code | `Observation` | public JSON `RunFinished` | `strict` when configurable |
| corpus, setup, or process failure | none | typed harness failure | `infrastructure-error` |

Strict score rows keep counts at or below `2^53`, require a non-overflowing
`u64` denominator, and compare the Rust result with the documented
`(killed as f64) / (decidable as f64)` projection. Exact rational equality is a
Lean claim, not an arbitrary `f64` claim.

## Proof obligations

Prove permutation invariance, one-field contribution, the inconclusive sum,
zero-denominator absence, score independence from inconclusive statuses,
killed/survived update equations, full exit precedence, and completeness.
The order-insensitivity theorem is stated over arbitrary status lists rather
than inferred from fixed cases.

## Executable evidence

Emit a closed JSONL corpus containing small status multisets, the complete
32-row exit-policy Boolean table, composed run-policy rows, and a model-only
large exact fraction. Fixed broken variants must produce minimized witnesses
for every family listed in #310. Exploration is bounded and reported as
evidence, separate from the universal proofs.

## Counterexample handling

If a same-premise mismatch appears, first retain its smallest corpus row as a
failing Rust test. Change production Rust only after that test fails, and make
the smallest policy correction that restores correspondence.
