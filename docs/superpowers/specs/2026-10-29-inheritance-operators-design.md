# Override deletion and super-call movement

Implement the remaining two useful operators requested by the user, default-on
with individual opt-out. The original issue-1123 report's shared inheritance
boundary and adoption checks are the specification; this document records the
implementation choices left to the builder.

Use one shared index, built only if either operator is selected. Recognize direct
top-level child/earlier-parent bindings in the same module: undecorated,
non-generic classes without class keywords, a parent with no explicit bases and
one simple-name base on the child. Reuse the ordered name-resolution builder
with class names and super tracked explicitly. Reject rebinding, class escapes,
method aliases/writes, wildcard imports and explicit dynamic namespace operations.
This is a conservative source model, not a proof against external monkey-patching.

Class suites contain methods, docstrings/pass and simple-name bindings to literal
values; bare-name values and decorated methods can invoke descriptors and are
excluded. Class-evaluated method headers must not capture class methods;
duplicate namespace bindings and lookup/subclass hooks are unsupported. Loads
of a tracked class as a base are exempt from escape checks only for supported
ordinary sole-base relationships. Candidate
methods are ordinary undecorated synchronous instance methods, self first,
matching parameter kinds/names, no defaults/annotations/variadics, special/private
methods, nested definitions or suspension. Cache method checks once per class.

`override_method_delete` replaces the whole child definition with pass. Parent,
class docstring and neighboring definitions remain unchanged. Emit one candidate.

`super_call_move` requires a single unshadowed builtin zero-argument super call,
delegating to the same method in a standalone direct-body expression. All moved
statements are simple expressions without walrus/suspension/star expansion.
Move first delegation to last, otherwise to first; retain the initial docstring
and reject edits that would create a new docstring. Substitute statement source
fragments into the existing separator slots, preserving comments/newlines/grouping.
Comments remain in their source slots. Emit one candidate per method.

Preselect the actual edited spans before building replacements; use existing
candidate identity, prefix caps, payload leases, cancellation, ranking and saved
plans. Keep these IDs out of selector families. Append identities/defaults;
catalog/default counts become 104/85. Preserve historical research receipts.
