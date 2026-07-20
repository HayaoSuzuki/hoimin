# Target selector documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent focused mutation runs from accidentally scanning a `--source` tree when a user intends to select one file.

**Architecture:** Update copied command examples to use the selector matching their scope. Place the union behavior and its concrete consequence in the target-selector reference, directly after the selector list.

**Tech Stack:** Markdown documentation, ripgrep verification.

## Global Constraints

- Do not change CLI behavior.
- Keep the explicit-selector union contract intact and state it plainly.
- A one-file command uses `--file` without `--source`.

---

### Task 1: Clarify focused-target documentation

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`
- Test: documentation command scan with `rg`

**Interfaces:**

- Consumes: CLI contract in `README.md` stating that explicit selectors form a union.
- Produces: Copyable one-file examples and an unambiguous selector reference.

- [ ] **Step 1: Locate the ambiguous examples and contract**

Run: `rg -n -- "--source|--file|form a union" README.md docs/development.md`

Expected: The one-file examples combine `--source` and `--file`, and the union contract appears only in the explanatory paragraph.

- [ ] **Step 2: Update the README example and selector reference**

Replace the focused-run command with `uvx hoimin run --root . --file src/calc.py --format json -- python -m pytest -q`.

Immediately after the selector list, state that explicit selectors form a union and give this consequence: `--source src --file src/calc.py` selects `src/calc.py` and every Python file below `src`. Recommend `--file` alone for a run limited to named files.

- [ ] **Step 3: Update the development-guide command**

Replace the focused mutation command prefix with `uv run hoimin run --root . --file crates/hoimin-cli/src/analyzer/mod.rs`. Retain all existing limit, format, and test-command arguments.

- [ ] **Step 4: Verify the rendered-source intent**

Run: `rg -n -- "--source .*--file|--file .*--source" README.md docs/development.md`

Expected: No output.

Run: `rg -n -- "form a union|every Python file below|--file alone" README.md`

Expected: Output identifies the union warning and focused-file recommendation.

- [ ] **Step 5: Commit the documentation change**

Run: `git add README.md docs/development.md && git commit -m "docs: clarify target selector union"`
