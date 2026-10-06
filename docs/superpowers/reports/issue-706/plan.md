# Container deletion implementation plan

Parent bbd7650; #705 cargo clean removed 4.3 GiB.

1. RED public tests for all kinds/positions, singleton/duplicate/nested/source cases.
2. Add streaming literal edit module, runtime target context and shared PEP613 role index.
3. Add opt-in ID/ranking, selectors/bounds/cancel and saved payload weak/strong probes.
4. Lean model, workspace/clippy, real-project trials, five implementation/test reviews
   and independent review; commit documents/PR, link stack, clean before #707.

## Plan self-review

1. Evaluate mutants to assert container types, not only compile them.
2. Distinguish a single tuple element from a parenthesized scalar.
3. Include comma-containing strings and nested literals; verify exact-edit deduplication.
4. Pair nonruntime exclusions with eligible normal annotations and subscription values.
5. Weak payload probe checks user_id; strong probe additionally requires enabled.
