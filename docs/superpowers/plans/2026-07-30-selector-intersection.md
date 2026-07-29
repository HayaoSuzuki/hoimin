# Selector Intersection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make combined line and symbol selectors conjunctive without changing either selector's standalone behavior.

**Architecture:** Preserve the current request types and target normalization. Express optional selector axes as independent boolean constraints in the Rust analyzer and require both constraints to pass.

**Tech Stack:** Rust, Ruff Python parser, Cargo test

## Global Constraints

- Do not change the public CLI or serialized target representation.
- Apply the same predicate to token and type-annotation candidates.
- Retain line-only, symbol-only, and unrestricted behavior.

---

### Task 1: Lock the combined-selector behavior

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: `analyze_with(path, lines, symbols, max_candidates, source)`
- Produces: a regression test requiring the intersection of non-empty line and symbol axes

- [ ] **Step 1: Change the existing union test into an intersection fixture**

Use three mutation sites: one matching both constraints, one matching only the
symbol, and one matching only the line. Assert the returned descriptor is the
single both-matching site.

- [ ] **Step 2: Run the test and verify RED**

Run: `cargo test -p hoimin-cli analyzer::rust::rust_tests::filters_candidates_by_line_and_symbol -- --exact`

Expected: FAIL because the current predicate returns candidates matching
either axis.

### Task 2: Implement conjunctive optional axes

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: `AnalyzeRequest.lines`, `AnalyzeRequest.symbols`, candidate line and symbol
- Produces: `selected(...) -> bool` with AND semantics across non-empty axes

- [ ] **Step 1: Implement the minimal predicate**

Compute `line_selected` as empty-or-matching and `symbol_selected` as
empty-or-matching, then return `line_selected && symbol_selected`.

- [ ] **Step 2: Verify GREEN**

Run: `cargo test -p hoimin-cli analyzer::rust::rust_tests::filters_candidates_by_line_and_symbol -- --exact`

Expected: PASS.

- [ ] **Step 3: Run focused and workspace verification**

Run:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all commands exit zero.

- [ ] **Step 4: Commit**

Commit the design, plan, regression test, and implementation together with a
message referencing #62.
