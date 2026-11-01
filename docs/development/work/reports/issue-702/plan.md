# Return tuple swap plan

Parent 4b37fcb; #701 cargo clean removed 4.1 GiB before worktree creation.

1. RED public fixtures: tuple forms, comments/Unicode, exclusions and scope matrix.
2. Reuse own-scope scanner with value-return policy; add tuple patch and registration.
3. Check keyword adjacency, nested default/body suspension, exact bytes, selectors,
   limits/cancellation, saved-plan weak/strong pair and full Rust checks; Lean proof.
4. Five implementation/test reviews, independent review, committed artifacts and PR;
   cargo clean and scratch removal before next issue.

## Plan self-review

1. Include both direct and parenthesized tuples, trailing commas and literal commas.
2. Distinguish own/nested yield and nested-header yield to prevent scope regression.
3. Assert preserved separators and exact original/replacement from independent strings.
4. Demonstrate length-only survival and ordered-value kill using saved-plan verify.
5. Re-run body-erasure coverage because its scanner is shared with this addition.
