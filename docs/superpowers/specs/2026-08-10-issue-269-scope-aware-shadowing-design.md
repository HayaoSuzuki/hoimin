# Issue 269 Scope-Aware Shadowing Design

## Goal

Replace file-wide builtin and exception shadow sets with one conservative
lexical resolver. A binding in an unrelated scope must stop suppressing safe
mutations, while every source and replacement name must still be proven to
resolve through Python's builtins namespace at the candidate site.

The current file-wide sets create false negatives. They also expose a separate
safety defect: builtin-call mutation checks only the source name. If `list`
still resolves to the builtin but `tuple` is locally rebound, Hoimin can emit
`list(items) -> tuple(items)` even though the replacement calls user code.

## Scope

The resolver covers the builtin-call and exception mutation families and the
following Python blocks and bindings:

- module, function, lambda, class, and comprehension scopes;
- parameters, assignments, imports, definitions, loop/with targets, match
  captures, named expressions, and exception targets;
- nested functions and closures, sibling isolation, and class non-closure;
- `global` and `nonlocal` directives;
- wildcard imports and control-flow ambiguity;
- the comprehension leftmost iterable boundary.

Mutation operator IDs, profiles, defaults, candidate ordering, spans, symbols,
limits, and report schemas remain unchanged.

## Python Semantics Used

The resolver follows the Python execution model:

- a binding anywhere in a function block makes the name local throughout that
  block unless redirected by `global` or `nonlocal`;
- free names use the nearest enclosing function scope, then the module and
  builtins namespaces;
- class namespace bindings are visible while the class body executes but are
  not closure bindings for ordinary methods;
- `global` redirects the whole block to the module namespace and `nonlocal`
  selects the nearest enclosing function binding;
- a comprehension has an implicit nested scope except that its leftmost
  iterable is evaluated in the enclosing scope.

These rules are specified by the Python reference's
[execution model](https://docs.python.org/3/reference/executionmodel.html),
[`global` and `nonlocal` statements](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement),
and [comprehension evaluation](https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries).

## Considered Approaches

### Scope graph plus occurrence resolution index (selected)

Build a compact scope graph and binding summaries from the parsed Ruff AST,
then perform a semantic source-order walk that records the resolution of every
tracked name load by byte offset. Candidate collection asks this shared index
about both the source and hypothetical destination name.

This makes comprehension boundaries explicit, keeps candidate collection free
of mutable scope state, and lets builtin-call and exception operators use the
same result.

### Mutable resolver inside candidate collection

Push and pop scopes while the existing candidate visitor walks the AST. This
avoids a separate occurrence map but couples source-order binding updates to
candidate production. Ruff's generic comprehension walk does not represent the
leftmost-iterable scope boundary, and a missed custom branch could silently
make emitted candidates unsafe.

### Range-only scope lookup

Find the innermost function/class range containing each byte offset. This fails
for the leftmost iterable of a comprehension, which is textually inside the
comprehension range but semantically belongs to the enclosing scope. It also
cannot represent exception-target lifetime precisely.

## Resolution Lattice

For each tracked name at a candidate position, the index returns one of:

- `DefinitelyBuiltin`: no visible lexical binding or uncertainty exists;
- `Shadowed`: a visible binding definitely prevents builtin lookup;
- `Unknown`: control flow, wildcard import, redirection, deletion, or an
  unsupported construct prevents a proof.

A mutation is emitted only when both its source name and every destination
name are `DefinitelyBuiltin`. `Shadowed` and `Unknown` have the same safe
outcome—candidate suppression—but remain distinct for tests and diagnostics.
This fixes the existing replacement-destination defect as well as the
file-wide false negatives.

## Scope Graph

Each scope has a stable `ScopeId`, kind, lexical parent, and per-name facts.
Tracked names are limited to the existing mutable builtin and curated exception
sets.

Function and comprehension scopes store whole-block local bindings because
Python determines locals by scanning the complete block. `global` and
`nonlocal` directive sets remove redirected names from the local set.

Module and class scopes additionally store ordered binding effects. Direct
expressions in those executable bodies consult effects before the occurrence.
Simple unconditional bindings transition to `Shadowed`; wildcard imports,
deletions, and control-flow-dependent effects transition to `Unknown` unless a
later unconditional binding proves `Shadowed`.

For a free name used by a deferred function, the module's whole-file summary is
used instead of definition-time source order: the function can be called before
or after a later module assignment. The summary is `DefinitelyBuiltin` only
when no tracked binding, wildcard import, or redirected global write exists
anywhere in the module.

Nested ordinary functions and comprehensions skip intervening class scopes.
Class-body expressions themselves use the class's ordered effects and then
fall back to the module. Annotation scopes remain governed by the existing
type-position suppression; any tracked occurrence the resolver cannot map is
`Unknown`.

## Occurrence Walk

A second AST walk carries the current `ScopeId` and ordered module/class state.
It records `Name` loads for tracked names in a map keyed by byte start. The walk
uses explicit order for definition headers, bodies, handlers, and
comprehensions.

For a comprehension, the first generator's iterable is visited in the outer
scope. A comprehension scope is then entered for its target, filters, later
generators, and result expressions. All comprehension targets are whole-scope
locals there.

Exception targets are function-wide locals in a function. At module/class
level they shadow only the handler body and are absent after handler cleanup;
uncertain pre-existing/control-flow state remains `Unknown` outside that exact
region.

## Shared Candidate Policy

`AstFacts` replaces `bound_builtin_names` and `bound_exception_names` with a
`NameResolutionIndex`. The following checks all call one helper:

- builtin call pairs such as `any/all`, `list/tuple`, and `set/frozenset`;
- `sorted/reversed`;
- exception type pairs and tuple additions;
- `Exception/BaseException` boundaries;
- bare-handler insertion of `Exception`.

For pair mutations the helper checks the source and destination at the same
occurrence. Tuple additions check every inserted destination. Existing syntax,
argument-contract, termination-exception, and final-handler guards remain in
place.

## Conservative Fallback

The resolver never guesses through wildcard imports, unsupported dynamic
binding constructs, ambiguous control flow, or a missing occurrence mapping.
Those cases return `Unknown` and suppress candidates. Dynamic mutation of the
`builtins` module remains outside static lexical proof; the safety contract is
that ordinary Python lexical resolution reaches the builtin name.

Bare calls to `exec`, `globals`, `locals`, or `vars` introduce wildcard
uncertainty at their possible effect point. They also taint deferred module
lookup because the returned namespace or executed code may write a tracked
global. Calls through aliases remain outside the static lexical model.

This policy favors retained safety over maximal coverage. Later refinements can
turn an `Unknown` into a proof without changing candidate semantics elsewhere.

## Lean Design Analysis

`ScopeResolutionModel.lean` models scope frames, whole-block versus
source-ordered binding facts, class skipping, `global`/`nonlocal`, and the
three-result knowledge lattice. `ScopeResolutionProofs.lean` proves:

- only `DefinitelyBuiltin` can be allowed;
- allowed resolutions admit only an actual builtin binding;
- both source and destination of an allowed replacement are builtin;
- unrelated sibling frames cannot affect resolution.

Executable examples cover late local binding, closures, class-body order,
method class skipping, comprehension body/outer separation, module-wide global
risk, nonlocal binding, and wildcard uncertainty. The model build uses no
`sorry`, `admit`, or custom axiom.

The Lean model exposed the destination-name defect before Rust implementation
and forced the distinction between direct module/class source order and
deferred function lookup through a whole-module summary.

### Correspondence boundary

| Premise or observation | Lean representation | Production configuration | Observation | Evidence | Mode |
| --- | --- | --- | --- | --- | --- |
| Three-result resolution and source/destination gating | `Knowledge`, `allowsReplacement` | Abstract frames are not a public input | Lean result only | model examples and theorems | `model-only` |
| Function, sibling, module/class, comprehension, and directive boundaries | ordered `Frame` paths | Python source fixture through the owned analyzer | candidate tuples | Rust analyzer unit tests | `internal-fixture` |
| Replacement safety and parseability | both knowledge inputs are `.builtin` | Python source fixture through the owned analyzer | emitted replacement and reparsed module | destination-shadow and reparse tests | `internal-fixture` |

There is no claim that Lean proves the Rust AST walker. The model fixes the
semantic policy; the owned analyzer fixtures separately exercise corresponding
premises. No public Lean-generated corpus adapter is part of this change.

### Sensitivity and evaluation placement

The imported proof module contains a deliberately broken source-only
replacement gate. Its fixed witness admits `.builtin -> .shadowed`, while the
real gate rejects the same pair. This covers the applicable boundary/precedence
risk and specifically detects the pre-existing destination-name defect.
Atomicity and idempotency are not applicable because resolution is a pure,
single-occurrence decision with no transition state or durable effect.

The model and kernel-checked proofs remain cheap imported modules; they contain
no exhaustive trace generation, corpus serialization, or `native_decide`.
`lake build` checks the complete formal project, and the new files are scanned
for `sorry`, `admit`, and custom `axiom` declarations.

## Testing

Regression tests use literal source fixtures and expected candidate tuples.
They cover:

- sibling bindings no longer suppressing source or destination names;
- visible module/local bindings and late function locals suppressing them;
- a shadowed destination rejecting an otherwise builtin source call;
- closures, nested siblings, classes, methods, and lambdas;
- comprehension targets and the leftmost iterable boundary;
- module and function exception-target lifetimes;
- `global`, `nonlocal`, imports, and wildcard imports;
- every builtin-call and exception family using the shared resolver;
- unchanged ordering and parseability of retained replacements.

Focused analyzer tests run first, followed by the Lean build, formatting,
Clippy, the full Rust workspace, E2E tests, and skill contract tests.
