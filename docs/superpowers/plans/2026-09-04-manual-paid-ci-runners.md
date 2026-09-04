# Manual Paid CI Runners Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent automatic pull-request and `main` CI from creating Windows or macOS runners while preserving an explicit full-platform manual run.

**Architecture:** A fail-closed Boolean dispatch input selects either an Ubuntu-only JSON runner list or the existing full runner list inside each affected matrix. Repository contract tests pin the trigger, input, affected job set, and allowed labels.

**Tech Stack:** GitHub Actions YAML, Python 3.14 `unittest`, PyYAML, Markdown

**Spec:** `docs/superpowers/specs/2026-09-04-manual-paid-ci-runners-design.md`

## Global Constraints

- Do not trigger GitHub-hosted Windows or macOS while implementing this change.
- Do not change any job's commands, runner label, dependencies, or release workflow.
- Default every automatic and unspecified-manual path to Ubuntu only.
- Use test-first RED/GREEN commits and run the complete local Python contract suite before push.

---

### Task 1: Pin the fail-closed workflow contract

**Files:**

- Modify: `tests/test_ci_workflow.py`

- [ ] Add constants for the four paid-runner matrix jobs and their exact full and automatic OS lists.
- [ ] Add a test that decodes `workflow_dispatch.run_paid_runners` and requires `type: boolean`, `required: true`, and `default: false`.
- [ ] Add a test that requires an Ubuntu-only fallback and the manual opt-in on every affected matrix, while rejecting paid labels from all other CI jobs.
- [ ] Update the two existing static-matrix assertions to assert the new guarded matrix contract.
- [ ] Run `./.venv/bin/python -m unittest tests.test_ci_workflow -v`; expect the new tests to fail because the workflow has no input and still uses static matrices.
- [ ] Commit the RED tests with `git commit -m "test(ci): require manual paid runners"`.

### Task 2: Implement dynamic runner selection

**Files:**

- Modify: `.github/workflows/ci.yml`

- [ ] Define `on.workflow_dispatch.inputs.run_paid_runners` with a precise description and a fail-closed Boolean default.
- [ ] Replace `quality`, `rust`, and `wheel-smoke` OS arrays with an expression that selects either `["ubuntu-latest"]` or `["ubuntu-latest","windows-latest","macos-14"]`.
- [ ] Replace `core-dependency-purity` with the equivalent two-OS full list.
- [ ] Keep all steps, `needs`, and `runs-on: ${{ matrix.os }}` declarations unchanged.
- [ ] Run `./.venv/bin/python -m unittest tests.test_ci_workflow -v`; expect all workflow contracts to pass.

### Task 3: Document deliberate paid-runner execution

**Files:**

- Modify: `docs/development.md`
- Modify: `tests/test_ci_workflow.py`

- [ ] Add a failing documentation contract for the exact `gh workflow run ci.yml --ref <REF> -f run_paid_runners=true` command and the final-ref/once policy.
- [ ] Document that PR and `main` CI are Ubuntu-only, paid runners require manual opt-in, and release workflows are separate.
- [ ] Update the randomized-order wording so it no longer claims Windows and macOS run automatically.
- [ ] Run `./.venv/bin/python -m unittest tests.test_ci_workflow -v`; expect GREEN.
- [ ] Commit implementation and documentation with `git commit -m "ci: make paid runners manual"`.

### Task 4: Verify and publish the policy change

**Files:**

- Verify only

- [ ] Run `./.venv/bin/python -m unittest discover -s tests -p 'test_*.py' -v`.
- [ ] Run `cargo fmt --all -- --check` using the repository's shared Cargo target directory.
- [ ] Run `git diff --check` and inspect `git diff origin/main...HEAD`.
- [ ] Confirm no workflow dispatch was issued and no mutation tests were run.
- [ ] Request code review, resolve concrete findings, and repeat the local checks.
- [ ] Push once, create the pull request, and inspect its check list to confirm no Windows or macOS runner was created.

## Plan Self-Review Record

### Review 1: Coverage

Mapped every paid OS occurrence in `.github/workflows/ci.yml` to a test and an
implementation step. Added documentation because the safe manual command is
part of the operational contract.

### Review 2: Execution Order

Placed the workflow contract before the YAML change and the documentation
contract before prose. Kept verification and publication last so no remote run
occurs against an intermediate commit.

### Review 3: Cost and Recovery

The plan contains no paid dispatch. It uses the existing shared Cargo target,
does not invoke mutation testing, and requires inspecting the PR job list after
the single push. If a paid job appears unexpectedly, cancel it before any
further push and correct the workflow locally.
