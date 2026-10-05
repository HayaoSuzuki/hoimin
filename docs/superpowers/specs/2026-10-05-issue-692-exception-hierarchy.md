# User-defined exception hierarchy mutations

Issue: #692. Branch: `investigate/user-defined-exception-mutations`.

## Behavior

Add the explicit-only operator `exception_hierarchy`. Preserve the existing default
operators, `exception_ops`, and builtin exception replacements. The new operator
changes a single exception reference in `except`, `except*`, and the primary
expression of `raise` (including constructor calls). Preserve arguments and causes.

Eligible pairs are distinct user-defined classes connected by a direct parent/child
edge, or sharing the same user-defined direct parent. Builtin classes are ancestry
seeds, never destinations. Resolve references by identity, not spelling. Emit one
deterministic spelling per destination identity visible at the mutation site.

Support unconditional top-level single-inheritance classes, project-local explicit
absolute/relative imports and import aliases, simple and module-qualified references.
Follow indirect ancestry to ordinary builtin Exception subclasses. Reject termination
and exception-group lineages. Do not add imports. Unsupported syntax, unresolved
bases, ambiguous names, duplicate bindings, class decorators, class keywords,
multiple inheritance, and nested definitions cannot establish trusted identities.
Treat imported re-exports and cycles conservatively. This is a conservative lexical
analysis, not proof against arbitrary external monkeypatching.

For raised exceptions, both endpoints must inherit the ordinary Exception
constructor without custom `__init__` or `__new__` anywhere on their user-defined
ancestry. Initially only ancestry rooted directly at builtin Exception qualifies
for construction; specialized builtin constructors are handler-only. Bare raise
and dynamic expressions are excluded. Handler eligibility does not require
constructor compatibility.

## Architecture

A separate `analyzer/exception_hierarchy` module owns a compact immutable project
index. Parse with the existing Ruff parser and existing recursion guard. Retain
class/import/binding summaries rather than whole project ASTs. Resolve ancestry
iteratively with explicit bounds. Build once per discovery/run analysis session,
and share through an Arc. Thread the project into both plan discovery and run's
blocking analysis; direct source-only analyzer callers build a single-module index.

The per-module analysis retains lexical binding exclusions for module and function
scopes. Parameters, assignments, imports, captures, exception targets, globals and
nonlocals must not make a shadowed source/destination appear available. Conservative
scope-wide exclusion is acceptable. Class bodies are excluded; methods resolve
module names without treating class locals as closures. Dynamic namespace writes
invalidate trust. Definition/base resolution respects definition order.

Feed hierarchy candidates through the existing AST producer so profile filtering,
line/symbol selection, source ordering, deduplication and bounded retention remain
shared. Check cancellation while building summaries, resolving edges and enumerating
destinations. Diagnostic messages explain unsupported hierarchy references without
changing the existing candidate schema.

## Project inputs

Discover Python inputs independently of mutation file/line/symbol/changed filters.
Reuse source-discovery include/exclude and built-in exclusion policy. Module search
roots follow worker ordering (project root, configured import roots, then source
roots; deduplicate preserving precedence). A selected Python file can participate
even if its parent is not an explicitly supplied source directory.

Include automatically discovered hierarchy inputs in existing fingerprint records.
Re-resolve the same input set during verify/resume and compare additions, deletions
and content changes. Keep user-declared fingerprint selectors intact. Propagate
automatic inputs into workspace revalidation. Never import target Python modules.

Index limits: 4096 Python files, 16 MiB per file, 64 MiB total decoded source,
256 user-defined ancestry steps (the terminal builtin consumes no step), 256 import
steps, and 65536 entries per module-name, binding, import-dependency and visible-alias table. Exceeding a project limit
fails with an explicit analysis error, never silently reuses a partial index. Candidate count still uses the
existing `max_candidates`; avoid constructing all class pairs upfront.

## Verification

Tests cover exact pairs/spans, aliases, cross-module and relative imports,
transitive ancestry, inaccessible destinations, reassignment and shadowing,
conditional/dynamic definitions, exception groups, constructor restrictions,
parseability, plan/run parity, dependency fingerprints, exclusions, limits,
cancellation and deterministic bounded candidate prefixes. Run focused tests then
workspace tests, formatting and clippy.

Use three documented self-review passes for design, plan, implementation and tests.
Lean now supplements the concrete regression tests. The follow-up
[formal audit](../reports/2026-10-05-exception-hierarchy-lean-audit.md) models
ancestry, binding precedence, load order, module identity, bounded insertion and
prepared-input cache transitions. Kernel-checked properties are paired with
Lean-generated expectations exercised through the public planner and owned
internal fixtures. This supersedes the initial decision to omit Lean: a graph-only
proof was insufficient, but the wider invariants still deserved formal checking.
The proof does not establish arbitrary Python semantics or Rust refinement.

Review corrections: module search precedence matches `build_command_environment_with_import_roots`.
Implicit inherited PYTHONPATH entries outside configured roots are not indexed.
Fingerprint discovery scans the allowed project Python files, including files outside
selected mutation roots, so a newly added module cannot silently alter resolution.
Unsupported references produce a bounded per-file diagnostic, not one message per
possible destination. Project limits are enforced before parsing additional files.

Implementation review clarification: classes with the same file but different imported
module names are conservatively rejected. Ambiguous namespace/regular-package layouts
are also rejected rather than claiming a complete Python import resolver. Function
uses require definitions preceding the function header; implicit class cells and
private names are excluded in method scopes. Candidate replacement retention is
bounded per site in addition to the existing producer bound.

Focused-review corrections: relative imports resolve under the module's imported
name; qualified references respect submodule load order. Private binding exclusions
include their class-mangled spellings. CPython 3.14 standard-library/frozen roots
and `__main__` cannot resolve to local project modules. Loaded raw bytes must match
prepared fingerprints, and prepared Python paths reserve opaque module origins
when absent from the current input set. This can suppress candidates but prevents
an absent higher-priority module from exposing a different lower-priority class.


Submodule imports can overwrite a same-name class in the parent package. Record
possible imports from every scope and invalidate colliding package bindings before
resolving class identities, including absolute/relative imports from other files.
This is conservative across call order: a colliding import can suppress candidates
even if that function is never called. Only imports outside function bodies form
the static initialization-cycle graph; deferred self-imports do not invalidate an
otherwise stable hierarchy. The import-dependency table has its own entry bound.
