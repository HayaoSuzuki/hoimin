from dataclasses import dataclass, field, fields, is_dataclass
from contextlib import AbstractContextManager, nullcontext
import json
import os
from pathlib import Path
import re
import sys
import threading
import uuid
from collections.abc import Callable
from typing import BinaryIO, cast

from .model import RunRecord
from .disk import (
    DiskFailure,
    DiskObservation,
    DiskPolicy,
    MAX_DIAGNOSTIC_DETAIL_BYTES,
    MAX_DIAGNOSTIC_DETAILS,
    MAX_REPORT_BYTES,
)
from .filesystem import (
    CreateDisposition,
    DirectoryCapability,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemBackend,
    SecurityDomain,
    SharePolicy,
    default_filesystem_backend,
)
from .lease import (
    LeaseLock,
    _OwnedDescriptor,
    _close_capability_once,
    _close_capability_retry,
    _close_lease_lock_all,
    _close_lease_lock_retry,
    validate_reported_path,
)


class ReportTooLarge(OSError):
    pass


MAX_OUTPUT_MARKER_BYTES = 64 * 1024


def _bounded_detail(value: str) -> str:
    encoded = value.encode("utf-8", errors="replace")
    if len(encoded) <= MAX_DIAGNOSTIC_DETAIL_BYTES:
        return value
    suffix = b"...[truncated]"
    return (encoded[: MAX_DIAGNOSTIC_DETAIL_BYTES - len(suffix)] + suffix).decode(
        "utf-8", errors="ignore"
    )


def _bounded_capability_detail(label: str, error: BaseException) -> str:
    return _bounded_detail(f"{label}: {type(error).__name__}: {error}")


def _append_capability_error(errors: list[str], detail: str) -> None:
    detail = _bounded_detail(detail)
    if detail in errors or len(errors) >= MAX_DIAGNOSTIC_DETAILS:
        return
    errors.append(detail)


def _attach_secondary(primary: BaseException, details: tuple[str, ...]) -> None:
    existing = tuple(getattr(primary, "__notes__", ()))
    for detail in details:
        if detail not in existing:
            primary.add_note(detail)
            existing = (*existing, detail)


class _OwnedBinaryStream:
    """A fixed-slot BinaryIO owner with two explicit close attempts."""

    def __init__(self) -> None:
        self.stream: BinaryIO | None = None
        self.close_attempts = 0
        self._close_detail_0: str | None = None
        self._close_detail_1: str | None = None
        self._close_detail_count = 0
        self._finalizer_attempted = False

    @property
    def close_errors(self) -> tuple[str, ...]:
        if self._close_detail_count == 0:
            return ()
        if self._close_detail_count == 1:
            assert self._close_detail_0 is not None
            return (self._close_detail_0,)
        assert self._close_detail_0 is not None
        assert self._close_detail_1 is not None
        return (self._close_detail_0, self._close_detail_1)

    @property
    def is_live(self) -> bool:
        return self.stream is not None and not self.stream.closed

    @property
    def closed(self) -> bool:
        return not self.is_live

    def _record_close_error(self, detail: str) -> None:
        detail = _bounded_detail(detail)
        if detail in self.close_errors:
            return
        if self._close_detail_count == 0:
            self._close_detail_0 = detail
            self._close_detail_count = 1
        elif self._close_detail_count == 1:
            self._close_detail_1 = detail
            self._close_detail_count = 2

    def adopt(self, stream: BinaryIO) -> None:
        if self.stream is not None:
            raise RuntimeError("binary stream owner is already occupied")
        self.stream = stream

    def write(self, value: bytes) -> int:
        stream = self.stream
        if stream is None:
            raise ValueError("I/O operation on closed command spool")
        return stream.write(value)

    def flush(self) -> None:
        stream = self.stream
        if stream is None:
            raise ValueError("I/O operation on closed command spool")
        stream.flush()

    def fileno(self) -> int:
        stream = self.stream
        if stream is None:
            raise ValueError("I/O operation on closed command spool")
        return stream.fileno()

    def writable(self) -> bool:
        return self.is_live

    def close(self) -> None:
        self.close_once("command spool stream")
        if self.is_live:
            detail = self.close_errors[-1] if self.close_errors else "stream close failed"
            raise OSError(detail)

    def __enter__(self) -> "_OwnedBinaryStream":
        if not self.is_live:
            raise ValueError("I/O operation on closed command spool")
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()

    def close_once(self, label: str) -> tuple[str, ...]:
        stream = self.stream
        if stream is None or stream.closed:
            self.stream = None
            return self.close_errors
        if self.close_attempts >= 2:
            return self.close_errors
        self.close_attempts += 1
        try:
            stream.close()
        except BaseException as error:
            self._record_close_error(
                _bounded_capability_detail(f"{label} close failed", error)
            )
        if stream.closed:
            self.stream = None
        return self.close_errors

    def close_retry(self, label: str) -> tuple[str, ...]:
        while self.is_live and self.close_attempts < 2:
            self.close_once(label)
        return self.close_errors

    def close_finalizer_once(self, label: str) -> tuple[str, ...]:
        stream = self.stream
        if stream is None or stream.closed:
            self.stream = None
            return self.close_errors
        if self._finalizer_attempted:
            return self.close_errors
        self._finalizer_attempted = True
        try:
            stream.close()
        except BaseException as error:
            self._record_close_error(
                _bounded_capability_detail(f"{label} final close failed", error)
            )
        if stream.closed:
            self.stream = None
        return self.close_errors

    def __del__(self) -> None:
        self.close_finalizer_once("binary stream")


class _OwnedTaskDescriptor:
    """Task 10 descriptor owner with a separate one-shot final close."""

    def __init__(self) -> None:
        self.fd = -1
        self.close_attempts = 0
        self._close_detail_0: str | None = None
        self._close_detail_1: str | None = None
        self._close_detail_count = 0
        self._finalizer_attempted = False

    @property
    def close_errors(self) -> tuple[str, ...]:
        if self._close_detail_count == 0:
            return ()
        if self._close_detail_count == 1:
            assert self._close_detail_0 is not None
            return (self._close_detail_0,)
        assert self._close_detail_0 is not None
        assert self._close_detail_1 is not None
        return (self._close_detail_0, self._close_detail_1)

    def _record_close_error(self, detail: str) -> None:
        detail = _bounded_detail(detail)
        if detail in self.close_errors:
            return
        if self._close_detail_count == 0:
            self._close_detail_0 = detail
            self._close_detail_count = 1
        elif self._close_detail_count == 1:
            self._close_detail_1 = detail
            self._close_detail_count = 2

    def adopt(self, descriptor: int) -> None:
        if self.fd >= 0 or descriptor < 0:
            raise RuntimeError("descriptor owner cannot adopt this descriptor")
        self.fd = descriptor
        self.close_attempts = 0
        self._close_detail_0 = None
        self._close_detail_1 = None
        self._close_detail_count = 0
        self._finalizer_attempted = False

    def detach(self) -> int:
        if self.fd < 0:
            raise RuntimeError("descriptor owner is empty")
        descriptor = self.fd
        self.fd = -1
        return descriptor

    def close_retry(self, label: str) -> tuple[str, ...]:
        while self.fd >= 0 and self.close_attempts < 2:
            descriptor = self.fd
            self.close_attempts += 1
            try:
                os.close(descriptor)
            except BaseException as error:
                self._record_close_error(
                    _bounded_capability_detail(f"{label} close failed", error)
                )
            else:
                self.fd = -1
        return self.close_errors

    def close_finalizer_once(self, label: str) -> tuple[str, ...]:
        if self.fd < 0 or self._finalizer_attempted:
            return self.close_errors
        self._finalizer_attempted = True
        descriptor = self.fd
        try:
            os.close(descriptor)
        except BaseException as error:
            self._record_close_error(
                _bounded_capability_detail(f"{label} final close failed", error)
            )
        else:
            self.fd = -1
        return self.close_errors

    def __del__(self) -> None:
        self.close_finalizer_once("descriptor")


class _OwnerOperationState:
    __slots__ = ("lock", "epoch")

    def __init__(self) -> None:
        self.lock = threading.RLock()
        self.epoch = 0

    def begin(self) -> int:
        self.epoch += 1
        return self.epoch


class _FileOwnerState:
    """One exact file identity with exactly one live ownership layer."""

    def __init__(self, name: str, operation_epoch: int) -> None:
        self.name = name
        self.identity: FileIdentity | None = None
        self.capability: FileCapability | DirectoryCapability | None = None
        self.descriptor = _OwnedTaskDescriptor()
        self.stream = _OwnedBinaryStream()
        self.renamed = False
        self.delete_pending = False
        self._layer_epoch = operation_epoch
        self._resume_attempted = False

    def begin_layer(self, operation_epoch: int) -> None:
        self._layer_epoch = operation_epoch
        self._resume_attempted = False

    @property
    def is_live(self) -> bool:
        return (
            (self.capability is not None and self.capability.is_open)
            or self.descriptor.fd >= 0
            or self.stream.is_live
        )

    @property
    def cleanup_pending(self) -> bool:
        return self.is_live or self.delete_pending

    def close_for_operation(
        self, label: str, operation_epoch: int
    ) -> tuple[str, ...]:
        details: list[str] = []
        if not self.is_live:
            return ()
        saturated = False
        if self.stream.is_live:
            saturated = self.stream.close_attempts >= 2
        elif self.descriptor.fd >= 0:
            saturated = self.descriptor.close_attempts >= 2
        else:
            capability = self.capability
            saturated = (
                capability is not None and capability._close_attempts >= 2
            )
        later_operation = operation_epoch > self._layer_epoch
        if saturated and later_operation:
            self._layer_epoch = operation_epoch
            if self._resume_attempted:
                return ()
            self._resume_attempted = True
            if self.stream.is_live:
                details.extend(self.stream.close_finalizer_once(label))
            elif self.descriptor.fd >= 0:
                details.extend(self.descriptor.close_finalizer_once(label))
            else:
                capability = self.capability
                if capability is not None and capability.is_open:
                    try:
                        capability.close()
                    except BaseException as error:
                        details.append(
                            _bounded_capability_detail(
                                f"{label} final close failed", error
                            )
                        )
            return tuple(dict.fromkeys(details))
        if self.stream.is_live:
            details.extend(self.stream.close_retry(label))
        if self.descriptor.fd >= 0:
            details.extend(self.descriptor.close_retry(label))
        capability = self.capability
        if capability is not None and capability.is_open:
            details.extend(_close_capability_retry(capability, label))
        self._layer_epoch = operation_epoch
        return tuple(dict.fromkeys(_bounded_detail(detail) for detail in details))


def _settle_owned_exact_child(
    parent: DirectoryCapability,
    owner: _FileOwnerState,
    backend: FilesystemBackend,
    label: str,
    operation_epoch: int,
) -> tuple[tuple[str, ...], bool]:
    """Close, delete, and prove absence without abandoning a live owner."""

    errors = list(owner.close_for_operation(label, operation_epoch))
    if owner.is_live:
        return tuple(dict.fromkeys(errors)), False
    identity = owner.identity
    if identity is None:
        return tuple(dict.fromkeys(errors)), True
    try:
        evidence = backend.entry(parent, owner.name)
    except BaseException as error:
        errors.append(_bounded_capability_detail(f"{label} lookup failed", error))
        return tuple(dict.fromkeys(errors)), False
    if evidence is None:
        return tuple(dict.fromkeys(errors)), True
    if (
        evidence.kind is not EntryKind.REGULAR
        or evidence.identity != identity
        or evidence.filesystem != parent.filesystem
    ):
        errors.append(
            _bounded_detail(
                f"{label} refused replacement: {owner.name}"
            )
        )
        return tuple(dict.fromkeys(errors)), False
    try:
        capability = backend.open_entry(
            parent, owner.name, SharePolicy.PINNED
        )
        owner.capability = capability
        owner.begin_layer(operation_epoch)
        if (
            capability.kind is not EntryKind.REGULAR
            or capability.identity != identity
            or capability.filesystem != parent.filesystem
        ):
            raise OSError(
                f"{label} identity changed before delete: {owner.name}"
            )
        owner.delete_pending = True
        backend.delete(capability)
    except BaseException as error:
        errors.append(_bounded_capability_detail(f"{label} failed", error))
    errors.extend(owner.close_for_operation(label, operation_epoch))
    if owner.is_live:
        return tuple(dict.fromkeys(errors)), False
    try:
        remaining = backend.entry(parent, owner.name)
    except BaseException as error:
        errors.append(
            _bounded_capability_detail(
                f"{label} absence verification failed", error
            )
        )
        return tuple(dict.fromkeys(errors)), False
    if remaining is not None:
        errors.append(
            _bounded_detail(
                f"{label} absence verification failed: {owner.name} remains"
            )
        )
        return tuple(dict.fromkeys(errors)), False
    return tuple(dict.fromkeys(errors)), True


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
    root: DirectoryCapability
    stdout_name: str
    stderr_name: str
    backend: FilesystemBackend
    _root_token: int
    _release_root: Callable[[int], None] = field(repr=False, compare=False)
    _spool_identities: dict[str, FileIdentity] = field(
        default_factory=dict, repr=False, compare=False
    )
    _spool_owners: dict[str, _FileOwnerState] = field(
        default_factory=dict, repr=False, compare=False
    )
    _operation_state: _OwnerOperationState = field(
        default_factory=_OwnerOperationState, repr=False, compare=False
    )
    capability_errors: list[str] = field(default_factory=list)

    def _name(self, stream_name: str) -> str:
        if stream_name == "stdout":
            return self.stdout_name
        if stream_name == "stderr":
            return self.stderr_name
        raise ValueError(f"unknown command stream: {stream_name!r}")

    def open_writer(self, stream_name: str) -> BinaryIO:
        with self._operation_state.lock:
            operation_epoch = self._operation_state.begin()
            return self._open_writer_locked(stream_name, operation_epoch)

    def _open_writer_locked(
        self, stream_name: str, operation_epoch: int
    ) -> BinaryIO:
        name = self._name(stream_name)
        if name in self._spool_identities:
            raise RuntimeError(f"command spool identity already exists: {name}")
        if len(self._spool_identities) >= 2:
            raise RuntimeError("command spool identity registry is full")
        owner = _FileOwnerState(name, operation_epoch)
        self._spool_owners[name] = owner
        try:
            capability = self.backend.open_file(
                self.root,
                name,
                access=FileAccess.WRITE,
                disposition=CreateDisposition.CREATE_NEW,
                share_policy=SharePolicy.PINNED,
            )
            owner.capability = capability
            owner.identity = capability.identity
            self._spool_identities[name] = capability.identity
            if capability.kind is not EntryKind.REGULAR:
                raise OSError(f"command spool is not a regular file: {name}")
            owner.descriptor.adopt(
                capability.detach_to_fd(
                    os.O_WRONLY | getattr(os, "O_BINARY", 0)
                )
            )
            owner.capability = None
            owner.begin_layer(operation_epoch)
            stream = os.fdopen(owner.descriptor.fd, "wb", closefd=True)
            owner.stream.adopt(stream)
            owner.descriptor.detach()
            owner.begin_layer(operation_epoch)
            return cast(BinaryIO, owner.stream)
        except BaseException as primary_error:
            details = list(
                owner.close_for_operation("command spool writer", operation_epoch)
            )
            if owner.identity is not None and not owner.is_live:
                discard_details, complete = self._discard_one(
                    owner, operation_epoch
                )
                details.extend(discard_details)
                if complete:
                    self._spool_identities.pop(name, None)
                    self._spool_owners.pop(name, None)
            elif owner.identity is None and not owner.is_live:
                self._spool_owners.pop(name, None)
            for detail in details:
                _append_capability_error(self.capability_errors, detail)
            _attach_secondary(primary_error, tuple(details))
            raise

    def available_bytes(self) -> int:
        return self.backend.available_bytes(self.root)

    def read(self, stream_name: str, capacity: int, *, tail: bool = False) -> tuple[bytes, int]:
        if capacity < 0:
            raise ValueError("command spool capacity cannot be negative")
        name = self._name(stream_name)
        identity = self._spool_identities.get(name)
        if identity is None:
            raise OSError(f"command spool has no recorded identity: {name}")
        capability: FileCapability | None = None
        descriptor = _OwnedDescriptor()
        try:
            evidence = self.backend.entry(self.root, name)
            if (
                evidence is None
                or evidence.kind is not EntryKind.REGULAR
                or evidence.identity != identity
                or evidence.filesystem != self.root.filesystem
            ):
                raise OSError(f"command spool identity changed: {name}")
            capability = self.backend.open_file(
                self.root,
                name,
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.PINNED,
            )
            if capability.kind is not EntryKind.REGULAR:
                raise OSError(f"command spool is not a regular file: {name}")
            if capability.identity != identity:
                raise OSError(f"command spool identity changed: {name}")
            observed = capability.logical_size
            descriptor.adopt(
                capability.detach_to_fd(
                    os.O_RDONLY | getattr(os, "O_BINARY", 0)
                )
            )
            if not tail and observed > capacity:
                raise ValueError(
                    f"bounded command spool exceeds {capacity} bytes: {name}"
                )
            if tail:
                os.lseek(descriptor.fd, max(0, observed - capacity), os.SEEK_SET)
                limit = min(observed, capacity)
            else:
                limit = capacity + 1
            value = bytearray()
            while len(value) < limit:
                chunk = os.read(
                    descriptor.fd, min(64 * 1024, limit - len(value))
                )
                if not chunk:
                    break
                value.extend(chunk)
            if not tail and len(value) > capacity:
                raise ValueError(
                    f"bounded command spool exceeds {capacity} bytes: {name}"
                )
            return bytes(value), observed
        except BaseException as primary_error:
            details: list[str] = []
            if capability is not None and capability.is_open:
                details.extend(_close_capability_retry(capability, "command spool reader"))
            details.extend(descriptor.close_retry("command spool reader"))
            for detail in details:
                _append_capability_error(self.capability_errors, detail)
            _attach_secondary(primary_error, tuple(details))
            raise
        finally:
            if sys.exception() is None:
                close_details = descriptor.close_retry("command spool reader")
                for detail in close_details:
                    _append_capability_error(self.capability_errors, detail)
                if descriptor.fd >= 0:
                    raise OSError("command spool reader close failed")

    def _discard_one(
        self,
        owner: _FileOwnerState,
        operation_epoch: int,
    ) -> tuple[tuple[str, ...], bool]:
        return _settle_owned_exact_child(
            self.root,
            owner,
            self.backend,
            "command spool cleanup",
            operation_epoch,
        )

    def discard(self) -> tuple[str, ...]:
        with self._operation_state.lock:
            operation_epoch = self._operation_state.begin()
            return self._discard_locked(operation_epoch)

    def _discard_locked(self, operation_epoch: int) -> tuple[str, ...]:
        errors: list[str] = []
        try:
            for name in (self.stdout_name, self.stderr_name):
                owner = self._spool_owners.get(name)
                if owner is not None:
                    errors.extend(
                        owner.close_for_operation(
                            "command spool writer", operation_epoch
                        )
                    )
                    if owner.is_live:
                        errors.append(
                            _bounded_detail(
                                f"command spool cleanup blocked by live owner: {name}"
                            )
                        )
                        break
                identity = self._spool_identities.get(name)
                if identity is not None and owner is not None:
                    details, complete = self._discard_one(
                        owner, operation_epoch
                    )
                    errors.extend(details)
                    if complete:
                        self._spool_identities.pop(name, None)
                        self._spool_owners.pop(name, None)
                    else:
                        break
        finally:
            errors.extend(self._close_locked(operation_epoch))
        for detail in errors:
            _append_capability_error(self.capability_errors, detail)
        return tuple(self.capability_errors)

    def close(self) -> tuple[str, ...]:
        with self._operation_state.lock:
            operation_epoch = self._operation_state.begin()
            return self._close_locked(operation_epoch)

    def _close_locked(self, operation_epoch: int) -> tuple[str, ...]:
        live_owner = False
        for name in (self.stdout_name, self.stderr_name):
            owner = self._spool_owners.get(name)
            if owner is None:
                continue
            for detail in owner.close_for_operation(
                "command spool writer", operation_epoch
            ):
                _append_capability_error(self.capability_errors, detail)
            if owner.cleanup_pending:
                live_owner = True
                _append_capability_error(
                    self.capability_errors,
                    f"command spool root retained for pending owner: {name}",
                )
        if live_owner:
            return tuple(self.capability_errors)
        if not self.root.is_open:
            self._release_root(self._root_token)
            return tuple(self.capability_errors)
        details = _close_capability_once(self.root, "command spool root")
        for detail in details:
            _append_capability_error(self.capability_errors, detail)
        if not self.root.is_open:
            self._release_root(self._root_token)
        return tuple(self.capability_errors)

    def __del__(self) -> None:
        try:
            self.close()
        except BaseException:
            pass


OUTPUT_OWNER_FILE = ".hoimin-output-owner"
_OUTPUT_MARKER_FIELDS = (
    "output_device",
    "output_inode",
    "owner_kind",
    "run_id",
    "schema_version",
)


def _delete_exact_child(
    parent: DirectoryCapability,
    name: str,
    identity: FileIdentity,
    backend: FilesystemBackend,
    label: str,
) -> tuple[str, ...]:
    errors: list[str] = []
    capability: FileCapability | DirectoryCapability | None = None
    try:
        evidence = backend.entry(parent, name)
    except BaseException as error:
        return (_bounded_capability_detail(f"{label} lookup failed", error),)
    if evidence is None:
        return ()
    if (
        evidence.kind is not EntryKind.REGULAR
        or evidence.identity != identity
        or evidence.filesystem != parent.filesystem
    ):
        return (
            _bounded_detail(f"{label} refused a same-name replacement: {name}"),
        )
    try:
        capability = backend.open_entry(parent, name, SharePolicy.PINNED)
        if (
            capability.kind is not EntryKind.REGULAR
            or capability.identity != identity
            or capability.filesystem != parent.filesystem
        ):
            raise OSError(f"{label} identity changed before delete: {name}")
        backend.delete(capability)
        if capability.is_open:
            errors.extend(_close_capability_retry(capability, label))
            if capability.is_open:
                return tuple(errors)
        remaining = backend.entry(parent, name)
        if remaining is not None:
            errors.append(
                _bounded_detail(
                    f"{label} absence verification failed: {name} remains"
                )
            )
    except BaseException as error:
        errors.append(_bounded_capability_detail(f"{label} failed", error))
        if capability is not None and capability.is_open:
            errors.extend(_close_capability_retry(capability, label))
        if capability is not None and not capability.is_open:
            try:
                remaining = backend.entry(parent, name)
            except BaseException as absence_error:
                errors.append(
                    _bounded_capability_detail(
                        f"{label} absence verification failed",
                        absence_error,
                    )
                )
            else:
                if remaining is not None:
                    errors.append(
                        _bounded_detail(
                            f"{label} absence verification failed: {name} remains"
                        )
                    )
    return tuple(errors)


class OwnedOutput:
    def __init__(
        self,
        path: Path,
        run_id: str,
        directory: DirectoryCapability,
        marker_lock: LeaseLock,
        marker_identity: FileIdentity,
        backend: FilesystemBackend,
        *,
        recovered_temporary_count: int = 0,
        startup_errors: tuple[str, ...] = (),
    ) -> None:
        self.path = path
        self.run_id = run_id
        self._directory = directory
        self._marker_lock = marker_lock
        self._marker_identity = marker_identity
        self._backend = backend
        self._identity = directory.identity
        self._filesystem = directory.filesystem
        self.recovered_temporary_count = recovered_temporary_count
        self.close_errors: list[str] = []
        for detail in startup_errors:
            _append_capability_error(self.close_errors, detail)
        self._report_owners: dict[str, _FileOwnerState] = {}
        self._operation_state = _OwnerOperationState()

    @classmethod
    def create(
        cls,
        path: Path,
        run_id: str,
        *,
        min_free_bytes: int | None = None,
        backend: FilesystemBackend | None = None,
    ) -> "OwnedOutput":
        try:
            if str(uuid.UUID(run_id)) != run_id:
                raise ValueError
        except (AttributeError, TypeError, ValueError) as error:
            raise ValueError("run_id must be a canonical UUID") from error
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
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
        selected_backend = default_filesystem_backend() if backend is None else backend
        directory: DirectoryCapability | None = None
        marker: FileCapability | None = None
        marker_descriptor = _OwnedDescriptor()
        marker_lock = LeaseLock(-1)
        marker_identity: FileIdentity | None = None
        startup_errors: list[str] = []
        try:
            directory = selected_backend.open_root(
                path, SharePolicy.PINNED, SecurityDomain.CALLER
            )
            if (
                directory.kind is not EntryKind.DIRECTORY
                or directory.security_domain is not SecurityDomain.CALLER
            ):
                raise ValueError("output path is not a real directory")
            recovered = _recover_abandoned_output(
                directory, selected_backend, startup_errors
            )
            if _bounded_output_names(
                directory, selected_backend, startup_errors
            ):
                raise ValueError("output path must be empty before startup")
            if min_free_bytes is not None:
                available = selected_backend.available_bytes(directory)
                if available <= min_free_bytes:
                    raise ValueError(
                        "filesystem reserve reached before output ownership"
                    )
            encoded = (
                json.dumps(
                    {
                        "schema_version": 1,
                        "run_id": run_id,
                        "owner_kind": "focused_python",
                        "output_device": directory.identity.volume,
                        "output_inode": directory.identity.file,
                    },
                    sort_keys=True,
                )
                + "\n"
            ).encode("utf-8")
            if len(encoded) > MAX_OUTPUT_MARKER_BYTES:
                raise ValueError("output owner marker exceeds 64 KiB encoded bytes")
            marker = selected_backend.open_file(
                directory,
                OUTPUT_OWNER_FILE,
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
                share_policy=SharePolicy.PINNED,
            )
            marker_identity = marker.identity
            marker_descriptor.adopt(
                marker.detach_to_fd(os.O_RDWR | getattr(os, "O_BINARY", 0))
            )
            offset = 0
            while offset < len(encoded):
                written = os.write(marker_descriptor.fd, encoded[offset:])
                if written <= 0:
                    raise OSError("output owner marker write made no progress")
                offset += written
            if os.fstat(marker_descriptor.fd).st_size != len(encoded):
                raise OSError("output owner marker write was incomplete")
            os.fsync(marker_descriptor.fd)
            marker_lock._prepare_reuse()
            marker_lock.fd = marker_descriptor.detach()
            marker_lock.acquire(blocking=False)
            return cls(
                path,
                run_id,
                directory,
                marker_lock,
                marker_identity,
                selected_backend,
                recovered_temporary_count=recovered,
                startup_errors=tuple(startup_errors),
            )
        except BaseException as primary_error:
            rollback_errors: list[str] = []
            if marker is not None and marker.is_open:
                rollback_errors.extend(
                    _close_capability_retry(marker, "output marker rollback")
                )
            rollback_errors.extend(
                marker_descriptor.close_retry("output marker rollback")
            )
            if marker_lock.fd >= 0:
                rollback_errors.extend(
                    _close_lease_lock_retry(marker_lock, "output marker rollback")
                )
            if (
                directory is not None
                and directory.is_open
                and marker_identity is not None
                and marker_lock.fd < 0
                and marker_descriptor.fd < 0
                and (marker is None or not marker.is_open)
            ):
                rollback_errors.extend(
                    _delete_exact_child(
                        directory,
                        OUTPUT_OWNER_FILE,
                        marker_identity,
                        selected_backend,
                        "output marker rollback",
                    )
                )
            if directory is not None and directory.is_open:
                rollback_errors.extend(
                    _close_capability_retry(directory, "output directory rollback")
                )
            _attach_secondary(
                primary_error,
                (*tuple(startup_errors), *tuple(rollback_errors)),
            )
            raise

    def _verify_retained_directory(self) -> None:
        if (
            not self._directory.is_open
            or not self._directory.owned_by(self._backend)
            or self._directory.kind is not EntryKind.DIRECTORY
            or self._directory.identity != self._identity
            or self._directory.filesystem != self._filesystem
            or self._directory.security_domain is not SecurityDomain.CALLER
            or self._directory.share_policy is not SharePolicy.PINNED
        ):
            raise OSError("output directory identity changed")

    def _verify(self) -> None:
        self._verify_retained_directory()
        verifier: DirectoryCapability | None = None
        try:
            verifier = self._backend.open_root(
                self.path, SharePolicy.PINNED, SecurityDomain.CALLER
            )
            if (
                verifier.kind is not EntryKind.DIRECTORY
                or verifier.identity != self._identity
                or verifier.filesystem != self._filesystem
                or verifier.security_domain is not SecurityDomain.CALLER
            ):
                raise OSError("output directory identity changed")
        except BaseException as primary_error:
            if verifier is not None and verifier.is_open:
                _attach_secondary(
                    primary_error,
                    _close_capability_retry(verifier, "output verifier"),
                )
            raise
        assert verifier is not None
        verifier_errors = _close_capability_retry(verifier, "output verifier")
        for detail in verifier_errors:
            _append_capability_error(self.close_errors, detail)
        if verifier.is_open:
            raise OSError("; ".join(verifier_errors) or "output verifier close failed")
        marker = self._backend.entry(self._directory, OUTPUT_OWNER_FILE)
        if (
            marker is None
            or marker.kind is not EntryKind.REGULAR
            or marker.identity != self._marker_identity
            or marker.filesystem != self._filesystem
        ):
            raise OSError("output owner marker identity changed")

    def available_bytes(self) -> int:
        self._verify()
        return self._backend.available_bytes(self._directory)

    def reopen_for_meter(self) -> DirectoryCapability:
        self._verify_retained_directory()
        duplicate: DirectoryCapability | None = None
        try:
            duplicate = self._backend.reopen_directory(self._directory)
            if (
                not duplicate.is_open
                or not duplicate.owned_by(self._backend)
                or duplicate.kind is not EntryKind.DIRECTORY
                or duplicate.identity != self._identity
                or duplicate.filesystem != self._filesystem
                or duplicate.security_domain is not SecurityDomain.CALLER
                or duplicate.share_policy is not SharePolicy.PINNED
            ):
                raise OSError("output meter capability identity changed")
            return duplicate
        except BaseException as primary_error:
            if duplicate is not None and duplicate.is_open:
                _attach_secondary(
                    primary_error,
                    _close_capability_retry(duplicate, "output meter duplicate"),
                )
            raise

    def post_flush_available_bytes(self) -> int:
        return self.available_bytes()

    def probe_close(self) -> tuple[str, ...]:
        if not self._directory.is_open:
            return ()
        duplicate: DirectoryCapability | None = None
        try:
            duplicate = self._backend.reopen_directory(self._directory)
            return _close_capability_once(duplicate, "output directory close probe")
        except BaseException as error:
            details = [_bounded_capability_detail("output directory close probe failed", error)]
            if duplicate is not None and duplicate.is_open:
                details.extend(
                    _close_capability_retry(duplicate, "output directory close probe")
                )
            return tuple(details)

    def _settle_report_owner(
        self, kind: str, operation_epoch: int
    ) -> tuple[str, ...]:
        owner = self._report_owners.get(kind)
        if owner is None:
            return ()
        details = list(
            owner.close_for_operation("report temporary", operation_epoch)
        )
        for detail in details:
            _append_capability_error(self.close_errors, detail)
        if owner.is_live:
            return tuple(details)
        identity = owner.identity
        complete = owner.renamed or identity is None
        if (
            not complete
            and self._directory.is_open
            and identity is not None
        ):
            cleanup_details, complete = _settle_owned_exact_child(
                self._directory,
                owner,
                self._backend,
                "report temporary rollback",
                operation_epoch,
            )
            details.extend(cleanup_details)
        if complete:
            self._report_owners.pop(kind, None)
        for detail in details:
            _append_capability_error(self.close_errors, detail)
        return tuple(dict.fromkeys(details))

    def write_atomic(
        self,
        kind: str,
        write: Callable[[BoundedTextWriter], None],
        *,
        capacity: int = MAX_REPORT_BYTES,
        before_chunk: Callable[[int, int], None] | None = None,
        after_flush: Callable[[int], None] | None = None,
    ) -> None:
        with self._operation_state.lock:
            operation_epoch = self._operation_state.begin()
            self._write_atomic_locked(
                kind,
                write,
                operation_epoch=operation_epoch,
                capacity=capacity,
                before_chunk=before_chunk,
                after_flush=after_flush,
            )

    def _write_atomic_locked(
        self,
        kind: str,
        write: Callable[[BoundedTextWriter], None],
        *,
        operation_epoch: int,
        capacity: int,
        before_chunk: Callable[[int, int], None] | None,
        after_flush: Callable[[int], None] | None,
    ) -> None:
        self._verify()
        destinations = {"json": "run.json", "markdown": "report.md"}
        try:
            destination = destinations[kind]
        except KeyError as error:
            raise ValueError(f"unsupported report kind: {kind!r}") from error
        temporary = f".hoimin-output-{self.run_id}-{kind}.tmp"
        pending_details = self._settle_report_owner(kind, operation_epoch)
        pending = self._report_owners.get(kind)
        if pending is not None:
            raise OSError(
                "; ".join(pending_details)
                or f"report temporary owner remains live: {temporary}"
            )
        owner = _FileOwnerState(temporary, operation_epoch)
        self._report_owners[kind] = owner
        try:
            opened = self._backend.open_file(
                self._directory,
                temporary,
                access=FileAccess.WRITE,
                disposition=CreateDisposition.CREATE_NEW,
                share_policy=SharePolicy.PINNED,
            )
            owner.capability = opened
            owner.identity = opened.identity
            if opened.kind is not EntryKind.REGULAR:
                raise OSError("report temporary is not a regular file")
            owner.descriptor.adopt(
                opened.detach_to_fd(os.O_WRONLY | getattr(os, "O_BINARY", 0))
            )
            owner.capability = None
            owner.begin_layer(operation_epoch)
            owner.stream.adopt(
                os.fdopen(owner.descriptor.fd, "wb", closefd=True)
            )
            owner.descriptor.detach()
            owner.begin_layer(operation_epoch)
            binary_stream = owner.stream
            try:
                stream = BoundedTextWriter(
                    cast(BinaryIO, binary_stream),
                    capacity=capacity,
                    before_chunk=before_chunk,
                )
                write(stream)
                stream.flush()
                os.fsync(binary_stream.fileno())
            except BaseException as primary_error:
                _attach_secondary(
                    primary_error,
                    owner.stream.close_retry("report temporary stream"),
                )
                raise
            stream_close_errors = owner.stream.close_retry(
                "report temporary stream"
            )
            if stream_close_errors:
                raise OSError("; ".join(stream_close_errors))
            self._verify()
            if after_flush is not None:
                after_flush(stream.written_bytes)
            self._verify()
            evidence = self._backend.entry(self._directory, temporary)
            if (
                evidence is None
                or evidence.kind is not EntryKind.REGULAR
                or evidence.identity != owner.identity
                or evidence.filesystem != self._filesystem
            ):
                raise OSError("report temporary identity changed after flush")
            capability = self._backend.open_entry(
                self._directory, temporary, SharePolicy.PINNED
            )
            owner.capability = capability
            owner.begin_layer(operation_epoch)
            if (
                capability.kind is not EntryKind.REGULAR
                or capability.identity != owner.identity
                or capability.filesystem != self._filesystem
            ):
                raise OSError("report temporary identity changed before replacement")
            self._backend.rename(
                capability,
                self._directory,
                destination,
                replace=True,
            )
            owner.renamed = True
            owner.name = destination
            close_errors = _close_capability_retry(
                capability, "report temporary after replacement"
            )
            if capability.is_open:
                raise OSError("; ".join(close_errors) or "report source close failed")
            owner.capability = None
            self._verify()
            installed = self._backend.entry(self._directory, destination)
            if (
                installed is None
                or installed.kind is not EntryKind.REGULAR
                or installed.identity != owner.identity
                or installed.filesystem != self._filesystem
            ):
                raise OSError("atomic report destination identity changed")
            self._report_owners.pop(kind, None)
        except BaseException as primary_error:
            rollback_errors = list(
                owner.close_for_operation("report temporary", operation_epoch)
            )
            if not owner.is_live:
                rollback_errors.extend(
                    self._settle_report_owner(kind, operation_epoch)
                )
            _attach_secondary(primary_error, tuple(rollback_errors))
            raise

    def close(self, *, remove_marker: bool = False) -> tuple[str, ...]:
        with self._operation_state.lock:
            operation_epoch = self._operation_state.begin()
            return self._close_locked(
                operation_epoch, remove_marker=remove_marker
            )

    def _close_locked(
        self, operation_epoch: int, *, remove_marker: bool
    ) -> tuple[str, ...]:
        if self._marker_lock.fd < 0 and not self._directory.is_open:
            return tuple(self.close_errors)
        for kind in tuple(self._report_owners):
            self._settle_report_owner(kind, operation_epoch)
        if self._report_owners:
            _append_capability_error(
                self.close_errors,
                "output close retained live report temporary owners",
            )
            return tuple(self.close_errors)
        self.release_marker(remove_marker=remove_marker)
        self.close_directory()
        return tuple(self.close_errors)

    def release_marker(self, *, remove_marker: bool = False) -> tuple[str, ...]:
        for detail in _close_lease_lock_all(self._marker_lock, "output marker"):
            _append_capability_error(self.close_errors, detail)
        if remove_marker and self._directory.is_open:
            if self._marker_lock.fd >= 0:
                _append_capability_error(
                    self.close_errors,
                    "output marker removal unavailable while marker owner is live",
                )
            else:
                for detail in _delete_exact_child(
                    self._directory,
                    OUTPUT_OWNER_FILE,
                    self._marker_identity,
                    self._backend,
                    "output marker removal",
                ):
                    _append_capability_error(self.close_errors, detail)
        return tuple(self.close_errors)

    def close_directory(self) -> tuple[str, ...]:
        if self._report_owners:
            _append_capability_error(
                self.close_errors,
                "output directory retained for report temporary owners",
            )
            return tuple(self.close_errors)
        for detail in _close_capability_once(self._directory, "output directory"):
            _append_capability_error(self.close_errors, detail)
        return tuple(self.close_errors)

    def __del__(self) -> None:
        try:
            self.close()
        except OSError:
            pass


def _bounded_output_names(
    directory: DirectoryCapability,
    backend: FilesystemBackend,
    diagnostics: list[str],
) -> list[str]:
    names: list[str] = []
    iterator = backend.entries(directory)
    primary: BaseException | None = None
    try:
        for entry in iterator:
            names.append(entry.name)
            if len(names) > 1_000:
                raise ValueError("output directory contains more than 1000 entries")
    except BaseException as error:
        primary = error

    close_details: list[str] = []
    for _attempt in range(2):
        if not iterator.directory.is_open:
            break
        try:
            iterator.close()
        except BaseException as error:
            _append_capability_error(
                close_details,
                _bounded_capability_detail("output inventory iterator close failed", error),
            )
    for detail in close_details:
        _append_capability_error(diagnostics, detail)
    if primary is not None:
        _attach_secondary(primary, tuple(close_details))
        raise primary
    if iterator.directory.is_open:
        terminal_error = OSError("output inventory iterator close failed")
        _attach_secondary(terminal_error, tuple(close_details))
        raise terminal_error
    return names


def _recover_abandoned_output(
    directory: DirectoryCapability,
    backend: FilesystemBackend,
    diagnostics: list[str],
) -> int:
    names = _bounded_output_names(directory, backend, diagnostics)
    if not names:
        return 0
    if OUTPUT_OWNER_FILE not in names:
        return 0
    marker: FileCapability | None = None
    marker_descriptor = _OwnedDescriptor()
    marker_lock = LeaseLock(-1)
    marker_identity: FileIdentity | None = None
    primary: BaseException | None = None
    try:
        evidence = backend.entry(directory, OUTPUT_OWNER_FILE)
        if evidence is None or evidence.kind is not EntryKind.REGULAR:
            return 0
        marker_identity = evidence.identity
        marker = backend.open_file(
            directory,
            OUTPUT_OWNER_FILE,
            access=FileAccess.READ_WRITE,
            disposition=CreateDisposition.OPEN_EXISTING,
            share_policy=SharePolicy.PINNED,
        )
        if marker.identity != marker_identity or marker.kind is not EntryKind.REGULAR:
            return 0
        marker_descriptor.adopt(
            marker.detach_to_fd(os.O_RDWR | getattr(os, "O_BINARY", 0))
        )
        marker_lock._prepare_reuse()
        marker_lock.fd = marker_descriptor.detach()
        try:
            marker_lock.acquire(blocking=False)
        except OSError:
            return 0
        os.lseek(marker_lock.fd, 0, os.SEEK_SET)
        initial_size = os.fstat(marker_lock.fd).st_size
        if initial_size < 0 or initial_size > MAX_OUTPUT_MARKER_BYTES:
            return 0
        chunks: list[bytes] = []
        encoded_size = 0
        while True:
            chunk = os.read(
                marker_lock.fd,
                min(16 * 1024, MAX_OUTPUT_MARKER_BYTES + 1 - encoded_size),
            )
            if not chunk:
                break
            chunks.append(chunk)
            encoded_size += len(chunk)
            if encoded_size > MAX_OUTPUT_MARKER_BYTES:
                return 0
        encoded = b"".join(chunks)
        final_size = os.fstat(marker_lock.fd).st_size
        if initial_size != final_size or final_size != len(encoded):
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
        if (
            value.get("output_device") != directory.identity.volume
            or value.get("output_inode") != directory.identity.file
        ):
            return 0
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
            temporary = backend.entry(directory, name)
            if temporary is None:
                continue
            if temporary.kind is not EntryKind.REGULAR:
                return 0
            deletion_errors = _delete_exact_child(
                directory,
                name,
                temporary.identity,
                backend,
                "output recovery temporary",
            )
            if deletion_errors:
                raise OSError("; ".join(deletion_errors))
            removed += 1
        marker_close_errors = _close_lease_lock_retry(
            marker_lock, "output recovery marker"
        )
        for detail in marker_close_errors:
            _append_capability_error(diagnostics, detail)
        if marker_lock.fd >= 0:
            raise OSError("; ".join(marker_close_errors) or "output recovery marker close failed")
        assert marker_identity is not None
        marker_delete_errors = _delete_exact_child(
            directory,
            OUTPUT_OWNER_FILE,
            marker_identity,
            backend,
            "output recovery marker",
        )
        if marker_delete_errors:
            raise OSError("; ".join(marker_delete_errors))
        for name in allowed:
            if backend.entry(directory, name) is not None:
                raise OSError(f"output recovery absence verification failed: {name}")
        return removed
    except BaseException as error:
        primary = error
        raise
    finally:
        details: list[str] = []
        if marker is not None and marker.is_open:
            for detail in _close_capability_retry(marker, "output recovery marker"):
                _append_capability_error(details, detail)
        for detail in marker_descriptor.close_retry("output recovery marker"):
            _append_capability_error(details, detail)
        if marker_lock.fd >= 0:
            for detail in _close_lease_lock_retry(marker_lock, "output recovery marker"):
                _append_capability_error(details, detail)
        for detail in details:
            _append_capability_error(diagnostics, detail)
        if details:
            if primary is not None:
                _attach_secondary(primary, tuple(details))
            elif (
                (marker is not None and marker.is_open)
                or marker_descriptor.fd >= 0
                or marker_lock.fd >= 0
            ):
                raise OSError("; ".join(details))


class RunStore:
    def __init__(
        self,
        output: Path | OwnedOutput,
        *,
        command_root: Path | None = None,
        command_root_capability: DirectoryCapability | None = None,
        backend: FilesystemBackend | None = None,
    ) -> None:
        if (command_root is None) != (command_root_capability is None):
            if command_root_capability is not None and command_root_capability.is_open:
                _close_capability_retry(
                    command_root_capability, "command spool directory rollback"
                )
            raise ValueError(
                "command_root and command_root_capability must be provided together"
            )
        self.owned_output = output if isinstance(output, OwnedOutput) else None
        self.output = output.path if isinstance(output, OwnedOutput) else output
        self._backend = (
            output._backend
            if isinstance(output, OwnedOutput) and backend is None
            else default_filesystem_backend() if backend is None else backend
        )
        self.commands = (
            self.output / "commands" if command_root is None else command_root
        )
        self._command_root = command_root_capability
        self._command_root_is_authoritative = command_root_capability is not None
        self._next_root_token = 1
        self._live_root_tokens: set[int] = set()
        self._command_capability_errors: list[str] = []
        try:
            if command_root_capability is not None:
                self._validate_command_root(
                    command_root_capability, require_managed=True
                )
            self._report_policy: DiskPolicy | None = None
            self._report_sample: Callable[
                [], tuple[DiskFailure | None, DiskObservation | None]
            ] | None = None
            self._report_freeze: (
                Callable[[], AbstractContextManager[Callable[[], bool]]] | None
            ) = None
            self._report_cancelled: Callable[[], bool] | None = None
        except BaseException as primary_error:
            if command_root_capability is not None and command_root_capability.is_open:
                _attach_secondary(
                    primary_error,
                    _close_capability_retry(
                        command_root_capability,
                        "command spool directory rollback",
                    ),
                )
            self._command_root = None
            raise

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
        if self.owned_output is None:
            if self.output.name.startswith("mutants.out"):
                raise ValueError("output path must not use the mutants.out prefix")
            if self.output.exists() and not self.output.is_dir():
                raise ValueError("output path exists and is not a directory")
        else:
            self.owned_output._verify_retained_directory()
        if self._command_root_is_authoritative:
            self._ensure_command_root()
        else:
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
        root = self._ensure_command_root()
        safe_label = re.sub(r"[^A-Za-z0-9_.-]+", "-", label).strip(".-") or "command"
        stem = f"{sequence:04d}-{safe_label}"
        stdout_name = f"{stem}.stdout"
        stderr_name = f"{stem}.stderr"
        stdout = self.commands / stdout_name
        stderr = self.commands / stderr_name
        validate_reported_path(stdout)
        validate_reported_path(stderr)
        duplicate: DirectoryCapability | None = None
        token = self._next_root_token
        self._next_root_token += 1
        try:
            duplicate = self._backend.reopen_directory(root)
            self._live_root_tokens.add(token)
            return CommandPaths(
                stdout=stdout,
                stderr=stderr,
                root=duplicate,
                stdout_name=stdout_name,
                stderr_name=stderr_name,
                backend=self._backend,
                _root_token=token,
                _release_root=self._release_command_root,
            )
        except BaseException as primary_error:
            self._live_root_tokens.discard(token)
            if duplicate is not None and duplicate.is_open:
                _attach_secondary(
                    primary_error,
                    _close_capability_retry(
                        duplicate, "command spool duplicate rollback"
                    ),
                )
            raise

    def _validate_command_root(
        self,
        root: DirectoryCapability,
        *,
        require_managed: bool,
    ) -> None:
        if (
            not root.owned_by(self._backend)
            or not root.is_open
            or root.kind is not EntryKind.DIRECTORY
            or root.share_policy is not SharePolicy.MUTATION
            or root.path_hint != self.commands
            or (
                require_managed
                and root.security_domain is not SecurityDomain.MANAGED
            )
        ):
            raise OSError("command spool directory capability is invalid")
        if root.filesystem.volume <= 0:
            raise OSError("command spool directory filesystem is invalid")

    def _ensure_command_root(self) -> DirectoryCapability:
        if self._command_root is not None:
            self._validate_command_root(
                self._command_root,
                require_managed=self._command_root_is_authoritative,
            )
            return self._command_root
        self.commands.mkdir(parents=True, exist_ok=True)
        capability: DirectoryCapability | None = None
        try:
            capability = self._backend.open_root(
                self.commands, SharePolicy.MUTATION, SecurityDomain.CALLER
            )
            self._command_root = capability
            self._validate_command_root(capability, require_managed=False)
        except BaseException as primary_error:
            if capability is not None and capability.is_open:
                _attach_secondary(
                    primary_error,
                    _close_capability_retry(
                        capability, "command spool directory rollback"
                    ),
                )
            self._command_root = None
            raise
        return capability

    def _release_command_root(self, token: int) -> None:
        self._live_root_tokens.discard(token)

    def close_command_root(self) -> tuple[str, ...]:
        if self._live_root_tokens:
            _append_capability_error(
                self._command_capability_errors,
                "command spool root still active",
            )
            return tuple(self._command_capability_errors)
        root = self._command_root
        if root is None:
            return tuple(self._command_capability_errors)
        for detail in _close_capability_once(root, "command spool directory"):
            _append_capability_error(self._command_capability_errors, detail)
        if not root.is_open:
            self._command_root = None
        return tuple(self._command_capability_errors)

    def __del__(self) -> None:
        try:
            self.close_command_root()
        except BaseException:
            pass
