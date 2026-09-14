# Issue 470 Candidate Column Index Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove repeated line-prefix scans while preserving Python source coordinates in analyzer discovery and candidate validation.

**Architecture:** Add a sparse reusable `PythonSourceIndex` in `hoimin-core`, use it in `CandidateValidationContext`, and wrap it in the analyzer's `LineIndex`. Test-only counters bound lookup work independently of elapsed time.

**Tech Stack:** Rust 2024, hoimin-core, hoimin-cli, Ruff Python parser.

**Spec:** `docs/superpowers/specs/2026-09-14-issue-470-column-index-design.md`

## Global Constraints

- Preserve byte spans, one-based physical lines, zero-based Unicode-scalar columns, and the leading-file-BOM exception.
- Preserve candidate validation error precedence, stable IDs, ordering, diagnostics, and truncation.
- Keep public single-query location helpers compatible.
- Use `/private/tmp/hoimin-analyzer-build` as `CARGO_TARGET_DIR`.

---

### Task 1: Specify indexed source locations

**Files:**
- Modify: `crates/hoimin-core/tests/candidate_policy.rs`
- Modify: `crates/hoimin-core/src/candidate.rs`

**Interfaces:**
- Produces: `PythonSourceIndex::new(&str) -> Result<Self, CandidateValidationError>` and `line_and_column(&self, usize) -> Option<(u32, u32)>`.
- Preserves: `python_line_starts`, `python_source_column`, and validation error ordering.

- [ ] Add literal table cases for empty/ASCII lines, CRLF, lone CR, combining characters, 2/3/4-byte scalars, leading BOM, and interior U+FEFF.
- [ ] Run `CARGO_TARGET_DIR=/private/tmp/hoimin-analyzer-build cargo test -p hoimin-core --test candidate_policy indexed_source_locations` and confirm failure because `PythonSourceIndex` is absent.
- [ ] Implement physical line starts plus sparse cumulative UTF-8 excess-byte entries, and answer arbitrary offsets by binary search.
- [ ] Route `CandidateValidationContext` through the index without changing checks before location validation.
- [ ] Run the focused test and the full candidate policy test.

### Task 2: Reuse the core index in analyzer discovery

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: `PythonSourceIndex`.
- Produces: analyzer `LineIndex` that delegates arbitrary position queries to the shared contract.

- [ ] Add an analyzer test with out-of-order ASCII and Unicode offsets and hand-derived `(line, column)` values.
- [ ] Run the focused analyzer test and confirm its assertion on zero prefix scans fails against the old implementation.
- [ ] Replace analyzer-owned line starts with `PythonSourceIndex` and preserve the infallible Ruff-size assumption.
- [ ] Run the focused test and existing analyzer tests.

### Task 3: Bound lookup work and record release evidence

**Files:**
- Modify: `crates/hoimin-core/src/candidate.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Produces: test-only location lookup statistics and an ignored release benchmark for long single-line and multiline inputs.

- [ ] Add test-only counters whose absence makes the complexity test fail to compile.
- [ ] Assert that an ASCII single line with increasing boolean candidates performs no Unicode-prefix character scan and preserves candidate output/truncation.
- [ ] Run the focused test red, implement counter propagation, and rerun green.
- [ ] Run the ignored benchmark in release mode and record source bytes, candidate counts, and elapsed times without timing assertions.

### Task 4: Update the knowledge catalog and verify

**Files:**
- Modify: `docs/knowledge/design/analyzer.md`
- Modify: `docs/knowledge/references/design-documents.md`
- Create: `docs/superpowers/reports/2026-09-14-issue-470-self-review.md`

**Interfaces:**
- Records: the indexed column contract, evidence scope, and five stages of three self-reviews.

- [ ] Add the design source with its real hash and working-tree state, and update both indexes.
- [ ] Run YAML/frontmatter, footnote, and link checks separately.
- [ ] Run fmt, clippy, core policy, analyzer, workspace, and CLI end-to-end tests.
- [ ] Review the complete diff three times for acceptance coverage, semantic boundaries, and cost evidence; record findings and corrections.
- [ ] Push `perf/issue-470-column-index`, create a PR closing #470, and review title/body/diff/checks three times before reporting it.

## Planセルフレビュー

1. 仕様対応を確認し、core単体と解析器の両方にUnicode・BOMの回帰試験を割り当てた。候補検証だけを高速化して解析器を残す漏れを修正した。
2. プレースホルダーを検索し、各実装手順に対象関数、失敗理由、実行コマンドを記載した。経過時間を合否条件にする記述は削除した。
3. 型と順序を確認し、`PythonSourceIndex` をcoreが所有し、解析器と検証contextが同じ問い合わせAPIを使う構成に統一した。公開helperを削除する案は互換性条件と矛盾するため除外した。
