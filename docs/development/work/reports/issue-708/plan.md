# Optional keyword deletion implementation plan

Parent 3c2a0fd; previous issue cargo clean removed 4.4 GiB.

1. RED public tests for positions/multiline/Unicode, signature validation and exclusions.
2. Factor shared unique-module resolution without changing enum behavior; add selected
   function-signature/hazard index and source reconstruction, then ID/rank/inventory.
3. Saved escaping fixture, default-object/evaluation-order tests; selector/bounds/cancel
   regression plus enum regression. Run Lean, full workspace, clippy and real projects.
4. Five implementation/test reviews and independent review. Commit all documents/proof,
   create/link final PR, clean artifacts and verify all issue worktrees/stack statuses.

## Plan self-review

1. Require every positive source variant and generated mutant to compile in CPython 3.14.
2. Test invalid original calls rather than assuming keyword presence means valid binding.
3. Pair weak successful string-return check with exact escaping behavior assertion.
4. Assert factory default is created once and removed argument effects disappear.
5. Test known rebinding/escape and call-order limits explicitly; document exclusions and
   unexecuted/equivalence-unknown real-project cases instead of treating them as success.
