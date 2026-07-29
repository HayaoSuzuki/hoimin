# Post-kill Reap Failure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make failed post-kill reaping observable and prevent command reuse while root-process state is unknown.

**Architecture:** Raise a typed lifecycle error from the final reap boundary, retain timeout/interruption as the primary command outcome while recording cleanup diagnostics, and poison the owning runner before another process can start. Extend persisted and Markdown reports with the failure and verify real process-tree termination on POSIX.

**Tech Stack:** Python 3.11+, `subprocess`, `unittest`, JSON checkpoints, Markdown reporting

## Global Constraints

- The original command timeout or interruption remains the primary outcome.
- A final post-kill wait timeout records a typed lifecycle failure.
- `exit_code=None` is never presented as successful cleanup.
- A poisoned runner rejects later commands before process creation.
- Timeout cleanup terminates descendants on supported platforms.

---

### Task 1: Specify lifecycle failure and fail-closed reuse

**Files:**
- Modify: `tests/test_focused_mutation_runner.py`
- Modify: `tools/focused_mutation_support/runner.py`

**Interfaces:**
- Produces: `ProcessLifecycleError`
- Produces: `CommandRunner._lifecycle_error: ProcessLifecycleError | None`
- Consumes: `CommandTimedOut.record` and `CommandInterrupted.record`

- [ ] **Step 1: Write deterministic failing runner tests**

Add a fake process whose first wait raises `TimeoutExpired` or
`KeyboardInterrupt`, whose graceful wait raises `TimeoutExpired`, and whose
post-kill wait also raises `TimeoutExpired`. Assert the outer exception remains
`CommandTimedOut` or `CommandInterrupted`, `record.exit_code is None`,
`record.cleanup_errors` names failed post-kill reaping, and a second `run`
raises `ProcessLifecycleError` without another `Popen` call.

- [ ] **Step 2: Run tests to verify RED**

Run:

```console
uv run --frozen python -m unittest \
  tests.test_focused_mutation_runner.RunnerTests.test_timeout_records_failed_post_kill_reap_and_blocks_reuse \
  tests.test_focused_mutation_runner.RunnerTests.test_interruption_records_failed_post_kill_reap_and_blocks_reuse -v
```

Expected: fail because `ProcessLifecycleError` and fail-closed runner state do
not exist.

- [ ] **Step 3: Implement the minimal lifecycle boundary**

Define `ProcessLifecycleError` with PID context. Make `_terminate` raise it
after the final wait expires. In `run`, retain it on the runner, append its
diagnostic to the current command cleanup errors, and preserve the original
timeout/interruption outcome. At the start of `run`, raise a retained lifecycle
error before incrementing sequence or calling `Popen`.

- [ ] **Step 4: Run focused tests to verify GREEN**

Run the two tests from Step 2 and expect both to pass.

### Task 2: Persist and render unknown lifecycle state

**Files:**
- Modify: `tests/test_focused_mutation_reporting.py`
- Modify: `tools/focused_mutation_support/reporting.py`

**Interfaces:**
- Consumes: `CommandRecord.cleanup_errors`
- Produces: Markdown `Command cleanup failures` section

- [ ] **Step 1: Write failing reporting tests**

Build a command record with `exit_code=None` and a post-kill reap diagnostic.
Assert `RunRecord.to_dict()` retains both values and `render_markdown` includes
the command label, `unknown` exit status, and diagnostic.

- [ ] **Step 2: Run test to verify RED**

Run:

```console
uv run --frozen python -m unittest \
  tests.test_focused_mutation_reporting.FocusedMutationReportingTests.test_report_exposes_unknown_exit_after_lifecycle_cleanup_failure -v
```

Expected: fail because Markdown omits command cleanup failures.

- [ ] **Step 3: Implement cleanup-failure rendering**

Append a Markdown section containing one bullet per cleanup diagnostic. Render
`exit unknown` when `exit_code is None`; render the numeric exit code
otherwise. Keep the section concise and omit command stdout/stderr contents.

- [ ] **Step 4: Run focused reporting tests to verify GREEN**

Run the test from Step 2 and expect it to pass.

### Task 3: Verify handled timeout leaves no POSIX descendants

**Files:**
- Modify: `tests/test_focused_mutation_runner.py`

**Interfaces:**
- Consumes: real `CommandRunner.run` POSIX process-group termination
- Produces: platform integration regression coverage

- [ ] **Step 1: Add the platform integration test**

On POSIX, launch a fixture root that writes its PID and a child PID before
sleeping. Run it through `CommandRunner` with a bounded timeout, catch
`CommandTimedOut`, and poll `os.kill(pid, 0)` until both PIDs are absent.

- [ ] **Step 2: Run the integration test**

Run:

```console
uv run --frozen python -m unittest \
  tests.test_focused_mutation_runner.RunnerTests.test_handled_timeout_leaves_no_posix_descendants -v
```

Expected: pass on POSIX and skip on Windows, whose existing inherited-handle
test exercises its platform path.

### Task 4: Verify and publish

**Files:**
- Inspect: `README.md`
- Verify: all changed files

**Interfaces:**
- Consumes: Tasks 1-3
- Produces: pull request closing #67

- [ ] **Step 1: Run focused verification**

```console
uv run --frozen python -m unittest discover -s tests -p 'test_focused_mutation*.py' -v
```

- [ ] **Step 2: Run full repository verification**

```console
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
```

All commands must exit zero.

- [ ] **Step 3: Review documentation impact**

Confirm the change remains internal to the development runner. Do not modify
README unless a published user-facing contract changed. If README alone needs
a follow-up commit, give that commit a `[skip ci]` suffix.

- [ ] **Step 4: Commit, review, push, and create PR**

Commit code and tests without skipping CI, request code review against
`origin/main`, resolve Critical and Important findings, push
`fix/issue-67-post-kill-reap`, and create a PR that closes #67.
