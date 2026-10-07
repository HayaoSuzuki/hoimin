# Issue 710: equal-budget line diversity assessment

Implement as an opt-in ordering policy. The useful case is a small batch whose
highest-score candidates concentrate on a few source lines. Reordering can expose
other assertion gaps without changing candidate generation, execution budget, or
default policy. It does not establish an optimal ranking or estimate a population
mutation score.

## Relationship to the original paper

[Petrović et al., *Practical Mutation Testing at Scale* (2021)](https://homes.cs.washington.edu/~rjust/publ/practical_mutation_testing_tr_2021.pdf)
§4.1 and §5.2 discuss selecting one mutant per changed, covered line and suppressing
unproductive locations. In the 5,000-changelist generation comparison, reported
median generated counts are 820 for traditional mutation, 77 for one per line,
and 7 when combined with arid-node suppression. Those are candidate-generation
counts, not measured hoimin runtime or defects found. Their random operator choice
and suppression differ from this deterministic, rank-preserving rotation that
retains every candidate. We use the paper as motivation for concentration control,
not as evidence of this policy's effect size. PDF text was inspected; the screenshot
service did not supply a visual rendering.

## Reproducible authored comparison

Run `trial.py` with the repository Python and built binary:

```sh
.venv/bin/python docs/superpowers/reports/issue-710/trial.py target/debug/hoimin observations.json
```

[Committed observations](observations.json) contain sources, source hashes, commands,
selected IDs/locations/statuses and every elapsed time. Two deterministic fixtures
have ten equality mutants on one return line and two on separate return lines.
Only `compare_eq_ne` is enabled. All twelve candidates share a score. Each policy
runs exactly three mutants, jobs=1, with identical limits and Python. Five weak-test
rounds rotate policy invocation order. One strong-test round adds assertions to the
two initially unchecked functions. Plan commands differ only by those assertions;
selected IDs remain identical within a policy. Temporary roots are removed and the
caller's source bytes are checked after each invocation.

| Fixture | Policy | Distinct start lines | Survivors / 3 | Median elapsed seconds |
| --- | --- | ---: | ---: | ---: |
| one file | strict | 1 | 0 | 0.197613 |
| one file | diverse | 1 | 0 | 0.187008 |
| one file | line-diverse | 3 | 2 | 0.162770 |
| two files | strict | 1 | 0 | 0.455365 |
| two files | diverse | 2 | 1 | 0.429885 |
| two files | line-diverse | 3 | 2 | 0.394898 |

All five rounds retain the same IDs/statuses. After adding the missing return-value
assertions, every selected mutant is killed, including both new line-diverse
survivors. This demonstrates two additional assertion gaps in these authored
examples; they are not newly discovered defects in a real project.

Elapsed time includes CLI startup, baseline, worker setup and selected executions.
A full workspace test run was active on the same machine, so timing is descriptive
only: no speedup, regression bound or statistical significance is claimed. Smaller
elapsed values for survivors can also reflect the avoided assertion traceback.
There is no representative-project sample. In a single-line source or singleton
line groups the policy may offer no additional reach; uncovered, equivalent or
low-value candidates remain possible. More lines alone do not prove more useful
mutants. Strict remains the default.
