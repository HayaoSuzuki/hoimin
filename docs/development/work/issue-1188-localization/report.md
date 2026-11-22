# Fault-localization results and adoption decision

This report helps hoimin maintainers decide what evidence is still needed for
[#1188](https://github.com/HayaoSuzuki/hoimin/issues/1188). The initial experiment
finds a small additional signal in one historical Python fault, but doesn't yet
justify a production localization command. It also doesn't establish that the
technique is ineffective: the observation model, corpus, and search scope are
limited. Keep the research issue open.

## Cases and method

The experiment uses unchanged regression tests from two historical
pytest-httpserver repairs. Each case exports the fixed revision and replaces one
complete module with its buggy parent version. The repair target, selected tests,
candidate order, and budgets are frozen before ranking:

| Case | Historical repair | Buggy baseline | Search scope | Frozen target |
| --- | --- | --- | --- | --- |
| Empty mapping query value | [Keep blank query values](https://github.com/csernazs/pytest-httpserver/commit/3e6c80d01f344929743cc25efa6e98ac4021f6de) | 12 failing, 7 passing | 527 executable module lines | `parse_qsl` call, line 242 |
| Repeated server start | [Fix readiness state](https://github.com/csernazs/pytest-httpserver/commit/96bbe1990c985df132f21f243d5bb4fe1f33a7ba) | 1 failing, 2 passing | 511 executable module lines | State assignment, line 985 |

Both fixed versions must pass the same selected tests twice. Buggy observations
must agree on test identities, ordinary call outcomes, setup/teardown success,
and coverage. Failure-message stability is checked separately. The repeated-start
case covers the moved state assignment; it doesn't cover the separate omitted
cleanup repaired in that commit. No claim about omission localization follows.

Coverage and mutation scores use Ochiai. Mutation scores measure which originally
passing and failing tests change, with two models: call outcome alone, or outcome
plus a digest of exception type and message. The score for a candidate's line is
the maximum of its mutant scores. This follows the aggregation idea in
[Metallaxis-FL](https://mpapad.github.io/publications/pdfs/STVR2015.pdf), but the
limited observations aren't a reproduction of its complete output model. Results
from [comparative real-fault research](https://arxiv.org/abs/1607.05512) motivate
evaluation and aren't transferred to Python as expected accuracy.

Candidate selection uses failing-test and import/collection coverage, then a
deterministic round-robin order across lines. Repair locations enter only the
rank calculation. All executable lines of the selected module remain in the
ranking, including unmutated and zero-score lines. A rank interval describes the
best and worst positions within ties; it isn't a measured number of lines a
maintainer inspected.

## Measured results

The final measurements and exact counters are retained in
[the evidence summary](evidence/summary.json). A zero-score tie over the full
module means the model provides no ordering of its lines. A missing fingerprint
rank means the baseline messages aren't stable enough to compare.

The run took place on 2026-11-22 with Windows 11, Python 3.14.9, and hoimin 0.15.0.
The hoimin reference checkout is `8a262a1832e89057d3ce5d9de576411279caa6df`;
this evaluation changes no production Rust source. Each record retains binary and harness
hashes, interpreter details, and observed dependency versions. Wall times describe
this host and run, not a performance guarantee.

| Case | Coverage rank | Outcome rank | Fingerprint rank | Sample status |
| --- | --- | --- | --- | --- |
| empty-query | 1-3 | 1-527 | 1-2 | Complete |
| readiness | 38-81 | 1-511 | Unavailable | Partial: time budget |

Neither case has an exception traceback frame in the selected source module;
the ordinary failure remains an assertion in the test. This records the absence
of a direct source-frame hit, not the absence of useful context in the failure
message. The harness doesn't simulate a maintainer reading that context.

The empty-query case narrows an already-small coverage tie by one line. Its
outcome-only model doesn't distinguish the module's lines. The fingerprint model
does because some mutants change a failure message while the test still fails.
The added runtime therefore buys a small ranking improvement in this case, with
no evidence yet of a net reduction in investigation time.

At a top-three inspection cutoff, both coverage and fingerprint ranking include
the empty-query target regardless of tie order. Neither guarantees it is first.
The summary retains possible/guaranteed top-1, top-3, top-5, and top-10 bounds for
each case instead of turning two observations into a population accuracy claim.

Repeated-start failure messages include varying local server details. Raw
fingerprints disagree across baseline repetitions, so their ranking is
unavailable. Removing ports or addresses after seeing the result would change the
observation model; this experiment preserves the rejection. Outcome evidence is
reported separately: after 42 attempted mutants, the 600-second budget expires
without an outcome-based ordering of the 511 lines. This partial sample doesn't
establish what the remaining mutants would show. In particular, the frozen
selection contains one candidate on target line 985, but execution doesn't reach
it before the deadline. The empty-query target has one selected candidate, which
does run. The second case therefore measures the information obtained within the
budget, not the effectiveness of mutating its repair location.

## Cost and rejected observations

Each case has a 600-second local budget, at most 96 mutants, and two observations
per mutant. The measured duration includes source export, input verification,
owned-copy preparation, fixed and buggy repetitions, candidate generation, and
mutant repetitions. Dependency downloads, repository cloning, and human review
time are excluded. The raw records separate preparation and individual process
durations. The elapsed field ends before the final record write and owned-directory
deletion, so whole-driver wall time can be slightly longer. Process cleanup can
also finish just after the deadline.

| Case | Generated / eligible / selected | Attempted | Comparable outcome / fingerprint | Rejected outcome / fingerprint | Elapsed seconds |
| --- | --- | --- | --- | --- | --- |
| empty-query | 557 / 35 / 35 | 35 | 27 / 27 | 8 / 8 | 128.60 |
| readiness | 552 / 151 / 96 | 42 | 31 / 0 | 11 / 42 | 600.02 |

The two buggy baseline/coverage observations take 3.61 seconds for empty-query and 15.11 seconds for repeated-start. These are components of the full elapsed cost, not additional costs. Preparation through candidate selection takes 8.67 and 34.73 seconds, respectively. Neither plan is truncated. The first selection covers every eligible candidate; the second selects 96 of 151 eligible candidates and reaches 42 before the time limit.

`complete` means every selected mutant was attempted twice or rejected for invalid
syntax. It doesn't mean all observations are comparable or all generated mutants
were selected. `sample_exhaustive` reports whether selection covered every eligible
candidate without a truncated plan. Timeouts, missing tests, collection and
fixture errors, special pytest outcomes, and inconsistent repetitions are
rejected instead of treated as useful reactions.

For outcome comparison, empty-query rejects eight incomplete executions.
Repeated-start rejects eight incomplete executions, two mutants with a timed-out
observation, and one whose observation reaches the case deadline. Its fingerprint
model excludes all 42 attempted mutants because the original rendering is
unstable; this doesn't imply 42 unstable mutant executions.

Candidate start lines can fall outside coverage.py's executable-line list, for
example inside a multiline expression. The summary counts these as unmapped;
their scores aren't assigned to a neighboring statement. This is an explicit
mapping limitation, not evidence that those mutations can't help localization.
In the empty-query case, all comparable unmapped candidates have no changed-test
signal under either model.

The design also retains the rejection of an initially considered type-guard
repair: its proposed normal-use inputs already failed in the constructor on both
versions. It was replaced before ranking. The earlier pilot measurements aren't
combined with the final run; the final driver includes source provenance checks,
owned copies, fresh bytecode-cache namespaces, and process-tree containment.

## Remaining evaluation work

The retained evidence meets the initial reproducibility and per-test observation
parts of #1188. It leaves these adoption questions open:

- Measure actual inspected locations and repair effort under the same total
  budget, including the time available to a coverage-only investigator while
  mutations execute. Rank intervals are only a proxy for this comparison.
- Include a historical case whose traceback already identifies the cause as a
  control, and more independent projects and fault classes. Two repairs in one
  module don't establish general effectiveness or a statistical hit rate.
- Evaluate whole-project search, omission regions, and multiple causes. Knowing
  the module reduces the search space before any ranking occurs.
- Improve observation and source-line mapping only with a separate evaluation
  set. A broader output model, stable diagnostics, or a different mutant budget
  could change the result; changing them on these two faults alone risks fitting
  the method to its examples.
- Measure maintenance cost for a supported collector and separate failing-baseline
  execution contract before adding a product command.

The evidence supports retaining a reproducible research harness and continuing
evaluation when suitable controls and investigation measurements are available.
It doesn't support weakening `run`/`verify` baseline checks or promising repairs
from mutants that make a test pass.

See the [reproduction procedure](README.md#reproduce-on-windows) and
[review and verification record](review.md) to audit these conclusions.
