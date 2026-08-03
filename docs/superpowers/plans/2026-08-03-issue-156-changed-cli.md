# Changed-Selection CLI Integration Test Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Verify that `run --changed` and `plan --changed --diff-base` expose the same single mutation candidate for the only edited function in a committed Git project.

**Architecture:** Build independent temporary Git repositories at the CLI integration boundary. Commit a two-function Python source, record the base revision before editing, and change only line 2. Exercise run and plan through `run_with_io`, compare each response to a hand-written full candidate tuple, and compare their stable candidate IDs exactly.

**Tech Stack:** Rust, Tokio integration tests, temporary Git repositories, `serde_json`, `hoimin-cli` JSON run and plan output.

## Global Constraints

- Change only `crates/hoimin-cli/tests/run_e2e.rs`, `crates/hoimin-cli/tests/plan.rs`, and this plan.
- Do not add or change a production API.
- Commit `src/calc.py` with both `changed` and `untouched` before editing only line 2.
- Select only `binary_add_sub` candidates.
- Derive the expected tuple by hand; never derive it from run or plan output.
- Reject empty output, universal predicates over possibly empty collections, and symbol-only assertions.

---

### Task 1: Exercise `run --changed`

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

- [ ] **Step 1: Build the committed fixture**

Initialize a temporary Git repository, configure a local test identity, commit `src/calc.py`, and then append `  # changed` to the return statement on line 2 without changing the second function.

- [ ] **Step 2: Run the complete CLI workflow**

Invoke `run --changed --operators binary_add_sub` with the controlled Python interpreter and parse its JSON report.

- [ ] **Step 3: Assert the independent oracle**

Require exactly one mutant and the exact tuple `(src/calc.py, 2, 13, binary_add_sub, +, -, changed)`.

### Task 2: Exercise `plan --changed --diff-base`

**Files:**
- Modify: `crates/hoimin-cli/tests/plan.rs`

- [ ] **Step 1: Preserve the base revision**

Resolve `HEAD` after committing the two-function fixture and before editing line 2.

- [ ] **Step 2: Compare run and plan boundaries**

Run `run --changed` and `plan --changed --diff-base <base>` with `binary_add_sub`. Require exactly one candidate from each command, compare both to the same hand-written full tuple, and require exact equality of their stable candidate-ID sets.

- [ ] **Step 3: Prove the assertions are load-bearing**

Temporarily bypass `intersect_changed` in target resolution. Run both focused tests and require each to fail because it observes two candidates instead of one. Restore the production source immediately and rerun both tests successfully.

### Task 3: Verify and Commit

**Files:**
- Verify: `crates/hoimin-cli/tests/run_e2e.rs`
- Verify: `crates/hoimin-cli/tests/plan.rs`
- Verify: `docs/superpowers/plans/2026-08-03-issue-156-changed-cli.md`

- [ ] **Step 1: Run focused and complete integration suites**

Run the two exact focused tests, followed by the complete `run_e2e` and `plan` integration-test targets.

- [ ] **Step 2: Run static checks**

Run `cargo fmt --all -- --check`, Clippy for `hoimin-cli` tests with warnings denied, and `git diff --check`.

- [ ] **Step 3: Review scope and commit**

Confirm that only the two named test files and this plan changed, preserve the untracked `.venv` symlink, and commit as `test: exercise changed selection through the CLI`.
