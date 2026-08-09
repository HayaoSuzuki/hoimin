# Issue 266 Candidate Validation Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Precompute source validation facts once per analyzed file while preserving strict external validation, error precedence, and stable candidate IDs.

**Architecture:** Introduce a borrowed `CandidateValidationContext` in `hoimin-core`, delegate the legacy strict wrapper to it, and pass one context through each CLI analyzer batch. Use indexed line starts for location validation and retain strict wrapper use in plan ingestion.

**Tech Stack:** Rust 2024, BLAKE3, Camino, serde JSONL candidate spools, Cargo tests and Clippy.

## Global Constraints

- Preserve the candidate schema, JSON, stable-ID inputs, candidate ordering and sequence values.
- Preserve the exact validation error precedence, including invalid UTF-8 cases.
- Keep `validate_candidate(source, descriptor)` as the strict external-boundary API.
- Do not add dependencies or cache source facts globally.
- Benchmark conversion, validation, spool writes and spool finalization—not analyzer parsing alone.
- Keep the design and implementation plan in the same Issue #266 worktree and PR.

---

### Task 1: Pin Context Equivalence and Error Precedence

**Files:**
- Modify: `crates/hoimin-core/tests/candidate_policy.rs`

**Interfaces:**
- Consumes: current `validate_candidate` behavior and stable ID fixtures.
- Produces: failing tests for `CandidateValidationContext` and `validate_candidate_with_context`.

- [ ] Add table-driven equivalence cases for ASCII, Unicode columns, CRLF, stale hash, stale original, out-of-range spans, and invalid locations.
- [ ] Add multiply-invalid descriptors that pin schema/path/hash/mutation/span/original/UTF-8 precedence.
- [ ] Validate several candidates through one context and assert their IDs match the legacy wrapper exactly.
- [ ] Run `cargo test -p hoimin-core --test candidate_policy` and capture the red compile result.
- [ ] Commit the regression tests with `test: specify reusable candidate validation facts`.

### Task 2: Implement the Core Validation Context

**Files:**
- Modify: `crates/hoimin-core/src/candidate.rs`
- Test: `crates/hoimin-core/tests/candidate_policy.rs`

**Interfaces:**
- Produces: `CandidateValidationContext::new`, `file_hash`, and `validate_candidate_with_context`.
- Preserves: `validate_candidate` as a fresh-context delegating wrapper.

- [ ] Add a context borrowing `&[u8]`, owning one lowercase hash string and line-start byte offsets, and retaining the UTF-8 decode result without making construction fallible.
- [ ] Move validation into `validate_candidate_with_context` without changing check order.
- [ ] Replace prefix scanning with partition-point line lookup and current-line character counting.
- [ ] Delegate `validate_candidate` through a fresh context.
- [ ] Run the core policy tests and the full `hoimin-core` suite.
- [ ] Commit with `perf(core): reuse candidate validation facts`.

### Task 3: Pin Analyzer Batch Conversion

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`

**Interfaces:**
- Consumes: private `mutation_candidate` conversion and both batch loops.
- Produces: tests proving context-backed conversion rejects stale metadata and preserves candidate values.

- [ ] Add a failing unit test that converts multiple analyzer candidates with one context and compares IDs and fields with the strict wrapper.
- [ ] Add stale hash/source/location assertions without deriving expected metadata through production helpers.
- [ ] Run the focused analyzer tests and capture the red result.
- [ ] Commit with `test: pin batch candidate validation semantics`.

### Task 4: Reuse One Context Per Analyzed File

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`
- Test: `crates/hoimin-cli/src/analyzer/mod.rs`

**Interfaces:**
- Consumes: `CandidateValidationContext` and `validate_candidate_with_context`.
- Produces: a context-accepting `mutation_candidate` used by discovery and store batches.

- [ ] Construct one context after each target analysis in `discover_targets_blocking`.
- [ ] Construct one context after analysis in `analyze_and_store`.
- [ ] Copy `context.file_hash()` into each descriptor and validate through the same context.
- [ ] Keep `plan.rs` on the strict legacy wrapper.
- [ ] Run focused analyzer, plan, and candidate-store tests.
- [ ] Commit with `perf(analyzer): reuse validation facts per source`.

### Task 5: Add the End-to-End Bridge Benchmark

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`
- Optionally modify: `docs/development.md` if benchmark invocation conventions are documented there.

**Interfaces:**
- Produces: ignored `benchmark_candidate_conversion_and_manifest` release-mode harness.

- [ ] Generate the fixed 307,200-byte source with 7,680 arithmetic candidates.
- [ ] Time analysis output conversion, context validation, `CandidateStore::push`, and `CandidateStore::finish` as one operation.
- [ ] Assert source size, candidate count, and final spool record count before printing elapsed milliseconds.
- [ ] Run the benchmark explicitly in release mode and record the command/output in the PR body.
- [ ] Commit code with `perf: benchmark candidate conversion and manifest` and pure documentation, if any, with `[skip ci]`.

### Task 6: Verify, Review, and Integrate

**Files:**
- Verify all Issue #266 files and unchanged external plan validation.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo test -p hoimin-cli --test run_e2e`.
- [ ] Run `uv run --frozen python -m unittest tests/test_skills.py`.
- [ ] Run `git diff --check` and review the complete diff for policy duplication or accidental external trust.
- [ ] Rebase onto the latest `main`, rerun affected verification, push, and create the Issue #266 PR.
- [ ] Monitor required CI; diagnose any failure before retrying.
- [ ] Squash merge, confirm Issue #266 closes, fast-forward local `main`, and remove the worktree and local branch.
