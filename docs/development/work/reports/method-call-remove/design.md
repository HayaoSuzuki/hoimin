# Method call removal design

Add opt-in `method_call_remove`: replace an attribute call with no positional or
keyword arguments by the parenthesized source of its receiver, e.g.
`text.strip()` -> `(text)`, `factory().clean()` -> `(factory())`. The receiver is
retained once; the attribute lookup (including descriptor effects) and invocation
are deliberately removed. This is a syntactic attribute-call operator: static
analysis does not establish that the callable is a bound method or preserve type.

Only runtime value contexts qualify. Exclude annotations, explicit type aliases,
patterns and assignment-target subtrees. Conservatively reject receivers containing
named expressions, await/yield/yield-from or generator expressions, including nested
occurrences. Calls with arguments/keywords/star expansion are out of initial scope.
Nested eligible calls yield separate mutants rather than combined mutations.

Reuse existing source-byte span/encoding, profile, selection, candidate identity,
bounded collection, cancellation, ranking (behavioral 80), plan/preview/verify and
report paths. Keep the 50 default operators and all_legacy unchanged; `all` adds
this new explicit operator. No external data import or runtime instrumentation.

The source is [Hybrid Fault-Driven Mutation Testing for Python, §2.5.3](https://arxiv.org/html/2601.19088v2#S2.SS5).
The paper uses dynamic analysis; this restricted static adaptation must be evaluated
independently. Record observable gaps in authored checks and small real-source trials;
never interpret syntax failures, type errors or survivor counts alone as benefit.

## Design self-reviews

1. Evaluation: copy receiver source once and parenthesize it; delete lookup/call effects intentionally, including custom descriptors. No purity or runtime-method guarantee.
2. Eligibility: zero arguments includes zero keywords; reused recursive guard rejects binding/suspension/generator constructs, with role and alias exclusions.
3. Source: whole call is replaced, parentheses preserve precedence, byte spans preserve outer source and encoding. Nested calls remain independent candidates.
4. Integration: only explicit operator/all change; default/all_legacy/ranking rules for old candidates stay fixed. Current schemas and inventory tests must accept the new name.
5. Evidence: Lean models evaluation/state separately from bounded public candidate correspondence; CPython tests check source validity and meaningful assertion gaps, not general effectiveness.
