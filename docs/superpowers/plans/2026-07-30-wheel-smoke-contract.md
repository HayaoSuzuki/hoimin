# Standalone Wheel-Smoke Contract Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the development guide, README, and standalone wheel-smoke behavior consistently require a prebuilt wheel.

**Architecture:** Preserve the script's artifact-consumer role. Enforce the contract with documentation assertions and a real subprocess launched from an empty repository-shaped directory.

**Tech Stack:** Python 3.14 `unittest`, Markdown documentation, Rust/maturin build workflow.

## Global Constraints

- Do not add implicit wheel builds to `tests/wheel_smoke.py`.
- `HOIMIN_WHEEL` continues to select an exact existing artifact.
- Without `HOIMIN_WHEEL`, `target/wheels` must already contain a compatible wheel.

---

### Task 1: Lock the standalone contract

**Files:**
- Modify: `tests/test_wheel_smoke.py`
- Modify: `docs/development.md`
- Verify: `README.md`

**Interfaces:**
- Consumes: `tests/wheel_smoke.py::main` and its existing missing-wheel diagnostic.
- Produces: executable documentation and subprocess coverage for the prebuilt-wheel contract.

- [ ] **Step 1: Add a failing documentation contract test**

Assert that the development guide says the wheel must be built before the
standalone script and that both the guide and README contain the build command
before the smoke command.

- [ ] **Step 2: Run the test and confirm RED**

Run:
`uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v`

Expected: failure because `docs/development.md` claims the script auto-builds.

- [ ] **Step 3: Add the empty-directory subprocess test**

Copy `tests/wheel_smoke.py` beneath a temporary repository root, execute it with
`HOIMIN_WHEEL` removed, and assert a nonzero exit, the `build a wheel first`
diagnostic, and no created `target/wheels` directory.

- [ ] **Step 4: Correct the development guide**

State that callers must run `uv run maturin build --release` first unless
`HOIMIN_WHEEL` selects an existing artifact.

- [ ] **Step 5: Verify GREEN and the full Python suite**

Run the focused discovery command, then:
`uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v`

- [ ] **Step 6: Commit**

Commit the tests and documentation together, then push the issue branch and
create a PR closing Issue #70.
