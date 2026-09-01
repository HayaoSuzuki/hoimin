from __future__ import annotations

import ctypes
import errno
import os
import re
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, cast

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
ULONGLONG = ctypes.c_uint64

DELETE = 0x0001_0000
READ_CONTROL = 0x0002_0000
WRITE_DAC = 0x0004_0000
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
FILE_RENAME_INFORMATION_CLASS = 10
FILE_DISPOSITION_INFO_CLASS = 4
FILE_DISPOSITION_INFO_EX_CLASS = 21
FILE_ID_INFO_CLASS = 18
FILE_ID_EXTD_DIRECTORY_INFO_CLASS = 19
FILE_ID_EXTD_DIRECTORY_RESTART_INFO_CLASS = 20
DUPLICATE_SAME_ACCESS = 0x0002
VOLUME_NAME_DOS = 0x0
VOLUME_NAME_GUID = 0x1

_WINDOWS_EPOCH_100NS = 116_444_736_000_000_000
_DIRECTORY_BUFFER_BYTES = 64 * 1024
_FINAL_PATH_INITIAL_UNITS = 512
_FINAL_PATH_MAX_UNITS = 16 * 1024
_MAX_UINT64 = (1 << 64) - 1
_MAX_TOKEN_USER_BYTES = 64 * 1024
_VOLUME_GUID_ROOT = re.compile(
    r"^\\\\\?\\Volume\{"
    r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-"
    r"[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}"
    r"\}\\"
)

ERROR_FILE_NOT_FOUND = 2
ERROR_PATH_NOT_FOUND = 3
ERROR_ACCESS_DENIED = 5
ERROR_NO_MORE_FILES = 18
ERROR_NOT_SUPPORTED = 50
ERROR_FILE_EXISTS = 80
ERROR_INVALID_PARAMETER = 87
ERROR_INSUFFICIENT_BUFFER = 122
ERROR_ALREADY_EXISTS = 183
INVALID_OWNED_HANDLE = 0
_INVALID_WIN32_HANDLE = ctypes.c_void_p(-1).value
_ERROR_EVIDENCE_BYTES = 4_096
_OPEN_OR_CREATE_CYCLES = 8

TOKEN_QUERY = 0x0008
TokenUser = 1
SDDL_REVISION_1 = 1
SE_FILE_OBJECT = 1
OWNER_SECURITY_INFORMATION = 0x0000_0001
GROUP_SECURITY_INFORMATION = 0x0000_0002
DACL_SECURITY_INFORMATION = 0x0000_0004
PROTECTED_DACL_SECURITY_INFORMATION = 0x8000_0000
SE_DACL_PROTECTED = 0x1000

FILE_DISPOSITION_FLAG_DELETE = 0x0000_0001
FILE_DISPOSITION_FLAG_POSIX_SEMANTICS = 0x0000_0002
FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE = 0x0000_0010

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


class SID_AND_ATTRIBUTES(ctypes.Structure):
    _fields_ = [("Sid", ctypes.c_void_p), ("Attributes", ULONG)]


class TOKEN_USER(ctypes.Structure):
    _fields_ = [("User", SID_AND_ATTRIBUTES)]


SECURITY_DESCRIPTOR_CONTROL = USHORT


class ACL(ctypes.Structure):
    _fields_ = [
        ("AclRevision", ctypes.c_uint8),
        ("Sbz1", ctypes.c_uint8),
        ("AclSize", USHORT),
        ("AceCount", USHORT),
        ("Sbz2", USHORT),
    ]


class FILE_DISPOSITION_INFO(ctypes.Structure):
    _fields_ = [("DeleteFile", ctypes.c_uint8)]


class FILE_DISPOSITION_INFO_EX(ctypes.Structure):
    _fields_ = [("Flags", ULONG)]


class _LocalAllocation:
    __slots__ = ("_api", "_label", "_pointer")

    def __init__(self, api: Any, pointer: ctypes.c_void_p, label: str) -> None:
        if not pointer.value:
            raise OSError(f"{label} is null")
        self._api = api
        self._pointer = pointer
        self._label = label

    @property
    def pointer(self) -> ctypes.c_void_p:
        if not self._pointer.value:
            raise RuntimeError(f"{self._label} is not owned")
        return self._pointer

    @property
    def is_owned(self) -> bool:
        return bool(self._pointer.value)

    def close(self) -> None:
        if not self._pointer.value:
            return
        failed = self._api.LocalFree(self._pointer)
        if failed:
            raise _error_from_win32(
                self._api.last_error(), f"release {self._label}", self._label
            )
        self._pointer = ctypes.c_void_p()

    def __del__(self) -> None:
        try:
            self.close()
        except (OSError, RuntimeError):
            pass


@dataclass(slots=True)
class _TokenUserValue:
    buffer: Any
    sid: ctypes.c_void_p
    sid_text: str


@dataclass(slots=True)
class _ManagedSecurityMaterial:
    sid_owner: _TokenUserValue
    allocation: _LocalAllocation
    descriptor: ctypes.c_void_p
    dacl: ctypes.c_void_p
    dacl_bytes: bytes

    @property
    def sid(self) -> ctypes.c_void_p:
        return self.sid_owner.sid

    def close(self) -> None:
        self.allocation.close()


@dataclass(slots=True)
class _SecuritySnapshot:
    allocation: _LocalAllocation
    owner: ctypes.c_void_p
    dacl: ctypes.c_void_p
    dacl_present: bool
    control: int
    dacl_bytes: bytes

    def close(self) -> None:
        self.allocation.close()


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


def _add_close_note(primary_error: BaseException, close: Callable[[], None]) -> None:
    try:
        close()
    except BaseException as close_error:
        primary_error.add_note(
            _bounded_evidence(
                f"filesystem capability close failed: {close_error}"
            )
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


def _managed_security_sddl(user_sid: str, *, directory: bool) -> str:
    inheritance = "OICI" if directory else ""
    return (
        f"O:{user_sid}D:P(A;{inheritance};FA;;;{user_sid})"
        f"(A;{inheritance};FA;;;SY)(A;{inheritance};FA;;;BA)"
    )


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


class _DirectoryRecordParser:
    def __init__(
        self,
        encoded: bytes | bytearray | memoryview,
        filesystem: FilesystemIdentity,
    ) -> None:
        self._encoded = memoryview(encoded).cast("B")
        self._filesystem = filesystem
        self._offset = 0
        self._finished = False

    @staticmethod
    def _aligned(value: int) -> int:
        return (value + 7) & ~7

    def next_record(self) -> DirectoryEntry | None:
        while not self._finished:
            offset = self._offset
            header_size = FILE_ID_EXTD_DIR_INFO.FileName.offset
            if offset < 0 or offset + header_size > len(self._encoded):
                raise OSError("directory record header is outside the buffer")
            (
                next_entry_offset,
                _file_index,
                _creation_time,
                _last_access_time,
                last_write_time,
                _change_time,
                end_of_file,
                _allocation_size,
                file_attributes,
                file_name_length,
                _ea_size,
                reparse_point_tag,
            ) = struct.unpack_from("<IIqqqqqqIIII", self._encoded, offset)

            if file_name_length % 2:
                raise OSError("directory record has an odd UTF-16 name length")
            name_start = offset + header_size
            name_end = name_start + file_name_length

            if next_entry_offset:
                if next_entry_offset % 8:
                    raise OSError("directory record offset is not aligned")
                next_offset = offset + next_entry_offset
                if next_offset <= offset:
                    raise OSError("directory record offset does not advance")
                if next_offset + header_size > len(self._encoded):
                    raise OSError("directory record offset is outside the buffer")
                if next_offset < self._aligned(name_end):
                    raise OSError("directory record offset overlaps this record")
                record_limit = next_offset
            else:
                record_limit = len(self._encoded)

            if name_end > record_limit:
                raise OSError("directory record name is outside the record")
            try:
                name = bytes(self._encoded[name_start:name_end]).decode(
                    "utf-16-le", errors="strict"
                )
            except UnicodeDecodeError as error:
                raise OSError("directory record name is invalid UTF-16") from error

            identity_value = int.from_bytes(
                self._encoded[offset + 72 : offset + 88], "little"
            )
            if identity_value == 0:
                raise OSError("directory record has a zero file identity")
            if self._filesystem.volume <= 0:
                raise OSError("directory record has a zero parent volume")
            if end_of_file < 0:
                raise OSError("directory record has a negative logical size")
            if last_write_time < 0:
                raise OSError("directory record has a negative raw FILETIME")
            is_reparse = bool(file_attributes & FILE_ATTRIBUTE_REPARSE_POINT)
            if is_reparse != bool(reparse_point_tag):
                raise OSError("directory record has an inconsistent reparse tag")

            if next_entry_offset:
                self._offset = offset + next_entry_offset
            else:
                padding_start = self._aligned(name_end)
                if any(self._encoded[padding_start:]):
                    raise OSError(
                        "directory record terminates before the final record"
                    )
                self._finished = True

            if name in {".", ".."}:
                continue
            return DirectoryEntry(
                name=name,
                kind=_entry_kind_from_attributes(file_attributes),
                identity=FileIdentity(self._filesystem.volume, identity_value),
                filesystem=self._filesystem,
                logical_size=end_of_file,
                modified_ns=_filetime_to_unix_ns(last_write_time),
            )
        return None


class _WindowsEntries:
    def __init__(
        self,
        backend: WindowsFilesystemBackend,
        source: DirectoryCapability,
    ) -> None:
        self._backend = backend
        self._buffer = ctypes.create_string_buffer(_DIRECTORY_BUFFER_BYTES)
        self._parser: _DirectoryRecordParser | None = None
        self._restart = True
        self._closed = False
        self._directory = source._move_for(backend)

    @property
    def directory(self) -> DirectoryCapability:
        return self._directory

    def __iter__(self) -> _WindowsEntries:
        return self

    def _refill(self) -> bool:
        resource = self._backend._resource(self._directory)
        information_class = (
            FILE_ID_EXTD_DIRECTORY_RESTART_INFO_CLASS
            if self._restart
            else FILE_ID_EXTD_DIRECTORY_INFO_CLASS
        )
        self._restart = False
        ctypes.memset(self._buffer, 0, _DIRECTORY_BUFFER_BYTES)
        if not self._backend._api.GetFileInformationByHandleEx(
            resource.handle,
            information_class,
            self._buffer,
            _DIRECTORY_BUFFER_BYTES,
        ):
            code = self._backend._api.last_error()
            if code == ERROR_NO_MORE_FILES:
                return False
            raise _error_from_win32(
                code,
                "enumerate directory capability",
                self._directory.path_hint,
            )
        self._parser = _DirectoryRecordParser(
            bytes(self._buffer), self._directory.filesystem
        )
        return True

    def __next__(self) -> DirectoryEntry:
        if self._closed:
            raise StopIteration
        while True:
            try:
                if self._parser is not None:
                    record = self._parser.next_record()
                    if record is not None:
                        return record
                    self._parser = None
                refilled = self._refill()
            except BaseException as primary_error:
                _add_close_note(primary_error, self.close)
                raise
            if not refilled:
                self.close()
                raise StopIteration

    def close(self) -> None:
        if self._closed:
            return
        self._directory.close()
        self._closed = True

    def __del__(self) -> None:
        if not hasattr(self, "_directory"):
            return
        try:
            self.close()
        except OSError:
            pass


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
        self.GetFinalPathNameByHandleW = kernel32.GetFinalPathNameByHandleW
        self.GetFinalPathNameByHandleW.argtypes = (
            HANDLE,
            ctypes.POINTER(WCHAR),
            ULONG,
            ULONG,
        )
        self.GetFinalPathNameByHandleW.restype = ULONG
        self.GetVolumePathNameW = kernel32.GetVolumePathNameW
        self.GetVolumePathNameW.argtypes = (
            ctypes.c_wchar_p,
            ctypes.POINTER(WCHAR),
            ULONG,
        )
        self.GetVolumePathNameW.restype = BOOL
        self.GetVolumeInformationW = kernel32.GetVolumeInformationW
        self.GetVolumeInformationW.argtypes = (
            ctypes.c_wchar_p,
            ctypes.POINTER(WCHAR),
            ULONG,
            ctypes.POINTER(ULONG),
            ctypes.POINTER(ULONG),
            ctypes.POINTER(ULONG),
            ctypes.POINTER(WCHAR),
            ULONG,
        )
        self.GetVolumeInformationW.restype = BOOL
        self.GetDiskFreeSpaceExW = kernel32.GetDiskFreeSpaceExW
        self.GetDiskFreeSpaceExW.argtypes = (
            ctypes.c_wchar_p,
            ctypes.POINTER(ULONGLONG),
            ctypes.POINTER(ULONGLONG),
            ctypes.POINTER(ULONGLONG),
        )
        self.GetDiskFreeSpaceExW.restype = BOOL
        self.GetDiskFreeSpaceW = kernel32.GetDiskFreeSpaceW
        self.GetDiskFreeSpaceW.argtypes = (
            ctypes.c_wchar_p,
            ctypes.POINTER(ULONG),
            ctypes.POINTER(ULONG),
            ctypes.POINTER(ULONG),
            ctypes.POINTER(ULONG),
        )
        self.GetDiskFreeSpaceW.restype = BOOL
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
        self.SetFileInformationByHandle = kernel32.SetFileInformationByHandle
        self.SetFileInformationByHandle.argtypes = (
            HANDLE,
            ctypes.c_int32,
            ctypes.c_void_p,
            ULONG,
        )
        self.SetFileInformationByHandle.restype = BOOL
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
        self.LocalFree = kernel32.LocalFree
        self.LocalFree.argtypes = (ctypes.c_void_p,)
        self.LocalFree.restype = ctypes.c_void_p

        advapi32 = _ADVAPI32
        self.OpenProcessToken = advapi32.OpenProcessToken
        self.OpenProcessToken.argtypes = (
            HANDLE,
            ULONG,
            ctypes.POINTER(HANDLE),
        )
        self.OpenProcessToken.restype = BOOL
        self.GetTokenInformation = advapi32.GetTokenInformation
        self.GetTokenInformation.argtypes = (
            HANDLE,
            ULONG,
            ctypes.c_void_p,
            ULONG,
            ctypes.POINTER(ULONG),
        )
        self.GetTokenInformation.restype = BOOL
        self.ConvertSidToStringSidW = advapi32.ConvertSidToStringSidW
        self.ConvertSidToStringSidW.argtypes = (
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_wchar_p),
        )
        self.ConvertSidToStringSidW.restype = BOOL
        self.ConvertStringSecurityDescriptorToSecurityDescriptorW = (
            advapi32.ConvertStringSecurityDescriptorToSecurityDescriptorW
        )
        self.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = (
            ctypes.c_wchar_p,
            ULONG,
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ULONG),
        )
        self.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype = BOOL
        self.GetSecurityInfo = advapi32.GetSecurityInfo
        self.GetSecurityInfo.argtypes = (
            HANDLE,
            ULONG,
            ULONG,
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
        )
        self.GetSecurityInfo.restype = ULONG
        self.SetSecurityInfo = advapi32.SetSecurityInfo
        self.SetSecurityInfo.argtypes = (
            HANDLE,
            ULONG,
            ULONG,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
        )
        self.SetSecurityInfo.restype = ULONG
        self.GetNamedSecurityInfoW = advapi32.GetNamedSecurityInfoW
        self.GetNamedSecurityInfoW.argtypes = (
            ctypes.c_wchar_p,
            ULONG,
            ULONG,
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
        )
        self.GetNamedSecurityInfoW.restype = ULONG
        self.GetSecurityDescriptorLength = advapi32.GetSecurityDescriptorLength
        self.GetSecurityDescriptorLength.argtypes = (ctypes.c_void_p,)
        self.GetSecurityDescriptorLength.restype = ULONG
        self.GetSecurityDescriptorControl = advapi32.GetSecurityDescriptorControl
        self.GetSecurityDescriptorControl.argtypes = (
            ctypes.c_void_p,
            ctypes.POINTER(SECURITY_DESCRIPTOR_CONTROL),
            ctypes.POINTER(ULONG),
        )
        self.GetSecurityDescriptorControl.restype = BOOL
        self.GetSecurityDescriptorDacl = advapi32.GetSecurityDescriptorDacl
        self.GetSecurityDescriptorDacl.argtypes = (
            ctypes.c_void_p,
            ctypes.POINTER(BOOL),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(BOOL),
        )
        self.GetSecurityDescriptorDacl.restype = BOOL
        self.EqualSid = advapi32.EqualSid
        self.EqualSid.argtypes = (ctypes.c_void_p, ctypes.c_void_p)
        self.EqualSid.restype = BOOL
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
        self.NtSetInformationFile = ntdll.NtSetInformationFile
        self.NtSetInformationFile.argtypes = (
            HANDLE,
            ctypes.POINTER(IO_STATUS_BLOCK),
            ctypes.c_void_p,
            ULONG,
            ULONG,
        )
        self.NtSetInformationFile.restype = NTSTATUS
        self.RtlNtStatusToDosError = ntdll.RtlNtStatusToDosError
        self.RtlNtStatusToDosError.argtypes = (NTSTATUS,)
        self.RtlNtStatusToDosError.restype = ULONG

    def last_error(self) -> int:
        return ctypes.get_last_error()

    @staticmethod
    def _buffer_text(buffer: Any, capacity: int) -> str:
        encoded = ctypes.string_at(buffer, capacity * ctypes.sizeof(WCHAR))
        terminator = next(
            (
                index
                for index in range(0, len(encoded), 2)
                if encoded[index : index + 2] == b"\0\0"
            ),
            None,
        )
        if terminator is None:
            raise OSError("Windows path buffer has no terminator")
        try:
            return encoded[:terminator].decode("utf-16-le", errors="strict")
        except UnicodeDecodeError as error:
            raise OSError("Windows path buffer is invalid UTF-16") from error

    def get_volume_path(self, path: str) -> str:
        buffer = (WCHAR * _FINAL_PATH_MAX_UNITS)()
        if not self.GetVolumePathNameW(path, buffer, _FINAL_PATH_MAX_UNITS):
            raise _error_from_win32(
                self.last_error(), "recover filesystem volume root", path
            )
        return self._buffer_text(buffer, _FINAL_PATH_MAX_UNITS)

    def get_volume_information(self, root: str) -> tuple[int, int, int]:
        serial = ULONG()
        maximum_component_length = ULONG()
        filesystem_flags = ULONG()
        if not self.GetVolumeInformationW(
            root,
            None,
            0,
            ctypes.byref(serial),
            ctypes.byref(maximum_component_length),
            ctypes.byref(filesystem_flags),
            None,
            0,
        ):
            raise _error_from_win32(
                self.last_error(), "read path filesystem identity", root
            )
        return (
            int(serial.value),
            int(maximum_component_length.value),
            int(filesystem_flags.value),
        )

    def get_disk_free_space_ex(self, path: str) -> int:
        available = ULONGLONG()
        if not self.GetDiskFreeSpaceExW(
            path, ctypes.byref(available), None, None
        ):
            raise _error_from_win32(
                self.last_error(), "read filesystem available bytes", path
            )
        return int(available.value)

    def get_disk_free_space(self, root: str) -> tuple[int, int]:
        sectors_per_cluster = ULONG()
        bytes_per_sector = ULONG()
        free_clusters = ULONG()
        total_clusters = ULONG()
        if not self.GetDiskFreeSpaceW(
            root,
            ctypes.byref(sectors_per_cluster),
            ctypes.byref(bytes_per_sector),
            ctypes.byref(free_clusters),
            ctypes.byref(total_clusters),
        ):
            raise _error_from_win32(
                self.last_error(), "read filesystem allocation unit", root
            )
        return int(sectors_per_cluster.value), int(bytes_per_sector.value)

    def before_relative_open(self) -> None:
        return None

    def after_relative_open(self) -> None:
        return None


@dataclass(slots=True)
class _CreatedRollbackState:
    parent: DirectoryCapability
    name: str
    primary_error: BaseException
    remaining_owners: int
    completed: bool = False


@dataclass(slots=True)
class _WindowsResource:
    handle: int
    parent: DirectoryCapability | None
    name: str | None
    delete_authority: bool
    desired_access: int = 0
    share_mode: int = 0
    disposition_set: bool = False
    rollback_state: _CreatedRollbackState | None = None
    secure_root_creation: bool = False


class _OpenCollision(Exception):
    def __init__(self, error: OSError) -> None:
        super().__init__(str(error))
        self.error = error


class WindowsFilesystemBackend:
    @property
    def directory_rename_requires_closed_descendants(self) -> bool:
        return True

    def _directory_creation_rollback_available(
        self, directory: DirectoryCapability
    ) -> bool:
        resource = self._resource(directory)
        return (
            directory.created
            and directory.kind is EntryKind.DIRECTORY
            and resource.parent is not None
            and resource.name is not None
            and resource.delete_authority
        )

    def __init__(
        self,
        *,
        api: Any | None = None,
        osfhandle_opener: Callable[[int, int], int] | None = None,
    ) -> None:
        self._failed_closes: list[_WindowsResource] = []
        self._failed_security_owners: list[Any] = []
        self._pending_directory_resources: dict[int, _WindowsResource] = {}
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
        rollback_state = native.rollback_state
        native.rollback_state = None
        if rollback_state is not None:
            rollback_state.remaining_owners -= 1
            if rollback_state.remaining_owners < 0:
                raise RuntimeError("created rollback owner count underflow")
            if rollback_state.remaining_owners == 0:
                self._verify_created_rollback_absence(rollback_state)

    def _verify_created_rollback_absence(
        self, state: _CreatedRollbackState
    ) -> None:
        if state.completed:
            return
        state.completed = True
        try:
            observed = self.entry(state.parent, state.name)
        except BaseException as observation_error:
            state.primary_error.add_note(
                _bounded_evidence(
                    "created-object absence verification failed: "
                    f"{observation_error}"
                )
            )
            return
        if observed is not None:
            state.primary_error.add_note(
                _bounded_evidence(
                    "created-object rollback found the original or a "
                    f"same-name replacement: {observed.identity!r}"
                )
            )

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

    def _close_security_owner(
        self,
        owner: Any,
        primary_error: BaseException | None = None,
    ) -> None:
        try:
            owner.close()
        except BaseException as close_error:
            self._failed_security_owners.append(owner)
            if primary_error is None:
                raise
            primary_error.add_note(
                _bounded_evidence(
                    f"security allocation release failed: {close_error}"
                )
            )

    @staticmethod
    def _acl_bytes(dacl: ctypes.c_void_p) -> bytes:
        if not dacl.value:
            raise OSError("security descriptor DACL is null")
        acl = ctypes.cast(dacl, ctypes.POINTER(ACL)).contents
        size = int(acl.AclSize)
        if size < ctypes.sizeof(ACL):
            raise OSError("security descriptor DACL size is invalid")
        return ctypes.string_at(dacl, size)

    def _current_token_user(self) -> _TokenUserValue:
        token = HANDLE()
        if not self._api.OpenProcessToken(
            self._api.GetCurrentProcess(),
            TOKEN_QUERY,
            ctypes.byref(token),
        ):
            self._raise_last_error("open current process token", "TOKEN_USER")
        token_handle = int(token.value or 0)
        if token_handle == INVALID_OWNED_HANDLE:
            raise OSError("OpenProcessToken returned an invalid handle")
        try:
            needed = ULONG()
            size_result = self._api.GetTokenInformation(
                token,
                TokenUser,
                None,
                0,
                ctypes.byref(needed),
            )
            if size_result or self._api.last_error() != ERROR_INSUFFICIENT_BUFFER:
                raise OSError("TOKEN_USER size query returned an unexpected result")
            size = int(needed.value)
            if size <= 0 or size > _MAX_TOKEN_USER_BYTES:
                raise OSError("TOKEN_USER size is outside the bounded buffer")
            buffer = ctypes.create_string_buffer(size)
            if not self._api.GetTokenInformation(
                token,
                TokenUser,
                buffer,
                size,
                ctypes.byref(needed),
            ):
                self._raise_last_error("read current token user", "TOKEN_USER")
            if int(needed.value) <= 0 or int(needed.value) > size:
                raise OSError("TOKEN_USER returned an invalid buffer length")
            token_user = ctypes.cast(buffer, ctypes.POINTER(TOKEN_USER)).contents
            sid = ctypes.c_void_p(token_user.User.Sid)
            if not sid.value:
                raise OSError("TOKEN_USER returned a null SID")
            sid_string = ctypes.c_wchar_p()
            if not self._api.ConvertSidToStringSidW(
                sid, ctypes.byref(sid_string)
            ):
                self._raise_last_error("convert current token SID", "TOKEN_USER")
            allocation = _LocalAllocation(
                self._api,
                ctypes.cast(sid_string, ctypes.c_void_p),
                "token SID string",
            )
            try:
                if not sid_string.value:
                    raise OSError("token SID conversion returned an empty string")
                sid_text = sid_string.value
            except BaseException as primary_error:
                self._close_security_owner(allocation, primary_error)
                raise
            self._close_security_owner(allocation)
            result = _TokenUserValue(buffer, sid, sid_text)
        except BaseException as primary_error:
            self._close_after_error(
                primary_error,
                token_handle,
                None,
                "process token",
                False,
            )
            raise
        token_resource = _WindowsResource(
            token_handle, None, "process token", False
        )
        try:
            self.close_resource(token_resource)
        except BaseException:
            self._failed_closes.append(token_resource)
            raise
        return result

    def _managed_security_material(
        self, *, directory: bool
    ) -> _ManagedSecurityMaterial:
        token_user = self._current_token_user()
        sddl = _managed_security_sddl(
            token_user.sid_text, directory=directory
        )
        descriptor = ctypes.c_void_p()
        descriptor_size = ULONG()
        if not self._api.ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl,
            SDDL_REVISION_1,
            ctypes.byref(descriptor),
            ctypes.byref(descriptor_size),
        ):
            self._raise_last_error("convert managed security descriptor", sddl)
        allocation = _LocalAllocation(
            self._api, descriptor, "managed security descriptor"
        )
        try:
            if int(descriptor_size.value) <= 0:
                raise OSError("managed security descriptor has zero length")
            present = BOOL()
            defaulted = BOOL()
            dacl = ctypes.c_void_p()
            if not self._api.GetSecurityDescriptorDacl(
                descriptor,
                ctypes.byref(present),
                ctypes.byref(dacl),
                ctypes.byref(defaulted),
            ):
                self._raise_last_error(
                    "read managed security descriptor DACL", sddl
                )
            if not present.value or not dacl.value:
                raise OSError("managed security descriptor has no DACL")
            dacl_bytes = self._acl_bytes(dacl)
            return _ManagedSecurityMaterial(
                token_user,
                allocation,
                descriptor,
                dacl,
                dacl_bytes,
            )
        except BaseException as primary_error:
            self._close_security_owner(allocation, primary_error)
            raise

    def _read_security_snapshot(
        self,
        resource: _WindowsResource,
        component: object,
    ) -> _SecuritySnapshot:
        owner = ctypes.c_void_p()
        dacl = ctypes.c_void_p()
        descriptor = ctypes.c_void_p()
        result = int(
            self._api.GetSecurityInfo(
                resource.handle,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                ctypes.byref(owner),
                None,
                ctypes.byref(dacl),
                None,
                ctypes.byref(descriptor),
            )
        )
        if result != 0:
            raise _error_from_win32(
                result, "read managed filesystem security", component
            )
        allocation = _LocalAllocation(
            self._api, descriptor, "queried security descriptor"
        )
        try:
            if not owner.value:
                raise OSError("managed filesystem owner is null")
            control = SECURITY_DESCRIPTOR_CONTROL()
            revision = ULONG()
            if not self._api.GetSecurityDescriptorControl(
                descriptor,
                ctypes.byref(control),
                ctypes.byref(revision),
            ):
                self._raise_last_error(
                    "read managed security descriptor control", component
                )
            present = BOOL()
            defaulted = BOOL()
            descriptor_dacl = ctypes.c_void_p()
            if not self._api.GetSecurityDescriptorDacl(
                descriptor,
                ctypes.byref(present),
                ctypes.byref(descriptor_dacl),
                ctypes.byref(defaulted),
            ):
                self._raise_last_error(
                    "read managed security descriptor DACL", component
                )
            if descriptor_dacl.value != dacl.value:
                raise OSError("managed security descriptor DACL is inconsistent")
            dacl_bytes = (
                self._acl_bytes(descriptor_dacl)
                if present.value and descriptor_dacl.value
                else b""
            )
            return _SecuritySnapshot(
                allocation,
                owner,
                descriptor_dacl,
                bool(present.value),
                int(control.value),
                dacl_bytes,
            )
        except BaseException as primary_error:
            self._close_security_owner(allocation, primary_error)
            raise

    def _verify_managed_security_resource(
        self,
        resource: _WindowsResource,
        *,
        directory: bool,
        component: object,
        repair_dacl: bool,
        material: Any | None = None,
    ) -> None:
        owns_material = material is None
        if material is None:
            material = self._managed_security_material(directory=directory)
        primary_error: BaseException | None = None
        try:
            repaired = False
            while True:
                snapshot = self._read_security_snapshot(resource, component)
                snapshot_error: BaseException | None = None
                try:
                    if not self._api.EqualSid(snapshot.owner, material.sid):
                        raise PermissionError(
                            "managed filesystem owner does not match TOKEN_USER"
                        )
                    dacl_matches = (
                        snapshot.dacl_present
                        and bool(snapshot.dacl.value)
                        and bool(snapshot.control & SE_DACL_PROTECTED)
                        and snapshot.dacl_bytes == material.dacl_bytes
                    )
                    if dacl_matches:
                        return
                    if not repair_dacl or repaired:
                        raise PermissionError(
                            "managed filesystem DACL does not match the protocol"
                        )
                    result = int(
                        self._api.SetSecurityInfo(
                            resource.handle,
                            SE_FILE_OBJECT,
                            DACL_SECURITY_INFORMATION
                            | PROTECTED_DACL_SECURITY_INFORMATION,
                            None,
                            None,
                            material.dacl,
                            None,
                        )
                    )
                    if result != 0:
                        raise _error_from_win32(
                            result,
                            "repair managed filesystem DACL",
                            component,
                        )
                    repaired = True
                except BaseException as error:
                    snapshot_error = error
                    raise
                finally:
                    self._close_security_owner(snapshot, snapshot_error)
        except BaseException as error:
            primary_error = error
            raise
        finally:
            if owns_material:
                self._close_security_owner(material, primary_error)

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
        security_descriptor: ctypes.c_void_p | None = None,
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
            security_descriptor,
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
        information = int(io_status.Information)
        if run_hooks:
            try:
                self._api.after_relative_open()
            except BaseException as primary_error:
                resource = _WindowsResource(
                    handle,
                    parent,
                    name,
                    bool(desired_access & DELETE),
                    desired_access,
                    share_mode,
                )
                if information == FILE_CREATED:
                    self._rollback_created_resources(primary_error, resource)
                else:
                    self._close_resources_after_error(
                        primary_error, (resource,)
                    )
                raise
        return handle, information

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
        share_mode: int | None = None,
    ) -> DirectoryCapability:
        actual_share_mode = (
            _share_mode(share_policy) if share_mode is None else share_mode
        )
        resource = self._pending_directory_resources.pop(handle, None)
        if resource is None:
            resource = _WindowsResource(
                handle,
                parent,
                name,
                bool(desired_access & DELETE),
                desired_access,
                actual_share_mode,
            )
        return DirectoryCapability(
            self,
            resource,
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
            handle,
            parent,
            name,
            bool(desired_access & DELETE),
            desired_access,
            _share_mode(share_policy),
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

    def _relative_directory_capability(
        self,
        *,
        handle: int,
        metadata: _Metadata,
        information: int,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
        security_domain: SecurityDomain,
        created: bool,
        path_hint: Path,
        actual_access: int,
        final_access: int,
        secure_root_creation: bool = False,
    ) -> DirectoryCapability:
        owned_access = (
            actual_access if information == FILE_CREATED else final_access
        )
        resource = _WindowsResource(
            handle,
            parent,
            name,
            bool(owned_access & DELETE),
            owned_access,
            _share_mode(share_policy),
            secure_root_creation=secure_root_creation,
        )
        try:
            self._pending_directory_resources[resource.handle] = resource
            capability = self._directory_capability(
                resource.handle,
                metadata,
                parent=parent,
                name=name,
                share_policy=share_policy,
                security_domain=security_domain,
                created=created,
                path_hint=path_hint,
                desired_access=owned_access,
            )
        except BaseException as primary_error:
            self._pending_directory_resources.pop(resource.handle, None)
            self._cleanup_relative_resources_after_error(
                primary_error,
                resource=resource,
                information=information,
            )
            raise
        owned_resource = self._resource(capability)
        if owned_resource is not resource:
            raise RuntimeError("directory capability handoff changed handle")
        return capability

    def _relative_file_capability(
        self,
        *,
        handle: int,
        metadata: _Metadata,
        information: int,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
        created: bool,
        actual_access: int,
        final_access: int,
    ) -> FileCapability:
        owned_access = (
            actual_access if information == FILE_CREATED else final_access
        )
        resource = _WindowsResource(
            handle,
            parent,
            name,
            bool(owned_access & DELETE),
            owned_access,
            _share_mode(share_policy),
        )
        try:
            capability = FileCapability(
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
        except BaseException as primary_error:
            self._cleanup_relative_resources_after_error(
                primary_error,
                resource=resource,
                information=information,
            )
            raise
        return capability

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

    def _set_delete_disposition(self, resource: _WindowsResource) -> None:
        extended = FILE_DISPOSITION_INFO_EX(
            FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE
        )
        if self._api.SetFileInformationByHandle(
            resource.handle,
            FILE_DISPOSITION_INFO_EX_CLASS,
            ctypes.byref(extended),
            ctypes.sizeof(extended),
        ):
            return
        code = self._api.last_error()
        if code not in {ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED}:
            raise _error_from_win32(
                code, "set exact-handle delete disposition", resource.name
            )
        legacy = FILE_DISPOSITION_INFO(True)
        if not self._api.SetFileInformationByHandle(
            resource.handle,
            FILE_DISPOSITION_INFO_CLASS,
            ctypes.byref(legacy),
            ctypes.sizeof(legacy),
        ):
            raise _error_from_win32(
                self._api.last_error(),
                "set legacy exact-handle delete disposition",
                resource.name,
            )

    def _rollback_created_handle(
        self,
        primary_error: BaseException,
        handle: int,
        parent: DirectoryCapability,
        name: str,
        desired_access: int,
        share_mode: int,
    ) -> None:
        guard = _WindowsResource(
            handle,
            parent,
            name,
            bool(desired_access & DELETE),
            desired_access,
            share_mode,
        )
        self._rollback_created_resources(primary_error, guard)

    def _close_resources_after_error(
        self,
        primary_error: BaseException,
        resources: tuple[_WindowsResource, ...],
    ) -> None:
        for resource in resources:
            try:
                self.close_resource(resource)
            except BaseException as close_error:
                primary_error.add_note(
                    _bounded_evidence(
                        f"filesystem capability close failed: {close_error}"
                    )
                )
                self._failed_closes.append(resource)

    def _rollback_created_resources(
        self,
        primary_error: BaseException,
        resource: _WindowsResource,
    ) -> None:
        try:
            self._set_delete_disposition(resource)
        except BaseException as rollback_error:
            primary_error.add_note(
                _bounded_evidence(
                    f"created-object exact-handle rollback failed: {rollback_error}"
                )
            )
            self._close_resources_after_error(primary_error, (resource,))
            return
        resource.disposition_set = True
        if resource.parent is None or resource.name is None:
            raise RuntimeError("created rollback lacks parent/name evidence")
        state = _CreatedRollbackState(
            resource.parent,
            resource.name,
            primary_error,
            1,
        )
        resource.rollback_state = state
        self._close_resources_after_error(primary_error, (resource,))

    def _cleanup_relative_open_after_error(
        self,
        primary_error: BaseException,
        *,
        handle: int,
        information: int,
        parent: DirectoryCapability,
        name: str,
        desired_access: int,
        share_policy: SharePolicy,
    ) -> None:
        if information == FILE_CREATED:
            self._rollback_created_handle(
                primary_error,
                handle,
                parent,
                name,
                desired_access,
                _share_mode(share_policy),
            )
            return
        self._close_after_error(
            primary_error,
            handle,
            parent,
            name,
            bool(desired_access & DELETE),
        )

    @staticmethod
    def _creation_access(
        desired_access: int, allowed_information: set[int]
    ) -> int:
        if FILE_CREATED in allowed_information:
            return desired_access | DELETE
        return desired_access

    def _cleanup_relative_resources_after_error(
        self,
        primary_error: BaseException,
        *,
        resource: _WindowsResource,
        information: int,
    ) -> None:
        if information == FILE_CREATED:
            self._rollback_created_resources(primary_error, resource)
            return
        self._close_resources_after_error(primary_error, (resource,))

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
            self._cleanup_relative_open_after_error(
                primary_error,
                handle=handle,
                information=information,
                parent=parent,
                name=name,
                desired_access=desired_access,
                share_policy=share_policy,
            )
            raise

    def _managed_relative_open(
        self,
        *,
        parent: DirectoryCapability,
        name: str,
        desired_access: int,
        share_policy: SharePolicy,
        disposition: int,
        create_options: int,
        file_attributes: int,
        expected: DirectoryEntry | None,
        expected_kind: EntryKind,
        allowed_information: set[int],
        apply_creation_descriptor: bool = True,
        capture_name_collision: bool = False,
    ) -> tuple[int, _Metadata, bool]:
        actual_access = self._creation_access(
            desired_access, allowed_information
        )
        material = self._managed_security_material(
            directory=expected_kind is EntryKind.DIRECTORY
        )
        cleanup_material: _ManagedSecurityMaterial | None = material
        handle: int | None = None
        information = 0
        finished = False
        try:
            try:
                handle, information = self._native_relative_open(
                    parent,
                    name,
                    desired_access=actual_access,
                    share_mode=_share_mode(share_policy),
                    disposition=disposition,
                    create_options=create_options,
                    file_attributes=file_attributes,
                    run_hooks=True,
                    security_descriptor=(
                        material.descriptor
                        if apply_creation_descriptor
                        else None
                    ),
                )
            except OSError as error:
                if capture_name_collision and error.winerror in {
                    ERROR_FILE_EXISTS,
                    ERROR_ALREADY_EXISTS,
                }:
                    raise _OpenCollision(error) from error
                raise
            metadata, created = self._finish_relative_open(
                handle=handle,
                information=information,
                parent=parent,
                name=name,
                desired_access=actual_access,
                share_policy=share_policy,
                expected=expected,
                expected_kind=expected_kind,
                allowed_information=allowed_information,
            )
            finished = True
            resource = _WindowsResource(
                handle,
                parent,
                name,
                bool(actual_access & DELETE),
                actual_access,
                _share_mode(share_policy),
            )
            self._verify_managed_security_resource(
                resource,
                directory=expected_kind is EntryKind.DIRECTORY,
                component=name,
                repair_dacl=not created,
                material=material,
            )
            owner = cleanup_material
            cleanup_material = None
            assert owner is not None
            self._close_security_owner(owner)
            post_security = self._metadata(handle, name)
            if post_security.identity != metadata.identity:
                raise OSError(
                    "managed filesystem identity changed after security validation"
                )
            if post_security.filesystem != metadata.filesystem:
                raise OSError(
                    "managed filesystem changed after security validation"
                )
            if post_security.kind is not metadata.kind:
                raise OSError(
                    "managed filesystem kind changed after security validation"
                )
            post_entry = self._observe_required(parent, name)
            if not self._same_evidence(post_security, post_entry):
                raise OSError(
                    "managed filesystem parent/name identity changed after "
                    "security validation"
                )
            return handle, post_security, created
        except BaseException as primary_error:
            if finished and handle is not None:
                self._cleanup_relative_open_after_error(
                    primary_error,
                    handle=handle,
                    information=information,
                    parent=parent,
                    name=name,
                    desired_access=actual_access,
                    share_policy=share_policy,
                )
            if cleanup_material is not None:
                self._close_security_owner(cleanup_material, primary_error)
            raise

    def open_root(
        self,
        path: Path,
        share_policy: SharePolicy,
        security_domain: SecurityDomain = SecurityDomain.CALLER,
    ) -> DirectoryCapability:
        if not path.is_absolute():
            raise ValueError("Windows filesystem root must be absolute")
        desired_access = _directory_access(share_policy, relative_target=False)
        if security_domain is SecurityDomain.MANAGED:
            desired_access |= WRITE_DAC
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
            if metadata.kind is not EntryKind.DIRECTORY:
                raise NotADirectoryError(path)
            self._validate_metadata(
                metadata,
                parent=None,
                expected=None,
                expected_kind=EntryKind.DIRECTORY,
                operation="filesystem root",
            )
            if security_domain is SecurityDomain.MANAGED:
                self._verify_managed_security_resource(
                    _WindowsResource(
                        handle,
                        None,
                        str(path),
                        bool(desired_access & DELETE),
                        desired_access,
                        _share_mode(share_policy),
                    ),
                    directory=True,
                    component=path,
                    repair_dacl=True,
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
        _encode_windows_component(name)
        desired_access = _directory_access(share_policy, relative_target=True)
        allowed_information = {FILE_CREATED}
        actual_access = self._creation_access(
            desired_access, allowed_information
        )
        if parent.security_domain is SecurityDomain.MANAGED:
            handle, metadata, created = self._managed_relative_open(
                parent=parent,
                name=name,
                desired_access=desired_access,
                share_policy=share_policy,
                disposition=FILE_CREATE,
                create_options=FILE_OPEN_REPARSE_POINT | FILE_DIRECTORY_FILE,
                file_attributes=FILE_ATTRIBUTE_DIRECTORY,
                expected=None,
                expected_kind=EntryKind.DIRECTORY,
                allowed_information=allowed_information,
            )
        else:
            handle, information = self._native_relative_open(
                parent,
                name,
                desired_access=actual_access,
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
                desired_access=actual_access,
                share_policy=share_policy,
                expected=None,
                expected_kind=EntryKind.DIRECTORY,
                allowed_information=allowed_information,
            )
        information = FILE_CREATED if created else FILE_OPENED
        return self._relative_directory_capability(
            handle=handle,
            metadata=metadata,
            information=information,
            parent=parent,
            name=name,
            share_policy=share_policy,
            security_domain=parent.security_domain,
            created=created,
            path_hint=parent.path_hint / name,
            actual_access=actual_access,
            final_access=desired_access,
        )

    def _open_relative_file_once(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        desired_access: int,
        share_policy: SharePolicy,
        native_disposition: int,
        expected: DirectoryEntry | None,
        allowed_information: set[int],
        managed_operation: bool,
        apply_creation_descriptor: bool,
        capture_name_collision: bool = False,
    ) -> tuple[int, _Metadata, bool, int]:
        actual_access = self._creation_access(
            desired_access, allowed_information
        )
        if managed_operation:
            handle, metadata, created = self._managed_relative_open(
                parent=parent,
                name=name,
                desired_access=desired_access,
                share_policy=share_policy,
                disposition=native_disposition,
                create_options=FILE_OPEN_REPARSE_POINT | FILE_NON_DIRECTORY_FILE,
                file_attributes=FILE_ATTRIBUTE_NORMAL,
                expected=expected,
                expected_kind=EntryKind.REGULAR,
                allowed_information=allowed_information,
                apply_creation_descriptor=apply_creation_descriptor,
                capture_name_collision=capture_name_collision,
            )
        else:
            try:
                handle, information = self._native_relative_open(
                    parent,
                    name,
                    desired_access=actual_access,
                    share_mode=_share_mode(share_policy),
                    disposition=native_disposition,
                    create_options=(
                        FILE_OPEN_REPARSE_POINT | FILE_NON_DIRECTORY_FILE
                    ),
                    file_attributes=FILE_ATTRIBUTE_NORMAL,
                    run_hooks=True,
                )
            except OSError as error:
                if capture_name_collision and error.winerror in {
                    ERROR_FILE_EXISTS,
                    ERROR_ALREADY_EXISTS,
                }:
                    raise _OpenCollision(error) from error
                raise
            metadata, created = self._finish_relative_open(
                handle=handle,
                information=information,
                parent=parent,
                name=name,
                desired_access=actual_access,
                share_policy=share_policy,
                expected=expected,
                expected_kind=EntryKind.REGULAR,
                allowed_information=allowed_information,
            )
        return handle, metadata, created, actual_access

    @staticmethod
    def _raise_collision_cleanup_failure(collision: _OpenCollision) -> None:
        notes = getattr(collision, "__notes__", ())
        if not notes:
            return
        for note in notes:
            collision.error.add_note(note)
        raise collision.error

    def _open_or_create_file(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        desired_access: int,
        share_policy: SharePolicy,
    ) -> tuple[int, _Metadata, bool, int]:
        managed = parent.security_domain is SecurityDomain.MANAGED
        for _cycle in range(_OPEN_OR_CREATE_CYCLES):
            try:
                return self._open_relative_file_once(
                    parent,
                    name,
                    desired_access=desired_access,
                    share_policy=share_policy,
                    native_disposition=FILE_CREATE,
                    expected=None,
                    allowed_information={FILE_CREATED},
                    managed_operation=managed,
                    apply_creation_descriptor=True,
                    capture_name_collision=True,
                )
            except _OpenCollision as collision:
                self._raise_collision_cleanup_failure(collision)
            expected = self.entry(parent, name)
            if expected is None:
                continue
            failed_closes = len(self._failed_closes)
            failed_security_owners = len(self._failed_security_owners)
            try:
                return self._open_relative_file_once(
                    parent,
                    name,
                    desired_access=desired_access,
                    share_policy=share_policy,
                    native_disposition=FILE_OPEN,
                    expected=expected,
                    allowed_information={FILE_OPENED},
                    managed_operation=managed,
                    apply_creation_descriptor=False,
                )
            except FileNotFoundError as error:
                if (
                    error.winerror != ERROR_FILE_NOT_FOUND
                    or len(self._failed_closes) != failed_closes
                    or len(self._failed_security_owners)
                    != failed_security_owners
                ):
                    raise
        raise OSError("open-or-create entry did not stabilize")

    def open_file(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        access: FileAccess,
        disposition: CreateDisposition,
        share_policy: SharePolicy = SharePolicy.MUTATION,
    ) -> FileCapability:
        _encode_windows_component(name)
        desired_access = _file_access(access, share_policy)
        managed = parent.security_domain is SecurityDomain.MANAGED
        if disposition is CreateDisposition.OPEN_EXISTING:
            handle, metadata, created, actual_access = (
                self._open_relative_file_once(
                    parent,
                    name,
                    desired_access=desired_access,
                    share_policy=share_policy,
                    native_disposition=FILE_OPEN,
                    expected=self._observe_required(parent, name),
                    allowed_information={FILE_OPENED},
                    managed_operation=False,
                    apply_creation_descriptor=False,
                )
            )
        elif disposition is CreateDisposition.CREATE_NEW:
            handle, metadata, created, actual_access = (
                self._open_relative_file_once(
                    parent,
                    name,
                    desired_access=desired_access,
                    share_policy=share_policy,
                    native_disposition=FILE_CREATE,
                    expected=None,
                    allowed_information={FILE_CREATED},
                    managed_operation=managed,
                    apply_creation_descriptor=True,
                )
            )
        else:
            if managed:
                desired_access |= WRITE_DAC
            handle, metadata, created, actual_access = (
                self._open_or_create_file(
                    parent,
                    name,
                    desired_access=desired_access,
                    share_policy=share_policy,
                )
            )
        information = FILE_CREATED if created else FILE_OPENED
        return self._relative_file_capability(
            handle=handle,
            metadata=metadata,
            information=information,
            parent=parent,
            name=name,
            share_policy=share_policy,
            created=created,
            actual_access=actual_access,
            final_access=desired_access,
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
        source = self._resource(directory)
        requested_access = _directory_access(
            effective_policy, relative_target=source.parent is not None
        )
        requested_share_mode = _share_mode(effective_policy)
        if requested_share_mode != source.share_mode:
            raise ValueError("duplicate handle cannot change its share mode")
        if requested_access & ~source.desired_access:
            raise ValueError("duplicate handle cannot widen its authority")
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
                desired_access=source.desired_access,
                share_mode=source.share_mode,
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
        _encode_windows_component(name)
        desired_access = (
            _directory_access(SharePolicy.MUTATION, relative_target=True)
            | READ_CONTROL
            | WRITE_DAC
        )
        allowed_information = {FILE_OPENED, FILE_CREATED}
        actual_access = self._creation_access(
            desired_access, allowed_information
        )
        handle, metadata, created = self._managed_relative_open(
            parent=parent,
            name=name,
            desired_access=desired_access,
            share_policy=SharePolicy.MUTATION,
            disposition=FILE_OPEN_IF,
            create_options=FILE_OPEN_REPARSE_POINT | FILE_DIRECTORY_FILE,
            file_attributes=FILE_ATTRIBUTE_DIRECTORY,
            expected=self.entry(parent, name),
            expected_kind=EntryKind.DIRECTORY,
            allowed_information=allowed_information,
        )
        return self._relative_directory_capability(
            handle=handle,
            metadata=metadata,
            information=FILE_CREATED if created else FILE_OPENED,
            parent=parent,
            name=name,
            share_policy=SharePolicy.MUTATION,
            security_domain=SecurityDomain.MANAGED,
            created=created,
            path_hint=parent.path_hint / name,
            actual_access=actual_access,
            final_access=desired_access,
            secure_root_creation=created,
        )

    def _prepare_secure_root_commit(
        self, directory: DirectoryCapability
    ) -> None:
        resource = self._resource(directory)
        if (
            directory.kind is not EntryKind.DIRECTORY
            or directory.security_domain is not SecurityDomain.MANAGED
            or directory.share_policy is not SharePolicy.MUTATION
            or resource.parent is None
            or resource.name is None
        ):
            raise RuntimeError("secure root commit requires its relative capability")
        if not directory.created:
            if resource.secure_root_creation:
                raise RuntimeError("existing secure root has creation provenance")
            return
        if not resource.secure_root_creation:
            raise RuntimeError("created secure root has no rollback provenance")

    def _commit_secure_root(self, directory: DirectoryCapability) -> None:
        resource = cast(_WindowsResource, directory._resource)
        resource.secure_root_creation = False

    def entries(self, parent: DirectoryCapability) -> DirectoryIterator:
        reopened = self.reopen_directory(parent)
        try:
            return self.entries_owned(reopened)
        except BaseException as primary_error:
            _add_close_note(primary_error, reopened.close)
            raise

    def entries_owned(self, parent: DirectoryCapability) -> DirectoryIterator:
        return _WindowsEntries(self, parent)

    def _final_path_string(
        self, directory: DirectoryCapability, volume_name: int
    ) -> str:
        resource = self._resource(directory)
        capacity = _FINAL_PATH_INITIAL_UNITS
        while True:
            buffer = (WCHAR * capacity)()
            result = int(
                self._api.GetFinalPathNameByHandleW(
                    resource.handle, buffer, capacity, volume_name
                )
            )
            if result == 0:
                self._raise_last_error(
                    "recover final directory path", directory.path_hint
                )
            if result >= capacity:
                if result <= capacity:
                    raise OSError("final path buffer did not make progress")
                if result > _FINAL_PATH_MAX_UNITS:
                    raise OSError("final path exceeds the bounded buffer")
                capacity = result
                continue
            try:
                path = ctypes.string_at(
                    buffer, result * ctypes.sizeof(WCHAR)
                ).decode("utf-16-le", errors="strict")
            except UnicodeDecodeError as error:
                raise OSError("final path is invalid UTF-16") from error
            if not path or "\0" in path:
                raise OSError("final path is empty or malformed")
            return path

    @staticmethod
    def _is_absolute_dos_path(path: str) -> bool:
        if "/" in path or "\0" in path:
            return False
        if len(path) >= 7 and path.startswith("\\\\?\\"):
            drive = path[4:7]
            if (
                drive[0].isascii()
                and drive[0].isalpha()
                and drive[1:] == ":\\"
            ):
                return True
        unc_prefix = "\\\\?\\UNC\\"
        if path.startswith(unc_prefix):
            components = path[len(unc_prefix) :].split("\\")
            return len(components) >= 2 and bool(components[0]) and bool(
                components[1]
            )
        return False

    def _volume_paths(
        self, directory: DirectoryCapability
    ) -> tuple[str, str]:
        volume_path = self._final_path_string(directory, VOLUME_NAME_GUID)
        match = _VOLUME_GUID_ROOT.match(volume_path)
        if match is None or "/" in volume_path or "\0" in volume_path:
            raise OSError("final path is not an absolute volume-GUID path")
        expected_root = match.group(0)
        volume_root = self._api.get_volume_path(volume_path)
        if volume_root != expected_root or not volume_root.endswith("\\"):
            raise OSError("filesystem volume root does not match the GUID path")
        return volume_path, volume_root

    def _verify_path_volume(
        self, volume_root: str, expected: FilesystemIdentity
    ) -> None:
        serial, maximum_component_length, filesystem_flags = (
            self._api.get_volume_information(volume_root)
        )
        if (
            serial != expected.volume & 0xFFFF_FFFF
            or maximum_component_length != expected.discriminator_a
            or filesystem_flags != expected.discriminator_b
        ):
            raise OSError("path filesystem identity changed")

    def _capacity_handle_metadata(
        self, directory: DirectoryCapability
    ) -> _Metadata:
        resource = self._resource(directory)
        metadata = self._metadata(resource.handle, directory.path_hint)
        if (
            metadata.identity != directory.identity
            or metadata.filesystem != directory.filesystem
            or metadata.kind is not EntryKind.DIRECTORY
        ):
            raise OSError("capacity root identity changed")
        return metadata

    @staticmethod
    def _metadata_matches_capability(
        metadata: _Metadata,
        capability: FileCapability | DirectoryCapability,
    ) -> bool:
        return (
            metadata.identity == capability.identity
            and metadata.filesystem == capability.filesystem
            and metadata.kind is capability.kind
        )

    def _relative_mutation_resource(
        self,
        capability: FileCapability | DirectoryCapability,
        operation: str,
        *,
        allow_created_secure_root: bool = False,
    ) -> tuple[_WindowsResource, DirectoryCapability, str]:
        resource = self._resource(capability)
        created_secure_root = (
            allow_created_secure_root
            and isinstance(capability, DirectoryCapability)
            and capability.created
            and capability.security_domain is SecurityDomain.MANAGED
            and capability.share_policy is SharePolicy.MUTATION
            and resource.secure_root_creation
        )
        if (
            capability.share_policy is not SharePolicy.PINNED
            and not created_secure_root
        ):
            raise RuntimeError(f"Windows {operation} requires a PINNED capability")
        if not resource.delete_authority:
            raise RuntimeError(f"Windows {operation} requires native DELETE authority")
        if resource.parent is None or resource.name is None:
            raise RuntimeError(f"Windows {operation} requires a relative capability")
        parent = resource.parent
        self._resource(parent)
        if capability.filesystem != parent.filesystem:
            raise OSError(f"Windows {operation} source crosses a filesystem boundary")
        return resource, parent, resource.name

    def _revalidate_mutation_source(
        self,
        capability: FileCapability | DirectoryCapability,
        resource: _WindowsResource,
        parent: DirectoryCapability,
        name: str,
        operation: str,
    ) -> _Metadata:
        metadata = self._metadata(resource.handle, capability.path_hint)
        if not self._metadata_matches_capability(metadata, capability):
            raise OSError(f"Windows {operation} source handle identity changed")
        observed = self.entry(parent, name)
        if observed is None or not self._same_evidence(metadata, observed):
            raise OSError(f"Windows {operation} source entry identity changed")
        return metadata

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        encoded_name = _encode_windows_component(destination_name)
        resource, source_parent, source_name = self._relative_mutation_resource(
            source, "rename"
        )
        destination_resource = self._resource(destination_parent)
        if source.filesystem != destination_parent.filesystem:
            raise OSError("Windows rename destination crosses a filesystem boundary")
        before = self._revalidate_mutation_source(
            source,
            resource,
            source_parent,
            source_name,
            "rename",
        )

        allocation_size = FILE_RENAME_INFORMATION.FileName.offset + len(encoded_name)
        buffer_size = max(
            allocation_size, ctypes.sizeof(FILE_RENAME_INFORMATION)
        )
        allocation = ctypes.create_string_buffer(buffer_size)
        information = FILE_RENAME_INFORMATION.from_buffer(allocation)
        information.ReplaceIfExists = int(replace)
        information.RootDirectory = destination_resource.handle
        information.FileNameLength = len(encoded_name)
        ctypes.memmove(
            ctypes.addressof(allocation) + FILE_RENAME_INFORMATION.FileName.offset,
            encoded_name,
            len(encoded_name),
        )
        io_status = IO_STATUS_BLOCK()
        status = int(
            self._api.NtSetInformationFile(
                resource.handle,
                ctypes.byref(io_status),
                allocation,
                buffer_size,
                FILE_RENAME_INFORMATION_CLASS,
            )
        )
        if status < 0:
            code = int(self._api.RtlNtStatusToDosError(status))
            raise _error_from_win32(
                code, "rename exact filesystem capability", destination_name
            )

        after = self._metadata(resource.handle, source.path_hint)
        if (
            not self._metadata_matches_capability(after, source)
            or after.identity != before.identity
            or after.filesystem != before.filesystem
        ):
            raise OSError("Windows rename source handle identity changed after mutation")
        destination = self.entry(destination_parent, destination_name)
        if destination is None or not self._same_evidence(after, destination):
            raise OSError("Windows rename destination identity is ambiguous")
        same_entry = (
            source_parent.identity == destination_parent.identity
            and source_parent.filesystem == destination_parent.filesystem
            and source_name == destination_name
        )
        if not same_entry and self.entry(source_parent, source_name) is not None:
            raise OSError("Windows rename old source entry remains present")

        resource.parent = destination_parent
        resource.name = destination_name
        source._path_hint = destination_parent.path_hint / destination_name

    def delete(self, capability: FileCapability | DirectoryCapability) -> None:
        resource, parent, name = self._relative_mutation_resource(
            capability, "delete", allow_created_secure_root=True
        )
        if not resource.disposition_set:
            self._revalidate_mutation_source(
                capability,
                resource,
                parent,
                name,
                "delete",
            )
            self._set_delete_disposition(resource)
            resource.disposition_set = True
        capability.close()
        remaining = self.entry(parent, name)
        if remaining is not None:
            raise OSError(
                "Windows delete found the original or a same-name replacement"
            )

    def available_bytes(self, directory: DirectoryCapability) -> int:
        before = self._capacity_handle_metadata(directory)
        volume_path, volume_root = self._volume_paths(directory)
        self._verify_path_volume(volume_root, before.filesystem)
        available = self._api.get_disk_free_space_ex(volume_path)
        self._verify_path_volume(volume_root, before.filesystem)
        after = self._capacity_handle_metadata(directory)
        if (
            before.identity != after.identity
            or before.filesystem != after.filesystem
        ):
            raise OSError("capacity root identity changed")
        if available < 0:
            raise OSError("filesystem reported negative available bytes")
        return available

    def allocation_unit(self, directory: DirectoryCapability) -> int:
        before = self._capacity_handle_metadata(directory)
        _volume_path, volume_root = self._volume_paths(directory)
        self._verify_path_volume(volume_root, before.filesystem)
        sectors_per_cluster, bytes_per_sector = (
            self._api.get_disk_free_space(volume_root)
        )
        self._verify_path_volume(volume_root, before.filesystem)
        after = self._capacity_handle_metadata(directory)
        if (
            before.identity != after.identity
            or before.filesystem != after.filesystem
        ):
            raise OSError("capacity root identity changed")
        if sectors_per_cluster <= 0 or bytes_per_sector <= 0:
            raise OSError("filesystem reported an invalid allocation unit")
        if sectors_per_cluster > _MAX_UINT64 // bytes_per_sector:
            raise OSError("filesystem allocation unit overflows 64 bits")
        return sectors_per_cluster * bytes_per_sector

    def touch(self, file: FileCapability) -> None:
        if not isinstance(file, FileCapability) or file.kind is not EntryKind.REGULAR:
            raise RuntimeError("Windows touch requires a regular file capability")
        resource = self._resource(file)
        before = self._metadata(resource.handle, file.path_hint)
        if not self._metadata_matches_capability(before, file):
            raise OSError("Windows touch file identity changed before mutation")
        now = FILETIME()
        self._api.GetSystemTimeAsFileTime(ctypes.byref(now))
        if not self._api.SetFileTime(
            resource.handle,
            None,
            None,
            ctypes.byref(now),
        ):
            self._raise_last_error("update file last-write time", file.path_hint)
        after = self._metadata(resource.handle, file.path_hint)
        if (
            not self._metadata_matches_capability(after, file)
            or after.identity != before.identity
            or after.filesystem != before.filesystem
        ):
            raise OSError("Windows touch file identity changed after mutation")

    def flush(self, file: FileCapability) -> None:
        if not isinstance(file, FileCapability) or file.kind is not EntryKind.REGULAR:
            raise RuntimeError("Windows flush requires a regular file capability")
        resource = self._resource(file)
        before = self._metadata(resource.handle, file.path_hint)
        if not self._metadata_matches_capability(before, file):
            raise OSError("Windows flush file identity changed before mutation")
        if not self._api.FlushFileBuffers(resource.handle):
            self._raise_last_error("flush file capability", file.path_hint)
        after = self._metadata(resource.handle, file.path_hint)
        if (
            not self._metadata_matches_capability(after, file)
            or after.identity != before.identity
            or after.filesystem != before.filesystem
        ):
            raise OSError("Windows flush file identity changed after mutation")

    def final_path(self, directory: DirectoryCapability) -> Path:
        path = self._final_path_string(directory, VOLUME_NAME_DOS)
        if not self._is_absolute_dos_path(path):
            raise OSError("final path is not an absolute DOS path")
        return Path(path)

    def verify_managed_security(
        self,
        capability: FileCapability | DirectoryCapability,
        *,
        repair_dacl: bool,
    ) -> None:
        if capability.security_domain is not SecurityDomain.MANAGED:
            raise PermissionError(
                "caller-domain capability is not managed protocol state"
            )
        resource = self._resource(capability)
        self._verify_managed_security_resource(
            resource,
            directory=capability.kind is EntryKind.DIRECTORY,
            component=capability.path_hint,
            repair_dacl=repair_dacl,
        )

    def __del__(self) -> None:
        for resource in getattr(self, "_failed_closes", ()):
            try:
                self.close_resource(resource)
            except (OSError, RuntimeError):
                pass
        for owner in getattr(self, "_failed_security_owners", ()):
            try:
                owner.close()
            except (OSError, RuntimeError):
                pass
