# Issue 474 explicit selector index implementation plan

**Goal:** Resolve explicit file, line, and symbol selectors through one platform-aware index instead of scanning all discovered files for every selector.

**Architecture:** Keep the existing ordered discovered-file map for enumeration and output stability. Build a second Python-only equality-key index once and route exact lookups through it.

**Tech Stack:** Rust 1.98, camino, standard `BTreeMap`, existing core target tests.

### Task 1: Establish the performance regression

**Files:** `crates/hoimin-core/src/target.rs`

1. Add a private test-only operation observer around path equality checks without changing the public API.
2. Add a unit test that resolves increasing equal-sized discovered-file and selector sets, asserts identical targets, and asserts work does not scale as F × S.
3. Run the focused test and record the expected failure against the linear scan.

### Task 2: Build and use the Python path index

**Files:** `crates/hoimin-core/src/target.rs`

1. Build `available_python: BTreeMap<String, Utf8PathBuf>` from the ordered `available` map.
2. Insert only Python paths and preserve the first path for an equality-key collision.
3. Change `require_python` to derive the requested key once and use `get`.
4. Pass the index to `resolve_symbol_path` while preserving `.py` before package precedence.
5. Run the focused regression test and core target tests.

### Task 3: Pin compatibility cases

**Files:** `crates/hoimin-core/src/target.rs`, `crates/hoimin-core/tests/target_policy.rs`

1. Add tests for non-Python collisions, normalized aliases, missing paths, file/line combinations, and symbol precedence where coverage is missing.
2. Run cost tests red against the old implementation. Record behavior-preservation tests that already pass as characterization tests.
3. Run `cargo test -p hoimin-core --test target_policy` and `cargo test -p hoimin-core`.

### Task 4: Document and measure the result

**Files:** `docs/knowledge/design/selection-plan-verify.md`, the design/report reference indexes, and `docs/superpowers/reports/2026-09-14-issue-474-selector-index.md`

1. Update the OKF contract with the index, complexity scope, and Windows limitation.
2. Validate YAML, links, citations, and Japanese prose separately.
3. Build a release runner against the current core library and measure multiple input sizes with repeated runs.
4. Record three review passes for OKF, design, plan, implementation, tests, and PR preparation in the issue report.

### Task 5: Run gates and publish

1. Run formatting, clippy for the affected package, core tests with and without contracts, workspace tests, and `git diff --check` using `/private/tmp/hoimin-target-build/issue-474`.
2. Review the complete diff three times for contract correctness, platform behavior, and maintainability/performance evidence; fix concrete findings.
3. Commit, push `perf/issue-474-selector-index`, create the PR with the repository template, and review the rendered PR three times.
