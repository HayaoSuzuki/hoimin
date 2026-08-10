# Lean Result Lifecycle Consistency Audit

Date: 2026-08-10

Base revision: `41495cd800a750ab89602ccbcaf83c41c4a2f048`

Audit branch: `audit/lean-result-lifecycle`

## Outcome

The audit found no implementation mismatch in the six strict public-CLI
scenarios. Accepted mutant identity and status agreed across the JSON report,
summary counts, SQLite session rows, metrics execution count, completion flags,
diagnostics, and exit code for every same-premise case.

Lean did not prove the Rust implementation. Lean owns the finite transition
model and generated expectations; the Rust adapter independently observes the
implementation and compares those observations. No production source was
changed. No counterexample ledger or bug Issue was created because no confirmed
implementation bug remained.

## Durable claim and boundary

Within the declared two-mutant, seven-status transition model, every transition accepted
by `ResultLifecycle.step` preserves:

- summary entries equal the ordered statuses of summarized results; and
- `metricsExecuted` equals the number of current-run accepted real results.

The executable `safe` predicate additionally checks unique accepted, durable,
summarized, and reported identities; backing of every cross-surface result;
status preservation; complete-run coverage; session finalization ordering; and
the relationship between stop/fatal diagnostics and run completeness.

The correspondence claim is limited to the six strict scenarios below. It does
not claim correctness for arbitrary numbers of mutants, arbitrary event depth,
hard-cgroup OOM/process-limit behavior, OS signal delivery, report-sink failure,
SQLite corruption, or every possible asynchronous interleaving. The
`internal-fixture` and `model-only` cases explicitly mark boundaries that are
not represented as stable public-CLI strict fixtures.

## Declared and implicit behavior

The declared behavior was taken from the state-machine, session, report, and
metrics contracts already implemented in the repository:

- A real result is accepted once and never becomes `not_run` later.
- Session persistence is atomic: a failed result transaction leaves no partial
  candidate or result row, but the classified result remains reportable.
- A resumed determinate result is durable and reportable but is not executed in
  the current run and contributes zero to current-run metrics.
- Stop preserves already accepted results and marks only remaining discovered
  candidates `not_run`.
- Metrics write failure is diagnostic-only; it does not change the report or
  normal exit policy.
- Fatal persistence/report diagnostics make a run incomplete; metrics failure
  alone does not.
- `timeout`, `out_of_memory`, `process_limit`, and `not_run` results are
  inconclusive and make the run incomplete; `error` additionally selects the
  infrastructure-error exit code. Only killed/survived result sets are
  conclusive.

The audit made two previously implicit distinctions explicit:

- `summarized` and successful report acknowledgement are separate states. This
  permits auditing stop after result accounting but before report delivery.
- `seededDurable` and current-run `accepted` have distinct provenance. Without
  that distinction, resume incorrectly increments current-run execution count.

## Model worksheet

State consists of `setup`, `accepted`, `durable`, `persistenceFailures`,
`summarized`, `reported`, `summary`, `metricsExecuted`, stop/session/metrics/
return/completion flags, and diagnostics. Setup carries session/metrics
availability, discovered roles, and resumed durable results.

The event alphabet is:

`discover`, `accept`, `persistOk`, `persistFailed`, `recordResult`, `reportOk`,
`reportFailed`, `stop`, `markNotRun`, `finishSession`, `finishMetrics`,
`metricsFailed`, and `returnRun`.

Invalid events are rejected without state change. Events after `returnRun` are
also rejected. Result-lifecycle mutations are rejected once either the session
or metrics surface has been finalized; the remaining surface may still be
finalized before return. `accept notRun` is invalid; `markNotRun` applies only after stop
and only when no accepted or durable real result exists. In a session run,
recording a current result requires either a matching durable row, a
persistence failure, or stop already taking precedence.

## Kernel-checked results

The following theorem bodies contain no `sorry`, `admit`, or custom axioms:

- `initial_invariant`
- `acceptState_invariant`
- `proposal_preserves_structural`
- `rejected_preserves_state`
- `step_preserves_invariant`
- `runWith_preserves_invariant`
- `run_preserves_invariant`
- `stopped_resume_not_run_is_rejected`
- `stopped_not_run_does_not_increment_metrics`
- `accepted_status_is_stable`
- `complete_report_summary_corresponds`
- `finalized_rejects_lifecycle_mutation`
- `incomplete_session_cannot_return_complete`
- `session_finalization_closes_result_lifecycle`
- `metrics_finalization_closes_result_lifecycle`
- `returned_complete_session_is_decisive`
- `timeout_result_is_incomplete`
- `error_result_is_infrastructure_failure`

Nine are general over the finite model's setup, state, event, or trace under
their stated premises. The remaining nine are checked named traces or
fixed boundary cases; they are not universal theorems about all Rust
executions. The transition-preservation proof uses a local 100,000-heartbeat
limit and focused list-membership lemmas rather than unbounded tactic search.

## Bounded exploration

The exhaustive executable domain uses exactly two mutant roles (`m0`, `m1`),
the single `auditSetup`, four accepted-status representatives (`killed`,
`survived`, `timeout`, `error`), and a stable 28-event alphabet. In the
transition rules, omitted accepted statuses `outOfMemory` and `processLimit`
have the same control behavior as these non-`notRun` representatives;
`notRun` cannot be accepted and is exercised through `markNotRun`. Breadth-first
exploration deduplicates exact states and checks all generated successors
through depth 5. Sessionless and resumed setups are covered by named corpus
traces and fixed theorems, not by this breadth-first state count.

Latest result:

```text
depth=5 alphabet=28 states=10708 transitions=78960 corpus_cases=9
```

Depth 9 from the initial plan was not retained: it exceeded 90 seconds in a
local run without completing. Depth 5 includes every deliberately broken
family's complete witness; the longest minimized witness has length 5. Named
corpus schedules longer than depth 5 are still evaluated directly in full.
This is a disclosed finite reduction, not an unbounded proof.

Expensive exploration, shrinking, statistics, and JSON serialization live in
`ResultLifecycleAuditMain.lean`, outside imported proof modules. Rebuilding the
changed result-lifecycle library and executable took about 2–3 seconds in the
final review run. The bounded stats and corpus check took about 2.0–2.5
seconds. Each final review invocation had an external
10- or 20-second deadline; the retained depth was not increased.

## Sensitivity witnesses

Each deliberately broken transition family is detected while the corresponding
correct trace remains safe:

```text
atomicity:     accept:m0:killed -> persist_failed:m0
uniqueness:    accept:m0:killed -> persist_ok:m0 -> record_result:m0
               -> report_ok:m0 -> report_ok:m0
boundary:      accept:m0:killed -> stop
cross_surface: accept:m0:killed -> persist_ok:m0
metrics:       stop -> mark_not_run:m0
```

These mutations cover premature/partial accounting, duplicate completion,
stop overwriting an accepted result, status divergence between surfaces, and
counting `not_run` as executed.

## Corpus inventory and correspondence

The checked-in JSONL corpus has schema 1, unique IDs, typed events/statuses,
and Lean-derived expected observations, including a Lean-rendered
`summary_counts` object independently checked against the ordered summary.
The Rust adapter uses
`deny_unknown_fields`, validates the vocabulary and setup consistency, maps
generated candidate IDs only to stable `m0`/`m1` roles, and reads statuses and
counts directly from the implementation surfaces. It does not derive expected
statuses, counts, or completeness.

| ID | Mode | Implementation surface | Result |
| --- | --- | --- | --- |
| `sessionless_complete` | `strict` | JSON report + metrics | match |
| `session_complete` | `strict` | JSON report + SQLite + metrics | match |
| `resume_reuses_determinate` | `strict` | resume, process marker, JSON, SQLite, metrics | match |
| `stop_preserves_accepted` | `strict` | total timeout after accepted result, JSON, SQLite, metrics | match |
| `metrics_write_failure` | `strict` | invalid sidecar target, warning, JSON, SQLite, exit | match |
| `session_persistence_failure` | `strict` | SQLite abort trigger, JSON, SQLite, metrics, exit | match |
| `stop_during_persist` | `internal-fixture` | machine-level persistence cancellation/deadline tests | supporting evidence |
| `duplicate_completion` | `model-only` | broken-transition sensitivity | detected boundary |
| `stop_after_summary_before_report` | `model-only` | report acknowledgement boundary | detected boundary |

Every strict observation compares independently marked accepted-completion
identities, durable and reported result triples, nonzero summary counts,
metrics execution (or corpus-owned
unobservability); stop/session/metrics/run/return flags; exit code; normalized
diagnostics; and equality of report, result, summary, baseline, metrics, and
session run IDs.

The implementation adapter invokes the public `hoimin` binary as a bounded
child process rather than a private machine helper. Each case uses an isolated
temporary Python project, controlled repository `.venv` interpreter, external
process-completion marker, independent SQLite queries, and
deserialized/validated `RunMetrics`. The marker records completion in a Python
`finally` block, retains duplicate completion identities for mismatch
detection, and does not count a process terminated during the stop fixture.
Each CLI child has a 12-second deadline; the case wrapper catches task panics
but has no competing timeout that can abort child cleanup. On
Unix the child receives `SIGINT` for bounded graceful cleanup and is then
terminated as an isolated process group if necessary; on Windows `taskkill /T`
terminates the tree. In both cases the root is explicitly reaped. A timeout
regression verifies that a spawned descendant does not survive cleanup.
Report mode also prints explicit `NotExecuted` dispositions for model-only and
internal-fixture cases; selecting either in strict mode reports its actual mode
instead of claiming that an unknown strict case was selected.

## Independent existing evidence

The following existing tests passed unchanged:

```text
cargo test -p hoimin-core --test machine \
  cancellation_during_result_persistence_reports_the_classified_result \
  -- --exact --nocapture

cargo test -p hoimin-core --test machine \
  deadline_during_result_persistence_reports_the_classified_result \
  -- --exact --nocapture

cargo test -p hoimin-cli --test run_e2e \
  sqlite_save_failure_reports_the_classification_but_leaves_no_partial_database_result \
  -- --exact --nocapture

cargo test -p hoimin-cli --test run_e2e \
  metrics_write_failure_warns_without_changing_run_result \
  -- --exact --nocapture

cargo test -p hoimin-cli --test run_e2e \
  serial_output_that_requests_stop_is_accepted_before_cancellation \
  -- --exact --nocapture
```

## Model and adapter defects removed during the audit

These were audit-artifact defects, not confirmed Hoimin bugs:

1. Resumed durable results initially lacked current-run provenance and were
   incorrectly counted as accepted/executed.
2. Missing parentheses around a `List.all fun ...` expression swallowed later
   safety clauses, producing a vacuous green check.
3. Normalization initially erased the deliberately broken atomicity signal.
4. Metrics write failure initially made the modeled run incomplete, contrary
   to the existing CLI contract.
5. Summary accounting and report acknowledgement were initially modeled as one
   atomic event, hiding their stop boundary.
6. The first transition function accepted events after return; breadth-first
   exploration produced `return_run -> mark_not_run:m0 -> mark_not_run:m1`.
7. The first duplicate-report mutation was over-broad and shrank to a trace
   that did not represent duplicate completion.
8. Metrics write failure initially projected conceptual execution count as a
   sidecar observation. The corpus now owns `metrics_observed=false` for that
   case.
9. A resumed durable identity could initially be marked `not_run` after stop;
   the model now rejects that two-event boundary and retains a fixed theorem.
10. The adapter's first independent execution projection recorded process
    start rather than accepted completion and deduplicated marker identities;
    the fixture now records completion and preserves duplicates.
11. Finalization initially did not close the lifecycle, so results could change
    after a session or metrics snapshot and an incomplete session could return
    `complete=true`; finalization is now a proved mutation barrier.
12. Unknown or noninteger public summary counts were initially filtered or
    converted into mismatches; they now produce infrastructure errors.
13. The first adapter deadline killed only the CLI root; bounded platform tree
    cleanup and explicit reaping now cover timeout descendants.
14. Single-case selection initially accepted non-strict IDs and then emitted a
    misleading empty-strict-set error; report and strict modes now expose the
    corpus disposition accurately.
15. The model initially treated timeout/OOM/process-limit/error results as
    complete and assigned status `error` exit code 4; conclusive-result guards
    and fixed timeout/error proofs now match production completion and exit
    precedence.
16. An outer case timeout could initially abort the inner process-tree cleanup;
    per-CLI bounded execution is now the sole deadline, while the panic-catching
    task wrapper waits for cleanup to finish.

This list is material evidence that the formal audit and bounded search changed
the quality of the specification rather than merely restating existing tests.

## Reproduction

Run all strict cases as a blocking test:

```bash
cargo test -p hoimin-cli --test lean_result_lifecycle_oracle -- --nocapture
```

Print classifications without making mismatches blocking:

```bash
HOIMIN_RESULT_LIFECYCLE_MODE=report \
  cargo test -p hoimin-cli --test lean_result_lifecycle_oracle \
  result_lifecycle_oracle_correspondence -- --exact --nocapture
```

Replay one exact case by adding, for example:

```bash
HOIMIN_RESULT_LIFECYCLE_CASE=session_persistence_failure
```

Rebuild and check the Lean evidence:

```bash
cd formal/HoiminOracle
lake build
lake exe generate_result_lifecycle -- --check corpus/result-lifecycle.jsonl
lake exe generate_result_lifecycle -- --stats
lake exe generate_result_lifecycle -- --sensitivity
```

## Classification and owner decisions

- Confirmed implementation bugs: none.
- Specification ambiguities remaining: none within the strict projection.
- Resolved model/adapter defects: sixteen, listed above.
- Infrastructure errors in the final strict run: none.
- Unresolved witnesses: none.
- Production repair: not applicable; the branch intentionally remains
  audit-only.
- Recommended next audit: candidate spool/ranking/top-selection consistency,
  in a separate worktree and PR.
