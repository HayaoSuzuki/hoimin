# Annotation descendant eligibility implementation plan

> Execute inline using executing-plans, TDD, and lean-test-oracle. The user has
> authorized the complete fix and reviews without further approval pauses.

**Goal:** Reject disallowed descendants in every tuple type-argument position.

**Architecture:** Extend the existing recursive gate; formal expectations remain
Lean-generated and the Rust adapter exercises public plan without reimplementing
the semantic gate.

**Tech Stack:** Rust/Ruff AST, Lean 4.32.2, CPython 3.14.

**Spec:** `docs/superpowers/specs/2026-09-24-issue-565-annotation-descendants.md`.

## Review focus

Test blocked dictionary keys as well as values, deep subscript/tuple combinations,
aliased forbidden constructors, a third tuple element, and all seven consumers.
Preserve #564 expectations; promote those rows to strict after stacking its fix.

## Task 1: recursive gate and oracle correspondence

Files: `crates/hoimin-cli/src/analyzer/rust.rs`, `rust_tests.rs`,
`crates/hoimin-cli/tests/lean_nullable_gate_oracle.rs`,
`formal/HoiminOracle/HoiminOracle/NullableGateModel.lean`,
`formal/HoiminOracle/NullableGateAuditMain.lean`, generated
`formal/HoiminOracle/corpus/nullable-gate.jsonl`, `docs/development.md`, and the
analyzer/nullable-audit/design-index OKF pages.

- [x] Add analyzer tests for blocked leaves in `dict[bad,int]`, `dict[str,bad]`,
  nested `list[dict[str,bad]]`, aliases, and `list[tuple[int,int,bad]]`. Compare
  literal expected counts using `analyze_with_only_operator`. Add positive controls
  for each matching shape and test all seven type operators.
- [x] Extend formal fixtures and witnesses. Run bounded Lean model build,
  generator sensitivity, generation and freshness commands. Keep original
  untrusted-name cases unchanged; their standalone report-only mode becomes
  strict after stacking #564.
- [x] Add a strict serde corpus parser and public plan adapter; assert source
  annotation evaluation and replacement compilation under CPython >=3.14.
  Expected observations are `Vec<(String,String)>` from the generated corpus.
- [x] Run `cargo test -p hoimin-cli --lib annotation_descendants` and
  `cargo test -p hoimin-cli --test lean_nullable_gate_oracle` to observe RED.
- [x] Add the tuple branch:
  `Expr::Tuple(tuple) => tuple.elts.iter().any(|item|
  contains_disallowed_annotation(item, imports))`; use the same rule for List
  elements and recurse through `Starred.value`.
- [x] Rerun the focused tests to GREEN; update developer and OKF contracts.
- [x] Perform three separate implementation reviews and three test reviews;
  retain concrete checks/findings in `docs/reviews/2026-09-24-issue-565.md`.
- [x] Run full workspace tests, `cargo fmt --all -- --check`, and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
  Validate OKF YAML, links, new source metadata and scope. Commit the verified fix.

## Stack integration

- [x] Preserve predecessor code and tests when rebasing; retain both sets of OKF entries.
- [x] Promote all 65 corpus rows to strict, reject report-only parser input, and
  preserve every expected pair.
- [ ] Run final stacked workspace tests, CI clippy, formatting, bounded Lean checks,
  and OKF checks; record exact results and rewritten source revisions.
