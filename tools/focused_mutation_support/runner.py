from collections.abc import Callable, Mapping, Sequence
from datetime import datetime, timezone
import os
from pathlib import Path, PureWindowsPath
import signal
import subprocess
import sys
import time
from typing import Any

from .model import CommandRecord
from .store import RunStore
from .windows_file import probe_delete_access


WINDOWS_LOG_RELEASE_TIMEOUT = 2.0
WINDOWS_LOG_RELEASE_POLL_INTERVAL = 0.01
WINDOWS_PROCESS_TERMINATION_TIMEOUT = 2.0
_POSIX_PROCESS_SUPERVISOR = """
import os
import signal
import subprocess
import sys
import time

terminating = False
def hold_after_term(_signum, _frame):
    global terminating
    terminating = True

signal.signal(signal.SIGTERM, hold_after_term)
def restore_sigterm() -> None:
    signal.signal(signal.SIGTERM, signal.SIG_DFL)

child = subprocess.Popen(sys.argv[1:], preexec_fn=restore_sigterm)
returncode = child.wait()
if terminating:
    while True:
        time.sleep(1)
if returncode < 0:
    os.kill(os.getpid(), -returncode)
raise SystemExit(returncode)
"""


def terminate_windows_process_tree(
    pid: int,
    *,
    run: Callable[..., Any] | None = None,
) -> None:
    system_root = os.environ.get("SystemRoot")
    if system_root is None:
        raise OSError("SystemRoot is not set")
    taskkill = str(
        PureWindowsPath(system_root) / "System32" / "taskkill.exe"
    )
    execute = subprocess.run if run is None else run
    completed = execute(
        [taskkill, "/PID", str(pid), "/T", "/F"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=WINDOWS_PROCESS_TERMINATION_TIMEOUT,
        check=False,
    )
    if completed.returncode != 0:
        raise OSError(
            f"taskkill exited with status {completed.returncode}"
        )


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


class CommandTimedOut(Exception):
    def __init__(self, record: CommandRecord) -> None:
        super().__init__(f"command timed out: {record.label}")
        self.record = record


class CommandInterrupted(Exception):
    def __init__(self, record: CommandRecord) -> None:
        super().__init__(f"command interrupted: {record.label}")
        self.record = record


class ProcessLifecycleError(RuntimeError):
    def __init__(self, pid: int, message: str | None = None) -> None:
        self.pid = pid
        super().__init__(
            message
            if message is not None
            else f"root process {pid} was not reaped after forced kill"
        )


class CommandRunner:
    def __init__(
        self,
        store: RunStore,
        *,
        extra_env: Mapping[str, str] | None = None,
        popen_factory: Callable[..., Any] = subprocess.Popen,
        monotonic: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], None] = time.sleep,
        utc_now: Callable[[], datetime] = lambda: datetime.now(timezone.utc),
        log_cleanup: Callable[[Sequence[Path]], list[str]] | None = None,
        windows_tree_terminator: Callable[
            [int], None
        ] = terminate_windows_process_tree,
    ) -> None:
        self._store = store
        self._extra_env = dict(extra_env or {})
        self._popen_factory = popen_factory
        self._monotonic = monotonic
        self._sleep = sleep
        self._utc_now = utc_now
        self._log_cleanup = (
            log_cleanup
            if log_cleanup is not None
            else self._default_log_cleanup
        )
        self._has_custom_log_cleanup = log_cleanup is not None
        self._windows_tree_terminator = windows_tree_terminator
        self._next_sequence = 1
        self._lifecycle_error: ProcessLifecycleError | None = None

    def run(
        self,
        argv: Sequence[str],
        cwd: Path,
        timeout: float,
        label: str,
    ) -> CommandRecord:
        if self._lifecycle_error is not None:
            raise self._lifecycle_error
        sequence = self._next_sequence
        self._next_sequence += 1
        paths = self._store.command_paths(sequence, label)
        started = self._monotonic()
        record = CommandRecord(
            sequence=sequence,
            label=label,
            argv=list(argv),
            cwd=str(cwd),
            started_at=self._utc_now().isoformat(),
            stdout_path=str(paths.stdout),
            stderr_path=str(paths.stderr),
        )
        environment = os.environ.copy()
        environment.update(self._extra_env)

        outcome: type[CommandTimedOut] | type[CommandInterrupted] | None = None
        with (
            paths.stdout.open("wb") as stdout_file,
            paths.stderr.open("wb") as stderr_file,
        ):
            process = self._popen_factory(
                self._launch_argv(argv),
                cwd=cwd,
                stdin=subprocess.DEVNULL,
                stdout=stdout_file,
                stderr=stderr_file,
                env=environment,
                shell=False,
                start_new_session=(os.name != "nt"),
            )
            try:
                process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                record.timed_out = True
                try:
                    self._terminate(process)
                except ProcessLifecycleError as error:
                    self._record_lifecycle_error(record, error)
                outcome = CommandTimedOut
            except KeyboardInterrupt:
                record.interrupted = True
                try:
                    self._terminate(process)
                except ProcessLifecycleError as error:
                    self._record_lifecycle_error(record, error)
                outcome = CommandInterrupted
            self._complete(record, process, started)

        try:
            cleanup_errors = self._log_cleanup((paths.stdout, paths.stderr))
        except Exception as error:
            record.cleanup_errors.append(
                "log cleanup callback failed: "
                f"{type(error).__name__}: {error}"
            )
            if self._has_custom_log_cleanup:
                try:
                    fallback_errors = self._default_log_cleanup(
                        (paths.stdout, paths.stderr)
                    )
                except Exception as fallback_error:
                    record.cleanup_errors.append(
                        "default log cleanup failed: "
                        f"{type(fallback_error).__name__}: {fallback_error}"
                    )
                else:
                    record.cleanup_errors.extend(fallback_errors)
        else:
            record.cleanup_errors.extend(cleanup_errors)
        if outcome is CommandTimedOut:
            raise CommandTimedOut(record) from None
        if outcome is CommandInterrupted:
            raise CommandInterrupted(record) from None
        return record

    def _default_log_cleanup(self, paths: Sequence[Path]) -> list[str]:
        if os.name != "nt":
            return []
        return wait_for_log_release(
            paths,
            probe=probe_delete_access,
            monotonic=self._monotonic,
            sleep=self._sleep,
        )

    def _launch_argv(self, argv: Sequence[str]) -> list[str]:
        if os.name == "nt":
            return list(argv)
        return [sys.executable, "-c", _POSIX_PROCESS_SUPERVISOR, *argv]

    def _complete(
        self,
        record: CommandRecord,
        process: Any,
        started: float,
    ) -> None:
        record.exit_code = process.returncode
        record.ended_at = self._utc_now().isoformat()
        record.elapsed_seconds = self._monotonic() - started

    def _record_lifecycle_error(
        self,
        record: CommandRecord,
        error: ProcessLifecycleError,
    ) -> None:
        self._lifecycle_error = error
        record.cleanup_errors.append(
            f"process lifecycle cleanup failed: {error}"
        )

    def _terminate(self, process: Any) -> None:
        if os.name == "nt":
            try:
                self._windows_tree_terminator(process.pid)
            except (OSError, subprocess.SubprocessError) as error:
                try:
                    process.kill()
                    process.wait(timeout=2.0)
                except (OSError, subprocess.TimeoutExpired):
                    pass
                raise ProcessLifecycleError(
                    process.pid,
                    f"process tree {process.pid} termination failed: "
                    f"{type(error).__name__}: {error}",
                ) from None
        else:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        try:
            process.wait(timeout=2.0)
            return
        except subprocess.TimeoutExpired:
            pass

        if os.name == "nt":
            process.kill()
        else:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            process.wait(timeout=2.0)
        except subprocess.TimeoutExpired:
            raise ProcessLifecycleError(process.pid) from None
