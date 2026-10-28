# Revised methods for issues 1126, 1127, 1128 and 1131

Historical method review preceding implementation. The maintainer subsequently
authorized all four additions in the same PR; see the
[current implementation](four-implementation.md). Its implementation and completion
status supersede the temporary open/undecided status below.

## Scope and correction

The maintainer asked to proceed first with 1125, 1129 and 1130, and reconsider
methods for the other four. PR 1134 therefore completes only those three product
implementations. The other issues remain open. This supersedes the initial
proposal to close all seven, while preserving the original bounded measurements.

The original evaluation mixed different judgments: observed usefulness,
implementation uncertainty, and overlap on one example. They do not all imply
low value. In particular factory sharing and encoding loss have useful assertion
leads absent from the nearest existing operators in the recorded examples.
An existing helper's eligibility restriction is an implementation boundary,
not evidence that no appropriately restricted implementation is possible.

These are proposed follow-up methods with fixed feasibility probes. No additional
Rust operators, selectors or defaults are implemented by this reconsideration.
Adopted future operators should follow the requested default-on/opt-out policy.

## 1126: sharing factories in ordinary dataclasses

Start with a top-level class with no explicit bases or metaclass, one statically
identified dataclasses decorator, and direct annotated field assignments.
Recognize a unique top-level `import dataclasses as dc` or explicit imported
aliases. Reject uncertain/rebound identities, dynamic class namespace writes,
shadowed factory names and class-local alias conflicts. Arbitrary bases,
metaclasses, additional decorators, nested classes and arbitrary factories are
outside the first implementation.

The existing CallableImports resolver should retain its class-load rejection.
A dataclass-specific eligibility analysis can establish an ordinary class
namespace and absence of conflicting bindings, then obtain imported-callable
identity through a separately guarded path. Do not globally relax resolve().
Both namespace evidence and imported identity must be established; syntactically
seeing `field` or `dataclass` is insufficient. Exact ownership of that guarded
resolver interface remains an implementation design task, with negative tests
for class rebinding and namespace customization before use.

For known builtin list/dict/set factories, replace only the factory expression:

```python
field(default_factory=lambda shared=[]: shared)
field(default_factory=lambda shared={}: shared)
field(default_factory=lambda shared={*()}: shared)
```

The shared parameter is local to the generated lambda. Empty-set literal syntax
avoids introducing a new builtin lookup. Existing field options and annotations
remain unchanged. Factory creation moves the known empty-container allocation to
class definition, intentionally inducing sharing; arbitrary factories must not
have their effects moved. Direct introspection/invocation of the factory itself
is a separate observable effect, not an interface-preservation promise.

Three fixed probes confirm class construction succeeds and list/dict/set state
can leak between instances, with weak success and strong assertion failure.
They do not validate a Ruff class-identity analyzer. Product acceptance requires
positive aliases/field options and negative metaclass/base/decorator/rebinding
cases, bounded admission, cancellation, grouping/trivia and Python parsing.

## 1127: swap a two-component sorting key

Start with an unshadowed builtin sorted call and an explicit key lambda returning
exactly two distinct simple reads. Leave list.sort recognition for a later scope
if it adds useful coverage. Exclude starred elements, calls, yields/await and
compound key expressions initially. Reuse byte-span swapping so separators,
comments and parentheses remain valid. Emit one candidate, with each component
still evaluated once.

Attribute/subscript reads can execute user code; excluding explicit calls does
not prove purity. Reordering component evaluation is part of this mutation,
not a reason to claim its behavior is universally safe or equivalent. Compare
that cost with existing structurally similar mutations under one consistent
policy instead of demanding whole-program purity only from this proposal.

The original tuple-deletion overlap is real, but one fixture does not establish
redundancy. Priority exchange preserves the two-part key while deletion discards
one part. Re-evaluate distinct API requirements (primary order, tie-breaking,
stability and key evaluation) and candidate cost, reporting shared detections
rather than forcing a unique test defect per operator. The fixed probe confirms
priority failure while its same-primary tie-order check still passes. It is not
a new equal-budget incumbent experiment or proof of incremental real-project
value. A product decision remains open.

## 1128: preserve calls while making an omitted capture late-bound

Removing the parameter was unnecessarily destructive. For the first simple
`lambda value=value: value` shape inside a matching ordinary for-loop, use a
private sentinel and getter closure:

```python
(lambda _get, _missing:
    lambda value=_missing: _get() if value is _missing else value
)(lambda: value, [])
```

The getter is constructed in the original enclosing scope, so it observes the
loop variable late. The returned callback retains its original parameter name,
positional/keyword acceptance and explicit override values, including None and
an explicit value equal to the original default. The empty list is a unique
private sentinel. There are no imports, external helpers or builtin lookups.
Generate wrapper-local names distinct from the public parameter; do not leak
new bindings to the module/loop scope.

Restrict the first producer to exactly one ordinary parameter, one same-name
Name default, a matching Name body, and a same-scope simple for target. Exclude
comprehensions, nested-scope binding mismatches, complex bodies/defaults,
variadics and positional-only/keyword-only variants until separately specified.
No whole-program proof of zero-argument callers is required by this method.

The wrapper allocates extra closures and a sentinel per callback. Defaults,
closure metadata and traceback shape change; full inspect.signature/default
identity preservation is not promised. These are explicit tradeoffs to compare
against simple parameter removal. Three probes (including helper-name collisions)
confirm omitted callbacks lose distinct values while explicit positional,
keyword-None and mutable-object overrides continue to work. Product emission,
source edits, limits/cancellation and further scope negatives remain to implement.

## 1131: explicit encoding-policy mutation under a consistent API policy

Start with explicit `errors='surrogateescape'` on recognized decode/encode calls,
changing only that literal to 'ignore'. Trusted codecs.encode/decode imports can
use existing imported-callable identity rules; keyword-only errors matching
avoids positional signature ambiguity in the first version.

For byte/string method syntax, matching a simple receiver's `.decode`/`.encode`
with the explicit keyword is a nominal API rule, like existing receiver-method
mutations. Receiver type inference is not a universal prerequisite for every
such operator. State this honestly: a custom same-named method can match, and
the resulting survivor does not establish a test defect. Alternatively start
with trusted codecs calls, with the smaller coverage clearly documented.
Do not replace arbitrary errors keywords or unrelated string literals.

The fixed method and codecs probes both retain ASCII behavior and fail the
lossless invalid-byte round-trip assertion through data loss, not an exception.
This resolves the argument that parameter receivers leave no workable method:
codecs functions accept those parameters directly, and nominal method matching
is another explicit scope choice. Product tests still need alias/shadowing,
unrelated APIs, grouping, annotation exclusion and separate encode/decode cases.
Decode and encode mutations exposing one requirement count as shared evidence,
not two independent test defects.

## Evidence and next gate

Run the [fixed probe](reconsideration_probe.py) with a result path. The
[3.14.9 results](reconsideration-python314.json) and
[3.15.0b1 beta results](reconsideration-python315.json) record sources, edits,
checks, actual verdicts, interpreter/platform, revision and check timings.
All nine originals pass every check; mutants pass weak/interface controls and
fail the intended strong assertion on both interpreters. These are fixed edits,
not production discovery or new incumbent comparisons. Processes have a
five-second timeout, emit bounded fixed-corpus output and create no target files.

Next implementation priority is factory sharing and encoding loss, where the
initial examples showed clear additional behavioral leads. Capture's revised
call interface is now feasible and merits a product-cost comparison. Sorting
still warrants evaluation without treating a single shared detection as a veto.
None of the four issues is completed or auto-closed by PR 1134.
