# Issue #547 Import Transfer Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement these steps in this issue worktree. User authorized autonomous execution and requires five self-review rounds per stage.

**Goal:** Eliminate per-statement full import copies in straight-line annotation analysis.
**Architecture:** Move the owned fallthrough state between statements; retain copies at scope/suite and branching boundaries where callers need an independent state.
**Tech Stack:** Rust, Ruff AST, existing test-only clone counters, OKF v0.2.
**Spec:** `docs/superpowers/specs/2026-09-15-issue-547-import-transfer-design.md`

## Global Constraints

- Rust MSRV 1.88; no dependencies or CLI schema changes.
- Preserve abrupt exit and unreachable-annotation behavior.
- Keep the production callback streaming; no per-annotation snapshots.
- Record five real review rounds for each stage in the issue report.

## Task 1: Deterministic copy regression

Files: `crates/hoimin-cli/src/analyzer/rust.rs` (existing `performance_cost_tests`).
Consumes: `AnnotationCollector::visit_each`, `IMPORT_CLONE_CALLS`, `IMPORT_CLONE_ENTRIES`.
Produces: a collector-wide cost test with independent I/A axes and observed callback count.

- [x] Add `straight_line_import_transfer_copy_cost_is_independent_of_annotations`: construct I imports and A `x: int` annotations for each I/A in `[0, 8, 32, 128]`; reset counters immediately before `visit_each`; assert callback count A, clone calls <= 1, entries <= I. Direct clone-boundary test already independently checks one actual clone copies every entry.
- [x] Run `cargo test --offline -p hoimin-cli --lib straight_line_import_transfer -- --nocapture` and record the expected counter failure.

```rust
let mut records = 0;
AnnotationCollector::visit_each(parsed.syntax(), &mut |_, _, _| records += 1);
assert_eq!(records, annotations);
assert!(IMPORT_CLONE_CALLS.get() <= 1);
assert!(IMPORT_CLONE_ENTRIES.get() <= imports);
```

## Task 2: Transfer ownership

Files: `crates/hoimin-cli/src/analyzer/rust.rs` (`visit_suite_flow` and simple fallthrough returns).
Consumes: owned `KnownImports` and `ControlFlowExits`.
Produces: the same exits and callback observations without redundant copies.

- [x] Use the following moves at the current clone sites; keep final suite restoration and all abrupt snapshots.

```rust
let mut fallthrough = Some(std::mem::take(&mut self.imports));
let mut statement_exits = self.visit_statement_flow(statement);
fallthrough = statement_exits.fallthrough.take();
// At simple owned fallthrough returns:
ControlFlowExits::fallthrough(std::mem::take(&mut self.imports))
```

- [x] Run the regression, library suite, and public annotation/loop/finally correspondence tests. Inspect unreachable-statement handling and class fallback restoration against the spec.
- [x] Add a selected/unselected public-analyzer counter test with exact real candidate assertions; verify existing callback clone sensitivity still passes.

## Task 3: Evidence and PR

Files: `docs/knowledge/design/analyzer.md`, reference catalogs, this spec/plan, issue report, `docs/performance/shapes.json`.

- [x] Register the deterministic cost gate with independent I/A scope and update OKF source hashes/links.
- [x] Run fmt, Clippy, workspace/default/contracts and relevant quality gates; record actual outcomes and environment limits.
- [x] Validate YAML using PyYAML, relative links and source hashes separately; complete five rounds of each stage's self-review.
- [x] Commit only this issue's files, prepare template-compliant PR text and review diff/base/claims/links/scope five times; parent publishes the reviewed branch and PR.
