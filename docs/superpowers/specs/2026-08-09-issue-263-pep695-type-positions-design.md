# Issue 263 PEP 695 Type-Position Design

## Goal

Model PEP 695 declarations consistently so Hoimin does not emit runtime
expression mutations from type-only syntax and does emit the existing
type-annotation mutations for supported type-alias expressions.

The analyzer currently treats the right-hand side of `type Alias = ...` as an
annotation range for runtime-mutation suppression, but the annotation
collector does not turn it into type-mutation candidates. Type-parameter bounds
and defaults on functions, classes, and type aliases are neither suppressed as
runtime syntax nor collected as type positions.

## Scope

The change covers parser-supported PEP 695 syntax:

- function, class, and type-alias type-parameter bounds;
- function, class, and type-alias type-parameter defaults;
- type-alias right-hand side expressions;
- nested and generic declarations;
- source-order import resolution and declaration-qualified symbols.

The following remain unchanged:

- mutation operator IDs, profiles, defaults, selectors, and report schemas;
- the supported type-expression shapes and replacement pairs;
- treatment of ordinary parameter, return, and annotated-assignment
  annotations;
- parser diagnostics for syntax unsupported by the pinned Ruff parser;
- candidate ordering, limits, fingerprints, and exact source spans.

## Considered Approaches

### Shared type-position enumeration (selected)

Introduce small helpers that enumerate the expression-valued type positions in
`TypeParams` and a type-alias statement. Both analyzer consumers use these
helpers:

- `AstFacts` records each expression range to suppress runtime token and AST
  mutations;
- `AnnotationCollector` records each expression with the import snapshot and
  declaration symbol used for type-annotation candidates.

This gives the two consumers one definition of PEP 695 type positions while
preserving their separate responsibilities.

### Independent visitor branches

Add matching `TypeParam` and `TypeAlias` branches separately to `AstFacts` and
`AnnotationCollector`. This is smaller initially, but recreates the drift that
caused the issue: adding a future type position to one visitor can silently
omit it from the other.

### Replacement reparse filtering

Generate candidates under the current rules, apply them, and retain only
parseable results. This cannot reject a parseable runtime mutation in a
type-only position and adds parser work per candidate, so it does not address
the semantic classification defect.

## Architecture

### Type-position helpers

A helper visits only expression-valued type positions. It does not treat the
type-parameter name itself as an expression and does not recursively classify
ordinary runtime expressions near a declaration.

For each `TypeParam` variant, it yields the bound and/or default expression
provided by the Ruff AST. A type-alias helper additionally yields the alias
right-hand side. The helper accepts a callback so consumers can retain their
own lifetime, state, and error-free traversal rules without allocating a
second AST representation.

### Runtime-mutation suppression

`AstFacts` records every yielded expression through the existing annotation
range mechanism. Existing token and AST candidate checks therefore suppress
runtime arithmetic, bitwise, collection, and structural mutations inside PEP
695 type positions without adding operator-specific exceptions.

The already-suppressed type-alias right-hand side remains suppressed. Moving
it through the common helper is a refactoring of that classification, not a
behavioral expansion.

### Type-mutation collection

`AnnotationCollector` records yielded expressions as `AnnotationSite` values.
It takes the same source-order `KnownImports` snapshot used by existing
annotations, so a typing import must be valid at the declaration site before a
replacement can use its spelling.

Function and class type parameters use the declaration's qualified symbol.
Type-alias positions use the alias's qualified symbol. Nested declarations
extend the current qualification stack and do not leak bindings into sibling
scopes.

Header expressions are collected before the declaration name is bound in the
enclosing scope. Existing conservative invalidation continues to account for
names referenced by decorators, defaults, bases, keywords, and type-parameter
expressions.

### Type-parameter annotation scope

PEP 695 gives declared type parameters a temporary annotation scope. Those
names are visible in bounds, defaults, function annotations, class bodies, and
type-alias values, and they shadow same-named imports from an enclosing scope.
Decorators and ordinary function parameter defaults remain outside that scope.

`AnnotationCollector` represents this with a cloned `KnownImports` overlay.
Every declared type-parameter name invalidates an imported direct name or
module alias and is marked as a type variable inside the overlay. This both
preserves the existing policy that skips mutations involving type variables
and prevents an unsafe replacement spelling such as `Sequence[int]` when
`Sequence` is the declaration's type parameter rather than `typing.Sequence`.
The outer import environment is restored after the generic declaration.

Generic function and class bodies receive the overlay while their annotations
are collected. A nested generic declaration adds its own parameters without
losing the enclosing annotation scope. Type-alias bounds, defaults, and value
use one overlay and one qualified alias symbol.

## Error Handling and Compatibility

The analyzer adds no new error class. Parser rejection continues to produce the
existing invalid-syntax diagnostic. A type expression unsupported by the
existing replacement logic remains a valid type position but produces no type
candidate.

Candidate byte spans are the exact AST expression ranges. Existing source
hashing and candidate validation remain unchanged. The change intentionally
removes only runtime candidates whose complete expression lies in a newly
recognized PEP 695 type position and adds only existing type operators for
supported alias, bound, or default expressions.

## Testing

Analyzer regressions will use hand-written expected candidate tuples rather
than deriving expectations through the production type-position helper. They
will cover:

- function and class type-variable bounds next to ordinary runtime operators;
- generic and nested type aliases;
- type-alias right-hand side collection mutations;
- defaults for the parser-supported type-parameter variants;
- exact original text, replacement, operator, span, line, and symbol;
- preservation of nearby runtime candidates;
- suppression of imported replacement spellings shadowed by a type parameter;
- absence of runtime bitwise/arithmetic candidates inside type positions;
- parseability of every emitted replacement;
- candidate ordering and selectors for declaration-qualified symbols.

Focused analyzer tests run first, followed by formatting, Clippy, the complete
Rust workspace suite, and Python contract tests. Lean is not used for this
issue because the defect is an AST context-classification mismatch rather than
a state machine or interleaving invariant. The following workspace lifecycle
audit will use Lean for its ownership and asynchronous-transition design.
