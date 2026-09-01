from __future__ import annotations

import ctypes
import errno
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from .filesystem import (
    CreateDisposition,
    DirectoryCapability,
    DirectoryEntry,
    DirectoryIterator,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemIdentity,
    SecurityDomain,
    SharePolicy,
    validate_component,
)


HANDLE = ctypes.c_void_p
NTSTATUS = ctypes.c_int32
ULONG_PTR = ctypes.c_size_t
USHORT = ctypes.c_uint16
ULONG = ctypes.c_uint32
WCHAR = ctypes.c_uint16
BOOL = ctypes.c_int32
LONGLONG = ctypes.c_int64

DELETE = 0x0001_0000
READ_CONTROL = 0x0002_0000
SYNCHRONIZE = 0x0010_0000
FILE_READ_DATA = 0x0001
FILE_LIST_DIRECTORY = FILE_READ_DATA
FILE_WRITE_DATA = 0x0002
FILE_ADD_FILE = FILE_WRITE_DATA
FILE_ADD_SUBDIRECTORY = 0x0004
FILE_TRAVERSE = 0x0020
FILE_DELETE_CHILD = 0x0040
FILE_READ_ATTRIBUTES = 0x0080
FILE_WRITE_ATTRIBUTES = 0x0100
FILE_SHARE_READ = 0x0001
FILE_SHARE_WRITE = 0x0002
FILE_SHARE_DELETE = 0x0004

FILE_ATTRIBUTE_DIRECTORY = 0x0010
FILE_ATTRIBUTE_DEVICE = 0x0040
FILE_ATTRIBUTE_NORMAL = 0x0080
FILE_ATTRIBUTE_REPARSE_POINT = 0x0400
FILE_FLAG_BACKUP_SEMANTICS = 0x0200_0000
FILE_FLAG_OPEN_REPARSE_POINT = 0x0020_0000
OPEN_EXISTING = 3

OBJ_CASE_INSENSITIVE = 0x0040
FILE_DIRECTORY_FILE = 0x0001
FILE_SYNCHRONOUS_IO_NONALERT = 0x0020
FILE_NON_DIRECTORY_FILE = 0x0040
FILE_OPEN_REPARSE_POINT = 0x0020_0000
FILE_OPEN = 1
FILE_CREATE = 2
FILE_OPEN_IF = 3
FILE_OPENED = 1
FILE_CREATED = 2
FILE_ID_INFO_CLASS = 18
DUPLICATE_SAME_ACCESS = 0x0002

_WINDOWS_EPOCH_100NS = 116_444_736_000_000_000

ERROR_FILE_NOT_FOUND = 2
ERROR_PATH_NOT_FOUND = 3
ERROR_ACCESS_DENIED = 5
INVALID_OWNED_HANDLE = 0
_INVALID_WIN32_HANDLE = ctypes.c_void_p(-1).value
_ERROR_EVIDENCE_BYTES = 4_096

_KERNEL32: Any | None
_ADVAPI32: Any | None
_NTDLL: Any | None
if os.name == "nt":
    _KERNEL32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _ADVAPI32 = ctypes.WinDLL("advapi32", use_last_error=True)
    _NTDLL = ctypes.WinDLL("ntdll")
else:
    _KERNEL32 = None
    _ADVAPI32 = None
    _NTDLL = None


class UNICODE_STRING(ctypes.Structure):
    _fields_ = [
        ("Length", USHORT),
        ("MaximumLength", USHORT),
        ("Buffer", ctypes.POINTER(WCHAR)),
    ]


class OBJECT_ATTRIBUTES(ctypes.Structure):
    _fields_ = [
        ("Length", ULONG),
        ("RootDirectory", HANDLE),
        ("ObjectName", ctypes.POINTER(UNICODE_STRING)),
        ("Attributes", ULONG),
        ("SecurityDescriptor", ctypes.c_void_p),
        ("SecurityQualityOfService", ctypes.c_void_p),
    ]


class IO_STATUS_BLOCK(ctypes.Structure):
    _fields_ = [("Status", NTSTATUS), ("Information", ULONG_PTR)]


class FILETIME(ctypes.Structure):
    _fields_ = [("dwLowDateTime", ULONG), ("dwHighDateTime", ULONG)]


class BY_HANDLE_FILE_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("dwFileAttributes", ULONG),
        ("ftCreationTime", FILETIME),
        ("ftLastAccessTime", FILETIME),
        ("ftLastWriteTime", FILETIME),
        ("dwVolumeSerialNumber", ULONG),
        ("nFileSizeHigh", ULONG),
        ("nFileSizeLow", ULONG),
        ("nNumberOfLinks", ULONG),
        ("nFileIndexHigh", ULONG),
        ("nFileIndexLow", ULONG),
    ]


class FILE_ID_128(ctypes.Structure):
    _fields_ = [("Identifier", ctypes.c_uint8 * 16)]


class FILE_ID_INFO(ctypes.Structure):
    _fields_ = [("VolumeSerialNumber", ctypes.c_uint64), ("FileId", FILE_ID_128)]


class FILE_BASIC_INFO(ctypes.Structure):
    _fields_ = [
        ("CreationTime", LONGLONG),
        ("LastAccessTime", LONGLONG),
        ("LastWriteTime", LONGLONG),
        ("ChangeTime", LONGLONG),
        ("FileAttributes", ULONG),
    ]


class FILE_ID_EXTD_DIR_INFO(ctypes.Structure):
    _fields_ = [
        ("NextEntryOffset", ULONG),
        ("FileIndex", ULONG),
        ("CreationTime", LONGLONG),
        ("LastAccessTime", LONGLONG),
        ("LastWriteTime", LONGLONG),
        ("ChangeTime", LONGLONG),
        ("EndOfFile", LONGLONG),
        ("AllocationSize", LONGLONG),
        ("FileAttributes", ULONG),
        ("FileNameLength", ULONG),
        ("EaSize", ULONG),
        ("ReparsePointTag", ULONG),
        ("FileId", FILE_ID_128),
        ("FileName", WCHAR * 1),
    ]


class FILE_RENAME_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("ReplaceIfExists", ctypes.c_uint8),
        ("RootDirectory", HANDLE),
        ("FileNameLength", ULONG),
        ("FileName", WCHAR * 1),
    ]


@dataclass(frozen=True, slots=True)
class _Metadata:
    identity: FileIdentity
    filesystem: FilesystemIdentity
    kind: EntryKind
    logical_size: int
    modified_ns: int


def _bounded_evidence(value: str) -> str:
    return (
        value.encode("utf-8", errors="backslashreplace")[:_ERROR_EVIDENCE_BYTES]
        .decode("utf-8", errors="ignore")
    )


_WIN32_ERRNO_FALLBACKS = {
    2: errno.ENOENT,
    3: errno.ENOENT,
    5: errno.EACCES,
    32: errno.EACCES,
    80: errno.EEXIST,
    123: errno.EINVAL,
}


def _errno_from_win32(code: int) -> int:
    if os.name == "nt":
        mapped = OSError(0, "", None, code).errno
        if mapped is not None:
            return int(mapped)
    return _WIN32_ERRNO_FALLBACKS.get(code, errno.EIO)


def _error_from_win32(
    code: int,
    operation: str,
    component: object,
    *,
    leaf: bool = False,
) -> OSError:
    bounded_operation = _bounded_evidence(operation)
    bounded_component = _bounded_evidence(str(component))
    error_number = _errno_from_win32(code)
    message = f"{bounded_operation} failed with Windows error {code}"
    if code == ERROR_FILE_NOT_FOUND and leaf:
        error: OSError = FileNotFoundError(
            errno.ENOENT, message, bounded_component
        )
    elif code == ERROR_ACCESS_DENIED:
        error = PermissionError(errno.EACCES, message, bounded_component)
    else:
        error = OSError()
        error.args = (error_number, message)
        error.errno = error_number
        error.strerror = message
        error.filename = bounded_component
    error.winerror = code  # type: ignore[attr-defined]
    return error


_DOS_DEVICE_BASENAMES = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "CONIN$",
    "CONOUT$",
    *(f"COM{digit}" for digit in "123456789¹²³"),
    *(f"LPT{digit}" for digit in "123456789¹²³"),
}
_FORBIDDEN_COMPONENT_CHARACTERS = frozenset('<>:"/\\|?*')


def _encode_windows_component(name: str) -> bytes:
    validate_component(name)
    if any(
        ord(character) < 0x20 or character in _FORBIDDEN_COMPONENT_CHARACTERS
        for character in name
    ):
        raise ValueError(f"invalid Windows filesystem component: {name!r}")
    if name.endswith((".", " ")):
        raise ValueError(f"invalid Windows filesystem component: {name!r}")
    if name.split(".", 1)[0].upper() in _DOS_DEVICE_BASENAMES:
        raise ValueError(f"invalid Windows filesystem component: {name!r}")
    try:
        encoded = name.encode("utf-16-le", errors="strict")
    except UnicodeEncodeError as error:
        raise ValueError(f"invalid Windows filesystem component: {name!r}") from error
    if len(encoded) > 0xFFFF:
        raise ValueError("Windows filesystem component is too long")
    return encoded


def _share_mode(policy: SharePolicy) -> int:
    common = FILE_SHARE_READ | FILE_SHARE_WRITE
    return common if policy is SharePolicy.PINNED else common | FILE_SHARE_DELETE


def _directory_access(policy: SharePolicy, *, relative_target: bool) -> int:
    scan = (
        SYNCHRONIZE
        | READ_CONTROL
        | FILE_READ_ATTRIBUTES
        | FILE_LIST_DIRECTORY
        | FILE_TRAVERSE
    )
    if policy is SharePolicy.SCAN:
        return scan
    child_mutation = FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY | FILE_DELETE_CHILD
    if policy is SharePolicy.PINNED:
        self_delete = DELETE if relative_target else 0
        return scan | child_mutation | self_delete
    return scan | child_mutation | DELETE | FILE_WRITE_ATTRIBUTES


def _file_access(access: FileAccess, policy: SharePolicy) -> int:
    value = SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES
    if access in {FileAccess.READ, FileAccess.READ_WRITE}:
        value |= FILE_READ_DATA
    if access in {FileAccess.WRITE, FileAccess.READ_WRITE}:
        value |= FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES
    if policy is SharePolicy.MUTATION:
        value |= DELETE
    return value


def _entry_access(policy: SharePolicy) -> int:
    value = SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES
    if policy in {SharePolicy.PINNED, SharePolicy.MUTATION}:
        value |= DELETE
    return value


def _entry_kind_from_attributes(attributes: int) -> EntryKind:
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT:
        return EntryKind.REPARSE
    if attributes & FILE_ATTRIBUTE_DIRECTORY:
        return EntryKind.DIRECTORY
    if attributes & FILE_ATTRIBUTE_DEVICE:
        return EntryKind.OTHER
    return EntryKind.REGULAR


def _filetime_to_unix_ns(value: int) -> int:
    return (value - _WINDOWS_EPOCH_100NS) * 100


class _WindowsApi:
    def __init__(self) -> None:
        if _KERNEL32 is None or _ADVAPI32 is None or _NTDLL is None:
            raise OSError("Windows native filesystem APIs are unavailable")
        kernel32 = _KERNEL32
        ntdll = _NTDLL
        self.CreateFileW = kernel32.CreateFileW
        self.CreateFileW.argtypes = (
            ctypes.c_wchar_p,
            ULONG,
            ULONG,
            ctypes.c_void_p,
            ULONG,
            ULONG,
            HANDLE,
        )
        self.CreateFileW.restype = HANDLE
        self.CloseHandle = kernel32.CloseHandle
        self.CloseHandle.argtypes = (HANDLE,)
        self.CloseHandle.restype = BOOL
        self.GetFileInformationByHandle = kernel32.GetFileInformationByHandle
        self.GetFileInformationByHandle.argtypes = (
            HANDLE,
            ctypes.POINTER(BY_HANDLE_FILE_INFORMATION),
        )
        self.GetFileInformationByHandle.restype = BOOL
        self.GetFileInformationByHandleEx = kernel32.GetFileInformationByHandleEx
        self.GetFileInformationByHandleEx.argtypes = (
            HANDLE,
            ctypes.c_int32,
            ctypes.c_void_p,
            ULONG,
        )
        self.GetFileInformationByHandleEx.restype = BOOL
        self.GetVolumeInformationByHandleW = kernel32.GetVolumeInformationByHandleW
        self.GetVolumeInformationByHandleW.argtypes = (
            HANDLE,
            ctypes.POINTER(WCHAR),
            ULONG,
            ctypes.POINTER(ULONG),
            ctypes.POINTER(ULONG),
            ctypes.POINTER(ULONG),
            ctypes.POINTER(WCHAR),
            ULONG,
        )
        self.GetVolumeInformationByHandleW.restype = BOOL
        self.GetSystemTimeAsFileTime = kernel32.GetSystemTimeAsFileTime
        self.GetSystemTimeAsFileTime.argtypes = (ctypes.POINTER(FILETIME),)
        self.GetSystemTimeAsFileTime.restype = None
        self.SetFileTime = kernel32.SetFileTime
        self.SetFileTime.argtypes = (
            HANDLE,
            ctypes.POINTER(FILETIME),
            ctypes.POINTER(FILETIME),
            ctypes.POINTER(FILETIME),
        )
        self.SetFileTime.restype = BOOL
        self.FlushFileBuffers = kernel32.FlushFileBuffers
        self.FlushFileBuffers.argtypes = (HANDLE,)
        self.FlushFileBuffers.restype = BOOL
        self.GetCurrentProcess = kernel32.GetCurrentProcess
        self.GetCurrentProcess.argtypes = ()
        self.GetCurrentProcess.restype = HANDLE
        self.DuplicateHandle = kernel32.DuplicateHandle
        self.DuplicateHandle.argtypes = (
            HANDLE,
            HANDLE,
            HANDLE,
            ctypes.POINTER(HANDLE),
            ULONG,
            BOOL,
            ULONG,
        )
        self.DuplicateHandle.restype = BOOL
        self.NtCreateFile = ntdll.NtCreateFile
        self.NtCreateFile.argtypes = (
            ctypes.POINTER(HANDLE),
            ULONG,
            ctypes.POINTER(OBJECT_ATTRIBUTES),
            ctypes.POINTER(IO_STATUS_BLOCK),
            ctypes.POINTER(LONGLONG),
            ULONG,
            ULONG,
            ULONG,
            ULONG,
            ctypes.c_void_p,
            ULONG,
        )
        self.NtCreateFile.restype = NTSTATUS
        self.RtlNtStatusToDosError = ntdll.RtlNtStatusToDosError
        self.RtlNtStatusToDosError.argtypes = (NTSTATUS,)
        self.RtlNtStatusToDosError.restype = ULONG

    def last_error(self) -> int:
        return ctypes.get_last_error()

    def before_relative_open(self) -> None:
        return None

    def after_relative_open(self) -> None:
        return None


@dataclass(slots=True)
class _WindowsResource:
    handle: int
    parent: DirectoryCapability | None
    name: str | None
    delete_authority: bool


class WindowsFilesystemBackend:
    def __init__(
        self,
        *,
        api: Any | None = None,
        osfhandle_opener: Callable[[int, int], int] | None = None,
    ) -> None:
        self._failed_closes: list[_WindowsResource] = []
        self._api = api if api is not None else _WindowsApi()
        if osfhandle_opener is None:
            if os.name != "nt":
                raise OSError("CRT handle transfer is unavailable")
            import msvcrt

            osfhandle_opener = msvcrt.open_osfhandle
        self._open_osfhandle = osfhandle_opener

    def _checked_resource(self, resource: object) -> _WindowsResource:
        if not isinstance(resource, _WindowsResource):
            raise RuntimeError("invalid Windows filesystem resource")
        if resource.handle == INVALID_OWNED_HANDLE:
            raise RuntimeError("Windows filesystem resource is not owned")
        return resource

    def close_resource(self, resource: object) -> None:
        native = self._checked_resource(resource)
        if not self._api.CloseHandle(native.handle):
            raise _error_from_win32(
                self._api.last_error(),
                "close filesystem capability",
                native.name,
            )
        native.handle = INVALID_OWNED_HANDLE

    def detach_file_resource(self, resource: object, flags: int) -> int:
        native = self._checked_resource(resource)
        descriptor = self._open_osfhandle(
            native.handle,
            flags | getattr(os, "O_NOINHERIT", 0),
        )
        native.handle = INVALID_OWNED_HANDLE
        return descriptor

    def _resource(
        self, capability: FileCapability | DirectoryCapability
    ) -> _WindowsResource:
        return self._checked_resource(capability._resource_for(self))

    def _raise_last_error(self, operation: str, component: object) -> None:
        raise _error_from_win32(self._api.last_error(), operation, component)

    def _raise_ntstatus(
        self, status: int, operation: str, component: object
    ) -> None:
        code = int(self._api.RtlNtStatusToDosError(status))
        raise _error_from_win32(code, operation, component, leaf=True)

    def _close_after_error(
        self,
        primary_error: BaseException,
        handle: int,
        parent: DirectoryCapability | None,
        name: str | None,
        delete_authority: bool,
    ) -> None:
        resource = _WindowsResource(handle, parent, name, delete_authority)
        try:
            self.close_resource(resource)
        except BaseException as close_error:
            primary_error.add_note(
                _bounded_evidence(
                    f"filesystem capability close failed: {close_error}"
                )
            )
            self._failed_closes.append(resource)

    def _metadata(self, handle: int, component: object) -> _Metadata:
        file_id = FILE_ID_INFO()
        if not self._api.GetFileInformationByHandleEx(
            handle,
            FILE_ID_INFO_CLASS,
            ctypes.byref(file_id),
            ctypes.sizeof(file_id),
        ):
            self._raise_last_error("read 128-bit file identity", component)
        volume = int(file_id.VolumeSerialNumber)
        identity_value = int.from_bytes(bytes(file_id.FileId.Identifier), "little")
        if volume == 0 or identity_value == 0:
            raise OSError("filesystem returned an unsupported zero identity")

        information = BY_HANDLE_FILE_INFORMATION()
        if not self._api.GetFileInformationByHandle(
            handle, ctypes.byref(information)
        ):
            self._raise_last_error("read file attributes", component)
        attributes = int(information.dwFileAttributes)
        kind = _entry_kind_from_attributes(attributes)
        logical_size = (int(information.nFileSizeHigh) << 32) | int(
            information.nFileSizeLow
        )
        modified_100ns = (
            int(information.ftLastWriteTime.dwHighDateTime) << 32
        ) | int(information.ftLastWriteTime.dwLowDateTime)
        modified_ns = _filetime_to_unix_ns(modified_100ns)

        volume_serial = ULONG()
        maximum_component_length = ULONG()
        filesystem_flags = ULONG()
        if not self._api.GetVolumeInformationByHandleW(
            handle,
            None,
            0,
            ctypes.byref(volume_serial),
            ctypes.byref(maximum_component_length),
            ctypes.byref(filesystem_flags),
            None,
            0,
        ):
            self._raise_last_error("read filesystem identity", component)
        if int(volume_serial.value) != volume & 0xFFFF_FFFF:
            raise OSError("filesystem volume identity is inconsistent")
        return _Metadata(
            FileIdentity(volume, identity_value),
            FilesystemIdentity(
                volume,
                int(maximum_component_length.value),
                int(filesystem_flags.value),
            ),
            kind,
            logical_size,
            modified_ns,
        )

    def _native_relative_open(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        desired_access: int,
        share_mode: int,
        disposition: int,
        create_options: int,
        file_attributes: int,
        run_hooks: bool,
    ) -> tuple[int, int]:
        encoded = _encode_windows_component(name)
        parent_resource = self._resource(parent)
        buffer_type = WCHAR * (len(encoded) // 2)
        buffer = buffer_type.from_buffer_copy(encoded)
        unicode_name = UNICODE_STRING(
            len(encoded),
            len(encoded),
            ctypes.cast(buffer, ctypes.POINTER(WCHAR)),
        )
        attributes = OBJECT_ATTRIBUTES(
            ctypes.sizeof(OBJECT_ATTRIBUTES),
            parent_resource.handle,
            ctypes.pointer(unicode_name),
            OBJ_CASE_INSENSITIVE,
            None,
            None,
        )
        result_handle = HANDLE()
        io_status = IO_STATUS_BLOCK()
        if run_hooks:
            self._api.before_relative_open()
        status = int(
            self._api.NtCreateFile(
                ctypes.byref(result_handle),
                desired_access,
                ctypes.byref(attributes),
                ctypes.byref(io_status),
                None,
                file_attributes,
                share_mode,
                disposition,
                create_options | FILE_SYNCHRONOUS_IO_NONALERT,
                None,
                0,
            )
        )
        if status < 0:
            self._raise_ntstatus(status, "open relative filesystem entry", name)
        handle = int(result_handle.value or 0)
        if handle == INVALID_OWNED_HANDLE:
            raise OSError("native relative open returned an invalid handle")
        if run_hooks:
            try:
                self._api.after_relative_open()
            except BaseException as primary_error:
                self._close_after_error(
                    primary_error,
                    handle,
                    parent,
                    name,
                    bool(desired_access & DELETE),
                )
                raise
        return handle, int(io_status.Information)

    @staticmethod
    def _same_evidence(metadata: _Metadata, expected: DirectoryEntry) -> bool:
        return (
            metadata.identity == expected.identity
            and metadata.filesystem == expected.filesystem
            and metadata.kind is expected.kind
        )

    def _validate_metadata(
        self,
        metadata: _Metadata,
        *,
        parent: DirectoryCapability | None,
        expected: DirectoryEntry | None,
        expected_kind: EntryKind | None,
        operation: str,
    ) -> None:
        if expected_kind is not None and metadata.kind is not expected_kind:
            raise OSError(f"{operation} has the wrong entry kind")
        if expected is not None and not self._same_evidence(metadata, expected):
            raise OSError(f"{operation} identity changed while opening")
        if parent is not None and metadata.filesystem != parent.filesystem:
            raise OSError(f"{operation} crosses a filesystem boundary")

    @staticmethod
    def _directory_entry(name: str, metadata: _Metadata) -> DirectoryEntry:
        return DirectoryEntry(
            name,
            metadata.kind,
            metadata.identity,
            metadata.filesystem,
            metadata.logical_size,
            metadata.modified_ns,
        )

    def _directory_capability(
        self,
        handle: int,
        metadata: _Metadata,
        *,
        parent: DirectoryCapability | None,
        name: str | None,
        share_policy: SharePolicy,
        security_domain: SecurityDomain,
        created: bool,
        path_hint: Path,
        desired_access: int,
    ) -> DirectoryCapability:
        return DirectoryCapability(
            self,
            _WindowsResource(
                handle, parent, name, bool(desired_access & DELETE)
            ),
            identity=metadata.identity,
            filesystem=metadata.filesystem,
            kind=EntryKind.DIRECTORY,
            logical_size=metadata.logical_size,
            modified_ns=metadata.modified_ns,
            security_domain=security_domain,
            share_policy=share_policy,
            created=created,
            path_hint=path_hint,
        )

    def _entry_capability(
        self,
        handle: int,
        metadata: _Metadata,
        *,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
        created: bool,
        desired_access: int,
    ) -> FileCapability | DirectoryCapability:
        resource = _WindowsResource(
            handle, parent, name, bool(desired_access & DELETE)
        )
        if metadata.kind is EntryKind.DIRECTORY:
            return DirectoryCapability(
                self,
                resource,
                identity=metadata.identity,
                filesystem=metadata.filesystem,
                kind=metadata.kind,
                logical_size=metadata.logical_size,
                modified_ns=metadata.modified_ns,
                security_domain=parent.security_domain,
                share_policy=share_policy,
                created=created,
                path_hint=parent.path_hint / name,
            )
        return FileCapability(
            self,
            resource,
            identity=metadata.identity,
            filesystem=metadata.filesystem,
            kind=metadata.kind,
            logical_size=metadata.logical_size,
            modified_ns=metadata.modified_ns,
            security_domain=parent.security_domain,
            share_policy=share_policy,
            created=created,
            path_hint=parent.path_hint / name,
        )

    def _observe_required(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryEntry:
        observed = self.entry(parent, name)
        if observed is None:
            raise _error_from_win32(
                ERROR_FILE_NOT_FOUND,
                "observe relative filesystem entry",
                name,
                leaf=True,
            )
        return observed

    def _finish_relative_open(
        self,
        *,
        handle: int,
        information: int,
        parent: DirectoryCapability,
        name: str,
        desired_access: int,
        share_policy: SharePolicy,
        expected: DirectoryEntry | None,
        expected_kind: EntryKind | None,
        allowed_information: set[int],
    ) -> tuple[_Metadata, bool]:
        try:
            if information not in allowed_information:
                raise OSError(
                    f"native open returned unsupported create result {information}"
                )
            metadata = self._metadata(handle, name)
            self._validate_metadata(
                metadata,
                parent=parent,
                expected=expected,
                expected_kind=expected_kind,
                operation="relative filesystem entry",
            )
            post_open = self._observe_required(parent, name)
            if not self._same_evidence(metadata, post_open):
                raise OSError(
                    "relative filesystem entry identity changed after opening"
                )
            return metadata, information == FILE_CREATED
        except BaseException as primary_error:
            self._close_after_error(
                primary_error,
                handle,
                parent,
                name,
                bool(desired_access & DELETE),
            )
            raise

    def open_root(
        self,
        path: Path,
        share_policy: SharePolicy,
        security_domain: SecurityDomain = SecurityDomain.CALLER,
    ) -> DirectoryCapability:
        if security_domain is not SecurityDomain.CALLER:
            raise NotImplementedError("managed Windows security is not available")
        if not path.is_absolute():
            raise ValueError("Windows filesystem root must be absolute")
        desired_access = _directory_access(share_policy, relative_target=False)
        handle_value = self._api.CreateFileW(
            str(path),
            desired_access,
            _share_mode(share_policy),
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
        handle = int(handle_value or 0)
        if handle == INVALID_OWNED_HANDLE or handle == _INVALID_WIN32_HANDLE:
            self._raise_last_error("open filesystem root", path)
        try:
            metadata = self._metadata(handle, path)
            self._validate_metadata(
                metadata,
                parent=None,
                expected=None,
                expected_kind=EntryKind.DIRECTORY,
                operation="filesystem root",
            )
            return self._directory_capability(
                handle,
                metadata,
                parent=None,
                name=None,
                share_policy=share_policy,
                security_domain=security_domain,
                created=False,
                path_hint=path,
                desired_access=desired_access,
            )
        except BaseException as primary_error:
            self._close_after_error(
                primary_error,
                handle,
                None,
                str(path),
                bool(desired_access & DELETE),
            )
            raise

    def entry(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryEntry | None:
        _encode_windows_component(name)
        desired_access = SYNCHRONIZE | READ_CONTROL | FILE_READ_ATTRIBUTES
        try:
            handle, information = self._native_relative_open(
                parent,
                name,
                desired_access=desired_access,
                share_mode=FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                disposition=FILE_OPEN,
                create_options=FILE_OPEN_REPARSE_POINT,
                file_attributes=0,
                run_hooks=False,
            )
        except FileNotFoundError:
            return None
        try:
            if information != FILE_OPENED:
                raise OSError(
                    f"observation returned unsupported create result {information}"
                )
            metadata = self._metadata(handle, name)
            self._validate_metadata(
                metadata,
                parent=parent,
                expected=None,
                expected_kind=None,
                operation="observed filesystem entry",
            )
            result = self._directory_entry(name, metadata)
        except BaseException as primary_error:
            self._close_after_error(
                primary_error, handle, parent, name, False
            )
            raise
        resource = _WindowsResource(handle, parent, name, False)
        try:
            self.close_resource(resource)
        except BaseException:
            self._failed_closes.append(resource)
            raise
        return result

    def open_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        expected = self._observe_required(parent, name)
        desired_access = _directory_access(share_policy, relative_target=True)
        handle, information = self._native_relative_open(
            parent,
            name,
            desired_access=desired_access,
            share_mode=_share_mode(share_policy),
            disposition=FILE_OPEN,
            create_options=FILE_OPEN_REPARSE_POINT | FILE_DIRECTORY_FILE,
            file_attributes=0,
            run_hooks=True,
        )
        metadata, created = self._finish_relative_open(
            handle=handle,
            information=information,
            parent=parent,
            name=name,
            desired_access=desired_access,
            share_policy=share_policy,
            expected=expected,
            expected_kind=EntryKind.DIRECTORY,
            allowed_information={FILE_OPENED},
        )
        return self._directory_capability(
            handle,
            metadata,
            parent=parent,
            name=name,
            share_policy=share_policy,
            security_domain=parent.security_domain,
            created=created,
            path_hint=parent.path_hint / name,
            desired_access=desired_access,
        )

    def create_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        if parent.security_domain is not SecurityDomain.CALLER:
            raise NotImplementedError("managed Windows security is not available")
        desired_access = _directory_access(share_policy, relative_target=True)
        handle, information = self._native_relative_open(
            parent,
            name,
            desired_access=desired_access,
            share_mode=_share_mode(share_policy),
            disposition=FILE_CREATE,
            create_options=FILE_OPEN_REPARSE_POINT | FILE_DIRECTORY_FILE,
            file_attributes=FILE_ATTRIBUTE_DIRECTORY,
            run_hooks=True,
        )
        metadata, created = self._finish_relative_open(
            handle=handle,
            information=information,
            parent=parent,
            name=name,
            desired_access=desired_access,
            share_policy=share_policy,
            expected=None,
            expected_kind=EntryKind.DIRECTORY,
            allowed_information={FILE_CREATED},
        )
        return self._directory_capability(
            handle,
            metadata,
            parent=parent,
            name=name,
            share_policy=share_policy,
            security_domain=parent.security_domain,
            created=created,
            path_hint=parent.path_hint / name,
            desired_access=desired_access,
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
        if (
            parent.security_domain is SecurityDomain.MANAGED
            and disposition is not CreateDisposition.OPEN_EXISTING
        ):
            raise NotImplementedError("managed Windows security is not available")
        _encode_windows_component(name)
        if disposition is CreateDisposition.OPEN_EXISTING:
            expected = self._observe_required(parent, name)
            native_disposition = FILE_OPEN
            allowed_information = {FILE_OPENED}
        elif disposition is CreateDisposition.CREATE_NEW:
            expected = None
            native_disposition = FILE_CREATE
            allowed_information = {FILE_CREATED}
        else:
            expected = self.entry(parent, name)
            native_disposition = FILE_OPEN_IF
            allowed_information = {FILE_OPENED, FILE_CREATED}
        desired_access = _file_access(access, share_policy)
        handle, information = self._native_relative_open(
            parent,
            name,
            desired_access=desired_access,
            share_mode=_share_mode(share_policy),
            disposition=native_disposition,
            create_options=FILE_OPEN_REPARSE_POINT | FILE_NON_DIRECTORY_FILE,
            file_attributes=FILE_ATTRIBUTE_NORMAL,
            run_hooks=True,
        )
        metadata, created = self._finish_relative_open(
            handle=handle,
            information=information,
            parent=parent,
            name=name,
            desired_access=desired_access,
            share_policy=share_policy,
            expected=expected,
            expected_kind=EntryKind.REGULAR,
            allowed_information=allowed_information,
        )
        resource = _WindowsResource(
            handle, parent, name, bool(desired_access & DELETE)
        )
        return FileCapability(
            self,
            resource,
            identity=metadata.identity,
            filesystem=metadata.filesystem,
            kind=EntryKind.REGULAR,
            logical_size=metadata.logical_size,
            modified_ns=metadata.modified_ns,
            security_domain=parent.security_domain,
            share_policy=share_policy,
            created=created,
            path_hint=parent.path_hint / name,
        )

    def open_entry(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> FileCapability | DirectoryCapability:
        expected = self._observe_required(parent, name)
        desired_access = _entry_access(share_policy)
        handle, information = self._native_relative_open(
            parent,
            name,
            desired_access=desired_access,
            share_mode=_share_mode(share_policy),
            disposition=FILE_OPEN,
            create_options=FILE_OPEN_REPARSE_POINT,
            file_attributes=0,
            run_hooks=True,
        )
        metadata, created = self._finish_relative_open(
            handle=handle,
            information=information,
            parent=parent,
            name=name,
            desired_access=desired_access,
            share_policy=share_policy,
            expected=expected,
            expected_kind=None,
            allowed_information={FILE_OPENED},
        )
        return self._entry_capability(
            handle,
            metadata,
            parent=parent,
            name=name,
            share_policy=share_policy,
            created=created,
            desired_access=desired_access,
        )

    def reopen_directory(
        self,
        directory: DirectoryCapability,
        share_policy: SharePolicy | None = None,
    ) -> DirectoryCapability:
        effective_policy = (
            directory.share_policy if share_policy is None else share_policy
        )
        if effective_policy is not directory.share_policy:
            raise ValueError("duplicate handle cannot change its share policy")
        source = self._resource(directory)
        process = self._api.GetCurrentProcess()
        duplicate = HANDLE()
        if not self._api.DuplicateHandle(
            process,
            source.handle,
            process,
            ctypes.byref(duplicate),
            0,
            False,
            DUPLICATE_SAME_ACCESS,
        ):
            self._raise_last_error("duplicate directory capability", source.name)
        handle = int(duplicate.value or 0)
        try:
            metadata = self._metadata(handle, source.name)
            if (
                metadata.identity != directory.identity
                or metadata.filesystem != directory.filesystem
                or metadata.kind is not EntryKind.DIRECTORY
            ):
                raise OSError("directory identity changed while duplicating")
            return self._directory_capability(
                handle,
                metadata,
                parent=source.parent,
                name=source.name,
                share_policy=effective_policy,
                security_domain=directory.security_domain,
                created=False,
                path_hint=directory.path_hint,
                desired_access=DELETE if source.delete_authority else 0,
            )
        except BaseException as primary_error:
            self._close_after_error(
                primary_error,
                handle,
                source.parent,
                source.name,
                source.delete_authority,
            )
            raise

    def create_secure_root(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryCapability:
        raise NotImplementedError("managed Windows security is not available")

    def entries(self, parent: DirectoryCapability) -> DirectoryIterator:
        raise NotImplementedError("Windows enumeration is not available")

    def entries_owned(self, parent: DirectoryCapability) -> DirectoryIterator:
        raise NotImplementedError("Windows enumeration is not available")

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        raise NotImplementedError("Windows rename is not available")

    def delete(self, capability: FileCapability | DirectoryCapability) -> None:
        raise NotImplementedError("Windows delete is not available")

    def available_bytes(self, directory: DirectoryCapability) -> int:
        raise NotImplementedError("Windows capacity is not available")

    def allocation_unit(self, directory: DirectoryCapability) -> int:
        raise NotImplementedError("Windows capacity is not available")

    def touch(self, file: FileCapability) -> None:
        raise NotImplementedError("Windows timestamp mutation is not available")

    def flush(self, file: FileCapability) -> None:
        raise NotImplementedError("Windows flush is not available")

    def final_path(self, directory: DirectoryCapability) -> Path:
        raise NotImplementedError("Windows final path is not available")

    def verify_managed_security(
        self,
        capability: FileCapability | DirectoryCapability,
        *,
        repair_dacl: bool,
    ) -> None:
        raise NotImplementedError("managed Windows security is not available")

    def __del__(self) -> None:
        for resource in getattr(self, "_failed_closes", ()):
            try:
                self.close_resource(resource)
            except (OSError, RuntimeError):
                pass
