# Focused Baseline Timeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep focused-mutation package baselines usable after the discovery deadline by assigning them the mutation-phase timeout.

**Architecture:** Preserve the existing `RunBudget` allocation and workflow state machine. Change only the timeout selected for a package baseline, from `discovery_timeout` to `mutation_timeout`, and protect that phase boundary with a workflow-level regression test.

**Tech Stack:** Python 3.14+, `unittest`, existing focused-mutation fake clock and runner.

## Global Constraints

- Do not add a baseline-specific deadline or configuration option.
- Do not change total-budget allocation or reporting reserve policy.
- Do not change baseline failure semantics.

---

### Task 1: Assign baseline commands to the mutation deadline

**Files:**
- Modify: `tests/test_focused_mutation_reporting.py`
- Modify: `tools/focused_mutation.py:258-266`

**Interfaces:**
- Consumes: `RunBudget.mutation_timeout(now: float) -> float` and `Dependencies.monotonic() -> float`.
- Produces: package baseline calls whose runner timeout is the remaining mutation window.

- [ ] **Step 1: Write the failing workflow test**

Add a test that advances the fake clock from `0.0` to `650.0` during candidate discovery, runs the workflow, and inspects the recorded `baseline-hoimin-core` call:

```python
def test_baseline_uses_mutation_deadline_after_discovery_window(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
        clock = FakeClock()
        options, dependencies, runner = workflow_fixture(directory, clock=clock)

        def finish_discovery(*_: object) -> list[Candidate]:
            clock.now = 650.0
            return []

        with mock.patch(
            "tools.focused_mutation.discover_candidates",
            side_effect=finish_discovery,
        ):
            run_workflow(options, dependencies)

        baseline_call = next(
            call for call in runner.calls if call[3] == "baseline-hoimin-core"
        )
        self.assertEqual(baseline_call[2], 850.0)
```

- [ ] **Step 2: Run the regression test and verify RED**

Run:

```bash
uv run --frozen python -m unittest tests.test_focused_mutation_reporting.FocusedMutationReportingTests.test_baseline_uses_mutation_deadline_after_discovery_window -v
```

Expected: FAIL because the baseline timeout is `0.0`, not `850.0`.

- [ ] **Step 3: Commit the RED test**

```bash
git add tests/test_focused_mutation_reporting.py
git commit -m "test: expose expired focused baseline deadline"
```

- [ ] **Step 4: Make the minimal timeout change**

In the `baseline-*` `run_command` call, replace:

```python
budget.discovery_timeout(dependencies.monotonic())
```

with:

```python
budget.mutation_timeout(dependencies.monotonic())
```

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```bash
uv run --frozen python -m unittest tests.test_focused_mutation_budget tests.test_focused_mutation_reporting -v
```

Expected: all focused budget and reporting tests pass.

- [ ] **Step 6: Commit the implementation**

```bash
git add tools/focused_mutation.py
git commit -m "fix: use mutation deadline for focused baselines"
```

### Task 2: Verify and integrate

**Files:**
- Verify: `tools/focused_mutation.py`
- Verify: `tests/test_focused_mutation_reporting.py`

**Interfaces:**
- Consumes: the focused workflow test command and the repository's full Python test command.
- Produces: reviewable evidence for Issue #130 and its pull request.

- [ ] **Step 1: Run the full Python suite**

```bash
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: all platform-applicable tests pass; Windows-only tests may skip on macOS.

- [ ] **Step 2: Assess targeted mutation verification**

Create a temporary plan for the changed baseline-call lines, following
`.agents/skills/hoimin-mutation-testing/SKILL.md`. Verify any candidate that
changes the baseline admission or timeout behavior. If the analyzer produces no
candidate for method-name selection, record that the RED/GREEN regression test
is the direct evidence for this one-token call-site fix.

- [ ] **Step 3: Check the branch diff**

```bash
git diff --check origin/main...HEAD
git status -sb
```

Expected: no whitespace errors and only the intentional `.venv` symlink remains untracked.

- [ ] **Step 4: Request independent review**

Review the complete `origin/main...HEAD` diff against Issue #130 and the design document. Fix every Critical or Important finding and re-run affected verification.

- [ ] **Step 5: Push, create, and merge the PR**

Push `fix/issue-130-baseline-timeout`, create a PR that closes #130, wait for all required CI checks, then squash-merge and remove the worktree.
