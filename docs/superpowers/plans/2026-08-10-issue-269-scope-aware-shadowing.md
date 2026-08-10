# Issue 269 Scope-Aware Shadowing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Recover safe builtin and exception mutations suppressed by unrelated bindings while emitting a candidate only when both source and destination names provably resolve to Python builtins.

**Architecture:** Build a scope graph and a byte-offset name-resolution index in `AstFacts`. Whole-block facts model functions/comprehensions; ordered facts model direct module/class execution; one three-result resolver is shared by builtin-call and exception candidate families. The checked-in Lean model is the semantic oracle for safety boundaries.

**Tech Stack:** Rust 2024, `littrs-ruff-python-ast` 0.6.2, Lean 4.32.2, Cargo tests and Clippy.

## Global Constraints

- Emit only when source and destination names are both `DefinitelyBuiltin`.
- Treat missing mappings, wildcard imports, and ambiguous constructs as `Unknown` and suppress.
- Preserve operator IDs, profiles, defaults, candidate order, spans, symbols, limits, and report schemas.
- Keep class non-closure and the comprehension leftmost-iterable boundary explicit.
- Do not duplicate resolution policy between builtin-call and exception operators.
- Keep design, implementation plan, Lean model, code, tests, and user documentation in this Issue #269 worktree and PR.

### Task 1: Pin False Negatives and the Destination Safety Bug

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

- [ ] Add a sibling-function fixture where `list`/`tuple` rebinding does not suppress `list -> tuple` in another function.
- [ ] Add a visible late local binding fixture proving whole-function shadowing still suppresses earlier loads.
- [ ] Add a failing regression where only `tuple` is locally rebound and `list(items) -> tuple(items)` must not be emitted.
- [ ] Add equivalent source/destination regressions for one exception pair.
- [ ] Run the focused tests and capture the red false-negative and false-positive results.
- [ ] Commit with `test: expose scope-wide shadowing defects`.

### Task 2: Pin Python Scope Boundaries

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

- [ ] Add module source-order and conditional/wildcard uncertainty fixtures.
- [ ] Add closure, sibling, lambda, class-body, and method class-skipping fixtures.
- [ ] Add comprehension fixtures separating the leftmost iterable from target/body scope.
- [ ] Add module and function exception-target lifetime fixtures.
- [ ] Add `global` and nearest-`nonlocal` fixtures.
- [ ] Assert literal `(original, replacement, operator, line, symbol)` tuples and reparse retained replacements.
- [ ] Commit with `test: specify Python shadow resolution boundaries`.

### Task 3: Build Scope and Binding Facts

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

- [ ] Introduce private `ScopeId`, `ScopeKind`, `BindingFact`, `Resolution`, and per-scope fact types mirroring the Lean model.
- [ ] Build stable module/function/lambda/class/comprehension scope nodes and lexical parents.
- [ ] Gather whole-block function/comprehension bindings while excluding nested scopes and applying `global`/`nonlocal` directives.
- [ ] Gather ordered module/class binding effects, exception-body ranges, wildcard uncertainty, and conservative control-flow joins.
- [ ] Gather module-wide possible bindings including redirected global writes.
- [ ] Add focused unit tests for scope graph facts without deriving expected candidate output from the resolver.
- [ ] Commit with `feat(analyzer): index lexical binding scopes`.

### Task 4: Build the Occurrence Resolution Index

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

- [ ] Walk tracked name loads with an explicit scope stack and ordered module/class state.
- [ ] Visit definition headers in their enclosing scope and bodies in their new scope.
- [ ] Visit the first comprehension iterable outside, then targets/filters/later generators/results inside the comprehension scope.
- [ ] Resolve normal, `global`, and `nonlocal` references through lexical parents, skipping class scopes for nested ordinary functions.
- [ ] Record `DefinitelyBuiltin`, `Shadowed`, or `Unknown` by byte start; missing entries resolve to `Unknown`.
- [ ] Check the Rust scenario table against the checked-in Lean examples.
- [ ] Commit with `feat(analyzer): resolve tracked names at each occurrence`.

### Task 5: Share Resolution Across Candidate Families

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

- [ ] Replace file-wide builtin/exception sets and query methods with the resolution index.
- [ ] Add one helper that requires source and destination names to be definitely builtin at an occurrence.
- [ ] Route builtin-call pairs, `sorted/reversed`, exception pairs/tuple additions, base boundaries, and bare-handler insertion through it.
- [ ] Preserve all existing syntax, argument-contract, tuple, termination-exception, and handler-finality guards.
- [ ] Run all new scope tests plus existing collection and exception suites.
- [ ] Commit with `feat(analyzer): make builtin shadowing scope-aware`.

### Task 6: Document the Resolver Policy

**Files:**
- Modify: `docs/development.md`
- Test: `crates/hoimin-cli/tests/cli_config.rs` or the nearest documentation contract test.

- [ ] Document definitely-builtin gating, source/destination checking, lexical boundaries, and conservative `Unknown` fallback.
- [ ] Add a documentation contract assertion for the policy anchors.
- [ ] Commit code and test together; use `[skip ci]` only if the final commit is purely documentation.

### Task 7: Verify, Review, and Integrate

- [ ] Run `lake build` in `formal/HoiminOracle` and scan new Lean files for `sorry`, `admit`, or `axiom`.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo test -p hoimin-cli --test run_e2e`.
- [ ] Run `uv run --frozen python -m unittest tests/test_skills.py`.
- [ ] Run `git diff --check` and review every resolver call site for fail-open behavior or policy duplication.
- [ ] Rebase onto latest `main`, rerun affected verification, push, and create the Issue #269 PR.
- [ ] Monitor CI, squash merge, confirm Issue #269 closes, then remove its worktree and local branch.
