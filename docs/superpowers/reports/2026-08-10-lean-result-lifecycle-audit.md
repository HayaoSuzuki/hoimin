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

Within the declared two-mutant, seven-status model, every transition accepted
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
also rejected. `accept notRun` is invalid; `markNotRun` applies only after stop
and only when no accepted real result exists. In a session run, recording a
current result requires either a matching durable row, a persistence failure,
or stop already taking precedence.

## Kernel-checked results

The following theorem bodies contain no `sorry`, `admit`, or custom axioms:

- `initial_invariant`
- `acceptState_invariant`
- `rejected_preserves_state`
- `step_preserves_invariant`
- `runWith_preserves_invariant`
- `run_preserves_invariant`
- `stopped_not_run_does_not_increment_metrics`
- `accepted_status_is_stable`
- `complete_report_summary_corresponds`

The first six are general over the finite model's setup, state, event, or
trace under their stated premises. The last three are checked named traces;
they are not universal theorems about all Rust executions. Structural
properties beyond the small inductive `Invariant` are checked by executable
`safe` over named and bounded states.

## Bounded exploration

The exhaustive executable domain uses exactly two mutant roles (`m0`, `m1`),
seven statuses, and a stable 28-event alphabet. Breadth-first exploration
deduplicates exact states and checks all generated successors through depth 5.

Latest result:

```text
depth=5 alphabet=28 states=29196 transitions=175896 corpus_cases=9
```

Depth 9 from the initial plan was not retained: it exceeded 90 seconds in a
local run without completing. Depth 5 includes every deliberately broken
family's complete witness; the longest minimized witness has length 5. Named
corpus schedules longer than depth 5 are still evaluated directly in full.
This is a disclosed finite reduction, not an unbounded proof.

Expensive exploration, shrinking, statistics, and JSON serialization live in
`ResultLifecycleAuditMain.lean`, outside imported proof modules. A cached
`lake build` took 0.38 seconds; rebuilding the changed result-lifecycle modules
during development took about 2.55 seconds. The bounded stats command took
12.62 seconds. Corpus generation/check took approximately 12–13 seconds per
invocation on this machine.

## Sensitivity witnesses

Each deliberately broken transition family is detected while the corresponding
correct trace remains safe:

```text
atomicity:     accept:m0:killed -> persist_failed:m0
uniqueness:    accept:m0:killed -> persist_ok:m0 -> record_result:m0
               -> report_ok:m0 -> report_ok:m0
boundary:      accept:m0:killed -> stop
cross_surface: accept:m0:killed -> persist_ok:m0
metrics:       mark_not_run:m0
```

These mutations cover premature/partial accounting, duplicate completion,
stop overwriting an accepted result, status divergence between surfaces, and
counting `not_run` as executed.

## Corpus inventory and correspondence

The checked-in JSONL corpus has schema 1, unique IDs, typed events/statuses,
and Lean-derived expected observations. The Rust adapter uses
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

Every strict observation compares accepted, durable, and reported result
triples; nonzero summary counts; metrics execution (or corpus-owned
unobservability); stop/session/metrics/run/return flags; exit code; normalized
diagnostics; and equality of report, result, summary, baseline, metrics, and
session run IDs.

The implementation adapter invokes the public `hoimin_cli::run_with_io`
entrypoint rather than a private machine helper. Each case uses an isolated
temporary Python project, controlled repository `.venv` interpreter, external
execution marker, independent SQLite queries, and deserialized/validated
`RunMetrics`.

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
- Resolved model/adapter defects: eight, listed above.
- Infrastructure errors in the final strict run: none.
- Unresolved witnesses: none.
- Production repair: not applicable; the branch intentionally remains
  audit-only.
- Recommended next audit: candidate spool/ranking/top-selection consistency,
  in a separate worktree and PR.
