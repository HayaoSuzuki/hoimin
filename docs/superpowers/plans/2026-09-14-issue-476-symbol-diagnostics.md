# Issue #476 Symbol Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Reject missing explicit definitions before baseline across run, plan and verify.
**Architecture:** Parse symbol-selected files during shared target resolution before Git intersection; collect exact AST qualnames independently of mutation eligibility.
**Tech Stack:** Rust, Ruff AST, Tokio, existing integration fixtures.
**Spec:** ../specs/2026-09-14-issue-476-symbol-diagnostics-design.md

## Global Constraints

- No Python code is executed for symbol validation.
- Preserve zero-candidate selections of existing definitions.
- Use `CARGO_BUILD_JOBS=2` and the shared existing Cargo target cache.
- Run inline without subagents; autonomous execution and PR are authorized.

## Task 1: Public regression and shared validation

Files: `crates/hoimin-cli/tests/plan.rs`, `crates/hoimin-cli/src/target/mod.rs`, `crates/hoimin-cli/src/analyzer/{mod.rs,rust.rs}`.
Consumes: `TargetHandler::resolve(&Selection) -> Result<Vec<TargetSlice>, TargetError>`.
Produces: crate-private `definition_names(source: &str) -> Result<BTreeSet<String>, String>`.

- [x] Add CLI tests using `plan_args`, change command to `run` as appropriate, and assert `exit == 2`, stderr contains `calc:missing`, and the baseline marker does not exist. Include multiple selectors, clean Git, and a valid edited manifest passed to verify.
- [x] Run `cargo test -p hoimin-cli --test plan symbol_definition` and observe the absent diagnostic failures.
- [x] Add `definition_names`: parse full source, reject syntax errors, run existing depth guard, visit function/class definitions with a qualname stack and insert `stack.join(".")` into a `BTreeSet`; walk control-flow bodies normally.
- [x] In target resolution, for each explicit target with nonempty symbols, read source, collect definitions once, and reject any requested qualname absent from the set before `resolve_changed_scoped`. Attach matching original `module:qualname` selectors and resolved file to errors.
- [x] Assert valid methods, classes, nested/async definitions, package init, and definitions lacking selected operators remain successful. Check changed-other-definition and clean Git empty selections.
- [x] Run focused tests then relevant target, analyzer, plan suites and Rust formatting/Clippy.

## Task 2: Contracts and review evidence

Files: README.md, docs/knowledge/design/selection-plan-verify.md, docs/knowledge/references/{design-documents,audit-documents}.md, docs/superpowers/reports/2026-09-14-issue-476-symbol-diagnostics-review.md.

- [x] Document exact definition validation versus empty candidate sets in README and the existing selection contract.
- [x] Record three separate actual self-reviews for OKF, design, plan and implementation with findings and corrections.
- [x] Add source metadata and footnotes for final spec/report content; validate YAML, local links, source hashes and claim scope separately.
- [ ] Record actual checks and limitations, commit changes, push branch and create PR using repository template with `Closes #476`.
