# Default Analyzer Operators Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this bounded change inline.

**Goal:** Enable the seven user-selected operators by default with explicit opt-out.

**Architecture:** Centralize the 50-ID default in MutationOperatorSelection. Keep
all_legacy, explicit selections, family selectors, and persisted selections stable.

**Tech Stack:** Rust, public CLI tests, Lean.

**Spec:** [design](../reports/default-analyzer-operators/design.md).

## Global constraints

69 registered IDs; 50 default IDs; frozen 43-ID legacy set. No collector/schema
changes. CPython 3.14 for public operator tests. Clean cargo artifacts at completion.

## Review focus

- Omitted CLI selectors must match Rust Default: core and public CLI tests.
- Excluding promoted IDs must recover legacy candidates: public CLI comparison.
- Explicit operators must not union with defaults: core/public CLI tests.
- Stored old selections must survive reload: PlanConfig and public verify tests.
- Extra candidates must not silently invalidate old test intent: classify full-suite failures individually.

## Task 1: Selection policy and behavioral contracts

Files: crates/hoimin-core/src/config.rs; core tests/operator_selection.rs;
CLI tests/default_operator_selection.rs, rust_analyzer.rs, statement_delete.rs.
Consumes RawRunConfig and MutationOperatorSelection; produces normalized RunConfig
with the seven exact IDs specified in the design added to default().

- [x] Add exact default/legacy membership, raw fallback, opt-out, explicit override and saved-plan tests; run core test and observe expected failure.
- [x] Add seven IDs to Default and use it for raw fallback; preserve all_legacy.
- [x] Update promoted operator assertions; add CLI candidate/serialized-selection/verify contracts and run focused tests.
- [x] Classify and fix only affected fixtures from cargo test --offline --workspace; rerun required tests.

## Task 2: Proof, documentation and delivery

Files: formal/HoiminOracle/DefaultOperatorSelection.lean; README.md;
CLI tests/cli_config.rs; docs/knowledge design/reference catalogs; this report directory.

- [x] Prove exclusion precedence, explicit override, default membership and persistence identity with bounded Lean execution.
- [x] Measure candidate counts on a bounded real-project fixture and record limitations.
- [x] Update current user documentation; preserve historical issue reports.
- [x] Run fmt/clippy/full tests; conduct five implementation and test self-reviews and independent review.
Delivery: commit all source, tests and docs; open a PR against main; cargo clean and remove task scratch files. The PR and completion message record delivery.

## Plan self-reviews

1. Spec coverage: each selected ID and both configuration paths are covered by task 1.
2. Step precision: separate RED and policy edits; no analyzer rewrite or registry extension.
3. Interface consistency: verified RawRunConfig, RunConfig, PlanConfig, include/exclude and all_legacy signatures.
4. Failure focus: added explicit override, all-seven exclusion, saved-plan reload and capped-prefix review.
5. Proportion/resources: one worktree and two tasks suffice; bounded proof and debug-free builds limit disk use.
