# Fault-localization evaluation design

## Decision and scope

Evaluate whether mutation reactions help maintainers locate an already-observed
Python fault within a local time budget. This is research code, not a new hoimin
command. Ordinary run/verify still require a passing original baseline.
The user authorized evaluation of #1188 under the established review and testing
requirements. Publish only independently reproducible, public-source evidence.

## Approaches

1. Change run/verify to tolerate failing baselines: rejected because it weakens the
   meaning of existing execution results and mixes two different contracts.
2. Build a permanent test collector and complete kill-matrix service: deferred;
   its maintenance cost is not justified before measuring additional value.
3. Use existing plan candidates with a separate bounded pytest experiment: chosen
   by the implementer. Keep observation, ranking and experiment policy separate.

## Observations and ranking

A pytest collector records each test's setup/call/teardown outcomes, a digest of
exception type and assertion/exception message, and per-test executed source
lines. Fingerprints omit traceback source text so editing a source line does not
itself create a reaction. No claim is made to observe every program output.
Call-outcome changes and stable failure-message changes are two separately
reported observation models; neither is represented as a full Metallaxis replica.

The analysis core validates matching test identities, complete executions and
repeat agreement before calculating scores. Setup/teardown failures, collection
failures, missing tests, timeouts and inconsistent repeats are incomparable, not
positive mutation evidence. Repeat agreement does not prove absence of flakiness.
Outcome and fingerprint stability are checked independently; unstable rendering disables only
the fingerprint ranking. No port/address stripping changes the observed messages.
Baseline must contain both passing and failing comparable tests. Fixed revision
must pass the same selected tests; otherwise a historical case is rejected.

Use Ochiai for coverage and changed-test sets: failed_changed divided by the square
root of (total_failed * all_changed). A statement receives the maximum score of
its mutants. Preserve zero-score and unsampled lines in the common executable-line
universe. Report best/worst tie ranks and top-k bounds, never alphabetical tie
breaking as localization accuracy. Scores are not probabilities or repair proofs.

## Experiment boundaries

Use two public historical repairs in pytest-httpserver (empty mapping query values
and repeated server startup), with fixed tests and buggy/fixed source
revisions pinned before ranking. Report added-code repairs as omission regions,
not fabricated exact faulty lines. Keep the full selected module as search scope;
state this scope reduction explicitly, not as whole-project localization.

Generate candidates with hoimin plan from unmodified buggy source. Select by
failing-test coverage plus import/collection coverage, and a deterministic,
line-diverse candidate order that does not use known repair locations. Keep the
original generated plan and record truncation, eligible and attempted counts.
Use isolated copies and validate byte offsets, original bytes and source hashes
before applying exactly one candidate. Restore original source in finally; never
modify the reference checkout. Disable bytecode writes and give every observation
a fresh cache namespace so it can't read cached bytecode from another variant.

Record ordinary failure traceback locations, coverage ranking, mutation ranking,
source hashes, revisions, selected tests, candidates and wall times. Charge setup,
collection, baseline repetitions, generation and mutant repetitions to a declared
experiment budget. Do not call a lower rank a measured human-time saving.
Use two mutant repeats, a 20-second subprocess deadline, at most 96 candidates and
600 seconds per case initially. Preserve incomplete status when limits bind;
limits bound this experiment, not future product scope. No external services run.

## Module responsibilities

- analysis.py: validate comparable observations, compute reaction/coverage scores
  and honest tie intervals. It knows nothing about pytest or process execution.
- collect.py: observe one pytest invocation using documented pytest and coverage
  APIs; tests own their requirements and expected values.
- evaluate.py: own revisions, temporary copies, source identity, process limits,
  candidate selection and durable evidence. It creates its own temporary copy and
  validates the preparation receipt. Fatal unconfirmed cleanup quarantines the
  copy and stops all subsequent cases. This remains a research driver.
- test_analysis.py and test_execution.py: independent expected scores, property
  checks and real subprocess fixtures, one case per invocation with AAA sections.

The scoring boundary uses the following normalized observation contract. It
doesn't expose pytest hook objects, coverage objects, or operating-system handles:

| Field | Meaning |
| --- | --- |
| `complete` | All selected tests have supported setup, call, and teardown results |
| `tests` | Test ID to setup/call/teardown state, special-result flag, and failure digest |
| `coverage` | Source path to line number to test IDs; empty ID denotes execution outside a test |
| `statements` | Source path to the full executable-line universe |
| `tracebacks` | Test ID to observed source path/line pairs |
| `process` | Execution receipt retained for auditing; scoring doesn't consume stdout or timings |

The collector owns conversion into this contract; analysis owns validation and
ranking. `prepare_public.py` owns source export and input receipts.
`check_evidence.py` recomputes summaries from the frozen public observations.

## Design self-review

1. Utility: compare with ordinary failure locations and coverage, not only random
   ranking. No measured benefit is assumed from paper results.
2. Observation: pass/fail misses changed assertions; assertion text can be unstable.
   Report both and reject inconsistent repeated outcomes rather than hiding this.
3. Leakage: repair locations enter only evaluation, never candidate sampling or
   scoring. An omitted statement is a region, not an invented existing line.
4. Lifecycle: reference checkout remains immutable; each source edit is checked
   and restored. Failed/incomplete processes cannot fabricate completed tests.
5. Scope and complexity: no product option, collector protocol or matrix framework.
   Commit only reproducible inputs and explicit methodology, not local artifacts.

## Design assessment

Initial combined driver: 6/8 diagnostics, 7.5/10. Interface simplicity and
information hiding fail if pytest/process structures reach the scoring formulas.
The three-module boundary addresses both; see the final eight-row
[assessment](review.md#final-design-assessment). This is a complexity score, not
evidence of effectiveness.

## Sources

- [Metallaxis-FL](https://mpapad.github.io/publications/pdfs/STVR2015.pdf).
- [Comparison on real faults](https://arxiv.org/abs/1607.05512).
- [pytest hook API](https://docs.pytest.org/en/stable/reference/reference.html).
- [Coverage API](https://coverage.readthedocs.io/en/latest/api_coverage.html).

## Independent design-review resolutions

- The case budget is a monotonic deadline; every invocation receives the smaller
  of its limit and remaining time. Kill the owned process tree before restoring
  source or launching another candidate. On Windows use a kill-on-close Job
  Object and a bootstrap handshake before target code starts; on POSIX use a
  new process group. Treat inability to establish containment as an error.
- Case metadata freezes target regions before rankings. For a multi-line region,
  report reaching any target: best = one plus strictly higher scores; worst =
  non-target lines at or above the highest target score plus one. Also report
  full tie-group bounds so region size does not hide ambiguous ranks.
- The first collector accepts ordinary pass/fail call outcomes only. Skip, xfail,
  xpass, reruns, duplicate phases, missing calls and collection errors make a
  run incomparable. This is an explicit experiment limitation.

## Preflight case selection

The originally proposed string-query guard repair at
`e95fe118cd43400293c02068e03768a55084ec0c` did not yield a normal-use regression:
the constructor already rejected each supplied invalid argument on both versions.
It was rejected before mutation ranking, and replaced by the double-start
regression at `96bbe1990c985df132f21f243d5bb4fe1f33a7ba`. The replacement tests
reproduce one changed assignment, not the separate missing exception cleanup in
that commit. Case metadata freezes this narrower fault before ranking.

The initial driver preflight used SHA256 for a candidate's file_hash. Inspection
of hoimin-core identified BLAKE3 as the actual plan contract; regression tests now
use that contract. No mutant results from that rejected execution support a claim.
Source archive export, input validation, copy creation, baselines, candidate
collection and mutations are charged to the local experiment. Downloading tools,
cloning the source repository and human review time are outside the measurement.
