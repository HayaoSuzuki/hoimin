# Windows Timeout Log Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Python `CommandRunner` wait boundedly for Windows stdout/stderr delete readiness while preserving timeout/interruption outcomes, partial logs, and process reaping.

**Architecture:** Add a focused Windows file probe module that tests delete access without changing the log, then call a bounded polling helper only after `CommandRunner` closes its parent streams. Persist path-specific failures on `CommandRecord.cleanup_errors`; retain `CommandTimedOut` and `CommandInterrupted` as the primary outcomes.

**Tech Stack:** Python 3.14 standard library (`ctypes`, `dataclasses`, `pathlib`, `subprocess`, `time`, `unittest`, `unittest.mock`), Windows `CreateFileW`/`CloseHandle`, uv, GitHub Actions.

## Global Constraints

- Keep the design and implementation plan in this worktree and include them in the Issue #47 PR.
- Support Python `>=3.14,<3.15`.
- Do not modify the Rust process supervisor or resource backends.
- Do not add broad CI or test retries.
- Do not suppress `PermissionError` from `TemporaryDirectory`.
- Do not wait without a fixed upper bound.
- Do not delete, rename, or truncate logs as part of readiness checking.
- Do not change command timeout classification or child exit-code semantics.
- Preserve partial timeout logs and the existing terminate, kill, and reap sequence.
- On cleanup failure, preserve `CommandTimedOut` or `CommandInterrupted` and add path-specific diagnostics to `CommandRecord.cleanup_errors`.

## File Structure

- Create `tools/focused_mutation_support/windows_file.py` — owns the Windows API delete-access probe and no runner policy.
- Modify `tools/focused_mutation_support/model.py` — persists cleanup diagnostics on `CommandRecord`.
- Modify `tools/focused_mutation_support/runner.py` — owns bounded polling, post-close lifecycle ordering, and primary exception precedence.
- Modify `tests/test_focused_mutation_runner.py` — covers probe behavior, polling deadlines, lifecycle ordering, timeout/interruption precedence, partial logs, and Windows inherited-handle cleanup.
- Modify `tests/test_focused_mutation_reporting.py` — proves cleanup diagnostics survive `RunRecord.to_dict()` JSON encoding.

---

### Task 1: Persist Command Cleanup Diagnostics

**Files:**
- Modify: `tools/focused_mutation_support/model.py:30-43`
- Modify: `tests/test_focused_mutation_reporting.py:185-202`

**Interfaces:**
- Consumes: Existing `CommandRecord` dataclass and recursive `RunRecord.to_dict()` encoder.
- Produces: `CommandRecord.cleanup_errors: list[str]`, defaulting to a fresh empty list for every record.

- [ ] **Step 1: Write the failing persistence test**

Add this test to `FocusedMutationReportingTests`:

```python
def test_command_cleanup_errors_are_json_serializable(self) -> None:
    record = fixture_record(candidates=[], state=RunState.COMMAND_FAILED)
    command = command_record()
    command.cleanup_errors.append(
        "stderr log was not delete-ready within 2.0 seconds"
    )
    record.commands.append(command)

    encoded = json.loads(json.dumps(record.to_dict()))

    self.assertEqual(
        encoded["commands"][0]["cleanup_errors"],
        ["stderr log was not delete-ready within 2.0 seconds"],
    )
    self.assertEqual(CommandRecord(
        sequence=2,
        label="empty",
        argv=[],
        cwd=".",
        started_at="2026-07-28T00:00:00+00:00",
    ).cleanup_errors, [])
```

- [ ] **Step 2: Run the test and verify the missing field fails**

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_reporting.FocusedMutationReportingTests.test_command_cleanup_errors_are_json_serializable -v
```

Expected: FAIL because `CommandRecord` has no `cleanup_errors` attribute.

- [ ] **Step 3: Add the dataclass field**

Append the field to `CommandRecord` in `model.py`:

```python
cleanup_errors: list[str] = field(default_factory=list)
```

Keep it after `stderr_path` so all existing positional constructor arguments retain their meaning.

- [ ] **Step 4: Run the focused reporting test**

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_reporting.FocusedMutationReportingTests.test_command_cleanup_errors_are_json_serializable -v
```

Expected: PASS, including JSON encoding and the independent empty-list default.

- [ ] **Step 5: Commit the model contract**

```powershell
git add tools/focused_mutation_support/model.py tests/test_focused_mutation_reporting.py
git commit -m "feat: record command cleanup errors"
```

---

### Task 2: Add a Side-Effect-Free Windows Delete-Access Probe

**Files:**
- Create: `tools/focused_mutation_support/windows_file.py`
- Modify: `tests/test_focused_mutation_runner.py:1-20`

**Interfaces:**
- Consumes: An existing `Path` naming a command log.
- Produces: `probe_delete_access(path: Path) -> None`; returns when Windows grants delete access, otherwise raises `OSError` with the native `winerror`.

- [ ] **Step 1: Write a Windows-only probe contract test**

Import `probe_delete_access`, then add:

```python
@unittest.skipUnless(os.name == "nt", "requires Windows file sharing")
def test_delete_probe_reports_a_live_nonsharing_handle(self) -> None:
    from tools.focused_mutation_support.windows_file import (
        open_without_delete_sharing_for_tests,
    )

    path = self.output / "locked.log"
    path.write_bytes(b"partial")
    handle = open_without_delete_sharing_for_tests(path)
    self.addCleanup(handle.close)

    with self.assertRaises(OSError) as caught:
        probe_delete_access(path)

    self.assertEqual(caught.exception.winerror, 32)
    self.assertEqual(path.read_bytes(), b"partial")
```

The test-only helper returns a small context-owning object with `close()`. It exists in the same module so the regression uses the exact complementary Windows sharing flags rather than an unrelated Python `open()` implementation.

- [ ] **Step 2: Run the probe contract and observe the missing module**

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_runner.RunnerTests.test_delete_probe_reports_a_live_nonsharing_handle -v
```

Expected on Windows: ERROR with `ModuleNotFoundError`. On non-Windows: the test is skipped after import placement is kept inside the test; validate the red state on the Windows development/CI runner before merging.

- [ ] **Step 3: Implement the Windows handle wrapper and probe**

Create `windows_file.py` with these constants and signatures:

```python
from __future__ import annotations

import ctypes
from ctypes import wintypes
import os
from pathlib import Path
from typing import Callable, Final, cast


DELETE: Final = 0x00010000
GENERIC_READ: Final = 0x80000000
FILE_SHARE_READ: Final = 0x00000001
FILE_SHARE_WRITE: Final = 0x00000002
FILE_SHARE_DELETE: Final = 0x00000004
OPEN_EXISTING: Final = 3
FILE_ATTRIBUTE_NORMAL: Final = 0x00000080


class WindowsHandle:
    def __init__(self, value: int, close_handle: Callable[[int], int]) -> None:
        self._value = value
        self._close_handle = close_handle

    def close(self) -> None:
        if self._value:
            self._close_handle(self._value)
            self._value = 0

    def __enter__(self) -> WindowsHandle:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


def _open(path: Path, access: int, share: int) -> WindowsHandle:
    if os.name != "nt":
        raise RuntimeError("Windows file probing is only available on Windows")
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    create_file = kernel32.CreateFileW
    create_file.argtypes = (
        wintypes.LPCWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.LPVOID,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.HANDLE,
    )
    create_file.restype = wintypes.HANDLE
    close_handle = kernel32.CloseHandle
    close_handle.argtypes = (wintypes.HANDLE,)
    close_handle.restype = wintypes.BOOL
    value = create_file(
        str(path),
        access,
        share,
        None,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        None,
    )
    invalid = wintypes.HANDLE(-1).value
    if value == invalid:
        code = ctypes.get_last_error()
        raise OSError(code, ctypes.FormatError(code), str(path))
    return WindowsHandle(
        cast(int, value),
        cast(Callable[[int], int], close_handle),
    )


def probe_delete_access(path: Path) -> None:
    with _open(
        path,
        DELETE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    ):
        pass


def open_without_delete_sharing_for_tests(path: Path) -> WindowsHandle:
    return _open(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE)
```

- [ ] **Step 4: Run the probe contract**

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_runner.RunnerTests.test_delete_probe_reports_a_live_nonsharing_handle -v
```

Expected on Windows: PASS and the file content remains `b"partial"`. Expected elsewhere: SKIP.

- [ ] **Step 5: Commit the native probe**

```powershell
git add tools/focused_mutation_support/windows_file.py tests/test_focused_mutation_runner.py
git commit -m "feat: probe Windows log delete readiness"
```

---

### Task 3: Poll Log Readiness With a Fixed Deadline

**Files:**
- Modify: `tools/focused_mutation_support/runner.py:1-41`
- Modify: `tests/test_focused_mutation_runner.py:20-89`

**Interfaces:**
- Consumes: `probe: Callable[[Path], None]`, `monotonic: Callable[[], float]`, `sleep: Callable[[float], None]`, and command log paths.
- Produces: `wait_for_log_release(paths: Sequence[Path], *, probe: Callable[[Path], None], monotonic: Callable[[], float], sleep: Callable[[float], None], timeout: float = 2.0, poll_interval: float = 0.01) -> list[str]`.

- [ ] **Step 1: Add a deterministic fake clock and retry-success test**

Add:

```python
class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


def test_log_release_wait_retries_sharing_violation_until_success(self) -> None:
    clock = FakeClock()
    attempts = 0

    def probe(_: Path) -> None:
        nonlocal attempts
        attempts += 1
        if attempts < 3:
            error = OSError(32, "sharing violation")
            error.winerror = 32
            raise error

    errors = wait_for_log_release(
        [Path("stderr.log")],
        probe=probe,
        monotonic=clock.monotonic,
        sleep=clock.sleep,
        timeout=0.2,
        poll_interval=0.05,
    )

    self.assertEqual(errors, [])
    self.assertEqual(attempts, 3)
    self.assertEqual(clock.sleeps, [0.05, 0.05])
```

- [ ] **Step 2: Add deadline and non-sharing-error tests**

Add:

```python
def test_log_release_wait_reports_each_path_at_deadline(self) -> None:
    clock = FakeClock()

    def locked(_: Path) -> None:
        error = OSError(32, "sharing violation")
        error.winerror = 32
        raise error

    errors = wait_for_log_release(
        [Path("stdout.log"), Path("stderr.log")],
        probe=locked,
        monotonic=clock.monotonic,
        sleep=clock.sleep,
        timeout=0.1,
        poll_interval=0.05,
    )

    self.assertEqual(len(errors), 2)
    self.assertIn("stdout.log", errors[0])
    self.assertIn("stderr.log", errors[1])
    self.assertTrue(all("0.1 seconds" in error for error in errors))


def test_log_release_wait_reports_nonsharing_error_without_retry(self) -> None:
    clock = FakeClock()

    def denied(path: Path) -> None:
        raise OSError(5, "access denied", str(path))

    errors = wait_for_log_release(
        [Path("stderr.log")],
        probe=denied,
        monotonic=clock.monotonic,
        sleep=clock.sleep,
    )

    self.assertEqual(len(errors), 1)
    self.assertIn("access denied", errors[0].lower())
    self.assertEqual(clock.sleeps, [])
```

- [ ] **Step 3: Run the tests and verify the helper is missing**

Run:

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_runner.RunnerTests.test_log_release_wait_retries_sharing_violation_until_success `
  tests.test_focused_mutation_runner.RunnerTests.test_log_release_wait_reports_each_path_at_deadline `
  tests.test_focused_mutation_runner.RunnerTests.test_log_release_wait_reports_nonsharing_error_without_retry -v
```

Expected: ERROR because `wait_for_log_release` is not defined.

- [ ] **Step 4: Implement bounded polling**

Add `WINDOWS_LOG_RELEASE_TIMEOUT = 2.0`,
`WINDOWS_LOG_RELEASE_POLL_INTERVAL = 0.01`, and the exact public-for-tests
helper signature from this task. Implement one shared deadline for the entire
path set:

```python
def wait_for_log_release(
    paths: Sequence[Path],
    *,
    probe: Callable[[Path], None],
    monotonic: Callable[[], float],
    sleep: Callable[[float], None],
    timeout: float = WINDOWS_LOG_RELEASE_TIMEOUT,
    poll_interval: float = WINDOWS_LOG_RELEASE_POLL_INTERVAL,
) -> list[str]:
    deadline = monotonic() + timeout
    pending = list(paths)
    failures: list[str] = []
    while pending:
        retry: list[Path] = []
        for path in pending:
            try:
                probe(path)
            except OSError as error:
                if getattr(error, "winerror", None) == 32:
                    retry.append(path)
                else:
                    failures.append(f"{path}: {error}")
        if not retry:
            break
        remaining = deadline - monotonic()
        if remaining <= 0:
            failures.extend(
                f"{path}: log was not delete-ready within {timeout} seconds"
                for path in retry
            )
            break
        sleep(min(poll_interval, remaining))
        pending = retry
    return failures
```

Do not probe successful paths again. Do not sleep after a non-retryable error
or after the deadline.

- [ ] **Step 5: Run the deterministic polling tests**

Run the three-test command from Step 3.

Expected: PASS on every operating system because these tests inject the probe
and clock.

- [ ] **Step 6: Commit bounded readiness policy**

```powershell
git add tools/focused_mutation_support/runner.py tests/test_focused_mutation_runner.py
git commit -m "feat: bound Windows log release polling"
```

---

### Task 4: Deliver Runner Results Only After Parent Logs Close

**Files:**
- Modify: `tools/focused_mutation_support/runner.py:26-129`
- Modify: `tests/test_focused_mutation_runner.py:53-149`

**Interfaces:**
- Consumes: `wait_for_log_release(...)` from Task 3 and `probe_delete_access(path)` from Task 2.
- Produces: `CommandRunner(..., log_cleanup: Callable[[Sequence[Path]], list[str]] | None = None)` and post-close cleanup for normal, timeout, and interruption outcomes.

- [ ] **Step 1: Add a lifecycle-order test**

Extend the test `runner()` factory to accept `log_cleanup`. Add:

```python
def test_runner_checks_logs_only_after_parent_streams_close(self) -> None:
    streams: list[BinaryIO] = []

    def popen_factory(*args: object, **kwargs: object) -> subprocess.Popen[bytes]:
        streams.extend([kwargs["stdout"], kwargs["stderr"]])
        return subprocess.Popen(*args, **kwargs)

    def cleanup(_: Sequence[Path]) -> list[str]:
        self.assertTrue(all(stream.closed for stream in streams))
        return []

    record = self.runner(
        popen_factory=popen_factory,
        log_cleanup=cleanup,
    ).run(
        [sys.executable, str(self.fake)],
        cwd=self.work,
        timeout=5.0,
        label="complete",
    )

    self.assertEqual(record.cleanup_errors, [])
    self.assertEqual(len(streams), 2)
```

Import `BinaryIO` from `typing`. The captured stream assertion directly proves the callback runs after both `with`-managed streams have exited.

- [ ] **Step 2: Add timeout and interruption precedence tests**

Add injected cleanup failures to the existing timeout and interruption cases:

```python
cleanup_error = "stderr.log: log was not delete-ready within 2.0 seconds"
runner = self.runner(log_cleanup=lambda _: [cleanup_error])
with self.assertRaises(CommandTimedOut) as caught:
    runner.run(
        [sys.executable, str(self.fake), "--sleep"],
        cwd=self.work,
        timeout=0.5,
        label="mutation",
    )
self.assertEqual(str(caught.exception), "command timed out: mutation")
self.assertEqual(caught.exception.record.cleanup_errors, [cleanup_error])
```

For `InterruptingProcess`, inject the same callback and assert
`CommandInterrupted`, `"command interrupted: interrupted"`,
`record.cleanup_errors`, `process.wait_calls == [5.0, 2.0]`, and the existing
platform-specific terminate/killpg behavior.

- [ ] **Step 3: Run the focused tests and observe premature delivery**

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_runner -v
```

Expected: FAIL because `CommandRunner` does not accept `log_cleanup` and still
raises timeout/interruption exceptions before the stream context exits.

- [ ] **Step 4: Refactor `CommandRunner.run` around a deferred outcome**

Add constructor dependencies:

```python
sleep: Callable[[float], None] = time.sleep,
log_cleanup: Callable[[Sequence[Path]], list[str]] | None = None,
```

When `log_cleanup` is omitted, assign a method that returns `[]` off Windows
and calls `wait_for_log_release` with `probe_delete_access`,
`self._monotonic`, and `sleep` on Windows.

Inside `run`, record an outcome enum or exception class without raising it:

```python
outcome: type[Exception] | None = None
with (...):
    process = ...
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        record.timed_out = True
        self._terminate(process)
        outcome = CommandTimedOut
    except KeyboardInterrupt:
        record.interrupted = True
        self._terminate(process)
        outcome = CommandInterrupted
    self._complete(record, process, started)

record.cleanup_errors.extend(
    self._log_cleanup((paths.stdout, paths.stderr))
)
if outcome is CommandTimedOut:
    raise CommandTimedOut(record) from None
if outcome is CommandInterrupted:
    raise CommandInterrupted(record) from None
return record
```

Keep `_complete` inside the stream context so elapsed/exit metadata is captured
immediately after process cleanup; only log readiness and result delivery move
after stream closure.

- [ ] **Step 5: Run runner and reporting tests**

Run:

```powershell
uv run --frozen python -m unittest `
  tests.test_focused_mutation_runner `
  tests.test_focused_mutation_reporting -v
```

Expected: PASS. Existing partial-output, working-directory, exit-code,
termination, and interruption assertions remain unchanged.

- [ ] **Step 6: Commit lifecycle integration**

```powershell
git add tools/focused_mutation_support/runner.py tests/test_focused_mutation_runner.py
git commit -m "fix: await Windows timeout log release"
```

---

### Task 5: Add the Windows Inherited-Handle Lifecycle Regression

**Files:**
- Modify: `tests/test_focused_mutation_runner.py:20-153`

**Interfaces:**
- Consumes: Real `CommandRunner`, Windows process/file APIs, and temporary command logs.
- Produces: A Windows-only regression proving inherited log handles are released before immediate directory cleanup.

- [ ] **Step 1: Add a synchronized helper program**

Add a second generated script fixture, `INHERITED_HANDLE_FAKE`. Its root writes
its PID and a ready marker, launches a grandchild with inherited stdout/stderr,
and sleeps. The grandchild writes a second ready marker, waits for a release
marker, then exits:

```python
INHERITED_HANDLE_FAKE = r"""
import os
from pathlib import Path
import subprocess
import sys
import time

root_ready = Path(sys.argv[1])
descendant_ready = Path(sys.argv[2])
release = Path(sys.argv[3])
if "--descendant" in sys.argv:
    print("DESCENDANT-READY", flush=True)
    descendant_ready.write_text(str(os.getpid()), encoding="utf-8")
    while not release.exists():
        time.sleep(0.01)
    raise SystemExit(0)

subprocess.Popen(
    [
        sys.executable,
        __file__,
        str(root_ready),
        str(descendant_ready),
        str(release),
        "--descendant",
    ],
    stdin=subprocess.DEVNULL,
    stdout=sys.stdout,
    stderr=sys.stderr,
    close_fds=False,
)
print("ROOT-READY", flush=True)
root_ready.write_text(str(os.getpid()), encoding="utf-8")
time.sleep(30)
"""
```

In `setUp`, write this fixture beside `self.fake`.

- [ ] **Step 2: Add the Windows-only lifecycle test**

Use a background release thread. It waits for both ready markers, opens the
root PID with `SYNCHRONIZE`, waits for that process handle to signal, writes
the release marker, and closes its handle. Then run:

```python
@unittest.skipUnless(os.name == "nt", "requires Windows handle inheritance")
def test_timeout_waits_for_inherited_log_handles_before_cleanup(self) -> None:
    root_ready = self.work / "root.ready"
    descendant_ready = self.work / "descendant.ready"
    release = self.work / "release"
    releaser = threading.Thread(
        target=release_descendant_after_root_exit,
        args=(root_ready, descendant_ready, release),
        daemon=True,
    )
    releaser.start()

    with self.assertRaises(CommandTimedOut) as caught:
        self.runner().run(
            [
                sys.executable,
                str(self.inherited_handle_fake),
                str(root_ready),
                str(descendant_ready),
                str(release),
            ],
            cwd=self.work,
            timeout=1.0,
            label="inherited-handle",
        )

    releaser.join(timeout=5.0)
    self.assertFalse(releaser.is_alive())
    record = caught.exception.record
    self.assertEqual(record.cleanup_errors, [])
    self.assertIn("ROOT-READY", Path(record.stdout_path).read_text())
    self.assertIn("DESCENDANT-READY", Path(record.stdout_path).read_text())
    self.temporary.cleanup()
```

Implement `release_descendant_after_root_exit` with `ctypes` constants
`SYNCHRONIZE = 0x00100000` and `INFINITE = 0xFFFFFFFF`. Fail the test through
a thread-safe error list if marker waiting, `OpenProcess`, or
`WaitForSingleObject` fails. The helper must not use a fixed post-timeout sleep
to decide when the root has exited.

- [ ] **Step 3: Run the Windows lifecycle regression**

Run:

```powershell
uv run --frozen python -m unittest tests.test_focused_mutation_runner.RunnerTests.test_timeout_waits_for_inherited_log_handles_before_cleanup -v
```

Expected on Windows with the implementation: PASS. Before Task 4's lifecycle
change, the test deterministically reaches `TemporaryDirectory.cleanup` while
the inherited handle is live and fails with `WinError 32`. Expected off
Windows: SKIP.

- [ ] **Step 4: Stress the regression without adding retries**

Run:

```powershell
1..20 | ForEach-Object {
  uv run --frozen python -m unittest `
    tests.test_focused_mutation_runner.RunnerTests.test_timeout_waits_for_inherited_log_handles_before_cleanup
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
```

Expected on Windows: 20 passes. This repeated local execution is evidence
collection only; do not encode repetition or retry behavior into CI.

- [ ] **Step 5: Commit the real lifecycle regression**

```powershell
git add tests/test_focused_mutation_runner.py
git commit -m "test: cover inherited Windows timeout logs"
```

---

### Task 6: Verify the Complete Python Change

**Files:**
- Verify: `tools/focused_mutation_support/model.py`
- Verify: `tools/focused_mutation_support/runner.py`
- Verify: `tools/focused_mutation_support/windows_file.py`
- Verify: `tests/test_focused_mutation_runner.py`
- Verify: `tests/test_focused_mutation_reporting.py`
- Verify: `docs/superpowers/specs/2026-07-28-windows-timeout-log-release-design.md`
- Verify: `docs/superpowers/plans/2026-07-28-windows-timeout-log-release.md`

**Interfaces:**
- Consumes: All preceding task outputs.
- Produces: Reviewable Issue #47 branch with focused and full-suite evidence.

- [ ] **Step 1: Run syntax compilation and focused tests**

```powershell
uv run --frozen python -m compileall -q tools tests
uv run --frozen python -m unittest `
  tests.test_focused_mutation_runner `
  tests.test_focused_mutation_reporting -v
```

Expected: both commands exit 0; Windows-only contracts pass on Windows.

- [ ] **Step 2: Run the full Python suite**

```powershell
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: exit 0 with no intermittent `TemporaryDirectory` cleanup failure.

- [ ] **Step 3: Run static type checks**

```powershell
uv run --frozen mypy tools tests
uv run --frozen ty check tools tests
```

Expected: both commands exit 0. Resolve ctypes callable and handle-value types
with precise annotations or narrow casts; do not add a blanket ignore.

- [ ] **Step 4: Run repository-wide quality gates**

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run --frozen maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

Expected: every command exits 0. Although this change is Python-only, these
are the repository's documented pre-PR gates.

- [ ] **Step 5: Inspect the final diff and working tree**

```powershell
git diff origin/main...HEAD --check
git diff origin/main...HEAD --stat
git status --short
```

Expected: the diff contains the design, plan, Python implementation, and tests.
Only the user's pre-existing `.idea/` remains untracked.

- [ ] **Step 6: Commit any verification-only correction**

If verification required a source or test correction, stage only the named
Issue #47 files and commit:

```powershell
git add `
  tools/focused_mutation_support/model.py `
  tools/focused_mutation_support/runner.py `
  tools/focused_mutation_support/windows_file.py `
  tests/test_focused_mutation_runner.py `
  tests/test_focused_mutation_reporting.py
git commit -m "test: finalize Windows timeout cleanup coverage"
```

If no files changed during verification, do not create an empty commit.

- [ ] **Step 7: Prepare the PR**

Use title:

```text
test: make focused mutation timeout cleanup resilient on Windows
```

The PR body must link `Closes #47` and summarize:

- post-close delete-readiness probing is Windows-only and bounded;
- primary timeout/interruption outcomes and partial logs are preserved;
- cleanup failures are persisted in `CommandRecord.cleanup_errors`;
- deterministic unit tests and the Windows inherited-handle lifecycle
  regression cover the previous race;
- no CI retry or `PermissionError` suppression was added.

Record the exact focused, full Python, type-check, Cargo, wheel, and Windows
stress-test results in the PR body.
