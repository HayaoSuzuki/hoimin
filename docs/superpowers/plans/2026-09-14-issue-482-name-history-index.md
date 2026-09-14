# Issue 482 Name History Index Plan

**Goal:** Avoid rescanning complete binding histories for ordinary source-ordered name lookups while preserving insertion-order fold semantics.

**Architecture:** Activate events by offset at insertion-order segment-tree leaves, snapshot the composed root after each equal-offset group, then binary-search snapshots.

**Spec:** `docs/superpowers/specs/2026-09-14-issue-482-name-history-index-design.md`

### Task 1: Independent semantics and cost tests

- [x] Implement an independent slow reference fold in tests.
- [x] Compare nonmonotonic, equal-offset and generated histories at every boundary.
- [x] Assert 4,096 preprocessing events, leaf and ancestor update operations, and logarithmic query-node visits.
- [x] Confirm the first node-visit bound was too tight and failed before correction to the two-boundary traversal bound.

### Task 2: Ordered history index

- [x] Preserve event insertion order in leaves and left-to-right transform composition.
- [x] Activate equal-offset groups at insertion-order leaves and snapshot the composed root.
- [x] Finalize every module/class name history after the existing builder completes.

### Task 3: Verification and publication

- [x] Record old/new release scaling for 8k through 64k assignments and calls.
- [x] Run analyzer, clippy and workspace suites.
- [x] Update analyzer OKF and design index with real source hash.
- [ ] Record six stages of three reviews, commit, push and create the closing PR.

## Planセルフレビュー

1. 意味: offset sortを計画から除き、挿入順referenceとの全境界比較を必須にした。
2. 費用: 構築event/nodeと照会nodeを別に測り、lookupだけの改善として扱わない。
3. 限界: 通常の最初のmin/max treeが交互offsetで線形照会を残す問題をレビューで検出し、任意履歴を二分探索するoffline activationへ計画を更新した。
