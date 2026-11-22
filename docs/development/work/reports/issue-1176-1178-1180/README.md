# PBT, stateful and resilience evaluation

This report helps the maintainer decide what to adopt from #1176, #1178 and
#1180. All three approaches detect deliberately omitted checks in the controlled
subjects. Existing hoimin operators and `plan`/`verify` suffice for this evaluation.
Adopt the evaluation procedures as test-writing guidance; defer dedicated product
subsystems until a real workflow demonstrates an unmet need. This is an initial
evaluation, not a multi-project effectiveness study or completion of every future
research question. No production implementation or upstream bug fix is included.

## Decisions by issue

| Issue | Evidence | Decision |
| --- | --- | --- |
| #1176: inputs versus properties | 0/3 kills with narrow inputs and a weak property; 2/3 by expanding inputs; 1/3 by strengthening the property; 3/3 by doing both | Adopt a four-arm comparison recipe. Do not add an automatic binary classifier: some defects require both improvements. A collector is not needed to run this recipe. |
| #1178: operation histories | Fresh write/read tests kill 0/3 selected mutants; histories kill 3/3 | Adopt model-based histories for stateful targets. Existing statement deletion exposes all three omissions here; no new stateful operator or execution engine is justified by this sample. |
| #1180: fault scenarios | Normal execution kills 3/7 in the controlled recovery subject; fixed fault schedules kill 6/7. A real retry-function projection improves from 0/14 to 9/14 | Adopt target-side fault schedules with a passing baseline under the same schedule. Do not add a general fault injector or dedicated operator on this evidence. The real function's behavior is already covered by upstream tests. |

These are recommendations to limit product scope, not claims that such product
features could never help. Issues remain open; this work does not change labels,
close issues, publish a PR or silently make a product implementation decision.

## Experimental scope

The baseline is main `89889b94b5113bf7979b05766a709cfd4afa74db`, hoimin 0.15.0,
Windows, Python 3.14.9 and Hypothesis 6.170.15. The executable hash and full
environment are in [provenance.json](evidence/provenance.json). Source snapshots
and their hashes are retained. The experimental subjects are artificial except
for the retry function, extracted from `tools/pypi_provenance.py` with its four
propagation constants. Extraction preserves function text modulo host newline
encoding; imports and external dependencies are replaced by a local test seam.
This is not execution of the complete provenance module or its upstream suite
under mutation.

The independent controlled requirements, limits and pre-execution decisions are
in [design.md](design.md). The real retry behavior was specified before this
study in the [propagation-wait review](../../reviews/2026-10-12-pypi-propagation-wait.md):
404-only retries, 600 seconds, capped exponential backoff and 61 attempts.
The separate upstream propagation test run passed 12 cases (65 deselected).

The main experiment uses 27 distinct candidates and 66 candidate executions in
12 arms. Every arm has a successful original baseline, an identical candidate
set within its comparison, a complete report, and no timeout, resource-limit,
infrastructure-error or inconclusive outcome. A normal nonzero exit alone is not
enough to call a failure useful: 21 distinct killed candidates were also replayed
in isolated copies to retain the failing test and diagnostics. All 21 reproduced
the intended behavior failure: assertions, a missing expected exception, or an
injected exception escaping before recovery. None was a collection/import failure.
See [analysis.json](evidence/analysis.json) for candidate-level results and output.

## #1176: separate effects and interaction

The subject returns exactly the distinct nonnegative integers in its input.
The weak property checks only that no negative value is returned. The strong
property checks membership against an independent finite-domain specification.
Narrow input uses positive, nonempty lists; broader input retains those same
cases and adds empty lists, negatives, zero and duplicates. The same concrete
corpus is replayed for both properties, without regenerating inputs per mutant.

| Mutation | Narrow / weak | Broad / weak | Narrow / strong | Broad / strong |
| --- | --- | --- | --- | --- |
| `>= 0` becomes `> 0` | survives | survives | survives | killed |
| Invert the filter | survives | killed | killed | killed |
| Remove the filter | survives | killed | survives | killed |

The boundary mutant needs both zero in the input and a requirement to retain
zero. More inputs alone cannot repair the weak property; strengthening the
property alone cannot expose an untested zero. The inverted filter can be exposed
by either intervention. The experiment therefore supports interaction-aware
comparisons, not an exclusive “generator defect or property defect” diagnosis.

Discovery seeds are 1176 and 1177; seed 2176 is held out. Each of the five input
generation runs produced 40 cases with no generation failure. Separately labeled
specification cases are included in discovery. The held-out-only corpus produced
the same broad weak/strong counts (2/3 and 3/3) without those forced cases.
All concrete values and settings are in [generation.json](evidence/generation.json)
and [corpus.json](project/corpus.json). No example database is used. Finite survival
does not establish unreachable inputs, equivalence or completeness.

There is no information here that requires a new collector: saving the corpus,
changing one test dimension, and comparing candidate IDs already exposes the
interaction. Product automation needs evidence that doing this externally is a
repeated material burden, not just a favorable mutation score on this specimen.

## #1178: state and operation sequences

The cache specification is a map: reads observe the last write, and deleted or
absent keys raise `KeyError`. The model has no cache or invalidation logic.
Any key can be put or deleted; deletion is idempotent; missing reads are valid
observations rather than invalid generated operations. Every history constructs
a fresh cache, so no execution depends on a prior case's state.

The three selected `statement_delete` mutants remove update invalidation,
storage deletion and deletion invalidation. Single write/read cases miss all
three. These histories expose them:

- `put(0, 1) → get(0) → put(0, 2) → get(0)`: expected reads `[1, 2]`, stale read `[1, 1]`.
- `put(0, 1) → get(0) → delete(0) → get(0)`: expected `[1, missing]`, stale value `[1, 1]`.

The corpus retains initial state, values and operations, plus 40 generated
histories from seed 1178. The primary improvement is demonstrated by the explicit
specification histories, not attributed to random discovery. An additional
Hypothesis test checks generated histories and an exhaustive small integer corpus
checks the PBT subject. The smaller `@given` sequence approach is sufficient here;
a `RuleBasedStateMachine` becomes useful when dependent operation generation needs
bundles or preconditions. This is consistent with the
[Hypothesis stateful guidance](https://hypothesis.readthedocs.io/en/latest/stateful.html).

All three omissions are expressible with existing operators. This says nothing
about concurrency, distributed state, arbitrary history length or the adequacy of
operators for assignment-only state updates. Those need separate examples before
expanding scope.

## #1180: same faults, different code

The controlled backend changes its value to a partial state before an injected
write failure. Fault schedules are 0, 1 or 2 failures followed by success, and
3 failures exhausting the attempt budget. Expected outcomes include the final
value, attempt count, closure and restoration of the previous value on exhaustion.
Clock waiting and external services are absent.

Normal tests already detect write removal, close removal and `finally_unwrap`.
The last is killed even on success because returning early skips the unwrapped
cleanup. It is therefore **not** an additional fault-scenario benefit here.
Fault cases additionally detect a two-attempt loop, raising after the second
failure, and failing to raise at exhaustion. The `range(4)` mutant survives:
the unchanged third-failure guard still prevents a fourth write. This finite
observation is not an equivalence proof.

There is a coverage limitation: this operator selection does not mutate the
assignment that restores the previous value. Testing rollback on the original
does not establish mutation sensitivity to rollback removal. Do not count that
requirement as mutation-validated, or add an operator without investigating other
existing transformations and a real target that needs it.

In the real retry projection, all 9 additional kills concern removing or changing
the specified backoff delays. Five selected mutants remain: logging removal and
changes in zero/one constants associated with conditions or diagnostics that the
small projection does not distinguish. Full deadline/attempt-exhaustion scenarios
already exist upstream but are outside this projected comparison. It would be
incorrect to claim nine new defects or general resilience coverage.

## Costs and review burden

| Measurement | Observed value |
| --- | --- |
| Complete primary experiment, including preparation | 88.43 s |
| Corpus generation/extraction preparation | 0.253 s |
| Twelve normal test commands, including startup | 10.31 s total; 0.79–0.97 s each |
| Twelve plan commands | 1.593 s total |
| Twelve verify commands, including their baselines and worker copies | 74.80 s total |
| Diagnostic replay and evidence audit | 20.07 s |
| Fresh Cache + Backend construction, 1,000 samples | 300 ns median; 7,600 ns maximum; 0.286 ms total |
| Maximum reported owned workspace size | 97,252 bytes |

Reset numbers measure these local in-memory objects only, with interpreter clock
resolution and scheduling noise. They do not estimate a database/service reset.
The corpus has no filtering-based generation failures. One worker and the saved
Windows Job Object limits bound CLI executions; no live service is contacted.

Review requires 27 candidate descriptions, 21 distinct failure traces and the
independent requirements. Additional kills versus each weak arm total 18 (3 PBT,
3 stateful, 3 controlled recovery, 9 real retry); these are not 18 independent bugs.
No human review-time comparison was measured, so there is no claim of saved human
effort or a production performance advantage. Process startup dominates these
small test subjects. Raw timings are in [execution.json](evidence/execution.json).

## Formal claims and limits

[Evaluation.lean](Evaluation.lean) checks eight model-level theorems:

1. Expanding an input domain preserves existing kills.
2. Strengthening a property by implication preserves existing kills.
3. The example's strong property implies its weak property.
4. A mutant can require both input and property improvement.
5. Fresh-cache reads can agree despite a missing invalidation.
6. A warm cache followed by a different write distinguishes that omission.
7. A successful normal call need not distinguish two versus three attempts.
8. Two failures followed by success distinguishes those attempt budgets.

Lean 4.32.2 accepted the model with no `sorryAx`. Two deliberately incorrect
definitions were rejected: making broad inputs equal narrow inputs and making
the stale write invalidate its cache. See [model-checks.json](evidence/model-checks.json).
The ordinary Lean axioms reported are `propext` and `Quot.sound`; no custom axiom
or admitted proof is introduced.

These are **model-only** claims. There is no generated Lean/Python correspondence
corpus, proof of the Rust executor, or theorem establishing empirical usefulness.
Python witnesses provide separate implementation evidence. Linux's aggregate RSS
guard is not available for the native Windows Lean binary: the recorded checks
use a 30-second wall deadline, Lean's internal 2 GiB limit, one thread and disabled
asynchronous elaboration. These are not an aggregate process-tree RSS measurement
or a new integrated CI proof target.

## Verification and corrected pilot

- Original subject tests: 646 passed, including Hypothesis membership/history tests.
- Evidence/boundary tests: 453 passed, including 400 exhaustive small lists,
  generated cache/recovery/rollback properties and invalid-evidence rejection.
- Existing upstream propagation tests: 12 passed, 65 deselected.
- Replayed mutation diagnostics: all 21 killed candidates reproduced.
- Lean: 8 theorems accepted, 2 incorrect definitions rejected.
- A clean copy materialized from staged Git blobs passed all 1,099 evaluation
  tests and matched all five source/corpus/configuration hashes. It did not use
  ignored pilot directories or the original worktree's import paths; see
  [clean-checkout.json](evidence/clean-checkout.json).

The first subject test collection failed because the subjects did not exist;
this is setup red, not behavioral fault sensitivity. A later explicit negative
test showed that the audit accepted pytest collection exit 2; the audit now
requires pytest exit 1 for a kill and exit 0 for a survivor. Tests also reject
failed baselines, incomplete reports and inconclusive execution states.

The initial pilot mistakenly combined `--source .` with a line selector, which
also selected other Python files, including test code. Those observations are
excluded. The accepted runner selects only `--line`, checks the candidate path,
and compares full candidate identities across arms. A sandbox-only Windows
workspace-creation error is also excluded; accepted CLI runs used normal Windows
process control. A Lean wrapper timeout and an initial incomplete proof attempt
are not included as successful proof results. Pilot records are retained separately
in `pilot-evidence.json.gz`, a gzip-compressed JSON map from filenames to contents.
The CLI was then rebuilt from the pinned main with `cargo build -p hoimin-cli
--bin hoimin --locked --offline`. All 12 arms were repeated with that binary;
candidate identities and outcomes matched the pre-rebuild run exactly. Final
timings above refer to the rebuilt executable. `pre-rebuild-evidence.json.gz`
retains the earlier complete run; it is not an additional independent subject.

## Reproduction

Use the repository's development environment and a built hoimin binary. Run the
following commands from the repository root; on Windows substitute
`.venv/Scripts/python.exe` for `python` and a native Lean 4.32.2 path for `LEAN_EXE`:

```console
python docs/development/work/reports/issue-1176-1178-1180/evaluate.py
python docs/development/work/reports/issue-1176-1178-1180/check_evidence.py
python -m pytest docs/development/work/reports/issue-1176-1178-1180 -q
python docs/development/work/reports/issue-1176-1178-1180/verify_model.py --lean LEAN_EXE
python -m pytest tests/test_pypi_provenance.py -k propagation -q
```

The evaluator regenerates plans with local absolute paths; saved plans record the
original Windows invocation and are not portable launch files. Inputs, source
snapshots and candidate descriptors are the portable evidence. Running on another
OS is a new measurement, not reproduction of the recorded Windows resource mode.
The local `.gitattributes` preserves the exact Python snapshot bytes so Git newline
conversion cannot invalidate the recorded byte spans or source hashes.

## Research interpretation

[Ravi and Coblenz](https://cseweb.ucsd.edu/~mcoblenz/assets/pdf/OOPSLA_2025_PBT.pdf)
evaluate PBT detection using mutation testing across Python projects. Their study
motivates studying input and property strength, but does not establish our proposed
four-arm diagnosis as a product feature. [PropCov](https://conf.researchr.org/details/issta-2026/issta-2026-research-papers/37/PropCov-Effective-Coverage-Reporting-for-Property-Based-Testing)
addresses PBT coverage reporting; we have not reproduced its reachability analysis.

[RESTler](https://www.microsoft.com/en-us/research/publication/restler-stateful-rest-api-fuzzing/)
uses producer/consumer dependencies and response feedback to explore REST request
sequences. The local cache comparison borrows the motivation to examine histories;
it does not reproduce RESTler or establish its reported cloud-service results here.

[Lineage-driven Fault Injection](https://people.ucsc.edu/~palvaro/molly.pdf)
searches fault combinations using dependencies of successful distributed results.
Our fixed local schedules do not implement lineage analysis, distributed failure
search, or its bounded certification. They test the narrower applicability claim
in #1180 at low cost.
