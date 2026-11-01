# Issue 695: implementation assessment

Decision: implement an opt-in sampling mode on the current `feat/sampling-with-seed`
branch, starting from `1e58e1f`. Issue retrieved with `gh issue view 695` on
2026-10-10. Existing strict, file-diverse and line-diverse selection preserve rank
tiers; paging and execution limits cannot sample across those tiers. The existing
saved-plan validation and ordered execution path make the addition bounded.

The [survey](https://mutationtesting.uni.lu/survey.pdf), §5.1.2, pp.23–25,
discusses random sampling as cost reduction. [Wong and Mathur](https://ics.uci.edu/~iftekha/pdf/paper3.pdf)
study mutation-score estimation with samples. Neither establishes that killing a
sample detects every defect that killing the population would detect. Redundant
mutants can distort scores. These are motivation, not evidence of hoimin speedup.

Alternatives considered: retaining only ranked selection does not meet the use
case; reservoir sampling is possible but requires traversing the full population
for every sample and a separate order rule; partial Fisher–Yates gives a simple
specified ordered sample and deterministic prefixes. Use the latter, with a
versioned local generator and rejection sampling, without new dependencies.

## Five assessment reviews

1. Feature overlap: read CLI and selection implementation; no existing random
   mode. `line-diverse` added since the issue still preserves score tiers.
2. Benefit versus claims: a smaller execution set reduces executed mutants, but
   not necessarily elapsed time or defect coverage. Keep empirical limits explicit.
3. Population ownership: only the candidates in a complete saved plan qualify;
   selector/profile/operators constrain the population. Reject truncation.
4. Integration risk: reuse source hashes, ranking checks, discovery validation,
   resource enforcement and ordered scheduler; do not change mutation operators.
5. Feasibility and decision: dependency-free Rust selection, existing proptest,
   libFuzzer and Lean infrastructure suffice. Proceed with design and tests.
