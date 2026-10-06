# Two-element return tuple swap (#702)

Opt-in return_tuple_swap targets direct Return tuples with exactly two simple
names or standalone number/string/Boolean/None literals in synchronous non-generator
functions. Unary signs on numeric literals are included; bytes/fstrings, implicit
string concatenation, nested containers, calls/attributes/subscripts, starred and
assignment/suspension expressions are excluded. No type-preservation claim.

Reuse the own-scope suspension walk from function_body_erase, parameterizing whether
value returns are rejected. Nested bodies are skipped but defaults/decorators/bases
are scanned in the enclosing scope. Each FunctionDef pushes/restores return eligibility.

Swap only AST element spans inside one contiguous tuple span, preserving every byte
between/around those spans. Preserve outer parentheses, trailing comma and comments.
If an unparenthesized tuple directly adjoins return (e.g. return'x', name), wrap the
replacement in parentheses to keep the keyword lexically separate. Skip identical
source element text and identical normalized names; do not infer general equivalence.

## Design self-review

1. Fault model: two positions carry separate meanings; names need not have equal types.
2. Eligibility: simple tokens avoid calls/attribute evaluation order; signed numeric
   constants are safe literals, but nested tuples and concatenated strings are not.
3. Scope: generator determination must see nested evaluated defaults, but ignore
   nested bodies; async functions are excluded independently.
4. Patch: source slices rather than comma splitting preserve string commas/comments;
   keyword-adjacent literals need a lexical separator after swapping to a name.
5. Integration/proof: use one candidate/span through shared selectors/limits. Lean
   proves pair-swap order/involution; parser and source correspondence are separate.
