from collections.abc import Callable, Mapping, Sequence
from datetime import datetime, timezone
import os
from pathlib import Path
import signal
import subprocess
import time
from typing import Any

from .model import CommandRecord
from .store import RunStore


WINDOWS_LOG_RELEASE_TIMEOUT = 2.0
WINDOWS_LOG_RELEASE_POLL_INTERVAL = 0.01


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


class CommandRunner:
    def __init__(
        self,
        store: RunStore,
        *,
        extra_env: Mapping[str, str] | None = None,
        popen_factory: Callable[..., Any] = subprocess.Popen,
        monotonic: Callable[[], float] = time.monotonic,
        utc_now: Callable[[], datetime] = lambda: datetime.now(timezone.utc),
    ) -> None:
        self._store = store
        self._extra_env = dict(extra_env or {})
        self._popen_factory = popen_factory
        self._monotonic = monotonic
        self._utc_now = utc_now
        self._next_sequence = 1

    def run(
        self,
        argv: Sequence[str],
        cwd: Path,
        timeout: float,
        label: str,
    ) -> CommandRecord:
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

        with (
            paths.stdout.open("wb") as stdout_file,
            paths.stderr.open("wb") as stderr_file,
        ):
            process = self._popen_factory(
                list(argv),
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
                self._terminate(process)
                self._complete(record, process, started)
                raise CommandTimedOut(record) from None
            except KeyboardInterrupt:
                record.interrupted = True
                self._terminate(process)
                self._complete(record, process, started)
                raise CommandInterrupted(record) from None
            self._complete(record, process, started)
        return record

    def _complete(
        self,
        record: CommandRecord,
        process: Any,
        started: float,
    ) -> None:
        record.exit_code = process.returncode
        record.ended_at = self._utc_now().isoformat()
        record.elapsed_seconds = self._monotonic() - started

    def _terminate(self, process: Any) -> None:
        if os.name == "nt":
            process.terminate()
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
            pass
