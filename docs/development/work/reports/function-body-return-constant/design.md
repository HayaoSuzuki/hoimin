# Value-returning function extreme mutation

Add opt-in `function_body_return_constant`. For a plain synchronous non-dunder
function with a direct builtin return annotation, replace the whole body after an
optional leading docstring with `return False` / `return True` (bool), `return 0` /
`return 1` (int), or `return ""` / `return "A"` (str). Headers, decorators, defaults,
docstrings and outer bytes remain unchanged. Annotation selection is a static
heuristic, not a guarantee of actual return type.

Use the existing deferred-annotation name resolver, requiring DefinitelyBuiltin.
Exclude quoted/aliased/qualified/composite/missing annotations, shadowed or ambiguous
builtins, async/generator/dunder functions, and bodies without an own-scope non-None
return. Nested function/class/lambda bodies neither qualify the outer function nor
make it a generator; immediately evaluated nested defaults/decorators still matter.
Skip a counterpart when the sole post-docstring statement returns that exact literal
value (bool/int distinguished; cooked strings and parenthesized literals recognized).
Other side effects are deliberately removed. Existing void `function_body_erase`
remains separate. Preserve shared span/encoding, selection, profile, limit, cancellation,
ID, ranking (behavioral 80), saved-plan and reporting behavior. Defaults remain 50.

Source: [Will My Tests Tell Me If I Break This Code?, §3](https://arxiv.org/pdf/1611.07163).
Its Java extreme mutants use constant returns; this Python adaptation initially
supports three explicit builtin annotations. Without coverage evidence, never label
a survivor pseudo-tested. External coverage/mutant ingestion stays out of scope.

## Design self-reviews

1. Mutation semantics: deliberately erase body effects, return a typed literal; do not infer behavior preservation or actual runtime type from an annotation.
2. Scope: require an own-scope value return; exclude async/yield and dunder, while nested bodies remain independent. Docstring is retained before the replacement.
3. Name resolution: Python 3.14 annotations may resolve late; use annotation_resolution including later writes/type parameters/custom class namespace, not ordinary definition-time lookup.
4. Candidate usefulness: complementary constants limit count to two per function; remove obvious sole-literal no-ops by AST value and exact literal kind, not Python bool/int equality.
5. Evidence: original paper informs the family, not Python effectiveness. Lean model/corpus, CPython, saved-plan weak/strong assertions and bounded source trials provide separate evidence with explicit limits.
