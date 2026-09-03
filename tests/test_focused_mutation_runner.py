from collections.abc import Callable, Mapping, Sequence
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock
from typing import Any, BinaryIO
from typing import cast

from tools.focused_mutation_support.model import CommandRecord
from tools.focused_mutation_support.disk import (
    DiskFailure,
    DiskStopReason,
)
from tools.focused_mutation_support.filesystem import SharePolicy
from tools.focused_mutation_support.lease import ManagedScratch, ScratchCleanupStatus
from tools.focused_mutation_support.runner import (
    CommandDrainFailed,
    CommandDiskStopped,
    CommandInterrupted,
    CommandLifecycleFailed,
    CommandRunner,
    CommandTimedOut,
    ProcessLifecycleError,
    terminate_windows_process_tree,
    wait_for_log_release,
)
from tools.focused_mutation_support.store import CommandPaths, RunStore
from tools.focused_mutation_support.windows_file import (
    WindowsHandle,
    probe_delete_access,
)


FAKE = f"""#!{sys.executable}
import os, sys, time
print("OUT:" + "|".join(sys.argv[1:]), flush=True)
print("ERR:" + os.getcwd(), file=sys.stderr, flush=True)
if "--sleep" in sys.argv:
    time.sleep(30)
raise SystemExit(int(os.environ.get("FAKE_EXIT", "0")))
"""


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


TERM_RESISTANT_DESCENDANT_FAKE = r"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

root_ready = Path(sys.argv[1])
descendant_ready = Path(sys.argv[2])
term_received = Path(sys.argv[3])
if "--descendant" in sys.argv:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    descendant_ready.write_text(str(os.getpid()), encoding="utf-8")
    while True:
        time.sleep(0.01)

subprocess.Popen(
    [
        sys.executable,
        __file__,
        str(root_ready),
        str(descendant_ready),
        str(term_received),
        "--descendant",
    ],
    stdin=subprocess.DEVNULL,
    stdout=sys.stdout,
    stderr=sys.stderr,
)
root_ready.write_text(str(os.getpid()), encoding="utf-8")
def handle_term(_signum, _frame):
    term_received.write_text("received", encoding="utf-8")
    raise SystemExit(0)

signal.signal(signal.SIGTERM, handle_term)
while True:
    time.sleep(0.01)
"""


NORMAL_EXIT_WITH_DESCENDANT_FAKE = r"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

descendant_ready = Path(sys.argv[1])
if "--descendant" in sys.argv:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    descendant_ready.write_text(str(os.getpid()), encoding="utf-8")
    print("DESCENDANT-READY", flush=True)
    while True:
        time.sleep(0.01)

subprocess.Popen(
    [sys.executable, __file__, str(descendant_ready), "--descendant"],
    stdin=subprocess.DEVNULL,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
deadline = time.monotonic() + 2.0
while not descendant_ready.exists():
    if time.monotonic() >= deadline:
        raise RuntimeError("descendant did not start")
    time.sleep(0.01)
print("ROOT-EXIT", flush=True)
"""


LOG_CLEANUP_CALLBACK_ERROR = (
    "log cleanup callback failed: RuntimeError: cleanup callback exploded"
)


def raising_log_cleanup(_: Sequence[Path]) -> list[str]:
    raise RuntimeError("cleanup callback exploded")


def posix_group_absent_after_signal(_pid: int, sent_signal: int) -> None:
    if sent_signal == 0:
        raise ProcessLookupError


SYNCHRONIZE = 0x00100000
PROCESS_TERMINATE = 0x0001
WAIT_OBJECT_0 = 0x00000000
WAIT_TIMEOUT = 0x00000102
WAIT_FAILED = 0xFFFFFFFF
PROCESS_EXIT_WAIT_TIMEOUT = 5.0
PROCESS_WAIT_SLICE_MS = 100
EVENT_WAIT_SLICE_SECONDS = 0.05


class WindowsProcessHandle:
    def __init__(
        self,
        value: int,
        wait_for_single_object: Any,
        terminate_process: Any,
        close_handle: Any,
    ) -> None:
        self._value = value
        self._wait_for_single_object = wait_for_single_object
        self._terminate_process = terminate_process
        self._close_handle = close_handle

    @classmethod
    def open(cls, pid: int, access: int) -> WindowsProcessHandle:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        open_process = kernel32.OpenProcess
        open_process.argtypes = (
            wintypes.DWORD,
            wintypes.BOOL,
            wintypes.DWORD,
        )
        open_process.restype = wintypes.HANDLE
        wait_for_single_object = kernel32.WaitForSingleObject
        wait_for_single_object.argtypes = (wintypes.HANDLE, wintypes.DWORD)
        wait_for_single_object.restype = wintypes.DWORD
        terminate_process = kernel32.TerminateProcess
        terminate_process.argtypes = (wintypes.HANDLE, wintypes.UINT)
        terminate_process.restype = wintypes.BOOL
        close_handle = kernel32.CloseHandle
        close_handle.argtypes = (wintypes.HANDLE,)
        close_handle.restype = wintypes.BOOL

        value = open_process(access, False, pid)
        if not value:
            raise ctypes.WinError(ctypes.get_last_error())
        return cls(
            value,
            wait_for_single_object,
            terminate_process,
            close_handle,
        )

    def wait(self, timeout_ms: int) -> int:
        wait_result = self._wait_for_single_object(self._value, timeout_ms)
        if wait_result == WAIT_FAILED:
            raise ctypes.WinError(ctypes.get_last_error())
        if wait_result not in (WAIT_OBJECT_0, WAIT_TIMEOUT):
            raise OSError(
                f"WaitForSingleObject returned unexpected result "
                f"{wait_result:#x}"
            )
        return wait_result

    def stop(self, *, grace_ms: int) -> None:
        if self.wait(grace_ms) == WAIT_OBJECT_0:
            return
        if not self._terminate_process(self._value, 1):
            code = ctypes.get_last_error()
            if self.wait(0) == WAIT_OBJECT_0:
                return
            raise ctypes.WinError(code)
        if self.wait(5_000) != WAIT_OBJECT_0:
            raise TimeoutError("terminated process did not signal")

    def close(self) -> None:
        if not self._value:
            return
        if not self._close_handle(self._value):
            raise ctypes.WinError(ctypes.get_last_error())
        self._value = 0


class NeverSignaledWindowsProcessHandle:
    def __init__(self, cancel: threading.Event) -> None:
        self._cancel = cancel
        self.wait_started = threading.Event()
        self.wait_calls: list[int] = []
        self.stop_calls: list[int] = []
        self.closed = False

    def wait(self, timeout_ms: int) -> int:
        self.wait_calls.append(timeout_ms)
        self.wait_started.set()
        self._cancel.wait(timeout=min(timeout_ms / 1_000, 0.05))
        return WAIT_TIMEOUT

    def stop(self, *, grace_ms: int) -> None:
        self.stop_calls.append(grace_ms)

    def close(self) -> None:
        self.closed = True


def read_ready_pid(marker: Path) -> int | None:
    try:
        return int(marker.read_text(encoding="utf-8"))
    except (FileNotFoundError, ValueError):
        return None


def wait_for_pid_exit(pid: int, timeout: float = 2.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        time.sleep(0.01)
    return False


def wait_for_pid_exit_or_zombie(pid: int, timeout: float = 2.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            state = subprocess.run(
                ["ps", "-o", "stat=", "-p", str(pid)],
                check=False,
                capture_output=True,
                text=True,
            ).stdout.strip()
        except OSError:
            return wait_for_pid_exit(
                pid,
                timeout=max(0.0, deadline - time.monotonic()),
            )
        if not state or state.startswith("Z"):
            return True
        time.sleep(0.01)
    return False


def wait_for_process_exit_or_cancel(
    handle: WindowsProcessHandle,
    cancel: threading.Event,
    *,
    timeout: float = PROCESS_EXIT_WAIT_TIMEOUT,
) -> bool:
    deadline = time.monotonic() + timeout
    while not cancel.is_set():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("root process did not signal")
        timeout_ms = max(
            1,
            min(PROCESS_WAIT_SLICE_MS, int(remaining * 1_000)),
        )
        if handle.wait(timeout_ms) == WAIT_OBJECT_0:
            return True
    return False


def wait_for_event_or_cancel(
    event: threading.Event,
    cancel: threading.Event,
    *,
    timeout: float,
) -> bool:
    deadline = time.monotonic() + timeout
    while not cancel.is_set():
        if event.is_set():
            return True
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return False
        cancel.wait(timeout=min(EVENT_WAIT_SLICE_SECONDS, remaining))
    return False


def release_descendant_after_root_exit(
    root_ready: Path,
    descendant_ready: Path,
    release: Path,
    cleanup_wait_started: threading.Event,
    cancel: threading.Event,
    captured_handles: dict[str, WindowsProcessHandle],
    capture_done: threading.Event,
    errors: list[str],
    errors_lock: threading.Lock,
) -> None:
    root_wait_handle: WindowsProcessHandle | None = None

    def record_error(context: str, error: BaseException) -> None:
        with errors_lock:
            errors.append(f"{context}: {type(error).__name__}: {error}")

    try:
        pending = {
            "root": root_ready,
            "descendant": descendant_ready,
        }
        deadline = time.monotonic() + 5.0
        while pending and not cancel.is_set():
            for name, marker in tuple(pending.items()):
                try:
                    pid = read_ready_pid(marker)
                except BaseException as error:
                    record_error(f"{name} marker", error)
                    del pending[name]
                    continue
                if pid is None:
                    continue

                cleanup_handle: WindowsProcessHandle | None = None
                wait_handle: WindowsProcessHandle | None = None
                try:
                    cleanup_handle = WindowsProcessHandle.open(
                        pid,
                        SYNCHRONIZE | PROCESS_TERMINATE,
                    )
                    if name == "root":
                        wait_handle = WindowsProcessHandle.open(
                            pid,
                            SYNCHRONIZE,
                        )
                except BaseException as error:
                    record_error(f"{name} OpenProcess", error)
                    for opened_handle in (wait_handle, cleanup_handle):
                        if opened_handle is None:
                            continue
                        try:
                            opened_handle.close()
                        except BaseException as close_error:
                            record_error(f"{name} CloseHandle", close_error)
                    del pending[name]
                    continue

                with errors_lock:
                    captured_handles[name] = cleanup_handle
                if name == "root":
                    root_wait_handle = wait_handle
                del pending[name]

            if not pending:
                break
            if time.monotonic() >= deadline:
                for name in pending:
                    record_error(
                        f"{name} marker",
                        TimeoutError("process marker was not ready"),
                    )
                break
            cancel.wait(timeout=0.01)
    except BaseException as error:
        record_error("process capture", error)
    finally:
        capture_done.set()

    try:
        if root_wait_handle is None or cancel.is_set():
            return
        if not wait_for_process_exit_or_cancel(root_wait_handle, cancel):
            return
        if not wait_for_event_or_cancel(
            cleanup_wait_started,
            cancel,
            timeout=5.0,
        ):
            if cancel.is_set():
                return
            raise TimeoutError("log cleanup did not observe the inherited lock")
        release.write_text("release", encoding="utf-8")
    except BaseException as error:
        record_error("descendant releaser", error)
    finally:
        if root_wait_handle is not None:
            try:
                root_wait_handle.close()
            except BaseException as error:
                record_error("root CloseHandle", error)


def cleanup_inherited_handle_fixture(
    release: Path,
    releaser: threading.Thread,
    cancel: threading.Event,
    captured_handles: dict[str, WindowsProcessHandle],
    capture_done: threading.Event,
    directory_cleanup_succeeded: threading.Event,
    thread_errors: list[str],
    errors_lock: threading.Lock,
) -> None:
    cleanup_errors: list[str] = []

    def record_cleanup_error(context: str, error: BaseException) -> None:
        cleanup_errors.append(f"{context}: {type(error).__name__}: {error}")

    if not capture_done.wait(timeout=5.5):
        record_cleanup_error(
            "process capture",
            TimeoutError("process-handle capture did not finish"),
        )
    cancel.set()

    if not directory_cleanup_succeeded.is_set():
        try:
            release.write_text("release", encoding="utf-8")
        except FileNotFoundError:
            pass
        except BaseException as error:
            record_cleanup_error("release cleanup", error)

    releaser.join(timeout=5.0)
    if releaser.is_alive():
        record_cleanup_error(
            "releaser cleanup",
            RuntimeError("releaser thread did not exit"),
        )

    with errors_lock:
        handles = dict(captured_handles)

    if not directory_cleanup_succeeded.is_set():
        for name, grace_ms in (("root", 0), ("descendant", 1_000)):
            handle = handles.get(name)
            if handle is None:
                continue
            try:
                handle.stop(grace_ms=grace_ms)
            except BaseException as error:
                record_cleanup_error(f"{name} cleanup", error)

    command_logs = release.parent.parent / "output" / "commands"
    if command_logs.exists():
        cleanup_errors.extend(
            wait_for_log_release(
                tuple(command_logs.glob("*.std*")),
                probe=probe_delete_access,
                monotonic=time.monotonic,
                sleep=time.sleep,
            )
        )

    for name, handle in handles.items():
        try:
            handle.close()
        except BaseException as error:
            record_cleanup_error(f"{name} CloseHandle", error)

    with errors_lock:
        cleanup_errors.extend(thread_errors)
    if cleanup_errors:
        raise AssertionError("\n".join(cleanup_errors))


class InterruptingProcess:
    pid = 12345
    returncode: int | None = None

    def __init__(self) -> None:
        self.wait_calls: list[float | None] = []
        self.terminated = False

    def wait(self, timeout: float | None = None) -> int:
        self.wait_calls.append(timeout)
        if len(self.wait_calls) == 1:
            raise KeyboardInterrupt
        self.returncode = -15
        return self.returncode

    def terminate(self) -> None:
        self.terminated = True

    def kill(self) -> None:
        raise AssertionError("kill should not be needed after successful termination")


class UnreapableProcess:
    pid = 12345
    returncode: int | None = None

    def __init__(self, initial_error: BaseException) -> None:
        self._initial_error = initial_error
        self.wait_calls: list[float | None] = []
        self.terminate_calls = 0
        self.kill_calls = 0

    def wait(self, timeout: float | None = None) -> int:
        self.wait_calls.append(timeout)
        if len(self.wait_calls) == 1:
            raise self._initial_error
        raise subprocess.TimeoutExpired(
            ["fake-command"],
            timeout if timeout is not None else 0.0,
        )

    def terminate(self) -> None:
        self.terminate_calls += 1

    def kill(self) -> None:
        self.kill_calls += 1


class TreeKillFallbackProcess:
    pid = 12345
    returncode: int | None = None

    def __init__(self) -> None:
        self.wait_calls: list[float | None] = []
        self.kill_calls = 0

    def wait(self, timeout: float | None = None) -> int:
        self.wait_calls.append(timeout)
        if len(self.wait_calls) == 1:
            raise KeyboardInterrupt
        self.returncode = -9
        return self.returncode

    def terminate(self) -> None:
        raise AssertionError("root terminate must not replace tree termination")

    def kill(self) -> None:
        self.kill_calls += 1


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


class _Task10RunnerCloseStream:
    def __init__(
        self,
        stream: BinaryIO,
        failures: int,
        events: list[str] | None = None,
    ) -> None:
        self.stream = stream
        self.failures = failures
        self.events = events
        self.close_calls = 0

    @property
    def closed(self) -> bool:
        return self.stream.closed

    def read(self, size: int = -1) -> bytes:
        return self.stream.read(size)

    def write(self, value: bytes) -> int:
        return self.stream.write(value)

    def flush(self) -> None:
        self.stream.flush()

    def fileno(self) -> int:
        return self.stream.fileno()

    def close(self) -> None:
        self.close_calls += 1
        if self.events is not None:
            self.events.append("stream-close")
        if self.failures:
            self.failures -= 1
            raise OSError("injected runner stream close failure")
        self.stream.close()

    def __enter__(self) -> "_Task10RunnerCloseStream":
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()


class _Task10BoundaryGuard:
    def __init__(self, stage: str) -> None:
        self.stage = stage
        self.failure = None
        self.sample_calls = 0

    def sample(self) -> None:
        self.sample_calls += 1
        if (
            (self.stage == "preflight" and self.sample_calls == 1)
            or (self.stage == "post-drain" and self.sample_calls == 2)
            or (self.stage == "post-write" and self.sample_calls == 3)
        ):
            raise OSError(f"injected {self.stage} sample failure")

    def reserve_additional_bytes(self, _value: int, **_kwargs: object) -> None:
        if self.stage == "reserve":
            raise OSError("injected reserve failure")


class RunnerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        self.work = root / "work"
        self.work.mkdir()
        self.output = root / "output"
        self.output.mkdir()
        self.store = RunStore(self.output)
        self.store.commands.mkdir()
        self.fake = root / "fake-command"
        self.fake.write_text(FAKE)
        self.fake.chmod(0o755)
        self.inherited_handle_fake = root / "inherited-handle-command.py"
        self.inherited_handle_fake.write_text(
            INHERITED_HANDLE_FAKE,
            encoding="utf-8",
        )
        self.term_resistant_descendant_fake = (
            root / "term-resistant-descendant-command.py"
        )
        self.term_resistant_descendant_fake.write_text(
            TERM_RESISTANT_DESCENDANT_FAKE,
            encoding="utf-8",
        )
        self.normal_exit_with_descendant_fake = (
            root / "normal-exit-with-descendant-command.py"
        )
        self.normal_exit_with_descendant_fake.write_text(
            NORMAL_EXIT_WITH_DESCENDANT_FAKE,
            encoding="utf-8",
        )

    def runner(
        self,
        *,
        extra_env: dict[str, str] | None = None,
        popen_factory: Callable[..., Any] = subprocess.Popen,
        log_cleanup: Callable[[Sequence[Path]], list[str]] | None = None,
        windows_tree_terminator: Callable[[int], None] | None = None,
    ) -> CommandRunner:
        keyword_arguments: dict[str, object] = {
            "extra_env": extra_env,
            "popen_factory": popen_factory,
            "log_cleanup": log_cleanup,
            "utc_now": lambda: datetime(2026, 7, 26, tzinfo=timezone.utc),
        }
        if windows_tree_terminator is not None:
            keyword_arguments["windows_tree_terminator"] = windows_tree_terminator
        return CommandRunner(
            self.store,
            **keyword_arguments,
        )

    def task10_store(self) -> tuple[RunStore, ManagedScratch]:
        root = Path(self.temporary.name)
        scratch_parent = root / "scratch"
        scratch_parent.mkdir(exist_ok=True)
        scratch = ManagedScratch.create(scratch_parent)
        command_root = scratch.create_child("commands")
        capability = scratch.open_child("commands", SharePolicy.MUTATION)
        try:
            store = RunStore(
                self.output,
                command_root=command_root,
                command_root_capability=capability,
            )
        except TypeError as error:
            capability.close()
            scratch.mark_cleanup_ready()
            scratch.cleanup()
            scratch.close_capabilities()
            self.fail(f"capability-backed RunStore API is missing: {error}")
        return store, scratch

    def close_task10_store(self, store: RunStore, scratch: ManagedScratch) -> None:
        errors = store.close_command_root()
        self.assertEqual(errors, ())
        scratch.mark_cleanup_ready()
        cleanup = scratch.cleanup()
        self.assertIs(cleanup.status, ScratchCleanupStatus.CLEAN)
        self.assertEqual(scratch.close_capabilities(), ())

    def stdout(self, record: CommandRecord) -> str:
        return Path(record.stdout_path).read_text()

    def test_windows_tree_terminator_uses_taskkill(self) -> None:
        run = mock.Mock(
            return_value=subprocess.CompletedProcess(["taskkill"], 0)
        )

        with mock.patch.dict(
            os.environ, {"SystemRoot": r"C:\Windows"}
        ):
            terminate_windows_process_tree(12345, run=run)

        run.assert_called_once_with(
            [
                r"C:\Windows\System32\taskkill.exe",
                "/PID",
                "12345",
                "/T",
                "/F",
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2.0,
            check=False,
        )

    def test_command_spool_refuses_precreated_stdout_symlink(self) -> None:
        root = Path(self.temporary.name)
        store, scratch = self.task10_store()
        commands = store.commands
        sentinel = root / "sentinel"
        sentinel.write_text("outside", encoding="utf-8")
        runner = CommandRunner(store)
        precreated = commands / "0001-swap.stdout"
        try:
            os.symlink(sentinel, precreated)
        except OSError:
            os.link(sentinel, precreated)

        try:
            with self.assertRaises(OSError):
                runner.run(
                    [sys.executable, "-c", "print('captured')"],
                    cwd=root,
                    timeout=5.0,
                    label="swap",
                )

            self.assertEqual(sentinel.read_text(encoding="utf-8"), "outside")
        finally:
            self.close_task10_store(store, scratch)

    def test_command_spool_paths_are_validated_before_writer_creation(self) -> None:
        store_module = __import__(
            "tools.focused_mutation_support.store",
            fromlist=["validate_reported_path"],
        )
        real_validate = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["validate_reported_path"],
        ).validate_reported_path

        def reject_stdout(path: Path) -> str:
            if path.name.endswith(".stdout"):
                raise ValueError("injected stdout report path rejection")
            return real_validate(path)

        with (
            mock.patch.object(
                store_module,
                "validate_reported_path",
                side_effect=reject_stdout,
                create=True,
            ),
            self.assertRaisesRegex(ValueError, "stdout report path"),
        ):
            self.store.command_paths(1, "validated")

        self.assertEqual(list((self.output / "commands").iterdir()), [])

    def test_command_spool_stays_on_pinned_directory_after_path_swap(self) -> None:
        root = Path(self.temporary.name)
        store, scratch = self.task10_store()
        commands = store.commands
        moved = commands.with_name("commands-moved")
        sentinel = root / "sentinel"
        sentinel.write_text("outside", encoding="utf-8")
        runner = CommandRunner(store)
        script = (
            "import os, pathlib, sys\n"
            "commands = pathlib.Path(sys.argv[1])\n"
            "moved = pathlib.Path(sys.argv[2])\n"
            "commands.rename(moved)\n"
            "commands.mkdir()\n"
            "os.link(sys.argv[3], commands / '0001-directory-swap.stdout')\n"
            "print('captured')\n"
        )

        try:
            record = runner.run(
                [
                    sys.executable,
                    "-c",
                    script,
                    str(commands),
                    str(moved),
                    str(sentinel),
                ],
                cwd=root,
                timeout=5.0,
                label="directory-swap",
            )

            self.assertEqual(record.exit_code, 0)
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "outside")
            self.assertEqual(
                (moved / "0001-directory-swap.stdout").read_text(), "captured\n"
            )
            spool = record._spool
            self.assertIsInstance(spool, CommandPaths)
            assert isinstance(spool, CommandPaths)
            self.assertEqual(spool.discard(), ())
            for child in commands.iterdir():
                child.unlink()
            commands.rmdir()
            moved.rename(commands)
        finally:
            self.close_task10_store(store, scratch)

    def test_second_spool_failure_removes_first_and_blocks_cleanup(self) -> None:
        root = Path(self.temporary.name)
        store, scratch = self.task10_store()
        commands = store.commands
        runner = CommandRunner(store)
        real_open_writer = __import__(
            "tools.focused_mutation_support.store",
            fromlist=["CommandPaths"],
        ).CommandPaths.open_writer

        def fail_stderr(paths: object, stream_name: str) -> object:
            if stream_name == "stderr":
                raise OSError("injected stderr spool open failure")
            return real_open_writer(paths, stream_name)

        try:
            with (
                mock.patch(
                    "tools.focused_mutation_support.store.CommandPaths.open_writer",
                    autospec=True,
                    side_effect=fail_stderr,
                ),
                self.assertRaisesRegex(OSError, "stderr spool"),
            ):
                runner.run(
                    [sys.executable, "-c", "print('captured')"],
                    cwd=root,
                    timeout=5.0,
                    label="two-spools",
                )

            self.assertFalse((commands / "0001-two-spools.stdout").exists())
            self.assertFalse(runner.output_drain_safe)
        finally:
            self.close_task10_store(store, scratch)

    def test_windows_tree_terminator_rejects_nonzero_exit(self) -> None:
        run = mock.Mock(
            return_value=subprocess.CompletedProcess(["taskkill"], 1)
        )

        with self.assertRaisesRegex(
            OSError, "taskkill exited with status 1"
        ), mock.patch.dict(
            os.environ, {"SystemRoot": r"C:\Windows"}
        ):
            terminate_windows_process_tree(12345, run=run)

    def test_windows_tree_terminator_requires_system_root(self) -> None:
        run = mock.Mock()

        with (
            mock.patch.dict(os.environ, {}, clear=True),
            self.assertRaisesRegex(OSError, "SystemRoot is not set"),
        ):
            terminate_windows_process_tree(12345, run=run)

        run.assert_not_called()

    def test_windows_tree_terminator_requires_keyword_runner(self) -> None:
        with self.assertRaises(TypeError):
            terminate_windows_process_tree(12345, mock.Mock())  # type: ignore

    def test_windows_interruption_terminates_the_process_tree(self) -> None:
        process = InterruptingProcess()
        tree_terminator = mock.Mock()
        runner = self.runner(
            popen_factory=mock.Mock(return_value=process),
            log_cleanup=lambda _: [],
            windows_tree_terminator=tree_terminator,
        )

        with (
            mock.patch(
                "tools.focused_mutation_support.runner.os.name", "nt"
            ),
            self.assertRaises(CommandInterrupted),
        ):
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="interrupted-tree",
            )

        tree_terminator.assert_called_once_with(process.pid)
        self.assertFalse(process.terminated)

    def test_windows_tree_failure_blocks_reuse_after_root_cleanup(self) -> None:
        process = TreeKillFallbackProcess()
        popen_factory = mock.Mock(return_value=process)
        runner = self.runner(
            popen_factory=popen_factory,
            log_cleanup=lambda _: [],
            windows_tree_terminator=mock.Mock(
                side_effect=OSError("taskkill unavailable")
            ),
        )

        with (
            mock.patch(
                "tools.focused_mutation_support.runner.os.name", "nt"
            ),
            self.assertRaises(CommandInterrupted) as caught,
        ):
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="tree-failure",
            )

        self.assertEqual(process.kill_calls, 1)
        self.assertEqual(
            caught.exception.record.cleanup_errors,
            [
                "process lifecycle cleanup failed: process tree 12345 "
                "termination failed: OSError: taskkill unavailable"
            ],
        )
        with self.assertRaises(ProcessLifecycleError):
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="must-not-start",
            )
        self.assertEqual(popen_factory.call_count, 1)

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

    @unittest.skipUnless(os.name == "nt", "requires Windows last-error state")
    def test_windows_handle_close_failure_preserves_ownership_and_error(
        self,
    ) -> None:
        close_calls: list[int] = []

        def close_handle(value: int) -> int:
            close_calls.append(value)
            if len(close_calls) == 1:
                ctypes.set_last_error(6)
                return 0
            return 1

        handle = WindowsHandle(123, close_handle)

        with self.assertRaises(OSError) as caught:
            handle.close()

        self.assertEqual(caught.exception.winerror, 6)
        handle.close()
        handle.close()
        self.assertEqual(close_calls, [123, 123])

    @unittest.skipUnless(os.name == "nt", "requires Windows last-error state")
    def test_log_release_wait_reports_close_failure_once_for_its_path(
        self,
    ) -> None:
        clock = FakeClock()
        attempts = 0

        def probe(_: Path) -> None:
            nonlocal attempts
            attempts += 1

            def fail_close(_: int) -> int:
                ctypes.set_last_error(6)
                return 0

            with WindowsHandle(123, fail_close):
                pass

        errors = wait_for_log_release(
            [Path("stderr.log")],
            probe=probe,
            monotonic=clock.monotonic,
            sleep=clock.sleep,
        )

        self.assertEqual(attempts, 1)
        self.assertEqual(len(errors), 1)
        self.assertIn("stderr.log", errors[0])
        self.assertIn("WinError 6", errors[0])
        self.assertEqual(clock.sleeps, [])

    @unittest.skipUnless(os.name == "nt", "requires Windows file sharing")
    def test_delete_probe_reports_a_live_nonsharing_handle(self) -> None:
        from tools.focused_mutation_support.windows_file import (
            open_without_delete_sharing_for_tests,
            probe_delete_access,
        )

        path = self.output / "locked.log"
        path.write_bytes(b"partial")
        handle = open_without_delete_sharing_for_tests(path)
        self.addCleanup(handle.close)

        with self.assertRaises(OSError) as caught:
            probe_delete_access(path)

        self.assertEqual(caught.exception.winerror, 32)
        self.assertEqual(path.read_bytes(), b"partial")

    def assert_recorded_working_directory(self, record: CommandRecord) -> None:
        stderr = Path(record.stderr_path).read_text()
        self.assertTrue(stderr.startswith("ERR:"), stderr)
        recorded = Path(stderr.removeprefix("ERR:").strip())
        self.assertTrue(recorded.samefile(self.work), (recorded, self.work))

    def test_records_native_arguments_output_and_nonzero_exit(self) -> None:
        record = self.runner(extra_env={"FAKE_EXIT": "7"}).run(
            [sys.executable, str(self.fake), "a b", "$(never-run)"],
            cwd=self.work,
            timeout=5.0,
            label="baseline",
        )
        self.assertEqual(record.argv[-2:], ["a b", "$(never-run)"])
        self.assertEqual(record.exit_code, 7)
        self.assertEqual(record.started_at, "2026-07-26T00:00:00+00:00")
        self.assertEqual(record.ended_at, "2026-07-26T00:00:00+00:00")
        self.assertIn("OUT:a b|$(never-run)", self.stdout(record))
        self.assert_recorded_working_directory(record)

    def test_runner_checks_logs_only_after_parent_streams_close(self) -> None:
        streams: list[BinaryIO] = []

        def popen_factory(
            argv: Sequence[str],
            *,
            cwd: Path,
            stdin: int,
            stdout: BinaryIO,
            stderr: BinaryIO,
            env: Mapping[str, str],
            shell: bool,
            start_new_session: bool,
        ) -> subprocess.Popen[bytes]:
            streams.extend([stdout, stderr])
            return subprocess.Popen(
                argv,
                cwd=cwd,
                stdin=stdin,
                stdout=stdout,
                stderr=stderr,
                env=env,
                shell=shell,
                start_new_session=start_new_session,
            )

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

    def test_normal_completion_preserves_exit_code_when_cleanup_callback_raises(
        self,
    ) -> None:
        result: CommandRecord | BaseException
        try:
            result = self.runner(
                extra_env={"FAKE_EXIT": "7"},
                log_cleanup=raising_log_cleanup,
            ).run(
                [sys.executable, str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="complete-with-cleanup-error",
            )
        except BaseException as error:
            result = error

        self.assertIsInstance(result, CommandRecord)
        if not isinstance(result, CommandRecord):
            return
        self.assertEqual(result.exit_code, 7)
        self.assertEqual(
            result.cleanup_errors,
            [LOG_CLEANUP_CALLBACK_ERROR],
        )

    def test_timeout_remains_primary_when_cleanup_callback_raises(self) -> None:
        runner = self.runner(log_cleanup=raising_log_cleanup)
        result: CommandRecord | BaseException
        with mock.patch.object(
            runner,
            "_default_log_cleanup",
            wraps=runner._default_log_cleanup,
        ) as fallback_cleanup:
            try:
                result = runner.run(
                    [sys.executable, str(self.fake), "--sleep"],
                    cwd=self.work,
                    timeout=0.5,
                    label="timeout-with-cleanup-error",
                )
            except BaseException as error:
                result = error

        self.assertIsInstance(result, CommandTimedOut)
        if not isinstance(result, CommandTimedOut):
            return
        fallback_cleanup.assert_called_once_with(
            (
                Path(result.record.stdout_path),
                Path(result.record.stderr_path),
            )
        )
        self.assertEqual(
            str(result),
            "command timed out: timeout-with-cleanup-error",
        )
        self.assertEqual(
            result.record.cleanup_errors,
            [LOG_CLEANUP_CALLBACK_ERROR],
        )

    def test_timeout_terminates_process_and_keeps_partial_logs(self) -> None:
        cleanup_error = (
            "stderr.log: log was not delete-ready within 2.0 seconds"
        )

        def report_cleanup_error(paths: Sequence[Path]) -> list[str]:
            if os.name == "nt":
                release_errors = wait_for_log_release(
                    paths,
                    probe=probe_delete_access,
                    monotonic=time.monotonic,
                    sleep=time.sleep,
                )
                self.assertEqual(release_errors, [])
            return [cleanup_error]

        with self.assertRaises(CommandTimedOut) as caught:
            self.runner(log_cleanup=report_cleanup_error).run(
                [sys.executable, str(self.fake), "--sleep"],
                cwd=self.work,
                timeout=0.5,
                label="mutation",
            )
        record = caught.exception.record
        self.assertEqual(str(caught.exception), "command timed out: mutation")
        self.assertEqual(record.cleanup_errors, [cleanup_error])
        self.assertTrue(record.timed_out)
        self.assertIsNotNone(record.elapsed_seconds)
        self.assertIn("OUT:--sleep", self.stdout(record))
        self.assert_recorded_working_directory(record)

    def test_timeout_records_failed_post_kill_reap_and_blocks_reuse(
        self,
    ) -> None:
        process = UnreapableProcess(
            subprocess.TimeoutExpired(["fake-command"], 5.0)
        )
        popen_factory = mock.Mock(return_value=process)
        runner = self.runner(
            popen_factory=popen_factory,
            windows_tree_terminator=mock.Mock(),
        )

        def invoke() -> None:
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="unreapable-timeout",
            )

        if os.name == "nt":
            with self.assertRaises(CommandTimedOut) as caught:
                invoke()
        else:
            with (
                mock.patch(
                    "tools.focused_mutation_support.runner.os.killpg"
                ) as killpg,
                self.assertRaises(CommandTimedOut) as caught,
            ):
                invoke()
            self.assertEqual(
                killpg.call_args_list,
                [
                    mock.call(process.pid, signal.SIGTERM),
                    mock.call(process.pid, signal.SIGKILL),
                ],
            )

        record = caught.exception.record
        self.assertIsNone(record.exit_code)
        self.assertEqual(
            record.cleanup_errors,
            [
                "process lifecycle cleanup failed: "
                "root process 12345 was not reaped after forced kill"
            ],
        )
        self.assertEqual(process.wait_calls, [5.0, 2.0, 2.0])
        self.assertEqual(process.terminate_calls, 0)
        self.assertEqual(process.kill_calls, int(os.name == "nt"))

        with self.assertRaises(ProcessLifecycleError):
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="must-not-start",
            )
        self.assertEqual(popen_factory.call_count, 1)

    def test_disk_stop_remains_primary_when_forced_reap_fails(self) -> None:
        process = UnreapableProcess(
            subprocess.TimeoutExpired(["fake-command"], 5.0)
        )
        popen_factory = mock.Mock(return_value=process)
        runner = self.runner(
            popen_factory=popen_factory,
            windows_tree_terminator=mock.Mock(),
        )
        failure = mock.Mock(code="filesystem.reserve.reached")
        guard = mock.Mock(failure=failure)
        guard.sample.return_value = None

        def invoke() -> None:
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="unreapable-disk-stop",
                disk_guard=guard,
            )

        if os.name == "nt":
            with self.assertRaises(CommandDiskStopped) as caught:
                invoke()
        else:
            with (
                mock.patch(
                    "tools.focused_mutation_support.runner.os.killpg"
                ),
                self.assertRaises(CommandDiskStopped) as caught,
            ):
                invoke()

        self.assertIs(caught.exception.failure, failure)
        self.assertFalse(runner.cleanup_safe)
        self.assertEqual(
            caught.exception.record.cleanup_errors,
            [
                "process lifecycle cleanup failed: "
                "root process 12345 was not reaped after forced kill"
            ],
        )

    def test_spool_reservation_stops_before_materializing_retained_output(
        self,
    ) -> None:
        runner = self.runner()
        failure = DiskFailure(
            code="filesystem.reserve.reached",
            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
        )
        guard = mock.Mock(failure=None)
        guard.sample.return_value = None
        guard.reserve_additional_bytes.return_value = failure

        with self.assertRaises(CommandDiskStopped) as caught:
            runner.run(
                [sys.executable, str(self.fake), "payload"],
                cwd=self.work,
                timeout=5.0,
                label="spool-reserve",
                disk_guard=guard,
            )

        guard.reserve_additional_bytes.assert_called_once()
        self.assertIs(caught.exception.failure, failure)
        self.assertFalse(Path(caught.exception.record.stdout_path).exists())
        self.assertFalse(Path(caught.exception.record.stderr_path).exists())

    def test_task10_reservation_uses_command_root_capability(self) -> None:
        store, scratch = self.task10_store()
        runner = CommandRunner(store)
        failure = DiskFailure(
            code="filesystem.reserve.reached",
            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
        )
        guard = mock.Mock(failure=None)
        guard.sample.return_value = None
        guard.reserve_additional_bytes.return_value = failure

        try:
            with self.assertRaises(CommandDiskStopped) as caught:
                runner.run(
                    [sys.executable, str(self.fake), "payload"],
                    cwd=self.work,
                    timeout=5.0,
                    label="capability-reserve",
                    disk_guard=guard,
                )
            spool = caught.exception.record._spool
            self.assertIsInstance(spool, CommandPaths)
            assert isinstance(spool, CommandPaths)
            reservation = guard.reserve_additional_bytes.call_args
            self.assertEqual(reservation.kwargs, {"filesystem": spool.root})
            self.assertGreater(reservation.args[0], 0)
            self.assertFalse(spool.root.is_open)
        finally:
            self.close_task10_store(store, scratch)

    def test_task10_preflight_stop_discards_and_releases_command_duplicate(
        self,
    ) -> None:
        store, scratch = self.task10_store()
        runner = CommandRunner(store)
        failure = DiskFailure(
            code="workspace.size.exceeded",
            reason=DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
        )
        guard = mock.Mock(failure=failure)
        guard.sample.return_value = failure

        try:
            with self.assertRaises(CommandDiskStopped) as caught:
                runner.run(
                    [str(self.fake)],
                    cwd=self.work,
                    timeout=5.0,
                    label="preflight-capability",
                    disk_guard=guard,
                )
            spool = caught.exception.record._spool
            self.assertIsInstance(spool, CommandPaths)
            assert isinstance(spool, CommandPaths)
            self.assertFalse(spool.root.is_open)
            self.assertEqual(store.close_command_root(), ())
        finally:
            self.close_task10_store(store, scratch)

    def test_task10_pipe_setup_failure_releases_spool_and_descriptors(self) -> None:
        for stage in ("first-pipe", "second-pipe", "fdopen"):
            with self.subTest(stage=stage):
                store, scratch = self.task10_store()
                runner = CommandRunner(store)
                created_descriptors: list[int] = []
                captured_paths: list[CommandPaths] = []
                real_pipe = os.pipe
                real_fdopen = os.fdopen
                real_command_paths = store.command_paths
                pipe_calls = 0

                def capture_paths(sequence: int, label: str) -> CommandPaths:
                    paths = real_command_paths(sequence, label)
                    captured_paths.append(paths)
                    return paths

                def staged_pipe() -> tuple[int, int]:
                    nonlocal pipe_calls
                    pipe_calls += 1
                    if stage == "first-pipe" or (
                        stage == "second-pipe" and pipe_calls == 2
                    ):
                        raise OSError(f"injected {stage} allocation failure")
                    descriptors = real_pipe()
                    created_descriptors.extend(descriptors)
                    return descriptors

                def staged_fdopen(*args: object, **kwargs: object) -> BinaryIO:
                    if stage == "fdopen":
                        raise OSError("injected fdopen setup failure")
                    return real_fdopen(*args, **kwargs)  # type: ignore[arg-type]

                try:
                    with (
                        mock.patch.object(
                            store,
                            "command_paths",
                            side_effect=capture_paths,
                        ),
                        mock.patch(
                            "tools.focused_mutation_support.runner.os.pipe",
                            side_effect=staged_pipe,
                        ),
                        mock.patch(
                            "tools.focused_mutation_support.runner.os.fdopen",
                            side_effect=staged_fdopen,
                        ),
                        self.assertRaisesRegex(OSError, "injected"),
                    ):
                        runner.run(
                            [sys.executable, "-c", "pass"],
                            cwd=self.work,
                            timeout=5.0,
                            label=f"setup-{stage}",
                        )

                    self.assertEqual(len(captured_paths), 1)
                    self.assertFalse(captured_paths[0].root.is_open)
                    self.assertEqual(store.close_command_root(), ())
                    for descriptor in created_descriptors:
                        with self.assertRaises(OSError):
                            os.fstat(descriptor)
                finally:
                    for descriptor in created_descriptors:
                        try:
                            os.close(descriptor)
                        except OSError:
                            pass
                    if captured_paths and captured_paths[0].root.is_open:
                        captured_paths[0].close()
                    self.close_task10_store(store, scratch)

    def test_task10_returned_writer_close_failure_blocks_delete_then_resumes(
        self,
    ) -> None:
        store, scratch = self.task10_store()
        runner = CommandRunner(store)
        real_fdopen = os.fdopen
        real_command_paths = store.command_paths
        captured_paths: list[CommandPaths] = []
        raw_writers: list[_Task10RunnerCloseStream] = []

        def capture_paths(sequence: int, label: str) -> CommandPaths:
            paths = real_command_paths(sequence, label)
            captured_paths.append(paths)
            return paths

        def wrap_spool_writer(*args: object, **kwargs: object) -> BinaryIO:
            raw = cast(Callable[..., BinaryIO], real_fdopen)(*args, **kwargs)
            if kwargs.get("closefd") is True and args[1:2] == ("wb",):
                wrapped = _Task10RunnerCloseStream(raw, 2)
                raw_writers.append(wrapped)
                return cast(BinaryIO, wrapped)
            return raw

        try:
            with (
                mock.patch.object(
                    store, "command_paths", side_effect=capture_paths
                ),
                mock.patch(
                    "tools.focused_mutation_support.runner.os.fdopen",
                    side_effect=wrap_spool_writer,
                ),
                self.assertRaisesRegex(OSError, "runner stream close failure"),
            ):
                runner.run(
                    [sys.executable, str(self.fake), "payload"],
                    cwd=self.work,
                    timeout=5.0,
                    label="writer-close-owner",
                )
            self.assertEqual(len(captured_paths), 1)
            paths = captured_paths[0]
            self.assertEqual(raw_writers[0].close_calls, 2)
            self.assertFalse(raw_writers[0].closed)
            self.assertTrue(paths.stdout.exists())
            self.assertTrue(paths.root.is_open)
            self.assertTrue(store._live_root_tokens)

            raw_writers[0].failures = 0
            raw_writers[0].close()
            paths.discard()
            self.assertFalse(paths.stdout.exists())
            self.assertFalse(paths.root.is_open)
            self.assertFalse(store._live_root_tokens)
        finally:
            for stream in raw_writers:
                if not stream.closed:
                    stream.failures = 0
                    stream.close()
            for paths in captured_paths:
                paths.discard()
            self.close_task10_store(store, scratch)

    def test_task10_runner_closes_pipe_owners_before_command_discard(self) -> None:
        store, scratch = self.task10_store()
        runner = CommandRunner(
            store,
            popen_factory=mock.Mock(side_effect=OSError("injected popen failure")),
        )
        real_fdopen = os.fdopen
        real_discard = CommandPaths.discard
        events: list[str] = []
        streams: list[_Task10RunnerCloseStream] = []

        def wrap_pipe(*args: object, **kwargs: object) -> BinaryIO:
            raw = cast(Callable[..., BinaryIO], real_fdopen)(*args, **kwargs)
            wrapped = _Task10RunnerCloseStream(raw, 0, events)
            streams.append(wrapped)
            return cast(BinaryIO, wrapped)

        def record_discard(paths: CommandPaths) -> tuple[str, ...]:
            events.append("discard")
            return real_discard(paths)

        try:
            with (
                mock.patch(
                    "tools.focused_mutation_support.runner.os.fdopen",
                    side_effect=wrap_pipe,
                ),
                mock.patch.object(
                    CommandPaths,
                    "discard",
                    autospec=True,
                    side_effect=record_discard,
                ),
                self.assertRaisesRegex(OSError, "popen failure"),
            ):
                runner.run(
                    [sys.executable, "-c", "pass"],
                    cwd=self.work,
                    timeout=5.0,
                    label="pipe-close-order",
                )
            self.assertIn("discard", events)
            discard_index = events.index("discard")
            self.assertGreaterEqual(discard_index, 4, events)
            self.assertTrue(
                all(event == "stream-close" for event in events[:discard_index]),
                events,
            )
        finally:
            for stream in streams:
                if not stream.closed:
                    stream.close()
            self.close_task10_store(store, scratch)

    def test_task10_disk_boundary_exceptions_release_token_and_exact_spools(
        self,
    ) -> None:
        for stage in ("preflight", "post-drain", "reserve", "post-write"):
            with self.subTest(stage=stage):
                store, scratch = self.task10_store()
                runner = CommandRunner(store)
                real_command_paths = store.command_paths
                captured: list[CommandPaths] = []

                def capture_paths(sequence: int, label: str) -> CommandPaths:
                    paths = real_command_paths(sequence, label)
                    captured.append(paths)
                    return paths

                try:
                    with (
                        mock.patch.object(
                            store,
                            "command_paths",
                            side_effect=capture_paths,
                        ),
                        self.assertRaisesRegex(
                            OSError, f"injected {stage}"
                        ) as caught,
                    ):
                        runner.run(
                            [sys.executable, str(self.fake), stage],
                            cwd=self.work,
                            timeout=5.0,
                            label=f"boundary-{stage}",
                            disk_guard=_Task10BoundaryGuard(stage),
                        )
                    self.assertEqual(str(caught.exception), f"injected {stage} " + (
                        "sample failure" if stage != "reserve" else "failure"
                    ))
                    self.assertEqual(len(captured), 1)
                    paths = captured[0]
                    self.assertFalse(paths.stdout.exists())
                    self.assertFalse(paths.stderr.exists())
                    self.assertFalse(paths.root.is_open)
                    self.assertFalse(store._live_root_tokens)
                    self.assertLessEqual(
                        len(getattr(caught.exception, "__notes__", ())), 64
                    )
                finally:
                    for paths in captured:
                        paths.discard()
                    self.close_task10_store(store, scratch)

    def test_post_drain_disk_stop_wins_over_command_timeout(self) -> None:
        runner = self.runner()
        failure = DiskFailure(
            code="filesystem.reserve.reached",
            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
        )
        guard = mock.Mock(failure=None)
        guard.sample.return_value = None
        guard.reserve_additional_bytes.return_value = failure

        with self.assertRaises(CommandDiskStopped) as caught:
            runner.run(
                [sys.executable, str(self.fake), "--sleep"],
                cwd=self.work,
                timeout=0.01,
                label="timeout-spool-reserve",
                disk_guard=guard,
            )

        guard.reserve_additional_bytes.assert_called_once()
        self.assertIs(caught.exception.failure, failure)
        self.assertTrue(caught.exception.record.timed_out)
        self.assertEqual(
            caught.exception.record.disk_stop_code,
            "filesystem.reserve.reached",
        )
        self.assertFalse(Path(caught.exception.record.stdout_path).exists())
        self.assertFalse(Path(caught.exception.record.stderr_path).exists())

    def test_command_cwd_is_validated_before_spool_or_launch(self) -> None:
        runner_module = __import__(
            "tools.focused_mutation_support.runner",
            fromlist=["validate_reported_path"],
        )
        real_validate = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["validate_reported_path"],
        ).validate_reported_path

        def reject_cwd(path: Path) -> str:
            if path == self.work:
                raise ValueError("injected cwd report path rejection")
            return real_validate(path)

        with (
            mock.patch.object(
                runner_module,
                "validate_reported_path",
                side_effect=reject_cwd,
                create=True,
            ),
            mock.patch.object(
                self.store,
                "command_paths",
                side_effect=AssertionError("spool path created before cwd validation"),
            ),
            self.assertRaisesRegex(ValueError, "cwd report path"),
        ):
            self.runner().run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="invalid-cwd",
            )

    def test_post_write_disk_stop_is_typed_before_return(self) -> None:
        runner = self.runner()
        failure = DiskFailure(
            code="filesystem.reserve.reached",
            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
        )
        guard = mock.Mock(failure=None)
        guard.sample.side_effect = [None, None, failure]
        guard.reserve_additional_bytes.return_value = None

        with self.assertRaises(CommandDiskStopped) as caught:
            runner.run(
                [sys.executable, str(self.fake), "payload"],
                cwd=self.work,
                timeout=5.0,
                label="post-write-disk-stop",
                disk_guard=guard,
            )

        self.assertIs(caught.exception.failure, failure)
        self.assertEqual(guard.sample.call_count, 3)

    def test_drain_error_remains_cleanup_unsafe_when_disk_also_stops(
        self,
    ) -> None:
        runner = self.runner()
        failure = DiskFailure(
            code="filesystem.reserve.reached",
            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
        )
        guard = mock.Mock(failure=None)
        guard.sample.return_value = None
        guard.reserve_additional_bytes.return_value = failure

        with (
            mock.patch(
                "tools.focused_mutation_support.runner._BoundedCapture.drain",
                autospec=True,
                side_effect=lambda capture, stream: (
                    setattr(capture, "error", OSError("injected drain failure")),
                    stream.close(),
                ),
            ),
            self.assertRaises(CommandDiskStopped) as caught,
        ):
            runner.run(
                [sys.executable, str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="drain-and-disk-stop",
                disk_guard=guard,
            )

        self.assertFalse(runner.output_drain_safe)
        self.assertTrue(
            any(
                "injected drain failure" in item
                for item in caught.exception.record.cleanup_errors
            )
        )

    def test_spool_read_preserves_primary_when_descriptor_close_fails(
        self,
    ) -> None:
        paths = self.store.command_paths(77, "read-close")
        with paths.open_writer("stdout") as stream:
            stream.write(b"payload")
        store_module = __import__(
            "tools.focused_mutation_support.store",
            fromlist=["CommandPaths"],
        )
        real_close = store_module.os.close

        def close_then_fail(descriptor: int) -> None:
            real_close(descriptor)
            raise OSError("injected spool descriptor close failure")

        with (
            mock.patch.object(
                store_module.os,
                "read",
                side_effect=OSError("injected spool read failure"),
            ),
            mock.patch.object(
                store_module.os,
                "close",
                side_effect=close_then_fail,
            ),
            self.assertRaisesRegex(
                OSError, "injected spool read failure"
            ) as caught,
        ):
            paths.read("stdout", 32)

        self.assertTrue(
            any(
                "spool descriptor close failure" in note
                for note in getattr(caught.exception, "__notes__", ())
            ),
            repr(getattr(caught.exception, "__notes__", ())),
        )
        self.assertTrue(
            any(
                "spool descriptor close failure" in detail
                for detail in paths.capability_errors
            )
        )
        paths.discard()
        self.store.close_command_root()

    @unittest.skipUnless(os.name == "nt", "requires Windows handle inheritance")
    def test_windows_timeout_terminates_inherited_handle_descendant(self) -> None:
        root_ready = self.work / "root.ready"
        descendant_ready = self.work / "descendant.ready"
        release = self.work / "release"
        captured: list[WindowsProcessHandle] = []
        capture_done = threading.Event()
        capture_errors: list[BaseException] = []

        def capture_descendant() -> None:
            try:
                deadline = time.monotonic() + 5.0
                while time.monotonic() < deadline:
                    pid = read_ready_pid(descendant_ready)
                    if pid is not None:
                        captured.append(
                            WindowsProcessHandle.open(
                                pid,
                                SYNCHRONIZE | PROCESS_TERMINATE,
                            )
                        )
                        return
                    time.sleep(0.01)
                raise TimeoutError("descendant marker was not ready")
            except BaseException as error:
                capture_errors.append(error)
            finally:
                capture_done.set()

        capture_thread = threading.Thread(
            target=capture_descendant,
            daemon=True,
        )

        def cleanup_descendant() -> None:
            release.write_text("release", encoding="utf-8")
            capture_done.wait(timeout=5.0)
            capture_thread.join(timeout=1.0)
            while captured:
                handle = captured.pop()
                try:
                    handle.stop(grace_ms=0)
                finally:
                    handle.close()

        self.addCleanup(cleanup_descendant)
        capture_thread.start()

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

        capture_done.wait(timeout=5.0)
        capture_thread.join(timeout=1.0)
        self.assertFalse(capture_thread.is_alive())
        self.assertEqual(capture_errors, [])
        self.assertEqual(len(captured), 1)
        if not captured:
            return
        descendant = captured.pop()
        try:
            self.assertEqual(descendant.wait(5_000), WAIT_OBJECT_0)
        finally:
            descendant.stop(grace_ms=0)
            descendant.close()

        record = caught.exception.record
        self.assertEqual(record.cleanup_errors, [])
        stdout = Path(record.stdout_path).read_text()
        self.assertIn("ROOT-READY", stdout)
        self.assertIn("DESCENDANT-READY", stdout)

    @unittest.skipIf(os.name == "nt", "requires POSIX process groups")
    def test_handled_timeout_leaves_no_posix_descendants(self) -> None:
        root_ready = self.work / "posix-root.ready"
        descendant_ready = self.work / "posix-descendant.ready"
        release = self.work / "posix-release"

        with self.assertRaises(CommandTimedOut):
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
                label="posix-process-tree-timeout",
            )

        root_pid = read_ready_pid(root_ready)
        descendant_pid = read_ready_pid(descendant_ready)
        self.assertIsNotNone(root_pid)
        self.assertIsNotNone(descendant_pid)
        if root_pid is None or descendant_pid is None:
            return
        self.assertTrue(wait_for_pid_exit(root_pid), f"root {root_pid} survived")
        self.assertTrue(
            wait_for_pid_exit(descendant_pid),
            f"descendant {descendant_pid} survived",
        )

    @unittest.skipIf(os.name == "nt", "requires POSIX process groups")
    def test_timeout_kills_term_resistant_descendant_after_root_exits(self) -> None:
        root_ready = self.work / "term-resistant-root.ready"
        descendant_ready = self.work / "term-resistant-descendant.ready"
        term_received = self.work / "term-resistant-root.term"
        descendant_pid: int | None = None

        try:
            with self.assertRaises(CommandTimedOut):
                self.runner().run(
                    [
                        sys.executable,
                        str(self.term_resistant_descendant_fake),
                        str(root_ready),
                        str(descendant_ready),
                        str(term_received),
                    ],
                    cwd=self.work,
                    timeout=0.2,
                    label="term-resistant-descendant",
                )
            descendant_pid = read_ready_pid(descendant_ready)
            self.assertIsNotNone(descendant_pid)
            self.assertTrue(term_received.is_file(), "root did not receive SIGTERM")
            if descendant_pid is not None:
                self.assertTrue(
                    wait_for_pid_exit_or_zombie(descendant_pid),
                    f"descendant {descendant_pid} survived",
                )
        finally:
            if descendant_pid is None:
                descendant_pid = read_ready_pid(descendant_ready)
            if descendant_pid is not None:
                try:
                    os.kill(descendant_pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

    @unittest.skipIf(os.name == "nt", "requires POSIX process groups")
    def test_normal_exit_with_live_descendant_blocks_cleanup_without_signalling(
        self,
    ) -> None:
        descendant_ready = self.work / "normal-exit-descendant.ready"
        result: list[CommandRecord] = []
        errors: list[BaseException] = []

        def invoke() -> None:
            try:
                result.append(
                    self.runner().run(
                        [
                            sys.executable,
                            str(self.normal_exit_with_descendant_fake),
                            str(descendant_ready),
                        ],
                        cwd=self.work,
                        timeout=2.0,
                        label="normal-exit-with-descendant",
                    )
                )
            except BaseException as error:
                errors.append(error)

        thread = threading.Thread(target=invoke, daemon=True)
        thread.start()
        descendant_pid: int | None = None
        try:
            deadline = time.monotonic() + 2.0
            while descendant_pid is None and time.monotonic() < deadline:
                descendant_pid = read_ready_pid(descendant_ready)
                time.sleep(0.01)
            self.assertIsNotNone(descendant_pid)
            thread.join(timeout=1.0)
            self.assertFalse(thread.is_alive(), "runner blocked probing descendants")
            self.assertEqual(result, [])
            self.assertEqual(len(errors), 1)
            self.assertIsInstance(errors[0], CommandLifecycleFailed)
            if descendant_pid is not None:
                os.kill(descendant_pid, 0)
        finally:
            if descendant_pid is None:
                descendant_pid = read_ready_pid(descendant_ready)
            if descendant_pid is not None:
                try:
                    os.kill(descendant_pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            thread.join(timeout=3.0)

    def test_output_drain_deadline_marks_cleanup_unsafe(self) -> None:
        held: list[int] = []

        class CompletedProcess:
            pid = 12345
            returncode = 0

            def wait(self, timeout: float | None = None) -> int:
                return 0

        def popen_factory(*_args: object, **kwargs: object) -> CompletedProcess:
            for name in ("stdout", "stderr"):
                stream = kwargs[name]
                held.append(os.dup(stream.fileno()))  # type: ignore[union-attr]
            return CompletedProcess()

        def release() -> None:
            time.sleep(0.5)
            for fd in held:
                os.close(fd)

        releaser = threading.Thread(target=release)
        releaser.start()
        runner = self.runner(popen_factory=popen_factory)
        try:
            with (
                mock.patch(
                    "tools.focused_mutation_support.runner.OUTPUT_DRAIN_JOIN_TIMEOUT",
                    0.05,
                    create=True,
                ),
                self.assertRaisesRegex(RuntimeError, "output drain"),
            ):
                runner.run(
                    ["fake"],
                    cwd=self.work,
                    timeout=1.0,
                    label="held-output-pipe",
                )
            self.assertFalse(runner.cleanup_safe)
        finally:
            releaser.join(timeout=2.0)

    @unittest.skipIf(os.name == "nt", "requires POSIX process groups")
    def test_completed_process_quiesce_error_blocks_cleanup(self) -> None:
        runner = self.runner()
        with mock.patch.object(
            CommandRunner,
            "_quiesce_completed_process_tree",
            side_effect=ProcessLifecycleError(12345, "permission denied"),
        ), self.assertRaises(CommandLifecycleFailed) as raised:
            runner.run(
                [sys.executable, "-c", "pass"], cwd=self.work,
                timeout=2.0, label="quiesce-error",
            )

        record = raised.exception.record
        self.assertEqual(record.exit_code, 0)
        self.assertFalse(runner.process_drain_safe)
        self.assertTrue(
            any("permission denied" in item for item in record.cleanup_errors)
        )

    @unittest.skipIf(os.name == "nt", "requires POSIX process groups")
    def test_lifecycle_failure_with_unsettled_drain_never_materializes_spool(
        self,
    ) -> None:
        runner = self.runner()
        with (
            mock.patch.object(
                CommandRunner,
                "_quiesce_completed_process_tree",
                side_effect=ProcessLifecycleError(12345, "permission denied"),
            ),
            mock.patch.object(threading.Thread, "is_alive", return_value=True),
            mock.patch(
                "tools.focused_mutation_support.runner._BoundedCapture.write_to",
                side_effect=AssertionError(
                    "spool materialized before output drain settled"
                ),
            ) as write_to,
            self.assertRaises(CommandLifecycleFailed),
        ):
            runner.run(
                [sys.executable, "-c", "pass"],
                cwd=self.work,
                timeout=2.0,
                label="quiesce-and-drain-error",
            )

        write_to.assert_not_called()
        self.assertFalse(runner.cleanup_safe)

    @unittest.skipIf(os.name == "nt", "requires POSIX process groups")
    def test_posix_permission_error_is_a_process_lifecycle_error(self) -> None:
        process = mock.Mock(spec=subprocess.Popen)
        process.pid = 12345
        runner = self.runner()
        with (
            mock.patch(
                "tools.focused_mutation_support.runner.os.killpg",
                side_effect=PermissionError("denied"),
            ),
            self.assertRaises(ProcessLifecycleError),
        ):
            runner._quiesce_completed_process_tree(process)

    @unittest.skipIf(os.name == "nt", "requires POSIX process groups")
    def test_reaped_process_group_probe_never_sends_a_signal(self) -> None:
        process = mock.Mock(spec=subprocess.Popen)
        process.pid = 12345
        clock = FakeClock()
        runner = CommandRunner(
            self.store,
            monotonic=clock.monotonic,
            sleep=clock.sleep,
        )

        with (
            mock.patch(
                "tools.focused_mutation_support.runner.os.killpg",
            ) as killpg,
            self.assertRaisesRegex(
                ProcessLifecycleError,
                "still exists after root reap",
            ),
        ):
            runner._quiesce_completed_process_tree(process)

        self.assertGreaterEqual(clock.now, 0.25)
        self.assertGreaterEqual(killpg.call_count, 2)
        self.assertTrue(
            all(call.args == (process.pid, 0) for call in killpg.call_args_list)
        )

    def test_cancellation_after_pipe_setup_still_prevents_popen(self) -> None:
        cancellation = threading.Event()
        popen_factory = mock.Mock(
            side_effect=AssertionError("Popen must not be called")
        )
        runner = CommandRunner(
            self.store,
            popen_factory=popen_factory,
            cancellation_event=cancellation,
        )
        real_pipe = os.pipe
        pipe_calls = 0

        def cancel_after_second_pipe() -> tuple[int, int]:
            nonlocal pipe_calls
            descriptors = real_pipe()
            pipe_calls += 1
            if pipe_calls == 2:
                cancellation.set()
            return descriptors

        with (
            mock.patch(
                "tools.focused_mutation_support.runner.os.pipe",
                side_effect=cancel_after_second_pipe,
            ),
            self.assertRaises(CommandInterrupted),
        ):
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=2.0,
                label="cancel-before-popen",
            )

        popen_factory.assert_not_called()

    def test_drain_thread_start_failure_forces_process_reap(self) -> None:
        launched: list[subprocess.Popen[bytes]] = []

        def popen_factory(
            argv: Sequence[str], **kwargs: object
        ) -> subprocess.Popen[bytes]:
            process = subprocess.Popen(argv, **kwargs)  # type: ignore[arg-type]
            launched.append(process)
            return process

        runner = self.runner(popen_factory=popen_factory)
        with (
            mock.patch(
                "tools.focused_mutation_support.runner.threading.Thread.start",
                side_effect=RuntimeError("injected thread start failure"),
            ),
            self.assertRaises(CommandLifecycleFailed) as caught,
        ):
            runner.run(
                [sys.executable, str(self.fake), "--sleep"],
                cwd=self.work,
                timeout=5.0,
                label="drain-start-failure",
            )

        self.assertEqual(len(launched), 1)
        self.assertIsNotNone(launched[0].returncode)
        self.assertIn("output-drain setup failed", str(caught.exception))
        self.assertTrue(runner.process_drain_safe)

    def test_output_read_error_blocks_cleanup_after_threads_join(self) -> None:
        def fail_drain(capture: object, stream: BinaryIO) -> None:
            setattr(capture, "error", OSError("injected read failure"))
            stream.close()

        runner = self.runner()
        with mock.patch(
            "tools.focused_mutation_support.runner._BoundedCapture.drain",
            new=fail_drain,
        ), self.assertRaises(CommandDrainFailed) as raised:
            runner.run(
                [sys.executable, "-c", "pass"],
                cwd=self.work,
                timeout=2.0,
                label="drain-read-error",
            )

        record = raised.exception.record
        self.assertEqual(record.exit_code, 0)
        self.assertFalse(runner.output_drain_safe)
        self.assertTrue(
            any("injected read failure" in item for item in record.cleanup_errors)
        )

    def test_wait_oserror_is_typed_and_marks_process_drain_unsafe(self) -> None:
        class WaitFailure:
            pid = 12345
            returncode = None

            def wait(self, timeout: float | None = None) -> int:
                raise OSError("injected wait failure")

        process = WaitFailure()
        runner = self.runner(popen_factory=lambda *_args, **_kwargs: process)

        with mock.patch.object(
            runner,
            "_terminate",
            side_effect=ProcessLifecycleError(12345, "forced cleanup attempted"),
        ) as terminate, self.assertRaises(CommandLifecycleFailed) as raised:
            runner.run(["fake"], cwd=self.work, timeout=2.0, label="wait-failure")

        terminate.assert_called_once_with(process)
        self.assertFalse(runner.process_drain_safe)
        self.assertTrue(
            any(
                "injected wait failure" in item
                for item in raised.exception.record.cleanup_errors
            )
        )

    def test_post_terminate_wait_oserror_is_process_lifecycle_error(self) -> None:
        process = mock.Mock()
        process.pid = 12345
        process.wait.side_effect = OSError("injected reap failure")
        runner = self.runner()

        if os.name == "nt":
            patcher = mock.patch.object(runner, "_windows_tree_terminator")
        else:
            patcher = mock.patch(
                "tools.focused_mutation_support.runner.os.killpg"
            )
        with patcher, self.assertRaisesRegex(
            ProcessLifecycleError, "injected reap failure"
        ):
            runner._terminate(process)

    def test_inherited_handle_failure_cleanup_cancels_and_closes_handles(
        self,
    ) -> None:
        root_ready = self.work / "root.ready"
        root_ready.write_text("101", encoding="utf-8")
        descendant_ready = self.work / "descendant.ready"
        descendant_ready.write_text("202", encoding="utf-8")
        release = self.work / "release"
        cleanup_wait_started = threading.Event()
        cancel = threading.Event()
        captured_handles: dict[str, WindowsProcessHandle] = {}
        capture_done = threading.Event()
        directory_cleanup_succeeded = threading.Event()
        thread_errors: list[str] = []
        errors_lock = threading.Lock()
        root_cleanup = NeverSignaledWindowsProcessHandle(cancel)
        root_wait = NeverSignaledWindowsProcessHandle(cancel)
        descendant_cleanup = NeverSignaledWindowsProcessHandle(cancel)

        def invoke_releaser() -> None:
            try:
                release_descendant_after_root_exit(
                    root_ready,
                    descendant_ready,
                    release,
                    cleanup_wait_started,
                    cancel,
                    captured_handles,
                    capture_done,
                    thread_errors,
                    errors_lock,
                )
            except BaseException as error:
                with errors_lock:
                    thread_errors.append(
                        f"releaser call: {type(error).__name__}: {error}"
                    )

        with mock.patch.object(
            WindowsProcessHandle,
            "open",
            side_effect=[root_cleanup, root_wait, descendant_cleanup],
        ):
            releaser = threading.Thread(target=invoke_releaser, daemon=True)
            releaser.start()
            self.assertTrue(capture_done.wait(timeout=1.0))
            self.assertTrue(root_wait.wait_started.wait(timeout=1.0))

            cleanup_inherited_handle_fixture(
                release,
                releaser,
                cancel,
                captured_handles,
                capture_done,
                directory_cleanup_succeeded,
                thread_errors,
                errors_lock,
            )

        self.assertTrue(cancel.is_set())
        self.assertFalse(releaser.is_alive())
        self.assertTrue(root_wait.wait_calls)
        self.assertTrue(
            all(
                0 < timeout_ms <= PROCESS_WAIT_SLICE_MS
                for timeout_ms in root_wait.wait_calls
            )
        )
        self.assertEqual(root_cleanup.stop_calls, [0])
        self.assertEqual(descendant_cleanup.stop_calls, [1_000])
        self.assertTrue(root_cleanup.closed)
        self.assertTrue(root_wait.closed)
        self.assertTrue(descendant_cleanup.closed)

    def test_early_failure_cleanup_captures_handles_before_cancelling(
        self,
    ) -> None:
        second_marker_read = threading.Event()
        cleanup_waiting_for_capture = threading.Event()
        markers_ready = threading.Event()
        cancel = threading.Event()

        class ObservedCaptureEvent(threading.Event):
            def wait(self, timeout: float | None = None) -> bool:
                cleanup_waiting_for_capture.set()
                return super().wait(timeout)

        capture_done = ObservedCaptureEvent()
        root_ready = self.work / "root.ready"
        descendant_ready = self.work / "descendant.ready"
        release = self.work / "release"
        cleanup_wait_started = threading.Event()
        captured_handles: dict[str, WindowsProcessHandle] = {}
        directory_cleanup_succeeded = threading.Event()
        thread_errors: list[str] = []
        errors_lock = threading.Lock()
        root_cleanup = NeverSignaledWindowsProcessHandle(cancel)
        root_wait = NeverSignaledWindowsProcessHandle(cancel)
        descendant_cleanup = NeverSignaledWindowsProcessHandle(cancel)
        marker_reads = 0

        def read_marker(path: Path) -> int | None:
            nonlocal marker_reads
            marker_reads += 1
            if marker_reads == 1:
                return None
            if marker_reads == 2:
                second_marker_read.set()
                if not markers_ready.wait(timeout=1.0):
                    raise TimeoutError("marker release was not signalled")
            if cancel.is_set():
                return None
            return 101 if path == root_ready else 202

        releaser = threading.Thread(
            target=release_descendant_after_root_exit,
            args=(
                root_ready,
                descendant_ready,
                release,
                cleanup_wait_started,
                cancel,
                captured_handles,
                capture_done,
                thread_errors,
                errors_lock,
            ),
            daemon=True,
        )
        cleanup_failures: list[BaseException] = []

        def invoke_cleanup() -> None:
            try:
                cleanup_inherited_handle_fixture(
                    release,
                    releaser,
                    cancel,
                    captured_handles,
                    capture_done,
                    directory_cleanup_succeeded,
                    thread_errors,
                    errors_lock,
                )
            except BaseException as error:
                cleanup_failures.append(error)

        cleanup_thread = threading.Thread(target=invoke_cleanup, daemon=True)
        try:
            with mock.patch.object(
                sys.modules[__name__],
                "read_ready_pid",
                side_effect=read_marker,
            ), mock.patch.object(
                WindowsProcessHandle,
                "open",
                side_effect=[
                    descendant_cleanup,
                    root_cleanup,
                    root_wait,
                ],
            ):
                releaser.start()
                self.assertTrue(second_marker_read.wait(timeout=1.0))
                cleanup_thread.start()
                self.assertTrue(
                    cleanup_waiting_for_capture.wait(timeout=1.0)
                )
                markers_ready.set()
                cleanup_thread.join(timeout=2.0)
        finally:
            markers_ready.set()
            cancel.set()
            releaser.join(timeout=1.0)
            if cleanup_thread.ident is not None:
                cleanup_thread.join(timeout=1.0)

        self.assertFalse(cleanup_thread.is_alive())
        self.assertFalse(releaser.is_alive())
        self.assertEqual(cleanup_failures, [])
        self.assertEqual(root_cleanup.stop_calls, [0])
        self.assertEqual(descendant_cleanup.stop_calls, [1_000])
        self.assertTrue(root_cleanup.closed)
        self.assertTrue(root_wait.closed)
        self.assertTrue(descendant_cleanup.closed)

    def test_interruption_remains_primary_when_cleanup_callback_raises(
        self,
    ) -> None:
        process = InterruptingProcess()
        popen_factory = mock.Mock(return_value=process)

        def invoke() -> CommandRecord:
            return self.runner(
                popen_factory=popen_factory,
                log_cleanup=raising_log_cleanup,
                windows_tree_terminator=mock.Mock(),
            ).run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="interruption-with-cleanup-error",
            )

        result: CommandRecord | BaseException
        try:
            if os.name == "nt":
                result = invoke()
            else:
                with mock.patch(
                    "tools.focused_mutation_support.runner.os.killpg",
                    side_effect=posix_group_absent_after_signal,
                ) as killpg:
                    result = invoke()
                self.assertEqual(
                    killpg.call_args_list,
                    [
                        mock.call(process.pid, signal.SIGTERM),
                        mock.call(process.pid, 0),
                    ],
                )
        except BaseException as error:
            result = error

        self.assertIsInstance(result, CommandInterrupted)
        if not isinstance(result, CommandInterrupted):
            return
        self.assertEqual(
            str(result),
            "command interrupted: interruption-with-cleanup-error",
        )
        self.assertEqual(
            result.record.cleanup_errors,
            [LOG_CLEANUP_CALLBACK_ERROR],
        )

    def test_interruption_terminates_process_and_returns_completed_record(self) -> None:
        process = InterruptingProcess()
        popen_factory = mock.Mock(return_value=process)
        cleanup_error = (
            "stderr.log: log was not delete-ready within 2.0 seconds"
        )

        def invoke() -> None:
            self.runner(
                popen_factory=popen_factory,
                log_cleanup=lambda _: [cleanup_error],
                windows_tree_terminator=mock.Mock(),
            ).run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="interrupted",
            )

        if os.name == "nt":
            with self.assertRaises(CommandInterrupted) as caught:
                invoke()
        else:
            with (
                mock.patch(
                    "tools.focused_mutation_support.runner.os.killpg",
                    side_effect=posix_group_absent_after_signal,
                ) as killpg,
                self.assertRaises(CommandInterrupted) as caught,
            ):
                invoke()
            self.assertEqual(
                killpg.call_args_list,
                [
                    mock.call(process.pid, signal.SIGTERM),
                    mock.call(process.pid, 0),
                ],
            )

        record = caught.exception.record
        self.assertEqual(
            str(caught.exception),
            "command interrupted: interrupted",
        )
        self.assertEqual(record.cleanup_errors, [cleanup_error])
        self.assertTrue(record.interrupted)
        self.assertEqual(record.exit_code, -15)
        self.assertIsNotNone(record.ended_at)
        self.assertIsNotNone(record.elapsed_seconds)
        if os.name == "nt":
            self.assertFalse(process.terminated)
        self.assertEqual(process.wait_calls, [5.0, 2.0])

    def test_interruption_records_failed_post_kill_reap_and_blocks_reuse(
        self,
    ) -> None:
        process = UnreapableProcess(KeyboardInterrupt())
        popen_factory = mock.Mock(return_value=process)
        runner = self.runner(
            popen_factory=popen_factory,
            windows_tree_terminator=mock.Mock(),
        )

        def invoke() -> None:
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="unreapable-interruption",
            )

        if os.name == "nt":
            with self.assertRaises(CommandInterrupted) as caught:
                invoke()
        else:
            with (
                mock.patch(
                    "tools.focused_mutation_support.runner.os.killpg"
                ) as killpg,
                self.assertRaises(CommandInterrupted) as caught,
            ):
                invoke()
            self.assertEqual(
                killpg.call_args_list,
                [
                    mock.call(process.pid, signal.SIGTERM),
                    mock.call(process.pid, signal.SIGKILL),
                ],
            )

        record = caught.exception.record
        self.assertIsNone(record.exit_code)
        self.assertEqual(
            record.cleanup_errors,
            [
                "process lifecycle cleanup failed: "
                "root process 12345 was not reaped after forced kill"
            ],
        )
        self.assertEqual(process.wait_calls, [5.0, 2.0, 2.0])

        with self.assertRaises(ProcessLifecycleError):
            runner.run(
                [str(self.fake)],
                cwd=self.work,
                timeout=5.0,
                label="must-not-start",
            )
        self.assertEqual(popen_factory.call_count, 1)


if __name__ == "__main__":
    unittest.main()
