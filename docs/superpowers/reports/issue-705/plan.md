# Condition clause deletion implementation plan

Parent dbb9635; previous issue cargo clean removed 4.3 GiB.

1. RED tests: two/three conditions, mixed nesting, source/compile and excluded contexts.
2. Add if/elif collector, inventory/ranking and opt-in integration test.
3. Verify saved authorization fixture with one negative case versus both negative cases.
4. Run Lean, workspace, clippy, real-project probes; record five implementation/test
   reviews, independent review, commit docs and PR, link stack and clean artifacts.

## Plan self-review

1. Compare candidate originals/counts and evaluate retained operand meaning.
2. Include repeated operands to check exact-edit deduplication.
3. Include Unicode, CRLF, comments, compact keyword syntax and nested BoolOp.
4. Keep existing boolean_and_or candidates distinguishable by operator identity.
5. Exercise exclusions, selectors, bounded retention and cancellation with shared path.
