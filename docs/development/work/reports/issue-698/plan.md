# Condition constants implementation plan

Spec: design.md. Parent: 93df0b4; #697 clean removed 3.9 GiB before this worktree.

- [x] Public CLI RED: if/elif whole-span candidates and nested exclusion boundaries.
- [x] Register condition_constant and collect eligible tests in visit_stmt before
  normal recursion, using removal checker and add_candidate.
- [x] CPython syntax/side-effect probes, saved-plan weak/strong test, focused/limit
  regression, Lean model, full suite and all-target/all-feature lint.
- [ ] Record 5 implementation and 5 test self-reviews; independent review, commit
  artifacts, create stacked PR, cargo clean and remove scratch before next issue.

## Plan self-review

1. Test expectations cover each clause independently; else has no condition candidate.
2. RED by selector string avoids compilation failure masquerading as behavioral RED.
3. Include named expression in nested lambda and await/yield inside condition.
4. Assert baseline and both outcome counts; timeout/error cannot satisfy killed.
5. Keep prior operators' regression suites; separate model proof and CPython checks.
