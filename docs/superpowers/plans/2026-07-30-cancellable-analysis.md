# Cancellable Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move parsing and candidate spooling off async scheduler workers and make long analysis loops cooperatively cancellable.

**Architecture:** `AnalyzerHandler` transfers owned per-request work and its candidate store to `spawn_blocking`. The analyzer receives a cancellation probe, returns a typed cancellation outcome, and checks it throughout long loops; only successful completion restores or finalizes the store.

**Tech Stack:** Rust 2024, Tokio, Ruff Python parser, tempfile-backed candidate store.

## Global Constraints

- Preserve stable candidate ordering and candidate limits.
- Preserve existing analyzer error codes except for a new blocking-task failure code.
- Never publish a partial candidate spool after cancellation.
- Keep tests portable across Linux, macOS, and Windows.

---

### Task 1: Add a deterministic in-progress cancellation regression

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`

**Interfaces:**
- Consumes: `ProcessCancellation`
- Produces: a test-only analysis checkpoint and a regression proving prompt cancellation

- [ ] **Step 1: Write a test that waits until blocking analysis starts, cancels it, and asserts `analyzer.cancelled` within a fixed timeout.**
- [ ] **Step 2: Assert that cancellation returns no `CandidateSpoolRef` and leaves no completed store.**
- [ ] **Step 3: Run `cargo test -p hoimin-cli analyzer::tests::in_progress_analysis_is_cancellable -- --exact` and verify RED because analysis still blocks the executor.**

### Task 2: Make Rust analysis cooperatively cancellable

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/rust.rs`
- Modify: `crates/hoimin-cli/src/analyzer/rust_tests.rs`

**Interfaces:**
- Produces: `analyze_source_cancellable(request, source, cancelled) -> Result<AnalyzerOutput, AnalysisCancelled>`
- Preserves: `analyze_source(request, source) -> AnalyzerOutput` for synchronous planning callers and existing tests

- [ ] **Step 1: Add cancellation tests for pre-cancelled and large token streams and verify RED.**
- [ ] **Step 2: Add the cancellable entry point and checks before/inside long loops.**
- [ ] **Step 3: Run `cargo test -p hoimin-cli --lib analyzer::rust::rust_tests` and verify GREEN.**

### Task 3: Offload parsing and spool writes

**Files:**
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`

**Interfaces:**
- Consumes: owned `AnalyzeFile`, operator selection, profile, source bytes, store, cancellation
- Produces: successful `(AnalysisFinished, Option<CandidateStore>)` or typed `EffectFailed`

- [ ] **Step 1: Move UTF-8 conversion, analysis, candidate conversion, pushes, and finalization into `spawn_blocking`.**
- [ ] **Step 2: Race the join handle with cancellation and map join failure to `analyzer.task`.**
- [ ] **Step 3: Ensure the store is restored only after a successful non-final request.**
- [ ] **Step 4: Run analyzer unit and handler integration tests and verify GREEN.**

### Task 4: Add large-source and run-loop deadline coverage

**Files:**
- Modify: `crates/hoimin-cli/tests/analyzer_handler.rs`
- Modify: the existing shell/run-loop test module selected during implementation

**Interfaces:**
- Produces: portable responsiveness regressions using deterministic synchronization

- [ ] **Step 1: Add a large-source cancellation test that preserves source-order candidates and configured truncation.**
- [ ] **Step 2: Add a total-deadline test using the slow-analysis seam.**
- [ ] **Step 3: Run the focused tests and `cargo test --workspace`.**

### Task 5: Verify and commit

**Files:**
- Modify: only files listed above

- [ ] **Step 1: Run `cargo fmt --all -- --check`.**
- [ ] **Step 2: Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.**
- [ ] **Step 3: Run `cargo test --workspace`.**
- [ ] **Step 4: Commit implementation and push `codex/issue-86`.**
- [ ] **Step 5: Create a PR closing Issue #86 and monitor all CI jobs.**
