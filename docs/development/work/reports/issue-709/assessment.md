# Issue #709: repeated verification adoption assessment

## Decision

**Defer the integrated `verify --repetitions` feature; retain the issue for
command-level evidence and a subsequent design decision.** This PR completes the
user-requested evaluation before implementation. It does not implement or claim
to close #709. Existing saved-plan verify invocations suffice for the bounded audit.

The problem is plausible and an integrated diagnostic could be useful. However,
this paper does not measure hoimin's observation unit, and the small local probe
found no decision-changing outcome in the real-package sample. That is insufficient
to justify a new execution/aggregation/report/progress contract now, and equally
insufficient to conclude that the feature would never be useful.

## Primary evidence

Parry et al., *Test Flimsiness*, ICSE 2026, §2.1–2.2 and Table 1: 15/28 projects
contained significant induced flakiness, affecting 58/8,400 mutants. A mutant
qualifies when at least one individual test changes from baseline stability to
intermittent failure. The experiment used 12,000 baseline runs and 10/100 runs per
mutant, fresh Docker filesystems, interleaved scheduling and JUnit observations.
§4.3 leaves consequences within particular mutant-driven techniques to future work.
These are neither a measured suite-level flip rate nor an upper bound on hoimin's
instability. [Original paper](https://o-parry.github.io/papers/2026a.pdf), pp. 3, 5, 10.

The [replication README](https://ndownloader.figshare.com/files/59157311) says its
pytest plugin forces exit 0 even when tests fail. Its process exits therefore
cannot estimate hoimin's classification changes; one would need the JUnit outcomes.
The [artifact metadata](https://api.figshare.com/v2/articles/30428569/versions/1)
listed a 9,544,554,500-byte archive; the README describes the separate raw dataset
as >130GB. Only metadata and the 10,683-byte README were downloaded. No artifact
pickle/code was loaded or executed. PDF tables were cross-checked against their
text labels; browser screenshot capture failed, so visual table verification was
unavailable. No claim depends on a plot-only observation.

## Applicability analysis

hoimin's `classify_mutant` observes the exit of the whole caller-supplied command.
For a conventional test runner, failure in any test yields nonzero. If test A is
intermittent while B always kills the mutant, every command still fails: the
mutation's killed label does not fluctuate. Neither 54% nor 0.69% measures the
frequency of the problem this CLI proposal would detect. Conversely, pre-existing
flakiness and rare/unobserved events can create additional command instability;
0.69% must not be presented as a bound.

Small N has limited sensitivity. Under the explicit assumptions of independent,
identically distributed ordinary exits, with per-run failure probability p, the
probability of observing both success and failure is `1 - p^N - (1-p)^N`.
For p=1%, N=3/5/10 gives about 3.0%/4.9%/9.6%; for p=50%, N=3 gives 75%.
These are algebraic illustrations, not measured rates, independence guarantees,
or a recommended universal repetition count. Finite agreement is not a proof of
future agreement. Shared external services and machine conditions may correlate runs.

Excluding observed mixed outcomes from a score changes its population. It can
raise or lower the score; it is not automatically an unbiased correction. An
implementation would need to report excluded counts, individual outcomes,
requested/completed attempts and incomplete resource outcomes explicitly.

The strongest argument for later integration is trustworthy orchestration:
shared deadlines and budgets, reset/integrity guarantees, traceable trial records,
consistent progress and one aggregation contract instead of user-written scripts.
Default N=1 would preserve ordinary cost. Those ergonomic benefits are real even
without reproducing the paper, but the implementation includes baseline gating,
partial attempts, limits, cancellation, report compatibility and score semantics;
it is larger than wrapping a process call in a loop.

## Local experiment and cost

Base: `9d813e5` (PR #728 merged). No product source was changed. The committed
[probe](probe.py) exercises the real CLI with temp projects and saved plans.
[observations.json](observations.json) records exact IDs, statuses, baselines,
completion flags, package versions/source hashes, authored checks and wall times.

| Controlled command behavior | Mutant observations, six invocations | Interpretation |
| --- | --- | --- |
| Always passes | six survived | positive control |
| Always fails | six killed | negative control |
| Alternates only under mutation | killed/survived alternating | observable command instability |
| Same alternation plus another always-failing check | six killed | per-test instability is masked |

Both failed-baseline controls produced no mutant results. The caller’s original source
remained unchanged after every successful-baseline control sequence. An external counter intentionally supplies
the alternating stimulus; it is not a natural-flakiness or independence experiment.

| Installed source with authored checks | Fixed candidates | Rounds | Changed labels | First round | Five rounds |
| --- | ---: | ---: | ---: | ---: | ---: |
| packaging/version.py | 4 | 5 | 0 | 0.321s | 1.610s |
| iniconfig/__init__.py | 4 | 5 | 0 | 0.212s | 1.071s |

These are narrow authored API checks, **not the upstream projects' test suites**.
They demonstrate the measurement workflow and its cost, not real-world prevalence.
The sample is too small and too deterministic to establish absence of flakiness.
No newly unstable real-package classification was found in this experiment.

Every verify invocation has its own baseline and budget. The audit is not a
simulation of a single N-repetition invocation with one shared deadline. Timings
include CLI startup/validation/copying and are single local measurements. Repeated
execution costs distinct-mutant coverage under a fixed wall budget; integration
might save some overhead, but cannot remove the repeated test executions.

## Formal model and correspondence

`formal/HoiminOracle/RepeatedVerificationAudit.lean` proves that an always-failing
check masks another check's result, and any finite all-passing prefix admits a
later failure. Two fixed broken-rule witnesses reject the opposite claims.
No `sorry`, `axiom` or `native_decide`; 20-second process timeout; no exhaustive
search or elevated tactic limits. The file checked successfully in 2.611s.

| Premise/observation | Representation | Public evidence | Mode |
| --- | --- | --- | --- |
| Conventional runner fails if any test fails | Boolean list `any` | controlled caller explicitly implements that rule | model-only |
| A deterministic failure masks another result | `deterministic_failure_masks` | six masked CLI observations | model-only |
| Finite observations do not imply future behavior | arbitrary n, replicated prefix plus failure | no finite experiment can certify all future results | model-only |

The CLI probe is an independent empirical check, not a Lean-generated corpus
adapter. The Lean proofs do not verify Rust, command behavior, randomness, workspace
reset, statistics, or all possible test-runner exit conventions.

## Reopening criteria

1. Gather a fixed candidate set from a real user's changing-outcome case and a
   separately identified sample, retaining initially killed and survived candidates.
2. Record baseline and mutant command outcomes, IDs, ordinary/resource terminations,
   timings and the concrete user decision changed by observing mixed outcomes.
3. Evaluate detection yield per additional test execution. Do not count a masked
   per-test fluctuation as a command-label correction or no observed flips as stability.
4. If repeated verification is a recurring need, design the integrated opt-in flag
   with explicit mixed/incomplete precedence, baseline gating, budgets and schema
   compatibility. Keep N=1 behavior and describe agreement only over observed trials.

## Validation and limitations

Built unchanged hoimin with `cargo build --offline -p hoimin-cli --bin hoimin`
using debug information and incremental compilation disabled. Executed the probe
with CPython 3.14; controlled assertions, source preservation and fixed candidate
identities passed. The initial package probe requested four candidates from an
operator set that yielded fewer for iniconfig; it failed its precondition and was
not counted as an observation. Adding identity/membership to the explicitly saved
operator set supplied four candidates, then the full probe passed.

An independent primary-source review recommended audit-first while recognizing
integration's potential usability benefit. This assessment preserves that nuance.
Product execution, report and score behavior are unchanged. Full Rust test-suite
execution is unnecessary for this documentation/model/experiment-only branch;
product behavior was exercised through the built public CLI.
