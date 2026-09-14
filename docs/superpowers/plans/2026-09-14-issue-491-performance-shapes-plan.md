# Issue #491 Performance Shapes Implementation Plan

> **For agentic workers:** Use reviewed execution, preserve autonomous PR authorization. Do not merge sibling issues.

**Goal:** Make performance dimensions and executable evidence traceable and reproducible.
**Architecture:** Versioned registry, exact-test gate, bounded subprocess release measurements using existing resource guard.
**Tech Stack:** Python 3.14 standard library, Rust tests, existing Lean corpus.
**Spec:** ../specs/2026-09-14-issue-491-performance-shapes-design.md

## Global Constraints

- Separate deterministic counters, retained heap, allocator peak and sampled process-tree RSS.
- Pending dependency tests are never counted as passing.
- Default N/2N/4N inputs, minimum 3 repeats, bounded subprocess time/RSS, no wall-time threshold in PR gate.
- No handwritten Lean expected corpus and no claim Lean proves native timing.

## Task 1: Registry and executable gate

- [x] Add `tests/test_performance_shapes.py` with independent invalid-registry cases (duplicate ID, missing dimension, invalid sizes/status/metric), missing/0 executed test failure, and fixture-output correctness checks.
- [x] Observe failure before creating `tools/performance_shapes.py`.
- [x] Implement registry validation and exact Rust test execution. Use `subprocess.run([...], timeout=...)` without shell. Parse actual test result and reject 0 matched/ignored-only tests.
- [x] Register active existing tests and pending Issue fixtures in `docs/performance/shapes.json`; cover eight dimensions and record Lean sources/adapter/freshness.
- [x] Run Python unit tests and active Rust gate, inspect results and perform three reviews.

## Task 2: Isolated release measurement

- [x] Add fixtures with independent expected candidates/truncation and invalid-output tests; assert positive counts cannot pass on empty output.
- [x] Generate bounded inputs in `TemporaryDirectory`, invoke guard with per-run stdout/stderr/stats files, parse semantic JSON after success, and preserve failed run evidence in the requested artifact directory.
- [x] Store versions, binary digests, command argv, source/input dimensions, raw measurements and medians. Keep baseline and candidate separately labeled.
- [x] Run small real release baseline/candidate experiment for every supported shape. Save explicit failures/unexecuted conditions and investigate before reporting success.
- [x] Apply hoimin mutation testing to changed production Python behavior after normal tests pass, within the skill's disk/job limits.

## Task 3: Documentation, CI and PR

- [x] Add active registry gate to Linux CI and a manual release measurement workflow with uploaded artifacts, preserving the existing workflow contract tests.
- [x] Update OKF performance concept/index, design and report references and provenance. Validate YAML, links and content separately.
- [x] Review each stage three times, run relevant gates, commit and create one Issue #491 PR with dependencies and evidence boundaries.

## 計画セルフレビュー

1. 設計の8次元・通常テスト・計測・モデル参照をTaskへ対応付けた。
2. 0件テスト成功、空の候補、監視失敗を成功扱いしないテストを先に指定した。
3. fixtureの一時ファイルと保存する証拠の所有者、shell不使用、未マージ依存の扱いを再確認した。

## 最終検証

Python79件、active Rust gate7件、全形状198回とtop1追試18回、既存Lean build/sensitivity/freshness/Rust adapter、OKF17ページの構造検査が成功した。限定変異はsurvivedからテスト追加後killed。詳細と未検証条件は [レビュー記録](../reports/2026-09-14-issue-491-performance-shapes-review.md) を参照。PR: https://github.com/tokyogas-tech/hoimin/pull/530 。公開後にbase=main、head=test/issue-491-performance-shapes、実装commit=0c6d4ecと17変更ファイルを照合した。

追記: 全10件の依存PRと実在するテスト名を台帳に登録した。個別Rust差分をローカルで組み合わせ、Clippyと全workspaceテスト（1,814成功、0失敗、19ignored）を確認した。詳細は [統合確認](../../performance/2026-09-14-integration-check.md) を参照。
