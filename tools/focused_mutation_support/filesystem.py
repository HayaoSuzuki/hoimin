from __future__ import annotations

import os
from dataclasses import dataclass
from enum import StrEnum
from functools import cache
from pathlib import Path
from typing import Protocol


@dataclass(frozen=True, slots=True)
class FileIdentity:
    volume: int
    file: int


@dataclass(frozen=True, slots=True)
class FilesystemIdentity:
    volume: int
    discriminator_a: int = 0
    discriminator_b: int = 0


class EntryKind(StrEnum):
    DIRECTORY = "directory"
    REGULAR = "regular"
    REPARSE = "reparse"
    OTHER = "other"


@dataclass(frozen=True, slots=True)
class DirectoryEntry:
    name: str
    kind: EntryKind
    identity: FileIdentity
    filesystem: FilesystemIdentity
    logical_size: int
    modified_ns: int


class SharePolicy(StrEnum):
    SCAN = "scan"
    PINNED = "pinned"
    MUTATION = "mutation"


class SecurityDomain(StrEnum):
    CALLER = "caller"
    MANAGED = "managed"


class FileAccess(StrEnum):
    READ = "read"
    WRITE = "write"
    READ_WRITE = "read_write"


class CreateDisposition(StrEnum):
    OPEN_EXISTING = "open_existing"
    CREATE_NEW = "create_new"
    OPEN_OR_CREATE = "open_or_create"


def validate_component(name: str) -> str:
    invalid = name in {"", ".", ".."} or "\0" in name or os.sep in name
    if os.altsep is not None:
        invalid = invalid or os.altsep in name
    if invalid:
        raise ValueError(f"invalid filesystem component: {name!r}")
    return name


class _CapabilityState(StrEnum):
    OPEN = "open"
    DETACHED = "detached"
    TRANSFERRED = "transferred"
    CLOSED = "closed"


class _Capability:
    __slots__ = (
        "_created",
        "_filesystem",
        "_identity",
        "_kind",
        "_logical_size",
        "_modified_ns",
        "_owner",
        "_path_hint",
        "_resource",
        "_security_domain",
        "_share_policy",
        "_state",
    )

    _valid_kinds: tuple[EntryKind, ...] = ()

    def __init__(
        self,
        owner: object,
        resource: object,
        *,
        identity: FileIdentity,
        filesystem: FilesystemIdentity,
        kind: EntryKind,
        logical_size: int,
        modified_ns: int,
        security_domain: SecurityDomain,
        share_policy: SharePolicy,
        created: bool,
        path_hint: Path,
    ) -> None:
        self._state = _CapabilityState.CLOSED
        self._resource = None
        if identity.volume <= 0 or identity.file <= 0:
            raise ValueError("file identity values must be nonzero")
        if filesystem.volume <= 0:
            raise ValueError("filesystem identity volume must be nonzero")
        if logical_size < 0:
            raise ValueError("logical size must be nonnegative")
        if not any(kind is valid_kind for valid_kind in self._valid_kinds):
            raise ValueError(f"invalid capability entry kind: {kind!r}")
        self._owner = owner
        self._identity = identity
        self._filesystem = filesystem
        self._kind = kind
        self._logical_size = logical_size
        self._modified_ns = modified_ns
        self._security_domain = security_domain
        self._share_policy = share_policy
        self._created = created
        self._path_hint = path_hint
        self._resource = resource
        self._state = _CapabilityState.OPEN

    @property
    def identity(self) -> FileIdentity:
        return self._identity

    @property
    def filesystem(self) -> FilesystemIdentity:
        return self._filesystem

    @property
    def kind(self) -> EntryKind:
        return self._kind

    @property
    def logical_size(self) -> int:
        return self._logical_size

    @property
    def modified_ns(self) -> int:
        return self._modified_ns

    @property
    def security_domain(self) -> SecurityDomain:
        return self._security_domain

    @property
    def share_policy(self) -> SharePolicy:
        return self._share_policy

    @property
    def created(self) -> bool:
        return self._created

    @property
    def path_hint(self) -> Path:
        return self._path_hint

    @property
    def is_open(self) -> bool:
        return self._state is _CapabilityState.OPEN

    @property
    def closed(self) -> bool:
        return self._state is _CapabilityState.CLOSED

    @property
    def detached(self) -> bool:
        return self._state is _CapabilityState.DETACHED

    @property
    def transferred(self) -> bool:
        return self._state is _CapabilityState.TRANSFERRED

    def owned_by(self, owner: object) -> bool:
        return self._owner is owner

    def _resource_for(self, owner: object) -> object:
        if self._owner is not owner:
            raise RuntimeError("filesystem capability belongs to another backend")
        if self._state is not _CapabilityState.OPEN:
            raise RuntimeError("filesystem capability is not open")
        return self._resource

    def close(self) -> None:
        if self._state is not _CapabilityState.OPEN:
            return
        self._owner.close_resource(self._resource)  # type: ignore[attr-defined]
        self._state = _CapabilityState.CLOSED
        self._resource = None

    def __enter__(self) -> _Capability:
        if self._state is not _CapabilityState.OPEN:
            raise RuntimeError("filesystem capability is not open")
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()

    def __del__(self) -> None:
        try:
            self.close()
        except OSError:
            pass


class FileCapability(_Capability):
    __slots__ = ()

    _valid_kinds = (EntryKind.REGULAR, EntryKind.REPARSE, EntryKind.OTHER)

    def detach_to_fd(self, flags: int) -> int:
        resource = self._resource_for(self._owner)
        if self._kind is not EntryKind.REGULAR:
            raise RuntimeError(
                "only regular file capabilities can detach to a descriptor"
            )
        descriptor = self._owner.detach_file_resource(  # type: ignore[attr-defined]
            resource, flags
        )
        self._state = _CapabilityState.DETACHED
        self._resource = None
        return descriptor


class DirectoryCapability(_Capability):
    __slots__ = ()

    _valid_kinds = (EntryKind.DIRECTORY,)

    def _move_for(self, owner: object) -> DirectoryCapability:
        resource = self._resource_for(owner)
        replacement = DirectoryCapability(
            owner,
            resource,
            identity=self._identity,
            filesystem=self._filesystem,
            kind=self._kind,
            logical_size=self._logical_size,
            modified_ns=self._modified_ns,
            security_domain=self._security_domain,
            share_policy=self._share_policy,
            created=self._created,
            path_hint=self._path_hint,
        )
        self._state = _CapabilityState.TRANSFERRED
        self._resource = None
        return replacement


class DirectoryIterator(Protocol):
    """Streaming iterator that owns its borrowed ``directory`` capability."""

    @property
    def directory(self) -> DirectoryCapability:
        raise NotImplementedError

    def __iter__(self) -> DirectoryIterator:
        raise NotImplementedError

    def __next__(self) -> DirectoryEntry:
        raise NotImplementedError

    def close(self) -> None:
        """Close owned resources, retaining ownership when close fails."""
        raise NotImplementedError


class FilesystemBackend(Protocol):
    @property
    def directory_rename_requires_closed_descendants(self) -> bool:
        return False

    def _directory_creation_rollback_available(
        self, directory: DirectoryCapability
    ) -> bool:
        """Return whether a created directory is atomically bound to its handle."""
        raise NotImplementedError

    def open_root(
        self,
        path: Path,
        share_policy: SharePolicy,
        security_domain: SecurityDomain = SecurityDomain.CALLER,
    ) -> DirectoryCapability:
        raise NotImplementedError

    def create_secure_root(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryCapability:
        raise NotImplementedError

    def _commit_secure_root(self, directory: DirectoryCapability) -> None:
        """Disarm rollback-only authority after managed-root handoff."""
        raise NotImplementedError

    def _prepare_secure_root_commit(
        self, directory: DirectoryCapability
    ) -> None:
        """Validate a later non-failing secure-root commit transition."""
        raise NotImplementedError

    def reopen_directory(
        self,
        directory: DirectoryCapability,
        share_policy: SharePolicy | None = None,
    ) -> DirectoryCapability:
        raise NotImplementedError

    def open_directory(
        self, parent: DirectoryCapability, name: str, share_policy: SharePolicy
    ) -> DirectoryCapability:
        raise NotImplementedError

    def create_directory(
        self, parent: DirectoryCapability, name: str, share_policy: SharePolicy
    ) -> DirectoryCapability:
        raise NotImplementedError

    def open_file(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        access: FileAccess,
        disposition: CreateDisposition,
        share_policy: SharePolicy = SharePolicy.MUTATION,
    ) -> FileCapability:
        raise NotImplementedError

    def open_entry(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> FileCapability | DirectoryCapability:
        raise NotImplementedError

    def entry(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryEntry | None:
        raise NotImplementedError

    def entries(self, parent: DirectoryCapability) -> DirectoryIterator:
        """Iterate through a reopened capability, preserving ``parent``."""
        raise NotImplementedError

    def entries_owned(self, parent: DirectoryCapability) -> DirectoryIterator:
        """Move ``parent`` into the iterator after replacement construction."""
        raise NotImplementedError

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        raise NotImplementedError

    def delete(self, capability: FileCapability | DirectoryCapability) -> None:
        """Consume an exact-object capability, then verify relative absence.

        Implementations retain parent/name evidence before namespace mutation,
        close the mutated capability before absence verification, and leave a
        capability retryable when that close fails. A replacement found at the
        same name is reported and is never used as a fallback deletion target.
        """
        raise NotImplementedError

    def available_bytes(self, directory: DirectoryCapability) -> int:
        raise NotImplementedError

    def allocation_unit(self, directory: DirectoryCapability) -> int:
        raise NotImplementedError

    def touch(self, file: FileCapability) -> None:
        raise NotImplementedError

    def flush(self, file: FileCapability) -> None:
        raise NotImplementedError

    def final_path(self, directory: DirectoryCapability) -> Path:
        raise NotImplementedError

    def verify_managed_security(
        self,
        capability: FileCapability | DirectoryCapability,
        *,
        repair_dacl: bool,
    ) -> None:
        raise NotImplementedError


def _platform_name() -> str:
    return os.name


@cache
def default_filesystem_backend() -> FilesystemBackend:
    if _platform_name() == "nt":
        from .windows_filesystem import WindowsFilesystemBackend

        return WindowsFilesystemBackend()

    from .posix_filesystem import PosixFilesystemBackend

    return PosixFilesystemBackend()


def _reset_default_filesystem_backend_for_tests() -> None:
    default_filesystem_backend.cache_clear()
