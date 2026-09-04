# Automatic Linux and Manual Non-Linux CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep Linux CI automatic while moving all Windows and macOS validation into an independent manually dispatched workflow whose results are not automatic conditions.

**Architecture:** The automatic workflow retains one-element Ubuntu matrices. A new workflow with only `workflow_dispatch` owns the existing non-Linux job variants; its jobs are independent and use distinct manual check names. Repository contract tests pin both sides of the split.

**Tech Stack:** GitHub Actions YAML, Python 3.14 `unittest`, PyYAML, Markdown

**Spec:** `docs/superpowers/specs/2026-09-04-manual-non-linux-ci-design.md`

## Global Constraints

- Do not trigger GitHub-hosted Windows or macOS during implementation.
- Do not describe Linux as free; it remains the deliberately automatic platform.
- Do not connect a manual job through `needs`, outputs, or an `if` condition to automatic CI.
- Preserve each platform's current commands and runner labels.
- Keep release and scheduled workflows unchanged.

---

### Task 1: Pin the workflow split with failing contracts

**Files:**

- Modify: `tests/test_ci_workflow.py`

- [ ] Add paths and constants for the automatic Linux matrices and the seven manual non-Linux variants.
- [ ] Add a test requiring the automatic workflow to contain no `windows-latest` or `macos-14` label and to use only `[ubuntu-latest]` in all four matrices.
- [ ] Add a test requiring the manual workflow to exist with only a `workflow_dispatch` trigger.
- [ ] Add a test requiring the exact manual OS coverage, distinct `Manual` names, and no `needs` or `if` on any manual job.
- [ ] Update existing static matrix assertions to the Linux-only automatic contract.
- [ ] Run `./.venv/bin/python -m unittest tests.test_ci_workflow -v`; expect RED because automatic CI still contains non-Linux runners and the manual workflow does not exist.
- [ ] Commit the RED contracts with `git commit -m "test(ci): require manual non-Linux workflow"`.

### Task 2: Make automatic CI Linux-only

**Files:**

- Modify: `.github/workflows/ci.yml`

- [ ] Change `quality`, `rust`, `core-dependency-purity`, and `wheel-smoke` to static `[ubuntu-latest]` matrices.
- [ ] Verify no Windows/macOS label or trigger-dependent OS expression remains in `ci.yml`.
- [ ] Preserve all commands, job dependencies, automatic triggers, and Linux-only jobs.
- [ ] Run the focused workflow contracts; expect only the missing manual-workflow assertions to remain RED.

### Task 3: Add independent manual non-Linux validation

**Files:**

- Create: `.github/workflows/non-linux-ci.yml`

- [ ] Add only the `workflow_dispatch` trigger and read-only contents permission.
- [ ] Copy the existing Windows/macOS quality, Rust test, core-purity, and wheel-smoke commands without adding Linux.
- [ ] Prefix every displayed job name with `Manual`.
- [ ] Give every manual job its own complete setup and omit `needs`, job `if`, outputs, and automatic-workflow calls.
- [ ] Run the focused workflow contracts; expect GREEN.

### Task 4: Document one-shot execution

**Files:**

- Modify: `tests/test_ci_workflow.py`
- Modify: `docs/development.md`

- [ ] Add a failing documentation contract for `gh workflow run non-linux-ci.yml --ref <REF>` and the final-ref/once policy.
- [ ] Document that Linux CI is automatic and consumes runner capacity, while non-Linux CI is separate, manual, and non-gating.
- [ ] Update the randomized-order wording so it no longer claims Windows and macOS run automatically.
- [ ] Run the focused workflow contracts; expect GREEN.
- [ ] Commit workflow and documentation with `git commit -m "ci: make non-Linux validation manual"`.

### Task 5: Verify and publish the policy change

**Files:**

- Verify only

- [ ] Run `./.venv/bin/python -m unittest discover -s tests -p 'test_*.py' -v`.
- [ ] Run `cargo fmt --all -- --check` with the shared Cargo target directory.
- [ ] Run `git diff --check` and inspect `git diff origin/main...HEAD`.
- [ ] Confirm no workflow dispatch and no mutation test was run.
- [ ] Request code review, address concrete findings, and repeat affected checks.
- [ ] Push once and create the pull request.
- [ ] Immediately inspect the PR run; if a Windows or macOS runner appears, cancel that run before making any further remote update.

## Plan Self-Review Record

### Review 1: Interpretation

Translated the updated requirement into two independent workflows instead of a
conditional runner list. The plan makes Linux automatic without calling it
free, and gives non-Linux results no role in the automatic job graph.

### Review 2: Regression Surface

Preserved Linux matrix job names, commands, and dependencies. The manual side
copies every removed platform variant and uses contract tests to prevent either
automatic reintroduction or manual coverage loss.

### Review 3: Remote Execution Safety

All implementation checks are local. The branch is pushed once after review,
and PR runs are inspected immediately. The plan never dispatches the manual
workflow; a cross-platform run occurs only later, deliberately, against a final
ref.
