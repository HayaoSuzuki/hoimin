from collections.abc import Callable, Mapping, Sequence
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock
from typing import Any, BinaryIO

from tools.focused_mutation_support.model import CommandRecord
from tools.focused_mutation_support.runner import (
    CommandInterrupted,
    CommandRunner,
    CommandTimedOut,
    wait_for_log_release,
)
from tools.focused_mutation_support.store import RunStore
from tools.focused_mutation_support.windows_file import probe_delete_access


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


SYNCHRONIZE = 0x00100000
PROCESS_TERMINATE = 0x0001
INFINITE = 0xFFFFFFFF
WAIT_OBJECT_0 = 0x00000000
WAIT_TIMEOUT = 0x00000102
WAIT_FAILED = 0xFFFFFFFF
ERROR_INVALID_PARAMETER = 87


def wait_for_ready_pids(
    root_ready: Path,
    descendant_ready: Path,
    *,
    timeout: float = 5.0,
) -> tuple[int, int]:
    deadline = time.monotonic() + timeout
    pids: dict[Path, int] = {}
    markers = (root_ready, descendant_ready)
    while len(pids) != len(markers):
        for marker in markers:
            if marker in pids:
                continue
            try:
                pids[marker] = int(marker.read_text(encoding="utf-8"))
            except (FileNotFoundError, ValueError):
                pass
        if len(pids) == len(markers):
            break
        if time.monotonic() >= deadline:
            missing = ", ".join(
                str(marker) for marker in markers if marker not in pids
            )
            raise TimeoutError(f"process markers were not ready: {missing}")
        time.sleep(0.01)
    return pids[root_ready], pids[descendant_ready]


def release_descendant_after_root_exit(
    root_ready: Path,
    descendant_ready: Path,
    release: Path,
    ready_pids: dict[str, int],
    errors: list[str],
    errors_lock: threading.Lock,
) -> None:
    handle: int | None = None
    close_handle: Any = None

    def record_error(context: str, error: BaseException) -> None:
        with errors_lock:
            errors.append(f"{context}: {type(error).__name__}: {error}")

    try:
        root_pid, descendant_pid = wait_for_ready_pids(
            root_ready,
            descendant_ready,
        )
        with errors_lock:
            ready_pids.update(root=root_pid, descendant=descendant_pid)
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
        close_handle = kernel32.CloseHandle
        close_handle.argtypes = (wintypes.HANDLE,)
        close_handle.restype = wintypes.BOOL

        handle = open_process(SYNCHRONIZE, False, root_pid)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        wait_result = wait_for_single_object(handle, INFINITE)
        if wait_result != WAIT_OBJECT_0:
            if wait_result == WAIT_FAILED:
                raise ctypes.WinError(ctypes.get_last_error())
            raise OSError(
                f"WaitForSingleObject returned unexpected result "
                f"{wait_result:#x}"
            )
        release.write_text("release", encoding="utf-8")
    except BaseException as error:
        record_error("descendant releaser", error)
    finally:
        if handle and close_handle is not None and not close_handle(handle):
            record_error(
                "CloseHandle",
                ctypes.WinError(ctypes.get_last_error()),
            )


def stop_windows_process(pid: int, *, grace_ms: int) -> None:
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

    handle = open_process(SYNCHRONIZE | PROCESS_TERMINATE, False, pid)
    if not handle:
        code = ctypes.get_last_error()
        if code == ERROR_INVALID_PARAMETER:
            return
        raise ctypes.WinError(code)
    try:
        wait_result = wait_for_single_object(handle, grace_ms)
        if wait_result == WAIT_TIMEOUT:
            if not terminate_process(handle, 1):
                raise ctypes.WinError(ctypes.get_last_error())
            wait_result = wait_for_single_object(handle, 5_000)
        if wait_result != WAIT_OBJECT_0:
            if wait_result == WAIT_FAILED:
                raise ctypes.WinError(ctypes.get_last_error())
            raise OSError(
                f"WaitForSingleObject returned unexpected result "
                f"{wait_result:#x}"
            )
    finally:
        if not close_handle(handle):
            raise ctypes.WinError(ctypes.get_last_error())


def cleanup_inherited_handle_fixture(
    root_ready: Path,
    descendant_ready: Path,
    release: Path,
    releaser: threading.Thread,
    ready_pids: dict[str, int],
    thread_errors: list[str],
    errors_lock: threading.Lock,
) -> None:
    cleanup_errors: list[str] = []

    def record_cleanup_error(context: str, error: BaseException) -> None:
        cleanup_errors.append(f"{context}: {type(error).__name__}: {error}")

    with errors_lock:
        root_pid = ready_pids.get("root")
        descendant_pid = ready_pids.get("descendant")
    if root_pid is None or descendant_pid is None:
        try:
            root_pid, descendant_pid = wait_for_ready_pids(
                root_ready,
                descendant_ready,
                timeout=0.5,
            )
        except TimeoutError:
            pass

    if root_pid is not None:
        try:
            stop_windows_process(root_pid, grace_ms=0)
            if release.parent.exists():
                release.write_text("release", encoding="utf-8")
        except BaseException as error:
            record_cleanup_error("root cleanup", error)

    releaser.join(timeout=5.0)
    if releaser.is_alive():
        record_cleanup_error(
            "releaser cleanup",
            RuntimeError("releaser thread did not exit"),
        )

    if descendant_pid is not None:
        try:
            stop_windows_process(descendant_pid, grace_ms=1_000)
        except BaseException as error:
            record_cleanup_error("descendant cleanup", error)

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


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


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

    def runner(
        self,
        *,
        extra_env: dict[str, str] | None = None,
        popen_factory: Callable[..., Any] = subprocess.Popen,
        log_cleanup: Callable[[Sequence[Path]], list[str]] | None = None,
    ) -> CommandRunner:
        return CommandRunner(
            self.store,
            extra_env=extra_env,
            popen_factory=popen_factory,
            log_cleanup=log_cleanup,
            utc_now=lambda: datetime(2026, 7, 26, tzinfo=timezone.utc),
        )

    def stdout(self, record: CommandRecord) -> str:
        return Path(record.stdout_path).read_text()

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

    def test_timeout_terminates_process_and_keeps_partial_logs(self) -> None:
        cleanup_error = (
            "stderr.log: log was not delete-ready within 2.0 seconds"
        )
        with self.assertRaises(CommandTimedOut) as caught:
            self.runner(log_cleanup=lambda _: [cleanup_error]).run(
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

    @unittest.skipUnless(os.name == "nt", "requires Windows handle inheritance")
    def test_timeout_waits_for_inherited_log_handles_before_cleanup(self) -> None:
        root_ready = self.work / "root.ready"
        descendant_ready = self.work / "descendant.ready"
        release = self.work / "release"
        ready_pids: dict[str, int] = {}
        thread_errors: list[str] = []
        errors_lock = threading.Lock()
        releaser = threading.Thread(
            target=release_descendant_after_root_exit,
            args=(
                root_ready,
                descendant_ready,
                release,
                ready_pids,
                thread_errors,
                errors_lock,
            ),
            daemon=True,
        )
        releaser.start()
        self.addCleanup(
            cleanup_inherited_handle_fixture,
            root_ready,
            descendant_ready,
            release,
            releaser,
            ready_pids,
            thread_errors,
            errors_lock,
        )

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
        with errors_lock:
            self.assertEqual(thread_errors, [])
        record = caught.exception.record
        self.assertEqual(record.cleanup_errors, [])
        stdout = Path(record.stdout_path).read_text()
        self.assertIn("ROOT-READY", stdout)
        self.assertIn("DESCENDANT-READY", stdout)

        temporary_root = Path(self.temporary.name)
        self.temporary.cleanup()
        self.assertFalse(temporary_root.exists())

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
                    "tools.focused_mutation_support.runner.os.killpg"
                ) as killpg,
                self.assertRaises(CommandInterrupted) as caught,
            ):
                invoke()
            killpg.assert_called_once_with(process.pid, 15)

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
            self.assertTrue(process.terminated)
        self.assertEqual(process.wait_calls, [5.0, 2.0])


if __name__ == "__main__":
    unittest.main()
