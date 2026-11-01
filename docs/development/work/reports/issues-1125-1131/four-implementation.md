# Implementation of the four reconsidered behavioral operators

The maintainer authorized implementation of 1126, 1127, 1128 and 1131 and then
specified inclusion in the same PR 1134. This implements the methods from the
[reconsideration](reconsideration.md), superseding its temporary open/undecided
status. PR 1134 now implements all seven issues, rather than closing four solely
on research conclusions. The original comparison JSON remains historical base
binary evidence; it is not relabeled as a new default-profile benchmark.

## Catalog and admission

Four IDs are appended: dataclass_factory_share, sort_key_swap,
lambda_capture_late and encoding_errors_ignore. There are 101 catalog IDs and
82 defaults, with individual --operators/--exclude-operators control. Existing
selector families, ranking version, schemas and supported Python requirements
are unchanged. New IDs use existing ranking categories. Each producer uses the
same payload admission, capped prefix, line/symbol selection, profiles and
cancellation. No dependency, project import, runtime state manager or legacy
reader is added. The original Lean cross-producer corpus explicitly marks these
as deferred from that model; existing proofs are preserved.

## Factory ownership (1126)

A dedicated producer checks direct top-level ordinary dataclasses. A guarded
CallableImports path establishes a trusted dataclass decorator and a simple
class namespace before permitting a field-call identity check. Existing resolve
still rejects ordinary class-namespace loads. Bases, metaclasses, type parameters,
additional decorators and class suites containing methods, assignments, control
flow or nested classes are initially excluded. Only direct annotated name fields,
pass and string literals are allowed; private/duplicate fields and conflicting
class bindings are excluded. Import aliases are supported under existing unique
binding/escape/namespace rules.

Builtin list/dict/set factory values are replaced by lambda-local shared empty
containers. Field options, grouping and comments stay intact; no global helper
or new builtin lookup is introduced. The set default uses {*()}. Arbitrary
factories, dynamic expansions, positional field arguments and simultaneous
default/default_factory are excluded. Factory allocation/metadata changes are
intentional. A later module rebinding after the class has evaluated does not
invalidate the original eager factory identity; the ordered builtin resolver
correctly permits that case.

Only class ranges are indexed, and one checked namespace is built per eligible
class rather than rescanning the entire class for each field. The initial body
restriction is explicit; support for methods/custom class setup is not implied.

## Sorting priority (1127)

The call builder requires proven builtin sorted with one iterable argument and
an explicit key lambda returning exactly two distinct simple reads. Supported
reads are names, attribute chains and literal-index subscripts. It excludes
calls, compound/constant components, starred elements, variable/slice indices,
duplicate reads, named key functions and list.sort. Token-expanded operand spans
retain grouping and source separators/comments. Selection checks the actual
edit-start line before replacement allocation, including multiline calls. Each component is evaluated once,
but its order changes; property/subscript user code is not claimed to be pure.
The earlier overlap with tuple deletion is recorded, not used as a universal
rejection criterion.

## Late capture with explicit overrides (1128)

A focused static visitor tracks same-scope for targets. It recognizes exactly
one ordinary parameter with a same-name Name default and Name body. It emits a
wrapper-local getter/sentinel lambda, preserving the original parameter spelling,
positional/keyword acceptance and explicit values (including None, an equal
value and a mutable object). Helper names avoid collisions with the public
parameter. The getter is constructed in the original enclosing scope and reads
late only when the returned callback's parameter is omitted.

Functions/classes reset loop context, and class-body loops are excluded because
a getter cannot close over class attributes; method-local loops remain eligible.
Nested lambdas and comprehensions are not
traversed for this producer. Async/unpacking bindings mask a matching outer name;
a differently named inner loop can retain an outer simple capture. Loop else,
complex defaults/bodies, positional-only/keyword-only and variadic parameters
are excluded. Existing annotation/type-alias exclusions remain. Edits preserve
original parameter/body grouping and comments, replacing two Name spans before
wrapping the existing lambda. New closures/sentinel allocation and changed
metadata/traceback shape are documented costs, not introspection compatibility.

The visitor retains borrowed loop targets only, checks selection before making
source-sized replacements, polls cancellation through target scans and stops
when admission fails. It does not retain an unbounded candidate vector.

## Encoding loss (1131)

The explicit errors literal surrogateescape becomes ignore for trusted
codecs.encode/decode aliases or nominal encode/decode methods on simple
Name/Attribute receivers. Receiver type proof is not required uniquely for this
method operator; custom same-named methods may match. Uncertain imported codecs
references do not fall back to nominal matching. An exclusion-only alias scan
retains module spellings even when dynamic namespace uncertainty invalidates
import identity. Files without codecs imports skip the imported-callable index.

Dynamic argument expansion, duplicate keywords, positional errors, excessive
positional arguments, computed receivers, implicit concatenation and f-strings
are excluded. Unrelated call shapes are rejected before keyword-set allocation.
Encoding and decoding mutants may expose the same losslessness requirement.
Syntax/runtime errors are not credited as useful assertion leads in research;
product contracts demonstrate successful execution with changed data.

## Verification and evidence boundary

Four default-positive analyzer tests failed with zero candidates before these
producers existed, then passed. Dedicated boundaries cover aliases, shadows,
UTF-8/trivia, class customization, nested scopes, explicit overrides, limits,
selection, profiles and cancellation. Public plan/verify tests check original
baseline success, weak survival, stronger kills, complete counts and unchanged
original files. CPython contracts additionally observe list/dict/set isolation,
sort order/read counts, callback override identity/helper collisions, and both
encoding directions/trusted aliases. See the [review record](four-implementation-review.md)
for actual final commands and outcomes. No real-project throughput, peak-memory,
universal effectiveness or new formal-proof claim is made.
