# Research evaluation design

Evaluate #1176, #1178 and #1180 before proposing product changes. The reader is
the maintainer deciding whether a local test-improvement workflow needs more
hoimin functionality. Baseline: main `89889b94b5113bf7979b05766a709cfd4afa74db`.

## Alternatives and decision

1. Implement a collector, state-machine engine and fault injector in hoimin.
   This adds interfaces before demonstrating a missing capability. Defer.
2. Use existing plan/verify with small, independently specified Python subjects,
   Hypothesis-generated concrete corpora, and a projected real retry function.
   This isolates each claim at low cost. Use this for the requested evaluation.
3. Run a large multi-project empirical study. This supports broader claims but
   costs substantially more and cannot substitute for a controlled first check.

These are evaluation choices, not approval of a production feature. No external
services, new dependencies, test collectors or product protocols are introduced.

## Subjects and independent requirements

- PBT: return exactly the distinct nonnegative input integers. Compare positive
  nonempty inputs with signed inputs including empty lists, zero and duplicates;
  cross both with a positivity-only property and exact membership requirements.
  This is a 2x2 factorial comparison, including interaction; never force a
  survivor into a single input/property cause.
- Stateful: a cache returns the most recent stored value; deleting a key makes
  subsequent reads fail. Compare fresh single-write reads with histories that
  warm, update and delete cached values. Each history starts with a fresh cache.
- Resilience: a bounded three-attempt publish operation retries transient write
  failures, restores the prior value on exhaustion and always closes the backend.
  Compare normal completion with fixed fail/succeed and exhaustion schedules.
- Real-code check: extract `verify_with_propagation` verbatim from
  `tools/pypi_provenance.py`, retain source hashes and constants, and replace only
  the external verification, clock and sleep dependencies. Compare immediate
  success with local HTTP-404 recovery scenarios. Existing upstream tests already
  cover recovery; this is a projected check, not a newly discovered product bug
  or a full upstream-suite run.

## Execution and evidence

Use one worker. Cap each subject at 24 candidates from explicitly named existing
operator families, each verify at 90 seconds, each mutant at 4 seconds, baseline
at 15 seconds, process-tree committed memory at 512 MiB and output at 256 KiB.
Use at most 15 minutes for the automated experiment; stop and retain partial
results if exhausted. All cases, seeds, settings, plans, candidate IDs, source
hashes, CLI reports and wall times are saved. Select candidates identically in
each arm; do not count a timeout, baseline failure or infrastructure error as an
assertion kill. Replayed tests use concrete JSON corpora, not just a saved seed.

PBT seeds 1176 and 1177 are discovery; 2176 is a held-out validation seed, with
40 generated examples per domain and no Hypothesis example database. Always
include separately labeled specification boundary cases. Record actual generated
examples and generation failures, including domains exhausted before 40 cases.
Normal test time includes Python/pytest startup. Record state construction/reset
time separately; record review units and review wall time rather than inventing
human-review savings. Different test commands are different conditions, so compare
candidate identities explicitly rather than pretending all reports form one
unchanged progress series.

## Design assessment

Initial score: 7.5/10 (6/8 diagnostic rows). The initial proposal lacks an explicit
interface contract and discoverable module boundaries. Resolve these by keeping
subject requirements in this document, test semantics in `project/test_subject.py`,
and CLI orchestration/evidence ownership in `evaluate.py`, with module docstrings.
One driver hides candidate selection, subprocess limits and evidence storage.
Avoid a configurable research framework: these four fixed subjects are the scope.
Final assessment must check all eight rows against the retained implementation.

## Evaluation plan

1. Write parameterized and property-based subject tests from the requirements;
   check that missing subjects fail collection, then provide bounded specimens.
2. Generate fixed corpora and retain concrete input/operation/fault schedules.
3. Confirm every original arm passes; discover with the real CLI; replay the same
   selected candidates across arms and inspect additional assertion kills.
4. Check representative witnesses independently, including generated histories,
   and distinguish already-covered real-code behavior from missing tests.
5. Assess whether a formal model clarifies a claim. Any checked theorem must
   state model assumptions and cannot certify Python/OS behavior or empirical
   effectiveness. Record resource bounds and any platform limitation.
6. Produce separate adopt/limit/defer decisions for each issue, with limitations,
   reproducible commands and complete evidence. Product implementation follows
   only if the evaluation demonstrates a concrete missing capability.

## Design self-review: five passes

1. Scope: evaluation is authorized; three new product subsystems are not assumed.
2. Causality: use a factorial PBT comparison and preserve interaction outcomes.
3. Independence: requirements precede specimens; real retry tests are already
   present upstream and must not be advertised as new fixes.
4. Resources: explicit finite candidates, one worker, fake local failures and a
   total deadline avoid external service effects and uncontrolled mutations.
5. Complexity: existing CLI owns mutation/validation; the probe owns only cases
   and observations; no generic plugin or collector boundary is created.

## Plan self-review: five passes

1. Save concrete cases in addition to seeds and record exhausted generation.
2. Compare identical candidate IDs and edits, despite intentionally changed argv.
3. Require passing original baselines and inspect intended assertion failures.
4. Charge preparation, generation, reset and execution; do not claim statistical
   speedups from a tiny controlled study or wall time dominated by process startup.
5. Keep finite execution, mathematical models and implementation correspondence
   separate; preserve inconclusive outcomes and avoid equivalent-mutant claims.
