# Lean Annotation-Scope Correspondence Audit Design

## Goal

Close the evidence gap left by the binding-flow join audit for annotation-site
state under `global` and `nonlocal`, and for name resolution across
comprehension scope boundaries. Lean will own the expected facts at uniquely
marked source positions. Crate-private Rust fixtures will project annotation
state from `AnnotationCollector` and expression resolution from
`NameResolutionIndex`. Public `hoimin plan` observations remain a separate
strict check.

The durable claim is:

> At every audited annotation site, Hoimin resolves supported typing imports
> from the correct lexical target scope, does not leak comprehension targets
> into the enclosing scope, and does not retain a typing fact after a write to
> the scope selected by `global` or `nonlocal` unless that scope is re-imported.

This audit strengthens correspondence evidence. It changes production behavior
only if a same-premise fixture first demonstrates a mismatch.

## Prior audit finding

The binding-flow audit in PR #297 found no production mismatch. Its public
adapter covered candidate presence and absence, and its private fixture covered
suite exits and loop heads. Two `global`/`nonlocal` cases remained `model-only`
because a module-suite exit snapshot is not the state observed at the nested
annotation. Comprehension evaluation order was excluded entirely.

The improvement is a marker-addressed annotation-site projection. It records
the already-computed `AnnotationSite` state instead of reconstructing it from a
later suite exit, so the Rust and Lean observations share the same premise.

## Considered approaches

### Marker-addressed private projections plus fixed Lean cases (selected)

Expose a `#[cfg(test)]` helper that parses one source string, finds exactly one
annotation containing a literal marker span, and returns normalized import
facts, symbol, and lexical scope kind. A second helper finds a uniquely marked
name occurrence and returns its `NameResolutionIndex` result. Extend the Lean
model with the minimum annotation observation and comprehension boundary needed
for fixed witnesses.

This directly closes the previous evidence gap without changing a public API or
expanding bounded structured-program search.

### Public manifest cases only

Public candidate presence is valuable, but it cannot distinguish two internal
states that both suppress a candidate. It therefore cannot promote the previous
model-only cases to same-premise correspondence.

### General-purpose analyzer tracing API

A reusable trace API could expose every intermediate collector state, but it
would enlarge production surface and serialization policy for a focused audit.
The crate-private projection is sufficient and easier to remove or evolve.

## Boundary

Included behavior:

- direct and aliased supported typing imports;
- `global` writes and re-imports targeting module scope;
- `nonlocal` writes and re-imports targeting the nearest enclosing function;
- nested class bodies whose external directives target a module or function;
- list, set, dictionary, and generator comprehension target isolation;
- first iterable evaluation in the enclosing scope and subsequent generator
  clauses in the comprehension scope, observed through the production
  name-resolution index;
- exact annotation byte range, symbol, normalized import facts, and scope kind;
- exact name-occurrence byte range and resolution inside comprehension stages;
- existing public candidate decisions for the same source fixtures.

Excluded behavior:

- arbitrary Python runtime mutation of modules or `builtins`;
- Ruff parser correctness and malformed-input recovery;
- asynchronous generators and runtime execution of comprehensions;
- walrus-expression restrictions beyond the AST bindings already exposed by
  Ruff;
- mutation ranking, candidate limits, mutant execution, and report rendering;
- increasing the existing structured-program exploration depth above 2.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| A nested function declares `global Sequence` | frame path with a module-directed name | parsed function containing `global Sequence` | exact facts at marked annotation before and after a write/re-import | private annotation-site projection | `internal-fixture` |
| An inner function declares `nonlocal Sequence` | frame path with nearest-function-directed name | parsed nested functions containing `nonlocal Sequence` | exact facts at marked inner and outer annotations | private annotation-site projection | `internal-fixture` |
| A class body redirects an external write | class frame with module/global or function/nonlocal target | parsed nested class with directive | exact method and enclosing annotation facts | private annotation-site projection | `internal-fixture` |
| A comprehension target shadows a tracked name only inside its comprehension | comprehension frame entered after the first iterable | parsed list/set/dict/generator comprehension | exact resolution at marked name occurrences in the first iterable, filters, later iterables, and result expression | private name-resolution projection | `internal-fixture` |
| A comprehension leaves enclosing annotation imports unchanged | enter and leave comprehension without exporting local bindings | marked annotations before and after a parsed comprehension | exact annotation facts and symbol | private annotation-site projection | `internal-fixture` |
| The same annotation produces or suppresses a mutation candidate | candidate gate over source and destination facts | isolated project passed through public `hoimin plan` | candidate identity, replacement, span, and symbol | existing public adapter | `strict` |
| Reduced frame/name domain | finite Lean types | no public option selects the reduced domain | Lean result only | fixed cases and proofs | `model-only` |

Parse failure, missing or duplicate markers, timeout, guard termination, panic,
or malformed corpus data is `infrastructure-error` and has no semantic verdict.

## Lean model extension

Keep `BindingFlowModel` as the common fact lattice. Add an annotation
observation containing the resolved environment, lexical scope kind, and a
stable site role, plus a name-occurrence observation containing the projected
resolution. A comprehension transition has two explicit stages:

1. evaluate the first iterable in the enclosing frame;
2. enter a comprehension frame, invalidate generator targets before evaluating
   that generator's filters and all later clauses, then leave without exporting
   comprehension-local bindings.

`global` and `nonlocal` cases use the existing frame path and directive model,
but their expected environments are observed at the annotation statement rather
than inferred from a module exit.

Imported proof modules will establish:

- leaving a comprehension preserves the enclosing environment;
- a comprehension target is unknown within the comprehension after binding;
- unrelated imported names remain available inside the comprehension;
- a global-directed write affects the modeled module target but not an
  unrelated function-local fact;
- a nonlocal-directed write affects the nearest enclosing function target but
  not the module fact;
- re-import restores exact knowledge only in the directed target scope.

Every non-trivial new theorem has `maxHeartbeats 100000`. The model contains no
`sorry`, `admit`, custom axioms, or unlimited heartbeat setting.

## Fixed cases and sensitivity

The corpus adds fixed positive and negative pairs for:

- global visible-before, hidden-after-write, and restored-after-re-import;
- nonlocal visible-before, hidden-after-write, restored-after-re-import, and
  unaffected outer/module annotations;
- nested class global/nonlocal projection into the intended target scope;
- each comprehension family with a shadowing target and an unrelated retained
  builtin or exception name;
- first-iterable outer-scope visibility and comprehension-body target shadowing.

No generated-depth expansion is required. Existing depth-2 statistics remain a
regression check; the new audit reports only the number of fixed cases and
transitions evaluated by its small comprehension witnesses.

Sensitivity must distinguish these broken variants with literal witnesses:

1. a comprehension target leaks into the enclosing environment;
2. a comprehension target is bound before the first iterable is evaluated;
3. `global` writes are incorrectly applied to the current function;
4. `nonlocal` skips the nearest enclosing function;
5. an annotation projection uses suite-exit state instead of site-entry state.

The earlier union-join, class-closure, finally-routing, loop-back-edge, and
source-only-gating witnesses continue to run unchanged.

## Rust projection and adapter

Add two crate-private `#[cfg(test)]` projections beside the existing
binding-flow fixtures. The annotation projection's input is source plus an
exact marker string. It must:

- parse through the production parser;
- collect through the production `AnnotationCollector`;
- require exactly one annotation whose byte range contains the marker;
- return the annotation byte range, symbol, scope kind, and sorted normalized
  `KnownImports` facts;
- return a typed fixture error for parse, zero-match, or duplicate-match setup
  failures instead of converting them into semantic mismatches.

`AnnotationSite` gains the scope kind captured at record time. This field is
private and behavior-neutral. The production candidate path continues to read
the annotation, symbol, and imports fields exactly as before.

The name-resolution projection parses the same source, requires exactly one
identifier range matching its marker, builds the production
`NameResolutionIndex`, and returns the exact `DefinitelyBuiltin`, `Shadowed`,
or `Unknown` result for that occurrence. It is used for comprehension-internal
first-iterable and bound-target cases; it does not infer those results from an
annotation snapshot.

The Rust oracle adapter accepts only the four formal modes, validates a closed
scenario set, and compares complete observations for `internal-fixture` cases.
Expected facts remain generated by Lean; the Rust adapter must not duplicate
them as hand-written constants.

## Resource safety

All Lean commands run serially with `-Kjobs=1` through
`formal/HoiminOracle/tools/lean_resource_guard.py` using:

- 20-second wall-clock deadline;
- 768 MiB root-plus-descendant RSS termination threshold;
- 250 ms sampling;
- local theorem limit `maxHeartbeats 100000`.

The depth remains 2. No depth-3 or depth-4 run is attempted because the prior
audit measured baseline Lean RSS above the 512 MiB expansion-stop line. A
timeout, RSS stop, swap growth, severe UI slowdown, or unexplained superlinear
growth ends the attempted command; limits are not raised. The model is split or
proved inductively instead.

## Verification and completion

Verification order:

1. focused Rust projection tests, written before implementation;
2. guarded Lean theorem consumer and focused build;
3. guarded fixed cases, all sensitivity witnesses, and corpus freshness;
4. Rust corpus schema, private correspondence, and public correspondence;
5. focused analyzer tests;
6. `cargo test --workspace --all-features -j 2`;
7. Clippy with warnings denied, Rust formatting, Python Ruff checks, and
   `git diff --check`;
8. independent code review and all applicable pull-request CI checks.

Completion requires zero unresolved same-premise mismatches and zero
infrastructure errors. Any confirmed mismatch is retained as a minimized fixed
case before production is repaired. The final report separately states what
Lean proves about its model, what Rust fixtures observe, retained resource
limits, and remaining exclusions.
