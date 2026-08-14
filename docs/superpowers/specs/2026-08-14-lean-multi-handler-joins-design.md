# Lean Multiple Handler Join Audit Design

Date: 2026-08-14
Issue: #300, phase 3

## Outcome and boundary

Audit this phase-specific claim:

> Multiple ordinary `except` handlers form an ordered selection frontier. Each
> reachable selected handler contributes all of its categorized exits after
> that handler's target cleanup has been applied exactly once; its non-selected
> remainder alone advances to the next handler. The final join meets exactly
> the reachable fallthrough paths, while break, continue, terminate, and the
> unhandled remainder retain their categories.

This phase generalizes the existing single-handler correspondence into a
compositional handler fold. It reuses the categorized-exit and `finally`
semantics established by phases 1 and 2, but does not re-audit those rules.

The model does not implement Python exception class matching. Handler
selection and non-selection are explicit premises, matching the analyzer's
conservative treatment of every syntactically reachable ordinary handler as a
possible path.

## Included behavior

- two and three ordinary `except` handlers in source order;
- a reachable selected exit and a non-selected remainder at each handler;
- handler-target cleanup applied independently to every selected exit;
- fallthrough, break, continue, and terminate category preservation;
- the meet of all and only reachable fallthrough handler outcomes;
- exclusion of a handler whose selection premise is unreachable;
- preservation of an unhandled final remainder as a terminate exit;
- one unrelated supported typing import to detect over-broad cleanup; and
- exact correspondence through production-backed internal projections and
  complete public candidate observations.

## Excluded behavior

- runtime exception subclass matching and exception object lifetime;
- arbitrary handler-type expressions or their runtime side effects;
- `except*` and exception-group remainder splitting, reserved for phase 5;
- nested `match`, loop consumption, and `finally` behavior already audited in
  phases 1 and 2;
- OR, AS, mapping, class patterns, and guard-failure propagation, reserved for
  phase 4;
- parser correctness, mutation execution, ranking, and arbitrary-name scaling;
- exhaustive syntax-tree or handler-combination generation.

## Approach selection

Three approaches were considered:

1. An ordered reachable-handler fold. This is selected because it directly
   expresses selected and non-selected paths, supports an unbounded theorem
   over handler lists, and maps to `AnnotationCollector::visit_try` without
   pretending to know runtime exception types.
2. A Python exception-type matcher. This would add class-hierarchy premises
   that production neither configures nor observes and would turn most rows
   into `model-only` evidence.
3. Fixed enumeration of handler combinations. This would exercise examples
   but would not establish the durable list-fold law requested by the issue.

## Semantic model

Add a focused Lean module over `BindingFlow.Exits` and
`NestedTryFlow.cleanupExits`.

`HandlerStep` records:

- `selected : Option Exits`, where `none` means that handler is unreachable as
  a selected branch;
- `remainder : Option Env`, where `none` means no non-selected path continues;
  and
- `target : Option Name`, applied only to that handler's selected exits.

`routeHandlers` folds the ordered steps from an initial remainder. A step is
reachable only when an incoming remainder exists. For a reachable step it:

1. cleans the selected exits with that step's target exactly once;
2. merges those exits into the accumulated result; and
3. passes only the step's non-selected remainder to the next step.

The fold returns both accumulated selected exits and the final remainder.
`finishHandlers` converts a surviving final remainder to a terminate exit and
merges it with the selected exits. It computes no meet itself;
`BindingFlow.Exits.merge` and `outgoingEnv` remain the shared join operations.

The model deliberately represents reachability rather than exception types.
This makes the premises explicit and prevents a different-premise public
comparison from being called strict.

## Proof obligations

Imported Lean proof modules will establish, with explicit premises:

- appending one reachable handler merges exactly its cleaned selected exits;
- a missing incoming remainder makes all later handlers unreachable;
- each selected handler target is absent from every one of its exit categories;
- unrelated facts survive cleanup;
- handler order affects remainder reachability but not the category assigned to
  an already selected exit;
- all reachable fallthrough results participate in the final meet;
- no unreachable fallthrough result participates in that meet;
- break, continue, and terminate states remain in their original categories;
  and
- a final unhandled remainder appears exactly once as terminate.

Non-trivial declarations use a local finite `maxHeartbeats`; no imported
module may contain `native_decide`, corpus serialization, `sorry`, `admit`, a
custom axiom, or unbounded heartbeats.

## Fixed cases and correspondence worksheet

The Lean-owned JSONL corpus will remain intentionally small. Planned semantic
rows are:

| Boundary | Mode | Production premise and observation |
|---|---|---|
| Two reachable fallthrough handlers disagree on `Sequence` | `internal-fixture` | Full post-try known-import snapshot from the real collector |
| Three reachable handlers agree on unrelated `Mapping` | `internal-fixture` | Full post-try known-import snapshot |
| Per-target cleanup across fallthrough and terminate exits | `internal-fixture` | Full categorized try-exit snapshot |
| Break and continue from different handlers | `internal-fixture` | Full categorized try-exit snapshot inside a loop fixture |
| Final unhandled remainder | `internal-fixture` | Full terminate snapshot from a production-backed test seam |
| All reachable handlers retain `Sequence` | `strict` | Complete overlapping public CLI candidate record |
| One reachable handler shadows `Sequence` | `strict` | Complete public CLI observation with zero overlapping candidates |

Rows may be consolidated when one fixture exposes multiple complete exit
categories without weakening diagnostics. An exact premise that production
cannot configure or expose starts as `model-only`; it is promoted only after a
same-premise adapter exists. Parse, marker, process, schema, timeout, and
capture failures are `infrastructure-error` rather than mismatches.

The worksheet must name the Rust observation function and source marker for
every implementation-facing row before the row is generated. Public rows
compare candidate count, path, byte span, operator, original, replacement, and
symbol, not presence alone.

## Sensitivity families

Fixed broken variants must be rejected for all applicable risks:

1. keep only the first selected handler;
2. keep only the last selected handler;
3. include a selected exit after its incoming remainder became unreachable;
4. omit one handler's target cleanup;
5. flatten a selected break, continue, or terminate exit into fallthrough; and
6. drop or duplicate the final unhandled remainder.

The first and last variants jointly satisfy the issue's “keep only the
selected handler instead of meeting selected and non-selected paths” family
without adding an exception-type matcher. If a listed family cannot be made
same-premise comparable with Rust, its Lean witness remains valid but its
correspondence mode and limitation are stated explicitly.

## Rust correspondence and repair rule

The internal adapter will call the production `AnnotationCollector` and read
test-only snapshots captured at handler entry or the completed try exit. It may
select an AST node by a unique marker and normalize stable known-import facts;
it may not reproduce handler routing, cleanup, or meet logic.

The existing `visit_try` implementation is not changed merely to expose a
preferred model. For every difference:

1. rerun one corpus row;
2. record the smallest source, expected and actual complete observations, and
   differing fields;
3. classify it as confirmed bug, specification ambiguity, model defect, or
   infrastructure error; and
4. only for a confirmed production bug, retain the Lean row as a failing Rust
   regression before applying the smallest semantic fix.

Test-only observation or mutation seams are allowed when they directly expose
production state and are compiled only under `cfg(test)`.

## Resource safety

All Lean and Lake commands run serially through
`formal/HoiminOracle/tools/lean_resource_guard.py` with:

- a 20-second wall-clock deadline;
- a 768 MiB root-plus-descendant RSS limit;
- 250 ms sampling; and
- `lake -Kjobs=1` for focused builds.

The audit uses an unbounded list theorem plus fixed witnesses, not bounded
generation. Reported generated depth, event alphabet, explored states, and
transitions are therefore zero. A timeout, RSS stop, or monitor failure is an
infrastructure result and must not cause higher resource limits.

## Test-driven workflow

1. Create a failing proof consumer naming the handler-fold obligations.
2. Add the minimal model and kernel-checked proofs.
3. Add fixed cases and broken variants, then generate the corpus from Lean.
4. Add closed-schema Rust tests and observe their initial failure.
5. Add the narrow production-backed adapter and classify every difference.
6. Add strict public CLI correspondence cases.
7. Run focused Lean, sensitivity, freshness, Rust, formatting, lint, and
   workspace verification.
8. Write a self-contained report, obtain review, create a PR, require all CI,
   squash-merge, update issue #300, and remove only this phase's worktree.

## Completion criteria

- The correspondence worksheet precedes modeling and assigns every row exactly
  one allowed mode.
- Imported Lean modules prove the ordered reachable-handler fold contract.
- Every applicable sensitivity family detects its deliberate defect.
- The deterministic Lean-owned corpus is schema-validated and freshness
  checked without hand editing.
- Every strict and internal row compares complete same-premise observations.
- Every mismatch is classified before correction.
- No production semantic change occurs without a focused failing regression.
- The final report separates Lean-model results from Rust observations and
  records exact commands, measurements, exclusions, and mismatch decisions.
- Focused tests, formatting, Clippy, relevant workspace tests, and all PR CI
  checks pass before merge.
