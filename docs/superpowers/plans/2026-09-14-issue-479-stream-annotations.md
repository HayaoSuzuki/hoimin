# Issue 479 Streaming Annotation Candidates Plan

**Goal:** Eliminate per-annotation import-map retention and skip type-only flow collection when no type operator is selected.

**Architecture:** Add a streaming callback mode to `AnnotationCollector`; production emits candidates inside `record`, while correspondence tests retain the existing owned snapshot mode.

**Spec:** `docs/superpowers/specs/2026-09-14-issue-479-stream-annotations-design.md`

### Task 1: Cost regression

- [x] Count annotation records and full import snapshot clones at their real sites.
- [x] Confirm RED: 256 annotations produce 256 records and 256 clones even with type operators disabled.
- [x] Specify disabled `(0,0)` and enabled `(256,0)` expectations.

### Task 2: Streaming implementation

- [x] Return early unless one of the seven type operators is selected.
- [x] Add `visit_each` callback mode and emit candidates while the current imports state is borrowed.
- [x] Preserve owned `collect` mode for flow/correspondence snapshots.

### Task 3: Verification and publication

- [x] Run annotation, correspondence, analyzer, clippy and workspace suites.
- [x] Measure old/new release RSS for disabled and enabled type paths.
- [x] Update analyzer OKF and design index with real hash.
- [x] Record six stages of three self-reviews, commit, push and create closing PR #531.

## Planセルフレビュー

1. 網羅性: 未選択の省略と選択時のsnapshot除去を別の期待値で検査し、issueの二つの費用経路を覆った。
2. 意味: import flow自体を単純化せず、同じ `record` 時点からborrowした状態で候補化する計画にした。
3. 回帰: test用owned mode、scope/branch/loop correspondence、候補prefix、release RSSを検証対象へ含めた。
