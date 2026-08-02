# Windows Process Tree Termination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Terminate Cargo and all descendants when a focused-mutation command times out or is interrupted on Windows.

**Architecture:** Add a small, injectable `taskkill /T /F` adapter and route the Windows lifecycle branch through it. Treat adapter failure as a lifecycle failure after best-effort root cleanup so the runner cannot start another command while descendants may still own logs or Cargo locks.

**Tech Stack:** Python >=3.14,<3.15, `unittest`, Windows `taskkill`, existing `CommandRunner` lifecycle model.

## Global Constraints

- Keep POSIX process-group termination unchanged.
- Do not introduce Job Objects or recursive process enumeration.
- Bound taskkill and root reaping to two seconds each.
- Fail closed and block runner reuse if tree termination cannot be confirmed.

---

### Task 1: Add the taskkill adapter

**Files:**
- Modify: `tools/focused_mutation_support/runner.py`
- Modify: `tests/test_focused_mutation_runner.py`

**Interfaces:**
- Produces: `terminate_windows_process_tree(pid: int, *, run: Callable[..., Any] | None = None) -> None`.
- Produces: `WINDOWS_PROCESS_TERMINATION_TIMEOUT = 2.0`.

- [ ] **Step 1: Write failing adapter tests**

Import `terminate_windows_process_tree`. Add one test with an injected mock runner that returns `CompletedProcess(..., 0)` and assert this exact call:

```python
run.assert_called_once_with(
    ["taskkill", "/PID", "12345", "/T", "/F"],
    stdin=subprocess.DEVNULL,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
    timeout=2.0,
    check=False,
)
```

Add a second test returning status `1` and assert `OSError` contains `taskkill exited with status 1`.

- [ ] **Step 2: Run adapter tests and verify RED**

Run:

```bash
uv run --frozen python -m unittest \
  tests.test_focused_mutation_runner.RunnerTests.test_windows_tree_terminator_uses_taskkill \
  tests.test_focused_mutation_runner.RunnerTests.test_windows_tree_terminator_rejects_nonzero_exit -v
```

Expected: import or attribute failure because the adapter does not exist.

- [ ] **Step 3: Commit RED tests**

```bash
git add tests/test_focused_mutation_runner.py
git commit -m "test: expose root-only Windows termination"
```

- [ ] **Step 4: Implement the adapter**

Add the timeout constant and function. Select `subprocess.run` only when no test runner is injected, invoke the exact argument vector above, and raise `OSError` for a nonzero return code.

- [ ] **Step 5: Run adapter tests and verify GREEN**

Run the two exact tests from Step 2. Expected: both pass.

### Task 2: Route Windows lifecycle cleanup through the tree adapter

**Files:**
- Modify: `tools/focused_mutation_support/runner.py`
- Modify: `tests/test_focused_mutation_runner.py`

**Interfaces:**
- Consumes: `terminate_windows_process_tree(pid)`.
- Extends: `CommandRunner.__init__(..., windows_tree_terminator: Callable[[int], None] = terminate_windows_process_tree)`.
- Extends: `ProcessLifecycleError(pid: int, message: str | None = None)` without changing the existing default message.

- [ ] **Step 1: Write failing lifecycle tests**

Extend the test `runner` factory to accept a tree terminator. Add a Windows-branch interruption test that patches `tools.focused_mutation_support.runner.os.name` to `"nt"`, injects a mock terminator, and asserts it receives PID `12345` while `process.terminate()` is not called.

Add a failure test whose terminator raises `OSError("taskkill unavailable")`. Assert the root process receives `kill()`, the command record contains:

```text
process lifecycle cleanup failed: process tree 12345 termination failed: OSError: taskkill unavailable
```

Then assert a second `runner.run` raises `ProcessLifecycleError` without creating another process.

- [ ] **Step 2: Run lifecycle tests and verify RED**

Run the two new exact tests. Expected: constructor or assertion failure because Windows still calls root `terminate()`.

- [ ] **Step 3: Implement injected fail-closed tree termination**

Store the injected callable. In `_terminate`, call it for the Windows branch. If it raises `OSError` or `subprocess.SubprocessError`, kill and reap the root best-effort, then raise `ProcessLifecycleError` with the exact failure message. Retain the existing post-tree root wait and forced-kill fallback.

- [ ] **Step 4: Run the runner test module**

```bash
uv run --frozen python -m unittest tests.test_focused_mutation_runner -v
```

Expected: all platform-applicable tests pass; Windows-only integration tests may skip on macOS.

- [ ] **Step 5: Commit the implementation**

```bash
git add tools/focused_mutation_support/runner.py tests/test_focused_mutation_runner.py
git commit -m "fix: terminate focused Windows process trees"
```

### Task 3: Verify and integrate

**Files:**
- Verify: `tools/focused_mutation_support/runner.py`
- Verify: `tests/test_focused_mutation_runner.py`

**Interfaces:**
- Produces: local, mutation, review, and CI evidence for Issue #131.

- [ ] **Step 1: Run the full Python suite**

```bash
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

- [ ] **Step 2: Run targeted mutation verification**

Following `.agents/skills/hoimin-mutation-testing/SKILL.md`, plan the new adapter and Windows branch lines, then verify exact relevant candidates. Strengthen behavioral tests for any non-equivalent survivor.

- [ ] **Step 3: Check diff and request independent review**

```bash
git diff --check origin/main...HEAD
git status -sb
```

Request review of the full diff against Issue #131 and the design. Resolve every Critical or Important finding and re-run affected verification.

- [ ] **Step 4: Push, create, and merge the PR**

Push `fix/issue-131-windows-process-tree`, create a PR closing #131, monitor all CI checks, squash-merge, and remove the worktree.
