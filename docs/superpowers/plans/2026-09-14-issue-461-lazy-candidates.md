# Issue 461 Lazy Candidate Strings Implementation Plan

**Goal:** Avoid constructing owned original and large collection replacement strings for candidates rejected by operator or target selection.

**Architecture:** Guard expensive collection helpers at their AST collection sites and order cheap eligibility checks before source cloning in `make_candidate`. Test-only counters observe the real allocation sites.

**Spec:** `docs/superpowers/specs/2026-09-14-issue-461-lazy-candidates-design.md`

### Task 1: Establish the cost regression

- [x] Add counters at collection replacement construction and original cloning.
- [x] Add a deep large-list fixture followed by a selected binary operator.
- [x] Confirm RED: old code reports `(64, 65)` rather than `(0, 1)`.

### Task 2: Move eligibility checks

- [x] Guard `collect_list_literal` and `collect_tuple_literal` before replacement helpers without pruning child visits.
- [x] In `make_candidate`, check operator, range, conversions, line and symbol before cloning original.
- [x] Confirm the focused test is GREEN and selected collection behavior remains covered.

### Task 3: Evidence and repository knowledge

- [x] Record repeatable old/new release measurements for depths 1, 30 and 100.
- [x] Update analyzer OKF and the design-document index with real source hash.
- [x] Run fmt, clippy, focused analyzer tests and workspace tests.
- [x] Record three self-reviews for OKF, design, plan, implementation, tests and PR.
- [ ] Commit, push and create a PR closing #461.

## Planセルフレビュー

1. 網羅性: issueの五条件をRED test、二つのguard、既存候補回帰、release実測、OKFへ対応付けた。
2. 実行性: 対象関数、観測counter、期待するRED値とGREEN値、専用target directoryを明示した。
3. 依存性: #470の位置索引を必要とせず `origin/main` から独立して実装する。将来統合時も変更箇所は `make_candidate` 近傍だけである。
