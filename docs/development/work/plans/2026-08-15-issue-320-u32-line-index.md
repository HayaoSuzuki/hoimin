# `Vec<u32>` Line-Start Index Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace both production `Vec<usize>` line-start indexes with `Vec<u32>` while preserving location semantics.

**Architecture:** The CLI and core retain independent private indexes. Constructors check each byte offset with `u32::try_from`; lookup converts the selected stored offset with `usize::from` before source slicing.

**Tech Stack:** Rust 2024, Cargo tests, workspace Rust 1.88, no new dependencies.

## Global Constraints

- Keep one-based line numbers, Unicode-scalar columns, LF and CRLF behavior, and valid-offset caller contracts unchanged.
- Add no production dependency.
- Preserve infallible index construction and make the source-size invariant explicit in conversion expectations.

---

### Task 1: Convert CLI `LineIndex`

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs:482-510`
- Test: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Consumes: `LineIndex::new(source: &str)` and `line_and_column(source, offset)`.
- Produces: identical `(u32, u32)` results with `starts: Vec<u32>`.

- [ ] **Step 1: Write the failing regression test**

Add a table-driven `LineIndex` test for `"alpha\nβeta\r\n終"`, with hand-derived line/column pairs at byte zero, after LF, after `β`, after CRLF, and source end.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p hoimin-cli line_index_uses_compact_offsets --lib`

Expected: FAIL because the new test is absent before it is added.

- [ ] **Step 3: Implement the representation change**

Change `starts` to `Vec<u32>`. In `new`, push `u32::try_from(index + 1).expect("Ruff source offset fits u32")`. In lookup, compare `usize::from(*start)` in `partition_point`, then convert the selected stored start with `usize::from` before slicing.

- [ ] **Step 4: Verify focused CLI tests**

Run: `cargo test -p hoimin-cli analyzer::rust::rust_tests --lib`

Expected: PASS with the existing ignored benchmark omitted.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/analyzer/rust.rs crates/hoimin-cli/src/analyzer/rust_tests.rs
git commit -m "perf(analyzer): store line starts as u32"
```

### Task 2: Convert core candidate-validation line starts

**Files:**
- Modify: `crates/hoimin-core/src/candidate.rs:88-216`
- Test: `crates/hoimin-core/tests/candidate_policy.rs`

**Interfaces:**
- Consumes: `CandidateValidationContext::new(source)` and `validate_candidate_with_context`.
- Produces: unchanged candidate acceptance and `LocationMismatch` behavior with `line_starts: Vec<u32>`.

- [ ] **Step 1: Write the failing core location regression**

Add a candidate-policy case for a multibyte prefix followed by CRLF. Assert that the valid candidate keeps its hand-derived one-based line and Unicode-scalar column.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p hoimin-core --test candidate_policy <new_test_name>`

Expected: FAIL before the new regression exists.

- [ ] **Step 3: Implement the representation change**

Change `line_starts` to `Vec<u32>`. Build starts with `u32::try_from(offset + 1).expect("candidate source offset fits u32")`; compare starts through `usize::from(*line_start)` and convert the selected start through `usize::from` before slicing.

- [ ] **Step 4: Verify focused core tests**

Run: `cargo test -p hoimin-core --test candidate_policy`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-core/src/candidate.rs crates/hoimin-core/tests/candidate_policy.rs
git commit -m "perf(core): store validation line starts as u32"
```

### Task 3: Run the full verification set

**Files:**
- Modify only if a focused failure identifies a defect in Tasks 1 or 2.

- [ ] **Step 1: Run formatting and all required checks**

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main...HEAD
```

Expected: each command exits 0; only intentional ignored benchmarks remain ignored.
