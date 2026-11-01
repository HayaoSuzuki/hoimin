# Nonempty string literal to empty (#703)

Opt-in string_literal_empty replaces a single-token nonempty str expression's whole
span with "". AST decoded value determines emptiness; raw/u/triple quotes and escapes
are included. Bytes, interpolated strings (including their fields), implicit adjacent
concatenation, annotations, patterns and docstrings are excluded.

Cache exclusion intervals for first string expressions in module/class/function
bodies and complete f/t strings. Shared annotation intervals exclude annotations and
PEP 695 alias expressions. Also conservatively exclude RHS of PEP 613 aliases marked
by TypeAlias imported from typing/typing_extensions, including import aliases. Marker
names are an over-approximation across scopes; shadowing may suppress a candidate.
Ordinary annotated assignment values remain runtime expressions. No function-name
heuristics exclude messages or hashing calls. Other string expression statements stay
eligible even when compilation optimizes them away; survival is not equivalence proof.

## Design self-review

1. Emptiness: decoded AST value, not source token length, handles escaped line joins
   and empty raw/triple strings. Replacement always represents an empty str.
2. Role: only first expression in actual definition/module bodies is a docstring;
   strings first in an if/loop/try body and later standalone strings remain eligible.
3. Types: shared annotation/PEP695 ranges plus conservative imported PEP613 markers
   exclude type expressions without suppressing normal x: str = 'value'.
4. Interpolation/source: skip whole f/t strings; replace one literal token/span,
   retaining surrounding parentheses/comments/newlines and unaffected equal strings.
5. Integration/proof: cache role intervals only when selected, poll cancellation,
   reuse selectors/retention. Lean proves nonempty-to-empty properties in a model;
   CPython tests check concrete parsing and exact source spans separately.

Quoted PEP613 markers are normalized conservatively for whitespace, grouping and
comments. This is a linear spelling normalization inside an already recognized
annotation role, not arbitrary forward-reference evaluation; false exclusions are
possible for unusual annotations or shadowed marker names.
