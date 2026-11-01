# Progress Summary Consistency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject progress reports whose embedded mutation summary differs from the canonical summary of their mutant events.

**Architecture:** Extend the existing structural-validation boundary in `progress/input.rs`. Extract typed mutant statuses only after event-kind validation, recompute with `hoimin_core::summarize`, and compare the whole `MutationSummary`.

**Tech Stack:** Rust, serde JSON, hoimin-core report types, cargo tests.

## Global Constraints

- Return `ProgressError::InvalidStructure` for every summary/event mismatch.
- Compare canonical score exactly, including `None` for no decidable mutants.
- Preserve the existing CLI infrastructure-error mapping.

---

### Task 1: Add failing consistency tests

**Files:**
- Modify: `crates/hoimin-cli/tests/progress.rs`

**Interfaces:**
- Consumes: `read_report(&Path) -> Result<InputReport, ProgressError>`
- Produces: acceptance coverage for canonical summary validation

- [ ] Add table-driven reports that mutate counts, records, `inconclusive`, and `score`.
- [ ] Add valid controls for ordinary and null-score summaries.
- [ ] Run `cargo test -p hoimin-cli --test progress` and confirm the mismatch cases fail.

### Task 2: Validate the canonical summary

**Files:**
- Modify: `crates/hoimin-cli/src/progress/input.rs`

**Interfaces:**
- Consumes: `hoimin_core::summarize(&[MutationStatus]) -> MutationSummary`
- Produces: `InvalidStructure` before the usability classification

- [ ] Import `summarize` and collect statuses from structurally validated mutant events.
- [ ] Compare the recomputed summary with `RunFinished.counts`.
- [ ] Return the stable message `summary counts must match mutant events` on mismatch.
- [ ] Run the focused progress suite and confirm all cases pass.

### Task 3: Verify CLI behavior and repository quality

**Files:**
- Modify: `crates/hoimin-cli/tests/progress.rs`

**Interfaces:**
- Consumes: the existing progress CLI error renderer
- Produces: exit-code-2 regression coverage

- [ ] Add a CLI invocation using a contradictory report and assert exit code 2 plus the invalid-structure diagnostic.
- [ ] Run formatting, clippy, workspace tests, contracts tests, and Python tests.
- [ ] Commit the implementation and request code review.
