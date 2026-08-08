# Issue 264 Typing Import Rebinding Design

## Goal

Prevent type-annotation mutation candidates from using a `typing` or
`collections.abc` spelling after its local name has been rebound to another
object.

Issue #264 is a correctness fix. The analyzer currently builds one
module-wide `KnownImports` map and uses it for every annotation. A later
assignment, definition, or competing import does not invalidate that map, so a
replacement such as `Sequence[str]` can resolve to a user class rather than
`typing.Sequence`. Such candidates create false kills and unreliable mutation
scores.

## Scope

The change covers the imported names used by all existing type-annotation
operators:

- direct imports such as `from typing import Sequence as Seq`;
- module imports such as `import typing as t`;
- supported imports from `collections.abc`;
- source-order rebinding by assignment, annotated or augmented assignment,
  imports, function and class definitions, parameters, loop/with/exception
  targets, named expressions, and pattern bindings;
- module, class, function, and nested lexical scopes;
- conservative joins after conditional or repeated control flow.

The following remain unchanged:

- operator IDs, selectors, profiles, candidate ordering, limits, and report
  schemas;
- the preferred spelling of a valid unshadowed direct or module import;
- builtin collection names and the separate builtin/exception shadowing
  policy;
- the supported annotation shapes and destination type pairs;
- syntax diagnostics and cancellation behavior.

No dependency, CLI option, report field, or plan schema is added.

## Considered Approaches

### 1. Annotation-site import snapshots (selected)

Walk statements in execution order while maintaining an abstract import
environment for the current lexical scope. Each collected annotation owns a
snapshot of the environment that is valid at that site. Bindings update the
environment, nested scopes use independent environments, and control-flow
paths retain an alias only when every reachable path agrees on the same
import.

This makes both resolution of an existing annotation and spelling of its
replacement use the same source-order fact. It keeps the current
`KnownImports` resolution helpers small and makes unsafe states
unrepresentable at candidate generation time.

### 2. Precomputed binding intervals

Record every binding and lexical scope, then answer `name at byte offset`
queries when generating candidates. This separates collection from lookup but
requires a second scope model, range indexing, and special interval rules for
branches and function locals. The snapshot walker represents the same facts
with less synchronization risk.

### 3. Whole-scope invalidation

Invalidate an imported alias if the same scope binds its name anywhere. This
is simple and safe, but incorrectly suppresses candidates before a later
module or class rebinding and prevents a later unconditional typing import
from re-establishing a safe name. It does not meet the source-order acceptance
criterion.

## Architecture

### Annotation sites carry resolution state

Replace the collector's tuple output with an internal record:

```rust
struct AnnotationSite<'ast> {
    annotation: &'ast Expr,
    symbol: Option<String>,
    imports: KnownImports,
}
```

`type_annotation_candidates` passes `site.imports` to
`annotation_replacements`. The candidate generator no longer consults the
module-wide import map in `AstFacts`.

`KnownImports` remains the resolved direct/module/type-variable vocabulary for
one point in execution. It gains explicit transfer operations that either
establish a known typing import or invalidate a local spelling. Its existing
`resolved_name` and `spelling_for` behavior remains the replacement policy.

### Scope-aware statement traversal

The annotation-site collector owns a stack of scope environments and walks
statement suites in execution order.

At module and class scope, an unconditional binding changes the environment
only after expressions and annotations evaluated by that statement. Thus:

```python
from typing import Sequence
before: list[str]
class Sequence: ...
after: list[str]
```

keeps `Sequence[str]` for `before`, invalidates it for `after`, and does not
prematurely hide the import while the class statement itself is being
evaluated.

An unconditional supported typing import establishes or re-establishes its
local spelling. A competing import or any non-import binding invalidates the
same spelling. Rebinding after an annotation does not retroactively change the
snapshot already attached to that annotation.

### Function scopes

Python determines function-local names for the whole function body. Before
walking a function body, a binding prepass collects its local names without
descending into nested function or class bodies. Those names hide matching
outer imports from the start of the body. Parameters begin as unknown local
values; an unconditional supported import can establish a safe typing value
from its execution point onward.

The defining function's parameter and return annotations are collected in the
enclosing environment before the function name is bound. The body then uses a
new function scope. This preserves Python's definition-time lookup:

```python
from typing import Sequence

def Sequence(value: Sequence[str]) -> Sequence[str]:
    ...
```

The signature can still resolve the imported `Sequence`; statements after the
definition cannot.

A function nested directly in a class does not use the class namespace as a
closure for its body. Its signature is evaluated in the class environment,
while free-name lookup in its body follows the enclosing non-class scope.

`global` and `nonlocal` declarations affect the target scope rather than
creating an ordinary local. If the collector cannot prove a unique known
import through such a declaration, it invalidates that spelling instead of
guessing.

### Conservative control-flow joins

Each branch starts from a clone of the incoming environment. The state after a
compound statement is the intersection of reachable exit states: a direct
name, module alias, or type variable remains known only when every exit maps it
to the same resolved symbol.

- an `if` without `else` includes the unchanged incoming path;
- a loop includes its zero-iteration path;
- `try` joins normal and handler exits before applying `finally`;
- `match` includes unmatched flow unless a case is irrefutable;
- a conditional import does not establish a name on all paths;
- a conditional competing binding invalidates an existing alias after the
  join unless every path restores the identical typing import.

This deliberately prefers a skipped candidate over a spelling whose runtime
binding depends on control flow.

### Nested scopes and binding targets

Class and function bodies receive new environments and do not leak their
ordinary bindings into the enclosing scope. The definition name itself is
bound in the enclosing scope only after the definition statement.

Binding extraction covers recursive tuple/list/starred targets and the named
targets exposed by imports, parameters, `for`, `with`, `except`, assignment
expressions, and match patterns. Comprehension-local bindings do not leak into
the surrounding environment.

## Behavioral Examples

The first annotation is eligible and the second is skipped:

```python
from typing import Sequence
before: list[str]
Sequence = local_sequence
after: list[str]
```

An unconditional re-import restores a safe spelling:

```python
from typing import Sequence
Sequence = local_sequence
from typing import Sequence as Sequence
value: list[str]  # replacement: Sequence[str]
```

Module aliases follow the same rule:

```python
import typing as t
before: list[str]
t = registry
after: list[str]
```

A function-local binding hides an outer alias throughout the function body,
including source locations before the binding:

```python
from typing import Sequence

def build():
    before: list[str]
    Sequence = local_sequence
```

The analyzer skips the import-dependent replacement for `before` rather than
assuming the outer import remains visible.

## Testing

### State-transfer unit tests

Small tests exercise `KnownImports` invalidation/intersection and the function
local-name prepass without deriving expected values through the production
resolver. They cover direct imports, module aliases, identical re-imports,
competing imports, branch joins, and nested-scope isolation.

### Analyzer regressions

Hand-written exact candidate vectors cover:

- assignment before and after an annotation;
- class and function definitions that reuse a direct imported name;
- a competing import followed by a safe re-import;
- `import typing as t` followed by rebinding `t`;
- function parameters and local assignments shadowing outer imports;
- nested function and class scope isolation;
- conditional and loop rebinding;
- unshadowed direct and qualified imports retaining current preferred
  spellings;
- reverse mutations being skipped when their source name no longer resolves
  to the intended typing symbol;
- every emitted replacement reparsing successfully.

Tests assert exact original, replacement, operator, span, and symbol where the
scope matters. They also assert non-empty unaffected candidates so an overly
broad suppression cannot pass vacuously.

### Integration regression

An analyzer integration fixture selects `type_list_sequence` for a file with
one annotation before and one after a direct-name or module-alias rebinding.
The streamed inventory contains only the safe source-order candidate and
retains its canonical descriptor. This verifies that the site snapshot reaches
the production analyzer protocol rather than only a helper function.

## Documentation and Delivery

README type-operator guidance states that replacement spellings are emitted
only while the corresponding import remains unshadowed at the annotation
site. `docs/development.md` records the source-order snapshot, function-local
prepass, and conservative control-flow join invariants.

The design, implementation plan, source changes, tests, and documentation live
in the Issue #264 worktree and one pull request. Pure documentation commits use
`[skip ci]`; the final branch contains a non-skip commit so GitHub Actions
validates the complete tree. The PR contains `Closes #264` and is squash-merged
after every required check succeeds.
