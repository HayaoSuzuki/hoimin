# Issue 632 Changed Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to execute this plan task by task.

**Goal:** Select configurable neighboring lines around Git changes without changing default selection.
**Architecture:** Carry a bounded context value through normalized selection to Git unified diff, then reuse existing normalization and explicit intersection.
**Tech Stack:** Rust, clap, serde, Git, Lean 4.
**Spec:** docs/superpowers/specs/2026-09-25-issue-632-changed-context-design.md

## Global Constraints

Separate issue worktree; commit this design and plan before product edits. Do not touch unrelated issues. Record at least three distinct self-reviews of design, plan, implementation and tests. Root coordinates formal files and runs a single Lean process at a time. No publication before final verification.

## Review Focus

Default/historical compatibility, deletion gap off-by-one, supported numeric range, selection intersection, plan/verify persistence, ranking explanations and independent formal expectations.

---

### Task 1: Regression tests (RED)

- [ ] Add CLI parsing assertions for the new option, requires-changed and numeric bounds.
- [ ] Add core deserialization/validation/round-trip tests using JSON to avoid coupling failures to a missing Rust field.
- [ ] Run those tests and record the expected missing-feature failures before implementation.

### Task 2: Rust implementation (GREEN)

- [ ] Add bounded `changed_context` propagation in cli.rs, core/config.rs and core/target.rs with legacy serde default.
- [ ] Pass context into the internal Git resolver; format --unified=N and preserve all other diff flags.
- [ ] Add real Git boundary/intersection tests and public plan/verify persistence coverage.
- [ ] Document CLI use, bound, deletion behavior and changed_line ranking meaning in README.

### Task 3: Formal integration

- [ ] Root contributes independent Nat interval model, proofs, executable cases, sensitivity checks and committed JSONL corpus under formal/HoiminOracle.
- [ ] Wire Lean generator/freshness into CI and test the corpus against public plan behavior, including maximum context and deletion cases.

### Task 4: Review, verify and commit

- [ ] Implementation review 1: trace CLI through raw config, normalized config, Git, ranking and verify.
- [ ] Implementation review 2: inspect boundary handling and compatibility with historical manifests and public effect API.
- [ ] Implementation review 3: inspect final diff for scope, resource behavior and platform assumptions.
- [ ] Test review 1: map each issue acceptance criterion to executable coverage.
- [ ] Test review 2: verify tests use actual Git and independent expected intervals, not mirrored parsing.
- [ ] Test review 3: check negative cases and run workspace tests, formatting and clippy after final edits.
- [ ] Commit implementation and evidence; root performs independent review and creates PR through gh-stack.

## Plan self-review

1. Dependency review: tests that operate on JSON and CLI input can fail behaviorally before adding the new Rust field; product changes follow the observed RED failure.
2. Coverage review: separated parsing/config persistence from Git selection; required deletion boundaries, empty files, large N and explicit restrictions. Added public plan/verify coverage so resolver-only success is insufficient.
3. Execution review: root owns formal/CI and the oracle adapter; issue worker owns Rust/config/docs/tests. Independent targets limit cross-worktree build interference, and Lean runs are serialized. Full workspace checks occur after integration.
