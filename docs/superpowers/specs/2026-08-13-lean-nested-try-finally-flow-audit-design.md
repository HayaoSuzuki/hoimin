# Lean Nested Try/Finally Flow Audit Design

Date: 2026-08-13

## Objective

Audit the first independently shippable phase of issue #300. The durable claim
is:

> The outgoing known-import environment of a `try` subtree is the meet of
> exactly its reachable categorized exits after handler-target cleanup and the
> `finally` suite have each been applied once. A falling-through `finally`
> preserves the incoming exit category; an abrupt `finally` replaces it.

Lean will establish this claim for a reduced compositional model. A generated
corpus and Rust adapters will separately check the exercised implementation
correspondence. The audit will not claim that Lean proves the Rust analyzer.

## Why This Slice

The existing binding-flow audit proves meet laws, loop convergence, and a
single finally-category preservation rule. The exception/match audit checks
handler cleanup and match propagation with fixed cases. The Rust analyzer now
combines these mechanisms in `AnnotationCollector::visit_try`,
`apply_finally`, and `route_finally_entry`, but no general Lean theorem covers
their composition.

This slice has high leverage because it reuses the existing fact lattice,
categorized exits, resource guard, corpus conventions, and production-backed
test projections. It also covers control-flow mistakes that can unsafely retain
known imports and enable invalid mutation candidates. Nested `match`, compound
patterns, multiple-handler selection semantics, and `except*` remain separate
phases so that failures stay attributable.

## Included Behavior

- `try` body normal completion and abrupt exits;
- `else` execution only after normal `try` completion;
- one representative selected handler and its non-selected path;
- handler-target cleanup on fallthrough, break, continue, return, and raise;
- the analyzer's shared `terminate` category, with return and raise retained as
  distinct fixture sources;
- `finally` routing for fallthrough, break, continue, and terminate;
- a falling-through `finally` restoring the incoming category;
- an abrupt `finally` replacing the incoming category;
- sequential composition that prevents statements after an abrupt exit from
  contributing reachable exits;
- meet over every and only reachable exit in a category;
- complete public candidate observations where the public CLI exposes the
  premise, and production-backed internal snapshots otherwise.

## Excluded Behavior

- nested `match` inside handlers or loops;
- multiple concrete exception handlers and runtime handler selection;
- OR, AS, mapping, and class-pattern binding behavior;
- `except*` remainder and sibling paths;
- runtime Python exception-object lifetime;
- parser correctness and arbitrary Python syntax;
- loop fixed-point convergence, which the prior binding-flow audit owns;
- arbitrary-name scaling beyond representative `Sequence` and unrelated
  `Mapping` facts;
- exhaustive syntax-tree or trace generation;
- performance claims about the analyzer.

## Declared and Implicit Behavior

Declared behavior comes from issue #300, the existing audit reports, and test
names stating that categorized exits survive `finally`, handler targets are
cleaned on every exit, and joins use conservative intersection.

The implementation also has behavior that must be made explicit in the model:

- `return` and `raise` are normalized to `terminate` after their expressions
  have invalidated facts;
- unreachable statements may still be visited for structural collection, but
  their state must not re-enter the reachable exit set;
- `finally` annotations are observed once at the meet of all reachable entries,
  while transfer is replayed without recording annotations for each incoming
  categorized exit;
- a falling-through finalizer resumes the original category, whereas any abrupt
  finalizer exit overrides it;
- handler entry is conservative over possible writes from the `try` body;
- the `try` body's abrupt exits and handler exits both participate in the final
  routing.

The audit models semantic routing and observations needed by candidate
resolution. It does not model the implementation's traversal mechanics unless
they affect those observations.

## Correspondence Worksheet

Cases start as `model-only`. A case is promoted only when the production
premise and complete observation have been identified.

| Premise or observation | Lean representation | Production configuration | Observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Falling-through finalizer preserves fallthrough | categorized `fallthrough` entry plus a falling-through finalizer | public Python fixture containing a unique annotated marker after the `try` | complete candidate identity at the marker | `hoimin plan` manifest | `strict` |
| Falling-through finalizer preserves break | categorized `break` entry in a loop | owned Python fixture with marker-selected enclosing `try` | post-finally categorized `KnownImports` snapshot | production collector test seam | `internal-fixture` |
| Falling-through finalizer preserves continue | categorized `continue` entry in a loop | owned Python fixture with marker-selected enclosing `try` | post-finally categorized `KnownImports` snapshot | production collector test seam | `internal-fixture` |
| Falling-through finalizer preserves terminate from return | categorized `terminate` entry from a return source | owned function fixture | post-finally terminate snapshot | production collector test seam | `internal-fixture` |
| Falling-through finalizer preserves terminate from raise | categorized `terminate` entry from a raise source | owned function fixture | post-finally terminate snapshot | production collector test seam | `internal-fixture` |
| Abrupt finalizer replaces the incoming category | every incoming category routed through a finalizer that terminates | owned fixtures for the same AST premise | categorized exit snapshots after the full `try` | production collector test seam | `internal-fixture` |
| Handler target is cleaned before finalizer entry | selected handler with a named target and each reachable exit category | public fixture when candidate presence is externally visible; otherwise owned fixture | complete candidate or entry snapshot | CLI manifest or production collector seam | initially `model-only`, promoted per row |
| Non-selected handler path participates in meet | selected and non-selected reduced paths | no exact public provenance observation | reduced environment only | Lean model | `model-only` |
| Unreachable statement is excluded from the meet | abrupt exit followed by a binding-changing statement | owned marker projection can establish reachability and final exit facts | categorized exit snapshot | production collector test seam | `internal-fixture` |

Infrastructure setup, parsing, marker selection, timeout, crash, malformed
corpus, or missing observation is classified as `infrastructure-error` and
cannot produce a semantic mismatch.

## Lean Architecture

Create three imported modules and one non-imported executable entry point:

- `NestedTryFlowModel.lean` defines a small statement algebra, reachable
  categorized exits, sequential composition, handler cleanup, and finalizer
  routing. It imports and reuses `BindingFlowModel` facts rather than defining
  another lattice.
- `NestedTryFlowProofs.lean` proves the reachable-exit theorem, cleanup-once
  theorem, falling-through category preservation, abrupt replacement, and
  meet soundness. Non-trivial declarations use local bounded
  `maxHeartbeats 100000`.
- `NestedTryFlowCases.lean` defines fixed correspondence rows and literal
  broken variants. It contains only cheap decidable examples suitable for
  normal imports.
- `NestedTryFlowAuditMain.lean` owns corpus rendering, freshness checking,
  statistics, and sensitivity execution. It is not imported by
  `HoiminOracle.lean`.

The model represents reachability explicitly with optional fallthrough and
lists of abrupt states per category. Sequential composition feeds only
fallthrough into the next statement and carries existing abrupt exits
unchanged. Finalization routes each reachable entry independently and joins the
resulting states only after category routing.

No generated-depth exploration is planned. Fixed semantic witnesses and
universally quantified theorems are sufficient for this slice. If a proof
requires computation with unexpected growth, the declaration will be split
into lemmas rather than increasing limits.

## Sensitivity Requirements

The executable must reject each applicable broken transition with a fixed
witness:

| Risk family | Broken variant | Required witness |
| --- | --- | --- |
| Atomicity/cleanup | apply handler-target cleanup before the handler body or omit it before finalizer entry | target reimported in the handler, then exit through finalizer |
| Finalizer coverage | apply `finally` only to fallthrough | break, continue, return, and raise sources |
| Boundary/category | flatten a falling-through finalizer to fallthrough | one witness for each abrupt category |
| Boundary/category | retain the incoming category after an abrupt finalizer | fallthrough and one abrupt incoming category |
| Reachability | include an unreachable post-exit state in a meet | abrupt exit followed by a fact-restoring statement |
| Reachability | omit one reachable abrupt exit | two reachable branches with disagreeing facts |

Uniqueness/idempotency does not apply as an effect-identity property in this
pure transfer model. Its analogous risk is cleanup duplication; a fixed case
will show that applying deletion twice is idempotent and cannot invent facts.

## Corpus and Rust Adapter

Lean owns a deterministic schema-1 JSON Lines corpus under
`formal/HoiminOracle/corpus/nested-try-flow.jsonl`. Rows include identity,
mode, source family, entry category, finalizer outcome, normalized expected
categorized facts, and public candidate expectations where applicable.

The Rust adapter will:

- deny unknown fields and reject unknown modes, families, categories, and case
  identities;
- enforce a closed mapping from each row to a concrete fixture so that a row
  cannot silently change premises;
- run cases in isolation;
- use `hoimin_cli::run_with_io` for strict public cases and inspect complete
  candidate identity, span, operator, original text, replacement, and symbol;
- use the production `AnnotationCollector` through the narrowest owned
  `#[cfg(test)]` categorized-exit projection for internal fixtures;
- normalize only stable `KnownImports` observations;
- report setup, parse, timeout, process, malformed output, missing marker, and
  duplicate marker failures as infrastructure errors.

The adapter will not encode expected values, replay transfer rules, read
private fields from an external integration test, or rewrite the corpus during
strict comparison.

## Mismatch Workflow and Rust Repair

Correspondence runs occur after the Lean model, proofs, sensitivity witnesses,
and corpus freshness check pass. Each difference records the smallest input,
intermediate categorized exits, expected observation, actual observation,
source locations, and model limitations.

Each witness is classified before correction:

- `confirmed bug`: same public or owned contract premise and complete
  observation disagree;
- `specification ambiguity`: repository intent does not own the disputed rule;
- `model defect`: the Lean abstraction omitted behavior relevant to the
  claimed contract;
- `infrastructure error`: setup or observation failed.

Only a confirmed bug authorizes a production change. The Lean-owned corpus row
is retained first, then a focused same-premise Rust regression is added and
observed failing for the expected semantic reason. The smallest change to
`visit_try`, `apply_finally`, or `route_finally_entry` is made, followed by the
focused test and relevant workspace tests. No unrelated analyzer refactoring
is included.

## Verification and Resource Safety

Every Lean or Lake command runs alone through
`formal/HoiminOracle/tools/lean_resource_guard.py` with:

- 20-second wall-clock deadline;
- 768 MiB root-plus-descendant RSS limit;
- 250 ms sampling;
- `-Kjobs=1` for Lake builds;
- no unlimited heartbeat setting.

The retained report records command, domain size, fixed-case count, elapsed
time, highest observed RSS sample, exit code, and reason. A timeout, RSS stop,
or monitor failure is an infrastructure result. Aggregate builds are not
retried after a resource stop. The work is split further if a focused module
cannot stay within the existing limits.

Rust verification covers the focused adapter, analyzer unit tests, strict
public cases, `cargo test --workspace --all-features`, Clippy with warnings
denied, formatting, corpus freshness, and `git diff --check`. Existing Python
resource-guard tests remain part of the final quality gate.

## Deliverables and Stop Condition

The worktree will contain:

- this design and a detailed implementation plan;
- imported Lean model, proof, and fixed-case modules;
- a non-imported audit executable and checked-in Lean-owned corpus;
- focused Rust correspondence tests and any required production-backed test
  seam;
- a retained Rust regression and minimal production fix if a confirmed bug is
  found;
- a self-contained audit report with correspondence worksheet, proof boundary,
  sensitivity matrix, counterexample ledger, commands, and resource ledger.

Stop after the reachable-exit invariant, finalizer routing, cleanup boundary,
and implementation-facing cases are covered. Nested match, compound patterns,
multiple-handler selection, and `except*` are subsequent phases rather than
reasons to enlarge this audit.
