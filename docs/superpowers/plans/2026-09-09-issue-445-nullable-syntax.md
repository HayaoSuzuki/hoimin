# Issue 445 nullable syntax Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Remove None without producing invalid annotation syntax or losing retained type semantics.

**Architecture:** Preserve the retained operand's grouping and Optional's source interior using existing AST and token boundaries. Keep candidate generation and result aggregation interfaces intact.

**Tech Stack:** Rust 1.88+, Ruff AST/tokens, CPython 3.14 contract tests.

**Spec:** docs/superpowers/specs/2026-09-09-issue-445-nullable-syntax-design.md

## Global Constraints

- Work only in .worktrees/issue-445-nullable-syntax, branch fix/issue-445-nullable-syntax.
- Rust MSRV 1.88. No new dependency or public schema/API.
- Preserve operator eligibility, candidate span, source-order behavior, and ordinary single-line replacements.
- Corrected replacement text can change candidate IDs by the existing ID algorithm.
- Avoid source comments; use names and small syntax-focused helpers.
- Do not drop valid candidates or add a per-mutant parse-and-discard fallback.

### Task 1: Repair nullable source extraction and verify real annotations

**Files:**
- Modify: crates/hoimin-cli/src/analyzer/rust.rs
- Test: crates/hoimin-cli/src/analyzer/rust_tests.rs
- Test: crates/hoimin-cli/tests/operator_function_contracts.rs
- Document: docs/superpowers/reports/2026-09-09-issue-445-nullable-syntax.md

**Interfaces:** Consume AnnotationCollector sites and AstFacts parser tokens; produce the same AnalyzerCandidate and plan manifest schemas. Thread facts/tokens through annotation_replacements and nullable_removal privately.

- [ ] Add a regression using this exact valid source, locate type_nullable_remove, require one candidate, apply its span and replacement, and assert Ruff parsing succeeds:

```python
from typing import Optional
x: Optional[(int
 | str)]
```

- [ ] Run the new focused analyzer regression before production edits and record the expected parse failure. Add counterparts for `None | (int\n | str)` and `(int\n | str) | None`, function parameters, return annotations and comments.
- [ ] Implement extraction following the spec. The intended boundaries are:

```rust
let range = ruff_python_ast::token::parenthesized_range(
    retained.into(), parent.into(), tokens,
).unwrap_or_else(|| retained.range());
let retained_source = &source[usize::from(range.start())..usize::from(range.end())];
```

For Optional, find its own Lsqb after the complete base, not a bracket in a comment or nested base. Preserve the original inner source. Keep a simple trivia-free argument unchanged; if removing brackets removes multiline/comment/grouping context, return a grouped expression containing that interior. Do not use Call-specific parenthesis rules for annotation operands.

- [ ] Extend CPython contract tests through actual plan output. Assert the emitted candidate count and evaluate annotation objects with annotationlib.get_annotations or typing.get_type_hints in variable, parameter and return contexts. For the reproduction, the baseline type members must be `{int, str, type(None)}` and the mutant `{int, str}`. Add nested grouping, multiline without explicit operand parentheses, comments and ordinary single-line controls. Recreate source for each mutation and avoid bytecode cache reuse.
- [ ] Run focused analyzer and operator_function_contracts tests. Record commands and outcomes in the tracked report.
- [ ] Self-review source boundaries and the tests, then commit only this task's code, tests and docs. Do not push or create PR; the controller owns integration verification and publishing.

## Controller validation and delivery

- Build CLI and run /private/tmp/hoimin-audit-nullable-run.py against this worktree's binary, requiring the corrected exit/status/score.
- Run `cargo test --offline --workspace --all-features -- --test-threads=1`.
- Run `cargo +1.88 check --offline --locked --workspace --all-targets --all-features` and `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`.
- Run `cargo fmt --all -- --check` and `git diff --check`.
- Task-scoped review, fixes/re-review if necessary, final whole-branch review, tracked final verification report, push and PR closing #445.

## Plan self-review 1 — acceptance coverage

Mapped all Issue acceptance criteria to Task 1 and controller validation. Added the unparenthesized multiline Optional case and leading/trailing interior comments to the required matrix so passing only the original grouped example cannot satisfy the task. The CPython check observes actual type members, not merely successful parsing.

## Plan self-review 2 — dependencies and execution

The helpers can consume existing AstFacts tokens without a new dependency or public interface. The implementation task owns both Rust regressions and CPython contracts; controller owns the full CLI check and final gates to avoid duplicate suite execution. Baseline failure must be observed before source changes. The controller's temporary repro script must be parameterized for the task binary instead of relying on a previously built path.

## Plan self-review 3 — completion evidence

Checked the plan against each design section and concrete file paths. Tests establish the old syntax failure, then parse and evaluate replacement output; the controller verifies the user-visible killed-to-survived change. Final gates and independent reviews cover the complete branch, and all design/plan/review evidence travels in tracked docs. No placeholder or missing acceptance step remains.
