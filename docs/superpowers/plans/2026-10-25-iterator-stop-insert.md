# Iterator stop insertion implementation plan

> **For agentic workers:** Use superpowers:executing-plans, inline implementation
> and one fresh whole-branch reviewer after validation.

**Goal:** Find tests that accept an iterator producing no expected values.
**Architecture:** Two opt-out AST operators share an entry-insertion helper.
Name resolution records protocol method entry sites and tracks builtin stop names.
**Tech Stack:** Rust/Ruff AST, CPython3.14/3.15.
**Spec:** docs/superpowers/specs/2026-10-25-iterator-stop-insert-design.md

## Global constraints

Only synchronous __next__/async __anext__ class methods; no generator or decorated
methods. Insert after docstring, retain body, require builtin resolution. Defaults
57, catalog76 including the break operator from the parent branch. No return or
general Exception/BaseException insertion in this change. Five reviews per stage.

## Review focus

- A nested yield must not exclude an ordinary iterator method.
- A shadowed, deleted or dynamically uncertain exception name must not be used.
- Inline suites/docstrings/decorated nested declarations must compile.
- Type parameter, class and function scopes must resolve the inserted name correctly.
- Prefix, selection, cancellation and payload contracts remain unchanged.

## Task 1: Operators and pure analysis

Files: core config and enum/default fixtures; python analyzer and iterator_stop.rs;
CLI ranking and operator inventory.
Interface: iterator_stop::entry(definition, source, exception, cancelled) returns
an optional anchored TextRange/replacement. Collector maintains lexical class
context; NameResolutionBuilder records entry resolution sites in function scope.

- [ ] Write failures for registration, exact insertion, yield and builtin exclusions.
- [ ] Run the new tests RED before adding production behavior.
- [ ] Implement registry, helper and name-resolution/collector gates.
- [ ] Run core/analyzer tests and five implementation self-reviews.

## Task 2: Public behavior, documentation and delivery

Files: dedicated CLI/CPython integration tests, default-selection fixture, usage,
compatibility, CLI reference, OKF/spec catalog and maintenance evidence.

- [ ] Execute genuine iterator/async iterator fixtures with both Python versions.
- [ ] Verify default inclusion, opt-out, explicit selection and saved plans.
- [ ] Update docs/OKF, generated reference and a bounded fuzz seed.
- [ ] Run workspace, Clippy, Python/doc checks and five validation reviews.
- [ ] Obtain one fresh independent review; repair findings with regressions.
- [ ] Complete five delivery reviews; create/attach a PR based on loop-break-insert.
