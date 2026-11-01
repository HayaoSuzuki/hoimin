# Issue #326: Loop back-edge name resolution design

## Problem

Ordered name resolution in module and class scopes considers only binding
events at or before the queried source offset. That models one linear pass
through a suite, but a loop can return from a later binding to an earlier load.

For example, the first evaluation of `sorted(xs)` below uses the builtin, but
every later evaluation can use `fake`:

```python
for _ in range(2):
    print(sorted(xs))
    sorted = fake
```

The analyzer currently classifies `sorted` as definitely builtin and emits a
`sorted` to `reversed` mutation whose builtin-pair precondition does not hold
on the loop back edge.

## Goals

- Treat a tracked name as uncertain at a loop load when the same ordered scope
  can bind that name later in the repeated region.
- Apply the rule to both the source and destination of builtin-pair operators.
- Model `for` bodies and the repeated test plus body of `while` statements.
- Preserve the current result when linear resolution is already `Shadowed` or
  `Unknown`.
- Keep bindings in nested functions and classes from leaking into an unrelated
  loop scope.
- Cover nested loops, conditional bindings, deletion, imports, and wildcard
  uncertainty through the existing binding-recording paths.

## Non-goals

- Proving exact loop reachability or iteration counts.
- Distinguishing paths through `break`, `continue`, exceptions, or conditional
  statements inside a loop.
- Replacing the existing ordered name-resolution model with a complete control
  flow graph.
- Changing function-local resolution, which is already scope-wide under Python
  local-variable semantics.

## Options considered

### Annotate occurrences while visiting active loops

Maintain a stack of active repeated regions. Each region records its ordered
scope, tracked load offsets in the region, and tracked names bound in that
scope. When the region closes, attach its binding set to every recorded
occurrence. Resolution demotes `DefinitelyBuiltin` to `Unknown` when the
queried name is in that occurrence's back-edge binding set.

This is the recommended design. It reuses the builder's scope and binding
knowledge, naturally composes for nested loops, and adds no second AST walk.

### Check loop source ranges during every resolution query

Store loop ranges and binding names, then test containment on demand. This
requires reconstructing which scope a binding affects and which part of a
`while` is repeated for every query. It also makes destination-name checks easy
to omit. This option is rejected.

### Build a control-flow graph for name resolution

A control-flow graph could represent exact loop joins and exits, but it is much
larger than the residual problem and duplicates analyzer flow infrastructure.
The conservative `Unknown` result is sufficient for candidate safety.

### Mark only the loaded spelling as uncertain

Recording that the `sorted` load itself has a back-edge binding fixes the issue
reproduction, but builtin-pair operators resolve both `sorted` and `reversed`
at the same occurrence offset. A later `reversed` binding must also suppress a
`sorted` to `reversed` candidate. This option is incomplete.

## Detailed design

Add a loop context to `NameResolutionBuilder` containing:

- the scope whose ordered bindings govern the repeated region;
- offsets of tracked load occurrences evaluated in that region;
- the set of tracked names bound by the same scope in that region.

`record_occurrence` adds a load offset to every active context whose ordered
scope is visible from the occurrence. Ordinary same-scope loads are visible.
Module bindings are also visible through nested class scopes because class
fallback resolution skips enclosing classes and consults the module. Function
or comprehension boundaries do not expose an outer ordered loop scope.

`record_binding`, `record_unknown`, and wildcard recording add affected names
to all active contexts owned by the binding scope. Existing helpers remain the
single source of truth for assignments, imports, deletes, definitions, dynamic
uncertainty, and other binding forms.

When a context closes, its binding names are stored for every recorded offset
in a sparse `NameResolutionIndex::back_edge_bindings` map. Occurrences outside
loops therefore gain no per-occurrence storage. `NameResolutionIndex::resolution`
first performs its existing temporary-shadow and scope resolution. If the
result is `DefinitelyBuiltin` and the queried name is in the offset's set, it
returns `Unknown`. Existing `Shadowed` and `Unknown` results are unchanged.

The set contains every binding name, not only the spelling loaded at that
offset. This is required because `resolves_builtin_pair` queries the
replacement name at the source name's occurrence offset.

## Loop boundaries

- `for`: evaluate the iterable outside the back-edge context, then keep target
  binding, target evaluation, and the body inside it. Complex assignment
  targets can evaluate loads on every iteration, while the iterable is
  evaluated only once. The `else` suite is not repeated.
- `while`: activate the context before visiting the test, keep it active for
  the body, then close it before the `else` suite. Both the test and body are
  evaluated again after a back edge.
- Nested loops in the same scope contribute bindings and occurrences to every
  enclosing active context. A possibly zero-iteration inner loop therefore
  conservatively makes the enclosing repeated load `Unknown`.

## Safety and precision

The model intentionally asks whether a later binding can affect a repeated
evaluation, not whether it must. Conditional execution, zero iterations, and
early exits therefore produce `Unknown`, which suppresses unsafe builtin-pair
candidates without falsely claiming a shadowed name. Loads outside the repeated
region retain the existing source-order result.

## Tests

Add name-resolution and analyzer regressions covering:

- the reported module-level `for` body binding;
- a replacement-name binding later in the body;
- a class-body loop;
- a `while` test affected by a body binding;
- nested loops in one ordered scope;
- a `for` iterable, which must remain definitely builtin;
- a complex `for` target load affected by a body binding;
- an unrelated nested-function binding, which must not leak into the loop;
- existing linear `Shadowed` and `Unknown` outcomes.

The focused analyzer regression asserts that no builtin-pair candidate is
emitted at unsafe loop loads while a safe load in a one-time `for` iterable is
still emitted.
