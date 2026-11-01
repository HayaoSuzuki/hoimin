# Lean `except*` Binding-Flow Audit

Date: 2026-08-14
Issue: #300, phase 5
Status: complete

## Claim and boundary

The audit checks this contract:

> An `except*` statement carries an unhandled subgroup through its ordered
> sibling handlers and delays propagation of handler-raised exceptions until
> every applicable sibling has run. Hoimin's conservative known-import summary
> retains a fact only when the fact survives each matching-subset route that
> the analyzer represents.

Lean models exact representative routes and a collapsed must-known summary.
Rust correspondence covers the collapsed summary because
`AnnotationCollector` does not store exception-group values, subgroup
identity, or a concrete runtime match set.

The model includes ordered handlers, matched and remaining subgroup premises,
handler fallthrough and raise, delayed terminate propagation, unhandled
remainder, handler-target cleanup, and the two-name known-import meet.

The model excludes exception class hierarchies, runtime matching, exception
values, traceback shape, `sys.exception()` lifetime, parser correctness, and
the phase 1 through phase 4 contracts. Python rejects `return`, `break`, and
`continue` inside `except*`, so this phase does not model those handler
exits.

Python specifies that several starred clauses can run for one exception group
and that the interpreter merges unhandled subgroups with exceptions raised by
the handlers after clause processing:

- https://docs.python.org/3/reference/compound_stmts.html#except-star
- https://peps.python.org/pep-0654/#except

## Declared and implicit production behavior

Ruff parses ordinary and starred statements into `StmtTry` and exposes
`is_star`. Literal `except*` source therefore configures a production AST
premise.

`AnnotationCollector::visit_try` currently uses one handler transfer for both
forms. It evaluates each syntactic handler from the same conservative handler
entry, merges handler exits, retains the try-body terminate states, and applies
`finally`. This behavior does not identify which subgroup matched a handler.

`binding_flow_try_exit_snapshot` runs the production collector and returns
the complete normalized fallthrough, break, continue, and terminate state
vectors for one marker-selected try statement. The public `plan` path can
observe a complete mutation candidate after a starred statement.

The audit treats an exact two-sibling runtime split as model-only. Internal and
strict rows compare the conservative summary over unknown matching subsets.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public or internal observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Starred syntax | starred route | Ruff `StmtTry::is_star` from literal source | Selected try snapshot | Ruff AST and parser-backed source | `internal-fixture` |
| Conservative starred summary | meet over all matching subsets | Literal starred handlers with unknown runtime subgroup identity | Complete try-exit snapshot | Production `visit_try` projection | `internal-fixture` |
| Two matching siblings in order | two selected route steps | Production cannot retain this concrete split | No same-premise observation | Python execution contract | `model-only` |
| Raise followed by a matching sibling | pending raise plus advancing remainder | Production cannot identify the subgroup reaching the sibling | No same-premise observation | Python execution contract | `model-only` |
| Handler target cleanup | cleanup on selected outcomes | `except* Error as Sequence` | Complete try-exit snapshot | Production cleanup path | `internal-fixture` |
| Definite post-statement import | collapsed fallthrough meet | Starred statement followed by marked annotation | Complete overlapping candidate | Public `plan` path | `strict` |
| Shadowed post-statement import | collapsed fallthrough meet removes fact | One syntactic starred handler shadows the name | No overlapping candidate | Public `plan` path | `strict` |
| Exact exception-group tree | abstract remainder token | Production stores no exception value | No observation | Excluded runtime state | `model-only` |

## Formal model and proof result

The imported model defines fact actions, subgroup splits, exact route steps,
final exit categorization, and conservative collapse. The proof module
establishes sibling continuation after a raised handler, one execution per
selected handler, remainder-only advancement, cleanup on fallthrough and
delayed terminate, final remainder propagation, the reachable-state meet
property, and the two-handler collapse theorem.

The collapse theorem covers two handlers, two names, and factwise keep,
invalidate, or restore-to-`typing` actions. It does not claim equivalence for
arbitrary Python expressions.

The executable owns eight fixed rows: four internal fixtures, two strict public
rows, and two model-only exact routes. Seven broken variants detect stopping
after the first match, dropping a sibling after raise, losing or duplicating a
remainder, eager raise propagation, omitted target cleanup, and exclusive
ordinary-handler collapse. The executable reports zero generated depth, states,
transitions, and event alphabet.

## Classification ledger

| Case | Mode | Expected | Actual | Classification | Decision |
| --- | --- | --- | --- | --- | --- |
| `starred_summary_preserves_common` | `internal-fixture` | complete try exits | exact match | no mismatch | retain regression |
| `starred_summary_meets_disagreement` | `internal-fixture` | complete try exits | exact match | no mismatch | retain regression |
| `starred_target_cleanup` | `internal-fixture` | complete try exits | exact match | no mismatch | retain regression |
| `starred_unhandled_remainder` | `internal-fixture` | complete try exits | exact match | no mismatch | retain regression |
| `two_matching_siblings_exact_route` | `model-only` | exact route | no production comparison | model-only boundary | retain outside correspondence |
| `raised_handler_allows_later_sibling` | `model-only` | exact route | no production comparison | model-only boundary | retain outside correspondence |
| `starred_public_candidate_present` | `strict` | complete candidate | exact match | no mismatch | retain regression |
| `starred_public_candidate_absent` | `strict` | no overlapping candidate | exact match | no mismatch | retain regression |

## Resource ledger

Lean commands use a 20-second deadline, 786,432 KiB root-plus-descendant RSS
cap, 250 ms sampling, and one process at a time. Measurements record the
inner command, exit, stop reason, elapsed milliseconds, and peak RSS KiB.

Initial and authoring measurements:

| Command | Exit / reason | Elapsed ms | Peak RSS KiB |
| --- | --- | ---: | ---: |
| Existing `MultipleHandlerJoinProofs` baseline build | 0 / `child_exit` | 3,217 | 703,680 |
| Proof consumer RED after baseline build | 1 / missing `ExceptStarFlowProofs.olean` | 281 | 2,832 |
| Model build | 0 / `child_exit` | 1,895 | 618,592 |
| First proof build | 1 / false fallthrough premise and proof-shape errors | 553 | 623,824 |
| Second proof build | 1 / missing constructor unfolding | 830 | 615,952 |
| Corrected proof build | 0 / `child_exit` | 563 | 654,768 |
| Proof consumer GREEN | 0 / `child_exit` | 550 | 502,224 |

Fresh Task 2 checks after import-boundary cleanup used 1,101 ms / 538,864 KiB
for the model, 558 ms / 680,752 KiB for proofs, and 553 ms / 54,544 KiB for the
proof consumer.

The first proof attempt exposed a missing `pendingRaised = false` premise in
the fallthrough theorem. A prior pending raise makes the final route terminate,
so the proof now states that premise. The other corrections added finite
`Target` cases and unfolded the exit constructor. No route transition or
collapse claim changed.

The first attempted RED ran before any baseline `.olean` existed and failed
on the root `HoiminOracle` prefix. The audit rejected that result, built an
existing module, and retained the specific missing-object RED shown above.

Corpus authoring measurements:

| Command | Exit / reason | Elapsed ms | Peak RSS KiB |
| --- | --- | ---: | ---: |
| Generator RED | 1 / missing `ExceptStarFlowAuditMain.lean` | 289 | 2,912 |
| Cases build | 0 / `child_exit` | 1,636 | 694,352 |
| Eight fixed cases | 0 / `child_exit` | 549 | 634,016 |
| Seven sensitivity families | 0 / `child_exit` | 566 | 629,152 |
| Statistics | 0 / `child_exit` | 2,977 | 667,456 |
| Corpus output | 0 / `child_exit` | 572 | 686,384 |
| Corpus freshness | 0 / `child_exit` | 571 | 683,024 |

Final focused measurements used cached prerequisites where available:

| Command | Exit / reason | Elapsed ms | Peak RSS KiB |
| --- | --- | ---: | ---: |
| Model build | 0 / `child_exit` | 545 | 56,288 |
| Proof build | 0 / `child_exit` | 314 | 2,848 |
| Proof consumer | 0 / `child_exit` | 2,442 | 612,384 |
| Cases rebuild after review | 0 / `child_exit` | 2,985 | 712,736 |
| Seven sensitivity families after review | 0 / `child_exit` | 558 | 666,416 |
| Corpus output after review | 0 / `child_exit` | 559 | 678,656 |
| Corpus freshness after review | 0 / `child_exit` | 569 | 686,048 |

Every final command exited through `child_exit`; no timeout, RSS stop, or
monitor error occurred. The generated corpus SHA-256 is
`3012d7e81bf7e44d1c21949dc8b554905b2809c6c3f2b579afa39ce6b4d49d2f`.

## Verification ledger

The isolated worktree baseline passed:

```text
cargo build --workspace
cargo test -p hoimin-cli --lib multiple_handler_join_oracle_tests --no-fail-fast
```

The internal oracle passed three tests: closed schema and source ownership,
four complete production snapshots, and three production-local mutation
checks. The mutations detect keeping only the first handler, omitting starred
target cleanup, and dropping the explicit body terminate path. Existing
multiple-handler tests passed 4/4 and nested-try tests passed 3/3.

No internal same-premise mismatch exists, so this phase does not change Rust
production semantics. The first formatting check reported one rustfmt layout
difference in the new test module; `cargo fmt --all` applied that mechanical
change and the next formatting check passed.

The public adapter passed both observation tests. Its closed-parser regression
also rejects a duplicate strict identity; that test first failed against the
count-only check and passed after the adapter required the exact two-ID set.
The present row produced one exact `target.py` candidate at byte 166 with
length 13, operator `type_list_sequence`, original `Sequence[int]`, replacement
`list[int]`, and no symbol. The absent row produced no overlapping candidate.
Adjacent multiple-handler and nested-try public suites passed 2/2 each.

The first nested-try command used the nonexistent target
`lean_nested_try_oracle`. Cargo listed `lean_nested_try_flow_oracle`; the
plan now uses that target, and the corrected command passed. This was a plan
authoring defect rather than an infrastructure or semantic result.

Local review also found two audit-quality defects. The design described an
internal deduplicating frontier that the reduced model does not implement, so
the document now describes explicit representative routes. The eager-raise
sensitivity check asserted only a correct-state property; it now compares the
early-finalized route directly with the route after the later sibling. The
revised seven-family sensitivity run and corpus freshness check passed.

Final local verification passed:

```text
cargo test -p hoimin-cli --lib except_star_flow_oracle_tests --no-fail-fast
cargo test -p hoimin-cli --test lean_except_star_flow_oracle --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -j 2 -- -D warnings
cargo test --workspace --all-features -j 2
git diff --check origin/main...HEAD
git diff --check
```

Clippy completed with warnings denied. The full workspace test completed with
no failures, and both whitespace checks passed. The final Lean output matched
the checked-in corpus byte for byte and the independent freshness check passed.

## Production decision and limitations

The four internal rows and two strict rows found no same-premise mismatch.
Rust production behavior is therefore unchanged; Rust changes are limited to
test-module registration and the internal and public regression adapters.

Lean proves the reduced two-name, two-handler model under explicit subgroup
split premises and factwise keep, invalidate, or restore-to-`typing` actions.
Exact exception-group trees, exception identity, runtime subclass matching,
tracebacks, and arbitrary Python expressions remain outside the correspondence
claim. The two exact sibling-route rows remain `model-only` because the
production analyzer has no state that can configure or observe those runtime
subgroups.

GitHub Actions run 31788965160 passed every executed job: Quality on macOS,
Ubuntu, and Windows; Rust on macOS, Ubuntu, and Windows; Rust 1.88 MSRV;
randomized-order Rust; wheel smoke on all three platforms; contracts; both
core-dependency purity jobs; and Linux best-effort. The environment-gated
Linux cgroup-v2-hard job was skipped as configured. No CI repair was needed.
