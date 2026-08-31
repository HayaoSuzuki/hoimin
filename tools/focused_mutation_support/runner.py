from collections.abc import Callable, Mapping, Sequence
from datetime import datetime, timezone
import os
from pathlib import Path, PureWindowsPath
import signal
import subprocess
import sys
import threading
import time
from typing import Any

from .model import CommandRecord
from .disk import DiskFailure, MAX_COMMAND_LOG_BYTES, MAX_LOG_BYTES
from .store import RunStore
from .lease import validate_reported_path
from .windows_file import probe_delete_access


WINDOWS_LOG_RELEASE_TIMEOUT = 2.0
WINDOWS_LOG_RELEASE_POLL_INTERVAL = 0.01
WINDOWS_PROCESS_TERMINATION_TIMEOUT = 2.0
OUTPUT_DRAIN_JOIN_TIMEOUT = 2.0
POSIX_REAP_PROBE_TIMEOUT = 0.25
POSIX_REAP_PROBE_INTERVAL = 0.01
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
# Give a process-group SIGTERM already in flight one scheduling point before
# deciding that this was an ordinary child exit.  Without this, a child can
# exit from SIGTERM just before the supervisor's Python handler runs, letting
# a TERM-resistant descendant escape the later SIGKILL phase.
time.sleep(0.01)
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


class CommandDiskStopped(Exception):
    def __init__(self, record: CommandRecord, failure: DiskFailure) -> None:
        super().__init__(f"command stopped by disk guard: {failure.code}")
        self.record = record
        self.failure = failure


class CommandDrainFailed(RuntimeError):
    def __init__(self, record: CommandRecord) -> None:
        super().__init__(f"output drain did not settle: {record.label}")
        self.record = record


class CommandLifecycleFailed(RuntimeError):
    def __init__(
        self, record: CommandRecord, error: "ProcessLifecycleError"
    ) -> None:
        super().__init__(f"process lifecycle failed: {record.label}: {error}")
        self.record = record
        self.error = error


class ProcessLifecycleError(RuntimeError):
    def __init__(self, pid: int, message: str | None = None) -> None:
        self.pid = pid
        super().__init__(
            message
            if message is not None
            else f"root process {pid} was not reaped after forced kill"
        )


class _BoundedCapture:
    def __init__(self, allowance: int) -> None:
        self.allowance = allowance
        self.prefix_capacity = (allowance + 1) // 2
        self.tail_capacity = allowance - self.prefix_capacity
        self.prefix = bytearray()
        self.tail = bytearray(self.tail_capacity)
        self.tail_length = 0
        self.tail_index = 0
        self.observed = 0
        self.error: BaseException | None = None

    def feed(self, value: bytes) -> None:
        self.observed += len(value)
        prefix_missing = self.prefix_capacity - len(self.prefix)
        if prefix_missing > 0:
            taken = min(prefix_missing, len(value))
            self.prefix.extend(value[:taken])
            value = value[taken:]
        if not value or self.tail_capacity == 0:
            return
        if len(value) >= self.tail_capacity:
            self.tail[:] = value[-self.tail_capacity :]
            self.tail_length = self.tail_capacity
            self.tail_index = 0
            return
        first = min(len(value), self.tail_capacity - self.tail_index)
        self.tail[self.tail_index : self.tail_index + first] = value[:first]
        remaining = len(value) - first
        if remaining:
            self.tail[:remaining] = value[first:]
        self.tail_index = (self.tail_index + len(value)) % self.tail_capacity
        self.tail_length = min(self.tail_capacity, self.tail_length + len(value))

    def drain(self, stream: Any) -> None:
        try:
            while True:
                chunk = stream.read(64 * 1024)
                if not chunk:
                    return
                self.feed(chunk)
        except BaseException as error:
            self.error = error
        finally:
            stream.close()

    def write_to(self, stream: Any) -> int:
        stream.write(self.prefix)
        if self.tail_length == 0:
            pass
        elif self.tail_length < self.tail_capacity:
            stream.write(memoryview(self.tail)[: self.tail_length])
        else:
            stream.write(memoryview(self.tail)[self.tail_index :])
            stream.write(memoryview(self.tail)[: self.tail_index])
        return len(self.prefix) + self.tail_length

    @property
    def truncated(self) -> bool:
        return self.observed > self.allowance


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
        cancellation_event: threading.Event | None = None,
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
        self._cancellation_event = cancellation_event
        self._next_sequence = 1
        self._lifecycle_error: ProcessLifecycleError | None = None
        self._cleanup_unsafe: str | None = None

    def set_store(self, store: RunStore) -> None:
        if self._next_sequence != 1:
            raise RuntimeError("command store cannot change after dispatch")
        self._store = store

    @property
    def cleanup_safe(self) -> bool:
        return self._lifecycle_error is None and self._cleanup_unsafe is None

    @property
    def process_drain_safe(self) -> bool:
        return self._lifecycle_error is None

    @property
    def output_drain_safe(self) -> bool:
        return self._cleanup_unsafe is None

    @property
    def interrupted(self) -> bool:
        return (
            self._cancellation_event is not None
            and self._cancellation_event.is_set()
        )

    def run(
        self,
        argv: Sequence[str],
        cwd: Path,
        timeout: float,
        label: str,
        *,
        environment: Mapping[str, str] | None = None,
        disk_guard: object | None = None,
        max_log_bytes: int = MAX_LOG_BYTES,
    ) -> CommandRecord:
        if not 1 <= max_log_bytes <= MAX_COMMAND_LOG_BYTES:
            raise ValueError(
                f"max_log_bytes must be in 1..={MAX_COMMAND_LOG_BYTES}"
            )
        if self._lifecycle_error is not None:
            raise self._lifecycle_error
        if self._cleanup_unsafe is not None:
            raise RuntimeError(self._cleanup_unsafe)
        validate_reported_path(cwd)
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
            _spool=paths,
        )
        if disk_guard is not None:
            preflight = getattr(disk_guard, "sample", None)
            if preflight is None:
                raise TypeError("disk_guard must provide sample()")
            failure = preflight()
            if failure is not None:
                record.disk_stop_code = failure.code
                raise CommandDiskStopped(record, failure)
        if self.interrupted:
            record.interrupted = True
            raise CommandInterrupted(record)
        effective_environment = os.environ.copy()
        effective_environment.update(self._extra_env)
        if environment is not None:
            effective_environment.update(environment)

        outcome: (
            type[CommandTimedOut]
            | type[CommandInterrupted]
            | type[CommandDiskStopped]
            | None
        ) = None
        disk_failure: DiskFailure | None = None
        lifecycle_failure: ProcessLifecycleError | None = None
        stdout_read_fd, stdout_write_fd = os.pipe()
        stderr_read_fd, stderr_write_fd = os.pipe()
        stdout_read = os.fdopen(stdout_read_fd, "rb", buffering=0)
        stderr_read = os.fdopen(stderr_read_fd, "rb", buffering=0)
        stdout_write = os.fdopen(stdout_write_fd, "wb", buffering=0)
        stderr_write = os.fdopen(stderr_write_fd, "wb", buffering=0)
        stdout_capture = _BoundedCapture((max_log_bytes + 1) // 2)
        stderr_capture = _BoundedCapture(max_log_bytes // 2)
        stdout_thread = threading.Thread(
            target=stdout_capture.drain,
            args=(stdout_read,),
            name=f"focused-stdout-{sequence}",
            daemon=True,
        )
        stderr_thread = threading.Thread(
            target=stderr_capture.drain,
            args=(stderr_read,),
            name=f"focused-stderr-{sequence}",
            daemon=True,
        )
        drain_failed = False
        try:
            if self.interrupted:
                record.interrupted = True
                raise CommandInterrupted(record)
            process = self._popen_factory(
                self._launch_argv(argv),
                cwd=cwd,
                stdin=subprocess.DEVNULL,
                stdout=stdout_write,
                stderr=stderr_write,
                env=effective_environment,
                shell=False,
                start_new_session=(os.name != "nt"),
            )
            try:
                stdout_write.close()
                stderr_write.close()
                stdout_thread.start()
                stderr_thread.start()
            except Exception as error:
                lifecycle_failure = ProcessLifecycleError(
                    process.pid,
                    f"process {process.pid} output-drain setup failed: "
                    f"{type(error).__name__}: {error}",
                )
                record.cleanup_errors.append(str(lifecycle_failure))
                try:
                    self._terminate(process)
                except ProcessLifecycleError as cleanup_error:
                    self._record_lifecycle_error(record, cleanup_error)
                else:
                    record.cleanup_errors.append(
                        "output-drain setup failed, but forced termination "
                        "and reap completed"
                    )
            try:
                if lifecycle_failure is not None:
                    pass
                elif disk_guard is None and self._cancellation_event is None:
                    process.wait(timeout=timeout)
                else:
                    deadline = self._monotonic() + timeout
                    while True:
                        if self.interrupted:
                            record.interrupted = True
                            try:
                                self._terminate(process)
                            except ProcessLifecycleError as error:
                                self._record_lifecycle_error(record, error)
                            outcome = CommandInterrupted
                            break
                        disk_failure = getattr(disk_guard, "failure", None)
                        if disk_guard is not None and disk_failure is not None:
                            record.disk_stop_code = disk_failure.code
                            try:
                                self._terminate(process)
                            except ProcessLifecycleError as error:
                                self._record_lifecycle_error(record, error)
                            outcome = CommandDiskStopped
                            break
                        remaining = deadline - self._monotonic()
                        if remaining <= 0:
                            raise subprocess.TimeoutExpired(argv, timeout)
                        try:
                            process.wait(timeout=min(0.1, remaining))
                            break
                        except subprocess.TimeoutExpired:
                            continue
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
            except (OSError, subprocess.SubprocessError) as error:
                lifecycle_failure = ProcessLifecycleError(
                    process.pid,
                    f"process {process.pid} wait failed: "
                    f"{type(error).__name__}: {error}",
                )
                record.cleanup_errors.append(str(lifecycle_failure))
                try:
                    self._terminate(process)
                except ProcessLifecycleError as cleanup_error:
                    self._record_lifecycle_error(record, cleanup_error)
                else:
                    record.cleanup_errors.append(
                        "process wait failed, but forced termination and reap completed"
                    )
            if (
                outcome is None
                and lifecycle_failure is None
                and os.name != "nt"
                and isinstance(process, subprocess.Popen)
            ):
                try:
                    self._quiesce_completed_process_tree(process)
                except ProcessLifecycleError as error:
                    self._record_lifecycle_error(record, error)
                    lifecycle_failure = error
            self._complete(record, process, started)
        finally:
            for stream_name, writer in (
                ("stdout", stdout_write),
                ("stderr", stderr_write),
            ):
                try:
                    writer.close()
                except OSError as error:
                    record.cleanup_errors.append(
                        f"{stream_name} pipe writer close failed: "
                        f"{type(error).__name__}: {error}"
                    )
            drain_deadline = time.monotonic() + OUTPUT_DRAIN_JOIN_TIMEOUT
            for thread, stream, stream_name in (
                (stdout_thread, stdout_read, "stdout"),
                (stderr_thread, stderr_read, "stderr"),
            ):
                if thread.ident is None:
                    stream.close()
                    continue
                thread.join(max(0.0, drain_deadline - time.monotonic()))
                if thread.is_alive():
                    drain_failed = True
                    record.cleanup_errors.append(
                        f"{stream_name} output drain did not settle within "
                        f"{OUTPUT_DRAIN_JOIN_TIMEOUT} seconds"
                    )

        if self.interrupted and outcome is None:
            record.interrupted = True
            outcome = CommandInterrupted

        if drain_failed:
            self._cleanup_unsafe = "output drain did not settle"
        if drain_failed and lifecycle_failure is not None:
            raise CommandLifecycleFailed(record, lifecycle_failure) from None
        if drain_failed:
            if outcome is CommandTimedOut:
                raise CommandTimedOut(record) from None
            if outcome is CommandInterrupted:
                raise CommandInterrupted(record) from None
            if outcome is CommandDiskStopped:
                if disk_failure is None:
                    raise RuntimeError("disk stop lost its failure evidence")
                raise CommandDiskStopped(record, disk_failure) from None
            raise CommandDrainFailed(record)

        record.stdout_observed_bytes = stdout_capture.observed
        record.stdout_truncated = stdout_capture.truncated
        record.stderr_observed_bytes = stderr_capture.observed
        record.stderr_truncated = stderr_capture.truncated
        capture_failed = False
        for capture, stream_name in (
            (stdout_capture, "stdout"),
            (stderr_capture, "stderr"),
        ):
            if capture.error is not None:
                capture_failed = True
                self._cleanup_unsafe = "output drain failed"
                record.cleanup_errors.append(
                    f"{stream_name} drain failed: "
                    f"{type(capture.error).__name__}: {capture.error}"
                )

        if disk_guard is not None:
            post_drain_failure = getattr(disk_guard, "sample")()
            if post_drain_failure is None:
                retained_bytes = (
                    len(stdout_capture.prefix)
                    + stdout_capture.tail_length
                    + len(stderr_capture.prefix)
                    + stderr_capture.tail_length
                )
                post_drain_failure = getattr(
                    disk_guard, "reserve_additional_bytes"
                )(
                    retained_bytes,
                    filesystem_fd=paths.root_fd,
                )
            if post_drain_failure is not None:
                if disk_failure is None:
                    disk_failure = post_drain_failure
                record.disk_stop_code = disk_failure.code
                if outcome in {None, CommandTimedOut} and lifecycle_failure is None:
                    outcome = CommandDiskStopped

        materialized = False
        if disk_failure is not None:
            record.cleanup_errors.extend(paths.discard())
        else:
            try:
                for capture, stream_name in (
                    (stdout_capture, "stdout"),
                    (stderr_capture, "stderr"),
                ):
                    with paths.open_writer(stream_name) as spool_stream:
                        retained = capture.write_to(spool_stream)
                    if stream_name == "stdout":
                        record.stdout_retained_bytes = retained
                    else:
                        record.stderr_retained_bytes = retained
                materialized = True
            except BaseException as primary_error:
                self._cleanup_unsafe = "command spool materialization failed"
                spool_errors = paths.discard()
                record.cleanup_errors.extend(spool_errors)
                for spool_error in spool_errors:
                    primary_error.add_note(spool_error)
                raise
        if materialized and disk_guard is not None:
            post_write_failure = getattr(disk_guard, "sample")()
            if post_write_failure is not None:
                if disk_failure is None:
                    disk_failure = post_write_failure
                record.disk_stop_code = disk_failure.code
                record.cleanup_errors.extend(paths.discard())
                if outcome in {None, CommandTimedOut} and lifecycle_failure is None:
                    outcome = CommandDiskStopped
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
        if outcome is CommandDiskStopped:
            if disk_failure is None:
                raise RuntimeError("disk stop lost its failure evidence")
            raise CommandDiskStopped(record, disk_failure) from None
        if lifecycle_failure is not None:
            raise CommandLifecycleFailed(record, lifecycle_failure) from None
        if capture_failed:
            raise CommandDrainFailed(record) from None
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

    def _quiesce_completed_process_tree(
        self,
        process: subprocess.Popen[Any],
    ) -> None:
        """Prove that a reaped root's process group is no longer present.

        Once ``wait`` has reaped the root, its PID/process-group ID can be
        reused.  Sending a terminating signal at that point could therefore
        kill an unrelated process group.  Signal zero is non-destructive; a
        group that remains present or cannot be inspected makes cleanup unsafe.
        """
        deadline = self._monotonic() + POSIX_REAP_PROBE_TIMEOUT
        while True:
            try:
                os.killpg(process.pid, 0)
            except ProcessLookupError:
                return
            except OSError as error:
                raise ProcessLifecycleError(
                    process.pid,
                    f"process group {process.pid} post-reap probe failed: "
                    f"{type(error).__name__}: {error}",
                ) from None
            remaining = deadline - self._monotonic()
            if remaining <= 0:
                raise ProcessLifecycleError(
                    process.pid,
                    f"process group {process.pid} still exists after root reap",
                )
            self._sleep(min(POSIX_REAP_PROBE_INTERVAL, remaining))

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
            except OSError as error:
                raise ProcessLifecycleError(
                    process.pid,
                    f"process group {process.pid} termination failed: "
                    f"{type(error).__name__}: {error}",
                ) from None
        try:
            process.wait(timeout=2.0)
            if os.name != "nt":
                self._quiesce_completed_process_tree(process)
            return
        except subprocess.TimeoutExpired:
            pass
        except (OSError, subprocess.SubprocessError) as error:
            raise ProcessLifecycleError(
                process.pid,
                f"process {process.pid} reap after termination failed: "
                f"{type(error).__name__}: {error}",
            ) from None

        if os.name == "nt":
            process.kill()
        else:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            except OSError as error:
                raise ProcessLifecycleError(
                    process.pid,
                    f"process group {process.pid} kill failed: "
                    f"{type(error).__name__}: {error}",
                ) from None
        try:
            process.wait(timeout=2.0)
        except subprocess.TimeoutExpired:
            raise ProcessLifecycleError(process.pid) from None
        except (OSError, subprocess.SubprocessError) as error:
            raise ProcessLifecycleError(
                process.pid,
                f"process {process.pid} reap after forced kill failed: "
                f"{type(error).__name__}: {error}",
            ) from None
        if os.name != "nt":
            self._quiesce_completed_process_tree(process)
