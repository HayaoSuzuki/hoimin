# Loop break insertion implementation plan

> **For agentic workers:** Use superpowers:executing-plans; implement inline and
> request one fresh whole-branch review at the end.

**Goal:** Detect tests that miss premature loop termination.
**Architecture:** Register one opt-out AST operator; a small source-layout helper
constructs an anchored insertion, admitted by the existing collector.
**Tech Stack:** Rust, Ruff AST, CPython 3.14 and 3.15.
**Spec:** `docs/superpowers/specs/2026-10-25-loop-break-insert-design.md`

## Global constraints

Default enabled; removable by `--exclude-operators loop_break_insert`.
One candidate per eligible for/async for/while body; no continue insertion.
Preserve source layout and existing budgets. Five self-reviews per stage.

## Review focus

- Inline suites and semicolons must remain valid Python.
- Nested suites must break the selected loop, including its else behavior.
- Tabs, CRLF and Unicode must preserve byte coordinates and syntax.
- Direct unconditional exits must suppress unreachable tail insertions.
- Selection, cancellation and candidate/payload caps retain their contracts.

## Task 1: Operator and analyzer

Files: core config/operator fixtures; python analyzer and new loop_break helper;
CLI default_operator_selection integration fixture.
Interface: `loop_break::replacement(statement: &Stmt, source: &str, tokens: &Tokens, cancelled: &F) -> Option<(TextRange, String)>`.

- [x] Add analyzer tests for the review focus and a failing default-selection test.
- [x] Run them and observe failure from the missing operator.
- [x] Register LoopBreakInsert and implement the helper/collector gate.
- [x] Run analyzer/core and default-selection tests; repair affected exact fixtures.
- [x] Review five times: semantics, syntax, scopes, resource limits, configuration.

## Task 2: Validation and delivery

Files: usage, compatibility, generated CLI reference, knowledge analyzer concept,
design catalog, maintenance evidence.

- [x] Demonstrate first-iteration termination and loop else behavior on 3.14/3.15.
- [x] Update user docs and OKF; regenerate CLI reference.
- [x] Run workspace tests, formatting, clippy, Python/documentation checks.
- [x] Perform five validation reviews and one fresh independent final review.
- [x] Fix findings with regression tests; finish five delivery reviews.
- [x] Commit, push the dedicated branch, create and attach the PR.
