# Issue #558: imported names in deferred annotations

## Intended outcome

Do not emit a type pair whose imported source or destination has been rebound
before Python evaluates the annotation. Preserve useful candidates for stable
imports. The issue explicitly permits documented conservative exclusions for
cached annotations and restored imports; no interpreter-version option exists.

## Root cause and selected design

`AnnotationCollector` correctly models source-order import provenance, but its
snapshot alone cannot establish provenance at a deferred evaluation. Keep that
flow analysis and add a second condition: an imported spelling must have no later
binding in the lexical scopes visible to the annotation. Reuse the scope tree in
`NameResolutionIndex`, recording evaluation-event positions for imported names. Annotation byte offsets
identify sites; stability compares binding events with the site event. This uses
the event ordering introduced by the stacked #560 change. Both source
resolution and destination selection use this condition lazily through an
annotation import view; scalar annotations do not scan unrelated imports. A removed module alias
must not fall back to its literal spelling (`typing.Sequence`). Before operator
eligibility, inspect referenced annotation AST names and reject any unstable
import, including names inside nested type arguments. Losing provenance must not
erase a known prohibition such as `typing.Any` and admit a new candidate.

This is preferable to replacing snapshots with the final module environment:
annotations can be evaluated and cached before the final statement. A precise
interpreter for all annotation accesses, decorators, imports and caches is beyond
this bounded correction. Whole-file invalidation would also reject unrelated
function locals and imports restored before the annotation; lexical filtering
retains those cases. The existing builtin checks remain independent.

## Timing and precision contract

Python 3.14 ordinary function/module/class annotations defer evaluation until
access. A cached `f.__annotations__` retains its value across later rebinding.
Python 3.13 and older ordinarily evaluate these annotations eagerly. Future
annotations store strings; a later `get_type_hints` evaluation has its own lookup
context. PEP 695 aliases and generic bounds/constraints are lazy from 3.12;
generic defaults are lazy from 3.13. Function-local variable annotations are not
runtime evaluated. See the official [annotation semantics](https://docs.python.org/3.14/library/annotationlib.html#annotation-semantics)
and [lazy evaluation](https://docs.python.org/3.14/reference/executionmodel.html#lazy-evaluation).

The analyzer does not infer a target interpreter. Apply the deferred safety
condition uniformly to runtime annotations, retaining source-order provenance as
a necessary condition. Function-local variable annotations, which Python never
evaluates, retain the original source-order candidate policy.
A later rebind conservatively suppresses a candidate even when an earlier cache
access makes it safe, or a later reimport restores the binding before first use.
An import restored before the annotation remains usable when no later binding
exists. Stable imports, aliases, and unrelated-scope writes remain usable. This
supersedes #264's claim that later rebinding can never affect earlier candidates.
Callable global/nonlocal writes conservatively exclude the affected imported
name irrespective of textual order; valueless class/module annotations do not
create a local binding. No general proof of arbitrary external namespace mutation, custom annotation
functions, dynamic imports or all Python execution schedules is claimed.

## Validation

Public plan tests cover direct/module aliases from typing/collections.abc,
source/destination, class follow-up bindings, closures, aliases/bounds/defaults,
cache/restoration policy, stable positives and exact candidate spans. Python 3.14
executes representative sources and retained replacements. Public run must report
zero killed candidates for the reported invalid pair. Existing analyzer and
formal-oracle tests retain their binding-flow assertions; any candidate expectation
that depended on eager lookup is explicitly reconciled with this contract.

A control-flow join can discard a formerly imported name before an annotation.
The resolver retains imported spelling history independently from the trusted
snapshot, and rejects a referenced name with a visible binding owner when that
snapshot is absent. This also applies to unevaluated local annotations: the
source-order exception requires a known import. An import confined to a sibling
scope does not by itself invalidate an otherwise unbound builtin spelling.
