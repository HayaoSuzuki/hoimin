# Issue #441 structural syntax implementation plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Execute the one bounded implementation task, then independent task and final reviews.

**Goal:** Generate valid, intended structural mutants for parenthesized/multiline expressions and comments.
**Architecture:** Preserve source syntax with AST/token boundaries and localized edits inside existing candidate spans.
**Tech Stack:** Rust, Ruff AST/parser tokens, existing Rust/CLI tests, external CPython 3.14.
**Spec:** `docs/superpowers/specs/2026-09-09-issue-441-structural-syntax-design.md`

## Global Constraints

No new dependencies or schema changes. Preserve MSRV 1.88, operator eligibility, ordering, candidate limits, and ordinary output. Minimize source comments. Only this worktree may be edited. No Python runtime feature or new Python test module.

### Task 1: Repair structural source builders

**Files:** Modify `crates/hoimin-cli/src/analyzer/rust.rs` and `rust_tests.rs`; extend the existing Rust-driven CPython integration tests in `crates/hoimin-cli/tests/operator_function_contracts.rs`. Record results in `docs/superpowers/reports/2026-09-09-issue-441-structural-syntax.md`.
**Interfaces:** Existing candidate API and full-call spans remain unchanged. AST/token helpers stay private. The controller owns broad validation and PR publication.

- [ ] Baseline: run `cargo test --offline -p hoimin-cli --lib structure_` and inspect existing structural/collection exact-output tests.
- [ ] Add failing tests using the existing analyze/apply-and-reparse helpers. Include literal fixtures and exact expected mutations:

```python
(items.sort)()                 # expected (items.reverse)()
(items.reverse)()              # expected (items.sort)()
(d\n    .data)[key]             # retain receiver grouping in .get conversion
(d\n    .data).get(key)         # retain receiver grouping in subscript conversion
(d # [ receiver comment\n)[key] # preserve comment and locate the actual bracket
(items\n    .append)(value)     # retain grouped callee when changing method/arguments
```

Also cover nested parentheses, CRLF/Unicode, tuple and multiline keys, both append/extend directions and insert/append. Assert each supported case yields a candidate, not merely that emitted candidates parse. Keep comments only where they are necessary fixture syntax.
- [ ] Run new focused tests and record RED caused by malformed replacement or lost syntax before production changes.
- [ ] Implement localized source edits. Existing `parenthesized_range(expr.into(), parent.into(), tokens)` supplies grouped receiver ranges; token lookup supplies real delimiters. Renaming a method inside `call.range()` preserves enclosing callee parentheses. For argument edits preserve the call skeleton and use expression/token ranges rather than trim/find heuristics. Verify helper availability/types from local Ruff source.
- [ ] Extend external CPython validation through the existing Rust-driven operator-test convention if feasible. Check syntax plus receiver/key evaluation preservation with literal expected behavior. Controller will additionally execute real CLI plan/run fixtures.
- [ ] Run focused structural and collection tests plus `cargo test --offline -p hoimin-cli --lib analyzer::rust::tests` (confirm actual module filter). Run fmt and scoped Clippy. Record exact commands, counts, RED/GREEN, and self-review in tracked report; commit source/tests/design/plan/report. Do not push or create PR.
- [ ] Controller: independent task review, fix/re-review if needed, whole-workspace all-features tests, MSRV 1.88, full Clippy, external CLI validation, separate final review, final tracked report, PR closing #441.

## Plan self-review 1 — coverage against design

Mapped delimiter ownership to exact-output and reparse tests; added neighboring append/insert/extend cases so the fix cannot stop at the issue's first example. Whole-call spans and ordinary cases explicitly remain covered.

## Plan self-review 2 — evidence quality

A loop over zero candidates would pass without fixing discovery. Require a concrete candidate for every supported case. RED must show malformed source, not test setup failure. External CPython and run validation complement Ruff reparsing without adding a Python runtime component.

## Plan self-review 3 — execution boundaries

One implementation owner covers tightly related helpers and tests. Controller owns independent broad tests and reviews, preventing overlapping edits. Source comments are only justified for syntax fixtures. Read actual external-test conventions before extending them; use existing Rust test helpers rather than adding Python infrastructure.
