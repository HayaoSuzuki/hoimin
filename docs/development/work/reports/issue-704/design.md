# While condition false (#704)

Opt-in while_condition_false replaces the complete while test with (False).
The condition is not evaluated, the body is skipped and the existing else suite runs.
Boolean constants and any nested named expression, await, yield or yield-from are
excluded. Other calls remain eligible, including calls with side effects. No for,
comprehension or if conditions are included. Parentheses prevent keyword adjacency
from joining False to while. Shared candidate retention, selectors and cancellation apply.

## Design self-review

1. Source boundary: use the test AST range, retaining body, else, comments outside it.
2. Syntax: (False) remains valid after while without whitespace and across multiline tests.
3. Semantics: deleting a condition also deletes its calls; preserve else execution explicitly.
4. Eligibility: exclude both boolean literals to avoid boolean_literal overlap, recursively
   exclude binding/suspension expressions with the existing deletion scanner.
5. Scope/proof: visit each nested loop separately; Lean models forced-false entry and
   effects only, while Rust/CPython tests establish implementation correspondence.
