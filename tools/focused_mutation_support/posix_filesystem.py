from __future__ import annotations

import ctypes
import os
import stat
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Protocol

from .filesystem import (
    CreateDisposition,
    DirectoryCapability,
    DirectoryEntry,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemIdentity,
    SecurityDomain,
    SharePolicy,
    validate_component,
)


_INVALID_FD = -1
_OPEN_OR_CREATE_CYCLES = 8
_OPENAT2_RESOLVE_MASK = 0x01 | 0x02 | 0x04 | 0x08


class _DirectoryStreamResource(Protocol):
    @property
    def fd(self) -> int: ...

    def next_name(self) -> str | None: ...

    def close(self) -> None: ...


@dataclass(slots=True)
class _PosixResource:
    fd: int
    parent: DirectoryCapability | None
    name: str | None
    access: FileAccess | None
    stream: _DirectoryStreamResource | None = None


def _directory_flags() -> int:
    return (
        os.O_RDONLY
        | getattr(os, "O_DIRECTORY", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )


def _file_flags(access: FileAccess) -> int:
    if access is FileAccess.READ:
        value = os.O_RDONLY
    elif access is FileAccess.WRITE:
        value = os.O_WRONLY
    else:
        value = os.O_RDWR
    return value | getattr(os, "O_NOFOLLOW", 0)


def _filesystem_identity(fd: int) -> FilesystemIdentity:
    metadata = os.fstat(fd)
    if sys.platform != "darwin":
        return FilesystemIdentity(metadata.st_dev)
    value = ctypes.create_string_buffer(4_096)
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.fstatfs(fd, ctypes.byref(value)) != 0:
        error_number = ctypes.get_errno()
        raise OSError(error_number, os.strerror(error_number))
    first, second = struct.unpack_from("=ii", value.raw, 48)
    return FilesystemIdentity(metadata.st_dev, first, second)


def _metadata(
    fd: int,
) -> tuple[FileIdentity, FilesystemIdentity, int, int, int]:
    value = os.fstat(fd)
    identity = FileIdentity(value.st_dev, value.st_ino)
    if identity.volume == 0 or identity.file == 0:
        raise OSError("filesystem returned an unsupported zero identity")
    filesystem = _filesystem_identity(fd)
    return identity, filesystem, value.st_mode, value.st_size, value.st_mtime_ns


def _entry_kind(mode: int) -> EntryKind:
    if stat.S_ISLNK(mode):
        return EntryKind.REPARSE
    if stat.S_ISDIR(mode):
        return EntryKind.DIRECTORY
    if stat.S_ISREG(mode):
        return EntryKind.REGULAR
    return EntryKind.OTHER


def _add_close_note(primary_error: BaseException, close: Callable[[], None]) -> None:
    try:
        close()
    except OSError as close_error:
        primary_error.add_note(f"filesystem capability close failed: {close_error}")


def _effective_uid() -> int:
    getter = getattr(os, "geteuid", None)
    if getter is None:
        raise OSError("effective user identity is unavailable")
    return int(getter())


class _NativeDirectoryStream:
    def __init__(self, fd: int) -> None:
        self._libc = ctypes.CDLL(None, use_errno=True)
        self._libc.fdopendir.argtypes = (ctypes.c_int,)
        self._libc.fdopendir.restype = ctypes.c_void_p
        self._libc.readdir.argtypes = (ctypes.c_void_p,)
        self._libc.readdir.restype = ctypes.c_void_p
        self._libc.dirfd.argtypes = (ctypes.c_void_p,)
        self._libc.dirfd.restype = ctypes.c_int
        self._libc.closedir.argtypes = (ctypes.c_void_p,)
        self._libc.closedir.restype = ctypes.c_int
        pointer = self._libc.fdopendir(fd)
        if not pointer:
            error_number = ctypes.get_errno()
            raise OSError(error_number, os.strerror(error_number))
        self._pointer: int | None = int(pointer)

    @property
    def fd(self) -> int:
        if self._pointer is None:
            raise RuntimeError("directory stream is closed")
        value = self._libc.dirfd(self._pointer)
        if value < 0:
            error_number = ctypes.get_errno()
            raise OSError(error_number, os.strerror(error_number))
        return int(value)

    def next_name(self) -> str | None:
        if self._pointer is None:
            raise RuntimeError("directory stream is closed")
        while True:
            ctypes.set_errno(0)
            pointer = self._libc.readdir(self._pointer)
            if not pointer:
                error_number = ctypes.get_errno()
                if error_number:
                    raise OSError(error_number, os.strerror(error_number))
                return None
            name_offset = 21 if sys.platform == "darwin" else 19
            encoded = ctypes.string_at(pointer + name_offset)
            name = os.fsdecode(encoded)
            if name not in {".", ".."}:
                return name

    def close(self) -> None:
        if self._pointer is None:
            return
        if self._libc.closedir(self._pointer) != 0:
            error_number = ctypes.get_errno()
            raise OSError(error_number, os.strerror(error_number))
        self._pointer = None


class _PosixEntries:
    def __init__(
        self,
        backend: PosixFilesystemBackend,
        directory: DirectoryCapability,
    ) -> None:
        self._backend = backend
        self._directory = directory
        self._closed = False

    @property
    def directory(self) -> DirectoryCapability:
        return self._directory

    def __iter__(self) -> _PosixEntries:
        return self

    def __next__(self) -> DirectoryEntry:
        if self._closed:
            raise StopIteration
        try:
            while True:
                resource = self._backend._resource(self._directory)
                stream = resource.stream
                if stream is None:
                    raise RuntimeError("directory capability has no stream")
                name = stream.next_name()
                if name is None:
                    self.close()
                    raise StopIteration
                entry = self._backend.entry(self._directory, name)
                if entry is not None:
                    return entry
        except StopIteration:
            raise
        except BaseException as primary_error:
            _add_close_note(primary_error, self.close)
            raise

    def close(self) -> None:
        if self._closed:
            return
        self._directory.close()
        self._closed = True

    def __del__(self) -> None:
        try:
            self.close()
        except OSError:
            pass


class PosixFilesystemBackend:
    def __init__(self) -> None:
        self._stream_factory: Callable[[int], _DirectoryStreamResource] = (
            _NativeDirectoryStream
        )
        self._before_relative_open: Callable[
            [DirectoryCapability, str], None
        ] = lambda _parent, _name: None
        self._effective_uid: Callable[[], int] = _effective_uid
        self._chmod_resource: Callable[[_PosixResource, int], None] = (
            lambda resource, mode: os.fchmod(resource.fd, mode)
        )

    def _checked_resource(self, resource: object) -> _PosixResource:
        if not isinstance(resource, _PosixResource):
            raise RuntimeError("invalid POSIX filesystem resource")
        return resource

    def _resource(
        self, capability: FileCapability | DirectoryCapability
    ) -> _PosixResource:
        return self._checked_resource(capability._resource_for(self))

    def close_resource(self, resource: object) -> None:
        native = self._checked_resource(resource)
        if native.stream is not None:
            native.stream.close()
            native.stream = None
            return
        if native.fd != _INVALID_FD:
            os.close(native.fd)
            native.fd = _INVALID_FD

    def detach_file_resource(self, resource: object, flags: int) -> int:
        native = self._checked_resource(resource)
        if native.stream is not None or native.fd == _INVALID_FD:
            raise RuntimeError("POSIX filesystem resource is not detachable")
        if native.access is None:
            raise RuntimeError("POSIX directory resource is not detachable")
        requested = flags & getattr(os, "O_ACCMODE", 3)
        compatible = {
            FileAccess.READ: {os.O_RDONLY},
            FileAccess.WRITE: {os.O_WRONLY},
            FileAccess.READ_WRITE: {os.O_RDONLY, os.O_WRONLY, os.O_RDWR},
        }
        if requested not in compatible[native.access]:
            raise ValueError("descriptor access is incompatible with capability")
        descriptor = native.fd
        native.fd = _INVALID_FD
        return descriptor

    def _directory_fd(self, directory: DirectoryCapability) -> int:
        resource = self._resource(directory)
        if resource.stream is not None:
            return resource.stream.fd
        if resource.fd == _INVALID_FD:
            raise RuntimeError("directory resource is closed")
        return resource.fd

    def _native_open_relative(
        self, parent_fd: int, name: str, flags: int, mode: int = 0
    ) -> int:
        if not sys.platform.startswith("linux"):
            return os.open(name, flags, mode, dir_fd=parent_fd)

        class OpenHow(ctypes.Structure):
            _fields_ = [
                ("flags", ctypes.c_uint64),
                ("mode", ctypes.c_uint64),
                ("resolve", ctypes.c_uint64),
            ]

        encoded = os.fsencode(name)
        if b"/" in encoded or encoded in {b"", b".", b".."}:
            raise OSError("filesystem component is not a direct child")
        how = OpenHow(
            flags=flags | getattr(os, "O_CLOEXEC", 0),
            mode=mode,
            resolve=_OPENAT2_RESOLVE_MASK,
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

    def _new_directory_capability(
        self,
        fd: int,
        *,
        parent: DirectoryCapability | None,
        name: str | None,
        security_domain: SecurityDomain,
        share_policy: SharePolicy,
        created: bool,
        path_hint: Path,
        expected: DirectoryEntry | None = None,
    ) -> DirectoryCapability:
        try:
            identity, filesystem, mode, size, modified_ns = _metadata(fd)
            if not stat.S_ISDIR(mode) or stat.S_ISLNK(mode):
                raise OSError("filesystem capability is not a real directory")
            if expected is not None and identity != expected.identity:
                raise OSError("directory identity changed while opening")
            if expected is not None and filesystem != expected.filesystem:
                raise OSError("directory filesystem changed while opening")
            if parent is not None and filesystem != parent.filesystem:
                raise OSError("directory crosses a filesystem boundary")
            resource = _PosixResource(fd, parent, name, None)
            return DirectoryCapability(
                self,
                resource,
                identity=identity,
                filesystem=filesystem,
                kind=EntryKind.DIRECTORY,
                logical_size=size,
                modified_ns=modified_ns,
                security_domain=security_domain,
                share_policy=share_policy,
                created=created,
                path_hint=path_hint,
            )
        except BaseException as primary_error:
            _add_close_note(primary_error, lambda: os.close(fd))
            raise

    def _new_file_capability(
        self,
        fd: int,
        *,
        parent: DirectoryCapability,
        name: str,
        access: FileAccess,
        share_policy: SharePolicy,
        created: bool,
        expected: DirectoryEntry | None = None,
    ) -> FileCapability:
        try:
            identity, filesystem, mode, size, modified_ns = _metadata(fd)
            if not stat.S_ISREG(mode):
                raise OSError("filesystem capability is not a regular file")
            if expected is not None and identity != expected.identity:
                raise OSError("file identity changed while opening")
            if filesystem != parent.filesystem:
                raise OSError("file crosses a filesystem boundary")
            resource = _PosixResource(fd, parent, name, access)
            return FileCapability(
                self,
                resource,
                identity=identity,
                filesystem=filesystem,
                kind=EntryKind.REGULAR,
                logical_size=size,
                modified_ns=modified_ns,
                security_domain=parent.security_domain,
                share_policy=share_policy,
                created=created,
                path_hint=parent.path_hint / name,
            )
        except BaseException as primary_error:
            _add_close_note(primary_error, lambda: os.close(fd))
            raise

    def open_root(
        self,
        path: Path,
        share_policy: SharePolicy,
        security_domain: SecurityDomain = SecurityDomain.CALLER,
    ) -> DirectoryCapability:
        descriptor = os.open(path, _directory_flags())
        return self._new_directory_capability(
            descriptor,
            parent=None,
            name=None,
            security_domain=security_domain,
            share_policy=share_policy,
            created=False,
            path_hint=Path(path),
        )

    def _open_observed_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        observed: DirectoryEntry,
        share_policy: SharePolicy,
        *,
        security_domain: SecurityDomain,
        created: bool,
    ) -> DirectoryCapability:
        self._before_relative_open(parent, name)
        descriptor = self._native_open_relative(
            self._directory_fd(parent), name, _directory_flags()
        )
        return self._new_directory_capability(
            descriptor,
            parent=parent,
            name=name,
            security_domain=security_domain,
            share_policy=share_policy,
            created=created,
            path_hint=parent.path_hint / name,
            expected=observed,
        )

    def create_secure_root(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryCapability:
        validate_component(name)
        parent_fd = self._directory_fd(parent)
        for _cycle in range(_OPEN_OR_CREATE_CYCLES):
            try:
                os.mkdir(name, mode=0o700, dir_fd=parent_fd)
                created = True
            except FileExistsError:
                created = False
            observed = self.entry(parent, name)
            if observed is None:
                continue
            try:
                capability = self._open_observed_directory(
                    parent,
                    name,
                    observed,
                    SharePolicy.MUTATION,
                    security_domain=SecurityDomain.MANAGED,
                    created=created,
                )
            except FileNotFoundError:
                continue
            try:
                self.verify_managed_security(capability, repair_dacl=True)
            except BaseException as primary_error:
                _add_close_note(primary_error, capability.close)
                raise
            return capability
        raise OSError("secure root entry did not stabilize")

    def reopen_directory(
        self,
        directory: DirectoryCapability,
        share_policy: SharePolicy | None = None,
    ) -> DirectoryCapability:
        descriptor = os.open(
            ".", _directory_flags(), dir_fd=self._directory_fd(directory)
        )
        expected = DirectoryEntry(
            name=".",
            kind=EntryKind.DIRECTORY,
            identity=directory.identity,
            filesystem=directory.filesystem,
            logical_size=directory.logical_size,
            modified_ns=directory.modified_ns,
        )
        return self._new_directory_capability(
            descriptor,
            parent=None,
            name=None,
            security_domain=directory.security_domain,
            share_policy=(
                directory.share_policy if share_policy is None else share_policy
            ),
            created=False,
            path_hint=directory.path_hint,
            expected=expected,
        )

    def open_directory(
        self, parent: DirectoryCapability, name: str, share_policy: SharePolicy
    ) -> DirectoryCapability:
        validate_component(name)
        observed = self.entry(parent, name)
        if observed is None:
            raise FileNotFoundError(name)
        if observed.kind is not EntryKind.DIRECTORY:
            raise OSError("filesystem entry is not a real directory")
        return self._open_observed_directory(
            parent,
            name,
            observed,
            share_policy,
            security_domain=parent.security_domain,
            created=False,
        )

    def create_directory(
        self, parent: DirectoryCapability, name: str, share_policy: SharePolicy
    ) -> DirectoryCapability:
        validate_component(name)
        os.mkdir(name, mode=0o700, dir_fd=self._directory_fd(parent))
        observed = self.entry(parent, name)
        if observed is None:
            raise OSError("created directory did not resolve through its parent")
        return self._open_observed_directory(
            parent,
            name,
            observed,
            share_policy,
            security_domain=parent.security_domain,
            created=True,
        )

    def _open_observed_file(
        self,
        parent: DirectoryCapability,
        name: str,
        observed: DirectoryEntry,
        *,
        access: FileAccess,
        share_policy: SharePolicy,
    ) -> FileCapability:
        self._before_relative_open(parent, name)
        descriptor = self._native_open_relative(
            self._directory_fd(parent), name, _file_flags(access)
        )
        return self._new_file_capability(
            descriptor,
            parent=parent,
            name=name,
            access=access,
            share_policy=share_policy,
            created=False,
            expected=observed,
        )

    def open_file(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        access: FileAccess,
        disposition: CreateDisposition,
        share_policy: SharePolicy = SharePolicy.MUTATION,
    ) -> FileCapability:
        validate_component(name)
        flags = _file_flags(access)
        parent_fd = self._directory_fd(parent)
        if disposition is CreateDisposition.OPEN_EXISTING:
            observed = self.entry(parent, name)
            if observed is None:
                raise FileNotFoundError(name)
            return self._open_observed_file(
                parent,
                name,
                observed,
                access=access,
                share_policy=share_policy,
            )
        if disposition is CreateDisposition.CREATE_NEW:
            descriptor = self._native_open_relative(
                parent_fd, name, flags | os.O_CREAT | os.O_EXCL, 0o600
            )
            return self._new_file_capability(
                descriptor,
                parent=parent,
                name=name,
                access=access,
                share_policy=share_policy,
                created=True,
            )

        for _cycle in range(_OPEN_OR_CREATE_CYCLES):
            try:
                descriptor = self._native_open_relative(
                    parent_fd, name, flags | os.O_CREAT | os.O_EXCL, 0o600
                )
            except FileExistsError:
                observed = self.entry(parent, name)
                if observed is None:
                    continue
                try:
                    return self._open_observed_file(
                        parent,
                        name,
                        observed,
                        access=access,
                        share_policy=share_policy,
                    )
                except FileNotFoundError:
                    continue
            else:
                return self._new_file_capability(
                    descriptor,
                    parent=parent,
                    name=name,
                    access=access,
                    share_policy=share_policy,
                    created=True,
                )
        raise OSError("open-or-create entry did not stabilize")

    def open_entry(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> FileCapability | DirectoryCapability:
        validate_component(name)
        observed = self.entry(parent, name)
        if observed is None:
            raise FileNotFoundError(name)
        if observed.kind is EntryKind.DIRECTORY:
            return self._open_observed_directory(
                parent,
                name,
                observed,
                share_policy,
                security_domain=parent.security_domain,
                created=False,
            )
        if observed.kind is EntryKind.REGULAR:
            return self._open_observed_file(
                parent,
                name,
                observed,
                access=FileAccess.READ,
                share_policy=share_policy,
            )
        if observed.filesystem != parent.filesystem:
            raise OSError("entry crosses a filesystem boundary")
        descriptor = os.open(
            ".", _directory_flags(), dir_fd=self._directory_fd(parent)
        )
        try:
            (
                reopened_identity,
                reopened_filesystem,
                reopened_mode,
                _reopened_size,
                _reopened_modified_ns,
            ) = _metadata(descriptor)
            if (
                reopened_identity != parent.identity
                or reopened_filesystem != parent.filesystem
                or not stat.S_ISDIR(reopened_mode)
                or stat.S_ISLNK(reopened_mode)
            ):
                raise OSError("parent identity changed while reopening entry")
            current = self.entry(parent, name)
            if current is None:
                raise FileNotFoundError(name)
            if (
                current.identity != observed.identity
                or current.kind is not observed.kind
            ):
                raise OSError("entry identity changed while opening")
            resource = _PosixResource(descriptor, parent, name, None)
            return FileCapability(
                self,
                resource,
                identity=observed.identity,
                filesystem=observed.filesystem,
                kind=observed.kind,
                logical_size=observed.logical_size,
                modified_ns=observed.modified_ns,
                security_domain=parent.security_domain,
                share_policy=share_policy,
                created=False,
                path_hint=parent.path_hint / name,
            )
        except BaseException as primary_error:
            _add_close_note(primary_error, lambda: os.close(descriptor))
            raise

    def entry(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryEntry | None:
        validate_component(name)
        try:
            metadata = os.stat(
                name,
                dir_fd=self._directory_fd(parent),
                follow_symlinks=False,
            )
        except FileNotFoundError:
            return None
        filesystem = (
            parent.filesystem
            if metadata.st_dev == parent.identity.volume
            else FilesystemIdentity(metadata.st_dev)
        )
        if metadata.st_size < 0:
            raise OSError("filesystem returned a negative logical size")
        return DirectoryEntry(
            name=name,
            kind=_entry_kind(metadata.st_mode),
            identity=FileIdentity(metadata.st_dev, metadata.st_ino),
            filesystem=filesystem,
            logical_size=metadata.st_size,
            modified_ns=metadata.st_mtime_ns,
        )

    def entries(self, parent: DirectoryCapability) -> _PosixEntries:
        reopened = self.reopen_directory(parent)
        try:
            return self.entries_owned(reopened)
        except BaseException as primary_error:
            _add_close_note(primary_error, reopened.close)
            raise

    def entries_owned(self, parent: DirectoryCapability) -> _PosixEntries:
        moved = parent._move_for(self)
        try:
            resource = self._resource(moved)
            stream = self._stream_factory(resource.fd)
            resource.stream = stream
            resource.fd = _INVALID_FD
            return _PosixEntries(self, moved)
        except BaseException as primary_error:
            _add_close_note(primary_error, moved.close)
            raise

    def _require_pinned_relative(
        self, capability: FileCapability | DirectoryCapability
    ) -> _PosixResource:
        if capability.share_policy is not SharePolicy.PINNED:
            raise ValueError("namespace mutation requires a pinned source capability")
        resource = self._resource(capability)
        if resource.parent is None or resource.name is None:
            raise OSError("root capability is not a relative mutation source")
        return resource

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        validate_component(destination_name)
        resource = self._require_pinned_relative(source)
        source_parent = resource.parent
        source_name = resource.name
        current = self.entry(source_parent, source_name)
        if current is None:
            raise FileNotFoundError(source_name)
        if current.identity != source.identity or current.kind is not source.kind:
            raise OSError("rename source identity changed")
        if source.filesystem != destination_parent.filesystem:
            raise OSError("rename crosses a filesystem boundary")
        operation = os.replace if replace else os.rename
        operation(
            source_name,
            destination_name,
            src_dir_fd=self._directory_fd(source_parent),
            dst_dir_fd=self._directory_fd(destination_parent),
        )
        resource.parent = destination_parent
        resource.name = destination_name
        source._path_hint = destination_parent.path_hint / destination_name

    def delete(self, capability: FileCapability | DirectoryCapability) -> None:
        resource = self._require_pinned_relative(capability)
        parent = resource.parent
        name = resource.name
        current = self.entry(parent, name)
        if current is None:
            raise FileNotFoundError(name)
        if (
            current.identity != capability.identity
            or current.kind is not capability.kind
        ):
            raise OSError("delete target identity changed")
        if capability.kind is EntryKind.DIRECTORY:
            os.rmdir(name, dir_fd=self._directory_fd(parent))
        else:
            os.unlink(name, dir_fd=self._directory_fd(parent))
        capability.close()
        if self.entry(parent, name) is not None:
            raise OSError("deleted entry still resolves through its parent")

    def available_bytes(self, directory: DirectoryCapability) -> int:
        value = os.fstatvfs(self._directory_fd(directory))
        if value.f_frsize <= 0 or value.f_bavail < 0:
            raise OSError("filesystem returned invalid capacity metadata")
        return value.f_frsize * value.f_bavail

    def allocation_unit(self, directory: DirectoryCapability) -> int:
        value = os.fstatvfs(self._directory_fd(directory))
        if value.f_frsize <= 0:
            raise OSError("filesystem returned an invalid allocation unit")
        return value.f_frsize

    def touch(self, file: FileCapability) -> None:
        resource = self._resource(file)
        if resource.fd == _INVALID_FD or resource.stream is not None:
            raise RuntimeError("file capability has no live descriptor")
        os.utime(resource.fd, None)

    def flush(self, file: FileCapability) -> None:
        resource = self._resource(file)
        if resource.fd == _INVALID_FD or resource.stream is not None:
            raise RuntimeError("file capability has no live descriptor")
        os.fsync(resource.fd)

    def final_path(self, directory: DirectoryCapability) -> Path:
        descriptor = self._directory_fd(directory)
        if sys.platform == "darwin":
            import fcntl

            command = getattr(fcntl, "F_GETPATH", None)
            if command is None:
                raise OSError("F_GETPATH is unavailable")
            raw = fcntl.fcntl(descriptor, command, b"\0" * 1_024)
            if not isinstance(raw, bytes):
                raise OSError("F_GETPATH returned a non-byte path")
            value = raw.split(b"\0", 1)[0].decode("utf-8", errors="strict")
        elif sys.platform.startswith("linux"):
            value = os.readlink(f"/proc/self/fd/{descriptor}")
        else:
            raise OSError("directory capability path recovery is unavailable")
        path = Path(value)
        if not path.is_absolute():
            raise OSError("directory capability path is not absolute")
        return path

    def verify_managed_security(
        self,
        capability: FileCapability | DirectoryCapability,
        *,
        repair_dacl: bool,
    ) -> None:
        if capability.security_domain is SecurityDomain.CALLER:
            return
        if capability.kind not in {EntryKind.DIRECTORY, EntryKind.REGULAR}:
            raise OSError("managed security does not follow non-regular entries")
        resource = self._resource(capability)
        if resource.stream is not None or resource.fd == _INVALID_FD:
            raise RuntimeError("managed capability has no direct descriptor")
        metadata = os.fstat(resource.fd)
        expected_uid = self._effective_uid()
        if metadata.st_uid != expected_uid:
            raise PermissionError("managed filesystem object has the wrong owner")
        expected_mode = 0o700 if capability.kind is EntryKind.DIRECTORY else 0o600
        if stat.S_IMODE(metadata.st_mode) != expected_mode:
            if not repair_dacl:
                raise PermissionError("managed filesystem object has the wrong mode")
            self._chmod_resource(resource, expected_mode)
        verified = os.fstat(resource.fd)
        if verified.st_uid != expected_uid:
            raise PermissionError("managed filesystem object owner changed")
        if stat.S_IMODE(verified.st_mode) != expected_mode:
            raise PermissionError(
                f"managed filesystem object is not mode {expected_mode:04o}"
            )
