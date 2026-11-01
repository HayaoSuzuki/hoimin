# Augmented assignment to assignment (#701)

Opt-in augmented_to_assignment applies to AugAssign with an ExprName target.
Replace the sole augmented-operator token with =. Preserve target, RHS, spaces,
comments, parentheses and statement boundaries. All 13 Python augmented operators
are supported. Attribute/subscript targets remain excluded. This removes old-value
lookup and in-place behavior; it is not a numeric-only or side-effect-preserving edit.
Existing augmented operator substitutions coexist without changing their IDs.

## Design self-review

1. Fault: replacing += with = loses the previous accumulator, unlike changing + to -.
2. Scope: an AST Name target excludes property/index lookup and duplicate evaluation.
3. Span: tokens between target end and RHS start identify the operator even when
   target/RHS is parenthesized; strings/comments cannot supply matching lexer tokens.
4. Integration: opt-in Arithmetic rank, existing selector/profile/retention pipeline;
   append enum ID to preserve default selection and prior IDs.
5. Proof: Lean models accumulator loss, one-step equality and two-step distinction;
   public CPython tests check token/source correspondence separately.
