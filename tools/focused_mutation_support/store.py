from dataclasses import dataclass, fields, is_dataclass
from contextlib import AbstractContextManager, nullcontext
import json
import os
from pathlib import Path
import re
import stat
import sys
import uuid
from collections.abc import Callable
from typing import BinaryIO

from .model import RunRecord
from .disk import DiskFailure, DiskObservation, DiskPolicy, MAX_REPORT_BYTES
from .lease import validate_reported_path


class ReportTooLarge(OSError):
    pass


def _close_descriptor_preserving_primary(
    descriptor: int,
    label: str,
    capability_errors: list[str] | None = None,
) -> None:
    primary_error = sys.exception()
    try:
        os.close(descriptor)
    except OSError as close_error:
        detail = (
            f"{label} close failed: "
            f"{type(close_error).__name__}: {close_error}"
        )
        if capability_errors is not None:
            capability_errors.append(detail)
        if primary_error is None:
            raise OSError(detail) from close_error
        primary_error.add_note(detail)


class BoundedTextWriter:
    def __init__(
        self,
        stream: BinaryIO,
        *,
        capacity: int = MAX_REPORT_BYTES,
        before_chunk: Callable[[int, int], None] | None = None,
    ) -> None:
        self._stream = stream
        self.capacity = capacity
        self.written_bytes = 0
        self._before_chunk = before_chunk

    def writable(self) -> bool:
        return True

    def write(self, value: str) -> int:
        # UTF-8 uses at most four bytes per Unicode scalar.  Slice the input
        # before encoding so one unexpectedly large encoder token cannot
        # allocate a second report-sized temporary in memory.
        for offset in range(0, len(value), 16 * 1024):
            chunk = value[offset : offset + 16 * 1024].encode(
                "utf-8", errors="strict"
            )
            if self.written_bytes + len(chunk) > self.capacity:
                raise ReportTooLarge(
                    f"report exceeds {self.capacity} encoded bytes"
                )
            if self._before_chunk is not None:
                self._before_chunk(self.written_bytes, len(chunk))
            self._stream.write(chunk)
            self.written_bytes += len(chunk)
        return len(value)

    def flush(self) -> None:
        self._stream.flush()


def _json_default(value: object) -> object:
    if is_dataclass(value) and not isinstance(value, type):
        return {
            item.name: getattr(value, item.name)
            for item in fields(value)
            if not item.name.startswith("_")
        }
    raise TypeError(f"cannot encode {type(value).__name__}")


@dataclass(frozen=True)
class CommandPaths:
    stdout: Path
    stderr: Path
    root_fd: int
    stdout_name: str
    stderr_name: str
    capability_errors: list[str]

    def _name(self, stream_name: str) -> str:
        if stream_name == "stdout":
            return self.stdout_name
        if stream_name == "stderr":
            return self.stderr_name
        raise ValueError(f"unknown command stream: {stream_name!r}")

    def open_writer(self, stream_name: str) -> BinaryIO:
        name = self._name(stream_name)
        descriptor = os.open(
            name,
            os.O_WRONLY
            | os.O_CREAT
            | os.O_EXCL
            | getattr(os, "O_NOFOLLOW", 0),
            0o600,
            dir_fd=self.root_fd,
        )
        try:
            metadata = os.fstat(descriptor)
            if not stat.S_ISREG(metadata.st_mode):
                raise OSError(f"command spool is not a regular file: {name}")
            return os.fdopen(descriptor, "wb", closefd=True)
        except BaseException:
            _close_descriptor_preserving_primary(
                descriptor,
                "command spool writer",
                self.capability_errors,
            )
            raise

    def available_bytes(self) -> int:
        values = os.fstatvfs(self.root_fd)
        return values.f_bavail * values.f_frsize

    def read(self, stream_name: str, capacity: int, *, tail: bool = False) -> tuple[bytes, int]:
        if capacity < 0:
            raise ValueError("command spool capacity cannot be negative")
        name = self._name(stream_name)
        descriptor = os.open(
            name,
            os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0),
            dir_fd=self.root_fd,
        )
        try:
            metadata = os.fstat(descriptor)
            if not stat.S_ISREG(metadata.st_mode):
                raise OSError(f"command spool is not a regular file: {name}")
            observed = metadata.st_size
            if not tail and observed > capacity:
                raise ValueError(
                    f"bounded command spool exceeds {capacity} bytes: {name}"
                )
            if tail:
                os.lseek(descriptor, max(0, observed - capacity), os.SEEK_SET)
                limit = min(observed, capacity)
            else:
                limit = capacity + 1
            value = bytearray()
            while len(value) < limit:
                chunk = os.read(descriptor, min(64 * 1024, limit - len(value)))
                if not chunk:
                    break
                value.extend(chunk)
            if not tail and len(value) > capacity:
                raise ValueError(
                    f"bounded command spool exceeds {capacity} bytes: {name}"
                )
            return bytes(value), observed
        finally:
            _close_descriptor_preserving_primary(
                descriptor,
                "command spool reader",
                self.capability_errors,
            )

    def discard(self) -> tuple[str, ...]:
        errors: list[str] = []
        for name in (self.stdout_name, self.stderr_name):
            try:
                os.unlink(name, dir_fd=self.root_fd)
            except FileNotFoundError:
                pass
            except OSError as error:
                errors.append(
                    "command spool cleanup failed: "
                    f"{type(error).__name__}: {error}"
                )
            try:
                os.stat(name, dir_fd=self.root_fd, follow_symlinks=False)
            except FileNotFoundError:
                pass
            except OSError as error:
                errors.append(
                    "command spool absence verification failed: "
                    f"{type(error).__name__}: {error}"
                )
            else:
                errors.append(f"command spool cleanup failed: path remains: {name}")
        return tuple(errors)


OUTPUT_OWNER_FILE = ".hoimin-output-owner"
_OUTPUT_MARKER_FIELDS = (
    "output_device",
    "output_inode",
    "owner_kind",
    "run_id",
    "schema_version",
)


class OwnedOutput:
    def __init__(
        self,
        path: Path,
        run_id: str,
        directory_fd: int,
        marker_fd: int,
        *,
        recovered_temporary_count: int = 0,
    ) -> None:
        self.path = path
        self.run_id = run_id
        self._directory_fd = directory_fd
        self._marker_fd = marker_fd
        metadata = os.fstat(directory_fd)
        self._identity = (metadata.st_dev, metadata.st_ino)
        self.recovered_temporary_count = recovered_temporary_count
        self.close_errors: list[str] = []

    @classmethod
    def create(
        cls,
        path: Path,
        run_id: str,
        *,
        min_free_bytes: int | None = None,
    ) -> "OwnedOutput":
        raw_path = os.fspath(path)
        try:
            encoded_path = raw_path.encode("utf-8", errors="strict")
            escaped_path = json.dumps(
                raw_path, ensure_ascii=True
            ).encode("utf-8")
        except UnicodeError as error:
            raise ValueError("output path is not strict UTF-8") from error
        if max(len(encoded_path), len(escaped_path)) > 16 * 1024:
            raise ValueError("output path exceeds 16 KiB encoded bytes")
        try:
            path.mkdir(mode=0o700, parents=True)
        except FileExistsError:
            pass
        flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        flags |= getattr(os, "O_NOFOLLOW", 0)
        directory_fd = os.open(path, flags)
        marker_fd = -1
        try:
            metadata = os.fstat(directory_fd)
            if not stat.S_ISDIR(metadata.st_mode):
                raise ValueError("output path is not a real directory")
            recovered = _recover_abandoned_output(directory_fd)
            if _bounded_output_names(directory_fd):
                raise ValueError("output path must be empty before startup")
            if min_free_bytes is not None:
                capacity = os.fstatvfs(directory_fd)
                available = capacity.f_bavail * capacity.f_frsize
                if available <= min_free_bytes:
                    raise ValueError(
                        "filesystem reserve reached before output ownership"
                    )
            marker_fd = os.open(
                OUTPUT_OWNER_FILE,
                os.O_RDWR | os.O_CREAT | os.O_EXCL,
                0o600,
                dir_fd=directory_fd,
            )
            encoded = (
                json.dumps(
                    {
                        "schema_version": 1,
                        "run_id": run_id,
                        "owner_kind": "focused_python",
                        "output_device": metadata.st_dev,
                        "output_inode": metadata.st_ino,
                    },
                    sort_keys=True,
                )
                + "\n"
            ).encode("utf-8")
            offset = 0
            while offset < len(encoded):
                written = os.write(marker_fd, encoded[offset:])
                if written <= 0:
                    raise OSError("output owner marker write made no progress")
                offset += written
            if os.fstat(marker_fd).st_size != len(encoded):
                raise OSError("output owner marker write was incomplete")
            os.fsync(marker_fd)
            if os.name == "nt":
                import msvcrt

                os.lseek(marker_fd, 0, os.SEEK_SET)
                msvcrt.locking(marker_fd, msvcrt.LK_NBLCK, 1)  # type: ignore[attr-defined]
            else:
                import fcntl

                fcntl.flock(marker_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return cls(
                path.resolve(strict=True),
                run_id,
                directory_fd,
                marker_fd,
                recovered_temporary_count=recovered,
            )
        except BaseException as primary_error:
            rollback_errors: list[str] = []
            if marker_fd >= 0:
                try:
                    os.close(marker_fd)
                except OSError as error:
                    rollback_errors.append(
                        "output marker rollback close failed: "
                        f"{type(error).__name__}: {error}"
                    )
                try:
                    os.unlink(OUTPUT_OWNER_FILE, dir_fd=directory_fd)
                except OSError as error:
                    rollback_errors.append(
                        "output marker rollback unlink failed: "
                        f"{type(error).__name__}: {error}"
                    )
            try:
                os.close(directory_fd)
            except OSError as error:
                rollback_errors.append(
                    "output directory rollback close failed: "
                    f"{type(error).__name__}: {error}"
                )
            for rollback_error in rollback_errors:
                primary_error.add_note(rollback_error)
            raise

    def _verify(self) -> None:
        current = os.stat(self.path, follow_symlinks=False)
        opened = os.fstat(self._directory_fd)
        if not stat.S_ISDIR(current.st_mode) or (
            current.st_dev,
            current.st_ino,
        ) != self._identity or (opened.st_dev, opened.st_ino) != self._identity:
            raise OSError("output directory identity changed")

    def available_bytes(self) -> int:
        self._verify()
        value = os.fstatvfs(self._directory_fd)
        return value.f_bavail * value.f_frsize

    def post_flush_available_bytes(self) -> int:
        return self.available_bytes()

    def probe_close(self) -> tuple[str, ...]:
        errors: list[str] = []
        for label, descriptor in (
            ("output marker", self._marker_fd),
            ("output directory", self._directory_fd),
        ):
            if descriptor < 0:
                continue
            duplicate = -1
            try:
                duplicate = os.dup(descriptor)
                os.close(duplicate)
                duplicate = -1
            except OSError as error:
                errors.append(
                    f"{label} close probe failed: {type(error).__name__}: {error}"
                )
            finally:
                if duplicate >= 0:
                    try:
                        os.close(duplicate)
                    except OSError:
                        pass
        return tuple(errors)

    def write_atomic(
        self,
        kind: str,
        write: Callable[[BoundedTextWriter], None],
        *,
        capacity: int = MAX_REPORT_BYTES,
        before_chunk: Callable[[int, int], None] | None = None,
        after_flush: Callable[[int], None] | None = None,
    ) -> None:
        self._verify()
        destinations = {"json": "run.json", "markdown": "report.md"}
        try:
            destination = destinations[kind]
        except KeyError as error:
            raise ValueError(f"unsupported report kind: {kind!r}") from error
        temporary = f".hoimin-output-{self.run_id}-{kind}.tmp"
        fd = os.open(
            temporary,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL,
            0o600,
            dir_fd=self._directory_fd,
        )
        try:
            binary_stream = os.fdopen(fd, "wb", closefd=True)
            fd = -1
            with binary_stream as binary:
                stream = BoundedTextWriter(
                    binary,
                    capacity=capacity,
                    before_chunk=before_chunk,
                )
                write(stream)
                stream.flush()
                os.fsync(binary.fileno())
            self._verify()
            if after_flush is not None:
                after_flush(stream.written_bytes)
            self._verify()
            os.replace(
                temporary,
                destination,
                src_dir_fd=self._directory_fd,
                dst_dir_fd=self._directory_fd,
            )
            os.fsync(self._directory_fd)
            self._verify()
        except BaseException as primary_error:
            rollback_errors: list[str] = []
            if fd >= 0:
                try:
                    os.close(fd)
                except OSError as error:
                    rollback_errors.append(
                        "report temporary close failed: "
                        f"{type(error).__name__}: {error}"
                    )
            try:
                os.unlink(temporary, dir_fd=self._directory_fd)
            except FileNotFoundError:
                pass
            except OSError as error:
                rollback_errors.append(
                    "report temporary unlink failed: "
                    f"{type(error).__name__}: {error}"
                )
            for rollback_error in rollback_errors:
                primary_error.add_note(rollback_error)
            raise

    def close(self, *, remove_marker: bool = False) -> tuple[str, ...]:
        if self._marker_fd < 0 and self._directory_fd < 0:
            return tuple(self.close_errors)
        self.release_marker(remove_marker=remove_marker)
        self.close_directory()
        return tuple(self.close_errors)

    def release_marker(self, *, remove_marker: bool = False) -> tuple[str, ...]:
        marker_fd = self._marker_fd
        self._marker_fd = -1
        if marker_fd >= 0:
            try:
                if os.name == "nt":
                    import msvcrt

                    os.lseek(marker_fd, 0, os.SEEK_SET)
                    msvcrt.locking(marker_fd, msvcrt.LK_UNLCK, 1)  # type: ignore[attr-defined]
                else:
                    import fcntl

                    fcntl.flock(marker_fd, fcntl.LOCK_UN)
            except OSError as error:
                self.close_errors.append(
                    f"output marker unlock failed: {type(error).__name__}: {error}"
                )
            try:
                os.close(marker_fd)
            except OSError as error:
                self.close_errors.append(
                    f"output marker close failed: {type(error).__name__}: {error}"
                )
        if remove_marker and self._directory_fd >= 0:
            try:
                os.unlink(OUTPUT_OWNER_FILE, dir_fd=self._directory_fd)
            except FileNotFoundError:
                pass
            except OSError as error:
                self.close_errors.append(
                    f"output marker removal failed: {type(error).__name__}: {error}"
                )
        return tuple(self.close_errors)

    def close_directory(self) -> tuple[str, ...]:
        directory_fd = self._directory_fd
        self._directory_fd = -1
        if directory_fd >= 0:
            try:
                os.close(directory_fd)
            except OSError as error:
                self.close_errors.append(
                    f"output directory close failed: {type(error).__name__}: {error}"
                )
        return tuple(self.close_errors)

    def __del__(self) -> None:
        try:
            self.close()
        except OSError:
            pass


def _bounded_output_names(directory_fd: int) -> list[str]:
    names: list[str] = []
    with os.scandir(directory_fd) as iterator:
        for entry in iterator:
            names.append(entry.name)
            if len(names) > 1_000:
                raise ValueError("output directory contains more than 1000 entries")
    return names


def _recover_abandoned_output(directory_fd: int) -> int:
    names = _bounded_output_names(directory_fd)
    if not names:
        return 0
    if OUTPUT_OWNER_FILE not in names:
        return 0
    flags = os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)
    marker_fd = os.open(OUTPUT_OWNER_FILE, flags, dir_fd=directory_fd)
    locked = False
    try:
        metadata = os.fstat(marker_fd)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 64 * 1024:
            return 0
        encoded = os.read(marker_fd, 64 * 1024 + 1)
        if len(encoded) > 64 * 1024:
            return 0
        def exact_output_marker(
            pairs: list[tuple[str, object]],
        ) -> dict[str, object]:
            if tuple(key for key, _value in pairs) != _OUTPUT_MARKER_FIELDS:
                raise ValueError("output marker fields are not canonical")
            return dict(pairs)

        try:
            value = json.loads(
                encoded.decode("utf-8", errors="strict"),
                object_pairs_hook=exact_output_marker,
            )
        except (UnicodeError, ValueError, json.JSONDecodeError):
            return 0
        if (
            not isinstance(value, dict)
            or type(value.get("schema_version")) is not int
            or value.get("schema_version") != 1
            or value.get("owner_kind") != "focused_python"
            or not isinstance(value.get("run_id"), str)
            or type(value.get("output_device")) is not int
            or type(value.get("output_inode")) is not int
        ):
            return 0
        output_metadata = os.fstat(directory_fd)
        if (
            value.get("output_device") != output_metadata.st_dev
            or value.get("output_inode") != output_metadata.st_ino
        ):
            return 0
        if os.name == "nt":
            import msvcrt

            os.lseek(marker_fd, 0, os.SEEK_SET)
            try:
                msvcrt.locking(marker_fd, msvcrt.LK_NBLCK, 1)  # type: ignore[attr-defined]
            except OSError:
                return 0
        else:
            import fcntl

            try:
                fcntl.flock(marker_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return 0
        locked = True
        run_id = value["run_id"]
        try:
            if str(uuid.UUID(run_id)) != run_id:
                return 0
        except ValueError:
            return 0
        allowed = {
            OUTPUT_OWNER_FILE,
            f".hoimin-output-{run_id}-json.tmp",
            f".hoimin-output-{run_id}-markdown.tmp",
        }
        if any(name not in allowed for name in names):
            return 0
        removed = 0
        for name in sorted(allowed - {OUTPUT_OWNER_FILE}):
            try:
                temporary = os.stat(
                    name,
                    dir_fd=directory_fd,
                    follow_symlinks=False,
                )
            except FileNotFoundError:
                continue
            if not stat.S_ISREG(temporary.st_mode):
                return 0
            os.unlink(name, dir_fd=directory_fd)
            removed += 1
        os.unlink(OUTPUT_OWNER_FILE, dir_fd=directory_fd)
        os.fsync(directory_fd)
        return removed
    finally:
        close_errors: list[OSError] = []
        if locked:
            try:
                if os.name == "nt":
                    import msvcrt

                    os.lseek(marker_fd, 0, os.SEEK_SET)
                    msvcrt.locking(marker_fd, msvcrt.LK_UNLCK, 1)  # type: ignore[attr-defined]
                else:
                    import fcntl

                    fcntl.flock(marker_fd, fcntl.LOCK_UN)
            except OSError as error:
                close_errors.append(error)
        try:
            os.close(marker_fd)
        except OSError as error:
            close_errors.append(error)
        if close_errors:
            active_error = sys.exc_info()[1]
            details = [
                "output recovery capability finalization failed: "
                f"{type(error).__name__}: {error}"
                for error in close_errors
            ]
            if active_error is not None:
                for detail in details:
                    active_error.add_note(detail)
            else:
                first = close_errors[0]
                for detail in details[1:]:
                    first.add_note(detail)
                raise first


class RunStore:
    def __init__(
        self,
        output: Path | OwnedOutput,
        *,
        command_root: Path | None = None,
    ) -> None:
        self.owned_output = output if isinstance(output, OwnedOutput) else None
        self.output = output.path if isinstance(output, OwnedOutput) else output
        self.commands = (
            self.output / "commands" if command_root is None else command_root
        )
        self._command_root_fd = -1
        self._command_capability_errors: list[str] = []
        if command_root is not None:
            self._ensure_command_root()
        self._report_policy: DiskPolicy | None = None
        self._report_sample: Callable[[], tuple[DiskFailure | None, DiskObservation | None]] | None = None
        self._report_freeze: (
            Callable[[], AbstractContextManager[Callable[[], bool]]] | None
        ) = None
        self._report_cancelled: Callable[[], bool] | None = None

    def configure_report_guard(
        self,
        policy: DiskPolicy,
        sample: Callable[[], tuple[DiskFailure | None, DiskObservation | None]],
        freeze: Callable[
            [], AbstractContextManager[Callable[[], bool]]
        ] | None = None,
        cancelled: Callable[[], bool] | None = None,
    ) -> None:
        self._report_policy = policy
        self._report_sample = sample
        self._report_freeze = freeze
        self._report_cancelled = cancelled

    def _owned_write(
        self,
        kind: str,
        write: Callable[[BoundedTextWriter], None],
    ) -> None:
        if self.owned_output is None:
            raise RuntimeError("owned write requires an owned output")
        policy = self._report_policy
        sample = self._report_sample
        owned_output = self.owned_output
        if policy is None or sample is None:
            raise RuntimeError("owned output report guard is not configured")
        boundary = (
            self._report_freeze()
            if self._report_freeze is not None
            else nullcontext(lambda: True)
        )
        with boundary as generation_is_current:
            if self._report_cancelled is not None and self._report_cancelled():
                raise ReportTooLarge("report write cancelled before boundary sample")
            failure, observation = sample()
            if failure is not None or observation is None:
                code = "disk.measurement.failed" if failure is None else failure.code
                raise ReportTooLarge(f"report boundary rejected by {code}")
            base_owned = observation.owned_bytes

            def before_chunk(written: int, next_length: int) -> None:
                if self._report_cancelled is not None and self._report_cancelled():
                    raise ReportTooLarge("report write cancelled while streaming")
                if base_owned + written + next_length >= policy.max_disk_bytes:
                    raise ReportTooLarge("report chunk would reach max disk")
                available = owned_output.available_bytes()
                if available <= policy.min_free_bytes + next_length:
                    raise ReportTooLarge("report chunk would cross filesystem reserve")

            def after_flush(written: int) -> None:
                if self._report_cancelled is not None and self._report_cancelled():
                    raise ReportTooLarge("report write cancelled before replacement")
                available = owned_output.post_flush_available_bytes()
                if available <= policy.min_free_bytes:
                    raise ReportTooLarge(
                        "post-flush report boundary crossed filesystem reserve"
                    )
                if not generation_is_current():
                    raise ReportTooLarge(
                        "report scratch registry generation changed"
                    )

            owned_output.write_atomic(
                kind,
                write,
                capacity=policy.max_report_bytes,
                before_chunk=before_chunk,
                after_flush=after_flush,
            )

    def initialize(self, record: RunRecord) -> None:
        if self.output.name.startswith("mutants.out"):
            raise ValueError("output path must not use the mutants.out prefix")
        if self.output.exists() and not self.output.is_dir():
            raise ValueError("output path exists and is not a directory")
        self.commands.mkdir(parents=True, exist_ok=True)
        self.checkpoint(record)

    def checkpoint(self, record: RunRecord) -> None:
        if self.owned_output is not None:
            encoder = json.JSONEncoder(
                sort_keys=True,
                indent=2,
                default=_json_default,
            )

            def write(stream: BoundedTextWriter) -> None:
                for chunk in encoder.iterencode(record):
                    stream.write(chunk)
                stream.write("\n")

            self._owned_write("json", write)
            return
        temporary = self.output / ".run.json.tmp"
        destination = self.output / "run.json"
        with temporary.open("xb") as binary:
            stream = BoundedTextWriter(binary)
            encoder = json.JSONEncoder(
                sort_keys=True,
                indent=2,
                default=_json_default,
            )
            for chunk in encoder.iterencode(record):
                stream.write(chunk)
            stream.write("\n")
            stream.flush()
            os.fsync(binary.fileno())
        temporary.replace(destination)

    def write_markdown(self, record: RunRecord) -> None:
        from .reporting import write_markdown

        if self.owned_output is not None:
            self._owned_write(
                "markdown",
                lambda stream: write_markdown(record, stream),
            )
            return
        temporary = self.output / ".report.md.tmp"
        destination = self.output / "report.md"
        with temporary.open("xb") as binary:
            stream = BoundedTextWriter(binary)
            write_markdown(record, stream)
            stream.flush()
            os.fsync(binary.fileno())
        temporary.replace(destination)

    def command_paths(self, sequence: int, label: str) -> CommandPaths:
        root_fd = self._ensure_command_root()
        safe_label = re.sub(r"[^A-Za-z0-9_.-]+", "-", label).strip(".-") or "command"
        stem = f"{sequence:04d}-{safe_label}"
        stdout_name = f"{stem}.stdout"
        stderr_name = f"{stem}.stderr"
        stdout = self.commands / stdout_name
        stderr = self.commands / stderr_name
        validate_reported_path(stdout)
        validate_reported_path(stderr)
        return CommandPaths(
            stdout=stdout,
            stderr=stderr,
            root_fd=root_fd,
            stdout_name=stdout_name,
            stderr_name=stderr_name,
            capability_errors=self._command_capability_errors,
        )

    def _ensure_command_root(self) -> int:
        if self._command_root_fd >= 0:
            return self._command_root_fd
        flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        flags |= getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(self.commands, flags)
        try:
            opened = os.fstat(descriptor)
            current = os.stat(self.commands, follow_symlinks=False)
            if not stat.S_ISDIR(opened.st_mode) or (
                opened.st_dev,
                opened.st_ino,
            ) != (current.st_dev, current.st_ino):
                raise OSError("command spool directory identity changed")
        except BaseException:
            os.close(descriptor)
            raise
        self._command_root_fd = descriptor
        return descriptor

    def close_command_root(self) -> tuple[str, ...]:
        errors = list(self._command_capability_errors)
        self._command_capability_errors.clear()
        descriptor = self._command_root_fd
        self._command_root_fd = -1
        if descriptor < 0:
            return tuple(errors)
        try:
            os.close(descriptor)
        except OSError as error:
            errors.append(
                "command spool directory close failed: "
                f"{type(error).__name__}: {error}"
            )
        return tuple(errors)

    def __del__(self) -> None:
        try:
            self.close_command_root()
        except BaseException:
            pass
