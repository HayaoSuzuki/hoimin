# Issue 129 Analyzer Line Offsets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Precompute line-start offsets once per analyzed Python file and reuse them for token and annotation candidate positions.

**Architecture:** A private immutable `LineIndex` in `crates/hoimin-cli/src/analyzer/rust.rs` owns line-start byte offsets. `analyze_source_cancellable` constructs one index and passes it to both candidate pipelines; lookups binary-search lines and scan Unicode characters only within the selected line.

**Tech Stack:** Rust 2024, Ruff Python parser/AST byte ranges, Cargo unit tests, ignored release benchmark.

## Global Constraints

- Preserve one-based lines, zero-based Unicode-scalar columns, candidate ordering, filters, spans, and cancellation behavior.
- Add no dependency and make no public API or schema change.
- Do not add timing thresholds to CI.
- Benchmark the identical committed fixture before and after the production optimization.

---

### Task 1: Commit a repeatable analyzer benchmark

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: `analyze(source: &str) -> AnalyzerOutput`
- Produces: ignored test `benchmark_candidate_line_positions`

- [ ] **Step 1: Add the ignored release benchmark**

Create a deterministic roughly 300 KiB Python source with thousands of operator candidates, time one real `analyze` call with `std::time::Instant`, pass the result through `std::hint::black_box`, assert the expected candidate count, and print `source_bytes`, `candidates`, and `elapsed_ms`.

- [ ] **Step 2: Verify the harness**

Run: `cargo test --release -p hoimin-cli benchmark_candidate_line_positions -- --ignored --nocapture`

Expected: PASS and one metrics line with nonzero elapsed time.

- [ ] **Step 3: Commit the baseline harness**

```bash
git add crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "test(analyzer): benchmark candidate position lookup"
```

- [ ] **Step 4: Capture baseline measurements**

Run the command from Step 2 five times at the harness commit and record all elapsed values for the report.

### Task 2: Add and integrate the line index with TDD

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Produces: `LineIndex::new(source: &str) -> LineIndex`
- Produces: `LineIndex::line_and_column(&self, source: &str, offset: usize) -> (u32, u32)`
- Changes: `type_annotation_candidates(..., line_index: &LineIndex, ...)`

- [ ] **Step 1: Write the failing position tests**

Import `LineIndex` in `rust_tests.rs`. Add a table-driven test using hand-derived literals for offsets in `"alpha\nβeta\r\n終 = left == right\n"`, covering offset zero, after LF, after the two-byte `β`, before CRLF, and the final token line. Add a candidate-level test asserting exact `(original, line, column)` tuples for a Unicode-prefixed token candidate and a later type annotation candidate.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p hoimin-cli analyzer::rust::rust_tests::line_index --lib`

Expected: compilation fails because `LineIndex` does not exist.

- [ ] **Step 3: Implement the minimal index**

Add private `LineIndex { starts: Vec<usize> }`. `new` starts with `0` and pushes `index + 1` for each `b'\n'`. `line_and_column` uses `partition_point(|start| *start <= offset) - 1`, computes line as `line_index + 1`, and counts `source[line_start..offset].chars()` for the column. Keep the existing justified truncation allowance on the method.

- [ ] **Step 4: Integrate both candidate pipelines**

Construct one `LineIndex` after `AstFacts`. Replace the token helper call with the index method. Add `&LineIndex` to `type_annotation_candidates` and use it for every annotation candidate. Remove the old free `line_and_column` helper.

- [ ] **Step 5: Verify GREEN and focused compatibility**

```bash
cargo test -p hoimin-cli analyzer::rust::rust_tests --lib
cargo test -p hoimin-cli --test rust_analyzer
```

Expected: all tests pass.

- [ ] **Step 6: Commit the optimization**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "perf(analyzer): precompute line offsets"
```

### Task 3: Measure, document, and verify

**Files:**
- Create: `docs/superpowers/reports/2026-08-05-issue-129-line-offsets.md`

**Interfaces:**
- Consumes: benchmark harness and optimized analyzer
- Produces: reproducible performance and correctness report

- [ ] **Step 1: Capture optimized measurements**

Run the ignored release benchmark five times at the optimized commit. Record all elapsed values and calculate baseline and optimized medians, percentage reduction, and speedup.

- [ ] **Step 2: Write the report**

Document the fixture, exact command, baseline harness commit, optimized commit, five raw values, medians, derived improvement, correctness commands, and the fact that elapsed time is supporting evidence rather than a CI threshold.

- [ ] **Step 3: Run full verification**

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

Expected: every command exits zero. If a known environment-specific signal test fails, reproduce it unchanged on `origin/main` and document the result rather than weakening coverage.

- [ ] **Step 4: Commit the report**

```bash
git add docs/superpowers/reports/2026-08-05-issue-129-line-offsets.md
git commit -m "docs: report issue 129 performance results"
```

- [ ] **Step 5: Review, publish, and merge**

Review the complete diff against issue #129 and this design. Fix important findings, rerun affected verification, push `perf/issue-129-line-offsets`, open a PR containing `Closes #129`, monitor all required checks, and squash-merge only after they pass.
