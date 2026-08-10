# Issue 267 AST Fact Indexes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace repeated linear AST-fact scans with exact immutable indexes while preserving all analyzer candidate observations.

**Architecture:** Finalize raw visitor facts into specialized indexes: prefix-maximum containment indexes for annotations/arid regions, a keyed unary-`not` map, and binary-searchable innermost-scope segments. Lean specifies the equivalence contracts; independent Rust tests check the concrete construction against legacy scans.

**Tech Stack:** Rust 2024, Ruff Python AST 0.6.2, Lean 4.32.2, Cargo tests and Clippy.

## Global Constraints

- Preserve candidate order, exact byte spans, replacements, symbols, profiles, limits, and report schemas.
- Do not retain a linear fallback in any hot fact query.
- Keep indexes immutable after `AstFacts::from_module` finalization.
- Use count/comparison invariants instead of wall-clock test thresholds.
- Keep design, plan, Lean model, implementation, tests, benchmark, and documentation in the Issue #267 worktree and PR.

### Task 1: Specify Range Index Semantics in Lean

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/FactIndexModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/FactIndexProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

- [ ] Model half-open ranges, containment, sorted prefix-maximum summaries, scopes, and disjoint scope segments.
- [ ] Prove a valid prefix summary returns true exactly when the legacy containment predicate does.
- [ ] Prove a valid scope segment identifies the same greatest-start containing range as the legacy rule.
- [ ] Add the overlapping-range and ended-inner-scope broken-index witnesses.
- [ ] Run `lake build` and scan the new modules for `sorry`, `admit`, and custom `axiom` declarations.
- [ ] Commit with `test(lean): model AST fact index equivalence`.

### Task 2: Pin Concrete Index Behavior Before Integration

**Files:**
- Create: `crates/hoimin-cli/src/analyzer/rust/fact_index.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

- [ ] Add red unit tests comparing planned containment lookups with independent linear scans for empty, nested, overlapping, touching, equal-start, and boundary cases.
- [ ] Add red tests for innermost scope lookup before, inside, between, and after nested scopes, including equal starts.
- [ ] Add a red keyed-`not` test that requires exact byte-start lookup.
- [ ] Add deterministic generated property cases without deriving expected values from index internals.
- [ ] Run the focused module tests and record the expected red result before implementation.
- [ ] Commit with `test(analyzer): specify AST fact index equivalence`.

### Task 3: Implement Immutable Fact Indexes

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust/fact_index.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`

- [ ] Implement sorted starts and prefix-maximum ends with explicit binary search.
- [ ] Implement the end-before-start scope event sweep and disjoint segment lookup.
- [ ] Preserve last-on-equal-start scope selection using stable ordinals.
- [ ] Replace unary-`not` vector lookup with an offset-keyed `HashMap` and reject duplicate starts in debug/test builds.
- [ ] Finalize every index exactly once after AST visitation and remove hot raw-vector scans.
- [ ] Run the new index tests and existing analyzer tests to green.
- [ ] Commit with `perf(analyzer): index AST fact lookups`.

### Task 4: Preserve Candidate Observations and Avoid Irrelevant Queries

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

- [ ] Reorder the token annotation guard so the cheap applicable-spelling check runs first.
- [ ] Add candidate snapshot fixtures containing nested definitions, annotations, unary `not`, and focused-profile arid ranges.
- [ ] Assert literal candidate order, operator, exact span, replacement, and symbol before and after indexing.
- [ ] Reparse representative retained replacements.
- [ ] Run the full analyzer test module.
- [ ] Commit with `perf(analyzer): avoid irrelevant annotation lookups`.

### Task 5: Add Complexity Counters and Adversarial Benchmark

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust/fact_index.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

- [ ] Add test-only query and comparison counters that cannot affect candidate selection or release output.
- [ ] Generate a large Python module containing many scopes, annotations, unary `not` expressions, and focused arid regions.
- [ ] Assert exact candidate counts and representative ordering/spans.
- [ ] Assert logarithmic comparison ceilings and keyed-lookup counts; expose no wall-clock pass condition.
- [ ] Keep the benchmark ignored by default and print elapsed time for explicit release runs.
- [ ] Run it with `cargo test --release ... --ignored --nocapture`.
- [ ] Commit with `test(analyzer): bound AST fact lookup work`.

### Task 6: Document the Performance Contract

**Files:**
- Modify: `docs/development.md`
- Modify: nearest existing documentation contract test if needed

- [ ] Document index construction, asymptotic lookup guarantees, benchmark command, and non-timing assertions.
- [ ] State that semantic equivalence is checked against the legacy linear definitions.
- [ ] Commit with `[skip ci]` only when the commit contains documentation and no executable test or code change.

### Task 7: Verify, Review, and Integrate

- [ ] Run `lake build` in `formal/HoiminOracle` and scan new Lean files for forbidden proof placeholders.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace` after creating the controlled `.venv`.
- [ ] Run `cargo test -p hoimin-cli --test run_e2e`.
- [ ] Run `uv run --frozen python -m unittest tests/test_skills.py`.
- [ ] Run `git diff --check` and inspect every replaced query for accidental linear fallback or changed boundary semantics.
- [ ] Rebase onto latest `main`, rerun affected verification, push, and create the Issue #267 PR.
- [ ] Monitor CI, squash merge, confirm Issue #267 closes, then remove its worktree and local branch.
