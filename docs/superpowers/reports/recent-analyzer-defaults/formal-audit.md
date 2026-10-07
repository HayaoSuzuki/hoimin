# Default promotion formal audit

Claim: add the two recent operators only when selectors are omitted; exclusions
win, and removing both additions recovers the previous selection. Explicit and
saved selections stay frozen. Existing DefaultOperatorSelection algebra applies
with `legacy` meaning the previous50 set and `promoted` meaning the new pair.

| Premise / observation | Lean representation | Implementation observation | Mode |
| --- | --- | --- | --- |
| Previous50 + two additions | `defaults previous recent` | exact core membership; public omitted plan | model-only |
| Explicit override / final exclusion | `select` | explicit plan and individual public exclusions | model-only |
| Undo only this rollout | `incremental_opt_out_recovers_previous` | both exclusions equal explicit previous50 candidates | model-only |
| Saved selection remains stable | `reload` | previous50 saved-plan verification | model-only |

The proof uses arbitrary membership functions on Nat, not Rust enum discriminants.
Rust/public tests independently check the actual71-ID configuration domain; they
are not a generated Lean oracle or a proof of serde/CLI/parser execution. No new
coverage, type, mutation-effectiveness or ranking claim. No exhaustive search or
trace bound; theorems are universal under the stated disjointness premises.
Sensitivity witnesses exercise incorrect explicit/default union, exclusion before
promotion, accidental removal of an old promotion, and default expansion on reload.
Atomicity/concurrency families do not apply to this pure selection function.

Local commands retain a20-second/2048-MiB external guard. CI gets an explicit guarded
check for the preexisting standalone selection module. Costs/results follow after
execution; no `sorry`, `axiom`, `native_decide` or increased proof limits.

RED rejected the claim that rolling back only the recent pair also removes an older
promotion: 4488ms,661872KiB, exit1 (the intended `decide` rejection). GREEN checked
all eight selection theorems and four fixed sensitivity inequalities: 2576ms,
594144KiB, exit0. Limits stayed20s/2048MiB; no larger run was attempted.

Commands from `formal/HoiminOracle`:
`python3 tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 2048 --sample-ms 100 --stats /tmp/selection-stats.json -- lake env lean -j1 -DElab.async=false DefaultOperatorSelection.lean`.
The RED used the same guarded command on a temporary copy with the deliberately
false earlier-promotion assertion; the permanent incremental-recovery inequality
retains that sensitivity. No counterexample to the actual stated model contracts.
