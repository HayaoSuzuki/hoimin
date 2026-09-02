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
from typing import TYPE_CHECKING, Any, cast
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


class LeaseLock:
    def __init__(self, fd: int) -> None:
        self.fd = fd
        self.locked = False

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
        try:
            self.close()
        except BaseException:
            pass


def _close_lease_lock_all(lock: LeaseLock, label: str) -> tuple[str, ...]:
    errors: list[str] = []
    descriptor = lock.fd
    try:
        lock.release()
    except BaseException as error:
        errors.append(_bounded_secondary(f"{label} unlock failed", error))
    if descriptor >= 0:
        try:
            os.close(descriptor)
        except BaseException as error:
            errors.append(_bounded_secondary(f"{label} close failed", error))
        else:
            lock.fd = -1
            lock.locked = False
    return tuple(errors)


def _close_lease_lock_retry(
    lock: LeaseLock, label: str
) -> tuple[str, ...]:
    errors: list[str] = []
    for _attempt in range(2):
        if lock.fd < 0:
            break
        errors.extend(_close_lease_lock_all(lock, label))
    return tuple(errors)


def _close_locked_coordinator_once(
    lock: LeaseLock, label: str
) -> tuple[str, ...]:
    """Close a held coordinator descriptor without an unlock/close gap."""
    if lock.fd < 0:
        if lock.locked:
            return (f"{label} close refused an inconsistent held lock",)
        return ()
    if not lock.locked:
        return (f"{label} close requires a held lock",)
    descriptor = lock.fd
    try:
        os.close(descriptor)
    except BaseException as error:
        return (_bounded_secondary(f"{label} close failed", error),)
    lock.fd = -1
    lock.locked = False
    return ()


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


def _bounded_secondary(label: str, error: BaseException) -> str:
    detail = f"{label}: {type(error).__name__}: {error}"
    encoded = detail.encode("utf-8", errors="replace")
    if len(encoded) <= MAX_DIAGNOSTIC_DETAIL_BYTES:
        return detail
    suffix = b"..."
    return (encoded[: MAX_DIAGNOSTIC_DETAIL_BYTES - len(suffix)] + suffix).decode(
        "utf-8", errors="ignore"
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

    def adopt(self, descriptor: int) -> None:
        if self.fd >= 0 or descriptor < 0:
            raise RuntimeError("descriptor owner cannot adopt this descriptor")
        self.fd = descriptor

    def detach(self) -> int:
        if self.fd < 0:
            raise RuntimeError("descriptor owner is empty")
        descriptor = self.fd
        self.fd = -1
        return descriptor

    def close_retry(self, label: str) -> tuple[str, ...]:
        errors: list[str] = []
        for _attempt in range(2):
            if self.fd < 0:
                break
            errors.extend(self.close_once(label))
        return tuple(errors)

    def close_once(self, label: str) -> tuple[str, ...]:
        if self.fd < 0:
            return ()
        descriptor = self.fd
        try:
            os.close(descriptor)
        except BaseException as error:
            return (_bounded_secondary(f"{label} close failed", error),)
        self.fd = -1
        return ()

    def __del__(self) -> None:
        if self.fd < 0:
            return
        try:
            os.close(self.fd)
        except BaseException:
            return
        self.fd = -1


class _MarkerOwnerSlot:
    __slots__ = ("capability", "descriptor", "lock")

    def __init__(self) -> None:
        self.capability: FileCapability | DirectoryCapability | None = None
        self.descriptor: _OwnedDescriptor | None = None
        self.lock: LeaseLock | None = None

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
        raise OSError("managed marker content is invalid") from error
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
        raise OSError("managed marker content does not match its owner")
    try:
        if str(uuid.UUID(value["run_id"])) != value["run_id"]:
            raise ValueError
        if str(uuid.UUID(value["lease_id"])) != value["lease_id"]:
            raise ValueError
    except (ValueError, TypeError, AttributeError) as error:
        raise OSError("managed marker UUID is not canonical") from error
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
    _owner_slot: _MarkerOwnerSlot | None = None,
) -> tuple[FileIdentity, dict[str, object]] | None:
    marker: FileCapability | None = None
    descriptor_owner = _OwnedDescriptor()
    completed = False
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
    try:
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
            completed = True
            return None
        if deadline is not None:
            _check_deadline(deadline, "managed marker read")
        if marker.kind is not EntryKind.REGULAR:
            raise OSError("managed marker is not a regular file")
        identity = marker.identity
        backend.verify_managed_security(marker, repair_dacl=False)
        if deadline is not None:
            _check_deadline(deadline, "managed marker read")
        descriptor_owner.adopt(
            marker.detach_to_fd(os.O_RDONLY | getattr(os, "O_BINARY", 0))
        )
        if _owner_slot is not None:
            _owner_slot.capability = None
        marker = None
        if deadline is not None:
            _check_deadline(deadline, "managed marker read")
        chunks: list[bytes] = []
        remaining = MARKER_CAPACITY + 1
        while remaining:
            chunk = os.read(descriptor_owner.fd, remaining)
            if deadline is not None:
                _check_deadline(deadline, "managed marker read")
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
                _owner_slot.descriptor = None
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
                    _owner_slot.descriptor = None


def _open_coordinator(
    root: DirectoryCapability,
    backend: FilesystemBackend,
    *,
    timeout: float = 5.0,
    deadline: float | None = None,
    monotonic: Callable[[], float] | None = None,
    sleep: Callable[[float], None] = time.sleep,
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
    coordinator: FileCapability | None = None
    descriptor_owner = _OwnedDescriptor()
    try:
        coordinator = backend.open_file(
            root,
            COORDINATOR_FILE,
            access=FileAccess.READ_WRITE,
            disposition=CreateDisposition.OPEN_OR_CREATE,
            share_policy=SharePolicy.PINNED,
        )
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
        for close_error in transfer_close_errors:
            primary_error.add_note(close_error)
        raise
    lock: LeaseLock | None = None
    try:
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
    _owner_slot: _MarkerOwnerSlot | None = None,
) -> FileCapability:
    marker: FileCapability | None = None
    if (
        _owner_slot is not None
        and _owner_slot.capability is not None
        and _owner_slot.capability.is_open
    ):
        raise RuntimeError("managed marker capability slot is already occupied")
    try:
        marker = backend.open_file(
            parent,
            name,
            access=access,
            disposition=CreateDisposition.OPEN_EXISTING,
            share_policy=SharePolicy.PINNED,
        )
        if _owner_slot is not None:
            _owner_slot.capability = marker
        if (
            marker.kind is not EntryKind.REGULAR
            or marker.identity != identity
            or marker.filesystem != parent.filesystem
        ):
            raise OSError(f"managed marker identity changed for {name}")
        backend.verify_managed_security(marker, repair_dacl=False)
        _require_current_entry(
            parent,
            name,
            backend,
            kind=EntryKind.REGULAR,
            identity=identity,
            filesystem=parent.filesystem,
            label=f"managed marker {name}",
        )
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
) -> dict[str, object]:
    os.lseek(lock.fd, 0, os.SEEK_SET)
    chunks: list[bytes] = []
    remaining = MARKER_CAPACITY + 1
    while remaining:
        chunk = os.read(lock.fd, remaining)
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
    if TYPE_CHECKING:
        # Task 8 legacy methods still type-check against their pending raw-fd
        # migration, but Task 7 never initializes or exposes these attributes.
        _managed_root_fd: int
        _root_fd: int
        _heartbeat_fd: int

    def __init__(
        self,
        *,
        backend: FilesystemBackend,
        managed_root: Path,
        path: Path,
        run_id: str,
        lease_id: str,
        lease: LeaseLock,
        managed_root_capability: DirectoryCapability,
        root: DirectoryCapability,
        heartbeat: FileCapability,
        heartbeat_identity: FileIdentity,
    ) -> None:
        self._backend = backend
        self.managed_root = managed_root
        self.path = path
        self.run_id = run_id
        self.lease_id = lease_id
        self._lease: LeaseLock | None = lease
        self._managed_root_capability: DirectoryCapability | None = (
            managed_root_capability
        )
        self._root: DirectoryCapability | None = root
        self._root_identity = root.identity
        self._root_filesystem = root.filesystem
        self._heartbeat: FileCapability | None = heartbeat
        self._heartbeat_identity = heartbeat_identity
        self._children: dict[
            str, tuple[FileIdentity, FilesystemIdentity]
        ] = {}
        self._cleanup_ready = False
        self._registry_lock = threading.RLock()
        self._registry_generation = 0
        self._registry_frozen = False

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
        """Publish one run graph.

        ``_reclaimer`` is a private Task 9 activation seam. Its production
        default intentionally performs no stale-root reclamation until that
        lifecycle is capability-backed.
        """
        selected_backend = (
            default_filesystem_backend() if backend is None else backend
        )
        managed_root_capability: DirectoryCapability | None = None
        managed_root, managed_root_capability = _ensure_managed_root(
            parent.resolve(strict=True), backend=selected_backend
        )
        try:
            validate_reported_path(managed_root)
            if _reclaimer is not None:
                for item in _reclaimer(managed_root):
                    if isinstance(item, JanitorDiagnostic):
                        if stale_diagnostics is not None:
                            stale_diagnostics.append(item)
                    elif stale_cleanup is not None:
                        stale_cleanup.append(item)
            run_id = str(uuid.uuid4()) if run_id is None else run_id
            if str(uuid.UUID(run_id)) != run_id:
                raise ValueError("run ID must be a canonical UUID")
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
        child_capability: DirectoryCapability | None = None
        try:
            child_capability = self._backend.create_directory(
                root, name, SharePolicy.PINNED
            )
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
            self._children[name] = (
                child_capability.identity,
                child_capability.filesystem,
            )
            child_capability.close()
        except BaseException as primary_error:
            self._children.pop(name, None)
            if child_capability is not None and child_capability.is_open:
                for rollback_error in _delete_owned_directory(
                    root,
                    name,
                    child_capability,
                    self._backend,
                    label="managed child rollback",
                ):
                    primary_error.add_note(rollback_error)
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
            identity, filesystem = expected
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

    def _remove_child_unlocked(self, child: Path) -> ScratchCleanupRecord:
        if child.parent != self.path or _CHILD_NAME.fullmatch(child.name) is None:
            return self._failed("cleanup target is not a direct managed child")
        expected_identity = self._children.get(child.name)
        if expected_identity is None:
            return self._failed("cleanup target is not an owned managed child")
        started = time.monotonic()
        deadline = started + OWNER_CLEANUP_SECONDS
        child_fd = -1
        try:
            current_identity = _entry_identity(self._root_fd, child.name)
            if time.monotonic() >= deadline:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    0,
                    0,
                    remaining_root=validate_reported_path(self.path),
                )
            if current_identity != expected_identity:
                return self._failed(
                    "managed child identity changed", deadline
                )
            child_fd = _open_directory_at(
                self._root_fd, child.name, deadline=deadline
            )
            opened_identity = _directory_identity(child_fd)
            if time.monotonic() >= deadline:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    0,
                    0,
                    remaining_root=validate_reported_path(self.path),
                )
            if opened_identity != expected_identity:
                return self._failed(
                    "managed child identity changed while opening", deadline
                )
            examined, removed, complete = _remove_payload(
                child_fd,
                started=started,
                absolute_deadline=deadline,
                examined=0,
                removed=0,
            )
            if not complete:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    examined,
                    removed,
                    remaining_root=validate_reported_path(self.path),
                )
            if time.monotonic() >= deadline:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    examined,
                    removed,
                    remaining_root=validate_reported_path(self.path),
                )
            current_identity = _entry_identity(self._root_fd, child.name)
            if time.monotonic() >= deadline:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    examined,
                    removed,
                    remaining_root=validate_reported_path(self.path),
                )
            if current_identity != expected_identity:
                return self._failed(
                    "managed child identity changed before removal", deadline
                )
            os.rmdir(child.name, dir_fd=self._root_fd)
            if time.monotonic() >= deadline:
                self._children.pop(child.name, None)
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.CLEAN,
                    examined,
                    removed + 1,
                )
            try:
                os.stat(
                    child.name,
                    dir_fd=self._root_fd,
                    follow_symlinks=False,
                )
            except FileNotFoundError:
                pass
            else:
                if time.monotonic() >= deadline:
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        examined,
                        removed,
                        ("managed child recovery deadline reached",),
                        validate_reported_path(self.path),
                    )
                return self._failed(
                    "managed child remains after removal", deadline
                )
            self._children.pop(child.name, None)
            return ScratchCleanupRecord(
                ScratchCleanupStatus.CLEAN,
                examined,
                removed + 1,
            )
        except _DeadlineExceeded as error:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                0,
                0,
                (f"{type(error).__name__}: {error}",),
                validate_reported_path(self.path),
            )
        except FileNotFoundError:
            if time.monotonic() >= deadline:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    0,
                    0,
                    ("managed child recovery deadline reached",),
                    validate_reported_path(self.path),
                )
            try:
                _recovered_identity = _entry_identity(
                    self._root_fd, child.name
                )
            except FileNotFoundError:
                self._children.pop(child.name, None)
                return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 0, 0)
            except OSError as error:
                if time.monotonic() >= deadline:
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        0,
                        0,
                        ("managed child recovery deadline reached",),
                        validate_reported_path(self.path),
                    )
                return self._failed(
                    f"managed child absence verification failed: {error}",
                    deadline,
                )
            if time.monotonic() >= deadline:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    0,
                    0,
                    ("managed child recovery deadline reached",),
                    validate_reported_path(self.path),
                )
            return self._failed(
                "managed child traversal entry vanished while root remains",
                deadline,
            )
        except OSError as error:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                0,
                0,
                (f"{type(error).__name__}: {error}",),
                validate_reported_path(self.path),
            )
        finally:
            if child_fd >= 0:
                os.close(child_fd)

    def mark_cleanup_ready(self) -> None:
        if self._cleanup_ready:
            return
        _write_marker_at(
            self._root_fd,
            CLEANUP_READY_FILE,
            _marker(self.run_id, self.lease_id),
        )
        self._cleanup_ready = True

    def retain(self) -> ScratchCleanupRecord:
        try:
            os.stat(RETAIN_FILE, dir_fd=self._root_fd, follow_symlinks=False)
        except FileNotFoundError:
            _write_marker_at(
                self._root_fd,
                RETAIN_FILE,
                _marker(self.run_id, self.lease_id),
            )
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
        started = time.monotonic()
        absolute_deadline = started + time_budget
        deleting = self.managed_root / f".deleting-{self.run_id}"
        validate_reported_path(deleting)
        try:
            managed_root_capability = self._managed_root_capability
            if managed_root_capability is None:
                raise OSError("managed root capability is unavailable")
            coordinator = _open_coordinator(
                managed_root_capability,
                self._backend,
                timeout=min(5.0, max(0.001, time_budget)),
                deadline=absolute_deadline,
            )
        except _DeadlineExceeded as error:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                0,
                0,
                (f"{type(error).__name__}: {error}",),
                validate_reported_path(self.path),
            )
        except OSError as error:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                0,
                0,
                (f"{type(error).__name__}: {error}",),
                validate_reported_path(self.path),
            )
        claim_record: ScratchCleanupRecord | None = None
        try:
            if time.monotonic() >= absolute_deadline:
                claim_record = ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    0,
                    0,
                    remaining_root=validate_reported_path(self.path),
                )
            else:
                match = _RUN_NAME.fullmatch(self.path.name)
                if match is None or match.group(1) != self.run_id:
                    claim_record = self._failed(
                        "managed root name changed", absolute_deadline
                    )
                elif (
                    _entry_identity(self._managed_root_fd, self.path.name)
                    != self._root_identity
                ):
                    claim_record = self._failed(
                        "managed root identity changed", absolute_deadline
                    )
                elif time.monotonic() >= absolute_deadline:
                    claim_record = ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        0,
                        0,
                        remaining_root=validate_reported_path(self.path),
                    )
                elif self.path != deleting:
                    try:
                        os.stat(
                            deleting.name,
                            dir_fd=self._managed_root_fd,
                            follow_symlinks=False,
                        )
                    except FileNotFoundError:
                        if time.monotonic() >= absolute_deadline:
                            claim_record = ScratchCleanupRecord(
                                ScratchCleanupStatus.DEFERRED,
                                0,
                                0,
                                remaining_root=validate_reported_path(self.path),
                            )
                        else:
                            os.rename(
                                self.path.name,
                                deleting.name,
                                src_dir_fd=self._managed_root_fd,
                                dst_dir_fd=self._managed_root_fd,
                            )
                            self.path = deleting
                            if time.monotonic() >= absolute_deadline:
                                claim_record = ScratchCleanupRecord(
                                    ScratchCleanupStatus.DEFERRED,
                                    0,
                                    0,
                                    remaining_root=validate_reported_path(
                                        deleting
                                    ),
                                )
                            elif (
                                _entry_identity(
                                    self._managed_root_fd, deleting.name
                                )
                                != self._root_identity
                            ):
                                claim_record = self._failed(
                                    "managed root identity changed while claiming cleanup",
                                    absolute_deadline,
                                )
                    else:
                        claim_record = self._failed(
                            "managed deleting destination already exists",
                            absolute_deadline,
                        )
        except OSError as error:
            claim_record = self._failed(
                f"cleanup claim failed: {type(error).__name__}: {error}",
                absolute_deadline,
            )
        coordinator_errors = _close_lease_lock_all(
            coordinator, "cleanup claim coordinator"
        )
        if coordinator_errors:
            if claim_record is None:
                claim_record = ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    0,
                    0,
                    coordinator_errors,
                    validate_reported_path(self.path),
                )
            else:
                claim_record = replace(
                    claim_record,
                    details=(*claim_record.details, *coordinator_errors),
                    remaining_root=claim_record.remaining_root,
                )
        if claim_record is not None:
            return claim_record
        examined = 0
        removed = 0
        details: list[str] = []
        try:
            while True:
                slice_examined, slice_removed, complete = _remove_payload(
                    self._root_fd,
                    started=time.monotonic(),
                    absolute_deadline=absolute_deadline,
                    examined=0,
                    removed=0,
                )
                examined += slice_examined
                removed += slice_removed
                if complete:
                    break
                if time.monotonic() >= absolute_deadline:
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        examined,
                        removed,
                        remaining_root=validate_reported_path(deleting),
                    )
                if slice_examined == 0 and slice_removed == 0:
                    return ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        examined,
                        removed,
                        ("cleanup slice made no progress",),
                        validate_reported_path(deleting),
                    )
            remaining = absolute_deadline - time.monotonic()
            if remaining <= 0:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    examined,
                    removed,
                    remaining_root=validate_reported_path(deleting),
                )
            try:
                managed_root_capability = self._managed_root_capability
                if managed_root_capability is None:
                    raise OSError("managed root capability is unavailable")
                tail_coordinator = _open_coordinator(
                    managed_root_capability,
                    self._backend,
                    timeout=min(5.0, max(0.001, remaining)),
                    deadline=absolute_deadline,
                )
            except _DeadlineExceeded as error:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    examined,
                    removed,
                    (f"{type(error).__name__}: {error}",),
                    validate_reported_path(deleting),
                )
            except OSError as error:
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    examined,
                    removed,
                    (f"{type(error).__name__}: {error}",),
                    validate_reported_path(deleting),
                )
            tail_record: ScratchCleanupRecord | None = None
            try:
                for marker in (
                    RETAIN_FILE,
                    HEARTBEAT_FILE,
                    CLEANUP_READY_FILE,
                    LEASE_FILE,
                ):
                    if time.monotonic() >= absolute_deadline:
                        tail_record = ScratchCleanupRecord(
                            ScratchCleanupStatus.DEFERRED,
                            examined,
                            removed,
                            remaining_root=validate_reported_path(deleting),
                        )
                        break
                    try:
                        os.unlink(marker, dir_fd=self._root_fd)
                        removed += 1
                    except FileNotFoundError:
                        pass
                if tail_record is None and time.monotonic() >= absolute_deadline:
                    tail_record = ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        examined,
                        removed,
                        remaining_root=validate_reported_path(deleting),
                    )
                if tail_record is None and (
                    _entry_identity(self._managed_root_fd, deleting.name)
                    != self._root_identity
                ):
                    tail_record = self._failed(
                        "managed root identity changed before removal",
                        absolute_deadline,
                    )
                if tail_record is None and time.monotonic() >= absolute_deadline:
                    tail_record = ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        examined,
                        removed,
                        remaining_root=validate_reported_path(deleting),
                    )
                if tail_record is None:
                    os.rmdir(deleting.name, dir_fd=self._managed_root_fd)
                    removed += 1
                    if time.monotonic() < absolute_deadline:
                        try:
                            os.stat(
                                deleting.name,
                                dir_fd=self._managed_root_fd,
                                follow_symlinks=False,
                            )
                        except FileNotFoundError:
                            pass
                        else:
                            tail_record = self._failed(
                                "managed root remains after removal",
                                absolute_deadline,
                            )
            except OSError as error:
                recovered = self._failed(
                    f"cleanup tail failed: {type(error).__name__}: {error}",
                    absolute_deadline,
                )
                tail_record = replace(
                    recovered,
                    examined_entries=examined,
                    removed_entries=removed,
                )
            finally:
                tail_close_errors = _close_lease_lock_all(
                    tail_coordinator, "cleanup coordinator"
                )
            if tail_record is not None:
                return replace(
                    tail_record,
                    details=(*tail_record.details, *tail_close_errors),
                    remaining_root=tail_record.remaining_root,
                )
            close_errors = (
                *tail_close_errors,
                *self.close_capabilities(),
            )
            return ScratchCleanupRecord(
                ScratchCleanupStatus.CLEAN,
                examined,
                removed,
                close_errors,
            )
        except OSError as error:
            details.append(f"{type(error).__name__}: {error}")
            return ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                examined,
                removed,
                tuple(details),
                validate_reported_path(deleting),
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
            return None, False, ("owned root identity search deadline reached",)
        try:
            if (
                _entry_identity(self._managed_root_fd, self.path.name)
                == self._root_identity
            ):
                return self.path, False, ()
        except FileNotFoundError:
            pass
        except OSError as error:
            return None, False, (
                "owned root expected-path lookup failed: "
                f"{type(error).__name__}: {error}",
            )
        if expired():
            return None, False, ("owned root identity search deadline reached",)
        try:
            linked = os.fstat(self._root_fd).st_nlink
        except OSError as error:
            return None, False, (
                "owned root link-state lookup failed: "
                f"{type(error).__name__}: {error}",
            )
        if expired():
            return None, False, ("owned root identity search deadline reached",)
        if linked == 0:
            return None, True, ()
        try:
            capability_path = _directory_path_from_capability(self._root_fd)
            validate_reported_path(capability_path)
        except (OSError, UnicodeError, ValueError):
            capability_path = None
        if expired():
            return None, False, ("owned root identity search deadline reached",)
        if (
            capability_path is not None
            and capability_path.parent == self.managed_root
        ):
            if expired():
                return None, False, (
                    "owned root identity search deadline reached",
                )
            recovered_fd = -1
            try:
                recovered_fd = _open_directory_at(
                    self._managed_root_fd,
                    capability_path.name,
                    deadline=absolute_deadline,
                )
            except FileNotFoundError:
                pass
            except OSError as error:
                return None, False, (
                    "owned root recovered-path open failed: "
                    f"{type(error).__name__}: {error}",
                )
            else:
                try:
                    if expired():
                        return None, False, (
                            "owned root identity search deadline reached",
                        )
                    recovered_identity = _directory_identity(recovered_fd)
                    if expired():
                        return None, False, (
                            "owned root identity search deadline reached",
                        )
                    recovered_filesystem = _filesystem_identity(recovered_fd)
                    if expired():
                        return None, False, (
                            "owned root identity search deadline reached",
                        )
                    root_filesystem = _filesystem_identity(self._root_fd)
                    if expired():
                        return None, False, (
                            "owned root identity search deadline reached",
                        )
                    if (
                        recovered_identity == self._root_identity
                        and recovered_filesystem == root_filesystem
                    ):
                        self.path = capability_path
                        return capability_path, False, ()
                finally:
                    os.close(recovered_fd)
        try:
            scan_fd = _open_directory_at(
                self._managed_root_fd,
                ".",
                deadline=absolute_deadline,
            )
        except OSError as error:
            return None, False, (
                "owned root namespace open failed: "
                f"{type(error).__name__}: {error}",
            )
        try:
            if expired():
                return None, False, (
                    "owned root identity search deadline reached",
                )
            iterator = os.scandir(scan_fd)
            try:
                if expired():
                    return None, False, (
                        "owned root identity search deadline reached",
                    )
                index = 0
                while True:
                    if expired():
                        return None, False, (
                            "owned root identity search deadline reached",
                        )
                    try:
                        entry = next(iterator)
                    except StopIteration:
                        break
                    if expired():
                        return None, False, (
                            "owned root identity search deadline reached",
                        )
                    index += 1
                    if index > 100_000:
                        return None, False, (
                            "managed root identity search exceeds 100000 entries",
                        )
                    try:
                        metadata = os.stat(
                            entry.name,
                            dir_fd=self._managed_root_fd,
                            follow_symlinks=False,
                        )
                    except FileNotFoundError:
                        continue
                    except OSError as error:
                        return None, False, (
                            "owned root namespace lookup failed: "
                            f"{type(error).__name__}: {error}",
                        )
                    if expired():
                        return None, False, (
                            "owned root identity search deadline reached",
                        )
                    if (
                        stat.S_ISDIR(metadata.st_mode)
                        and (metadata.st_dev, metadata.st_ino)
                        == self._root_identity
                    ):
                        candidate = self.managed_root / entry.name
                        validate_reported_path(candidate)
                        self.path = candidate
                        return candidate, False, ()
            finally:
                iterator.close()
        finally:
            os.close(scan_fd)
        if expired():
            return None, False, ("owned root identity search deadline reached",)
        try:
            linked = os.fstat(self._root_fd).st_nlink
        except OSError as error:
            return None, False, (
                "owned root link-state lookup failed: "
                f"{type(error).__name__}: {error}",
            )
        if linked == 0:
            return None, True, ()
        return None, False, (
            "owned root remains linked outside the verified managed namespace",
        )

    def _failed(
        self, message: str, absolute_deadline: float | None = None
    ) -> ScratchCleanupRecord:
        remaining, proven_absent, lookup_details = self._owned_root_path(
            absolute_deadline
        )
        if proven_absent:
            close_errors = self.close_capabilities()
            return ScratchCleanupRecord(
                ScratchCleanupStatus.CLEAN,
                0,
                0,
                (
                    message,
                    "owned root is absent from the managed namespace",
                    *lookup_details,
                    *close_errors,
                ),
            )
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

    def close_capabilities(self) -> tuple[str, ...]:
        errors: list[str] = []
        lease = getattr(self, "_lease", None)
        if lease is not None:
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
            try:
                capability.close()
            except BaseException as error:
                errors.append(
                    _bounded_secondary(f"managed {label} close failed", error)
                )
            else:
                setattr(self, attribute, None)
        return tuple(errors)

    def __del__(self) -> None:
        try:
            self.close_capabilities()
        except BaseException:
            pass


def _remove_payload(
    root_capability: int,
    *,
    started: float,
    absolute_deadline: float,
    examined: int,
    removed: int,
) -> tuple[int, int, bool]:
    control = {LEASE_FILE, HEARTBEAT_FILE, CLEANUP_READY_FILE, RETAIN_FILE}
    if time.monotonic() >= absolute_deadline:
        return examined, removed, False
    root_metadata = os.fstat(root_capability)
    if time.monotonic() >= absolute_deadline:
        return examined, removed, False
    root_identity = (root_metadata.st_dev, root_metadata.st_ino)
    if time.monotonic() >= absolute_deadline:
        return examined, removed, False
    root_filesystem = _filesystem_identity(root_capability)
    if time.monotonic() >= absolute_deadline:
        return examined, removed, False
    components: list[tuple[str, tuple[int, int]]] = []
    operation_deadline = min(
        absolute_deadline, started + MAX_CLEANUP_SLICE_SECONDS
    )

    def exhausted() -> bool:
        return (
            examined >= MAX_CLEANUP_SLICE_ENTRIES
            or time.monotonic() - started >= MAX_CLEANUP_SLICE_SECONDS
            or time.monotonic() >= absolute_deadline
        )

    def observe(count: int) -> None:
        if count > MAX_CLEANUP_OPEN_DIRECTORIES:
            raise OSError(
                "cleanup directory handle limit exceeded: "
                f"{count} > {MAX_CLEANUP_OPEN_DIRECTORIES}"
            )
        _cleanup_handle_observer(count)

    def reopen_current() -> int:
        if exhausted():
            raise _DeadlineExceeded("cleanup slice deadline reached")
        current_fd = _open_directory_at(
            root_capability, ".", deadline=operation_deadline
        )
        observe(2)
        try:
            if exhausted():
                raise _DeadlineExceeded("cleanup slice deadline reached")
            if _directory_identity(current_fd) != root_identity:
                raise OSError("cleanup root identity changed while reopening")
            if exhausted():
                raise _DeadlineExceeded("cleanup slice deadline reached")
            for name, expected_identity in components:
                if exhausted():
                    raise _DeadlineExceeded("cleanup slice deadline reached")
                child_fd = _open_directory_at(
                    current_fd, name, deadline=operation_deadline
                )
                observe(3)
                try:
                    if exhausted():
                        raise _DeadlineExceeded("cleanup slice deadline reached")
                    if _directory_identity(child_fd) != expected_identity:
                        raise OSError(
                            "cleanup cursor identity changed while reopening"
                        )
                    if exhausted():
                        raise _DeadlineExceeded("cleanup slice deadline reached")
                    if _filesystem_identity(child_fd) != root_filesystem:
                        raise OSError(
                            "cleanup cursor crossed a filesystem boundary"
                        )
                    if exhausted():
                        raise _DeadlineExceeded("cleanup slice deadline reached")
                except BaseException:
                    os.close(child_fd)
                    raise
                os.close(current_fd)
                current_fd = child_fd
                observe(2)
            return current_fd
        except BaseException:
            os.close(current_fd)
            raise

    current_fd = reopen_current()
    try:
        while True:
            if exhausted():
                return examined, removed, False
            if os.name != "nt":
                os.fchmod(current_fd, 0o700)
            if exhausted():
                return examined, removed, False
            selected: tuple[str, os.stat_result] | None = None
            iterator = os.scandir(current_fd)
            try:
                observe(3)
                if exhausted():
                    return examined, removed, False
                while True:
                    if exhausted():
                        return examined, removed, False
                    try:
                        entry = next(iterator)
                    except StopIteration:
                        break
                    if not components and entry.name in control:
                        continue
                    if exhausted():
                        return examined, removed, False
                    try:
                        metadata = os.stat(
                            entry.name,
                            dir_fd=current_fd,
                            follow_symlinks=False,
                        )
                    except FileNotFoundError:
                        continue
                    if exhausted():
                        return examined, removed, False
                    selected = (entry.name, metadata)
                    break
            finally:
                iterator.close()
            observe(2)

            if selected is None:
                if not components:
                    return examined, removed, True
                child_name, child_identity = components.pop()
                os.close(current_fd)
                current_fd = -1
                observe(1)
                current_fd = reopen_current()
                if exhausted():
                    return examined, removed, False
                try:
                    current_identity = _entry_identity(current_fd, child_name)
                    if exhausted():
                        return examined, removed, False
                    if current_identity != child_identity:
                        raise OSError(
                            "cleanup directory identity changed before removal"
                        )
                except FileNotFoundError:
                    continue
                if exhausted():
                    return examined, removed, False
                os.rmdir(child_name, dir_fd=current_fd)
                removed += 1
                continue

            name, metadata = selected
            examined += 1
            if stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode):
                child_depth = len(components) + 1
                if child_depth > MAX_CLEANUP_DEPTH:
                    raise OSError(f"cleanup depth exceeds {MAX_CLEANUP_DEPTH}")
                cursor_bytes = sum(
                    len(os.fsencode(component)) + 1
                    for component, _ in components
                ) + len(os.fsencode(name)) + 1
                if cursor_bytes > MAX_CLEANUP_CURSOR_BYTES:
                    raise OSError(
                        "cleanup cursor exceeds "
                        f"{MAX_CLEANUP_CURSOR_BYTES} encoded bytes"
                    )
                if metadata.st_dev != root_metadata.st_dev:
                    raise OSError("cleanup refuses to cross a filesystem boundary")
                if exhausted():
                    return examined, removed, False
                child_fd = _open_directory_at(
                    current_fd, name, deadline=operation_deadline
                )
                observe(3)
                if exhausted():
                    os.close(child_fd)
                    return examined, removed, False
                opened_identity = _directory_identity(child_fd)
                if exhausted():
                    os.close(child_fd)
                    return examined, removed, False
                if opened_identity != (metadata.st_dev, metadata.st_ino):
                    os.close(child_fd)
                    raise OSError(
                        "cleanup directory identity changed while opening"
                    )
                if _filesystem_identity(child_fd) != root_filesystem:
                    os.close(child_fd)
                    raise OSError(
                        "cleanup refuses to cross a filesystem boundary"
                    )
                if exhausted():
                    os.close(child_fd)
                    return examined, removed, False
                if os.name != "nt":
                    os.fchmod(child_fd, 0o700)
                if exhausted():
                    os.close(child_fd)
                    return examined, removed, False
                components.append((name, opened_identity))
                os.close(current_fd)
                current_fd = child_fd
                observe(2)
                continue

            if exhausted():
                return examined, removed, False
            try:
                current = os.stat(
                    name,
                    dir_fd=current_fd,
                    follow_symlinks=False,
                )
            except FileNotFoundError:
                continue
            if exhausted():
                return examined, removed, False
            if (current.st_dev, current.st_ino, current.st_mode) != (
                metadata.st_dev,
                metadata.st_ino,
                metadata.st_mode,
            ):
                raise OSError("cleanup entry identity changed before removal")
            if exhausted():
                return examined, removed, False
            os.unlink(name, dir_fd=current_fd)
            removed += 1
    except _DeadlineExceeded:
        return examined, removed, False
    finally:
        if current_fd >= 0:
            os.close(current_fd)


def _cleanup_handle_observer(_open_directories: int) -> None:
    """Test seam for the capability bound; production intentionally does nothing."""


def _read_valid_marker_at(
    directory_fd: int,
    filename: str,
    run_id: str,
    lease_id: str | None = None,
    *,
    deadline: float | None = None,
) -> dict[str, object] | None:
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    if deadline is not None:
        _check_deadline(deadline, "janitor marker read")
    try:
        fd = os.open(filename, flags, dir_fd=directory_fd)
    except FileNotFoundError:
        return None
    try:
        if deadline is not None:
            _check_deadline(deadline, "janitor marker read")
        metadata = os.fstat(fd)
        if deadline is not None:
            _check_deadline(deadline, "janitor marker read")
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > MARKER_CAPACITY:
            return None
        encoded = os.read(fd, MARKER_CAPACITY + 1)
        if deadline is not None:
            _check_deadline(deadline, "janitor marker read")
        if len(encoded) > MARKER_CAPACITY:
            return None
        def exact_marker(
            pairs: list[tuple[str, object]],
        ) -> dict[str, object]:
            if tuple(key for key, _value in pairs) != _MARKER_FIELDS:
                raise ValueError("managed marker fields are not canonical")
            return dict(pairs)

        value = json.loads(
            encoded.decode("utf-8", errors="strict"),
            object_pairs_hook=exact_marker,
        )
    except TimeoutError:
        raise
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError):
        return None
    finally:
        active_error = sys.exc_info()[1]
        close_errors = _close_descriptors_all(
            (("janitor marker", fd),)
        )
        if close_errors:
            if active_error is not None:
                for close_error in close_errors:
                    active_error.add_note(close_error)
            else:
                raise OSError("; ".join(close_errors))
    if (
        not isinstance(value, dict)
        or type(value.get("schema_version")) is not int
        or value.get("schema_version") != 1
        or value.get("owner_kind") != OWNER_KIND
        or not isinstance(value.get("run_id"), str)
        or value.get("run_id") != run_id
        or not isinstance(value.get("lease_id"), str)
        or (lease_id is not None and value.get("lease_id") != lease_id)
    ):
        return None
    return value


def _reclaim_empty_unleased_candidate(
    managed_root: Path,
    managed_fd: int,
    name: str,
    *,
    current_time: float,
    deadline: float,
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    is_deleting = name.startswith(".deleting-")
    is_staging = name.startswith(".staging-")
    if not is_deleting and not is_staging:
        return None
    candidate = managed_root / name
    candidate_fd = -1
    identity: tuple[int, int] | None = None
    removal_did_not_unlink = False
    try:
        _check_deadline(deadline, "empty janitor")
        candidate_fd = _open_directory_at(
            managed_fd, name, deadline=deadline
        )
        _check_deadline(deadline, "empty janitor")
        metadata = os.fstat(candidate_fd)
        _check_deadline(deadline, "empty janitor")
        if (
            is_staging
            and metadata.st_mtime > current_time - STALE_AFTER_SECONDS
        ):
            return None
        identity = _directory_identity(candidate_fd)
        _check_deadline(deadline, "empty janitor")
        if not _directory_is_empty_at(candidate_fd, deadline=deadline):
            return None
        _check_deadline(deadline, "empty janitor")
        coordinator = cast(Any, _open_coordinator)(
            managed_root,
            root_fd=managed_fd,
            timeout=max(0.001, min(5.0, deadline - time.monotonic())),
            deadline=deadline,
        )
        removed_root = False
        eligible = True
        try:
            _check_deadline(deadline, "empty janitor claim")
            if time.monotonic() >= deadline:
                eligible = False
            if eligible and _entry_identity(managed_fd, name) != identity:
                eligible = False
            if eligible:
                _check_deadline(deadline, "empty janitor claim")
            if eligible and not _directory_is_empty_at(
                candidate_fd, deadline=deadline
            ):
                eligible = False
            if eligible and time.monotonic() >= deadline:
                eligible = False
            if eligible:
                os.rmdir(name, dir_fd=managed_fd)
                removed_root = True
                if time.monotonic() < deadline:
                    try:
                        os.stat(
                            name,
                            dir_fd=managed_fd,
                            follow_symlinks=False,
                        )
                    except FileNotFoundError:
                        pass
                    else:
                        removed_root = False
                        removal_did_not_unlink = True
        finally:
            coordinator_close_errors = _close_lease_lock_all(
                coordinator, "empty janitor coordinator"
            )
        if removed_root:
            return ScratchCleanupRecord(
                ScratchCleanupStatus.CLEAN,
                0,
                1,
                coordinator_close_errors,
            )
        if removal_did_not_unlink:
            detail = "empty candidate remains after removal"
            if coordinator_close_errors:
                detail += "; " + "; ".join(coordinator_close_errors)
            return _janitor_failure(detail, candidate)
        if coordinator_close_errors:
            if time.monotonic() >= deadline:
                return _janitor_diagnostic(
                    "empty janitor deadline reached; "
                    + "; ".join(coordinator_close_errors)
                )
            try:
                remaining_valid = (
                    identity is not None
                    and _entry_identity(managed_fd, name) == identity
                )
            except OSError:
                remaining_valid = False
            return (
                _janitor_failure(
                    "; ".join(coordinator_close_errors), candidate
                )
                if remaining_valid
                else _janitor_diagnostic(
                    "; ".join(coordinator_close_errors)
                )
            )
        return None
    except _DeadlineExceeded as error:
        return _janitor_diagnostic(str(error))
    except FileNotFoundError:
        if time.monotonic() >= deadline:
            return _janitor_diagnostic(
                "empty janitor deadline reached during disappearance recovery"
            )
        try:
            remaining_identity = _entry_identity(managed_fd, name)
        except FileNotFoundError:
            return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 0, 0)
        except OSError as error:
            return _janitor_diagnostic(
                f"empty candidate absence verification failed: {error}"
            )
        detail = "empty candidate traversal entry vanished while root remains"
        return (
            _janitor_failure(detail, candidate)
            if identity is not None and remaining_identity == identity
            else _janitor_diagnostic(detail)
        )
    except OSError as error:
        remaining_valid = False
        if identity is not None and time.monotonic() < deadline:
            try:
                remaining_valid = _entry_identity(managed_fd, name) == identity
            except OSError:
                pass
        detail = f"{type(error).__name__}: {error}"
        return (
            _janitor_failure(detail, candidate)
            if remaining_valid
            else _janitor_diagnostic(detail)
        )
    finally:
        if candidate_fd >= 0:
            os.close(candidate_fd)


def reclaim_abandoned(
    managed_root: Path,
    *,
    now: float | None = None,
    managed_root_fd: int | None = None,
) -> list[ScratchCleanupRecord | JanitorDiagnostic]:
    """Reclaim a bounded set of valid unlocked roots.

    Fresh/malformed/missing heartbeats are preserved unless a valid
    cleanup-ready marker proves the previous run reached quiescence.
    """
    current_time = time.time() if now is None else now
    janitor_started = time.monotonic()
    janitor_deadline = janitor_started + JANITOR_CLEANUP_SECONDS
    selection_deadline = min(
        janitor_deadline, janitor_started + JANITOR_SELECTION_SECONDS
    )
    if managed_root_fd is None:
        verified_root, task9_root_owner = _ensure_managed_root(
            managed_root.parent.resolve(strict=True),
            deadline=selection_deadline,
        )
        # Task 9 replaces this isolated legacy descriptor boundary.
        selection_root_fd: int = cast(Any, task9_root_owner)
        if verified_root != managed_root.resolve(strict=True):
            os.close(selection_root_fd)
            raise OSError("janitor managed root path changed")
    else:
        selection_root_fd = _open_directory_at(
            managed_root_fd,
            ".",
            deadline=selection_deadline,
        )
    selection_diagnostics: list[JanitorDiagnostic] = []
    selection_failed = False
    try:
        selection_remaining = selection_deadline - time.monotonic()
        if selection_remaining <= 0:
            raise _DeadlineExceeded("janitor selection deadline exceeded")
        coordinator = cast(Any, _open_coordinator)(
            managed_root,
            root_fd=selection_root_fd,
            timeout=selection_remaining,
            deadline=selection_deadline,
        )
    except BaseException as primary_error:
        for close_error in _close_descriptors_all(
            (("janitor selection root", selection_root_fd),)
        ):
            primary_error.add_note(close_error)
        raise
    try:
        _check_deadline(selection_deadline, "janitor selection")
        state = _read_coordinator_state(
            coordinator.fd,
            deadline=selection_deadline,
        )
        _check_deadline(selection_deadline, "janitor selection")
        after: list[str] = []
        wrapped: list[str] = []
        iterator = os.scandir(selection_root_fd)
        try:
            _check_deadline(selection_deadline, "janitor selection")
            examined = 0
            while True:
                _check_deadline(selection_deadline, "janitor selection")
                entry = next(iterator, None)
                _check_deadline(selection_deadline, "janitor selection")
                if entry is None:
                    break
                examined += 1
                if examined > 100_000:
                    selection_diagnostics.append(
                        _janitor_diagnostic(
                            "janitor direct-child scan exceeds 100000 entries"
                        )
                    )
                    selection_failed = True
                    break
                if _RUN_NAME.fullmatch(entry.name) is None:
                    continue
                target = after if entry.name > state.cursor else wrapped
                bisect.insort(target, entry.name)
                if len(target) > MAX_RECLAIM_CANDIDATES:
                    target.pop()
        except _DeadlineExceeded as error:
            selection_diagnostics.append(_janitor_diagnostic(str(error)))
            selection_failed = True
        finally:
            iterator.close()
        candidates = [] if selection_failed else (after if after else wrapped)
        if candidates:
            try:
                _check_deadline(selection_deadline, "janitor selection")
                _persist_coordinator_cursor(
                    coordinator.fd,
                    state,
                    candidates[-1],
                    deadline=selection_deadline,
                )
                _check_deadline(selection_deadline, "janitor selection")
            except OSError as error:
                selection_diagnostics.append(
                    _janitor_diagnostic(
                        f"janitor cursor persistence failed: "
                        f"{type(error).__name__}: {error}"
                    )
                )
    except BaseException as primary_error:
        for close_error in _close_descriptors_all(
            (("janitor selection root", selection_root_fd),)
        ):
            primary_error.add_note(close_error)
        raise
    finally:
        coordinator_close_errors = _close_lease_lock_all(
            coordinator, "janitor selection coordinator"
        )
        if coordinator_close_errors:
            active_error = sys.exc_info()[1]
            if active_error is not None:
                for close_error in coordinator_close_errors:
                    active_error.add_note(close_error)
            else:
                selection_diagnostics.append(
                    _janitor_diagnostic("; ".join(coordinator_close_errors))
                )
    records: list[ScratchCleanupRecord | JanitorDiagnostic] = [
        *selection_diagnostics
    ]
    deferred: list[tuple[int, str, tuple[int, int]]] = []
    for name in candidates:
        remaining = JANITOR_CLEANUP_SECONDS - (
            time.monotonic() - janitor_started
        )
        if remaining <= 0:
            break
        match = _RUN_NAME.fullmatch(name)
        if match is None:
            continue
        run_id = match.group(1)
        candidate = managed_root / name
        lease: LeaseLock | None = None
        candidate_validated = False
        managed_root_fd = -1
        root_fd = -1
        root_identity: tuple[int, int] | None = None
        flags = os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)
        try:
            _check_deadline(janitor_deadline, "janitor candidate")
            managed_root_fd = _open_directory_at(
                selection_root_fd, ".", deadline=janitor_deadline
            )
            _check_deadline(janitor_deadline, "janitor candidate")
            root_fd = _open_directory_at(
                managed_root_fd,
                candidate.name,
                deadline=janitor_deadline,
            )
            _check_deadline(janitor_deadline, "janitor candidate")
            root_identity = _directory_identity(root_fd)
            _check_deadline(janitor_deadline, "janitor candidate")
            if (
                _entry_identity(managed_root_fd, candidate.name)
                != root_identity
            ):
                raise OSError("janitor candidate identity changed")
            _check_deadline(janitor_deadline, "janitor candidate")
            lease_value = _read_valid_marker_at(
                root_fd,
                LEASE_FILE,
                run_id,
                deadline=janitor_deadline,
            )
            if lease_value is None:
                recovered = _reclaim_empty_unleased_candidate(
                    managed_root,
                    managed_root_fd,
                    name,
                    current_time=current_time,
                    deadline=janitor_deadline,
                )
                if recovered is not None:
                    records.append(recovered)
                elif not candidate.name.startswith(".staging-"):
                    records.append(
                        _janitor_diagnostic(
                            "janitor candidate has an invalid, missing, or "
                            "unknown lease marker"
                        )
                    )
                continue
            lease_id = lease_value["lease_id"]
            if not isinstance(lease_id, str):
                continue
            _check_deadline(janitor_deadline, "janitor candidate")
            lease_fd = os.open(
                LEASE_FILE, flags, dir_fd=root_fd
            )
            try:
                _check_deadline(janitor_deadline, "janitor candidate")
            except BaseException as primary_error:
                for close_error in _close_descriptors_all(
                    (("janitor candidate lease", lease_fd),)
                ):
                    primary_error.add_note(close_error)
                raise
            lease_metadata = os.fstat(lease_fd)
            _check_deadline(janitor_deadline, "janitor candidate")
            lease_entry = os.stat(
                LEASE_FILE,
                dir_fd=root_fd,
                follow_symlinks=False,
            )
            _check_deadline(janitor_deadline, "janitor candidate")
            if (
                not stat.S_ISREG(lease_metadata.st_mode)
                or (lease_metadata.st_dev, lease_metadata.st_ino)
                != (lease_entry.st_dev, lease_entry.st_ino)
            ):
                identity_error = OSError(
                    "janitor lease identity changed while opening"
                )
                for close_error in _close_descriptors_all(
                    (("janitor candidate lease", lease_fd),)
                ):
                    identity_error.add_note(close_error)
                raise identity_error
            lease = LeaseLock(lease_fd)
            try:
                _check_deadline(janitor_deadline, "janitor candidate")
                lease.acquire(blocking=False)
                _check_deadline(janitor_deadline, "janitor candidate")
            except OSError as error:
                if isinstance(error, BlockingIOError) or error.errno in {
                    errno.EACCES,
                    errno.EAGAIN,
                    errno.EWOULDBLOCK,
                }:
                    continue
                raise
            if (
                _read_valid_marker_at(
                    root_fd,
                    LEASE_FILE,
                    run_id,
                    deadline=janitor_deadline,
                )
                != lease_value
            ):
                raise OSError("janitor lease changed after locking")
            candidate_validated = True
            retained = _read_valid_marker_at(
                root_fd,
                RETAIN_FILE,
                run_id,
                lease_id,
                deadline=janitor_deadline,
            )
            if retained is not None:
                continue
            ready = _read_valid_marker_at(
                root_fd,
                CLEANUP_READY_FILE,
                run_id,
                lease_id,
                deadline=janitor_deadline,
            )
            heartbeat = _read_valid_marker_at(
                root_fd,
                HEARTBEAT_FILE,
                run_id,
                lease_id,
                deadline=janitor_deadline,
            )
            _check_deadline(janitor_deadline, "janitor candidate")
            stale = False
            if heartbeat is not None:
                try:
                    modified = os.stat(
                        HEARTBEAT_FILE,
                        dir_fd=root_fd,
                        follow_symlinks=False,
                    ).st_mtime
                except OSError:
                    modified = current_time
                _check_deadline(janitor_deadline, "janitor candidate")
                stale = modified <= current_time - STALE_AFTER_SECONDS
            lease_only = _directory_contains_only_lease_at(
                root_fd, deadline=janitor_deadline
            )
            deleting_tail = candidate.name.startswith(".deleting-") and lease_only
            staging_is_old = False
            if candidate.name.startswith(".staging-"):
                _check_deadline(janitor_deadline, "janitor candidate")
                staging_is_old = (
                    os.fstat(root_fd).st_mtime
                    <= current_time - STALE_AFTER_SECONDS
                ) and lease_only
                _check_deadline(janitor_deadline, "janitor candidate")
            if candidate.name.startswith(".staging-"):
                if not staging_is_old:
                    continue
            elif ready is None and not stale and not deleting_tail:
                continue
            if time.monotonic() >= janitor_deadline:
                break
            deleting = managed_root / f".deleting-{run_id}"
            coordinator = cast(Any, _open_coordinator)(
                managed_root,
                root_fd=managed_root_fd,
                timeout=min(
                    5.0,
                    max(0.001, janitor_deadline - time.monotonic()),
                ),
                deadline=janitor_deadline,
            )
            try:
                if time.monotonic() >= janitor_deadline:
                    break
                if candidate != deleting:
                    try:
                        os.stat(
                            deleting.name,
                            dir_fd=managed_root_fd,
                            follow_symlinks=False,
                        )
                    except FileNotFoundError:
                        pass
                    else:
                        continue
                    if time.monotonic() >= janitor_deadline:
                        break
                    os.rename(
                        candidate.name,
                        deleting.name,
                        src_dir_fd=managed_root_fd,
                        dst_dir_fd=managed_root_fd,
                    )
                    candidate = deleting
                    if time.monotonic() >= janitor_deadline:
                        pass
                    elif (
                        _entry_identity(managed_root_fd, deleting.name)
                        != root_identity
                    ):
                        raise OSError(
                            "janitor candidate identity changed while claiming"
                        )
            finally:
                active_error = sys.exc_info()[1]
                coordinator_close_errors = _close_lease_lock_all(
                    coordinator, "janitor claim coordinator"
                )
                if coordinator_close_errors:
                    if active_error is not None:
                        for close_error in coordinator_close_errors:
                            active_error.add_note(close_error)
                    else:
                        raise OSError("; ".join(coordinator_close_errors))
            managed = cast(Any, ManagedScratch)(
                managed_root=managed_root,
                path=deleting,
                run_id=run_id,
                lease_id=lease_id,
                lease=lease,
                managed_root_fd=managed_root_fd,
                root_fd=root_fd,
                root_identity=root_identity,
            )
            lease = None
            managed_root_fd = -1
            root_fd = -1
            managed._cleanup_ready = ready is not None
            remaining = janitor_deadline - time.monotonic()
            if remaining <= 0:
                cleanup = managed.defer("janitor cleanup budget exhausted")
                disposal_errors = managed.close_capabilities()
                if disposal_errors:
                    cleanup = replace(
                        cleanup,
                        details=(*cleanup.details, *disposal_errors),
                    )
                records.append(cleanup)
                break
            cleanup = managed.cleanup(
                time_budget=min(MAX_CLEANUP_SLICE_SECONDS, remaining)
            )
            disposal_errors = managed.close_capabilities()
            if disposal_errors:
                cleanup = replace(
                    cleanup,
                    details=(*cleanup.details, *disposal_errors),
                )
            records.append(cleanup)
            if cleanup.status is ScratchCleanupStatus.DEFERRED:
                deferred.append(
                    (len(records) - 1, deleting.name, root_identity)
                )
        except _DeadlineExceeded as error:
            records.append(_janitor_diagnostic(_exception_detail(error)))
            break
        except BaseException as error:
            detail = (
                f"janitor candidate {name} failed: {_exception_detail(error)}"
            )
            remaining_valid = False
            if (
                time.monotonic() < janitor_deadline
                and candidate_validated
                and root_identity is not None
                and managed_root_fd >= 0
            ):
                try:
                    remaining_valid = (
                        _entry_identity(managed_root_fd, candidate.name)
                        == root_identity
                    )
                except OSError:
                    pass
            records.append(
                _janitor_failure(detail, candidate)
                if remaining_valid
                else _janitor_diagnostic(detail)
            )
        finally:
            if lease is not None:
                lease_close_errors = _close_lease_lock_all(
                    lease, "janitor candidate lease"
                )
                if lease_close_errors:
                    remaining_valid = False
                    if (
                        time.monotonic() < janitor_deadline
                        and candidate_validated
                        and root_identity is not None
                        and managed_root_fd >= 0
                    ):
                        try:
                            remaining_valid = (
                                _entry_identity(managed_root_fd, candidate.name)
                                == root_identity
                            )
                        except OSError:
                            pass
                    records.append(
                        _janitor_failure(
                            "; ".join(lease_close_errors), candidate
                        )
                        if remaining_valid
                        else _janitor_diagnostic(
                            "; ".join(lease_close_errors)
                        )
                    )
            descriptor_close_errors = _close_descriptors_all(
                (
                    ("janitor candidate root", root_fd),
                    ("janitor candidate managed root", managed_root_fd),
                )
            )
            if descriptor_close_errors:
                records.append(
                    _janitor_diagnostic("; ".join(descriptor_close_errors))
                )
    for index, name, expected_identity in deferred:
        remaining = janitor_deadline - time.monotonic()
        if remaining <= 0:
            break
        try:
            resumed = _resume_deferred_cleanup(
                managed_root,
                selection_root_fd,
                name,
                expected_identity,
                deadline=janitor_deadline,
            )
        except _DeadlineExceeded as error:
            records.append(_janitor_diagnostic(str(error)))
            break
        except OSError as error:
            detail = (
                "deferred janitor lease disposal failed: "
                f"{type(error).__name__}: {error}"
            )
            remaining_valid = False
            if time.monotonic() < janitor_deadline:
                try:
                    remaining_valid = (
                        _entry_identity(selection_root_fd, name)
                        == expected_identity
                    )
                except OSError:
                    pass
            diagnostic = (
                _janitor_failure(detail, managed_root / name)
                if remaining_valid
                else _janitor_diagnostic(detail)
            )
            prior = records[index]
            if (
                isinstance(prior, ScratchCleanupRecord)
                and isinstance(diagnostic, ScratchCleanupRecord)
            ):
                records[index] = replace(
                    diagnostic,
                    examined_entries=(
                        prior.examined_entries + diagnostic.examined_entries
                    ),
                    removed_entries=(
                        prior.removed_entries + diagnostic.removed_entries
                    ),
                    details=(*prior.details, *diagnostic.details),
                    omitted_detail_count=(
                        prior.omitted_detail_count
                        + diagnostic.omitted_detail_count
                    ),
                )
            else:
                records.append(diagnostic)
            continue
        if resumed is not None:
            prior = records[index]
            if (
                isinstance(prior, ScratchCleanupRecord)
                and isinstance(resumed, ScratchCleanupRecord)
            ):
                resumed = replace(
                    resumed,
                    examined_entries=(
                        prior.examined_entries + resumed.examined_entries
                    ),
                    removed_entries=(
                        prior.removed_entries + resumed.removed_entries
                    ),
                    details=(*prior.details, *resumed.details),
                    omitted_detail_count=(
                        prior.omitted_detail_count
                        + resumed.omitted_detail_count
                    ),
                )
                records[index] = resumed
            else:
                records.append(resumed)
    selection_close_errors = _close_descriptors_all(
        (("janitor selection root", selection_root_fd),)
    )
    if selection_close_errors:
        records.append(_janitor_diagnostic("; ".join(selection_close_errors)))
    return _bound_cleanup_records(records)


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


def _resume_deferred_cleanup(
    managed_root: Path,
    selection_root_fd: int,
    name: str,
    expected_identity: tuple[int, int],
    *,
    deadline: float,
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    match = _RUN_NAME.fullmatch(name)
    if match is None or not name.startswith(".deleting-"):
        return None
    resources: dict[str, object] = {
        "managed_root_fd": -1,
        "root_fd": -1,
        "lease": None,
    }
    try:
        result = _resume_deferred_cleanup_inner(
            managed_root,
            selection_root_fd,
            name,
            expected_identity,
            match.group(1),
            deadline=deadline,
            resources=resources,
        )
    except BaseException as primary_error:
        disposal_errors = _dispose_deferred_resources(resources)
        for disposal_error in disposal_errors:
            primary_error.add_note(disposal_error)
        raise
    disposal_errors = _dispose_deferred_resources(resources)
    if not disposal_errors:
        return result
    if isinstance(result, ScratchCleanupRecord):
        return replace(result, details=(*result.details, *disposal_errors))
    if isinstance(result, JanitorDiagnostic):
        return replace(result, details=(*result.details, *disposal_errors))
    return _janitor_diagnostic("; ".join(disposal_errors))


def _dispose_deferred_resources(resources: dict[str, object]) -> tuple[str, ...]:
    errors: list[str] = []
    lease = resources["lease"]
    resources["lease"] = None
    if isinstance(lease, LeaseLock):
        errors.extend(_close_lease_lock_all(lease, "deferred janitor lease"))
    root_fd = resources["root_fd"]
    managed_root_fd = resources["managed_root_fd"]
    resources["root_fd"] = -1
    resources["managed_root_fd"] = -1
    assert isinstance(root_fd, int)
    assert isinstance(managed_root_fd, int)
    errors.extend(
        _close_descriptors_all(
            (
                ("deferred janitor root", root_fd),
                ("deferred janitor managed root", managed_root_fd),
            )
        )
    )
    return tuple(errors)


def _resume_deferred_cleanup_inner(
    managed_root: Path,
    selection_root_fd: int,
    name: str,
    expected_identity: tuple[int, int],
    run_id: str,
    *,
    deadline: float,
    resources: dict[str, object],
) -> ScratchCleanupRecord | JanitorDiagnostic | None:
    managed_root_fd = -1
    root_fd = -1
    lease: LeaseLock | None = None
    candidate_validated = False
    try:
        _check_deadline(deadline, "deferred janitor")
        managed_root_fd = _open_directory_at(
            selection_root_fd, ".", deadline=deadline
        )
        resources["managed_root_fd"] = managed_root_fd
        _check_deadline(deadline, "deferred janitor")
        try:
            root_fd = _open_directory_at(
                managed_root_fd, name, deadline=deadline
            )
            resources["root_fd"] = root_fd
        except FileNotFoundError:
            return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 0, 0)
        _check_deadline(deadline, "deferred janitor")
        if _directory_identity(root_fd) != expected_identity:
            return _janitor_diagnostic("deferred cleanup identity changed")
        _check_deadline(deadline, "deferred janitor")
        if _entry_identity(managed_root_fd, name) != expected_identity:
            return _janitor_diagnostic("deferred cleanup identity changed")
        _check_deadline(deadline, "deferred janitor")
        lease_value = _read_valid_marker_at(
            root_fd, LEASE_FILE, run_id, deadline=deadline
        )
        if lease_value is None:
            return None
        lease_id = lease_value.get("lease_id")
        if not isinstance(lease_id, str):
            return None
        _check_deadline(deadline, "deferred janitor")
        lease_fd = os.open(
            LEASE_FILE,
            os.O_RDWR | getattr(os, "O_NOFOLLOW", 0),
            dir_fd=root_fd,
        )
        try:
            _check_deadline(deadline, "deferred janitor")
        except BaseException as primary_error:
            for close_error in _close_descriptors_all(
                (("deferred janitor lease", lease_fd),)
            ):
                primary_error.add_note(close_error)
            raise
        lease = LeaseLock(lease_fd)
        resources["lease"] = lease
        try:
            lease.acquire(blocking=False)
            _check_deadline(deadline, "deferred janitor")
        except OSError as error:
            if isinstance(error, BlockingIOError) or error.errno in {
                errno.EACCES,
                errno.EAGAIN,
                errno.EWOULDBLOCK,
            }:
                return None
            return _janitor_diagnostic(
                "deferred janitor lease lock failed: "
                f"{type(error).__name__}: {error}",
            )
        if _read_valid_marker_at(
            root_fd, LEASE_FILE, run_id, deadline=deadline
        ) != lease_value:
            return None
        candidate_validated = True
        ready = _read_valid_marker_at(
            root_fd,
            CLEANUP_READY_FILE,
            run_id,
            lease_id,
            deadline=deadline,
        )
        if ready is None and not _directory_contains_only_lease_at(
            root_fd, deadline=deadline
        ):
            return None
        _check_deadline(deadline, "deferred janitor")
        managed = cast(Any, ManagedScratch)(
            managed_root=managed_root,
            path=managed_root / name,
            run_id=run_id,
            lease_id=lease_id,
            lease=lease,
            managed_root_fd=managed_root_fd,
            root_fd=root_fd,
            root_identity=expected_identity,
        )
        lease = None
        managed_root_fd = -1
        root_fd = -1
        resources["lease"] = None
        resources["managed_root_fd"] = -1
        resources["root_fd"] = -1
        managed._cleanup_ready = ready is not None
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            cleanup = managed.defer("deferred janitor budget exhausted")
        else:
            cleanup = managed.cleanup(
                time_budget=min(MAX_CLEANUP_SLICE_SECONDS, remaining)
            )
        disposal_errors = managed.close_capabilities()
        if disposal_errors:
            cleanup = replace(
                cleanup,
                details=(*cleanup.details, *disposal_errors),
            )
        return cleanup
    except _DeadlineExceeded as error:
        return _janitor_diagnostic(str(error))
    except OSError as error:
        remaining_valid = False
        if candidate_validated and time.monotonic() < deadline:
            try:
                remaining_valid = (
                    _entry_identity(selection_root_fd, name)
                    == expected_identity
                )
            except OSError:
                pass
        detail = f"{type(error).__name__}: {error}"
        return (
            _janitor_failure(detail, managed_root / name)
            if remaining_valid
            else _janitor_diagnostic(detail)
        )


def _directory_is_empty_at(candidate_fd: int, *, deadline: float) -> bool:
    _check_deadline(deadline, "janitor directory scan")
    iterator = os.scandir(candidate_fd)
    try:
        _check_deadline(deadline, "janitor directory scan")
        _check_deadline(deadline, "janitor directory scan")
        entry = next(iterator, None)
        _check_deadline(deadline, "janitor directory scan")
        return entry is None
    finally:
        iterator.close()


def _directory_contains_only_lease_at(
    candidate_fd: int, *, deadline: float | None = None
) -> bool:
    try:
        if deadline is not None:
            _check_deadline(deadline, "janitor lease-only scan")
        iterator = os.scandir(candidate_fd)
        try:
            if deadline is not None:
                _check_deadline(deadline, "janitor lease-only scan")
            seen_lease = False
            while True:
                if deadline is not None:
                    _check_deadline(deadline, "janitor lease-only scan")
                entry = next(iterator, None)
                if deadline is not None:
                    _check_deadline(deadline, "janitor lease-only scan")
                if entry is None:
                    break
                if entry.name != LEASE_FILE or seen_lease:
                    return False
                seen_lease = True
            return seen_lease
        finally:
            iterator.close()
    except TimeoutError:
        raise
    except OSError:
        return False
