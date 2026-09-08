# Issue #431 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore valid class-comprehension operator_function candidates.
**Architecture:** ClassLookupScan distinguishes the enclosing first iterable from the implicit comprehension scope; all identity safeguards remain in ImportScan.
**Tech Stack:** Rust 1.98, MSRV 1.88, Ruff AST, CPython 3.14.
**Spec:** docs/superpowers/specs/2026-09-08-issue-431-class-comprehensions-design.md

## Global Constraints

- No CLI/schema changes or new dependencies.
- Rust MSRV remains 1.88; controlled Python is 3.14.
- Add no source comments except where required by Rust safety/lint contracts.
- Work only in this Issue worktree and preserve existing unrelated changes in the main checkout.

### Task 1: implement and validate comprehension lookup scopes

**Files:** Modify `crates/hoimin-cli/src/analyzer/rust/operator_functions.rs`, `crates/hoimin-cli/src/analyzer/rust_tests.rs`, `crates/hoimin-cli/tests/operator_function_contracts.rs`; update `docs/development.md`; create `docs/superpowers/reports/2026-09-08-issue-431-class-comprehensions.md`.
**Interfaces:** Consume existing ClassLookupScan visitor and public plan test helpers. Preserve OperatorImports::replacement and all public interfaces.

- [x] Add analyzer tests before production edits. Minimal case:

```python
import operator as op
class Subject:
    values = [op.add(2, 3) for _ in (0,)]
    callback = lambda: op.add(2, 3)
```

Expected operator_function candidates: exactly lines 3 and 4, add to sub. Extend literal expectation tables for set/dict/generator, from-import aliases, filters and later iterables. Negative controls: first iterable evaluated in class namespace, alias rebinding in a comprehension target, and private-name imports.
- [x] Run `cargo test --offline -p hoimin-cli --lib operator_function` and record the expected missing-candidate failure.
- [x] Handle Expr::ListComp, SetComp, DictComp and Generator explicitly in ClassLookupScan. Scope transition algorithm:

```text
visit first iterable in current scope
save current scope; enter Function
visit first target/filter and remaining generators
visit produced element or key/value
restore saved scope
```

Use the actual Ruff fields/visitor API, retain cancellation checks and support nested expressions without changing identity detection.
- [x] Add an external contract using existing assert_contract/plan helpers with a metaclass class binding, ordinary module binding and generated candidate application. Assert literal baseline/mutant outputs, and preserve first-iterable exclusion. Run `HOIMIN_OPERATOR_TEST_PYTHON=/Users/hayao/RustroverProjects/hoimin/.venv/bin/python cargo test --offline -p hoimin-cli --test operator_function_contracts`.
- [x] Run analyzer suite, fmt, clippy and record results. Controller additionally runs full workspace tests and MSRV. Update development guide narrowly and record limitations in report.
- [x] Self-review and commit only this task's files and its documents. Return commit, RED/GREEN evidence and unresolved concerns for independent review.
