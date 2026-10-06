# While condition false implementation plan

Parent 22483e4; #703 cargo clean removed 4.2 GiB before this worktree.

1. Add public RED tests for source forms, exclusions and saved queue-processing plans.
2. Add selected-only while collector and operator inventory/rank entry.
3. Test selectors, cancellation, limits, nested loops, condition effects and else;
   check Lean model, workspace tests, formatting and clippy.
4. Perform five implementation and test reviews, independent review, commit docs,
   create stacked PR, then cargo clean and delete issue scratch before next issue.

## Plan self-review

1. Compile every generated source variant with CPython 3.14.
2. Assert exact one-candidate-per-loop count, including nested loops.
3. Use excluded boolean tests alongside boolean_literal to detect duplicates.
4. Execute a mutant whose original condition records effects; assert no effects and else.
5. Verify the same saved-plan workflow with weak completion and strong value assertions.
