# Effective Python behavior mutations implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add the five approved opt-out behavior mutations.
**Architecture:** Local AST builders and existing selection/admission machinery.
**Tech Stack:** Rust, vendored Ruff AST, CPython3.14/3.15, uv.
**Spec:** ../specs/2026-10-25-effective-python-mutations-design.md

## Global constraints

Use five exact IDs/spec semantics;66 default/85 catalog/23opt-out regression IDs.
Preserve scopes/evaluation once, UTF8 ranges/LFCRLFCR, budgets/cancellation/order.
No dependency/schema change; jobs/test threads2; each stage at least5self-reviews.

## Review focus

- is-not-None used as a value must remain bool; reverse operands and chains.
- Explicit None must remain fresh after only the omitted default becomes shared.
- nonlocal in inline/multiple-name declarations must leave compilable code.
- YieldFrom nested/implicit operand parentheses must preserve legal syntax.
- Syntactic method replacement must preserve comments/effects and exclude splats.

## Task1: Registration and analyzer builders

Files: core config/operator fixtures; python analyzer rust.rs, rust/effective_python.rs
and effective_python_tests.rs (new); CLI ranking and its complete operator fixture.
Interfaces: expression(expression,source,operators,cancelled,emit),
statement(statement,source,operators,cancelled,emit); emit receives TextRange,
String,MutationOperator and returns bool (continue admission).

- [x] Baseline core/python library tests; write registration RED for five names.
- [x] Register IDs/defaults and update enum exhaustive ranking,85-ID fixtures.
- [x] Add positive/negative RED for all five behaviors and review focus inputs.
- [x] Implement local builders/hooks; selection gates before copies.
- [ ] Run core/python suite and five implementation reviews; commit task.

## Task2: Public behavior and delivery

Files: CLI default/config/valid-python tests and inventory; support/effective_python.rs
(new); usage/compatibility/CLI reference; knowledge analyzer/spec catalog; fuzz seed;
maintenance verification record.
Interface: existing analyze/descriptor/python_observe harness, observations compile,
exception and stdout, LFCRLFCR and both Python versions.

- [x] Add66-default/23-opt-out and explicit/saved-plan fixtures.
- [x] Add runtime assertions supplementing the analyzer RED->GREEN for all five transformations.
- [x] Update docs, OKF source provenance and fuzz seed; build/check references.
- [x] Run targeted runtime/config/docs checks and five validation reviews.
- [x] One fresh whole-branch review; fix substantive findings RED->GREEN.
- [x] Full locked workspace suite, all-feature/target Clippy, formatting/reference.
- [ ] Five delivery reviews; commit/push/create/attach main-based PR.
- [ ] Remove owned build/site outputs and record cleanup; preserve primary .gitignore.
