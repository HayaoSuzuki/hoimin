# Lean `except*` Binding-Flow Audit

Date: 2026-08-14
Issue: #300, phase 5
Status: in progress

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

The executable will own eight fixed rows and seven sensitivity families. It
will report zero generated depth, states, transitions, and event alphabet.

## Classification ledger

| Case | Mode | Expected | Actual | Classification | Decision |
| --- | --- | --- | --- | --- | --- |
| `starred_summary_preserves_common` | `internal-fixture` | complete try exits | not run | not run | wait for same-premise execution |
| `starred_summary_meets_disagreement` | `internal-fixture` | complete try exits | not run | not run | wait for same-premise execution |
| `starred_target_cleanup` | `internal-fixture` | complete try exits | not run | not run | wait for same-premise execution |
| `starred_unhandled_remainder` | `internal-fixture` | complete try exits | not run | not run | wait for same-premise execution |
| `two_matching_siblings_exact_route` | `model-only` | exact route | no production comparison | not run | retain outside correspondence |
| `raised_handler_allows_later_sibling` | `model-only` | exact route | no production comparison | not run | retain outside correspondence |
| `starred_public_candidate_present` | `strict` | complete candidate | not run | not run | wait for public execution |
| `starred_public_candidate_absent` | `strict` | no overlapping candidate | not run | not run | wait for public execution |

## Resource ledger

Lean commands use a 20-second deadline, 786,432 KiB root-plus-descendant RSS
cap, 250 ms sampling, and one process at a time. Measurements will record the
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

The first proof attempt exposed a missing `pendingRaised = false` premise in
the fallthrough theorem. A prior pending raise makes the final route terminate,
so the proof now states that premise. The other corrections added finite
`Target` cases and unfolded the exit constructor. No route transition or
collapse claim changed.

The first attempted RED ran before any baseline `.olean` existed and failed
on the root `HoiminOracle` prefix. The audit rejected that result, built an
existing module, and retained the specific missing-object RED shown above.

## Verification ledger

The isolated worktree baseline passed:

```text
cargo build --workspace
cargo test -p hoimin-cli --lib multiple_handler_join_oracle_tests --no-fail-fast
```

The focused oracle, public adapter, workspace test, Clippy, formatting, corpus
freshness, and CI results have not run.
