# Implementation plan

Parent c7920e0; #700 cargo clean removed 4.1 GiB before worktree creation.

1. RED public CLI cases: all operators, excluded targets, coexistence and saved plan.
2. Implement AST AugAssign/Name check and token-only edit; register opt-in ID/rank.
3. Test multiline/semicolon/Unicode, exact span, weak/strong behavior, selector/limit
   and cancellation; run full workspace and clippy; Lean model with deadline.
4. Five implementation/test reviews, independent review, commit source/docs/proof,
   create/link PR and clean binaries/scratch before proceeding.

## Plan self-review

1. Assert all 13 spellings, including floor/power/shift/matrix, instead of one example.
2. Check existing -= candidate coexists at the same span under its original operator.
3. Use a single-element versus different multi-element total to expose missed state.
4. Compare unchanged prefix/suffix to ensure whitespace/comments are not reconstructed.
5. Explicit default/selector/cap checks complement public saved-plan verification.
