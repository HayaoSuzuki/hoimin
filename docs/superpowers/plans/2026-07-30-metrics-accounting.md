# Metrics Accounting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce internally consistent discovered, executed, and per-worker process accounting.

**Architecture:** Add typed validation errors in hoimin-core, perform checked cross-field validation at the metrics boundary, preserve validation before persistence, and document schema constraints.

**Tech Stack:** Rust 2024, serde, thiserror, JSON Schema draft 2020-12.

## Global Constraints

- `executed` must not exceed `discovered`.
- Checked worker process sum must equal `executed`.
- Overflow must have a dedicated typed error.
- No invalid metrics document may replace an existing destination.

---

### Task 1: Add RED accounting tests

**Files:**
- Modify: `crates/hoimin-core/tests/telemetry.rs`
- Modify: `crates/hoimin-cli/src/metrics.rs`

- [ ] **Step 1: Add executed-above-discovered, lower-sum, higher-sum, overflow, valid multi-worker, and zero-work cases.**
- [ ] **Step 2: Add a writer test proving invalid metrics do not replace the destination.**
- [ ] **Step 3: Run focused tests and verify RED.**

### Task 2: Implement typed validation

**Files:**
- Modify: `crates/hoimin-core/src/telemetry.rs`
- Modify: `crates/hoimin-cli/src/metrics.rs`

- [ ] **Step 1: Add `MetricsValidationError` with typed accounting variants.**
- [ ] **Step 2: Enforce discovered/executed ordering and checked worker sum.**
- [ ] **Step 3: Convert core errors to display text only at the CLI metrics error boundary.**
- [ ] **Step 4: Run focused tests and verify GREEN.**

### Task 3: Align JSON Schema documentation

**Files:**
- Modify: `docs/json-schema/run-metrics.schema.json`

- [ ] **Step 1: Add descriptions for the run-wide and worker accounting invariants.**
- [ ] **Step 2: Preserve all nonnegative integer constraints and strict additional properties.**

### Task 4: Verify and publish

- [ ] **Step 1: Run format, clippy, workspace tests, and Python schema tests.**
- [ ] **Step 2: Commit, push, create a PR closing Issue #84, and monitor CI.**
