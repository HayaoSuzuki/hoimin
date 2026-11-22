---
type: Decision
title: Software-testing research evaluations
description: Seven scoped evaluations, adopted test-design guidance, and remaining product-adoption questions.
status: draft
sources:
- id: pbt-state-fault
  resource: ../work/reports/issue-1176-1178-1180/README.md
  revision: 10f06abe036791947e29d3cc127e192033dde2c5
  working_tree: clean
  sha256: 2e333bc0886195d2aea1f0d5cf62dce711a3d4a355e31ebbcae6f909a10697d5
- id: relations-contracts
  resource: ../work/reports/issue-1177-1179-1182-1186/README.md
  revision: 10f06abe036791947e29d3cc127e192033dde2c5
  working_tree: clean
  sha256: 131c48c9cb3cf5128e52b838f62d8cf60099ae68eda92c46094e0187798633ea
- id: improvement-skill
  resource: ../../../skills/hoimin-mutation-improvement/SKILL.md
  revision: 10f06abe036791947e29d3cc127e192033dde2c5
  working_tree: modified
  sha256: e6077ba8681dd8a1ea943f1b94f285dfbe4ad2d6cf1e8270b8cf31182b388da5
- id: test-design
  resource: ../../../skills/hoimin-mutation-improvement/test-design.md
  revision: 10f06abe036791947e29d3cc127e192033dde2c5
  working_tree: untracked
  sha256: 763db0041b5e6a8fcb2b47e76d35ea47aa13e7064097a188a2351867517d4a49
---

# Software-testing research evaluations

This page helps maintainers choose which testing techniques to use with hoimin
and whether they justify additional product features. It consolidates the November
2026 evaluations of seven research issues. Use the
[bundled test-design guide](../../../skills/hoimin-mutation-improvement/test-design.md)
for the reusable improvement procedure; use the linked reports for reproduction.[^test-design][^improvement-skill]

The experiments support specification-based test design with the existing CLI.
They do not establish a need for a built-in generator, fault injector, combination
engine or mock-drift classifier. These are scoped adoption decisions, not evidence
that those features could never help on other projects.

## Evidence and provenance

Both evaluations use product source at
`89889b94b5113bf7979b05766a709cfd4afa74db`, Windows, Python 3.14.9 and hoimin 0.15.0.
Source and binary hashes, inputs, commands, resource budgets, original baselines,
candidate results and failure diagnostics are retained in the reports:

| Evaluation record | Research commit | Population and scope |
| --- | --- | --- |
| [Property, history and fault testing](../work/reports/issue-1176-1178-1180/README.md) | `1e23185e` | 27 distinct candidates, 66 executions, 12 comparisons; three controlled subjects and a real retry-function projection [^pbt-state-fault] |
| [Relations, combinations, contracts and amplification](../work/reports/issue-1177-1179-1182-1186/README.md) | `10f06abe` | 19 distinct candidates, 140 executions, 25 comparisons; copies of three real code regions in hoimin's Python tooling [^relations-contracts] |

The first record has 646 subject cases and 453 evidence/boundary checks (1,099 in
total), 12 separate upstream propagation cases, and 21 failure replays.
The second has 538 original evaluation
cases and 19 failure replays. These are historical results at the recorded revision,
not promises about every host or later checkout. They are different populations;
do not combine their kill ratios into an effectiveness score.

Both records include finite Lean models and negative controls: eight theorems in
the first, six in the second, and two deliberately incorrect variants rejected in
each. These establish stated model properties and counterexamples, not correctness
of the Python, Git or hoimin implementations. The native Windows checks used Lean's
internal memory bound and wall deadlines, not an aggregate process-tree RSS guard.

Historical reports retain their original task-status statements. This page explains
what to adopt; it does not redefine old measurements or claim completion of all
future research questions.

## Findings by technique

| Issue and technique | Observed result | Adoption decision |
| --- | --- | --- |
| [#1176: property-based inputs and assertions](https://github.com/HayaoSuzuki/hoimin/issues/1176) | Controlled subject: narrow/weak 0/3, broad/weak 2/3, narrow/strong 1/3, broad/strong 3/3 | Compare input and observation changes separately when the cause is unclear. Some improvements require both. |
| [#1178: operation histories](https://github.com/HayaoSuzuki/hoimin/issues/1178) | Controlled stateful subject: single write/read 0/3, histories 3/3 | Use a specification-based model and meaningful sequences for stateful targets. No new execution engine is justified by this sample. |
| [#1180: fault scenarios](https://github.com/HayaoSuzuki/hoimin/issues/1180) | Controlled recovery: happy path 3/7, faults 6/7. Real retry projection: 0/14 versus 9/14; upstream tests already cover that behavior | Use deterministic, resettable fault schedules with passing original baselines. A general fault injector is not required for these cases. |
| [#1177: metamorphic relations](https://github.com/HayaoSuzuki/hoimin/issues/1177) | Real total function: unit cases 3/4, relations 2/4, unit cases plus relations 3/4. Existing aggregate tests already reach 4/4 | Use independently justified relations; evaluate a target with an expensive direct oracle before pursuing automatic relation generation. |
| [#1179: combinations](https://github.com/HayaoSuzuki/hoimin/issues/1179) | Four-factor gate: existing tests, full enumeration, pairwise and completed 3-way all detect 6/6 | Enumerate small domains. Verify generated coverage independently; evaluate constraints and larger domains before adding a generator. |
| [#1182: mock/fake contracts](https://github.com/HayaoSuzuki/hoimin/issues/1182) | Fixed fake 0/9, exact-argv fake 9/9, real Git and existing integration tests 8/9 | Compare observable contracts across real dependencies and substitutes. More kills from stricter call matching can be overconstraint. |
| [#1186: test amplification](https://github.com/HayaoSuzuki/hoimin/issues/1186) | Controlled weak tests on a real total function: narrow/weak 2/4, broad/weak 2/4, narrow/strong 3/4, broad/strong 4/4. Existing aggregate tests already reach 4/4 | Adopt the input/assertion comparison and a review record. Automatic generation and human productivity benefits remain unmeasured. |

All comparisons used successful original baselines and completed CLI results with
matching selected candidates within each comparison. Inspect failure reasons:
neither a raw nonzero exit nor a kill count alone proves a useful new test.

## Interpret the limits

### Inputs and observations interact

The two input/assertion experiments use different subjects and properties, so their
individual counts need not agree. Both show that broadening inputs alone can leave
a weak observation unchanged. Test both factors when necessary, keep the requirement
fixed, and compare with the smallest manual alternative and existing related tests.

An artificial weak control illustrates a mechanism; it is not the project's actual
test suite. In the real total and retry cases, related tests already detect behavior
that a selected narrow test misses. Do not count that as a newly discovered bug.

### Relation validity is separate from detection

A function that always returns zero satisfies both permutation and concatenation
relations for sums. An independent anchor or another justified observation is still
needed. Domain assumptions also matter: in the recorded Python environment,
`[1e308, 1e308, -1e308]` and its reversal have different sums because of overflow.

[MR-Coupler](https://arxiv.org/abs/2604.10126) motivates coupling-based relation
generation and mutation-assisted evaluation. The local study tests external
relations, not that generator's performance or its real-bug detection claims.

### Generated coverage needs verification

In the recorded allpairspy 2.5.1 invocation, `n=3` produced five rows missing 14 of
32 required three-factor projections. The evaluation stopped, retained that output,
then completed the set deterministically to ten rows and independently checked it.
The reported 3-way result belongs to the completed set, not the raw generator output.

Pairwise used six cases and detected all six CLI candidates. Equal-sized random
sets detected 6/6, 6/6 and 4/6 across three seeds. That small result is not a general
advantage estimate. Lean also provides a valid 3-way set that misses a specified
four-factor fault. Coverage strength does not guarantee arbitrary fault detection.

[NIST's combinatorial-testing guidance](https://www.nist.gov/publications/practical-combinatorial-testing)
motivates the method; the next useful evaluation needs enough factors or constraints
that complete enumeration is no longer an inexpensive alternative.

### Contract disagreement is not a call-shape difference

The fake objects in the Git experiment were deliberately constructed controls.
Ordinary contract tests already exposed their failure to honor `--no-renames` and
`-z`. Conversely, rejecting omission of a final `--` added one exact-argv kill
without demonstrating a different specified result on the tested valid requests.
Neither result establishes a newly found defect in an existing project's mock.

[The mock-object study](https://pure.tudelft.nl/ws/portalfiles/portal/94079936/Spadini2019_Article_MockObjectsForTestingJavaSyste.pdf)
motivates keeping substitutes aligned with real dependencies. Further evidence
needs an existing substitute, a real repair or integration-test addition, and
examples where differences are correctly permitted. Exceptions and state changes
also need broader evaluation than the recorded path-listing contract.

## Apply the findings and continue the research

The improvement skill incorporates the #1182/#1186 diagnostic procedure: check
existing coverage, separate inputs and assertions, identify a mock boundary,
compare observable contracts, verify intended failures, and leave a review record.
It uses the existing plan/verify workflow and does not change CLI formats or limits.

The following questions remain open:

- Can the procedures produce useful changes in other projects' existing tests,
  rather than only distinguish deliberately weak controls?
- Do relation tests help when an independent exact oracle is expensive?
- Do constrained combination models repay their construction and maintenance cost?
- Which generated proposals do humans accept, reject or defer, and how long does
  review take compared with manual survivor investigation?

[DSpot](https://arxiv.org/abs/1811.08330) motivates evaluating input/assertion
amplification and developer adoption. No human acceptance rate, review-time saving
or cross-project productivity result was measured here. Keep those questions open
when deciding whether dedicated product features would repay their complexity.

[^pbt-state-fault]: [Property, history and fault evaluation report](../work/reports/issue-1176-1178-1180/README.md).
[^relations-contracts]: [Relations, combinations, contracts and amplification report](../work/reports/issue-1177-1179-1182-1186/README.md).
[^improvement-skill]: [Mutation improvement skill](../../../skills/hoimin-mutation-improvement/SKILL.md).
[^test-design]: [Test-design guide](../../../skills/hoimin-mutation-improvement/test-design.md).
