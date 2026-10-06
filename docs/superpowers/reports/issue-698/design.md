# Constant if/elif conditions (#698)

Opt-in condition_constant emits True and False for the whole if/elif test, excluding
literal booleans and any nested named/await/yield/yield-from node. Reuse #696's
cancellable removal eligibility. Preserve bodies, else clauses and all other
operators. Conditions themselves are not evaluated after replacement; this removes
their side effects. While/assert/ternary/comprehension predicates are out of scope.
Reuse byte-span mapping, selection/profile, prefix bounds, ranking and IDs.

## Design self-review

1. Coverage: Ruff represents elif in elif_else_clauses; inspect each optional test.
2. Eligibility: exclude bool literals even under parentheses to avoid boolean_literal
   duplication; recursively reject binding/suspension constructs using RemovalCheck.
3. Span: test range replaces expression only, preserving colon/outer parentheses.
   Use (True)/(False) so keyword-adjacent forms such as if[] remain separated.
4. Semantics: side effects disappear; body/else text remains, with fixed branch
   selection. Focused main-guard exclusion remains shared retained_by_profile.
5. Integration: append enum, HighValueControl ranking, no default changes; model
   branch selection separately from Rust parsing and source rewriting.

Model-only correspondence: constant flag -> literal AST classification; safe flag
-> recursive removal gate; requested Bool -> True/False source. The Lean model
proves gate exclusions, chosen branch and absence of condition effects, not AST
classification or full Python semantics. Atomicity/replay are not relevant.
