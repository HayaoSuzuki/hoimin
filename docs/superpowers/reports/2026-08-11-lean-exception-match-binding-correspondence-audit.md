# Lean Exception and Match Binding Correspondence Audit

Date: 2026-08-12

## Result

PASS for the audited surface. The Lean model's 25 fixed schema-1 cases are
current, all 15 implementation-facing internal fixtures match the Rust
analyzer's test-only exact-site projections, and all 8 strict public cases match
the `hoimin plan` manifest. The public set contains 3 present and 5 absent
candidate expectations. No same-premise production mismatch was found, so no
production semantic correction was made.

This result is correspondence evidence over fixed cases. **Lean proves only the
reduced model described below; it does not prove the Rust or Python
implementation.** Rust correspondence tests separately establish agreement at
the exercised internal and public observation sites.

## Claim and audited surface

The durable claim is that Hoimin's known-typing-import facts obey these binding
rules at the audited Python sites:

- an exception handler's type is observed before its target is bound;
- the target is shadowed in the handler body and deleted on fallthrough,
  `break`, `continue`, and the shared `terminate` category used for both
  `return` and `raise` fixtures;
- handler joins meet the selected, cleaned path with non-selected paths while
  preserving an unrelated supported `Mapping` import;
- partial pattern failure and false-guard state reach the following case;
- refutable unmatched state participates in the final match join, while an
  irrefutable case removes that unmatched path; and
- unrelated supported imports survive ordered match-case propagation.

The corpus has 25 fixed, human-readable rows, 13 in the `handler` family and 12
in `match-case`:

| Mode | Observation | Rows | Correspondence conclusion |
|---|---|---:|---|
| `internal-fixture` | 10 annotation facts and 5 exit facts | 15 | All matched the owned `#[cfg(test)]` exact-site Rust projection. |
| `model-only` | Exact `unknown` / `shadowed` resolution | 2 | Lean-only; deliberately not compared with Rust. |
| `strict` | Public `type_list_sequence` candidates | 8 | All matched `hoimin plan` for presence, operator, original, replacement, and symbol. |

The two model-only cases are `handler_nonselected_join` and
`match_partial_failure_next_case`. Rust's production `KnownImports` state does
not expose the model's exact `unknown` versus `shadowed` distinction, so
claiming internal or public correspondence for those labels would compare
different observations. Separately derived observable fixtures cover the same
handler-join and failed-pattern transitions without inventing test-only
provenance.

The public harness writes one source into an isolated temporary fixture project,
executes `hoimin plan` with one analyzer job, parses `PlanManifest`, and retains
all candidates whose byte span overlaps the unique marker. Zero or one retained
candidate is normalized for exact comparison. More than one overlapping
candidate is a semantic mismatch because the normalized count cannot equal the
0/1 expectation; it is not reported as infrastructure. Nonzero command exits,
timeouts, stderr, malformed JSON/manifests, invalid candidate spans, and missing
or duplicate markers are infrastructure errors and make no semantic claim.

## Formal result and model boundary

`ExceptionMatchBindingProofs.lean` contains 12 universally quantified theorems
over the reduced `BindingFlow` environments and exits:

1. handler type-before-target and body-after-target ordering (2);
2. target cleanup for fallthrough, break, continue, and terminate (4);
3. unrelated-name preservation and handler meet behavior (2);
4. pattern-failure and false-guard propagation (2); and
5. irrefutable exhaustion and inclusion of refutable unmatched state (2).

Six concrete decidable witnesses distinguish deliberately broken definitions.
The corpus executable uses the same six sensitivity families, all detected:

| Sensitivity family | Deliberate defect detected |
|---|---|
| `bind-before-type` | Bind the exception target before observing the type. |
| `handler-exit-cleanup` | Leave the target on a terminate exit. |
| `handler-join-meet` | Keep the left path instead of meeting both paths. |
| `pattern-failure` | Discard the failed-pattern successor. |
| `guard-failure` | Discard the post-guard failure contribution. |
| `irrefutable-exhaustion` | Retain an unmatched path after an irrefutable case. |

This is not bounded exploration: `structured_depth=0` and
`generated_depth_expansion=false`. The result covers only the fixed semantic
equivalence cases plus the quantified theorems in the reduced two-name fact
lattice. Parsing, marker selection, JSON, files, process execution, and actual
Ruff/Rust control flow are outside Lean and are covered only by executable
tests.

## Exclusions

The audit excludes arbitrary Python exception and pattern syntax, parser
correctness, `except*` unless it follows an already exercised implementation
path, runtime exception-object lifetime, exact return-versus-raise identity
beyond the shared terminate category, arbitrary names outside the supported
`Sequence`/`Mapping` fixtures, mutation ranking and execution, concurrency,
performance, and generated or exhaustive case exploration.

The audit also does not infer semantics from infrastructure failures, compare
the two model-only resolution labels with a different Rust observation, or
claim that passing public fixtures proves unexercised analyzer behavior.

## Mismatch and correction ledger

No confirmed production defect remains and production semantics did not change.
Intermediate Reds were classified before correction:

- two exact Lean resolution labels were not represented by Rust's positive-fact
  state; they were correctly retained as `model-only`, with separate observable
  internal and strict witnesses;
- `typing.TypeAlias` was not a production-supported known type, so the two
  unrelated-name fixtures were regenerated from Lean with supported `Mapping`;
- marker selection initially admitted unreachable or over-broad compound
  ranges; only `#[cfg(test)]` exact-site projection code was narrowed and
  bounded; and
- no public candidate mismatch appeared: the final 8/8 strict cases matched on
  the first completed public normalization run.

These were model/fixture or observation-adapter corrections, not changes to the
production transfer rules.

## Corpus ownership and freshness

The JSONL is rendered by
`ExceptionMatchBindingAuditMain.lean -- --output`; it was not hand-edited. The
final guarded Lean freshness check after the supported-`Mapping` correction
returned an exact byte match. Task 4 did not rerun Lean because no formal or
generated artifact changed.

Fresh Task 4 checks found:

- SHA-256
  `ab9667b5153bed101ec615fa8913fba482fd986c9518dec0aee7da86c9add136`;
- no corpus diff from commit `53458e1`;
- 25 unique IDs and 25 markers occurring exactly once in their source;
- modes `15 internal-fixture / 2 model-only / 8 strict`;
- families `13 handler / 12 match-case`; and
- kinds `10 annotation / 5 exits / 2 resolution / 8 public-candidate`.

## Guarded Lean resource ledger

All retained Lean commands ran alone with a 20-second wall deadline, 768 MiB
(786,432 KiB) root-plus-descendant RSS limit, 250 ms sampling, and one Lake
build job. `child_exit` means the guarded child exited normally; an exit 1 on a
named RED consumer is expected TDD evidence, not a semantic or infrastructure
failure.

### Task 1 retained proof evidence

| Phase / inner command | Exit | Reason | Elapsed ms | Highest sampled RSS KiB |
|---|---:|---|---:|---:|
| Required RED: `lake env lean /tmp/hoimin-exception-match-proof-consumer.lean` | 1 | `child_exit` (module absent) | 816 | 156,480 |
| Existing baseline: `lake -Kjobs=1 build HoiminOracle.BindingFlowProofs` | 0 | `child_exit` | 2,705 | 723,840 |
| Focused proofs: `lake -Kjobs=1 build HoiminOracle.ExceptionMatchBindingProofs` | 0 | `child_exit` | 2,166 | 576,656 |
| External proof consumer: `lake env lean /tmp/hoimin-exception-match-proof-consumer.lean` | 0 | `child_exit` | 555 | 657,760 |

### Final retained Task 2 corpus evidence

These are the final guarded records after the last generated-corpus correction,
not superseded earlier fix-round runs.

| Phase / inner command | Exit | Reason | Elapsed ms | Highest sampled RSS KiB |
|---|---:|---|---:|---:|
| RED drift consumer: `lake env lean --run /private/tmp/hoimin-exception-match-task2-fix-r3-drift.lean` | 1 | `child_exit` (old unsupported fixtures) | 2,703 | 630,128 |
| Focused cases build: `lake -Kjobs=1 build HoiminOracle.ExceptionMatchBindingCases` | 0 | `child_exit` | 831 | 670,448 |
| Drift consumer GREEN: same consumer | 0 | `child_exit` | 561 | 54,144 |
| `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --cases` | 0 | `child_exit` (25/25 valid) | 558 | 645,600 |
| `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --sensitivity` | 0 | `child_exit` (6/6 detected) | 567 | 682,960 |
| `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --stats` | 0 | `child_exit` | 548 | 659,552 |
| `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --output corpus/exception-match-binding-correspondence.jsonl` | 0 | `child_exit` | 561 | 683,360 |
| `lake env lean --run ExceptionMatchBindingAuditMain.lean -- --check corpus/exception-match-binding-correspondence.jsonl` | 0 | `child_exit` (exact byte match) | 555 | 683,584 |

One optional aggregate `lake -Kjobs=1 build HoiminOracle` attempt from Task 1
was deliberately **not retained as verification evidence**. The guard stopped
it with exit 125 and `reason="rss_limit"` after 312 ms at 985,296 KiB, above
the fixed 786,432 KiB cap. It was not retried and the cap was not raised. This
is an infrastructure/resource result, not evidence against the model or
implementation. Initial sandbox-only guard invocations that could not run
`ps` were likewise setup infrastructure errors and were rerun with unchanged
limits once process-tree sampling was available.

## Executable verification

Task 3's committed private evidence covers all 15 internal rows, both
model-only exclusions, the four implementation-facing broken transitions, and
unreachable/header marker bounds. Task 4's public integration test covers the
closed schema, malformed/nonzero infrastructure classification, overlapping
candidate semantics, and all 8 strict public rows.

The final commands are:

```bash
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_exception_match_binding_oracle
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_annotation_scope_oracle
CARGO_BUILD_JOBS=2 cargo test -p hoimin-cli --test lean_binding_flow_oracle
```

Fresh final results were 6/6 tests in the exception/match harness, 4/4 in the
annotation-scope harness, and 4/4 in the binding-flow harness. Focused Clippy
with `-D warnings`, `cargo fmt --all -- --check`, and the staged diff check also
exited 0 without warnings or formatting errors.
