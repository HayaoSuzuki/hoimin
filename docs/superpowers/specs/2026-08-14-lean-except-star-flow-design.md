# Lean `except*` Binding-Flow Audit Design

Date: 2026-08-14
Issue: #300, phase 5

## Outcome and claim

Audit this phase-specific claim:

> An `except*` statement carries each unhandled subgroup to the next handler,
> permits more than one matching sibling handler to run, and waits until all
> siblings have been considered before propagating handler-raised and unhandled
> exceptions. Hoimin's conservative summary must retain a known import only
> when it survives every matching-subset route that the analyzer represents.

The audit projects Python exception groups onto Hoimin's existing
`KnownImports` lattice and `ControlFlowExits` categories. Lean proves the
reduced transfer. Rust tests establish correspondence at the exercised AST and
CLI boundaries.

## Why this slice

Ruff exposes ordinary `try` and starred `try` through `StmtTry::is_star`.
`AnnotationCollector::visit_try` currently receives that flag but applies the
ordinary-handler transfer to both forms. The internal try-exit projection can
select a starred statement by source marker and return the production
fallthrough and terminate states. The public `plan` path can observe whether a
typing import remains definite after the statement. The analyzer does not
retain exception-group values or the runtime split chosen by each clause, so an
exact two-sibling route cannot qualify as implementation correspondence.

Python's language reference requires ordered subgroup splitting. One exception
group can run several `except*` clauses, and the interpreter merges unhandled
subgroups with exceptions raised by handler bodies after it has considered all
clauses. Python rejects `return`, `break`, and `continue` inside an `except*`
clause. The reduced model therefore audits fallthrough and terminate paths and
reuses the existing `finally` transfer.

References:

- https://docs.python.org/3/reference/compound_stmts.html#except-star
- https://peps.python.org/pep-0654/#except

## Considered approaches

### Selected: reduced subgroup frontier

Represent a finite frontier of possible subgroup routes. At each handler, an
unmatched route advances without running the body. A matched route runs the
handler once, cleans its target, records exceptions raised by that body, and
advances its remaining subgroup to the next sibling. The model joins duplicate
environments after each step so the frontier remains small for the fixed
two-handler domain.

Prove a separate collapse theorem for the analyzer's factwise keep, invalidate,
and restore actions. The theorem compares the meet of all matching subsets with
the conservative incoming-plus-single-handler summary. This approach exercises
the semantic difference between `except` and `except*` while staying within
Hoimin's must-known fact lattice.

### Rejected: reuse the ordinary-handler model

Treating handlers as exclusive alternatives would restate the current Rust
transfer. It cannot detect a handler-raised path that still requires a later
sibling to run.

### Rejected: model exception-group trees and subclass matching

Tree shape, traceback metadata, and runtime subclass matching do not affect the
two representative known-import facts. A full interpreter model would increase
proof and search cost without improving the implementation decision.

## Model boundary

The model includes:

- ordered starred handlers;
- explicit match and non-match premises for a representative subgroup;
- two siblings executing for one incoming exception group;
- handler fallthrough and raise outcomes;
- delayed propagation of handler-raised exceptions;
- an unhandled final remainder;
- target cleanup on each handler outcome;
- fallthrough and terminate environment meets; and
- the existing `finally` routing contract.

The model excludes:

- exception class hierarchies and runtime matching;
- nested exception-group shape and traceback identity;
- exception values and `sys.exception()` lifetime;
- forbidden `return`, `break`, and `continue` statements in starred handlers;
- parser correctness;
- ordinary-handler rules already covered by phase 3; and
- generated syntax or deep trace exploration.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Statement uses starred handlers | `starred := true` route | Ruff `StmtTry::is_star` from literal `except*` source | Internal selected try snapshot | Ruff AST definition and parser-backed fixture | `internal-fixture` |
| Conservative starred summary | meet over all matching subsets | Literal `except*` syntax; runtime subgroup identity remains unknown | Complete post-try exit snapshot | Production `visit_try` projection | `internal-fixture` |
| Two sibling subgroups match in order | two selected `StarStep` values sharing one route | Analyzer cannot configure or retain this runtime split | No same-premise production observation | Python execution model only | `model-only` |
| First handler raises and second sibling still runs | delayed terminate plus advancing remainder | Analyzer cannot identify the subgroup that reaches the second handler | No same-premise production observation | Python execution model only | `model-only` |
| Handler target is deleted | `cleanupExits target` | `except* Error as Sequence` | Complete handler/try exits | Existing production cleanup plus selected try projection | `internal-fixture` |
| Definite import after all reachable starred paths | fallthrough meet | Literal starred statement followed by one marked annotation | Complete candidate record from `plan` | Public CLI adapter | `strict` |
| One starred path shadows the import | fallthrough meet removes the fact | Literal starred statement with one shadowing handler | No overlapping candidate | Public CLI adapter | `strict` |
| Exact subgroup tree and exception identity | abstract remainder token | Production analyzer does not store exception-group values | No same-premise observation | Python runtime detail outside `KnownImports` | `model-only` |

Rows start in `model-only`. A row moves to `internal-fixture` or `strict` only
after its source premise and complete observation pass the closed adapter.

## Lean structure

Add three imported modules and one executable:

- `ExceptStarFlowModel.lean` defines `StarStep`, route state, delayed raised
  states, remainder advancement, target cleanup, and finalization.
- `ExceptStarFlowProofs.lean` proves the ordered sibling and delayed-propagation
  invariants with explicit premises.
- `ExceptStarFlowCases.lean` owns the closed fixed corpus and broken variants.
- `ExceptStarFlowAuditMain.lean` validates cases, sensitivity, statistics,
  serialization, and freshness without entering the imported library graph.

The route state separates:

- the current unhandled subgroup environment, if one remains;
- fallthrough environments after handled routes;
- exceptions raised by handler bodies; and
- the final unhandled remainder.

`routeStarHandler` advances both selected and unselected possibilities. A
selected handler's fallthrough continues to the next sibling with its updated
environment. Its terminate outcome enters a delayed list while the sibling
frontier continues. `finishStarHandlers` merges delayed raises and the final
remainder into terminate exits. The fallthrough meet includes the normal
try/else route and fully handled starred routes.

`collapseStarSummary` maps the exact frontier to the two-name must-known
summary used for correspondence. The collapse theorem assumes each handler
applies one factwise action per tracked name: keep, invalidate, or restore a
fixed known import. These actions cover the production fixtures without
claiming equivalence for arbitrary Python expressions.

The proof surface covers:

1. a reachable later sibling remains reachable after an earlier handler raises;
2. each selected handler executes at most once per route;
3. only the remainder advances to the next matcher;
4. target cleanup reaches fallthrough and delayed terminate outcomes;
5. finalization keeps one representative unhandled remainder;
6. the outgoing meet contains a fact only when each reachable state contains it;
   and
7. the conservative summary equals the exact subset meet under the stated
   factwise-action premises.

## Fixed cases and sensitivity

Keep the corpus between seven and ten rows:

- two matching siblings preserve a common import;
- sibling disagreement removes a fact from the outgoing meet;
- model-only witnesses for two selected siblings and for a later sibling after
  an earlier handler raises;
- target cleanup affects fallthrough and raised outcomes;
- an unmatched remainder becomes a terminate path;
- one model-only exact-remainder witness;
- one strict present public candidate; and
- one strict absent public candidate.

Applicable broken variants must:

- stop after the first matching handler;
- drop the sibling after a handler raise;
- duplicate or lose the final remainder;
- propagate a handler raise before considering later siblings;
- omit target cleanup on fallthrough or terminate; and
- apply ordinary exclusive-handler routing to a starred statement.

Atomicity applies to delayed raise and sibling completion. Boundary/precedence
applies to ordered remainder consumption. Idempotency does not apply because
the analyzer stores no durable event identity; the report records that reason.

## Rust correspondence and repair rule

Add a closed internal corpus parser and reuse the production-backed selected
try projection. Add a public integration adapter that runs `hoimin plan` with
only `type_list_sequence`, rejects malformed corpus fields, and compares full
candidate records.

Test mutations branch inside `visit_try`; the adapter does not reimplement the
correct transfer. Mutations cover dropped siblings, eager raised propagation,
lost or duplicated remainder, and missed target cleanup.

If a retained same-premise collapsed-summary row fails, keep it red and change
the smallest `is_star` branch in `visit_try`. The ordinary path remains
unchanged. Do not change production code to satisfy an exact-route model-only
witness, and do not add runtime exception matching or a public API.

## Resource limits

Run Lean commands one at a time through
`formal/HoiminOracle/tools/lean_resource_guard.py` with:

- 20-second wall-clock deadline;
- 786,432 KiB root-plus-descendant RSS cap;
- 250 ms sampling; and
- `lake -Kjobs=1` for focused builds.

Use bounded `maxHeartbeats` on non-trivial declarations. Record elapsed time,
peak RSS, fixed-case count, sensitivity-family count, and zero generated depth.
Do not retry a timeout or RSS stop with a larger limit.

## Verification and delivery

Run focused Lean builds, proof consumer, cases, sensitivity, stats, corpus
generation, freshness check, and byte comparison. Run internal and public Rust
oracle tests before workspace tests, Clippy, formatting, and diff checks.

Create one PR that references #300 and does not auto-close it until the phase-5
result satisfies the parent issue acceptance criteria. After all CI checks pass,
squash-merge the PR, update issue #300 with the correspondence result, and
remove only this phase's worktree and branches.
