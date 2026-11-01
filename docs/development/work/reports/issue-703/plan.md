# String erasure implementation plan

Parent 625e68e; #702 cargo clean removed 4.2 GiB before worktree creation.

1. RED public tests for strings/escapes/source spans, docstrings/types/patterns and
   saved-plan type-only versus content assertion outcomes.
2. Add selected-only exclusion index; reuse annotation/context gates; append ID/rank.
3. Verify raw/u/triple, interpolation/concatenation, class/function/module roles,
   PEP613 aliases, limits/selectors/cancellation, full workspace, clippy and Lean.
4. Five implementation/test reviews, independent review, committed artifacts, stacked
   PR and cargo clean/scratch deletion before next issue.

## Plan self-review

1. Use exact candidate source text to distinguish duplicate string occurrences.
2. Include a syntactically nonempty token whose decoded value is empty.
3. Compare docstring and later standalone string in every relevant definition scope.
4. Test normal annotated RHS alongside both explicit alias syntaxes.
5. Saved plan verifies actual baseline/mutant outcomes, not plan generation alone.
