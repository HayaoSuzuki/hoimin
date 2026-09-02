from __future__ import annotations

from dataclasses import dataclass, replace
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from enum import StrEnum
import json
import os
import errno
import bisect
from pathlib import Path
import re
import shlex
import stat
import struct
import sys
import threading
import time
from typing import cast
import uuid
import zlib

from .disk import (
    MAX_CLEANUP_CURSOR_BYTES,
    MAX_CLEANUP_DEPTH,
    MAX_CLEANUP_OPEN_DIRECTORIES,
    MAX_CLEANUP_SLICE_ENTRIES,
    MAX_CLEANUP_SLICE_SECONDS,
    MAX_DIAGNOSTIC_DETAIL_BYTES,
    MAX_DIAGNOSTIC_DETAILS,
    MAX_RECLAIM_CANDIDATES,
    MAX_REPORTED_PATH_BYTES,
    JANITOR_CLEANUP_SECONDS,
    JANITOR_SELECTION_SECONDS,
    OWNER_CLEANUP_SECONDS,
)
from .filesystem import (
    CreateDisposition,
    DirectoryCapability,
    DirectoryEntry,
    DirectoryIterator,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemBackend,
    FilesystemIdentity,
    SecurityDomain,
    SharePolicy,
    default_filesystem_backend,
    validate_component,
)


MANAGED_DIRECTORY = "hoimin-focused-v1"
LEASE_FILE = ".hoimin-lease.json"
HEARTBEAT_FILE = ".hoimin-heartbeat.json"
CLEANUP_READY_FILE = ".hoimin-cleanup-ready.json"
RETAIN_FILE = ".hoimin-retain.json"
COORDINATOR_FILE = ".hoimin-coordinator"
OWNER_KIND = "focused_python"
MARKER_CAPACITY = 64 * 1024
STALE_AFTER_SECONDS = 24 * 60 * 60
COORDINATOR_BYTES = 1_025
COORDINATOR_SLOT_BYTES = 512
COORDINATOR_CURSOR_BYTES = 480
COORDINATOR_MAGIC = b"HMCUR001"
COORDINATOR_SCHEMA = 1
_RUN_NAME = re.compile(
    r"^(?:run-|\.staging-|\.deleting-)([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$"
)
_CHILD_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$")
_MARKER_FIELDS = ("lease_id", "owner_kind", "run_id", "schema_version")


class ScratchCleanupStatus(StrEnum):
    CLEAN = "clean"
    FAILED = "failed"
    DEFERRED = "deferred"
    RETAINED = "retained"


@dataclass(frozen=True)
class ScratchCleanupRecord:
    status: ScratchCleanupStatus
    examined_entries: int
    removed_entries: int
    details: tuple[str, ...] = ()
    remaining_root: str | None = None
    omitted_detail_count: int = 0


@dataclass(frozen=True)
class JanitorDiagnostic:
    details: tuple[str, ...]
    omitted_detail_count: int = 0


class _DeadlineExceeded(TimeoutError):
    """A cooperative Hoimin deadline, distinct from filesystem ETIMEDOUT."""


class _InvalidManagedMarker(OSError):
    """Marker bytes are untrusted protocol data, not an I/O failure."""


class LeaseLock:
    def __init__(self, fd: int) -> None:
        self.fd = fd
        self.locked = False
        self._close_attempts = 0
        self._close_detail_0: str | None = None
        self._close_detail_1: str | None = None
        self._close_detail_count = 0
        self._finalizer_attempted = False

    @property
    def _close_details(self) -> tuple[str, ...]:
        if self._close_detail_count == 0:
            return ()
        if self._close_detail_count == 1:
            assert self._close_detail_0 is not None
            return (self._close_detail_0,)
        assert self._close_detail_0 is not None
        assert self._close_detail_1 is not None
        return (self._close_detail_0, self._close_detail_1)

    def _record_close_detail(self, detail: str) -> None:
        if self._close_detail_count == 0:
            self._close_detail_0 = detail
            self._close_detail_count = 1
        elif self._close_detail_count == 1:
            self._close_detail_1 = detail
            self._close_detail_count = 2

    def _prepare_reuse(self) -> None:
        if self.fd >= 0 or self.locked:
            raise RuntimeError("live lease lock cannot be reused")
        self._close_attempts = 0
        self._close_detail_0 = None
        self._close_detail_1 = None
        self._close_detail_count = 0
        self._finalizer_attempted = False

    def acquire(self, *, blocking: bool) -> None:
        if os.name == "nt":
            import msvcrt

            os.lseek(self.fd, 0, os.SEEK_SET)
            mode = msvcrt.LK_LOCK if blocking else msvcrt.LK_NBLCK  # type: ignore[attr-defined]
            msvcrt.locking(self.fd, mode, 1)  # type: ignore[attr-defined]
        else:
            import fcntl

            mode = fcntl.LOCK_EX | (  # type: ignore[attr-defined]
                0 if blocking else fcntl.LOCK_NB  # type: ignore[attr-defined]
            )
            fcntl.flock(self.fd, mode)  # type: ignore[attr-defined]
        self.locked = True

    def release(self) -> None:
        if not self.locked:
            return
        if os.name == "nt":
            import msvcrt

            os.lseek(self.fd, 0, os.SEEK_SET)
            msvcrt.locking(self.fd, msvcrt.LK_UNLCK, 1)  # type: ignore[attr-defined]
        else:
            import fcntl

            fcntl.flock(  # type: ignore[attr-defined]
                self.fd, fcntl.LOCK_UN  # type: ignore[attr-defined]
            )
        self.locked = False

    def close(self) -> None:
        errors = _close_lease_lock_all(self, "lease")
        if errors:
            raise OSError("; ".join(errors))

    def __del__(self) -> None:
        if self.fd < 0 or self._finalizer_attempted:
            return
        self._finalizer_attempted = True
        descriptor = self.fd
        self._close_attempts += 1
        try:
            os.close(descriptor)
        except BaseException:
            return
        self.fd = -1
        self.locked = False


def _close_lease_lock_all(lock: LeaseLock, label: str) -> tuple[str, ...]:
    if lock.fd < 0 or lock._close_attempts >= 2:
        return lock._close_details
    descriptor = lock.fd
    try:
        lock.release()
    except BaseException as error:
        lock._record_close_detail(
            _bounded_secondary(f"{label} unlock failed", error)
        )
    if descriptor >= 0:
        lock._close_attempts += 1
        try:
            os.close(descriptor)
        except BaseException as error:
            lock._record_close_detail(
                _bounded_secondary(f"{label} close failed", error)
            )
        else:
            lock.fd = -1
            lock.locked = False
    return lock._close_details


def _close_lease_lock_retry(
    lock: LeaseLock, label: str
) -> tuple[str, ...]:
    while lock.fd >= 0 and lock._close_attempts < 2:
        _close_lease_lock_all(lock, label)
    return lock._close_details


def _close_locked_coordinator_once(
    lock: LeaseLock, label: str
) -> tuple[str, ...]:
    """Close a held coordinator descriptor without an unlock/close gap."""
    if lock.fd < 0:
        if lock.locked:
            return (f"{label} close refused an inconsistent held lock",)
        return lock._close_details
    if not lock.locked:
        return (f"{label} close requires a held lock",)
    if lock._close_attempts >= 2:
        return lock._close_details
    descriptor = lock.fd
    lock._close_attempts += 1
    try:
        os.close(descriptor)
    except BaseException as error:
        lock._record_close_detail(
            _bounded_secondary(f"{label} close failed", error)
        )
        return lock._close_details
    lock.fd = -1
    lock.locked = False
    return lock._close_details


def _close_descriptors_all(
    descriptors: tuple[tuple[str, int], ...],
) -> tuple[str, ...]:
    errors: list[str] = []
    for label, descriptor in descriptors:
        if descriptor < 0:
            continue
        try:
            os.close(descriptor)
        except OSError as error:
            errors.append(
                f"{label} close failed: {type(error).__name__}: {error}"
            )
    return tuple(errors)


def validate_reported_path(path: Path) -> str:
    value = os.fspath(path)
    if "`" in value or any(ord(character) < 0x20 or ord(character) == 0x7F for character in value):
        raise ValueError("reported path is not safely representable in Markdown")
    try:
        encoded = value.encode("utf-8", errors="strict")
        json_encoded = json.dumps(value, ensure_ascii=True).encode("utf-8")
        shell_encoded = shlex.quote(value).encode("utf-8", errors="strict")
    except UnicodeError as error:
        raise ValueError("reported path is not strict UTF-8") from error
    if max(len(encoded), len(json_encoded), len(shell_encoded)) > MAX_REPORTED_PATH_BYTES:
        raise ValueError(
            "reported path exceeds "
            f"{MAX_REPORTED_PATH_BYTES} destination-specific escaped bytes"
        )
    return value


def _exception_detail(error: BaseException) -> str:
    parts = [f"{type(error).__name__}: {error}"]
    parts.extend(f"secondary: {note}" for note in getattr(error, "__notes__", ()))
    return "; ".join(parts)


def _bounded_diagnostic_detail(detail: str) -> str:
    encoded = detail.encode("utf-8", errors="replace")
    if len(encoded) <= MAX_DIAGNOSTIC_DETAIL_BYTES:
        return detail
    suffix = b"..."
    return (encoded[: MAX_DIAGNOSTIC_DETAIL_BYTES - len(suffix)] + suffix).decode(
        "utf-8", errors="ignore"
    )


def _bounded_secondary(label: str, error: BaseException) -> str:
    return _bounded_diagnostic_detail(
        f"{label}: {type(error).__name__}: {error}"
    )


_CAPABILITY_CLOSE_ATTEMPT_LIMIT = 2


def _close_capability_with_budget(
    capability: FileCapability | DirectoryCapability,
    label: str,
    *,
    maximum_new_attempts: int,
) -> tuple[str, ...]:
    errors: list[str] = []
    target_attempts = min(
        _CAPABILITY_CLOSE_ATTEMPT_LIMIT,
        capability._close_attempts + maximum_new_attempts,
    )
    while capability.is_open and capability._close_attempts < target_attempts:
        try:
            capability.close()
        except BaseException as error:
            errors.append(_bounded_secondary(f"{label} close failed", error))
        else:
            break
    return tuple(errors)


def _close_capability_retry(
    capability: FileCapability | DirectoryCapability,
    label: str,
) -> tuple[str, ...]:
    return _close_capability_with_budget(
        capability,
        label,
        maximum_new_attempts=_CAPABILITY_CLOSE_ATTEMPT_LIMIT,
    )


def _close_capability_once(
    capability: FileCapability | DirectoryCapability,
    label: str,
) -> tuple[str, ...]:
    return _close_capability_with_budget(
        capability,
        label,
        maximum_new_attempts=1,
    )


class _OwnedDescriptor:
    """A preallocated retryable owner for one detached CRT descriptor."""

    def __init__(self) -> None:
        self.fd = -1
        self._close_attempts = 0
        self._close_detail_0: str | None = None
        self._close_detail_1: str | None = None
        self._close_detail_count = 0
        self._finalizer_attempted = False

    @property
    def _close_details(self) -> tuple[str, ...]:
        if self._close_detail_count == 0:
            return ()
        if self._close_detail_count == 1:
            assert self._close_detail_0 is not None
            return (self._close_detail_0,)
        assert self._close_detail_0 is not None
        assert self._close_detail_1 is not None
        return (self._close_detail_0, self._close_detail_1)

    def _record_close_detail(self, detail: str) -> None:
        if self._close_detail_count == 0:
            self._close_detail_0 = detail
            self._close_detail_count = 1
        elif self._close_detail_count == 1:
            self._close_detail_1 = detail
            self._close_detail_count = 2

    def _prepare_reuse(self) -> None:
        if self.fd >= 0:
            raise RuntimeError("live descriptor owner cannot be reused")
        self._close_attempts = 0
        self._close_detail_0 = None
        self._close_detail_1 = None
        self._close_detail_count = 0
        self._finalizer_attempted = False

    def adopt(self, descriptor: int) -> None:
        if self.fd >= 0 or descriptor < 0:
            raise RuntimeError("descriptor owner cannot adopt this descriptor")
        self._prepare_reuse()
        self.fd = descriptor

    def detach(self) -> int:
        if self.fd < 0:
            raise RuntimeError("descriptor owner is empty")
        descriptor = self.fd
        self.fd = -1
        return descriptor

    def close_retry(self, label: str) -> tuple[str, ...]:
        while self.fd >= 0 and self._close_attempts < 2:
            self.close_once(label)
        return self._close_details

    def close_once(self, label: str) -> tuple[str, ...]:
        if self.fd < 0 or self._close_attempts >= 2:
            return self._close_details
        descriptor = self.fd
        self._close_attempts += 1
        try:
            os.close(descriptor)
        except BaseException as error:
            self._record_close_detail(
                _bounded_secondary(f"{label} close failed", error)
            )
            return self._close_details
        self.fd = -1
        return self._close_details

    def __del__(self) -> None:
        if self.fd < 0 or self._finalizer_attempted:
            return
        self._finalizer_attempted = True
        descriptor = self.fd
        self._close_attempts += 1
        try:
            os.close(descriptor)
        except BaseException:
            return
        self.fd = -1


class _MarkerOwnerSlot:
    __slots__ = ("capability", "descriptor", "lock", "fixed")

    def __init__(
        self,
        *,
        fixed: bool = False,
        with_lock: bool = False,
    ) -> None:
        self.capability: FileCapability | DirectoryCapability | None = None
        self.descriptor: _OwnedDescriptor | None = (
            _OwnedDescriptor() if fixed else None
        )
        self.lock: LeaseLock | None = (
            LeaseLock(-1) if with_lock else None
        )
        self.fixed = fixed

    def descriptor_owner(self) -> _OwnedDescriptor:
        descriptor = self.descriptor
        if descriptor is None:
            descriptor = _OwnedDescriptor()
            self.descriptor = descriptor
        return descriptor

    def clear_descriptor_if_empty(self) -> None:
        if (
            not self.fixed
            and self.descriptor is not None
            and self.descriptor.fd < 0
        ):
            self.descriptor = None

    def has_open_owner(self) -> bool:
        return (
            self.capability is not None
            and self.capability.is_open
        ) or (
            self.descriptor is not None
            and self.descriptor.fd >= 0
        ) or (
            self.lock is not None
            and self.lock.fd >= 0
        )


class _CoordinatorOwnerSlot:
    __slots__ = ("capability", "descriptor", "lock")

    def __init__(self) -> None:
        self.capability: FileCapability | None = None
        self.descriptor = _OwnedDescriptor()
        self.lock = LeaseLock(-1)

    def prepare(self) -> None:
        if self.capability is not None and self.capability.is_open:
            raise RuntimeError("coordinator capability slot is occupied")
        if self.descriptor.fd >= 0 or self.lock.fd >= 0:
            raise RuntimeError("coordinator owner slot is occupied")
        self.capability = None
        self.lock._prepare_reuse()

    def has_open_owner(self) -> bool:
        return (
            self.capability is not None and self.capability.is_open
        ) or self.descriptor.fd >= 0 or self.lock.fd >= 0


class _MarkerRollbackOwners:
    __slots__ = ("lease", "heartbeat")

    def __init__(self) -> None:
        self.lease = _MarkerOwnerSlot()
        self.heartbeat = _MarkerOwnerSlot()

    def slot(self, name: str) -> _MarkerOwnerSlot:
        if name == LEASE_FILE:
            return self.lease
        if name == HEARTBEAT_FILE:
            return self.heartbeat
        raise ValueError(f"unsupported managed marker owner: {name!r}")

    def has_open_owner(self) -> bool:
        return self.lease.has_open_owner() or self.heartbeat.has_open_owner()


def _check_deadline(deadline: float, label: str) -> None:
    if time.monotonic() >= deadline:
        raise _DeadlineExceeded(f"{label} deadline exceeded")


def _write_marker_at(
    directory_fd: int, name: str, value: dict[str, object]
) -> None:
    encoded = (json.dumps(value, sort_keys=True) + "\n").encode("utf-8")
    if len(encoded) > MARKER_CAPACITY:
        raise ValueError("managed marker exceeds 64 KiB")
    fd = os.open(
        name,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
        0o600,
        dir_fd=directory_fd,
    )
    try:
        offset = 0
        while offset < len(encoded):
            written = os.write(fd, encoded[offset:])
            if written <= 0:
                raise OSError("managed marker write made no progress")
            offset += written
        if os.fstat(fd).st_size != len(encoded):
            raise OSError("managed marker write was incomplete")
        os.fsync(fd)
    finally:
        os.close(fd)


def _marker(run_id: str, lease_id: str) -> dict[str, object]:
    return {
        "schema_version": 1,
        "run_id": run_id,
        "owner_kind": OWNER_KIND,
        "lease_id": lease_id,
    }


def _encoded_marker(value: dict[str, object]) -> bytes:
    encoded = (json.dumps(value, sort_keys=True) + "\n").encode("utf-8")
    if len(encoded) > MARKER_CAPACITY:
        raise ValueError("managed marker exceeds 64 KiB")
    return encoded


def _decode_marker(
    encoded: bytes,
    *,
    expected_run_id: str,
    expected_lease_id: str | None,
) -> dict[str, object]:
    def exact_marker(pairs: list[tuple[str, object]]) -> dict[str, object]:
        if tuple(key for key, _value in pairs) != _MARKER_FIELDS:
            raise ValueError("managed marker fields are not canonical")
        return dict(pairs)

    try:
        value = json.loads(
            encoded.decode("utf-8", errors="strict"),
            object_pairs_hook=exact_marker,
        )
    except (UnicodeError, ValueError, json.JSONDecodeError) as error:
        raise _InvalidManagedMarker("managed marker content is invalid") from error
    if (
        not isinstance(value, dict)
        or type(value.get("schema_version")) is not int
        or value.get("schema_version") != 1
        or value.get("owner_kind") != OWNER_KIND
        or not isinstance(value.get("run_id"), str)
        or value.get("run_id") != expected_run_id
        or not isinstance(value.get("lease_id"), str)
        or (
            expected_lease_id is not None
            and value.get("lease_id") != expected_lease_id
        )
    ):
        raise _InvalidManagedMarker(
            "managed marker content does not match its owner"
        )
    try:
        if str(uuid.UUID(value["run_id"])) != value["run_id"]:
            raise ValueError
        if str(uuid.UUID(value["lease_id"])) != value["lease_id"]:
            raise ValueError
    except (ValueError, TypeError, AttributeError) as error:
        raise _InvalidManagedMarker("managed marker UUID is not canonical") from error
    return value


def _delete_exact_marker(
    parent: DirectoryCapability,
    name: str,
    identity: FileIdentity,
    backend: FilesystemBackend,
    *,
    _owner_slot: _MarkerOwnerSlot | None = None,
) -> tuple[str, ...]:
    errors: list[str] = []
    current: FileCapability | DirectoryCapability | None = None
    try:
        current = backend.open_entry(parent, name, SharePolicy.PINNED)
    except FileNotFoundError:
        return ()
    except BaseException as error:
        return (
            _bounded_secondary(f"managed marker rollback open failed for {name}", error),
        )
    if _owner_slot is not None:
        if (
            _owner_slot.capability is not None
            and _owner_slot.capability.is_open
        ):
            errors.extend(
                _close_capability_retry(
                    current, f"managed marker unregistered rollback {name}"
                )
            )
            return tuple(errors)
        _owner_slot.capability = current
    if current.kind is not EntryKind.REGULAR or current.identity != identity:
        errors.append(f"managed marker rollback preserved replacement for {name}")
        errors.extend(
            _close_capability_retry(current, f"managed marker replacement {name}")
        )
        if not current.is_open and _owner_slot is not None:
            _owner_slot.capability = None
        return tuple(errors)
    try:
        backend.delete(current)
    except BaseException as error:
        errors.append(
            _bounded_secondary(f"managed marker rollback delete failed for {name}", error)
        )
    if current.is_open:
        errors.extend(
            _close_capability_retry(current, f"managed marker rollback {name}")
        )
    if current.is_open:
        return tuple(errors)
    if _owner_slot is not None:
        _owner_slot.capability = None
    try:
        remaining = backend.entry(parent, name)
    except BaseException as error:
        errors.append(
            _bounded_secondary(
                f"managed marker rollback absence check failed for {name}", error
            )
        )
    else:
        if remaining is not None:
            errors.append(f"managed marker rollback left an entry for {name}")
    return tuple(errors)


def _delete_owned_directory(
    parent: DirectoryCapability,
    name: str,
    directory: DirectoryCapability,
    backend: FilesystemBackend,
    *,
    label: str,
) -> tuple[str, ...]:
    """Delete one exact owned directory without adopting a replacement."""
    errors: list[str] = []
    if not directory.created:
        errors.append(
            _bounded_secondary(
                f"{label} rollback unavailable",
                RuntimeError("directory capability was not created by this transaction"),
            )
        )
        errors.extend(_close_capability_retry(directory, label))
        return tuple(errors)
    try:
        rollback_available = backend._directory_creation_rollback_available(
            directory
        )
    except BaseException as error:
        errors.append(
            _bounded_secondary(f"{label} rollback unavailable", error)
        )
        rollback_available = False
    if not rollback_available:
        if not errors:
            errors.append(
                _bounded_secondary(
                    f"{label} rollback unavailable",
                    RuntimeError(
                        "directory creation identity was not atomically bound"
                    ),
                )
            )
        errors.extend(_close_capability_retry(directory, label))
        return tuple(errors)
    if directory._close_attempts >= _CAPABILITY_CLOSE_ATTEMPT_LIMIT:
        errors.append(
            _bounded_secondary(
                f"{label} rollback unavailable",
                RuntimeError("capability close attempt budget is exhausted"),
            )
        )
        return tuple(errors)
    delete_failed = False
    try:
        backend.delete(directory)
    except BaseException as error:
        delete_failed = True
        errors.append(_bounded_secondary(f"{label} failed", error))
    if directory.is_open:
        errors.extend(_close_capability_retry(directory, label))
    if directory.is_open:
        return tuple(errors)
    if delete_failed:
        try:
            remaining = backend.entry(parent, name)
        except BaseException as absence_error:
            errors.append(
                _bounded_secondary(
                    f"{label} absence check failed", absence_error
                )
            )
        else:
            if remaining is not None:
                errors.append(f"{label} left a same-name entry")
    return tuple(errors)


def _marker_result(
    identity: FileIdentity, descriptor: int
) -> tuple[FileIdentity, int]:
    return identity, descriptor


def _create_marker(
    parent: DirectoryCapability,
    name: str,
    value: dict[str, object],
    backend: FilesystemBackend,
    *,
    _owner_slot: _MarkerOwnerSlot | None = None,
) -> tuple[FileIdentity, int]:
    encoded = _encoded_marker(value)
    marker: FileCapability | None = None
    descriptor_owner = _OwnedDescriptor()
    if _owner_slot is not None:
        if (
            _owner_slot.capability is not None
            and _owner_slot.capability.is_open
        ) or (
            _owner_slot.descriptor is not None
            and _owner_slot.descriptor.fd >= 0
        ):
            raise RuntimeError("managed marker owner slot is already occupied")
        _owner_slot.descriptor = descriptor_owner
    identity: FileIdentity | None = None
    try:
        marker = backend.open_file(
            parent,
            name,
            access=FileAccess.READ_WRITE,
            disposition=CreateDisposition.CREATE_NEW,
            share_policy=SharePolicy.PINNED,
        )
        if _owner_slot is not None:
            _owner_slot.capability = marker
        identity = marker.identity
        if marker.kind is not EntryKind.REGULAR:
            raise OSError("managed marker is not a regular file")
        backend.verify_managed_security(marker, repair_dacl=False)
        descriptor_owner.adopt(
            marker.detach_to_fd(os.O_RDWR | getattr(os, "O_BINARY", 0))
        )
        if _owner_slot is not None:
            _owner_slot.capability = None
        marker = None
        offset = 0
        while offset < len(encoded):
            written = os.write(descriptor_owner.fd, encoded[offset:])
            if written <= 0:
                raise OSError("managed marker write made no progress")
            offset += written
        os.fsync(descriptor_owner.fd)
        if os.fstat(descriptor_owner.fd).st_size != len(encoded):
            raise OSError("managed marker write was incomplete")
        assert identity is not None
        descriptor = descriptor_owner.fd
        result = _marker_result(identity, descriptor)
        detached = descriptor_owner.detach()
        assert detached == descriptor
        if _owner_slot is not None:
            _owner_slot.descriptor = None
        return result
    except BaseException as primary_error:
        close_errors: list[str] = []
        owner_closed = True
        if marker is not None:
            close_errors.extend(
                _close_capability_retry(marker, f"managed marker {name}")
            )
            owner_closed = not marker.is_open
            if owner_closed and _owner_slot is not None:
                _owner_slot.capability = None
        elif descriptor_owner.fd >= 0:
            close_errors.extend(
                descriptor_owner.close_retry(f"managed marker {name}")
            )
            owner_closed = descriptor_owner.fd < 0
            if owner_closed and _owner_slot is not None:
                _owner_slot.descriptor = None
        for close_error in close_errors:
            primary_error.add_note(close_error)
        if identity is not None and owner_closed:
            for rollback_error in _delete_exact_marker(
                parent,
                name,
                identity,
                backend,
                _owner_slot=_owner_slot,
            ):
                primary_error.add_note(rollback_error)
        raise


def _read_marker(
    parent: DirectoryCapability,
    name: str,
    backend: FilesystemBackend,
    *,
    expected_run_id: str,
    expected_lease_id: str | None = None,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
    _owner_slot: _MarkerOwnerSlot | None = None,
) -> tuple[FileIdentity, dict[str, object]] | None:
    marker: FileCapability | None = None
    descriptor_owner = (
        _OwnedDescriptor()
        if _owner_slot is None
        else _owner_slot.descriptor_owner()
    )
    completed = False
    clock = time.monotonic if monotonic is None else monotonic

    def check() -> None:
        if deadline is not None:
            _check_absolute_deadline(
                deadline, clock, "managed marker read deadline"
            )

    if _owner_slot is not None:
        if (
            _owner_slot.capability is not None
            and _owner_slot.capability.is_open
        ) or (
            _owner_slot.descriptor is not None
            and _owner_slot.descriptor.fd >= 0
        ):
            raise RuntimeError("managed marker owner slot is already occupied")
    try:
        check()
        try:
            marker = backend.open_file(
                parent,
                name,
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.PINNED,
            )
            if _owner_slot is not None:
                _owner_slot.capability = marker
        except FileNotFoundError:
            check()
            completed = True
            return None
        check()
        if marker.kind is not EntryKind.REGULAR:
            raise OSError("managed marker is not a regular file")
        identity = marker.identity
        check()
        backend.verify_managed_security(marker, repair_dacl=False)
        check()
        descriptor_owner.adopt(
            marker.detach_to_fd(os.O_RDONLY | getattr(os, "O_BINARY", 0))
        )
        if _owner_slot is not None:
            _owner_slot.capability = None
        marker = None
        check()
        chunks: list[bytes] = []
        remaining = MARKER_CAPACITY + 1
        while remaining:
            check()
            chunk = os.read(descriptor_owner.fd, remaining)
            check()
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        encoded = b"".join(chunks)
        if len(encoded) > MARKER_CAPACITY:
            raise OSError("managed marker exceeds capacity")
        value = _decode_marker(
            encoded,
            expected_run_id=expected_run_id,
            expected_lease_id=expected_lease_id,
        )
        result = (identity, value)
        completed = True
        return result
    except BaseException as primary_error:
        close_errors: tuple[str, ...] = ()
        if marker is not None:
            close_errors = _close_capability_retry(
                marker, f"managed marker read {name}"
            )
        elif descriptor_owner.fd >= 0:
            close_errors = descriptor_owner.close_retry(
                f"managed marker read {name}"
            )
        for close_error in close_errors:
            primary_error.add_note(close_error)
        if _owner_slot is not None:
            if marker is not None and not marker.is_open:
                _owner_slot.capability = None
            if descriptor_owner.fd < 0:
                _owner_slot.clear_descriptor_if_empty()
        raise
    finally:
        if completed:
            success_close_errors: tuple[str, ...] = ()
            if marker is not None:
                success_close_errors = _close_capability_retry(
                    marker, f"managed marker read {name}"
                )
            elif descriptor_owner.fd >= 0:
                success_close_errors = descriptor_owner.close_retry(
                    f"managed marker read {name}"
                )
            if success_close_errors:
                raise OSError("; ".join(success_close_errors))
            if _owner_slot is not None:
                if marker is not None and not marker.is_open:
                    _owner_slot.capability = None
                if descriptor_owner.fd < 0:
                    _owner_slot.clear_descriptor_if_empty()


def _open_coordinator(
    root: DirectoryCapability,
    backend: FilesystemBackend,
    *,
    timeout: float = 5.0,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
    sleep: Callable[[float], None] = time.sleep,
    _owner_slot: _CoordinatorOwnerSlot | None = None,
) -> LeaseLock:
    clock = time.monotonic if monotonic is None else monotonic
    timeout_deadline = clock() + timeout
    absolute_deadline = (
        timeout_deadline
        if deadline is None
        else min(timeout_deadline, deadline)
    )
    _check_absolute_deadline(
        absolute_deadline, clock, "managed coordinator lock deadline"
    )
    if _owner_slot is not None:
        _owner_slot.prepare()
    coordinator: FileCapability | None = None
    descriptor_owner = (
        _OwnedDescriptor()
        if _owner_slot is None
        else _owner_slot.descriptor
    )
    try:
        coordinator = backend.open_file(
            root,
            COORDINATOR_FILE,
            access=FileAccess.READ_WRITE,
            disposition=CreateDisposition.OPEN_OR_CREATE,
            share_policy=SharePolicy.PINNED,
        )
        if _owner_slot is not None:
            _owner_slot.capability = coordinator
        _check_absolute_deadline(
            absolute_deadline, clock, "managed coordinator lock deadline"
        )
        if coordinator.kind is not EntryKind.REGULAR:
            raise OSError("managed coordinator is not a regular file")
        backend.verify_managed_security(coordinator, repair_dacl=True)
        _check_absolute_deadline(
            absolute_deadline, clock, "managed coordinator lock deadline"
        )
        descriptor_owner.adopt(
            coordinator.detach_to_fd(
                os.O_RDWR | getattr(os, "O_BINARY", 0)
            )
        )
        coordinator = None
        if _owner_slot is not None:
            _owner_slot.capability = None
        _check_absolute_deadline(
            absolute_deadline, clock, "managed coordinator lock deadline"
        )
    except BaseException as primary_error:
        transfer_close_errors: list[str] = []
        transfer_close_errors.extend(
            descriptor_owner.close_retry("managed coordinator file")
        )
        if coordinator is not None:
            transfer_close_errors.extend(
                _close_capability_retry(coordinator, "managed coordinator")
            )
            if not coordinator.is_open and _owner_slot is not None:
                _owner_slot.capability = None
        for close_error in transfer_close_errors:
            primary_error.add_note(close_error)
        raise
    lock: LeaseLock | None = (
        None if _owner_slot is None else _owner_slot.lock
    )
    try:
        if lock is None:
            lock = LeaseLock(-1)
        lock.fd = descriptor_owner.detach()
        while True:
            _check_absolute_deadline(
                absolute_deadline, clock, "managed coordinator lock deadline"
            )
            try:
                lock.acquire(blocking=False)
            except BlockingIOError:
                pass
            except OSError as error:
                if error.errno not in {errno.EACCES, errno.EAGAIN}:
                    raise
            else:
                _check_absolute_deadline(
                    absolute_deadline,
                    clock,
                    "managed coordinator lock deadline",
                )
                _initialize_or_validate_coordinator(
                    lock.fd,
                    deadline=absolute_deadline,
                    monotonic=clock,
                )
                if hasattr(backend, "coordinator"):
                    setattr(backend, "coordinator", lock)
                return lock
            remaining = absolute_deadline - clock()
            if remaining <= 0:
                raise _DeadlineExceeded("managed coordinator lock timed out")
            sleep(min(0.01, remaining))
    except BaseException as primary_error:
        if lock is not None:
            initialization_close_errors = _close_lease_lock_retry(
                lock, "managed coordinator initialization"
            )
        else:
            initialization_close_errors = descriptor_owner.close_retry(
                "managed coordinator initialization"
            )
        for close_error in initialization_close_errors:
            primary_error.add_note(close_error)
        raise


@dataclass(frozen=True)
class _CoordinatorState:
    active_slot: int
    generation: int
    cursor: str


def _check_absolute_deadline(
    deadline: float,
    monotonic: Callable[[], float],
    label: str,
) -> None:
    if monotonic() >= deadline:
        raise _DeadlineExceeded(f"{label} exceeded")


def _coordinator_slot(generation: int, cursor: str) -> bytes:
    encoded = cursor.encode("ascii", errors="strict")
    if generation < 0 or generation > (1 << 64) - 1:
        raise OverflowError("coordinator generation overflow")
    if len(encoded) > COORDINATOR_CURSOR_BYTES or (
        cursor and _RUN_NAME.fullmatch(cursor) is None
    ):
        raise ValueError("invalid coordinator cursor")
    slot = bytearray(COORDINATOR_SLOT_BYTES)
    slot[0:8] = COORDINATOR_MAGIC
    struct.pack_into("<I", slot, 8, COORDINATOR_SCHEMA)
    struct.pack_into("<Q", slot, 12, generation)
    struct.pack_into("<H", slot, 20, len(encoded))
    slot[22 : 22 + len(encoded)] = encoded
    struct.pack_into("<I", slot, 508, zlib.crc32(slot[:508]) & 0xFFFF_FFFF)
    return bytes(slot)


def _decode_coordinator_slot(slot: bytes) -> tuple[int, str] | None:
    if len(slot) != COORDINATOR_SLOT_BYTES or slot[:8] != COORDINATOR_MAGIC:
        return None
    if struct.unpack_from("<I", slot, 8)[0] != COORDINATOR_SCHEMA:
        return None
    cursor_length = struct.unpack_from("<H", slot, 20)[0]
    if cursor_length > COORDINATOR_CURSOR_BYTES:
        return None
    if any(slot[22 + cursor_length : 508]):
        return None
    if struct.unpack_from("<I", slot, 508)[0] != (
        zlib.crc32(slot[:508]) & 0xFFFF_FFFF
    ):
        return None
    try:
        cursor = slot[22 : 22 + cursor_length].decode("ascii", errors="strict")
    except UnicodeError:
        return None
    if cursor and _RUN_NAME.fullmatch(cursor) is None:
        return None
    return struct.unpack_from("<Q", slot, 12)[0], cursor


def _read_at(fd: int, size: int, offset: int) -> bytes:
    os.lseek(fd, offset, os.SEEK_SET)
    return os.read(fd, size)


def _write_at(fd: int, value: bytes, offset: int) -> int:
    os.lseek(fd, offset, os.SEEK_SET)
    return os.write(fd, value)


def _initialize_or_validate_coordinator(
    fd: int,
    *,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
) -> None:
    clock = time.monotonic if monotonic is None else monotonic
    if deadline is not None:
        _check_absolute_deadline(deadline, clock, "coordinator deadline")
    metadata = os.fstat(fd)
    if deadline is not None:
        _check_absolute_deadline(deadline, clock, "coordinator deadline")
    if metadata.st_size == 0:
        initial = _coordinator_slot(0, "")
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "coordinator deadline")
        encoded = b"\0" + initial + initial
        if _write_at(fd, encoded, 0) != len(encoded):
            raise OSError("managed coordinator initialization was incomplete")
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "coordinator deadline")
        os.fsync(fd)
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "coordinator deadline")
        metadata = os.fstat(fd)
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "coordinator deadline")
    if metadata.st_size != COORDINATOR_BYTES:
        raise OSError("managed coordinator has an invalid fixed size")
    _read_coordinator_state(fd, deadline=deadline, monotonic=clock)


def _read_coordinator_state(
    fd: int,
    *,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
) -> _CoordinatorState:
    clock = time.monotonic if monotonic is None else monotonic
    if deadline is not None:
        _check_absolute_deadline(deadline, clock, "coordinator deadline")
    encoded = _read_at(fd, COORDINATOR_BYTES, 0)
    if deadline is not None:
        _check_absolute_deadline(deadline, clock, "coordinator deadline")
    if len(encoded) != COORDINATOR_BYTES or encoded[0] != 0:
        raise OSError("managed coordinator layout is invalid")
    first = _decode_coordinator_slot(encoded[1:513])
    second = _decode_coordinator_slot(encoded[513:1025])
    if first is None and second is None:
        raise OSError("both managed coordinator slots are invalid")
    if first is not None and second is not None:
        if first[0] == second[0] and first[1] != second[1]:
            raise OSError("managed coordinator slots disagree")
        if first[0] >= second[0]:
            return _CoordinatorState(0, first[0], first[1])
        return _CoordinatorState(1, second[0], second[1])
    if first is not None:
        return _CoordinatorState(0, first[0], first[1])
    assert second is not None
    return _CoordinatorState(1, second[0], second[1])


def _persist_coordinator_cursor(
    fd: int,
    state: _CoordinatorState,
    cursor: str,
    *,
    deadline: float | None = None,
) -> None:
    if deadline is not None:
        _check_deadline(deadline, "janitor selection")
    generation = state.generation + 1
    slot = _coordinator_slot(generation, cursor)
    inactive_slot = 1 - state.active_slot
    written = _write_at(
        fd, slot, 1 + inactive_slot * COORDINATOR_SLOT_BYTES
    )
    if deadline is not None:
        _check_deadline(deadline, "janitor selection")
    if written != len(slot):
        raise OSError("managed coordinator cursor write was incomplete")
    os.fsync(fd)
    if deadline is not None:
        _check_deadline(deadline, "janitor selection")


def _rollback_created_managed_root(
    root: DirectoryCapability,
    parent: DirectoryCapability,
    backend: FilesystemBackend,
) -> tuple[str, ...]:
    if not root.created:
        return _close_capability_retry(root, "managed root")
    errors: list[str] = []
    try:
        rollback_available = backend._directory_creation_rollback_available(root)
    except BaseException as error:
        errors.append(
            _bounded_secondary("managed root rollback unavailable", error)
        )
        rollback_available = False
    if not rollback_available:
        if not errors:
            errors.append(
                _bounded_secondary(
                    "managed root rollback unavailable",
                    RuntimeError(
                        "directory creation identity was not atomically bound"
                    ),
                )
            )
        errors.extend(_close_capability_retry(root, "managed root rollback"))
        return tuple(errors)
    if root._close_attempts >= _CAPABILITY_CLOSE_ATTEMPT_LIMIT:
        errors.append(
            _bounded_secondary(
                "managed root rollback unavailable",
                RuntimeError("capability close attempt budget is exhausted"),
            )
        )
        return tuple(errors)
    delete_failed = False
    try:
        backend.delete(root)
    except BaseException as error:
        delete_failed = True
        errors.append(
            _bounded_secondary("managed root rollback failed", error)
        )
    if root.is_open:
        errors.extend(_close_capability_retry(root, "managed root rollback"))
    if root.is_open:
        return tuple(errors)
    if delete_failed:
        try:
            remaining = backend.entry(parent, MANAGED_DIRECTORY)
        except BaseException as error:
            errors.append(
                _bounded_secondary(
                    "managed root rollback absence check failed", error
                )
            )
        else:
            if remaining is not None:
                errors.append("managed root rollback left a same-name entry")
    return tuple(errors)


def _ensure_managed_root(
    parent: Path,
    *,
    backend: FilesystemBackend | None = None,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
) -> tuple[Path, DirectoryCapability]:
    clock = time.monotonic if monotonic is None else monotonic
    selected_backend = (
        default_filesystem_backend() if backend is None else backend
    )
    root = parent / MANAGED_DIRECTORY
    if deadline is not None:
        _check_absolute_deadline(deadline, clock, "managed root deadline")
    parent_capability: DirectoryCapability | None = None
    root_capability: DirectoryCapability | None = None
    result: tuple[Path, DirectoryCapability] | None = None
    try:
        parent_capability = selected_backend.open_root(
            parent, SharePolicy.MUTATION, SecurityDomain.CALLER
        )
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "managed root deadline")
        root_capability = selected_backend.create_secure_root(
            parent_capability, MANAGED_DIRECTORY
        )
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "managed root deadline")
        if (
            root_capability.kind is not EntryKind.DIRECTORY
            or root_capability.security_domain is not SecurityDomain.MANAGED
        ):
            raise PermissionError("managed scratch root capability is invalid")
        if root_capability.filesystem != parent_capability.filesystem:
            raise OSError("managed scratch root crosses a filesystem boundary")
        selected_backend.verify_managed_security(
            root_capability, repair_dacl=True
        )
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "managed root deadline")
        current = selected_backend.entry(parent_capability, MANAGED_DIRECTORY)
        if (
            current is None
            or current.kind is not EntryKind.DIRECTORY
            or current.identity != root_capability.identity
            or current.filesystem != root_capability.filesystem
        ):
            raise OSError("managed scratch root identity changed after securing")
        if deadline is not None:
            _check_absolute_deadline(deadline, clock, "managed root deadline")
        selected_backend._prepare_secure_root_commit(root_capability)
        result = (root, root_capability)
    except BaseException as primary_error:
        cleanup_errors: list[str] = []
        if root_capability is not None:
            if parent_capability is not None and parent_capability.is_open:
                cleanup_errors.extend(
                    _rollback_created_managed_root(
                        root_capability,
                        parent_capability,
                        selected_backend,
                    )
                )
            else:
                cleanup_errors.extend(
                    _close_capability_retry(root_capability, "managed root")
                )
        if parent_capability is not None:
            cleanup_errors.extend(
                _close_capability_retry(
                    parent_capability, "managed root parent"
                )
            )
        for close_error in cleanup_errors:
            primary_error.add_note(close_error)
        raise
    assert parent_capability is not None
    assert root_capability is not None
    assert result is not None
    try:
        parent_capability.close()
    except BaseException as primary_error:
        cleanup_errors = list(
            _rollback_created_managed_root(
                root_capability,
                parent_capability,
                selected_backend,
            )
        )
        try:
            parent_capability.close()
        except BaseException as close_error:
            cleanup_errors.append(
                _bounded_secondary(
                    "managed root parent close failed", close_error
                )
            )
        for cleanup_error in cleanup_errors:
            primary_error.add_note(cleanup_error)
        raise
    selected_backend._commit_secure_root(root_capability)
    return result


def _directory_flags() -> int:
    return (
        os.O_RDONLY
        | getattr(os, "O_DIRECTORY", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )


def _open_directory_at(
    parent_fd: int,
    name: str,
    *,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
) -> int:
    clock = time.monotonic if monotonic is None else monotonic
    if deadline is not None:
        _check_absolute_deadline(deadline, clock, "directory reopen deadline")
    if name == ".":
        reopened = os.open(".", _directory_flags(), dir_fd=parent_fd)
        try:
            if deadline is not None:
                _check_absolute_deadline(
                    deadline, clock, "directory reopen deadline"
                )
            reopened_identity = _directory_identity(reopened)
            if deadline is not None:
                _check_absolute_deadline(
                    deadline, clock, "directory reopen deadline"
                )
            parent_identity = _directory_identity(parent_fd)
            if deadline is not None:
                _check_absolute_deadline(
                    deadline, clock, "directory reopen deadline"
                )
            if reopened_identity != parent_identity:
                raise OSError(
                    "managed directory identity changed while reopening"
                )
        except BaseException as primary_error:
            for close_error in _close_descriptors_all(
                (("reopened managed directory", reopened),)
            ):
                primary_error.add_note(close_error)
            raise
        return reopened
    if sys.platform.startswith("linux"):
        opened = _linux_open_directory_at(parent_fd, name)
    else:
        opened = os.open(name, _directory_flags(), dir_fd=parent_fd)
    try:
        if deadline is not None:
            _check_absolute_deadline(
                deadline, clock, "directory reopen deadline"
            )
    except BaseException as primary_error:
        for close_error in _close_descriptors_all(
            (("opened managed directory", opened),)
        ):
            primary_error.add_note(close_error)
        raise
    return opened


def _linux_open_directory_at(parent_fd: int, name: str) -> int:
    import ctypes

    class OpenHow(ctypes.Structure):
        _fields_ = [
            ("flags", ctypes.c_uint64),
            ("mode", ctypes.c_uint64),
            ("resolve", ctypes.c_uint64),
        ]

    encoded = os.fsencode(name)
    if b"/" in encoded or encoded in {b"", b".", b".."}:
        raise OSError("cleanup component is not a direct child")
    how = OpenHow(
        flags=_directory_flags() | getattr(os, "O_CLOEXEC", 0),
        mode=0,
        # NO_XDEV | NO_MAGICLINKS | NO_SYMLINKS | BENEATH
        resolve=0x01 | 0x02 | 0x04 | 0x08,
    )
    libc = ctypes.CDLL(None, use_errno=True)
    result = libc.syscall(
        437,
        parent_fd,
        ctypes.c_char_p(encoded),
        ctypes.byref(how),
        ctypes.sizeof(how),
    )
    if result < 0:
        error_number = ctypes.get_errno()
        raise OSError(error_number, os.strerror(error_number), name)
    return int(result)


def _directory_identity(fd: int) -> tuple[int, int]:
    metadata = os.fstat(fd)
    if not stat.S_ISDIR(metadata.st_mode):
        raise OSError("managed capability is not a directory")
    return metadata.st_dev, metadata.st_ino


def _filesystem_identity(fd: int) -> tuple[int, int, int]:
    metadata = os.fstat(fd)
    if sys.platform != "darwin":
        return metadata.st_dev, 0, 0
    import ctypes
    value = ctypes.create_string_buffer(4_096)
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.fstatfs(fd, ctypes.byref(value)) != 0:
        error_number = ctypes.get_errno()
        raise OSError(error_number, os.strerror(error_number))
    first, second = struct.unpack_from("=ii", value.raw, 48)
    return metadata.st_dev, first, second


def _directory_path_from_capability(fd: int) -> Path:
    value: str
    if sys.platform == "darwin":
        import fcntl

        command = getattr(fcntl, "F_GETPATH", None)
        if command is None:
            raise OSError("F_GETPATH is unavailable")
        raw = fcntl.fcntl(fd, command, b"\0" * min(1_024, MAX_REPORTED_PATH_BYTES))
        if not isinstance(raw, bytes):
            raise OSError("F_GETPATH returned a non-byte path")
        value = raw.split(b"\0", 1)[0].decode("utf-8", errors="strict")
    elif sys.platform.startswith("linux"):
        value = os.readlink(f"/proc/self/fd/{fd}")
    else:
        raise OSError("directory capability path recovery is unavailable")
    path = Path(value)
    if not path.is_absolute():
        raise OSError("directory capability path is not absolute")
    return path


def _entry_identity(parent_fd: int, name: str) -> tuple[int, int]:
    metadata = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
        raise OSError("managed entry is not a real directory")
    return metadata.st_dev, metadata.st_ino


def _require_current_entry(
    parent: DirectoryCapability,
    name: str,
    backend: FilesystemBackend,
    *,
    kind: EntryKind,
    identity: FileIdentity,
    filesystem: FilesystemIdentity,
    label: str,
) -> None:
    current = backend.entry(parent, name)
    if (
        current is None
        or current.kind is not kind
        or current.identity != identity
        or current.filesystem != filesystem
    ):
        raise OSError(f"{label} identity changed")


def _open_owned_marker(
    parent: DirectoryCapability,
    name: str,
    backend: FilesystemBackend,
    *,
    access: FileAccess,
    identity: FileIdentity,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
    _owner_slot: _MarkerOwnerSlot | None = None,
) -> FileCapability:
    marker: FileCapability | None = None
    clock = time.monotonic if monotonic is None else monotonic

    def check() -> None:
        if deadline is not None:
            _check_absolute_deadline(
                deadline, clock, "managed marker reopen deadline"
            )

    if (
        _owner_slot is not None
        and _owner_slot.capability is not None
        and _owner_slot.capability.is_open
    ):
        raise RuntimeError("managed marker capability slot is already occupied")
    try:
        check()
        marker = backend.open_file(
            parent,
            name,
            access=access,
            disposition=CreateDisposition.OPEN_EXISTING,
            share_policy=SharePolicy.PINNED,
        )
        if _owner_slot is not None:
            _owner_slot.capability = marker
        check()
        if (
            marker.kind is not EntryKind.REGULAR
            or marker.identity != identity
            or marker.filesystem != parent.filesystem
        ):
            raise OSError(f"managed marker identity changed for {name}")
        check()
        backend.verify_managed_security(marker, repair_dacl=False)
        check()
        _require_current_entry(
            parent,
            name,
            backend,
            kind=EntryKind.REGULAR,
            identity=identity,
            filesystem=parent.filesystem,
            label=f"managed marker {name}",
        )
        check()
        return marker
    except BaseException as primary_error:
        if marker is not None:
            for close_error in _close_capability_retry(
                marker, f"managed marker {name}"
            ):
                primary_error.add_note(close_error)
            if not marker.is_open and _owner_slot is not None:
                _owner_slot.capability = None
        raise


def _read_locked_marker(
    lock: LeaseLock,
    *,
    expected_run_id: str,
    expected_lease_id: str,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
) -> dict[str, object]:
    clock = time.monotonic if monotonic is None else monotonic

    def check() -> None:
        if deadline is not None:
            _check_absolute_deadline(
                deadline, clock, "locked marker read deadline"
            )

    check()
    os.lseek(lock.fd, 0, os.SEEK_SET)
    check()
    chunks: list[bytes] = []
    remaining = MARKER_CAPACITY + 1
    while remaining:
        check()
        chunk = os.read(lock.fd, remaining)
        check()
        if not chunk:
            break
        chunks.append(chunk)
        remaining -= len(chunk)
    encoded = b"".join(chunks)
    if len(encoded) > MARKER_CAPACITY:
        raise OSError("managed marker exceeds capacity")
    return _decode_marker(
        encoded,
        expected_run_id=expected_run_id,
        expected_lease_id=expected_lease_id,
    )


class ManagedScratch:
    def __init__(
        self,
        *,
        backend: FilesystemBackend,
        managed_root: Path,
        path: Path,
        run_id: str,
        lease_id: str,
        lease: LeaseLock,
        lease_identity: FileIdentity,
        managed_root_capability: DirectoryCapability,
        root: DirectoryCapability,
        heartbeat: FileCapability | None,
        heartbeat_identity: FileIdentity,
    ) -> None:
        self._backend = backend
        self.managed_root = managed_root
        self.path = path
        self.run_id = run_id
        self.lease_id = lease_id
        self._lease: LeaseLock | None = lease
        self._lease_identity = lease_identity
        self._managed_root_capability: DirectoryCapability | None = (
            managed_root_capability
        )
        self._root: DirectoryCapability | None = root
        self._root_identity = root.identity
        self._root_filesystem = root.filesystem
        self._heartbeat: FileCapability | None = heartbeat
        self._heartbeat_identity = heartbeat_identity
        self._children: dict[str, _ChildCleanupState] = {}
        self._cleanup_ready = False
        self._registry_lock = threading.RLock()
        self._registry_generation = 0
        self._registry_frozen = False
        self._cleanup_owned_capabilities: (
            _FixedOwnerRegistry
            | list[FileCapability | DirectoryCapability]
        ) = _FixedOwnerRegistry()
        self._cleanup_graph: _CleanupOwnerGraph | None = None
        self._cleanup_cursor: _CleanupCursor | None = None

    def _ensure_cleanup_graph(self) -> _CleanupOwnerGraph:
        graph = self._cleanup_graph
        if graph is not None:
            return graph
        registry = self._cleanup_owned_capabilities
        fixed_registry = (
            registry
            if isinstance(registry, _FixedOwnerRegistry)
            else _FixedOwnerRegistry()
        )
        graph = _CleanupOwnerGraph(fixed_registry)
        self._cleanup_graph = graph
        return graph

    def _close_cleanup_graph(
        self,
        *,
        retry: bool,
        attempted: set[int] | None = None,
    ) -> tuple[str, ...]:
        graph = self._cleanup_graph
        if graph is None:
            return ()
        errors: list[str] = []
        seen_capabilities: set[int] = set()
        seen_locks: set[int] = set()

        def close_capability(
            owner: FileCapability | DirectoryCapability | None,
            label: str,
        ) -> None:
            if owner is None or not owner.is_open:
                return
            identity = id(owner)
            if identity in seen_capabilities:
                return
            seen_capabilities.add(identity)
            if attempted is not None:
                attempted.add(identity)
            errors.extend(
                _close_capability_retry(owner, label)
                if retry
                else _close_capability_once(owner, label)
            )

        def close_lock(lock: LeaseLock | None, label: str) -> None:
            if lock is None or lock.fd < 0:
                return
            identity = id(lock)
            if identity in seen_locks:
                return
            seen_locks.add(identity)
            if attempted is not None:
                attempted.add(identity)
            errors.extend(
                _close_lease_lock_retry(lock, label)
                if retry
                else _close_lease_lock_all(lock, label)
            )

        iterator = graph.walker_iterator.owner
        if iterator is not None:
            directory = iterator.directory
            identity = id(directory)
            seen_capabilities.add(identity)
            if attempted is not None:
                attempted.add(identity)
            if retry:
                errors.extend(
                    _close_cleanup_iterator(iterator, "cleanup iterator owner")
                )
            elif (
                directory.is_open
                and directory._close_attempts
                < _CAPABILITY_CLOSE_ATTEMPT_LIMIT
            ):
                try:
                    iterator.close()
                except BaseException as error:
                    errors.append(
                        _bounded_secondary(
                            "cleanup iterator owner close failed", error
                        )
                    )
            if not directory.is_open:
                graph.walker_iterator.owner = None

        for capability_slot in graph.capability_slots():
            close_capability(
                capability_slot.owner, "managed cleanup graph owner"
            )
            if (
                capability_slot.owner is not None
                and not capability_slot.owner.is_open
            ):
                capability_slot.owner = None
        for marker_slot in graph.marker_slots():
            close_capability(
                marker_slot.capability, "managed marker graph owner"
            )
            if (
                marker_slot.capability is not None
                and not marker_slot.capability.is_open
            ):
                marker_slot.capability = None
            descriptor = marker_slot.descriptor
            if descriptor is not None and descriptor.fd >= 0:
                errors.extend(
                    descriptor.close_retry("managed marker graph descriptor")
                    if retry
                    else descriptor.close_once(
                        "managed marker graph descriptor"
                    )
                )
            close_lock(marker_slot.lock, "managed marker graph lock")
        for coordinator_slot in (
            graph.claim_coordinator,
            graph.tail_coordinator,
        ):
            close_capability(
                coordinator_slot.capability,
                "managed coordinator graph capability",
            )
            if (
                coordinator_slot.capability is not None
                and not coordinator_slot.capability.is_open
            ):
                coordinator_slot.capability = None
            if coordinator_slot.descriptor.fd >= 0:
                errors.extend(
                    coordinator_slot.descriptor.close_retry(
                        "managed coordinator graph descriptor"
                    )
                    if retry
                    else coordinator_slot.descriptor.close_once(
                        "managed coordinator graph descriptor"
                    )
                )
            close_lock(
                coordinator_slot.lock, "managed coordinator graph lock"
            )
        for owner in graph.blocked:
            close_capability(owner, "managed blocked cleanup owner")
        graph.blocked.clear_closed()
        for detail in errors:
            graph.details.add(detail)
        return tuple(errors)

    def _cleanup_graph_has_blocked_owner(self) -> bool:
        graph = self._cleanup_graph
        if graph is None:
            return False
        if any(owner.is_open for owner in graph.blocked):
            return True
        if graph.walker_iterator.owner is not None and (
            graph.walker_iterator.owner.directory.is_open
        ):
            return True
        if any(
            slot.owner is not None and slot.owner.is_open
            for slot in graph.capability_slots()
        ):
            return True
        if graph.claim_coordinator.has_open_owner():
            return True
        if graph.tail_coordinator.has_open_owner():
            return True
        if graph.lease_read.has_open_owner():
            return True
        if graph.heartbeat_read.has_open_owner():
            return True
        if (
            graph.restored_lease.capability is not None
            and graph.restored_lease.capability.is_open
        ) or (
            graph.restored_lease.descriptor is not None
            and graph.restored_lease.descriptor.fd >= 0
        ) or (
            graph.restored_lease.lock is not None
            and graph.restored_lease.lock.fd >= 0
            and graph.restored_lease.lock is not self._lease
        ):
            return True
        if (
            graph.restored_heartbeat.capability is not None
            and graph.restored_heartbeat.capability.is_open
            and graph.restored_heartbeat.capability is not self._heartbeat
        ):
            return True
        return False

    @classmethod
    def create(
        cls,
        parent: Path,
        *,
        run_id: str | None = None,
        stale_cleanup: list[ScratchCleanupRecord] | None = None,
        stale_diagnostics: list[JanitorDiagnostic] | None = None,
        backend: FilesystemBackend | None = None,
        _reclaimer: Callable[
            [Path], list[ScratchCleanupRecord | JanitorDiagnostic]
        ]
        | None = None,
    ) -> "ManagedScratch":
        """Publish one run graph after capability-backed stale reclamation."""
        selected_backend = (
            default_filesystem_backend() if backend is None else backend
        )
        managed_root_capability: DirectoryCapability | None = None
        managed_root, managed_root_capability = _ensure_managed_root(
            parent.resolve(strict=True), backend=selected_backend
        )
        try:
            validate_reported_path(managed_root)
            run_id = str(uuid.uuid4()) if run_id is None else run_id
            if str(uuid.UUID(run_id)) != run_id:
                raise ValueError("run ID must be a canonical UUID")
            if _reclaimer is None:
                janitor_root = selected_backend.reopen_directory(
                    managed_root_capability, SharePolicy.MUTATION
                )
                try:
                    reclaimed = reclaim_abandoned(
                        managed_root,
                        backend=selected_backend,
                        managed_root_capability=janitor_root,
                    )
                finally:
                    janitor_close_errors = _close_capability_retry(
                        janitor_root, "managed scratch janitor root"
                    )
                    if janitor_close_errors and sys.exc_info()[1] is None:
                        raise OSError("; ".join(janitor_close_errors))
            else:
                reclaimed = _reclaimer(managed_root)
            for item in reclaimed:
                if isinstance(item, JanitorDiagnostic):
                    if stale_diagnostics is not None:
                        stale_diagnostics.append(item)
                elif stale_cleanup is not None:
                    stale_cleanup.append(item)
        except BaseException as primary_error:
            for close_error in _close_capability_retry(
                managed_root_capability, "managed scratch creation root"
            ):
                primary_error.add_note(close_error)
            raise
        try:
            coordinator: LeaseLock | None = _open_coordinator(
                managed_root_capability, selected_backend
            )
        except BaseException as primary_error:
            for close_error in _close_capability_retry(
                managed_root_capability, "managed scratch creation root"
            ):
                primary_error.add_note(close_error)
            raise
        assert run_id is not None
        lease_id: str | None = None
        staging: Path | None = None
        active: Path | None = None
        deleting: Path | None = None
        lease: LeaseLock | None = None
        heartbeat: FileCapability | None = None
        heartbeat_descriptor_owner: _OwnedDescriptor | None = None
        marker_owners: _MarkerRollbackOwners | None = None
        staging_capability: DirectoryCapability | None = None
        lease_identity: FileIdentity | None = None
        heartbeat_identity: FileIdentity | None = None
        published = False
        scratch: ManagedScratch | None = None
        heartbeat_descriptor_close_attempted = False
        heartbeat_close_attempted = False
        lease_close_attempted = False
        try:
            lease_id = str(uuid.uuid4())
            staging = managed_root / f".staging-{run_id}"
            active = managed_root / f"run-{run_id}"
            deleting = managed_root / f".deleting-{run_id}"
            heartbeat_descriptor_owner = _OwnedDescriptor()
            marker_owners = _MarkerRollbackOwners()
            validate_reported_path(staging)
            validate_reported_path(active)
            validate_reported_path(deleting)
            if selected_backend.entry(
                managed_root_capability, active.name
            ) is not None:
                raise FileExistsError(
                    errno.EEXIST,
                    "managed active root already exists",
                    str(active),
                )
            if selected_backend.entry(
                managed_root_capability, staging.name
            ) is not None:
                raise FileExistsError(
                    errno.EEXIST,
                    "managed staging root already exists",
                    str(staging),
                )
            staging_capability = selected_backend.create_directory(
                managed_root_capability,
                staging.name,
                SharePolicy.PINNED,
            )
            if (
                staging_capability.kind is not EntryKind.DIRECTORY
                or staging_capability.filesystem
                != managed_root_capability.filesystem
                or staging_capability.security_domain
                is not SecurityDomain.MANAGED
            ):
                raise OSError("managed staging capability is invalid")
            selected_backend.verify_managed_security(
                staging_capability, repair_dacl=False
            )
            _require_current_entry(
                managed_root_capability,
                staging.name,
                selected_backend,
                kind=EntryKind.DIRECTORY,
                identity=staging_capability.identity,
                filesystem=staging_capability.filesystem,
                label="managed staging root",
            )
            marker = _marker(run_id, lease_id)
            lease = LeaseLock(-1)
            marker_owners.lease.lock = lease
            lease_identity, lease.fd = _create_marker(
                staging_capability,
                LEASE_FILE,
                marker,
                selected_backend,
                _owner_slot=marker_owners.lease,
            )
            lease.acquire(blocking=True)
            heartbeat_result = _create_marker(
                staging_capability,
                HEARTBEAT_FILE,
                marker,
                selected_backend,
                _owner_slot=marker_owners.heartbeat,
            )
            heartbeat_identity = heartbeat_result[0]
            marker_owners.heartbeat.descriptor = heartbeat_descriptor_owner
            heartbeat_descriptor_owner.adopt(heartbeat_result[1])
            heartbeat_descriptor_close_attempted = True
            heartbeat_close_errors = heartbeat_descriptor_owner.close_once(
                "managed heartbeat creation"
            )
            if heartbeat_close_errors:
                raise OSError("; ".join(heartbeat_close_errors))
            heartbeat = _open_owned_marker(
                staging_capability,
                HEARTBEAT_FILE,
                selected_backend,
                access=FileAccess.WRITE,
                identity=heartbeat_identity,
                _owner_slot=marker_owners.heartbeat,
            )
            if selected_backend.entry(
                managed_root_capability, active.name
            ) is not None:
                raise FileExistsError(
                    errno.EEXIST,
                    "managed active root already exists",
                    str(active),
                )
            if selected_backend.directory_rename_requires_closed_descendants:
                heartbeat_close_attempted = True
                heartbeat_close_errors = _close_capability_once(
                    heartbeat, "managed heartbeat handoff"
                )
                if heartbeat_close_errors:
                    raise OSError("; ".join(heartbeat_close_errors))
                heartbeat = None
                lease_close_attempted = True
                lease_close_errors = _close_lease_lock_all(
                    lease, "managed lease handoff"
                )
                if lease_close_errors:
                    if lease.fd < 0:
                        lease = None
                    raise OSError("; ".join(lease_close_errors))
                lease = None
            selected_backend.rename(
                staging_capability,
                managed_root_capability,
                active.name,
                replace=False,
            )
            published = True
            _require_current_entry(
                managed_root_capability,
                active.name,
                selected_backend,
                kind=EntryKind.DIRECTORY,
                identity=staging_capability.identity,
                filesystem=staging_capability.filesystem,
                label="published managed root",
            )
            if selected_backend.directory_rename_requires_closed_descendants:
                lease_read = _read_marker(
                    staging_capability,
                    LEASE_FILE,
                    selected_backend,
                    expected_run_id=run_id,
                    expected_lease_id=lease_id,
                    _owner_slot=marker_owners.lease,
                )
                if lease_read is None or lease_read != (
                    lease_identity,
                    marker,
                ):
                    raise OSError("published managed lease changed")
                heartbeat_read = _read_marker(
                    staging_capability,
                    HEARTBEAT_FILE,
                    selected_backend,
                    expected_run_id=run_id,
                    expected_lease_id=lease_id,
                    _owner_slot=marker_owners.heartbeat,
                )
                if heartbeat_read is None or heartbeat_read != (
                    heartbeat_identity,
                    marker,
                ):
                    raise OSError("published managed heartbeat changed")
                lease = LeaseLock(-1)
                lease_close_attempted = False
                marker_owners.lease.lock = lease
                lease_capability = _open_owned_marker(
                    staging_capability,
                    LEASE_FILE,
                    selected_backend,
                    access=FileAccess.READ_WRITE,
                    identity=lease_identity,
                    _owner_slot=marker_owners.lease,
                )
                try:
                    lease.fd = lease_capability.detach_to_fd(
                        os.O_RDWR | getattr(os, "O_BINARY", 0)
                    )
                    lease.acquire(blocking=True)
                    if _read_locked_marker(
                        lease,
                        expected_run_id=run_id,
                        expected_lease_id=lease_id,
                    ) != marker:
                        raise OSError("locked managed lease changed")
                except BaseException as primary_error:
                    if lease_capability.is_open:
                        for close_error in _close_capability_retry(
                            lease_capability, "published managed lease"
                        ):
                            primary_error.add_note(close_error)
                    if lease.fd >= 0:
                        lease_close_attempted = True
                        for close_error in _close_lease_lock_all(
                            lease, "published managed lease"
                        ):
                            primary_error.add_note(close_error)
                    raise
                heartbeat = _open_owned_marker(
                    staging_capability,
                    HEARTBEAT_FILE,
                    selected_backend,
                    access=FileAccess.WRITE,
                    identity=heartbeat_identity,
                    _owner_slot=marker_owners.heartbeat,
                )
                heartbeat_close_attempted = False
            assert lease is not None
            assert heartbeat is not None
            assert lease_identity is not None
            assert heartbeat_identity is not None
            scratch = cls(
                backend=selected_backend,
                managed_root=managed_root,
                path=active,
                run_id=run_id,
                lease_id=lease_id,
                lease=lease,
                lease_identity=lease_identity,
                managed_root_capability=managed_root_capability,
                root=staging_capability,
                heartbeat=heartbeat,
                heartbeat_identity=heartbeat_identity,
            )
            assert coordinator is not None
            coordinator_errors = _close_locked_coordinator_once(
                coordinator, "managed coordinator"
            )
            if coordinator_errors:
                raise OSError("; ".join(coordinator_errors))
            coordinator = None
            return scratch
        except BaseException as primary_error:
            rollback_errors: list[str] = []
            if scratch is not None:
                scratch._lease = None
                scratch._heartbeat = None
                scratch._root = None
                scratch._managed_root_capability = None
            if lease is not None:
                if lease_close_attempted:
                    rollback_errors.extend(
                        _close_lease_lock_all(lease, "managed lease rollback")
                    )
                else:
                    rollback_errors.extend(
                        _close_lease_lock_retry(lease, "managed lease rollback")
                    )
                if lease.fd < 0:
                    lease = None
            if heartbeat is not None:
                if heartbeat_close_attempted:
                    rollback_errors.extend(
                        _close_capability_once(
                            heartbeat, "managed heartbeat rollback"
                        )
                    )
                else:
                    rollback_errors.extend(
                        _close_capability_retry(
                            heartbeat, "managed heartbeat rollback"
                        )
                    )
                if not heartbeat.is_open:
                    heartbeat = None
            if heartbeat_descriptor_owner is not None:
                if heartbeat_descriptor_close_attempted:
                    rollback_errors.extend(
                        heartbeat_descriptor_owner.close_once(
                            "managed heartbeat rollback"
                        )
                    )
                else:
                    rollback_errors.extend(
                        heartbeat_descriptor_owner.close_retry(
                            "managed heartbeat rollback"
                        )
                    )
            marker_owner_open = (
                marker_owners is not None
                and marker_owners.has_open_owner()
            ) or (
                lease is not None and lease.fd >= 0
            ) or (
                heartbeat is not None and heartbeat.is_open
            ) or (
                heartbeat_descriptor_owner is not None
                and heartbeat_descriptor_owner.fd >= 0
            )
            namespace_cleanup_unavailable_noted = False
            if marker_owner_open:
                rollback_errors.append(
                    _bounded_secondary(
                        "managed namespace cleanup unavailable",
                        RuntimeError(
                            "a marker owner remains open"
                        ),
                    )
                )
                namespace_cleanup_unavailable_noted = True
            renamed_back = False
            if (
                published
                and staging_capability is not None
                and staging is not None
                and not marker_owner_open
            ):
                try:
                    selected_backend.rename(
                        staging_capability,
                        managed_root_capability,
                        staging.name,
                        replace=False,
                    )
                    published = False
                    renamed_back = True
                except BaseException as error:
                    rollback_errors.append(
                        _bounded_secondary(
                            "managed publication rename rollback failed", error
                        )
                    )
            if (
                renamed_back
                and selected_backend.directory_rename_requires_closed_descendants
                and staging_capability is not None
                and lease_identity is not None
                and heartbeat_identity is not None
                and lease_id is not None
                and marker_owners is not None
                and not marker_owner_open
            ):
                restored_lease: LeaseLock | None = None
                restored_heartbeat: FileCapability | None = None
                try:
                    lease_read = _read_marker(
                        staging_capability,
                        LEASE_FILE,
                        selected_backend,
                        expected_run_id=run_id,
                        expected_lease_id=lease_id,
                        _owner_slot=marker_owners.lease,
                    )
                    heartbeat_read = _read_marker(
                        staging_capability,
                        HEARTBEAT_FILE,
                        selected_backend,
                        expected_run_id=run_id,
                        expected_lease_id=lease_id,
                        _owner_slot=marker_owners.heartbeat,
                    )
                    if lease_read != (lease_identity, marker) or heartbeat_read != (
                        heartbeat_identity,
                        marker,
                    ):
                        raise OSError("managed publication restore content changed")
                    restored_lease = LeaseLock(-1)
                    marker_owners.lease.lock = restored_lease
                    restored_capability = _open_owned_marker(
                        staging_capability,
                        LEASE_FILE,
                        selected_backend,
                        access=FileAccess.READ_WRITE,
                        identity=lease_identity,
                        _owner_slot=marker_owners.lease,
                    )
                    try:
                        restored_lease.fd = restored_capability.detach_to_fd(
                            os.O_RDWR | getattr(os, "O_BINARY", 0)
                        )
                    except BaseException as restore_error:
                        for close_error in _close_capability_retry(
                            restored_capability, "restored managed lease"
                        ):
                            restore_error.add_note(close_error)
                        raise
                    restored_lease.acquire(blocking=True)
                    if _read_locked_marker(
                        restored_lease,
                        expected_run_id=run_id,
                        expected_lease_id=lease_id,
                    ) != marker:
                        raise OSError("restored managed lease changed")
                    restored_heartbeat = _open_owned_marker(
                        staging_capability,
                        HEARTBEAT_FILE,
                        selected_backend,
                        access=FileAccess.WRITE,
                        identity=heartbeat_identity,
                        _owner_slot=marker_owners.heartbeat,
                    )
                except BaseException as error:
                    rollback_errors.append(
                        _bounded_secondary(
                            "managed publication owner restore failed", error
                        )
                    )
                    rollback_errors.extend(
                        getattr(error, "__notes__", ())
                    )
                finally:
                    if restored_lease is not None:
                        rollback_errors.extend(
                            _close_lease_lock_retry(
                                restored_lease, "restored managed lease"
                            )
                        )
                    if restored_heartbeat is not None:
                        rollback_errors.extend(
                            _close_capability_retry(
                                restored_heartbeat,
                                "restored managed heartbeat",
                            )
                        )
            marker_owner_open = marker_owner_open or (
                marker_owners is not None
                and marker_owners.has_open_owner()
            ) or (
                lease is not None and lease.fd >= 0
            ) or (
                heartbeat is not None and heartbeat.is_open
            ) or (
                heartbeat_descriptor_owner is not None
                and heartbeat_descriptor_owner.fd >= 0
            )
            if marker_owner_open and not namespace_cleanup_unavailable_noted:
                rollback_errors.append(
                    _bounded_secondary(
                        "managed namespace cleanup unavailable",
                        RuntimeError("a marker owner remains open"),
                    )
                )
                namespace_cleanup_unavailable_noted = True
            if (
                staging_capability is not None
                and staging is not None
                and active is not None
            ):
                if marker_owner_open:
                    rollback_errors.extend(
                        _close_capability_retry(
                            staging_capability, "managed staging rollback"
                        )
                    )
                else:
                    if heartbeat_identity is not None:
                        rollback_errors.extend(
                            _delete_exact_marker(
                                staging_capability,
                                HEARTBEAT_FILE,
                                heartbeat_identity,
                                selected_backend,
                                _owner_slot=(
                                    None
                                    if marker_owners is None
                                    else marker_owners.heartbeat
                                ),
                            )
                        )
                        marker_owner_open = (
                            marker_owners is not None
                            and marker_owners.has_open_owner()
                        )
                    if not marker_owner_open and lease_identity is not None:
                        rollback_errors.extend(
                            _delete_exact_marker(
                                staging_capability,
                                LEASE_FILE,
                                lease_identity,
                                selected_backend,
                                _owner_slot=(
                                    None
                                    if marker_owners is None
                                    else marker_owners.lease
                                ),
                            )
                        )
                        marker_owner_open = (
                            marker_owners is not None
                            and marker_owners.has_open_owner()
                        )
                    if marker_owner_open:
                        if not namespace_cleanup_unavailable_noted:
                            rollback_errors.append(
                                _bounded_secondary(
                                    "managed namespace cleanup unavailable",
                                    RuntimeError(
                                        "a marker owner remains open"
                                    ),
                                )
                            )
                            namespace_cleanup_unavailable_noted = True
                        rollback_errors.extend(
                            _close_capability_retry(
                                staging_capability,
                                "managed staging rollback",
                            )
                        )
                    else:
                        rollback_errors.extend(
                            _delete_owned_directory(
                                managed_root_capability,
                                active.name if published else staging.name,
                                staging_capability,
                                selected_backend,
                                label="managed staging rollback",
                            )
                        )
            elif staging_capability is not None:
                rollback_errors.append(
                    "managed staging rollback unavailable: creation state missing"
                )
                rollback_errors.extend(
                    _close_capability_retry(
                        staging_capability, "managed staging rollback"
                    )
                )
            rollback_errors.extend(
                _close_capability_retry(
                    managed_root_capability, "managed root rollback"
                )
            )
            for rollback_error in rollback_errors:
                primary_error.add_note(rollback_error)
            raise
        finally:
            if coordinator is not None:
                active_error = sys.exc_info()[1]
                coordinator_errors = _close_locked_coordinator_once(
                    coordinator, "managed coordinator"
                )
                if coordinator.fd < 0:
                    coordinator = None
                if coordinator_errors:
                    if active_error is None:
                        raise OSError("; ".join(coordinator_errors))
                    for coordinator_error in coordinator_errors:
                        active_error.add_note(coordinator_error)

    @contextmanager
    def freeze_registry(self) -> Iterator[int]:
        with self._registry_lock:
            if self._registry_frozen:
                raise RuntimeError("scratch registry is already frozen")
            self._registry_frozen = True
            generation = self._registry_generation
            try:
                yield generation
            finally:
                self._registry_frozen = False

    @contextmanager
    def report_boundary(self) -> Iterator[Callable[[], bool]]:
        with self.freeze_registry() as generation:
            yield lambda: self.registry_generation_is(generation)

    def registry_generation_is(self, generation: int) -> bool:
        with self._registry_lock:
            return self._registry_generation == generation

    def note_dispatch(self) -> None:
        with self._registry_lock:
            if self._registry_frozen:
                raise RuntimeError("scratch registry is frozen")
            self._registry_generation += 1

    def create_child(self, name: str) -> Path:
        with self._registry_lock:
            if self._registry_frozen:
                raise RuntimeError("scratch registry is frozen")
            child = self._create_child_unlocked(name)
            self._registry_generation += 1
            return child

    def _create_child_unlocked(self, name: str) -> Path:
        if _CHILD_NAME.fullmatch(name) is None or validate_component(name) != name:
            raise ValueError(f"invalid managed child name: {name!r}")
        if name in self._children:
            raise FileExistsError(name)
        root = self._root
        if root is None or not root.is_open:
            raise OSError("managed run root capability is unavailable")
        child = self.path / name
        validate_reported_path(child)
        count = 0
        iterator = self._backend.entries(root)
        try:
            for entry in iterator:
                if entry.name.startswith("."):
                    continue
                count += 1
                if count >= 100_000:
                    raise OSError(
                        "managed scratch direct-child limit reached"
                    )
        except BaseException as primary_error:
            for _attempt in range(2):
                try:
                    iterator.close()
                except BaseException as close_error:
                    primary_error.add_note(
                        _bounded_secondary(
                            "managed child iterator close failed", close_error
                        )
                    )
                else:
                    break
            raise
        close_errors: list[str] = []
        for _attempt in range(2):
            try:
                iterator.close()
            except BaseException as close_error:
                close_errors.append(
                    _bounded_secondary(
                        "managed child iterator close failed", close_error
                    )
                )
            else:
                break
        if close_errors:
            raise OSError("; ".join(close_errors))
        state = _ChildCleanupState()
        self._children[name] = state
        child_capability: DirectoryCapability | None = None
        try:
            child_capability = self._backend.create_directory(
                root, name, SharePolicy.PINNED
            )
            state.root_owner.owner = child_capability
            state.identity = child_capability.identity
            state.filesystem = child_capability.filesystem
            if (
                child_capability.kind is not EntryKind.DIRECTORY
                or child_capability.filesystem != root.filesystem
                or child_capability.security_domain is not SecurityDomain.MANAGED
            ):
                raise OSError("managed child capability is invalid")
            _require_current_entry(
                root,
                name,
                self._backend,
                kind=EntryKind.DIRECTORY,
                identity=child_capability.identity,
                filesystem=child_capability.filesystem,
                label="managed child",
            )
            child_capability.close()
            state.root_owner.owner = None
        except BaseException as primary_error:
            if child_capability is not None and child_capability.is_open:
                for rollback_error in _delete_owned_directory(
                    root,
                    name,
                    child_capability,
                    self._backend,
                    label="managed child rollback",
                ):
                    primary_error.add_note(rollback_error)
            if child_capability is None or not child_capability.is_open:
                state.root_owner.owner = None
                if self._children.get(name) is state:
                    self._children.pop(name, None)
            raise
        return child

    def open_child(
        self,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        if _CHILD_NAME.fullmatch(name) is None or validate_component(name) != name:
            raise ValueError(f"invalid managed child name: {name!r}")
        expected = self._children.get(name)
        if expected is None:
            raise OSError("managed child is not registered")
        root = self._root
        if root is None or not root.is_open:
            raise OSError("managed run root capability is unavailable")
        opened: DirectoryCapability | None = None
        try:
            opened = self._backend.open_directory(root, name, share_policy)
            identity = expected.identity
            filesystem = expected.filesystem
            if identity is None or filesystem is None:
                raise OSError("managed child registration is incomplete")
            if (
                opened.kind is not EntryKind.DIRECTORY
                or opened.identity != identity
                or opened.filesystem != filesystem
            ):
                raise OSError("managed child identity changed")
            _require_current_entry(
                root,
                name,
                self._backend,
                kind=EntryKind.DIRECTORY,
                identity=identity,
                filesystem=filesystem,
                label="managed child",
            )
            return opened
        except BaseException as primary_error:
            if opened is not None:
                for close_error in _close_capability_retry(
                    opened, "managed child reopen"
                ):
                    primary_error.add_note(close_error)
            raise

    def reopen_for_meter(self) -> DirectoryCapability:
        root = self._root
        if root is None or not root.is_open:
            raise OSError("managed run root capability is unavailable")
        return self._backend.reopen_directory(root)

    def refresh_heartbeat(
        self,
        *,
        deadline: float | None = None,
        monotonic: Callable[[], float] | None = None,
    ) -> None:
        heartbeat = self._heartbeat
        root = self._root
        if (
            heartbeat is None
            or not heartbeat.is_open
            or root is None
            or not root.is_open
        ):
            raise OSError("managed heartbeat capability is unavailable")
        clock = time.monotonic if monotonic is None else monotonic

        def check_deadline() -> None:
            if deadline is not None:
                _check_absolute_deadline(
                    deadline, clock, "managed heartbeat deadline"
                )

        _require_current_entry(
            root,
            HEARTBEAT_FILE,
            self._backend,
            kind=EntryKind.REGULAR,
            identity=self._heartbeat_identity,
            filesystem=root.filesystem,
            label="heartbeat",
        )
        check_deadline()
        self._backend.touch(heartbeat)
        check_deadline()
        self._backend.flush(heartbeat)
        check_deadline()
        _require_current_entry(
            root,
            HEARTBEAT_FILE,
            self._backend,
            kind=EntryKind.REGULAR,
            identity=self._heartbeat_identity,
            filesystem=root.filesystem,
            label="heartbeat",
        )
        check_deadline()

    def remove_child(self, child: Path) -> ScratchCleanupRecord:
        with self._registry_lock:
            if self._registry_frozen:
                raise RuntimeError("scratch registry is frozen")
            record = self._remove_child_unlocked(child)
            self._registry_generation += 1
            return record

    def _namespace_cleanup_unavailable(
        self,
        message: str,
        *,
        examined: int,
        removed: int,
        owners: tuple[FileCapability | DirectoryCapability, ...] = (),
        details: tuple[str, ...] = (),
    ) -> ScratchCleanupRecord:
        graph = self._cleanup_graph
        if graph is None:
            raise RuntimeError("cleanup owner graph is unavailable")
        for owner in owners:
            if owner.is_open:
                graph.blocked.retain(owner)
                registry = self._cleanup_owned_capabilities
                if isinstance(registry, _FixedOwnerRegistry):
                    registry.retain(owner)
        bounded = _bounded_secondary(
            "managed namespace cleanup unavailable",
            RuntimeError("an owned capability remains open"),
        )
        graph.details.add_many(details)
        graph.details.add(bounded)
        self._close_cleanup_graph(retry=True)
        lease = self._lease
        if lease is not None:
            for detail in _close_lease_lock_retry(lease, "managed lease"):
                graph.details.add(detail)
            if lease.fd < 0:
                self._lease = None
        for label, attribute in (
            ("heartbeat", "_heartbeat"),
            ("root", "_root"),
            ("managed root", "_managed_root_capability"),
        ):
            capability = cast(
                FileCapability | DirectoryCapability | None,
                getattr(self, attribute),
            )
            if capability is None:
                continue
            for detail in _close_capability_retry(
                capability, f"managed {label}"
            ):
                graph.details.add(detail)
            if not capability.is_open:
                setattr(self, attribute, None)
        return ScratchCleanupRecord(
            ScratchCleanupStatus.FAILED,
            examined,
            removed,
            _cleanup_record_details(graph.details, message),
            validate_reported_path(self.path),
            graph.details.omitted,
        )

    def _remove_child_unlocked(self, child: Path) -> ScratchCleanupRecord:
        if child.parent != self.path or _CHILD_NAME.fullmatch(child.name) is None:
            return self._failed("cleanup target is not a direct managed child")
        expected = self._children.get(child.name)
        if expected is None:
            return self._failed("cleanup target is not an owned managed child")
        graph = self._ensure_cleanup_graph()
        if self._cleanup_graph_has_blocked_owner():
            return self._namespace_cleanup_unavailable(
                "managed child cleanup owner remains open",
                examined=0,
                removed=0,
            )
        started = time.monotonic()
        absolute_deadline = started + OWNER_CLEANUP_SECONDS
        examined = 0
        removed = 0
        root = self._root
        if root is None or not root.is_open:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                0,
                0,
                ("managed run root capability is unavailable",),
                validate_reported_path(self.path),
            )

        def deferred(*details: str) -> ScratchCleanupRecord:
            primary = details[0]
            return ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                examined,
                removed,
                _cleanup_record_details(
                    graph.details,
                    primary,
                    secondary=tuple(details[1:]),
                ),
                validate_reported_path(self.path),
                graph.details.omitted,
            )

        def check(label: str) -> None:
            _check_absolute_deadline(
                absolute_deadline, time.monotonic, label
            )

        expected_identity = expected.identity
        expected_filesystem = expected.filesystem
        if expected_identity is None or expected_filesystem is None:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                0,
                0,
                ("managed child registration is incomplete",),
                validate_reported_path(self.path),
            )
        child_owner = cast(
            DirectoryCapability | None, expected.root_owner.owner
        )
        cursor = expected.cursor
        try:
            if expected.pending_absence.committed:
                removed = max(
                    removed, expected.pending_absence.removed_after
                )
                if child_owner is not None and child_owner.is_open:
                    close_errors = _close_cleanup_capability(
                        child_owner, "managed child consumed owner"
                    )
                    graph.details.add_many(close_errors)
                    if child_owner.is_open:
                        return self._namespace_cleanup_unavailable(
                            "managed child consumed owner remains open",
                            examined=examined,
                            removed=removed,
                            owners=(child_owner,),
                        )
                    expected.root_owner.owner = None
                    child_owner = None
                check("managed child absence")
                remaining = self._backend.entry(root, child.name)
                check("managed child absence")
                if remaining is not None:
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        examined,
                        removed,
                        _cleanup_record_details(
                            graph.details,
                            "managed child was replaced after exact removal",
                        ),
                        validate_reported_path(self.path),
                        graph.details.omitted,
                    )
                expected.pending_absence.clear()
                self._children.pop(child.name, None)
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.CLEAN,
                    examined,
                    removed,
                    graph.details.details(),
                    omitted_detail_count=graph.details.omitted,
                )

            if child_owner is None and not cursor.components:
                check("managed child entry lookup")
                current = self._backend.entry(root, child.name)
                check("managed child entry lookup")
                if current is None:
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        0,
                        0,
                        ("managed child is missing before exact removal",),
                        validate_reported_path(self.path),
                    )
                if (
                    current.kind is not EntryKind.DIRECTORY
                    or current.identity != expected_identity
                    or current.filesystem != expected_filesystem
                ):
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        0,
                        0,
                        ("managed child identity changed",),
                        validate_reported_path(self.path),
                    )
                check("managed child open")
                child_owner = self._backend.open_directory(
                    root, child.name, SharePolicy.PINNED
                )
                expected.root_owner.owner = child_owner
                try:
                    _validate_cleanup_directory(
                        child_owner,
                        _CleanupComponent(
                            child.name,
                            expected_identity,
                            expected_filesystem,
                        ),
                        label="managed child",
                    )
                    check("managed child open")
                except BaseException as primary_error:
                    close_errors = _close_cleanup_capability(
                        child_owner, "managed child"
                    )
                    if child_owner.is_open:
                        raise _CleanupOwnershipBlocked(
                            "managed child owner remains open",
                            (child_owner,),
                            close_errors,
                            primary=primary_error,
                        ) from primary_error
                    for detail in close_errors:
                        primary_error.add_note(detail)
                    raise

            while True:
                slice_started = time.monotonic()
                before = (examined, removed, cursor)
                result = _remove_payload(
                    self._backend,
                    root,
                    child_owner,
                    root_name=child.name,
                    root_identity=expected_identity,
                    root_filesystem=expected_filesystem,
                    cursor=cursor,
                    started=slice_started,
                    absolute_deadline=absolute_deadline,
                    examined=examined,
                    removed=removed,
                    owner_graph=graph,
                    pending_absence=expected.pending_absence,
                )
                child_owner = result.root
                expected.root_owner.owner = child_owner
                examined = result.examined_entries
                removed = result.removed_entries
                cursor = result.cursor
                expected.cursor = cursor
                if result.blocked_owners:
                    return self._namespace_cleanup_unavailable(
                        "managed child cleanup owner remains open",
                        examined=examined,
                        removed=removed,
                        owners=result.blocked_owners,
                        details=result.details,
                    )
                graph.details.add_many(result.details)
                if result.complete:
                    break
                if time.monotonic() >= absolute_deadline:
                    return deferred("managed child cleanup deadline reached")
                if (
                    (examined, removed, cursor) == before
                ):
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        examined,
                        removed,
                        _cleanup_record_details(
                            graph.details,
                            "managed child cleanup slice made no progress",
                        ),
                        validate_reported_path(self.path),
                        graph.details.omitted,
                    )

            assert child_owner is not None
            check("managed child delete")
            removed_after = removed + 1
            expected.pending_absence.arm(
                scope="child",
                name=child.name,
                identity=expected_identity,
                filesystem=expected_filesystem,
                removed_after=removed_after,
            )
            try:
                self._backend.delete(child_owner)
            except BaseException as primary_error:
                if child_owner.is_open:
                    expected.pending_absence.clear()
                else:
                    expected.pending_absence.commit()
                    removed = removed_after
                close_errors = _close_cleanup_capability(
                    child_owner, "managed child"
                )
                if child_owner.is_open:
                    return self._namespace_cleanup_unavailable(
                        f"managed child delete failed: {primary_error}",
                        examined=examined,
                        removed=removed,
                        owners=(child_owner,),
                        details=close_errors,
                    )
                for detail in close_errors:
                    primary_error.add_note(detail)
                raise
            else:
                expected.pending_absence.commit()
                removed = removed_after
            if child_owner.is_open:
                close_errors = _close_cleanup_capability(
                    child_owner, "managed child"
                )
                graph.details.add_many(close_errors)
                if child_owner.is_open:
                    return self._namespace_cleanup_unavailable(
                        "managed child owner remains open after delete",
                        examined=examined,
                        removed=removed,
                        owners=(child_owner,),
                    )
            removed = removed_after
            child_owner = None
            expected.root_owner.owner = None
            if time.monotonic() >= absolute_deadline:
                return deferred("managed child absence deadline reached")
            check("managed child absence")
            remaining = self._backend.entry(root, child.name)
            check("managed child absence")
            if remaining is not None:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    examined,
                    removed,
                    _cleanup_record_details(
                        graph.details,
                        "managed child was replaced after exact removal",
                    ),
                    validate_reported_path(self.path),
                    graph.details.omitted,
                )
            expected.pending_absence.clear()
            self._children.pop(child.name, None)
            return ScratchCleanupRecord(
                ScratchCleanupStatus.CLEAN,
                examined,
                removed,
                graph.details.details(),
                omitted_detail_count=graph.details.omitted,
            )
        except _DeadlineExceeded as error:
            if child_owner is not None and child_owner.is_open:
                expected.root_owner.owner = child_owner
            expected.cursor = cursor
            return ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                examined,
                removed,
                _cleanup_record_details(
                    graph.details,
                    f"{type(error).__name__}: {error}",
                    error=error,
                ),
                validate_reported_path(self.path),
                graph.details.omitted,
            )
        except _CleanupOwnershipBlocked as error:
            if (
                expected.pending_absence.committed
                and expected.pending_absence.scope
                in {"payload", "directory"}
            ):
                expected.cursor = _CleanupCursor(
                    expected.pending_absence.parent_components
                )
                removed = max(
                    removed, expected.pending_absence.removed_after
                )
            _record_cleanup_exception_notes(graph.details, error.primary)
            primary, blocked_details = _cleanup_blocked_report(error)
            return self._namespace_cleanup_unavailable(
                primary,
                examined=examined,
                removed=removed,
                owners=error.owners,
                details=blocked_details,
            )
        except FileNotFoundError as error:
            _record_cleanup_exception_notes(graph.details, error)
            if time.monotonic() >= absolute_deadline:
                return deferred("managed child recovery deadline reached")
            try:
                check("managed child recovery evidence")
                recovered = self._backend.entry(root, child.name)
                check("managed child recovery evidence")
            except _DeadlineExceeded as recovery_deadline:
                _record_cleanup_exception_notes(
                    graph.details, recovery_deadline
                )
                return deferred("managed child recovery deadline reached")
            except OSError as recovery_error:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    examined,
                    removed,
                    _cleanup_record_details(
                        graph.details,
                        "managed child recovery evidence failed: "
                        f"{type(recovery_error).__name__}: {recovery_error}",
                        error=recovery_error,
                    ),
                    validate_reported_path(self.path),
                    graph.details.omitted,
                )
            primary = (
                "managed child traversal entry vanished"
                if recovered is None
                else "managed child traversal entry changed while opening"
            )
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                examined,
                removed,
                _cleanup_record_details(
                    graph.details,
                    primary,
                    error=error,
                    secondary=(f"{type(error).__name__}: {error}",),
                ),
                validate_reported_path(self.path),
                graph.details.omitted,
            )
        except BaseException as error:
            child_cleanup_close_errors: tuple[str, ...] = ()
            if child_owner is not None and child_owner.is_open:
                child_cleanup_close_errors = _close_cleanup_capability(
                    child_owner, "managed child"
                )
                if child_owner.is_open:
                    return self._namespace_cleanup_unavailable(
                        f"managed child cleanup failed: {type(error).__name__}: {error}",
                        examined=examined,
                        removed=removed,
                        owners=(child_owner,),
                        details=child_cleanup_close_errors,
                    )
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                examined,
                removed,
                _cleanup_record_details(
                    graph.details,
                    f"{type(error).__name__}: {error}",
                    error=error,
                    secondary=child_cleanup_close_errors,
                ),
                validate_reported_path(self.path),
                graph.details.omitted,
            )

    def mark_cleanup_ready(self) -> None:
        if self._cleanup_ready:
            return
        root = self._root
        if root is None or not root.is_open:
            raise OSError("managed run root capability is unavailable")
        _identity, descriptor = _create_marker(
            root,
            CLEANUP_READY_FILE,
            _marker(self.run_id, self.lease_id),
            self._backend,
        )
        owner = _OwnedDescriptor()
        owner.adopt(descriptor)
        errors = owner.close_retry("managed cleanup-ready marker")
        if errors:
            raise OSError("; ".join(errors))
        self._cleanup_ready = True

    def retain(self) -> ScratchCleanupRecord:
        root = self._root
        if root is None or not root.is_open:
            raise OSError("managed run root capability is unavailable")
        existing = self._backend.entry(root, RETAIN_FILE)
        if existing is None:
            _identity, descriptor = _create_marker(
                root,
                RETAIN_FILE,
                _marker(self.run_id, self.lease_id),
                self._backend,
            )
            owner = _OwnedDescriptor()
            owner.adopt(descriptor)
            errors = owner.close_retry("managed retain marker")
            if errors:
                raise OSError("; ".join(errors))
        elif _read_marker(
            root,
            RETAIN_FILE,
            self._backend,
            expected_run_id=self.run_id,
            expected_lease_id=self.lease_id,
        ) is None:
            raise OSError("managed retain marker disappeared")
        return ScratchCleanupRecord(
            ScratchCleanupStatus.RETAINED,
            0,
            0,
            remaining_root=validate_reported_path(self.path),
        )

    def defer(self, detail: str) -> ScratchCleanupRecord:
        """Abandon this root without making it permanently retained.

        No cleanup-ready marker is published because the caller could not prove
        quiescence.  Once this process releases the lease, the startup janitor
        may reclaim the root only after the normal heartbeat grace period.
        """
        return ScratchCleanupRecord(
            ScratchCleanupStatus.DEFERRED,
            0,
            0,
            (detail,),
            remaining_root=validate_reported_path(self.path),
        )

    def cleanup(
        self,
        *,
        time_budget: float = OWNER_CLEANUP_SECONDS,
    ) -> ScratchCleanupRecord:
        with self._registry_lock:
            if self._registry_frozen:
                raise RuntimeError("scratch registry is frozen")
            record = self._cleanup_unlocked(time_budget=time_budget)
            self._registry_generation += 1
            return record

    def _cleanup_unlocked(
        self,
        *,
        time_budget: float,
    ) -> ScratchCleanupRecord:
        graph = self._ensure_cleanup_graph()
        if self._cleanup_graph_has_blocked_owner():
            return self._namespace_cleanup_unavailable(
                "managed cleanup owner remains open",
                examined=0,
                removed=0,
            )
        started = time.monotonic()
        absolute_deadline = started + time_budget
        deleting = self.managed_root / f".deleting-{self.run_id}"
        validate_reported_path(deleting)
        examined = 0
        removed = 0

        def deferred(*details: str) -> ScratchCleanupRecord:
            primary = details[0]
            return ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                examined,
                removed,
                _cleanup_record_details(
                    graph.details,
                    primary,
                    secondary=tuple(details[1:]),
                ),
                remaining_root=(
                    None
                    if graph.pending_absence.committed
                    and graph.pending_absence.scope == "root"
                    else validate_reported_path(self.path)
                ),
                omitted_detail_count=graph.details.omitted,
            )

        def check(label: str) -> None:
            _check_absolute_deadline(
                absolute_deadline, time.monotonic, label
            )

        managed_root_capability = self._managed_root_capability
        if (
            managed_root_capability is None
            or not managed_root_capability.is_open
        ):
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                0,
                0,
                ("managed root capability is unavailable",),
                validate_reported_path(self.path),
            )

        def close_coordinator(
            coordinator: LeaseLock, label: str
        ) -> tuple[str, ...]:
            return _close_lease_lock_retry(coordinator, label)

        def pin_claimed_root() -> None:
            old_root = self._root
            if old_root is not None and old_root.is_open and (
                old_root.share_policy is SharePolicy.PINNED
            ):
                return
            if old_root is not None and not old_root.is_open:
                raise OSError("managed claimed root capability is unavailable")
            check("cleanup pinned root open")
            pinned_root = self._backend.open_directory(
                managed_root_capability,
                deleting.name,
                SharePolicy.PINNED,
            )
            graph.walker_current.owner = pinned_root
            try:
                _validate_cleanup_directory(
                    pinned_root,
                    _CleanupComponent(
                        deleting.name,
                        self._root_identity,
                        self._root_filesystem,
                    ),
                    label="managed claimed root",
                )
                check("cleanup pinned root validation")
            except BaseException as primary_error:
                close_errors = _close_capability_retry(
                    pinned_root, "managed claimed root"
                )
                if not pinned_root.is_open:
                    graph.walker_current.owner = None
                if pinned_root.is_open:
                    raise _CleanupOwnershipBlocked(
                        "managed claimed root owner remains open",
                        (pinned_root,),
                        close_errors,
                        primary=primary_error,
                    ) from primary_error
                for detail in close_errors:
                    primary_error.add_note(detail)
                raise
            if old_root is None:
                self._root = pinned_root
                graph.walker_current.owner = None
                return
            old_errors = _close_capability_retry(
                old_root, "managed claimed root handoff"
            )
            if old_root.is_open:
                pinned_errors = _close_capability_retry(
                    pinned_root, "managed pinned root"
                )
                if not pinned_root.is_open:
                    graph.walker_current.owner = None
                raise _CleanupOwnershipBlocked(
                    "managed root cleanup handoff failed",
                    tuple(
                        owner
                        for owner in (old_root, pinned_root)
                        if owner.is_open
                    ),
                    (*old_errors, *pinned_errors),
                )
            self._root = pinned_root
            graph.walker_current.owner = None

        try:
            coordinator = _open_coordinator(
                managed_root_capability,
                self._backend,
                timeout=min(5.0, max(0.001, time_budget)),
                deadline=absolute_deadline,
                _owner_slot=graph.claim_coordinator,
            )
        except _DeadlineExceeded as error:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                0,
                0,
                _cleanup_record_details(
                    graph.details,
                    f"{type(error).__name__}: {error}",
                    error=error,
                ),
                validate_reported_path(self.path),
                graph.details.omitted,
            )
        except OSError as error:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                0,
                0,
                _cleanup_record_details(
                    graph.details,
                    f"{type(error).__name__}: {error}",
                    error=error,
                ),
                validate_reported_path(self.path),
                graph.details.omitted,
            )
        claim_record: ScratchCleanupRecord | None = None
        try:
            check("cleanup claim")
            root_pending = graph.pending_absence
            if (
                root_pending.committed
                and root_pending.scope == "root"
            ):
                if root_pending.committed:
                    removed = max(removed, root_pending.removed_after)
                consumed_name = root_pending.name
                check("cleanup root absence")
                remaining_entry = self._backend.entry(
                    managed_root_capability, consumed_name
                )
                check("cleanup root absence")
                if remaining_entry is None:
                    if (
                        root_pending.committed
                        and root_pending.scope == "root"
                    ):
                        root_pending.clear()
                    close_errors = close_coordinator(
                        coordinator, "cleanup claim coordinator"
                    )
                    for detail in (
                        *close_errors,
                        *self.close_capabilities(),
                    ):
                        graph.details.add(detail)
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.CLEAN,
                        examined,
                        removed,
                        graph.details.details(),
                        omitted_detail_count=graph.details.omitted,
                    )
                claim_record = ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    examined,
                    removed,
                    ("managed root was replaced after exact removal",),
                )
            else:
                root = self._root
                resume_without_root = (
                    self._cleanup_cursor is not None
                    and self.path.name == deleting.name
                    and root is None
                )
                if (
                    (root is None or not root.is_open)
                    and not resume_without_root
                ):
                    claim_record = ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        0,
                        0,
                        ("managed run root capability is unavailable",),
                        validate_reported_path(self.path),
                    )
                else:
                    active_name = f"run-{self.run_id}"
                    if self.path.name not in {active_name, deleting.name}:
                        claim_record = self._failed_precondition(
                            "managed root name changed", absolute_deadline
                        )
                    else:
                        check("cleanup root evidence")
                        current = self._backend.entry(
                            managed_root_capability, self.path.name
                        )
                        check("cleanup root evidence")
                        if current is None:
                            claim_record = ScratchCleanupRecord(
                                ScratchCleanupStatus.FAILED,
                                0,
                                0,
                                ("managed root is missing before exact removal",),
                                validate_reported_path(self.path),
                            )
                        elif (
                            current.kind is not EntryKind.DIRECTORY
                            or current.identity != self._root_identity
                            or current.filesystem != self._root_filesystem
                        ):
                            claim_record = self._failed_precondition(
                                "managed root identity changed",
                                absolute_deadline,
                            )

                if claim_record is None and self.path.name == active_name:
                    check("cleanup destination evidence")
                    destination = self._backend.entry(
                        managed_root_capability, deleting.name
                    )
                    check("cleanup destination evidence")
                    if destination is not None:
                        claim_record = self._failed_precondition(
                            "managed deleting destination already exists",
                            absolute_deadline,
                        )

                if (
                    claim_record is None
                    and self.path.name == active_name
                    and self._backend.directory_rename_requires_closed_descendants
                ):
                    heartbeat = self._heartbeat
                    if heartbeat is not None:
                        heartbeat_errors = _close_capability_retry(
                            heartbeat, "managed heartbeat cleanup handoff"
                        )
                        for detail in heartbeat_errors:
                            graph.details.add(detail)
                        if heartbeat.is_open:
                            close_coordinator(
                                coordinator, "cleanup claim coordinator"
                            )
                            return self._namespace_cleanup_unavailable(
                                "managed heartbeat cleanup handoff failed",
                                examined=0,
                                removed=0,
                                owners=(heartbeat,),
                                details=heartbeat_errors,
                            )
                        self._heartbeat = None
                    lease = self._lease
                    if lease is not None:
                        lease_errors = _close_lease_lock_retry(
                            lease, "managed lease cleanup handoff"
                        )
                        for detail in lease_errors:
                            graph.details.add(detail)
                        if lease.fd >= 0:
                            close_coordinator(
                                coordinator, "cleanup claim coordinator"
                            )
                            return self._namespace_cleanup_unavailable(
                                "managed lease cleanup handoff failed",
                                examined=0,
                                removed=0,
                                details=lease_errors,
                            )
                        self._lease = None

                if claim_record is None and self.path.name == active_name:
                    check("cleanup root rename")
                    assert self._root is not None
                    self._backend.rename(
                        self._root,
                        managed_root_capability,
                        deleting.name,
                        replace=False,
                    )
                    self.path = deleting
                    if time.monotonic() >= absolute_deadline:
                        claim_record = deferred(
                            "cleanup claim deadline reached after rename"
                        )

                if claim_record is None:
                    check("cleanup claimed root evidence")
                    claimed = self._backend.entry(
                        managed_root_capability, deleting.name
                    )
                    check("cleanup claimed root evidence")
                    if (
                        claimed is None
                        or claimed.kind is not EntryKind.DIRECTORY
                        or claimed.identity != self._root_identity
                        or claimed.filesystem != self._root_filesystem
                    ):
                        claim_record = ScratchCleanupRecord(
                            ScratchCleanupStatus.FAILED,
                            0,
                            0,
                            (
                                "managed root identity changed while "
                                "claiming cleanup",
                            ),
                            validate_reported_path(self.path),
                        )

                pending_marker_name = (
                    graph.pending_absence.name
                    if graph.pending_absence.committed
                    and graph.pending_absence.scope == "marker"
                    else None
                )
                lease_tail_complete = (
                    graph.lease_removed
                    or pending_marker_name == LEASE_FILE
                )
                heartbeat_tail_complete = (
                    graph.heartbeat_removed
                    or pending_marker_name == HEARTBEAT_FILE
                )
                if (
                    claim_record is None
                    and self._backend.directory_rename_requires_closed_descendants
                    and (
                        (
                            not lease_tail_complete
                            and self._lease is None
                        )
                        or (
                            not heartbeat_tail_complete
                            and self._heartbeat is None
                        )
                    )
                ):
                    assert self._root is not None
                    marker = _marker(self.run_id, self.lease_id)
                    pin_claimed_root()
                    if not lease_tail_complete:
                        live_lease = self._lease
                        if live_lease is not None and live_lease.fd < 0:
                            self._lease = None
                            live_lease = None
                        if live_lease is None:
                            check("cleanup lease verification")
                            lease_read = _read_marker(
                                self._root,
                                LEASE_FILE,
                                self._backend,
                                expected_run_id=self.run_id,
                                expected_lease_id=self.lease_id,
                                deadline=absolute_deadline,
                                monotonic=time.monotonic,
                                _owner_slot=graph.lease_read,
                            )
                            if lease_read != (self._lease_identity, marker):
                                raise OSError("managed cleanup lease changed")
                            lease_slot = graph.restored_lease
                            restored_lease = lease_slot.lock
                            assert restored_lease is not None
                            restored_lease._prepare_reuse()
                            check("cleanup lease reopen")
                            lease_capability = _open_owned_marker(
                                self._root,
                                LEASE_FILE,
                                self._backend,
                                access=FileAccess.READ_WRITE,
                                identity=self._lease_identity,
                                deadline=absolute_deadline,
                                monotonic=time.monotonic,
                                _owner_slot=lease_slot,
                            )
                            descriptor_owner = lease_slot.descriptor_owner()
                            try:
                                descriptor_owner.adopt(
                                    lease_capability.detach_to_fd(
                                        os.O_RDWR
                                        | getattr(os, "O_BINARY", 0)
                                    )
                                )
                                lease_slot.capability = None
                                check("cleanup lease detach")
                                restored_lease.fd = descriptor_owner.detach()
                                restored_lease.acquire(blocking=False)
                                check("cleanup lease lock")
                                if _read_locked_marker(
                                    restored_lease,
                                    expected_run_id=self.run_id,
                                    expected_lease_id=self.lease_id,
                                    deadline=absolute_deadline,
                                    monotonic=time.monotonic,
                                ) != marker:
                                    raise OSError(
                                        "locked managed cleanup lease changed"
                                    )
                            except BaseException as primary_error:
                                if lease_capability.is_open:
                                    for detail in _close_capability_retry(
                                        lease_capability,
                                        "managed cleanup lease",
                                    ):
                                        primary_error.add_note(detail)
                                if descriptor_owner.fd >= 0:
                                    for detail in descriptor_owner.close_retry(
                                        "managed cleanup lease"
                                    ):
                                        primary_error.add_note(detail)
                                for detail in _close_lease_lock_retry(
                                    restored_lease, "managed cleanup lease"
                                ):
                                    primary_error.add_note(detail)
                                raise
                            self._lease = restored_lease
                        else:
                            check("cleanup live lease validation")
                            if _read_locked_marker(
                                live_lease,
                                expected_run_id=self.run_id,
                                expected_lease_id=self.lease_id,
                                deadline=absolute_deadline,
                                monotonic=time.monotonic,
                            ) != marker:
                                raise OSError(
                                    "locked managed cleanup lease changed"
                                )

                    if not heartbeat_tail_complete:
                        live_heartbeat = self._heartbeat
                        if (
                            live_heartbeat is not None
                            and not live_heartbeat.is_open
                        ):
                            self._heartbeat = None
                            live_heartbeat = None
                        if live_heartbeat is None:
                            check("cleanup heartbeat verification")
                            heartbeat_read = _read_marker(
                                self._root,
                                HEARTBEAT_FILE,
                                self._backend,
                                expected_run_id=self.run_id,
                                expected_lease_id=self.lease_id,
                                deadline=absolute_deadline,
                                monotonic=time.monotonic,
                                _owner_slot=graph.heartbeat_read,
                            )
                            if heartbeat_read != (
                                self._heartbeat_identity,
                                marker,
                            ):
                                raise OSError(
                                    "managed cleanup heartbeat changed"
                                )
                            check("cleanup heartbeat reopen")
                            live_heartbeat = _open_owned_marker(
                                self._root,
                                HEARTBEAT_FILE,
                                self._backend,
                                access=FileAccess.WRITE,
                                identity=self._heartbeat_identity,
                                deadline=absolute_deadline,
                                monotonic=time.monotonic,
                                _owner_slot=graph.restored_heartbeat,
                            )
                            self._heartbeat = live_heartbeat
                        else:
                            check("cleanup live heartbeat validation")
                            _require_current_entry(
                                self._root,
                                HEARTBEAT_FILE,
                                self._backend,
                                kind=EntryKind.REGULAR,
                                identity=self._heartbeat_identity,
                                filesystem=self._root_filesystem,
                                label="managed cleanup heartbeat",
                            )
                            check("cleanup live heartbeat validation")

                if claim_record is None:
                    pin_claimed_root()
        except _CleanupOwnershipBlocked as error:
            _record_cleanup_exception_notes(graph.details, error.primary)
            primary, blocked_details = _cleanup_blocked_report(error)
            close_coordinator(coordinator, "cleanup claim coordinator")
            return self._namespace_cleanup_unavailable(
                primary,
                examined=0,
                removed=0,
                owners=error.owners,
                details=blocked_details,
            )
        except _DeadlineExceeded as error:
            _record_cleanup_exception_notes(graph.details, error)
            claim_record = deferred(f"{type(error).__name__}: {error}")
        except BaseException as error:
            _record_cleanup_exception_notes(graph.details, error)
            message = (
                f"cleanup claim failed: {type(error).__name__}: {error}"
            )
            if time.monotonic() >= absolute_deadline:
                claim_record = ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    0,
                    0,
                    (message,),
                )
            else:
                claim_record = self._failed_precondition(
                    message, absolute_deadline
                )
        coordinator_errors = close_coordinator(
            coordinator, "cleanup claim coordinator"
        )
        for detail in coordinator_errors:
            graph.details.add(detail)
        claim_owner_blocked = (
            graph.claim_coordinator.has_open_owner()
            or graph.lease_read.has_open_owner()
            or graph.heartbeat_read.has_open_owner()
            or (
                graph.restored_lease.capability is not None
                and graph.restored_lease.capability.is_open
            )
            or (
                graph.restored_lease.descriptor is not None
                and graph.restored_lease.descriptor.fd >= 0
            )
            or (
                graph.restored_lease.lock is not None
                and graph.restored_lease.lock.fd >= 0
                and graph.restored_lease.lock is not self._lease
            )
            or (
                graph.restored_heartbeat.capability is not None
                and graph.restored_heartbeat.capability.is_open
                and graph.restored_heartbeat.capability
                is not self._heartbeat
            )
        )
        if claim_owner_blocked:
            return self._namespace_cleanup_unavailable(
                (
                    "managed cleanup claim owner remains open"
                    if claim_record is None
                    else claim_record.details[0]
                ),
                examined=0,
                removed=0,
                details=(
                    *(
                        ()
                        if claim_record is None
                        else claim_record.details[1:]
                    ),
                ),
            )
        if claim_record is not None:
            if claim_record.details:
                claim_record = replace(
                    claim_record,
                    details=_cleanup_record_details(
                        graph.details,
                        claim_record.details[0],
                        secondary=claim_record.details[1:],
                    ),
                    omitted_detail_count=graph.details.omitted,
                    remaining_root=claim_record.remaining_root,
                )
            return claim_record

        cursor = self._cleanup_cursor or _CleanupCursor()
        root_owner = self._root
        self._root = None
        try:
            while True:
                before = (examined, removed, cursor)
                result = _remove_payload(
                    self._backend,
                    managed_root_capability,
                    root_owner,
                    root_name=deleting.name,
                    root_identity=self._root_identity,
                    root_filesystem=self._root_filesystem,
                    cursor=cursor,
                    started=time.monotonic(),
                    absolute_deadline=absolute_deadline,
                    examined=examined,
                    removed=removed,
                    owner_graph=graph,
                    pending_absence=graph.pending_absence,
                )
                root_owner = result.root
                examined = result.examined_entries
                removed = result.removed_entries
                cursor = result.cursor
                if result.blocked_owners:
                    return self._namespace_cleanup_unavailable(
                        "managed payload cleanup owner remains open",
                        examined=examined,
                        removed=removed,
                        owners=result.blocked_owners,
                        details=result.details,
                    )
                graph.details.add_many(result.details)
                if result.complete:
                    break
                if time.monotonic() >= absolute_deadline:
                    self._cleanup_cursor = cursor
                    if root_owner is not None:
                        self._root = root_owner
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        examined,
                        removed,
                        graph.details.details(),
                        remaining_root=validate_reported_path(deleting),
                        omitted_detail_count=graph.details.omitted,
                    )
                if (examined, removed, cursor) == before:
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        examined,
                        removed,
                        _cleanup_record_details(
                            graph.details,
                            "cleanup slice made no progress",
                        ),
                        validate_reported_path(deleting),
                        graph.details.omitted,
                    )
            self._cleanup_cursor = None
            assert root_owner is not None
            self._root = root_owner
            remaining = absolute_deadline - time.monotonic()
            if remaining <= 0:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    examined,
                    removed,
                    graph.details.details(),
                    remaining_root=validate_reported_path(deleting),
                    omitted_detail_count=graph.details.omitted,
                )
            try:
                tail_managed_root_capability = self._managed_root_capability
                if tail_managed_root_capability is None:
                    raise OSError("managed root capability is unavailable")
                tail_coordinator = _open_coordinator(
                    tail_managed_root_capability,
                    self._backend,
                    timeout=min(5.0, max(0.001, remaining)),
                    deadline=absolute_deadline,
                    _owner_slot=graph.tail_coordinator,
                )
            except _DeadlineExceeded as error:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    examined,
                    removed,
                    _cleanup_record_details(
                        graph.details,
                        f"{type(error).__name__}: {error}",
                        error=error,
                    ),
                    validate_reported_path(deleting),
                    graph.details.omitted,
                )
            except OSError as error:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    examined,
                    removed,
                    _cleanup_record_details(
                        graph.details,
                        f"{type(error).__name__}: {error}",
                        error=error,
                    ),
                    validate_reported_path(deleting),
                    graph.details.omitted,
                )
            tail_record: ScratchCleanupRecord | None = None
            root_delete_started = False
            try:
                for marker_name in (
                    RETAIN_FILE,
                    HEARTBEAT_FILE,
                    CLEANUP_READY_FILE,
                    LEASE_FILE,
                ):
                    if (
                        marker_name == HEARTBEAT_FILE
                        and self._heartbeat is not None
                    ):
                        marker_errors = _close_capability_retry(
                            self._heartbeat, "managed heartbeat cleanup"
                        )
                        for detail in marker_errors:
                            graph.details.add(detail)
                        if self._heartbeat.is_open:
                            close_coordinator(
                                tail_coordinator, "cleanup coordinator"
                            )
                            return self._namespace_cleanup_unavailable(
                                "managed heartbeat owner remains open",
                                examined=examined,
                                removed=removed,
                                owners=(self._heartbeat,),
                                details=marker_errors,
                            )
                        self._heartbeat = None
                    if marker_name == LEASE_FILE and self._lease is not None:
                        lease_errors = _close_lease_lock_retry(
                            self._lease, "managed lease cleanup"
                        )
                        for detail in lease_errors:
                            graph.details.add(detail)
                        if self._lease.fd >= 0:
                            close_coordinator(
                                tail_coordinator, "cleanup coordinator"
                            )
                            return self._namespace_cleanup_unavailable(
                                "managed lease owner remains open",
                                examined=examined,
                                removed=removed,
                                details=lease_errors,
                            )
                        self._lease = None
                    if graph.marker_removed(marker_name):
                        continue
                    check("cleanup marker open")
                    marker_pending = graph.pending_absence
                    if (
                        marker_pending.committed
                        and marker_pending.scope == "marker"
                        and marker_pending.name == marker_name
                    ):
                        if marker_pending.committed:
                            removed = max(
                                removed, marker_pending.removed_after
                            )
                        remaining_marker = self._backend.entry(
                            root_owner, marker_name
                        )
                        check("cleanup marker absence")
                        if remaining_marker is not None:
                            raise OSError(
                                "managed marker "
                                f"{marker_name} was replaced after removal"
                            )
                        if (
                            marker_pending.committed
                            and marker_pending.scope == "marker"
                            and marker_pending.name == marker_name
                        ):
                            marker_pending.clear()
                        graph.mark_marker_removed(marker_name)
                        continue
                    try:
                        marker_owner = self._backend.open_entry(
                            root_owner, marker_name, SharePolicy.PINNED
                        )
                        graph.tail_marker.owner = marker_owner
                    except FileNotFoundError:
                        if marker_name in {HEARTBEAT_FILE, LEASE_FILE}:
                            raise OSError(
                                "managed marker "
                                f"{marker_name} is missing before removal"
                            )
                        continue
                    expected_identity = (
                        self._heartbeat_identity
                        if marker_name == HEARTBEAT_FILE
                        else self._lease_identity
                        if marker_name == LEASE_FILE
                        else marker_owner.identity
                    )
                    try:
                        if (
                            marker_owner.kind is not EntryKind.REGULAR
                            or marker_owner.identity != expected_identity
                            or marker_owner.filesystem != self._root_filesystem
                        ):
                            raise OSError(
                                f"managed marker {marker_name} identity changed"
                            )
                        check("cleanup marker delete")
                        removed_after = removed + 1
                        graph.pending_absence.arm(
                            scope="marker",
                            name=marker_name,
                            identity=marker_owner.identity,
                            filesystem=marker_owner.filesystem,
                            removed_after=removed_after,
                        )
                        try:
                            self._backend.delete(marker_owner)
                        except BaseException:
                            if marker_owner.is_open:
                                graph.pending_absence.clear()
                            else:
                                graph.pending_absence.commit()
                                removed = removed_after
                            raise
                        else:
                            graph.pending_absence.commit()
                            removed = removed_after
                        if marker_owner.is_open:
                            marker_errors = _close_capability_retry(
                                marker_owner, f"managed marker {marker_name}"
                            )
                            if marker_owner.is_open:
                                close_coordinator(
                                    tail_coordinator, "cleanup coordinator"
                                )
                                return self._namespace_cleanup_unavailable(
                                    "managed marker "
                                    f"{marker_name} owner remains open",
                                    examined=examined,
                                    removed=removed,
                                    owners=(marker_owner,),
                                    details=marker_errors,
                                )
                        if not marker_owner.is_open:
                            graph.tail_marker.owner = None
                    except BaseException as primary_error:
                        if marker_owner.is_open:
                            marker_errors = _close_capability_retry(
                                marker_owner, f"managed marker {marker_name}"
                            )
                            if marker_owner.is_open:
                                raise _CleanupOwnershipBlocked(
                                    "managed marker "
                                    f"{marker_name} owner remains open",
                                    (marker_owner,),
                                    marker_errors,
                                    primary=primary_error,
                                ) from primary_error
                            for detail in marker_errors:
                                primary_error.add_note(detail)
                        else:
                            graph.tail_marker.owner = None
                        raise
                    removed = removed_after
                    if time.monotonic() >= absolute_deadline:
                        tail_record = deferred(
                            "cleanup marker absence deadline reached"
                        )
                        break
                    check("cleanup marker absence")
                    remaining_marker = self._backend.entry(
                        root_owner, marker_name
                    )
                    check("cleanup marker absence")
                    if remaining_marker is not None:
                        raise OSError(
                            "managed marker "
                            f"{marker_name} was replaced after removal"
                        )
                    graph.pending_absence.clear()
                    graph.mark_marker_removed(marker_name)

                if tail_record is None:
                    check("cleanup root delete")
                    removed_after = removed + 1
                    graph.pending_absence.arm(
                        scope="root",
                        name=deleting.name,
                        identity=self._root_identity,
                        filesystem=self._root_filesystem,
                        removed_after=removed_after,
                    )
                    root_delete_started = True
                    try:
                        self._backend.delete(root_owner)
                    except BaseException:
                        if root_owner.is_open:
                            graph.pending_absence.clear()
                        else:
                            graph.pending_absence.commit()
                            removed = removed_after
                        raise
                    else:
                        graph.pending_absence.commit()
                        removed = removed_after
                    if root_owner.is_open:
                        root_errors = _close_capability_retry(
                            root_owner, "managed root cleanup"
                        )
                        if root_owner.is_open:
                            close_coordinator(
                                tail_coordinator, "cleanup coordinator"
                            )
                            return self._namespace_cleanup_unavailable(
                                "managed root owner remains open",
                                examined=examined,
                                removed=removed,
                                owners=(root_owner,),
                                details=root_errors,
                            )
                    self._root = None
                    root_owner = None
                    removed = removed_after
                    if time.monotonic() >= absolute_deadline:
                        tail_record = deferred(
                            "cleanup root absence deadline reached"
                        )
                    else:
                        check("cleanup root absence")
                        remaining_root = self._backend.entry(
                            tail_managed_root_capability, deleting.name
                        )
                        check("cleanup root absence")
                        if remaining_root is not None:
                            tail_record = ScratchCleanupRecord(
                                ScratchCleanupStatus.FAILED,
                                examined,
                                removed,
                                ("managed root was replaced after exact removal",),
                            )
                        else:
                            graph.pending_absence.clear()
            except _DeadlineExceeded as error:
                _record_cleanup_exception_notes(graph.details, error)
                tail_record = deferred(f"{type(error).__name__}: {error}")
            except _CleanupOwnershipBlocked as error:
                _record_cleanup_exception_notes(
                    graph.details, error.primary
                )
                primary, blocked_details = _cleanup_blocked_report(error)
                close_coordinator(
                    tail_coordinator, "cleanup coordinator"
                )
                return self._namespace_cleanup_unavailable(
                    primary,
                    examined=examined,
                    removed=removed,
                    owners=error.owners,
                    details=blocked_details,
                )
            except BaseException as error:
                _record_cleanup_exception_notes(graph.details, error)
                message = (
                    f"cleanup tail failed: {type(error).__name__}: {error}"
                )
                recovered = self._failed_precondition(
                    message, absolute_deadline
                )
                tail_record = replace(
                    recovered,
                    examined_entries=examined,
                    removed_entries=removed,
                )
                root_close_errors: tuple[str, ...] = ()
                if root_delete_started:
                    if root_owner is not None and root_owner.is_open:
                        root_close_errors = _close_capability_retry(
                            root_owner, "managed root cleanup failure"
                        )
                        if root_owner.is_open:
                            raise _CleanupOwnershipBlocked(
                                "managed root cleanup failure owner remains open",
                                (root_owner,),
                                root_close_errors,
                                primary=error,
                            ) from error
                    self._root = None
                    root_owner = None
                elif root_owner is not None and root_owner.is_open:
                    self._root = root_owner
                tail_record = replace(
                    tail_record,
                    details=(*tail_record.details, *root_close_errors),
                    remaining_root=tail_record.remaining_root,
                )
            finally:
                tail_close_errors = (
                    ()
                    if tail_coordinator.fd < 0
                    else close_coordinator(
                        tail_coordinator, "cleanup coordinator"
                    )
                )
            for detail in tail_close_errors:
                graph.details.add(detail)
            if graph.tail_coordinator.has_open_owner():
                return self._namespace_cleanup_unavailable(
                    "managed cleanup coordinator owner remains open",
                    examined=examined,
                    removed=removed,
                    details=(
                        () if tail_record is None else tail_record.details
                    ),
                )
            if tail_record is not None:
                primary = (
                    tail_record.details[0]
                    if tail_record.details
                    else "cleanup tail did not complete"
                )
                return replace(
                    tail_record,
                    details=_cleanup_record_details(
                        graph.details,
                        primary,
                        secondary=(
                            *tail_record.details[1:],
                            *tail_close_errors,
                        ),
                    ),
                    omitted_detail_count=graph.details.omitted,
                    remaining_root=tail_record.remaining_root,
                )
            for detail in self.close_capabilities():
                graph.details.add(detail)
            return ScratchCleanupRecord(
                ScratchCleanupStatus.CLEAN,
                examined,
                removed,
                graph.details.details(),
                omitted_detail_count=graph.details.omitted,
            )
        except _CleanupOwnershipBlocked as error:
            if (
                graph.pending_absence.committed
                and graph.pending_absence.scope
                in {"payload", "directory"}
            ):
                self._cleanup_cursor = _CleanupCursor(
                    graph.pending_absence.parent_components
                )
                removed = max(
                    removed, graph.pending_absence.removed_after
                )
            _record_cleanup_exception_notes(graph.details, error.primary)
            primary, blocked_details = _cleanup_blocked_report(error)
            return self._namespace_cleanup_unavailable(
                primary,
                examined=examined,
                removed=removed,
                owners=error.owners,
                details=blocked_details,
            )
        except BaseException as error:
            if (
                graph.pending_absence.committed
                and graph.pending_absence.scope
                in {"payload", "directory"}
            ):
                self._cleanup_cursor = _CleanupCursor(
                    graph.pending_absence.parent_components
                )
                removed = max(
                    removed, graph.pending_absence.removed_after
                )
            if root_owner is not None and root_owner.is_open:
                self._root = root_owner
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                examined,
                removed,
                _cleanup_record_details(
                    graph.details,
                    f"{type(error).__name__}: {error}",
                    error=error,
                ),
                validate_reported_path(deleting),
                graph.details.omitted,
            )

    def _owned_root_path(
        self, absolute_deadline: float | None = None
    ) -> tuple[Path | None, bool, tuple[str, ...]]:
        def expired() -> bool:
            return (
                absolute_deadline is not None
                and time.monotonic() >= absolute_deadline
            )

        if expired():
            return None, False, ("owned root report deadline reached",)
        root = self._root
        anchor = self._managed_root_capability
        if root is None or not root.is_open:
            return None, False, ("owned root capability is unavailable",)
        if anchor is None or not anchor.is_open:
            return None, False, ("managed root capability is unavailable",)
        try:
            capability_path = self._backend.final_path(root)
            validate_reported_path(capability_path)
        except (OSError, UnicodeError, ValueError) as error:
            return None, False, (
                "owned root final-path lookup failed: "
                f"{type(error).__name__}: {error}",
            )
        if expired():
            return None, False, ("owned root report deadline reached",)
        try:
            reported_name = validate_component(capability_path.name)
            reported_path = self.managed_root / reported_name
            validate_reported_path(reported_path)
        except (UnicodeError, ValueError) as error:
            return None, False, (
                "owned root final-path component is invalid: "
                f"{type(error).__name__}: {error}",
            )
        try:
            evidence = self._backend.entry(anchor, reported_name)
        except OSError as error:
            return None, False, (
                "owned root evidence lookup failed: "
                f"{type(error).__name__}: {error}",
            )
        if expired():
            return None, False, ("owned root report deadline reached",)
        if (
            evidence is not None
            and evidence.kind is EntryKind.DIRECTORY
            and evidence.identity == self._root_identity
            and evidence.filesystem == self._root_filesystem
        ):
            self.path = reported_path
            return reported_path, False, ()
        return None, False, (
            "owned root final-path evidence does not match its capability",
        )

    def _failed(
        self, message: str, absolute_deadline: float | None = None
    ) -> ScratchCleanupRecord:
        remaining, proven_absent, lookup_details = self._owned_root_path(
            absolute_deadline
        )
        del proven_absent
        if remaining is None:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                0,
                0,
                (message, *lookup_details),
            )
        return ScratchCleanupRecord(
            ScratchCleanupStatus.FAILED,
            0,
            0,
            (message,),
            validate_reported_path(remaining),
        )

    def _failed_precondition(
        self, message: str, absolute_deadline: float | None = None
    ) -> ScratchCleanupRecord:
        remaining, proven_absent, lookup_details = self._owned_root_path(
            absolute_deadline
        )
        del proven_absent
        return ScratchCleanupRecord(
            ScratchCleanupStatus.FAILED,
            0,
            0,
            (message, *lookup_details),
            (
                None
                if remaining is None
                else validate_reported_path(remaining)
            ),
        )

    def close_capabilities(self) -> tuple[str, ...]:
        errors: list[str] = []
        attempted: set[int] = set()
        errors.extend(
            self._close_cleanup_graph(
                retry=False,
                attempted=attempted,
            )
        )
        registry = self._cleanup_owned_capabilities
        for owner in registry:
            if not owner.is_open:
                continue
            if id(owner) in attempted:
                continue
            attempted.add(id(owner))
            errors.extend(
                _close_capability_once(owner, "managed cleanup owner")
            )
        if isinstance(registry, _FixedOwnerRegistry):
            registry.clear_closed()
        for state in self._children.values():
            child_owner = state.root_owner.owner
            if child_owner is None or not child_owner.is_open:
                continue
            if id(child_owner) in attempted:
                continue
            attempted.add(id(child_owner))
            errors.extend(
                _close_capability_once(
                    child_owner, "managed child cleanup owner"
                )
            )
            if not child_owner.is_open:
                state.root_owner.owner = None
        lease = getattr(self, "_lease", None)
        if lease is not None:
            if id(lease) not in attempted:
                attempted.add(id(lease))
                errors.extend(_close_lease_lock_all(lease, "managed lease"))
            if lease.fd < 0:
                self._lease = None
        for label, attribute in (
            ("heartbeat", "_heartbeat"),
            ("root", "_root"),
            ("managed root", "_managed_root_capability"),
        ):
            capability = getattr(self, attribute, None)
            if capability is None:
                continue
            if id(capability) in attempted:
                if not capability.is_open:
                    setattr(self, attribute, None)
                continue
            attempted.add(id(capability))
            errors.extend(
                _close_capability_once(capability, f"managed {label}")
            )
            if not capability.is_open:
                setattr(self, attribute, None)
        return tuple(errors)

    def __del__(self) -> None:
        try:
            self.close_capabilities()
        except BaseException:
            pass
        graph = getattr(self, "_cleanup_graph", None)
        if graph is not None:
            graph.walker_iterator.owner = None
            for slot in graph.capability_slots():
                slot.owner = None
            for slot in graph.marker_slots():
                slot.capability = None
                slot.descriptor = None
                slot.lock = None
            for slot in (
                graph.claim_coordinator,
                graph.tail_coordinator,
            ):
                slot.capability = None
                del slot.descriptor
                del slot.lock
            graph.blocked.clear_all()
            self._cleanup_graph = None
        registry = getattr(self, "_cleanup_owned_capabilities", ())
        if isinstance(registry, _FixedOwnerRegistry):
            registry.clear_all()
        elif isinstance(registry, list):
            registry.clear()
        children = getattr(self, "_children", {})
        for state in children.values():
            state.root_owner.owner = None
        children.clear()
        for attribute in (
            "_lease",
            "_heartbeat",
            "_root",
            "_managed_root_capability",
        ):
            setattr(self, attribute, None)


@dataclass(frozen=True, slots=True)
class _CleanupComponent:
    name: str
    identity: FileIdentity
    filesystem: FilesystemIdentity

    def encoded_value(self) -> list[str | int]:
        return [
            self.name,
            self.identity.volume,
            self.identity.file,
            self.filesystem.volume,
            self.filesystem.discriminator_a,
            self.filesystem.discriminator_b,
        ]


@dataclass(frozen=True, slots=True)
class _CleanupCursor:
    components: tuple[_CleanupComponent, ...] = ()
    pending_absence: _CleanupComponent | None = None

    def encode(self) -> bytes:
        value = {
            "components": [item.encoded_value() for item in self.components],
            "pending_absence": (
                None
                if self.pending_absence is None
                else self.pending_absence.encoded_value()
            ),
        }
        return json.dumps(
            value,
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")


@dataclass(frozen=True, slots=True)
class _CleanupSlice:
    examined_entries: int
    removed_entries: int
    complete: bool
    cursor: _CleanupCursor
    root: DirectoryCapability | None = None
    blocked_owners: tuple[FileCapability | DirectoryCapability, ...] = ()
    details: tuple[str, ...] = ()


class _CapabilityOwnerSlot:
    __slots__ = ("owner",)

    def __init__(self) -> None:
        self.owner: FileCapability | DirectoryCapability | None = None


class _IteratorOwnerSlot:
    __slots__ = ("owner",)

    def __init__(self) -> None:
        self.owner: DirectoryIterator | None = None


class _PendingAbsenceCell:
    __slots__ = (
        "armed",
        "committed",
        "scope",
        "name",
        "identity",
        "filesystem",
        "parent_components",
        "removed_after",
    )

    def __init__(self) -> None:
        self.armed = False
        self.committed = False
        self.scope = ""
        self.name = ""
        self.identity: FileIdentity | None = None
        self.filesystem: FilesystemIdentity | None = None
        self.parent_components: tuple[_CleanupComponent, ...] = ()
        self.removed_after = 0

    def arm(
        self,
        *,
        scope: str,
        name: str,
        identity: FileIdentity,
        filesystem: FilesystemIdentity,
        parent_components: tuple[_CleanupComponent, ...] = (),
        removed_after: int,
    ) -> None:
        if self.armed:
            if (
                self.scope == scope
                and self.name == name
                and self.identity == identity
                and self.filesystem == filesystem
                and self.parent_components == parent_components
                and self.removed_after == removed_after
            ):
                return
            raise RuntimeError("pending absence cell is already armed")
        self.scope = scope
        self.name = name
        self.identity = identity
        self.filesystem = filesystem
        self.parent_components = parent_components
        self.removed_after = removed_after
        self.committed = False
        self.armed = True

    def commit(self) -> None:
        if not self.armed:
            raise RuntimeError("pending absence cell is not armed")
        self.committed = True

    def clear(self) -> None:
        self.armed = False
        self.committed = False
        self.scope = ""
        self.name = ""
        self.identity = None
        self.filesystem = None
        self.parent_components = ()
        self.removed_after = 0


class _FixedOwnerRegistry:
    __slots__ = ("_owners",)

    def __init__(self, capacity: int = 16) -> None:
        self._owners: list[
            FileCapability | DirectoryCapability | None
        ] = [None] * capacity

    def __iter__(self) -> Iterator[FileCapability | DirectoryCapability]:
        return (
            owner for owner in self._owners if owner is not None
        )

    def retain(
        self, owner: FileCapability | DirectoryCapability
    ) -> bool:
        first_empty = -1
        for index, current in enumerate(self._owners):
            if current is owner:
                return True
            if current is None and first_empty < 0:
                first_empty = index
        if first_empty < 0:
            return False
        self._owners[first_empty] = owner
        return True

    def clear_closed(self) -> None:
        for index, owner in enumerate(self._owners):
            if owner is not None and not owner.is_open:
                self._owners[index] = None

    def clear_all(self) -> None:
        for index in range(len(self._owners)):
            self._owners[index] = None


class _FixedDetailLedger:
    __slots__ = ("_items", "count", "omitted")

    def __init__(self) -> None:
        self._items: list[str | None] = [None] * MAX_DIAGNOSTIC_DETAILS
        self.count = 0
        self.omitted = 0

    def reset(self) -> None:
        for index in range(self.count):
            self._items[index] = None
        self.count = 0
        self.omitted = 0

    def add(self, detail: str) -> None:
        detail = _bounded_diagnostic_detail(detail)
        for index in range(self.count):
            if self._items[index] == detail:
                return
        if self.count >= len(self._items):
            self.omitted += 1
            return
        self._items[self.count] = detail
        self.count += 1

    def add_many(self, details: tuple[str, ...] | list[str]) -> None:
        for detail_index, detail in enumerate(details):
            bounded_detail = _bounded_diagnostic_detail(detail)
            duplicate = False
            for prior_index in range(detail_index):
                if (
                    _bounded_diagnostic_detail(details[prior_index])
                    == bounded_detail
                ):
                    duplicate = True
                    break
            if not duplicate:
                self.add(bounded_detail)

    def reserve_primary(self, primary: str) -> str:
        bounded_primary = _bounded_diagnostic_detail(primary)
        allowed_secondary = max(0, len(self._items) - 1)
        original_count = self.count
        write_index = 0
        secondary_count = 0
        for read_index in range(original_count):
            detail = cast(str, self._items[read_index])
            if detail == bounded_primary:
                self._items[write_index] = detail
                write_index += 1
                continue
            if secondary_count >= allowed_secondary:
                self.omitted += 1
                continue
            self._items[write_index] = detail
            write_index += 1
            secondary_count += 1
        for clear_index in range(write_index, original_count):
            self._items[clear_index] = None
        self.count = write_index
        return bounded_primary

    def details(self) -> tuple[str, ...]:
        return tuple(
            cast(str, self._items[index]) for index in range(self.count)
        )


def _record_cleanup_exception_notes(
    ledger: _FixedDetailLedger,
    error: BaseException | None,
) -> None:
    if error is None:
        return
    notes = getattr(error, "__notes__", None)
    if notes is None:
        return
    ledger.add_many(cast(list[str], notes))


def _cleanup_record_details(
    ledger: _FixedDetailLedger,
    primary: str,
    *,
    error: BaseException | None = None,
    secondary: tuple[str, ...] = (),
) -> tuple[str, ...]:
    _record_cleanup_exception_notes(ledger, error)
    ledger.add_many(secondary)
    bounded_primary = ledger.reserve_primary(primary)
    return (
        bounded_primary,
        *(
            detail
            for detail in ledger.details()
            if detail != bounded_primary
        ),
    )


class _ChildCleanupState:
    __slots__ = (
        "identity",
        "filesystem",
        "cursor",
        "root_owner",
        "pending_absence",
    )

    def __init__(self) -> None:
        self.identity: FileIdentity | None = None
        self.filesystem: FilesystemIdentity | None = None
        self.cursor = _CleanupCursor()
        self.root_owner = _CapabilityOwnerSlot()
        self.pending_absence = _PendingAbsenceCell()

    def __iter__(self) -> Iterator[FileIdentity | FilesystemIdentity]:
        identity = self.identity
        filesystem = self.filesystem
        if identity is None or filesystem is None:
            raise RuntimeError("managed child registration is incomplete")
        yield identity
        yield filesystem


class _CleanupOwnerGraph:
    __slots__ = (
        "claim_coordinator",
        "tail_coordinator",
        "lease_read",
        "heartbeat_read",
        "restored_lease",
        "restored_heartbeat",
        "tail_marker",
        "walker_current",
        "walker_parent",
        "walker_child",
        "walker_entry",
        "walker_iterator",
        "blocked",
        "pending_absence",
        "details",
        "retain_removed",
        "heartbeat_removed",
        "cleanup_ready_removed",
        "lease_removed",
    )

    def __init__(self, blocked: _FixedOwnerRegistry) -> None:
        self.claim_coordinator = _CoordinatorOwnerSlot()
        self.tail_coordinator = _CoordinatorOwnerSlot()
        self.lease_read = _MarkerOwnerSlot(fixed=True)
        self.heartbeat_read = _MarkerOwnerSlot(fixed=True)
        self.restored_lease = _MarkerOwnerSlot(
            fixed=True, with_lock=True
        )
        self.restored_heartbeat = _MarkerOwnerSlot(fixed=True)
        self.tail_marker = _CapabilityOwnerSlot()
        self.walker_current = _CapabilityOwnerSlot()
        self.walker_parent = _CapabilityOwnerSlot()
        self.walker_child = _CapabilityOwnerSlot()
        self.walker_entry = _CapabilityOwnerSlot()
        self.walker_iterator = _IteratorOwnerSlot()
        self.blocked = blocked
        self.pending_absence = _PendingAbsenceCell()
        self.details = _FixedDetailLedger()
        self.retain_removed = False
        self.heartbeat_removed = False
        self.cleanup_ready_removed = False
        self.lease_removed = False

    def capability_slots(self) -> tuple[_CapabilityOwnerSlot, ...]:
        return (
            self.tail_marker,
            self.walker_current,
            self.walker_parent,
            self.walker_child,
            self.walker_entry,
        )

    def marker_slots(self) -> tuple[_MarkerOwnerSlot, ...]:
        return (
            self.lease_read,
            self.heartbeat_read,
            self.restored_lease,
            self.restored_heartbeat,
        )

    def marker_removed(self, name: str) -> bool:
        if name == RETAIN_FILE:
            return self.retain_removed
        if name == HEARTBEAT_FILE:
            return self.heartbeat_removed
        if name == CLEANUP_READY_FILE:
            return self.cleanup_ready_removed
        if name == LEASE_FILE:
            return self.lease_removed
        raise ValueError(f"unsupported cleanup marker: {name!r}")

    def mark_marker_removed(self, name: str) -> None:
        if name == RETAIN_FILE:
            self.retain_removed = True
        elif name == HEARTBEAT_FILE:
            self.heartbeat_removed = True
        elif name == CLEANUP_READY_FILE:
            self.cleanup_ready_removed = True
        elif name == LEASE_FILE:
            self.lease_removed = True
        else:
            raise ValueError(f"unsupported cleanup marker: {name!r}")


class _CleanupOwnershipBlocked(OSError):
    def __init__(
        self,
        message: str,
        owners: tuple[FileCapability | DirectoryCapability, ...],
        details: tuple[str, ...],
        *,
        primary: BaseException | None = None,
    ) -> None:
        super().__init__(message)
        self.owners = owners
        self.details = details
        self.primary = primary


def _cleanup_blocked_report(
    error: _CleanupOwnershipBlocked,
) -> tuple[str, tuple[str, ...]]:
    if error.primary is None:
        return str(error), error.details
    return (
        _bounded_secondary("cleanup failed", error.primary),
        (str(error), *error.details),
    )


def _validate_cleanup_cursor(cursor: _CleanupCursor) -> None:
    if len(cursor.components) > MAX_CLEANUP_DEPTH:
        raise OSError(f"cleanup depth exceeds {MAX_CLEANUP_DEPTH}")
    for component in (
        *cursor.components,
        *((cursor.pending_absence,) if cursor.pending_absence is not None else ()),
    ):
        if validate_component(component.name) != component.name:
            raise OSError("cleanup cursor contains an invalid component")
        if component.identity.volume <= 0 or component.identity.file <= 0:
            raise OSError("cleanup cursor contains an invalid identity")
        if component.filesystem.volume <= 0:
            raise OSError("cleanup cursor contains an invalid filesystem")
    if len(cursor.encode()) > MAX_CLEANUP_CURSOR_BYTES:
        raise OSError(
            f"cleanup cursor exceeds {MAX_CLEANUP_CURSOR_BYTES} encoded bytes"
        )


def _cleanup_observe_directories(count: int) -> None:
    if count > MAX_CLEANUP_OPEN_DIRECTORIES:
        raise OSError(
            "cleanup directory handle limit exceeded: "
            f"{count} > {MAX_CLEANUP_OPEN_DIRECTORIES}"
        )
    _cleanup_handle_observer(count)


def _close_cleanup_capability(
    capability: FileCapability | DirectoryCapability,
    label: str,
) -> tuple[str, ...]:
    return _close_capability_retry(capability, label)


def _close_cleanup_iterator(
    iterator: DirectoryIterator,
    label: str,
) -> tuple[str, ...]:
    errors: list[str] = []
    directory = iterator.directory
    while (
        directory.is_open
        and directory._close_attempts < _CAPABILITY_CLOSE_ATTEMPT_LIMIT
    ):
        try:
            iterator.close()
        except BaseException as error:
            errors.append(_bounded_secondary(f"{label} close failed", error))
        else:
            break
    return tuple(errors)


def _raise_cleanup_blocked(
    message: str,
    owners: tuple[FileCapability | DirectoryCapability, ...],
    details: tuple[str, ...],
) -> None:
    live = tuple(owner for owner in owners if owner.is_open)
    if live:
        raise _CleanupOwnershipBlocked(message, live, details)


def _validate_cleanup_directory(
    directory: DirectoryCapability,
    component: _CleanupComponent,
    *,
    label: str,
) -> None:
    if (
        directory.kind is not EntryKind.DIRECTORY
        or directory.identity != component.identity
        or directory.filesystem != component.filesystem
    ):
        raise OSError(f"{label} identity changed")


def _remove_payload(
    backend: FilesystemBackend,
    anchor: DirectoryCapability,
    root: DirectoryCapability | None,
    *,
    root_name: str,
    root_identity: FileIdentity,
    root_filesystem: FilesystemIdentity,
    cursor: _CleanupCursor,
    started: float,
    absolute_deadline: float,
    examined: int,
    removed: int,
    monotonic: Callable[[], float] | None = None,
    owner_graph: _CleanupOwnerGraph | None = None,
    pending_absence: _PendingAbsenceCell | None = None,
) -> _CleanupSlice:
    """Remove one bounded payload slice using only exact capabilities."""
    _validate_cleanup_cursor(cursor)
    clock = time.monotonic if monotonic is None else monotonic
    pending_cell = (
        _PendingAbsenceCell()
        if pending_absence is None
        else pending_absence
    )
    control = {LEASE_FILE, HEARTBEAT_FILE, CLEANUP_READY_FILE, RETAIN_FILE}
    slice_examined = examined

    def exhausted() -> bool:
        now = clock()
        return (
            examined - slice_examined >= MAX_CLEANUP_SLICE_ENTRIES
            or now - started >= MAX_CLEANUP_SLICE_SECONDS
            or now >= absolute_deadline
        )

    def require_operation(label: str) -> None:
        if exhausted():
            raise _DeadlineExceeded(f"{label} deadline reached")

    def retain_or_transport_details(
        details: tuple[str, ...],
    ) -> tuple[str, ...]:
        if owner_graph is None:
            return details
        owner_graph.details.add_many(details)
        return ()

    def close_or_block(
        owner: FileCapability | DirectoryCapability,
        label: str,
        message: str,
    ) -> tuple[str, ...]:
        errors = _close_cleanup_capability(owner, label)
        transport = retain_or_transport_details(errors)
        _raise_cleanup_blocked(message, (owner,), transport)
        return errors

    def open_cursor(
        supplied_root: DirectoryCapability | None,
        components: tuple[_CleanupComponent, ...],
    ) -> DirectoryCapability:
        require_operation("cleanup cursor reopen")
        current = supplied_root
        if current is None:
            current = backend.open_directory(
                anchor, root_name, SharePolicy.PINNED
            )
        if owner_graph is not None:
            owner_graph.walker_current.owner = current
        _cleanup_observe_directories(2)
        root_component = _CleanupComponent(
            root_name, root_identity, root_filesystem
        )
        try:
            _validate_cleanup_directory(
                current, root_component, label="cleanup root"
            )
            require_operation("cleanup cursor validation")
            for component in components:
                require_operation("cleanup cursor reopen")
                child = backend.open_directory(
                    current, component.name, SharePolicy.PINNED
                )
                if owner_graph is not None:
                    owner_graph.walker_child.owner = child
                _cleanup_observe_directories(3)
                try:
                    _validate_cleanup_directory(
                        child, component, label="cleanup cursor"
                    )
                    require_operation("cleanup cursor validation")
                except BaseException as primary_error:
                    close_errors = _close_cleanup_capability(
                        child, "cleanup cursor child"
                    )
                    transport = retain_or_transport_details(close_errors)
                    if child.is_open:
                        raise _CleanupOwnershipBlocked(
                            "cleanup cursor child owner remains open",
                            (child,),
                            transport,
                            primary=primary_error,
                        ) from primary_error
                    if owner_graph is not None:
                        owner_graph.walker_child.owner = None
                    for detail in transport:
                        primary_error.add_note(detail)
                    raise
                parent_errors = _close_cleanup_capability(
                    current, "cleanup cursor parent"
                )
                parent_transport = retain_or_transport_details(parent_errors)
                if current.is_open:
                    child_errors = _close_cleanup_capability(
                        child, "cleanup cursor child"
                    )
                    child_transport = retain_or_transport_details(child_errors)
                    _raise_cleanup_blocked(
                        "cleanup cursor parent remains open",
                        (current, child),
                        (*parent_transport, *child_transport),
                    )
                if owner_graph is not None:
                    owner_graph.walker_current.owner = child
                    owner_graph.walker_child.owner = None
                current = child
                _cleanup_observe_directories(2)
            return current
        except BaseException as primary_error:
            cursor_close_errors: tuple[str, ...] = ()
            if current.is_open:
                cursor_close_errors = _close_cleanup_capability(
                    current, "cleanup cursor"
                )
            cursor_transport = retain_or_transport_details(
                cursor_close_errors
            )
            if owner_graph is not None and not current.is_open:
                owner_graph.walker_current.owner = None
            if isinstance(primary_error, _CleanupOwnershipBlocked):
                owners = list(primary_error.owners)
                if current.is_open and current not in owners:
                    owners.append(current)
                raise _CleanupOwnershipBlocked(
                    str(primary_error),
                    tuple(owners),
                    (*primary_error.details, *cursor_transport),
                    primary=primary_error.primary,
                ) from primary_error
            if current.is_open:
                raise _CleanupOwnershipBlocked(
                    "cleanup cursor owner remains open",
                    (current,),
                    cursor_transport,
                    primary=primary_error,
                ) from primary_error
            for detail in cursor_transport:
                primary_error.add_note(detail)
            raise

    def finish_iterator(
        iterator: DirectoryIterator,
        next_cursor: _CleanupCursor,
    ) -> _CleanupSlice:
        errors = _close_cleanup_iterator(iterator, "cleanup iterator")
        directory = iterator.directory
        if owner_graph is not None and not directory.is_open:
            owner_graph.walker_iterator.owner = None
        if directory.is_open:
            return _CleanupSlice(
                examined,
                removed,
                False,
                next_cursor,
                blocked_owners=(directory,),
                details=errors,
            )
        return _CleanupSlice(
            examined,
            removed,
            False,
            next_cursor,
            details=errors,
        )

    if exhausted():
        if owner_graph is not None:
            owner_graph.walker_current.owner = root
        deferred = _CleanupSlice(
            examined, removed, False, cursor, root=root
        )
        if owner_graph is not None:
            owner_graph.walker_current.owner = None
        return deferred

    current: DirectoryCapability | None = None
    iterator: DirectoryIterator | None = None
    try:
        if pending_cell.armed and not pending_cell.committed:
            pending_cell.clear()
        resume_components = cursor.components
        if (
            pending_cell.committed
            and pending_cell.scope in {"payload", "directory"}
        ):
            removed = max(removed, pending_cell.removed_after)
            resume_components = pending_cell.parent_components
        current = open_cursor(root, resume_components)
        if owner_graph is not None:
            owner_graph.walker_current.owner = current
        root = None
        if (
            pending_cell.committed
            and pending_cell.scope in {"payload", "directory"}
        ):
            require_operation("cleanup pending absence")
            remaining = backend.entry(current, pending_cell.name)
            require_operation("cleanup pending absence")
            if remaining is not None:
                raise OSError(
                    "cleanup preserved a same-name replacement after removal"
                )
            pending_cell.clear()
            cursor = _CleanupCursor(resume_components)
        if cursor.pending_absence is not None:
            require_operation("cleanup pending absence")
            remaining = backend.entry(
                current, cursor.pending_absence.name
            )
            require_operation("cleanup pending absence")
            if remaining is not None:
                raise OSError(
                    "cleanup preserved a same-name replacement after removal"
                )
            cursor = _CleanupCursor(cursor.components)

        require_operation("cleanup iterator construction")
        iterator = backend.entries_owned(current)
        if owner_graph is not None:
            owner_graph.walker_iterator.owner = iterator
            owner_graph.walker_current.owner = None
        current = None
        _cleanup_observe_directories(2)
        if exhausted():
            return finish_iterator(iterator, cursor)

        while True:
            require_operation("cleanup enumeration")
            try:
                entry = next(iterator)
            except StopIteration:
                close_errors = _close_cleanup_iterator(
                    iterator, "cleanup completed iterator"
                )
                directory_owner = iterator.directory
                if directory_owner.is_open:
                    return _CleanupSlice(
                        examined,
                        removed,
                        False,
                        cursor,
                        blocked_owners=(directory_owner,),
                        details=close_errors,
                    )
                iterator = None
                if not cursor.components:
                    if exhausted():
                        return _CleanupSlice(
                            examined,
                            removed,
                            False,
                            cursor,
                            details=close_errors,
                        )
                    current = open_cursor(None, ())
                    complete = _CleanupSlice(
                        examined,
                        removed,
                        True,
                        _CleanupCursor(),
                        root=current,
                        details=close_errors,
                    )
                    if owner_graph is not None:
                        owner_graph.walker_current.owner = None
                    current = None
                    return complete

                completed_transport = retain_or_transport_details(close_errors)
                completed = cursor.components[-1]
                parent_components = cursor.components[:-1]
                parent = open_cursor(None, parent_components)
                if owner_graph is not None:
                    owner_graph.walker_parent.owner = parent
                child: DirectoryCapability | None = None
                deferred_cursor: _CleanupCursor | None = None
                try:
                    require_operation("cleanup completed directory reopen")
                    child = backend.open_directory(
                        parent, completed.name, SharePolicy.PINNED
                    )
                    if owner_graph is not None:
                        owner_graph.walker_child.owner = child
                    _cleanup_observe_directories(3)
                    _validate_cleanup_directory(
                        child,
                        completed,
                        label="cleanup completed directory",
                    )
                    require_operation("cleanup completed directory delete")
                    removed_after = removed + 1
                    pending_cell.arm(
                        scope="directory",
                        name=completed.name,
                        identity=completed.identity,
                        filesystem=completed.filesystem,
                        parent_components=parent_components,
                        removed_after=removed_after,
                    )
                    try:
                        backend.delete(child)
                    except BaseException:
                        if child.is_open:
                            pending_cell.clear()
                        else:
                            pending_cell.commit()
                            removed = removed_after
                        raise
                    else:
                        pending_cell.commit()
                        removed = removed_after
                    if child.is_open:
                        close_or_block(
                            child,
                            "cleanup completed directory",
                            "cleanup directory owner remains open",
                        )
                    removed = removed_after
                    next_cursor = _CleanupCursor(
                        parent_components, completed
                    )
                    if exhausted():
                        deferred_cursor = next_cursor
                    else:
                        require_operation("cleanup directory absence")
                        remaining = backend.entry(parent, completed.name)
                        require_operation("cleanup directory absence")
                        if remaining is not None:
                            raise OSError(
                                "cleanup preserved a same-name directory replacement"
                            )
                        pending_cell.clear()
                        cursor = _CleanupCursor(parent_components)
                except BaseException as primary_error:
                    completed_blocked: list[
                        FileCapability | DirectoryCapability
                    ] = []
                    completed_close_details: list[str] = list(
                        completed_transport
                    )
                    blocked_message = "cleanup owner remains open"
                    blocked_primary: BaseException | None = primary_error
                    if isinstance(primary_error, _CleanupOwnershipBlocked):
                        completed_blocked.extend(primary_error.owners)
                        completed_close_details.extend(primary_error.details)
                        blocked_message = str(primary_error)
                        blocked_primary = primary_error.primary
                    if child is not None and child.is_open:
                        child_errors = _close_cleanup_capability(
                            child, "cleanup completed directory"
                        )
                        completed_close_details.extend(
                            retain_or_transport_details(child_errors)
                        )
                        if child.is_open and child not in completed_blocked:
                            completed_blocked.append(child)
                    elif owner_graph is not None:
                        owner_graph.walker_child.owner = None
                    if parent.is_open:
                        parent_errors = _close_cleanup_capability(
                            parent, "cleanup completed directory parent"
                        )
                        completed_close_details.extend(
                            retain_or_transport_details(parent_errors)
                        )
                        if parent.is_open and parent not in completed_blocked:
                            completed_blocked.append(parent)
                    elif owner_graph is not None:
                        owner_graph.walker_parent.owner = None
                    if completed_blocked:
                        raise _CleanupOwnershipBlocked(
                            blocked_message,
                            tuple(completed_blocked),
                            tuple(completed_close_details),
                            primary=blocked_primary,
                        ) from primary_error
                    for detail in completed_close_details:
                        primary_error.add_note(detail)
                    raise
                parent_errors = _close_cleanup_capability(
                    parent, "cleanup completed directory parent"
                )
                parent_transport = retain_or_transport_details(parent_errors)
                if parent.is_open:
                    raise _CleanupOwnershipBlocked(
                        "cleanup parent owner remains open",
                        (parent,),
                        parent_transport,
                    )
                if owner_graph is not None:
                    owner_graph.walker_parent.owner = None
                    if child is not None and not child.is_open:
                        owner_graph.walker_child.owner = None
                if deferred_cursor is not None:
                    return _CleanupSlice(
                        examined,
                        removed,
                        False,
                        deferred_cursor,
                        details=completed_transport,
                    )
                current = open_cursor(None, cursor.components)
                if owner_graph is not None:
                    owner_graph.walker_current.owner = current
                require_operation("cleanup iterator construction")
                iterator = backend.entries_owned(current)
                if owner_graph is not None:
                    owner_graph.walker_iterator.owner = iterator
                    owner_graph.walker_current.owner = None
                current = None
                _cleanup_observe_directories(2)
                continue

            if not cursor.components and entry.name in control:
                continue
            examined += 1
            if exhausted():
                return finish_iterator(iterator, cursor)

            component = _CleanupComponent(
                entry.name, entry.identity, entry.filesystem
            )
            if entry.filesystem != root_filesystem:
                raise OSError("cleanup refuses to cross a filesystem boundary")

            if entry.kind is EntryKind.DIRECTORY:
                next_cursor = _CleanupCursor(
                    (*cursor.components, component)
                )
                _validate_cleanup_cursor(next_cursor)
                require_operation("cleanup directory open")
                try:
                    child = backend.open_directory(
                        iterator.directory,
                        entry.name,
                        SharePolicy.PINNED,
                    )
                    if owner_graph is not None:
                        owner_graph.walker_child.owner = child
                except FileNotFoundError:
                    continue
                _cleanup_observe_directories(3)
                try:
                    _validate_cleanup_directory(
                        child, component, label="cleanup directory"
                    )
                    require_operation("cleanup directory validation")
                except BaseException as primary_error:
                    close_errors = _close_cleanup_capability(
                        child, "cleanup directory"
                    )
                    transport = retain_or_transport_details(close_errors)
                    if child.is_open:
                        raise _CleanupOwnershipBlocked(
                            "cleanup directory owner remains open",
                            (child,),
                            transport,
                            primary=primary_error,
                        ) from primary_error
                    for detail in transport:
                        primary_error.add_note(detail)
                    raise
                parent_errors = _close_cleanup_iterator(
                    iterator, "cleanup parent iterator"
                )
                parent = iterator.directory
                if parent.is_open:
                    child_errors = _close_cleanup_capability(
                        child, "cleanup directory"
                    )
                    return _CleanupSlice(
                        examined,
                        removed,
                        False,
                        cursor,
                        blocked_owners=tuple(
                            owner
                            for owner in (parent, child)
                            if owner.is_open
                        ),
                        details=(*parent_errors, *child_errors),
                    )
                retain_or_transport_details(parent_errors)
                if owner_graph is not None:
                    owner_graph.walker_iterator.owner = None
                iterator = None
                cursor = next_cursor
                current = child
                require_operation("cleanup iterator construction")
                iterator = backend.entries_owned(current)
                if owner_graph is not None:
                    owner_graph.walker_iterator.owner = iterator
                    owner_graph.walker_child.owner = None
                current = None
                _cleanup_observe_directories(2)
                continue

            require_operation("cleanup entry open")
            try:
                opened = backend.open_entry(
                    iterator.directory, entry.name, SharePolicy.PINNED
                )
                if owner_graph is not None:
                    owner_graph.walker_entry.owner = opened
            except FileNotFoundError:
                continue
            try:
                if (
                    opened.kind is not entry.kind
                    or opened.identity != entry.identity
                    or opened.filesystem != entry.filesystem
                ):
                    raise OSError("cleanup entry identity changed while opening")
                require_operation("cleanup entry delete")
                removed_after = removed + 1
                pending_cell.arm(
                    scope="payload",
                    name=component.name,
                    identity=component.identity,
                    filesystem=component.filesystem,
                    parent_components=cursor.components,
                    removed_after=removed_after,
                )
                try:
                    backend.delete(opened)
                except BaseException:
                    if opened.is_open:
                        pending_cell.clear()
                    else:
                        pending_cell.commit()
                        removed = removed_after
                    raise
                else:
                    pending_cell.commit()
                    removed = removed_after
                if opened.is_open:
                    close_or_block(
                        opened,
                        f"cleanup entry {entry.name}",
                        "cleanup entry owner remains open",
                    )
                removed = removed_after
            except BaseException as primary_error:
                if opened.is_open:
                    close_errors = _close_cleanup_capability(
                        opened, f"cleanup entry {entry.name}"
                    )
                    transport = retain_or_transport_details(close_errors)
                    if opened.is_open:
                        raise _CleanupOwnershipBlocked(
                            "cleanup entry owner remains open",
                            (opened,),
                            transport,
                            primary=primary_error,
                        ) from primary_error
                    for detail in transport:
                        primary_error.add_note(detail)
                if owner_graph is not None and not opened.is_open:
                    owner_graph.walker_entry.owner = None
                raise
            if owner_graph is not None:
                owner_graph.walker_entry.owner = None

            pending = _CleanupCursor(cursor.components, component)
            if exhausted():
                return finish_iterator(iterator, pending)
            require_operation("cleanup entry absence")
            remaining = backend.entry(iterator.directory, entry.name)
            require_operation("cleanup entry absence")
            if remaining is not None:
                raise OSError(
                    "cleanup preserved a same-name replacement after removal"
                )
            pending_cell.clear()
    except _DeadlineExceeded:
        if iterator is not None:
            return finish_iterator(iterator, cursor)
        current_errors: tuple[str, ...] = ()
        if current is not None:
            current_errors = _close_cleanup_capability(
                current, "cleanup current"
            )
            if current.is_open:
                return _CleanupSlice(
                    examined,
                    removed,
                    False,
                    cursor,
                    blocked_owners=(current,),
                    details=current_errors,
                )
        return _CleanupSlice(
            examined,
            removed,
            False,
            cursor,
            details=current_errors,
        )
    except BaseException as primary_error:
        blocked: list[FileCapability | DirectoryCapability] = []
        close_details: list[str] = []
        if iterator is not None:
            iterator_errors = _close_cleanup_iterator(
                iterator, "cleanup iterator"
            )
            close_details.extend(
                retain_or_transport_details(iterator_errors)
            )
            if iterator.directory.is_open:
                blocked.append(iterator.directory)
            elif owner_graph is not None:
                owner_graph.walker_iterator.owner = None
        if current is not None and current.is_open:
            current_errors = _close_cleanup_capability(
                current, "cleanup current"
            )
            close_details.extend(
                retain_or_transport_details(current_errors)
            )
            if current.is_open:
                blocked.append(current)
            elif owner_graph is not None:
                owner_graph.walker_current.owner = None
        if isinstance(primary_error, _CleanupOwnershipBlocked):
            owners = list(primary_error.owners)
            for owner in blocked:
                if owner not in owners:
                    owners.append(owner)
            raise _CleanupOwnershipBlocked(
                str(primary_error),
                tuple(owners),
                (*primary_error.details, *close_details),
                primary=primary_error.primary,
            ) from primary_error
        if blocked:
            raise _CleanupOwnershipBlocked(
                "cleanup owner remains open",
                tuple(blocked),
                tuple(close_details),
                primary=primary_error,
            ) from primary_error
        for detail in close_details:
            primary_error.add_note(detail)
        raise


def _cleanup_handle_observer(_open_directories: int) -> None:
    """Test seam for the capability bound; production intentionally does nothing."""


def _read_valid_marker_at(
    directory: DirectoryCapability,
    filename: str,
    run_id: str,
    backend: FilesystemBackend,
    lease_id: str | None = None,
    *,
    deadline: float | None = None,
) -> tuple[FileIdentity, dict[str, object]] | None:
    """Read one validated marker without converting I/O failures to absence."""
    try:
        return _read_marker(
            directory,
            filename,
            backend,
            expected_run_id=run_id,
            expected_lease_id=lease_id,
            deadline=deadline,
            monotonic=time.monotonic,
        )
    except _InvalidManagedMarker:
        return None


@dataclass(frozen=True, slots=True)
class _JanitorCandidate:
    name: str
    identity: FileIdentity
    filesystem: FilesystemIdentity
    run_id: str
    modified_ns: int


def _is_pin_step_live_owner(error: OSError, *, pin_step: bool) -> bool:
    """Windows sharing is evidence of a live owner only at root pinning."""
    return pin_step and getattr(error, "winerror", None) in {32, 33}


def _select_janitor_candidates(
    managed_root_capability: DirectoryCapability,
    backend: FilesystemBackend,
    *,
    cursor: str,
    deadline: float,
) -> tuple[list[_JanitorCandidate], int]:
    """Stream a direct-child inventory, retaining at most 256 candidates."""
    _check_deadline(deadline, "janitor selection")
    opened_scan = backend.reopen_directory(
        managed_root_capability, SharePolicy.SCAN
    )
    scan: DirectoryCapability | None = opened_scan
    _check_deadline(deadline, "janitor selection")
    iterator: DirectoryIterator | None = None
    selected: list[tuple[tuple[int, str], _JanitorCandidate]] = []
    examined = 0
    try:
        iterator = backend.entries_owned(opened_scan)
        scan = None
        _check_deadline(deadline, "janitor selection")
        while True:
            _check_deadline(deadline, "janitor selection")
            try:
                entry = next(iterator)
            except StopIteration:
                _check_deadline(deadline, "janitor selection")
                break
            _check_deadline(deadline, "janitor selection")
            examined += 1
            if examined > 100_000:
                raise OSError(
                    "janitor direct-child scan exceeds 100000 entries"
                )
            match = _RUN_NAME.fullmatch(entry.name)
            if (
                match is None
                or entry.kind is not EntryKind.DIRECTORY
                or entry.filesystem != managed_root_capability.filesystem
            ):
                continue
            candidate = _JanitorCandidate(
                entry.name,
                entry.identity,
                entry.filesystem,
                match.group(1),
                entry.modified_ns,
            )
            key = (0 if entry.name > cursor else 1, entry.name)
            bisect.insort(selected, (key, candidate))
            if len(selected) > MAX_RECLAIM_CANDIDATES:
                selected.pop()
        if not selected:
            return [], examined
        phase = selected[0][0][0]
        return [
            candidate
            for key, candidate in selected
            if key[0] == phase
        ], examined
    finally:
        close_errors: tuple[str, ...] = ()
        if iterator is not None:
            close_errors = _close_cleanup_iterator(
                iterator, "janitor selection iterator"
            )
        elif isinstance(scan, DirectoryCapability):
            close_errors = _close_capability_retry(
                scan, "janitor selection scan"
            )
        if close_errors:
            active = sys.exc_info()[1]
            if active is not None:
                for detail in close_errors:
                    active.add_note(detail)
            else:
                raise OSError("; ".join(close_errors))


def _reclaim_empty_unleased_candidate_inner(
    managed_root: Path,
    managed_root_capability: DirectoryCapability,
    candidate: DirectoryCapability,
    selected: _JanitorCandidate,
    backend: FilesystemBackend,
    *,
    current_time: float,
    deadline: float,
    cleanup_details: list[str],
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    name = selected.name
    is_deleting = name.startswith(".deleting-")
    is_staging = name.startswith(".staging-")
    if not is_deleting and not is_staging:
        return None
    reported = managed_root / name
    coordinator: LeaseLock | None = None
    try:
        _check_deadline(deadline, "empty janitor")
        if (
            candidate.identity != selected.identity
            or candidate.filesystem != selected.filesystem
        ):
            return _janitor_diagnostic("empty candidate identity changed")
        _check_deadline(deadline, "empty janitor")
        current = backend.entry(managed_root_capability, name)
        _check_deadline(deadline, "empty janitor")
        if current is None:
            return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 0, 0)
        if (
            current.kind is not EntryKind.DIRECTORY
            or current.identity != selected.identity
            or current.filesystem != selected.filesystem
        ):
            return _janitor_diagnostic("empty candidate identity changed")
        if is_staging and (
            selected.modified_ns / 1_000_000_000
            > current_time - STALE_AFTER_SECONDS
        ):
            return None
        _check_deadline(deadline, "empty janitor")
        if not _directory_is_empty_at(
            managed_root_capability,
            candidate,
            name,
            backend,
            deadline=deadline,
        ):
            return None
        _check_deadline(deadline, "empty janitor")
        coordinator = _open_coordinator(
            managed_root_capability,
            backend,
            timeout=max(0.001, min(5.0, deadline - time.monotonic())),
            deadline=deadline,
        )
        try:
            _check_deadline(deadline, "empty janitor claim")
            _require_current_entry(
                managed_root_capability,
                name,
                backend,
                kind=EntryKind.DIRECTORY,
                identity=selected.identity,
                filesystem=selected.filesystem,
                label="empty candidate",
            )
            _check_deadline(deadline, "empty janitor claim")
            if not _directory_is_empty_at(
                managed_root_capability,
                candidate,
                name,
                backend,
                deadline=deadline,
            ):
                return None
            _check_deadline(deadline, "empty janitor claim")
            backend.delete(candidate)
            _check_deadline(deadline, "empty janitor claim")
            if backend.entry(managed_root_capability, name) is not None:
                return _janitor_failure(
                    "empty candidate remains after removal", reported
                )
            _check_deadline(deadline, "empty janitor claim")
        finally:
            coordinator_close_errors = _close_lease_lock_retry(
                coordinator, "empty janitor coordinator"
            )
            cleanup_details.extend(coordinator_close_errors)
        return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 0, 1)
    except _DeadlineExceeded as error:
        return _janitor_diagnostic(str(error))
    except FileNotFoundError:
        try:
            _check_deadline(deadline, "empty janitor recovery")
        except _DeadlineExceeded as error:
            return _janitor_diagnostic(str(error))
        return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 0, 0)
    except OSError as error:
        detail = f"{type(error).__name__}: {error}"
        return _janitor_failure(detail, reported)


def _reclaim_empty_unleased_candidate(
    managed_root: Path,
    managed_root_capability: DirectoryCapability,
    candidate: DirectoryCapability,
    selected: _JanitorCandidate,
    backend: FilesystemBackend,
    *,
    current_time: float,
    deadline: float,
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    cleanup_details: list[str] = []
    result = _reclaim_empty_unleased_candidate_inner(
        managed_root,
        managed_root_capability,
        candidate,
        selected,
        backend,
        current_time=current_time,
        deadline=deadline,
        cleanup_details=cleanup_details,
    )
    if not cleanup_details:
        return result
    bounded = tuple(_bounded_janitor_detail(item) for item in cleanup_details)
    if isinstance(result, ScratchCleanupRecord):
        return replace(result, details=(*result.details, *bounded))
    if isinstance(result, JanitorDiagnostic):
        return replace(result, details=(*result.details, *bounded))
    return JanitorDiagnostic(bounded)


def _bound_cleanup_records(
    records: list[ScratchCleanupRecord | JanitorDiagnostic],
) -> list[ScratchCleanupRecord | JanitorDiagnostic]:
    bounded: list[ScratchCleanupRecord | JanitorDiagnostic] = []
    remaining_details = MAX_DIAGNOSTIC_DETAILS
    for record in records:
        kept = record.details[:remaining_details]
        omitted = len(record.details) - len(kept)
        bounded.append(
            replace(
                record,
                details=kept,
                omitted_detail_count=record.omitted_detail_count + omitted,
            )
        )
        remaining_details -= len(kept)
    return bounded


def _bounded_janitor_detail(detail: str) -> str:
    encoded = detail.encode("utf-8", errors="replace")
    if len(encoded) > MAX_DIAGNOSTIC_DETAIL_BYTES:
        suffix = b" [truncated]"
        encoded = encoded[: MAX_DIAGNOSTIC_DETAIL_BYTES - len(suffix)]
        detail = encoded.decode("utf-8", errors="ignore") + suffix.decode("ascii")
    return detail


def _janitor_diagnostic(detail: str) -> JanitorDiagnostic:
    return JanitorDiagnostic((_bounded_janitor_detail(detail),))


def _janitor_failure(detail: str, remaining: Path) -> ScratchCleanupRecord:
    return ScratchCleanupRecord(
        ScratchCleanupStatus.FAILED,
        0,
        0,
        (_bounded_janitor_detail(detail),),
        validate_reported_path(remaining),
    )


def _bounded_candidate_entries(
    managed_root_capability: DirectoryCapability,
    candidate: DirectoryCapability,
    name: str,
    backend: FilesystemBackend,
    *,
    deadline: float,
) -> tuple[DirectoryEntry, ...]:
    """Return no more than two entries from an identity-checked SCAN open."""
    _check_deadline(deadline, "janitor bounded directory scan")
    opened_scan = backend.open_directory(
        managed_root_capability, name, SharePolicy.SCAN
    )
    scan: DirectoryCapability | None = opened_scan
    _check_deadline(deadline, "janitor bounded directory scan")
    iterator: DirectoryIterator | None = None
    try:
        if (
            opened_scan.identity != candidate.identity
            or opened_scan.filesystem != candidate.filesystem
        ):
            raise OSError("janitor scan identity changed")
        _require_current_entry(
            managed_root_capability,
            name,
            backend,
            kind=EntryKind.DIRECTORY,
            identity=candidate.identity,
            filesystem=candidate.filesystem,
            label="janitor scan root",
        )
        _check_deadline(deadline, "janitor bounded directory scan")
        iterator = backend.entries_owned(opened_scan)
        scan = None
        found: list[DirectoryEntry] = []
        while len(found) < 2:
            _check_deadline(deadline, "janitor bounded directory scan")
            try:
                entry = next(iterator)
            except StopIteration:
                _check_deadline(deadline, "janitor bounded directory scan")
                break
            _check_deadline(deadline, "janitor bounded directory scan")
            found.append(entry)
        return tuple(found)
    finally:
        errors: tuple[str, ...] = ()
        if iterator is not None:
            errors = _close_cleanup_iterator(
                iterator, "janitor bounded directory iterator"
            )
        elif isinstance(scan, DirectoryCapability):
            errors = _close_capability_retry(scan, "janitor scan root")
        if errors:
            active = sys.exc_info()[1]
            if active is not None:
                for detail in errors:
                    active.add_note(detail)
            else:
                raise OSError("; ".join(errors))


def _directory_is_empty_at(
    managed_root_capability: DirectoryCapability,
    candidate: DirectoryCapability,
    name: str,
    backend: FilesystemBackend,
    *,
    deadline: float,
) -> bool:
    return not _bounded_candidate_entries(
        managed_root_capability,
        candidate,
        name,
        backend,
        deadline=deadline,
    )


def _directory_contains_only_lease_at(
    managed_root_capability: DirectoryCapability,
    candidate: DirectoryCapability,
    name: str,
    backend: FilesystemBackend,
    *,
    deadline: float,
) -> bool:
    entries = _bounded_candidate_entries(
        managed_root_capability,
        candidate,
        name,
        backend,
        deadline=deadline,
    )
    return len(entries) == 1 and entries[0].name == LEASE_FILE


@dataclass(slots=True)
class _DeferredJanitorResources:
    managed_root: DirectoryCapability | None = None
    root: DirectoryCapability | None = None
    lease: LeaseLock | None = None


def _dispose_deferred_resources(
    resources: _DeferredJanitorResources,
) -> tuple[str, ...]:
    """Dispose every typed owner without inventing a third close attempt."""
    errors: list[str] = []
    lease = resources.lease
    resources.lease = None
    if lease is not None:
        errors.extend(_close_lease_lock_retry(lease, "deferred janitor lease"))
        if lease.fd >= 0:
            resources.lease = lease
    root = resources.root
    resources.root = None
    if root is not None:
        errors.extend(_close_capability_retry(root, "deferred janitor root"))
        if root.is_open:
            resources.root = root
    managed = resources.managed_root
    resources.managed_root = None
    if managed is not None:
        errors.extend(
            _close_capability_retry(managed, "deferred janitor managed root")
        )
        if managed.is_open:
            resources.managed_root = managed
    return tuple(errors)


def _marker_status(
    root: DirectoryCapability,
    name: str,
    run_id: str,
    backend: FilesystemBackend,
    *,
    lease_id: str | None,
    deadline: float,
) -> tuple[tuple[FileIdentity, dict[str, object]] | None, bool]:
    marker = _read_valid_marker_at(
        root,
        name,
        run_id,
        backend,
        lease_id,
        deadline=deadline,
    )
    if marker is not None:
        return marker, False
    _check_deadline(deadline, "janitor marker evidence")
    present = backend.entry(root, name) is not None
    _check_deadline(deadline, "janitor marker evidence")
    return None, present


def _janitor_candidate_record_inner(
    managed_root: Path,
    managed_root_capability: DirectoryCapability,
    selected: _JanitorCandidate,
    backend: FilesystemBackend,
    *,
    current_time: float,
    deadline: float,
    cleanup_details: list[str],
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    reported = managed_root / selected.name
    root: DirectoryCapability | None = None
    lease: LeaseLock | None = None
    heartbeat_owner: FileCapability | None = None
    managed_owner: DirectoryCapability | None = None
    transferred = False
    result: ScratchCleanupRecord | JanitorDiagnostic | None = None
    try:
        _check_deadline(deadline, "janitor candidate pin")
        try:
            root = backend.open_directory(
                managed_root_capability,
                selected.name,
                SharePolicy.PINNED,
            )
        except OSError as error:
            if _is_pin_step_live_owner(error, pin_step=True):
                return None
            raise
        _check_deadline(deadline, "janitor candidate pin")
        if (
            root.identity != selected.identity
            or root.filesystem != selected.filesystem
            or root.kind is not EntryKind.DIRECTORY
        ):
            return _janitor_diagnostic("janitor candidate identity changed")
        backend.verify_managed_security(root, repair_dacl=False)
        _check_deadline(deadline, "janitor candidate pin")
        _require_current_entry(
            managed_root_capability,
            selected.name,
            backend,
            kind=EntryKind.DIRECTORY,
            identity=selected.identity,
            filesystem=selected.filesystem,
            label="janitor candidate",
        )
        _check_deadline(deadline, "janitor candidate pin")
        lease_read, lease_malformed = _marker_status(
            root,
            LEASE_FILE,
            selected.run_id,
            backend,
            lease_id=None,
            deadline=deadline,
        )
        if lease_read is None:
            if lease_malformed:
                return _janitor_diagnostic(
                    "janitor candidate has an invalid lease marker"
                )
            result = _reclaim_empty_unleased_candidate(
                managed_root,
                managed_root_capability,
                root,
                selected,
                backend,
                current_time=current_time,
                deadline=deadline,
            )
            return result
        lease_identity, lease_value = lease_read
        lease_id = cast(str, lease_value["lease_id"])
        _check_deadline(deadline, "janitor lease open")
        lease_capability = _open_owned_marker(
            root,
            LEASE_FILE,
            backend,
            access=FileAccess.READ_WRITE,
            identity=lease_identity,
            deadline=deadline,
            monotonic=time.monotonic,
        )
        descriptor_owner = _OwnedDescriptor()
        lock = LeaseLock(-1)
        try:
            descriptor_owner.adopt(
                lease_capability.detach_to_fd(
                    os.O_RDWR | getattr(os, "O_BINARY", 0)
                )
            )
            _check_deadline(deadline, "janitor lease open")
            lock.fd = descriptor_owner.detach()
            lease = lock
            try:
                lease.acquire(blocking=False)
            except OSError as error:
                if isinstance(error, BlockingIOError) or error.errno in {
                    errno.EACCES,
                    errno.EAGAIN,
                    errno.EWOULDBLOCK,
                }:
                    return None
                raise
            _check_deadline(deadline, "janitor lease lock")
            if _read_locked_marker(
                lease,
                expected_run_id=selected.run_id,
                expected_lease_id=lease_id,
                deadline=deadline,
                monotonic=time.monotonic,
            ) != lease_value:
                return _janitor_diagnostic(
                    "janitor lease changed after locking"
                )
        except BaseException as error:
            if lease_capability.is_open:
                for detail in _close_capability_retry(
                    lease_capability, "janitor lease capability"
                ):
                    error.add_note(detail)
            for detail in descriptor_owner.close_retry(
                "janitor lease descriptor"
            ):
                error.add_note(detail)
            if lease is None:
                for detail in _close_lease_lock_retry(
                    lock, "janitor lease lock"
                ):
                    error.add_note(detail)
            raise

        retained, retain_malformed = _marker_status(
            root,
            RETAIN_FILE,
            selected.run_id,
            backend,
            lease_id=lease_id,
            deadline=deadline,
        )
        if retained is not None or retain_malformed:
            return None
        ready, ready_malformed = _marker_status(
            root,
            CLEANUP_READY_FILE,
            selected.run_id,
            backend,
            lease_id=lease_id,
            deadline=deadline,
        )
        if ready_malformed:
            return None
        heartbeat, heartbeat_malformed = _marker_status(
            root,
            HEARTBEAT_FILE,
            selected.run_id,
            backend,
            lease_id=lease_id,
            deadline=deadline,
        )
        if heartbeat_malformed and ready is None:
            return None
        lease_only = _directory_contains_only_lease_at(
            managed_root_capability,
            root,
            selected.name,
            backend,
            deadline=deadline,
        )
        deleting_tail = selected.name.startswith(".deleting-") and lease_only
        old_staging = (
            selected.name.startswith(".staging-")
            and selected.modified_ns / 1_000_000_000
            <= current_time - STALE_AFTER_SECONDS
            and lease_only
        )
        stale = False
        heartbeat_identity: FileIdentity | None = None
        if heartbeat is not None:
            heartbeat_identity = heartbeat[0]
            _check_deadline(deadline, "janitor heartbeat evidence")
            heartbeat_entry = backend.entry(root, HEARTBEAT_FILE)
            _check_deadline(deadline, "janitor heartbeat evidence")
            stale = (
                heartbeat_entry is not None
                and heartbeat_entry.kind is EntryKind.REGULAR
                and heartbeat_entry.identity == heartbeat_identity
                and heartbeat_entry.modified_ns / 1_000_000_000
                <= current_time - STALE_AFTER_SECONDS
            )
        if selected.name.startswith(".staging-"):
            if not old_staging:
                return None
        elif ready is None and not stale and not deleting_tail:
            return None

        deleting_name = f".deleting-{selected.run_id}"
        coordinator: LeaseLock | None = None
        try:
            _check_deadline(deadline, "janitor claim")
            coordinator = _open_coordinator(
                managed_root_capability,
                backend,
                timeout=min(5.0, max(0.001, deadline - time.monotonic())),
                deadline=deadline,
            )
            if hasattr(backend, "coordinator"):
                setattr(backend, "coordinator", coordinator)
            _check_deadline(deadline, "janitor claim")
            _require_current_entry(
                managed_root_capability,
                selected.name,
                backend,
                kind=EntryKind.DIRECTORY,
                identity=selected.identity,
                filesystem=selected.filesystem,
                label="janitor claim root",
            )
            if selected.name != deleting_name:
                destination = backend.entry(
                    managed_root_capability, deleting_name
                )
                _check_deadline(deadline, "janitor claim")
                if destination is not None:
                    return None
            if backend.directory_rename_requires_closed_descendants:
                lease_errors = _close_lease_lock_retry(
                    lease, "janitor candidate lease handoff"
                )
                if lease.fd >= 0:
                    root_errors = _close_capability_retry(
                        root, "janitor candidate root handoff"
                    )
                    return _janitor_failure(
                        "; ".join((*lease_errors, *root_errors)), reported
                    )
                lease = None
            if selected.name != deleting_name:
                _check_deadline(deadline, "janitor claim")
                backend.rename(
                    root,
                    managed_root_capability,
                    deleting_name,
                    replace=False,
                )
                _check_deadline(deadline, "janitor claim")
                reported = managed_root / deleting_name
                _require_current_entry(
                    managed_root_capability,
                    deleting_name,
                    backend,
                    kind=EntryKind.DIRECTORY,
                    identity=selected.identity,
                    filesystem=selected.filesystem,
                    label="janitor claimed root",
                )
                _check_deadline(deadline, "janitor claim")
            if backend.directory_rename_requires_closed_descendants:
                old_root = root
                old_root_errors = _close_capability_retry(
                    old_root, "janitor pre-handoff root"
                )
                if old_root.is_open:
                    return _janitor_failure(
                        "; ".join(old_root_errors), reported
                    )
                root = backend.open_directory(
                    managed_root_capability,
                    deleting_name,
                    SharePolicy.PINNED,
                )
                _check_deadline(deadline, "janitor handoff reopen")
                if (
                    root.identity != selected.identity
                    or root.filesystem != selected.filesystem
                ):
                    raise OSError("janitor handoff root identity changed")
                reopened = _open_owned_marker(
                    root,
                    LEASE_FILE,
                    backend,
                    access=FileAccess.READ_WRITE,
                    identity=lease_identity,
                    deadline=deadline,
                    monotonic=time.monotonic,
                )
                lease = LeaseLock(
                    reopened.detach_to_fd(
                        os.O_RDWR | getattr(os, "O_BINARY", 0)
                    )
                )
                lease.acquire(blocking=False)
                _check_deadline(deadline, "janitor handoff lease")
                if _read_locked_marker(
                    lease,
                    expected_run_id=selected.run_id,
                    expected_lease_id=lease_id,
                    deadline=deadline,
                    monotonic=time.monotonic,
                ) != lease_value:
                    raise OSError("janitor handoff lease changed")
        finally:
            if coordinator is not None:
                coordinator_errors = _close_lease_lock_retry(
                    coordinator, "janitor claim coordinator"
                )
                if coordinator_errors:
                    active = sys.exc_info()[1]
                    if active is not None:
                        for detail in coordinator_errors:
                            active.add_note(detail)
                    else:
                        raise OSError("; ".join(coordinator_errors))

        if heartbeat_identity is not None:
            heartbeat_owner = _open_owned_marker(
                root,
                HEARTBEAT_FILE,
                backend,
                access=FileAccess.WRITE,
                identity=heartbeat_identity,
                deadline=deadline,
                monotonic=time.monotonic,
            )
        managed_owner = backend.reopen_directory(
            managed_root_capability, SharePolicy.MUTATION
        )
        _check_deadline(deadline, "janitor cleanup transfer")
        if lease is None:
            raise OSError("janitor lease owner is unavailable")
        managed = ManagedScratch(
            backend=backend,
            managed_root=managed_root,
            path=reported,
            run_id=selected.run_id,
            lease_id=lease_id,
            lease=lease,
            lease_identity=lease_identity,
            managed_root_capability=managed_owner,
            root=root,
            heartbeat=heartbeat_owner,
            heartbeat_identity=(heartbeat_identity or lease_identity),
        )
        lease = None
        root = None
        heartbeat_owner = None
        managed_owner = None
        transferred = True
        managed._cleanup_ready = ready is not None
        graph = managed._ensure_cleanup_graph()
        if retained is None:
            graph.retain_removed = True
        if heartbeat is None:
            graph.heartbeat_removed = True
        if ready is None:
            graph.cleanup_ready_removed = True
        remaining = deadline - time.monotonic()
        cleanup = (
            managed.defer("janitor cleanup budget exhausted")
            if remaining <= 0
            else managed.cleanup(
                time_budget=min(MAX_CLEANUP_SLICE_SECONDS, remaining)
            )
        )
        disposal = managed.close_capabilities()
        if disposal:
            cleanup = replace(
                cleanup, details=(*cleanup.details, *disposal)
            )
        return cleanup
    except _DeadlineExceeded as error:
        return _janitor_diagnostic(str(error))
    except OSError as error:
        return _janitor_failure(
            _exception_detail(error), reported
        )
    finally:
        if not transferred:
            details: list[str] = []
            if heartbeat_owner is not None:
                details.extend(
                    _close_capability_retry(
                        heartbeat_owner, "janitor candidate heartbeat"
                    )
                )
            if lease is not None:
                details.extend(
                    _close_lease_lock_retry(
                        lease, "janitor candidate lease"
                    )
                )
            if root is not None:
                details.extend(
                    _close_capability_retry(root, "janitor candidate root")
                )
            if managed_owner is not None:
                details.extend(
                    _close_capability_retry(
                        managed_owner, "janitor candidate managed root"
                    )
                )
            cleanup_details.extend(details)


def _janitor_candidate_record(
    managed_root: Path,
    managed_root_capability: DirectoryCapability,
    selected: _JanitorCandidate,
    backend: FilesystemBackend,
    *,
    current_time: float,
    deadline: float,
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    """Run one candidate slice and attach every owner-disposal diagnostic."""
    cleanup_details: list[str] = []
    result = _janitor_candidate_record_inner(
        managed_root,
        managed_root_capability,
        selected,
        backend,
        current_time=current_time,
        deadline=deadline,
        cleanup_details=cleanup_details,
    )
    if not cleanup_details:
        return result
    bounded = tuple(_bounded_janitor_detail(item) for item in cleanup_details)
    if isinstance(result, ScratchCleanupRecord):
        return replace(result, details=(*result.details, *bounded))
    if isinstance(result, JanitorDiagnostic):
        return replace(result, details=(*result.details, *bounded))
    return JanitorDiagnostic(bounded)


def reclaim_abandoned(
    managed_root: Path,
    *,
    now: float | None = None,
    backend: FilesystemBackend | None = None,
    managed_root_capability: DirectoryCapability | None = None,
) -> list[ScratchCleanupRecord | JanitorDiagnostic]:
    """Capability-relative bounded selection, claim, and cleanup."""
    selected_backend = default_filesystem_backend() if backend is None else backend
    started = time.monotonic()
    deadline = started + JANITOR_CLEANUP_SECONDS
    selection_deadline = min(deadline, started + JANITOR_SELECTION_SECONDS)
    root: DirectoryCapability | None = managed_root_capability
    records: list[ScratchCleanupRecord | JanitorDiagnostic] = []
    try:
        if root is None:
            verified, root = _ensure_managed_root(
                managed_root.parent.resolve(strict=True),
                backend=selected_backend,
                deadline=selection_deadline,
            )
            if verified != managed_root.resolve(strict=True):
                raise OSError("janitor managed root path changed")
        if root.share_policy is not SharePolicy.MUTATION:
            replacement = selected_backend.reopen_directory(
                root, SharePolicy.MUTATION
            )
            close_errors = _close_capability_retry(
                root, "janitor supplied managed root"
            )
            if close_errors:
                _close_capability_retry(
                    replacement, "janitor replacement managed root"
                )
                raise OSError("; ".join(close_errors))
            root = replacement
        _check_deadline(selection_deadline, "janitor selection")
        coordinator = _open_coordinator(
            root,
            selected_backend,
            timeout=max(0.001, selection_deadline - time.monotonic()),
            deadline=selection_deadline,
        )
        if hasattr(selected_backend, "coordinator"):
            setattr(selected_backend, "coordinator", coordinator)
        try:
            state = _read_coordinator_state(
                coordinator.fd, deadline=selection_deadline
            )
            _check_deadline(selection_deadline, "janitor selection")
            candidates, _examined = _select_janitor_candidates(
                root,
                selected_backend,
                cursor=state.cursor,
                deadline=selection_deadline,
            )
            if candidates:
                try:
                    _persist_coordinator_cursor(
                        coordinator.fd,
                        state,
                        candidates[-1].name,
                        deadline=selection_deadline,
                    )
                    _check_deadline(selection_deadline, "janitor selection")
                except OSError as error:
                    records.append(
                        _janitor_diagnostic(
                            "janitor cursor persistence failed: "
                            + _exception_detail(error)
                        )
                    )
        except (OSError, _DeadlineExceeded) as error:
            records.append(_janitor_diagnostic(_exception_detail(error)))
            candidates = []
        finally:
            close_errors = _close_lease_lock_retry(
                coordinator, "janitor selection coordinator"
            )
            if close_errors:
                records.append(_janitor_diagnostic("; ".join(close_errors)))
        current_time = time.time() if now is None else now
        deferred: list[tuple[int, _JanitorCandidate]] = []
        for candidate in candidates:
            if time.monotonic() >= deadline:
                break
            record = _janitor_candidate_record(
                managed_root,
                root,
                candidate,
                selected_backend,
                current_time=current_time,
                deadline=deadline,
            )
            if record is not None:
                records.append(record)
                if (
                    isinstance(record, ScratchCleanupRecord)
                    and record.status is ScratchCleanupStatus.DEFERRED
                ):
                    deferred.append(
                        (
                            len(records) - 1,
                            replace(
                                candidate,
                                name=f".deleting-{candidate.run_id}",
                            ),
                        )
                    )
        for record_index, candidate in deferred:
            if time.monotonic() >= deadline:
                break
            resumed = _resume_deferred_cleanup(
                managed_root,
                root,
                candidate.name,
                candidate.identity,
                candidate.filesystem,
                selected_backend,
                deadline=deadline,
            )
            if isinstance(resumed, ScratchCleanupRecord):
                first = cast(ScratchCleanupRecord, records[record_index])
                records[record_index] = replace(
                    resumed,
                    examined_entries=(
                        first.examined_entries + resumed.examined_entries
                    ),
                    removed_entries=(
                        first.removed_entries + resumed.removed_entries
                    ),
                    details=(*first.details, *resumed.details),
                    omitted_detail_count=(
                        first.omitted_detail_count
                        + resumed.omitted_detail_count
                    ),
                )
            elif resumed is not None:
                records.append(resumed)
    finally:
        if root is not None:
            close_errors = _close_capability_retry(
                root, "janitor managed root"
            )
            active = sys.exc_info()[1]
            if close_errors and active is not None:
                for detail in close_errors:
                    active.add_note(detail)
            elif close_errors:
                records.append(_janitor_diagnostic("; ".join(close_errors)))
    return _bound_cleanup_records(records)


def _resume_deferred_cleanup_inner(
    managed_root: Path,
    selection_root: DirectoryCapability,
    name: str,
    expected_identity: FileIdentity,
    expected_filesystem: FilesystemIdentity,
    backend: FilesystemBackend,
    *,
    deadline: float,
    cleanup_details: list[str],
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    match = _RUN_NAME.fullmatch(name)
    if match is None or not name.startswith(".deleting-"):
        return None
    resources = _DeferredJanitorResources()
    result: ScratchCleanupRecord | JanitorDiagnostic | None = None
    try:
        _check_deadline(deadline, "deferred janitor")
        resources.managed_root = backend.reopen_directory(
            selection_root, SharePolicy.MUTATION
        )
        _check_deadline(deadline, "deferred janitor")
        try:
            resources.root = backend.open_directory(
                resources.managed_root, name, SharePolicy.PINNED
            )
        except FileNotFoundError:
            return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 0, 0)
        _check_deadline(deadline, "deferred janitor")
        if (
            resources.root.identity != expected_identity
            or resources.root.filesystem != expected_filesystem
        ):
            return _janitor_diagnostic("deferred cleanup identity changed")
        selected = _JanitorCandidate(
            name,
            expected_identity,
            expected_filesystem,
            match.group(1),
            resources.root.modified_ns,
        )
        root_errors = _close_capability_retry(
            resources.root, "deferred probe root"
        )
        if not resources.root.is_open:
            resources.root = None
        if root_errors:
            return _janitor_diagnostic("; ".join(root_errors))
        managed_errors = _close_capability_retry(
            resources.managed_root, "deferred probe managed root"
        )
        if not resources.managed_root.is_open:
            resources.managed_root = None
        if managed_errors:
            return _janitor_diagnostic("; ".join(managed_errors))
        result = _janitor_candidate_record(
            managed_root,
            selection_root,
            selected,
            backend,
            current_time=time.time(),
            deadline=deadline,
        )
        return result
    finally:
        errors = _dispose_deferred_resources(resources)
        if errors:
            active = sys.exc_info()[1]
            if active is not None:
                for detail in errors:
                    active.add_note(detail)
            cleanup_details.extend(errors)


def _resume_deferred_cleanup(
    managed_root: Path,
    selection_root: DirectoryCapability,
    name: str,
    expected_identity: FileIdentity,
    expected_filesystem: FilesystemIdentity,
    backend: FilesystemBackend,
    *,
    deadline: float,
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    """Resume a deferred slice without losing probe-owner diagnostics."""
    cleanup_details: list[str] = []
    try:
        result = _resume_deferred_cleanup_inner(
            managed_root,
            selection_root,
            name,
            expected_identity,
            expected_filesystem,
            backend,
            deadline=deadline,
            cleanup_details=cleanup_details,
        )
    except (_DeadlineExceeded, OSError) as error:
        result = _janitor_diagnostic(_exception_detail(error))
    if not cleanup_details:
        return result
    bounded = tuple(_bounded_janitor_detail(item) for item in cleanup_details)
    if isinstance(result, ScratchCleanupRecord):
        return replace(result, details=(*result.details, *bounded))
    if isinstance(result, JanitorDiagnostic):
        return replace(result, details=(*result.details, *bounded))
    return JanitorDiagnostic(bounded)
