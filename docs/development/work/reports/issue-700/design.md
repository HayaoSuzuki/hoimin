# Enum member replacement (#700)

Opt-in enum_member_replace indexes top-level classes directly inheriting a trusted
stdlib Enum, IntEnum or StrEnum import (including aliases). Each Load reference
replaces only the attribute identifier with each first-declared representative of
a different value. Cache definitions once. Alias references are accepted, but
same-value destinations and duplicate alias destinations are excluded.

Supported bodies contain homogeneous integer or string literals, or all standard
auto() calls; methods/property definitions are not members. Conservatively reject
custom decorators/metaclasses, custom allocation/generation, mixed/dynamic values,
reserved configuration including _ignore_, and ambiguous bindings with a diagnostic.
Rejecting the whole _ignore_ class avoids accidentally retaining ignored members.
StrEnum auto names use ASCII lowercase initially; non-ASCII auto names are diagnosed
rather than approximating Python's Unicode lowercase rules. Signed integer magnitude
must fit u64, matching the parser's exact bounded value access.

A separate resolution index tracks module enum/import names without changing the
existing builtin resolver. Require exactly one unconditional module binding; local,
parameter, comprehension, nonlocal, wildcard, class-namespace or global write ambiguity
excludes references. Definitions never mutate themselves. Imports are never executed.
Explicit dynamic namespace operations and attribute writes invalidate the index.

## Design self-review

1. Identity: imports/classes need both a unique module binding and a valid load scope;
   source spelling alone cannot distinguish shadowed parameters or reassigned imports.
2. Values: canonical destinations are grouped by exact value, not number of auto calls;
   StrEnum A/a aliases collapse. Mixed and unsupported values yield diagnostics.
3. Syntax: change only attribute token, retaining parentheses, comments and Unicode
   byte spans; skip annotations, patterns and Store/Del contexts.
4. Cost: index once, stream destinations through bounded candidate retention, poll
   cancellation during definition scan and destination enumeration.
5. Proof: Lean proves distinct-value destination filtering and fixed alias examples;
   CPython tests establish parser/value correspondence separately (model-only proof).

Review corrections: preserve original identifier spelling independently of normalized
lookup names (a fullwidth spelling can normalize to a Python keyword). Reject U+FFFD
in decoded string values because Ruff replaces surrogate escapes with that value.
Tracked global declarations, alias assignments and namespace dictionary access also
invalidate identity. These deliberate false negatives receive definition diagnostics.
Canonical groups and alias lookup are cached with hash maps; per-reference buffering
is bounded at max_candidates + 1 and cancellation is checked during enumeration.

Bare loads of the imported enum module also invalidate the index, covering typed,
walrus and unpacking aliases. Only direct module-attribute receivers are allowed.
