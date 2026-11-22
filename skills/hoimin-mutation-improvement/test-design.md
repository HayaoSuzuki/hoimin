# Diagnose a surviving mutation

Use this guide after identifying a surviving candidate and an independent
behavioral requirement. It helps you choose a test change and explain its value.
The [improvement skill](SKILL.md) owns execution budgets, plan validity, cleanup
and stop conditions; these examples do not change those rules.

## Establish the comparison

1. Record the candidate, selected production boundary, normal test argv and
   requirement. Obtain the expected result from a specification, documented API
   or agreed contract, not by copying the current implementation's output.
2. Inspect existing unit, property-based and integration tests for that requirement.
   Record a test that already detects the candidate before proposing another.
   A focused run's survivor is not proof of a gap across the existing suite.
3. Choose the smallest useful comparison within the remaining budget. Keep input
   corpora or seeds reproducible and reset dependency state between cases.
   Include setup and reset costs when comparing runtime.
4. Keep source, selectors, operators, profile, limits, fingerprint inputs and test
   argv fixed for each plan. Test-only edits can use that plan with a fresh baseline.
   Replan before changing argv, including a pytest `-k` expression. Record environment
   and dependency changes as different execution conditions; matching candidate IDs
   alone does not make those observations a controlled improvement comparison.

Compare only complete results and identical candidate-ID sets with `progress`.
If a comparison cannot be completed within budget, retain the observations and
mark its conclusion unresolved.

## Separate inputs from assertions

An input change reaches a behavior; an assertion observes a requirement. They can
be jointly necessary. When the cause of a gap is unclear, compare these alternatives:

| Variant | Inputs | Observations |
| --- | --- | --- |
| Original | Existing cases | Existing assertions |
| Input only | Add or broaden cases | Keep the original assertion logic |
| Assertion only | Keep the existing cases | Add a specification-backed assertion |
| Combined | Use the broader cases | Use the stronger assertion |

Run alternatives independently rather than cumulatively so each result has an
attributable cause. Keep only the justified final test change. This four-way
comparison is a diagnostic aid, not a requirement to spend four runs on an obvious
gap. Record a skipped alternative as unmeasured.

For example, suppose the contract is the sum of bounded nonnegative integers and
a faulty implementation returns zero for every input. With `[0]` and an assertion
that the result is nonnegative, it survives. Adding `[2, 3]` while keeping that
assertion still misses it. Checking exact equality for `[0]` also misses it.
The combined check, an expected total of `5` for `[2, 3]`, detects the wrong result.
Both changes matter; calling this only an input gap or only an assertion gap loses
the interaction. This is a constructed example, not a newly discovered bug.

Before claiming added detection, inspect tests of callers too. If an existing
aggregate test already checks a nonzero total and kills the candidate, adding the
small unit case does not add a suite-level kill. It can still have another stated
benefit, such as a more precise failure location; do not invent a speed benefit.

Use parameterized tests for concrete cases and property-based tests for justified
input domains. Keep one scenario per test invocation with Arrange, Act and Assert
separated. For properties or metamorphic relations, state the domain and assumptions;
relation agreement alone does not establish correctness. For example, a function
that always returns zero satisfies permutation and concatenation relations.

## Check a mock or fake against its contract

1. Identify which object is replaced and whether the mutated behavior lies inside
   the caller, substitute or real dependency. A caller test intentionally replacing
   a dependency cannot assess that dependency's internal behavior.
2. Define the observations shared by the real dependency and substitute: returned
   values, specified exceptions, and externally observable state changes. Include
   permitted differences, such as ordering when the API promises only a set.
3. Apply the same contract cases to a real local dependency and the substitute.
   Use isolated temporary data and reset it between cases. If the real dependency
   is unavailable, report that limit; substitute-only success cannot establish
   agreement. Do not require an external service merely to complete this workflow.
4. Check ordinary contract tests first. Record discrepancies they already expose
   separately from any additional insight obtained through mutation.
5. Propose a substitute repair or an integration test only when an independent
   requirement supports it. Keep correct caller isolation when testing the caller
   is the intended scope.

Consider a Git adapter that lists changed paths. A fixed-name fake can ignore
whether its request asks for NUL-delimited output or disabled rename detection.
An ordinary contract test with a local rename and a non-ASCII path can expose that
disagreement. Mutation can help identify the unobserved argument, but the contract
test provides the evidence of disagreement.

An argv-exact mock can also reject a harmless call change. If removing a final
`--` leaves the specified results unchanged on the tested valid requests, an extra
kill proves an argv difference, not a behavioral defect on those inputs. It does
not prove equivalence on all inputs either. If an API explicitly requires a call
shape, ordering, or a number of calls, test that requirement and cite its basis.

## Verify the proposed change

Run the original code and confirm that the proposed test passes. Then verify the
same candidate and inspect why the test fails. A completed `killed` status is
necessary execution evidence, but an unrelated exception, collection failure or
an overconstrained mock does not demonstrate the proposed assertion's value.
For an expected exception or state transition, identify the corresponding contract
observation explicitly. Diagnose incomplete or error results before proceeding.

Do not describe a weaker artificial control as the existing test suite. Separate
newly detected requirements from already-covered behavior and from better localized
tests. Compare a generated proposal with the smallest manual alternative when
assessing whether generation is worth adopting. A reasoned alternative can establish
that a simpler test exists; claims about human effort or runtime require measured
work, not an estimate presented as an observation.

## Leave a review record

Use a short human-readable record; no new hoimin file format is required:

| Field | Record |
| --- | --- |
| Requirement | Independent source, observable outcome and valid input domain |
| Context | Candidate IDs, target boundary, test argv, dependency/configuration context and any replan reason |
| Existing coverage | Relevant unit/property/integration tests and observed results; unrun tests remain unverified |
| Alternatives | Input-only, assertion-only, combined or real/substitute comparison, and why any alternative was skipped |
| Evidence | Original result, mutant status, exact behavioral failure and retained report location |
| Additional value | Newly checked requirement, already-covered behavior, or another justified benefit |
| Recommendation | Adopt, reject or defer, with rationale; keep actual human acceptance separate |
| Cost and uncertainty | Cases, setup/reset/execution time when measured, review time, maintenance burden and unresolved assumptions |

Write `not measured` for unknown costs and `not reviewed` for unknown human
acceptance. A fixed corpus's repeated cases are not independent samples. Reject
proposals that merely freeze an implementation detail; defer unresolved material
specification questions to the human. Stop when no justified improvement remains
within the authorized scope and budget.
