from __future__ import annotations

import errno
import ctypes
import gc
import os
import struct
import subprocess
import tempfile
import types
import unittest
from pathlib import Path
from typing import Any, cast
from unittest import mock

from tools.focused_mutation_support.filesystem import (
    CreateDisposition,
    DirectoryCapability,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemIdentity,
    SecurityDomain,
    SharePolicy,
)
from tools.focused_mutation_support import windows_filesystem as windows_native
from tools.focused_mutation_support.windows_filesystem import (
    DELETE,
    FILE_ID_EXTD_DIR_INFO,
    FILE_ID_INFO,
    FILE_RENAME_INFORMATION,
    FILE_SHARE_DELETE,
    FILE_SHARE_READ,
    FILE_SHARE_WRITE,
    INVALID_OWNED_HANDLE,
    IO_STATUS_BLOCK,
    OBJECT_ATTRIBUTES,
    UNICODE_STRING,
    WindowsFilesystemBackend,
    _WindowsApi,
    _WindowsResource,
    _directory_access,
    _encode_windows_component,
    _entry_access,
    _error_from_win32,
    _file_access,
    _share_mode,
)


def _align_directory_record(value: int) -> int:
    return (value + 7) & ~7


def _directory_record(
    name: str,
    *,
    file_id: int = 7,
    next_offset: int = 0,
    record_size: int | None = None,
    attributes: int = windows_native.FILE_ATTRIBUTE_NORMAL,
    reparse_tag: int = 0,
    logical_size: int = 11,
    modified_100ns: int = 116_444_736_000_000_123,
) -> bytearray:
    encoded_name = name.encode("utf-16-le", errors="strict")
    minimum_size = FILE_ID_EXTD_DIR_INFO.FileName.offset + len(encoded_name)
    if record_size is None:
        record_size = _align_directory_record(minimum_size)
    encoded = bytearray(record_size)
    struct.pack_into(
        "<IIqqqqqqIIII",
        encoded,
        0,
        next_offset,
        0,
        0,
        0,
        modified_100ns,
        0,
        logical_size,
        logical_size,
        attributes,
        len(encoded_name),
        0,
        reparse_tag,
    )
    encoded[72:88] = file_id.to_bytes(16, "little")
    encoded[88 : 88 + len(encoded_name)] = encoded_name
    return encoded


class WindowsTask4LayoutTests(unittest.TestCase):
    def test_extended_directory_layout_uses_documented_fixed_offsets(self) -> None:
        import ctypes

        self.assertEqual(ctypes.sizeof(windows_native.FILE_ID_128), 16)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.NextEntryOffset.offset, 0)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.LastWriteTime.offset, 24)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.EndOfFile.offset, 40)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.FileAttributes.offset, 56)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.FileNameLength.offset, 60)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.ReparsePointTag.offset, 68)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.FileId.offset, 72)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.FileName.offset, 88)

    def test_enumeration_constants_preserve_bounded_restart_contract(self) -> None:
        self.assertEqual(windows_native._DIRECTORY_BUFFER_BYTES, 64 * 1024)
        self.assertEqual(windows_native.FILE_ID_EXTD_DIRECTORY_INFO_CLASS, 19)
        self.assertEqual(
            windows_native.FILE_ID_EXTD_DIRECTORY_RESTART_INFO_CLASS, 20
        )
        self.assertEqual(windows_native.ERROR_NO_MORE_FILES, 18)


class WindowsDirectoryRecordTests(unittest.TestCase):
    filesystem = FilesystemIdentity(0x1234_5678_9ABC_DEF0, 255, 0x4006)

    def _parser(self, encoded: bytes | bytearray) -> Any:
        return windows_native._DirectoryRecordParser(encoded, self.filesystem)

    def test_parser_yields_two_records_lazily_with_exact_metadata(self) -> None:
        first_size = _align_directory_record(88 + len("alpha".encode("utf-16-le")))
        encoded = _directory_record(
            "alpha",
            file_id=7,
            next_offset=first_size,
            record_size=first_size,
            attributes=windows_native.FILE_ATTRIBUTE_DIRECTORY,
            logical_size=0,
        )
        encoded += _directory_record("beta", file_id=9, logical_size=17)
        parser = self._parser(encoded)

        first = parser.next_record()
        second = parser.next_record()

        assert first is not None
        assert second is not None
        self.assertEqual(first.name, "alpha")
        self.assertIs(first.kind, EntryKind.DIRECTORY)
        self.assertEqual(first.identity, FileIdentity(self.filesystem.volume, 7))
        self.assertEqual(first.filesystem, self.filesystem)
        self.assertEqual(first.logical_size, 0)
        self.assertEqual(first.modified_ns, 12_300)
        self.assertEqual(second.name, "beta")
        self.assertIs(second.kind, EntryKind.REGULAR)
        self.assertEqual(second.identity, FileIdentity(self.filesystem.volume, 9))
        self.assertEqual(second.logical_size, 17)
        self.assertIsNone(parser.next_record())

    def test_parser_does_not_decode_a_later_record_eagerly(self) -> None:
        first_size = _align_directory_record(88 + len("first".encode("utf-16-le")))
        encoded = _directory_record(
            "first", next_offset=first_size, record_size=first_size
        )
        invalid = _directory_record("x", file_id=8)
        invalid[88:90] = b"\x00\xd8"
        encoded += invalid
        parser = self._parser(encoded)

        first = parser.next_record()

        assert first is not None
        self.assertEqual(first.name, "first")
        with self.assertRaisesRegex(OSError, "UTF-16"):
            parser.next_record()

    def test_parser_filters_only_dot_entries_after_strict_decode(self) -> None:
        dot_size = _align_directory_record(90)
        dotdot_size = _align_directory_record(92)
        encoded = _directory_record(
            ".", file_id=1, next_offset=dot_size, record_size=dot_size
        )
        encoded += _directory_record(
            "..", file_id=2, next_offset=dotdot_size, record_size=dotdot_size
        )
        encoded += _directory_record("kept", file_id=3)

        parser = self._parser(encoded)

        record = parser.next_record()
        assert record is not None
        self.assertEqual(record.name, "kept")
        self.assertIsNone(parser.next_record())

    def test_parser_rejects_truncated_header_before_field_access(self) -> None:
        with self.assertRaisesRegex(OSError, "header"):
            self._parser(bytes(87)).next_record()

    def test_parser_rejects_invalid_offset_boundaries_and_progress(self) -> None:
        minimum = _align_directory_record(88 + len("alpha".encode("utf-16-le")))
        cases = {
            "too-small": _directory_record("alpha", next_offset=8),
            "misaligned": _directory_record("alpha", next_offset=minimum + 1),
            "wraps-backward": _directory_record(
                "alpha", next_offset=0xFFFF_FFF8
            ),
            "out-of-buffer": _directory_record("alpha", next_offset=4_096),
            "missing-final-record": _directory_record(
                "alpha", next_offset=minimum, record_size=minimum
            ),
        }
        premature = _directory_record("alpha", next_offset=0)
        premature += _directory_record("hidden", file_id=8)
        cases["self-terminates-before-final-record"] = premature

        for label, encoded in cases.items():
            with self.subTest(label=label), self.assertRaises(OSError):
                self._parser(encoded).next_record()

    def test_parser_rejects_invalid_name_lengths_bounds_and_encoding(self) -> None:
        odd = _directory_record("x")
        struct.pack_into("<I", odd, 60, 1)
        outside = _directory_record("x")
        struct.pack_into("<I", outside, 60, len(outside))
        invalid_utf16 = _directory_record("x")
        invalid_utf16[88:90] = b"\x00\xd8"

        for label, encoded in (
            ("odd", odd),
            ("outside", outside),
            ("invalid-utf16", invalid_utf16),
        ):
            with self.subTest(label=label), self.assertRaises(OSError):
                self._parser(encoded).next_record()

    def test_parser_rejects_zero_identity_or_parent_volume(self) -> None:
        with self.assertRaisesRegex(OSError, "file identity"):
            self._parser(_directory_record("zero", file_id=0)).next_record()
        with self.assertRaisesRegex(OSError, "volume"):
            windows_native._DirectoryRecordParser(
                _directory_record("item"), FilesystemIdentity(0)
            ).next_record()

    def test_parser_rejects_negative_size_and_raw_filetime(self) -> None:
        for label, encoded in (
            ("size", _directory_record("item", logical_size=-1)),
            ("time", _directory_record("item", modified_100ns=-1)),
        ):
            with self.subTest(label=label), self.assertRaises(OSError):
                self._parser(encoded).next_record()

    def test_parser_rejects_inconsistent_reparse_attributes_and_tag(self) -> None:
        for label, encoded in (
            (
                "missing-tag",
                _directory_record(
                    "link",
                    attributes=windows_native.FILE_ATTRIBUTE_REPARSE_POINT,
                    reparse_tag=0,
                ),
            ),
            (
                "unexpected-tag",
                _directory_record(
                    "file",
                    attributes=windows_native.FILE_ATTRIBUTE_NORMAL,
                    reparse_tag=0xA000_0003,
                ),
            ),
        ):
            with self.subTest(label=label), self.assertRaises(OSError):
                self._parser(encoded).next_record()

    def test_parser_accepts_exact_header_boundary_records(self) -> None:
        single = self._parser(
            _directory_record("", file_id=17, record_size=88)
        )
        record = single.next_record()
        assert record is not None
        self.assertEqual(record.name, "")
        self.assertIsNone(single.next_record())

        encoded = _directory_record(
            "", file_id=19, next_offset=88, record_size=88
        )
        encoded += _directory_record("", file_id=23, record_size=88)
        pair = self._parser(encoded)
        first = pair.next_record()
        second = pair.next_record()
        assert first is not None
        assert second is not None
        self.assertEqual(first.identity.file, 19)
        self.assertEqual(second.identity.file, 23)
        self.assertIsNone(pair.next_record())

    def test_parser_accepts_zero_raw_filetime(self) -> None:
        parser = self._parser(
            _directory_record("epoch", modified_100ns=0)
        )

        record = parser.next_record()

        assert record is not None
        self.assertEqual(record.modified_ns, -11_644_473_600_000_000_000)
        self.assertIsNone(parser.next_record())


class _DirectoryEnumerationApi:
    def __init__(
        self,
        responses: list[bytes | bytearray | int],
        *,
        close_outcomes: list[bool] | None = None,
    ) -> None:
        self.responses = responses
        self.close_outcomes = close_outcomes or [True]
        self.enumeration_calls: list[tuple[int, int, int]] = []
        self.closed: list[int] = []
        self._last_error = 0

    def GetFileInformationByHandleEx(
        self,
        handle: int,
        information_class: int,
        buffer: Any,
        buffer_size: int,
    ) -> bool:
        import ctypes

        self.enumeration_calls.append((handle, information_class, buffer_size))
        response = self.responses.pop(0)
        if isinstance(response, int):
            self._last_error = response
            return False
        if len(response) > buffer_size:
            raise AssertionError("test response exceeds native buffer")
        ctypes.memset(buffer, 0, buffer_size)
        ctypes.memmove(buffer, bytes(response), len(response))
        return True

    def CloseHandle(self, handle: int) -> bool:
        self.closed.append(handle)
        return self.close_outcomes.pop(0)

    def last_error(self) -> int:
        return self._last_error or 6


def _directory_capability(
    backend: WindowsFilesystemBackend,
    *,
    handle: int = 71,
    filesystem: FilesystemIdentity = FilesystemIdentity(11, 255, 0x4006),
) -> DirectoryCapability:
    return DirectoryCapability(
        backend,
        _WindowsResource(handle, None, None, False),
        identity=FileIdentity(filesystem.volume, 13),
        filesystem=filesystem,
        kind=EntryKind.DIRECTORY,
        logical_size=0,
        modified_ns=0,
        security_domain=SecurityDomain.CALLER,
        share_policy=SharePolicy.SCAN,
        created=False,
        path_hint=Path("C:/root"),
    )


class WindowsIteratorOwnershipTests(unittest.TestCase):
    def test_partially_initialized_iterator_finalizer_is_safe(self) -> None:
        entries = windows_native._WindowsEntries.__new__(
            windows_native._WindowsEntries
        )

        entries.__del__()

    def test_entries_owned_moves_one_handle_and_borrows_replacement(self) -> None:
        api = _DirectoryEnumerationApi([windows_native.ERROR_NO_MORE_FILES])
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        source = _directory_capability(backend)
        source_resource = backend._resource(source)

        entries = backend.entries_owned(source)

        self.assertTrue(source.transferred)
        self.assertTrue(entries.directory.is_open)
        self.assertIsNot(entries.directory, source)
        self.assertIs(backend._resource(entries.directory), source_resource)
        entries.close()
        self.assertEqual(api.closed, [71])

    def test_entries_owned_constructor_failure_leaves_source_open(self) -> None:
        source_api = _DirectoryEnumerationApi([])
        target_api = _DirectoryEnumerationApi([])
        source_backend = WindowsFilesystemBackend(
            api=source_api, osfhandle_opener=lambda _handle, _flags: 0
        )
        target_backend = WindowsFilesystemBackend(
            api=target_api, osfhandle_opener=lambda _handle, _flags: 0
        )
        source = _directory_capability(source_backend)

        with self.assertRaisesRegex(RuntimeError, "another backend"):
            target_backend.entries_owned(source)

        self.assertTrue(source.is_open)
        source.close()
        self.assertEqual(source_api.closed, [71])

    def test_first_refill_restarts_then_continues_with_fixed_buffer(self) -> None:
        filesystem = FilesystemIdentity(11, 255, 0x4006)
        api = _DirectoryEnumerationApi(
            [
                _directory_record("first"),
                _directory_record("second", file_id=8),
                windows_native.ERROR_NO_MORE_FILES,
            ]
        )
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        entries = backend.entries_owned(
            _directory_capability(backend, filesystem=filesystem)
        )

        self.assertEqual(next(entries).name, "first")
        self.assertEqual(next(entries).name, "second")
        with self.assertRaises(StopIteration):
            next(entries)

        self.assertEqual(
            api.enumeration_calls,
            [
                (
                    71,
                    windows_native.FILE_ID_EXTD_DIRECTORY_RESTART_INFO_CLASS,
                    64 * 1024,
                ),
                (
                    71,
                    windows_native.FILE_ID_EXTD_DIRECTORY_INFO_CLASS,
                    64 * 1024,
                ),
                (
                    71,
                    windows_native.FILE_ID_EXTD_DIRECTORY_INFO_CLASS,
                    64 * 1024,
                ),
            ],
        )
        self.assertEqual(api.closed, [71])

    def test_parser_failure_closes_once_and_keeps_close_failure_secondary(self) -> None:
        malformed = _directory_record("x")
        struct.pack_into("<I", malformed, 60, 1)
        api = _DirectoryEnumerationApi(
            [malformed], close_outcomes=[False, True]
        )
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        entries = backend.entries_owned(_directory_capability(backend))

        with self.assertRaisesRegex(OSError, "odd UTF-16") as raised:
            next(entries)

        notes = list(getattr(raised.exception, "__notes__", ()))
        self.assertEqual(len(notes), 1)
        self.assertIn("close failed", notes[0])
        self.assertLessEqual(len(notes[0].encode("utf-8")), 4_096)
        self.assertTrue(entries.directory.is_open)
        entries.close()
        self.assertEqual(api.closed, [71, 71])

    def test_refill_failure_is_not_exhaustion_and_closes_once(self) -> None:
        api = _DirectoryEnumerationApi([5])
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        entries = backend.entries_owned(_directory_capability(backend))

        with self.assertRaises(PermissionError):
            next(entries)

        self.assertEqual(api.closed, [71])
        self.assertTrue(entries.directory.closed)

    def test_explicit_close_failure_remains_retryable_and_idempotent(self) -> None:
        api = _DirectoryEnumerationApi([], close_outcomes=[False, True])
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        entries = backend.entries_owned(_directory_capability(backend))

        with self.assertRaises(OSError):
            entries.close()
        self.assertTrue(entries.directory.is_open)
        entries.close()
        entries.close()

        self.assertEqual(api.closed, [71, 71])
        self.assertTrue(entries.directory.closed)

    def test_finalizer_closes_the_moved_capability_once(self) -> None:
        api = _DirectoryEnumerationApi([])
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        source = _directory_capability(backend)
        entries = backend.entries_owned(source)

        del entries
        gc.collect()

        self.assertTrue(source.transferred)
        self.assertEqual(api.closed, [71])


class _DuplicateApi:
    def __init__(self) -> None:
        self.duplicates = 0
        self.closed: list[int] = []

    def GetCurrentProcess(self) -> int:
        return 1

    def DuplicateHandle(self, *args: object) -> bool:
        import ctypes

        self.duplicates += 1
        result = ctypes.cast(
            cast(Any, args[3]), ctypes.POINTER(ctypes.c_void_p)
        )
        result.contents.value = 100 + self.duplicates
        return True

    def CloseHandle(self, handle: int) -> bool:
        self.closed.append(handle)
        return True

    def last_error(self) -> int:
        return 6


class _ReopenBackend(WindowsFilesystemBackend):
    def __init__(self, api: _DuplicateApi, metadata: Any) -> None:
        super().__init__(api=api, osfhandle_opener=lambda _handle, _flags: 0)
        self._fixed_metadata = metadata

    def _metadata(self, handle: int, component: object) -> Any:
        return self._fixed_metadata


class WindowsReopenPolicyTests(unittest.TestCase):
    def _source(
        self,
        backend: WindowsFilesystemBackend,
        policy: SharePolicy,
        desired_access: int,
        share_mode: int,
    ) -> DirectoryCapability:
        filesystem = FilesystemIdentity(17, 255, 0x4006)
        return DirectoryCapability(
            backend,
            _WindowsResource(
                81,
                None,
                None,
                bool(desired_access & DELETE),
                desired_access=desired_access,
                share_mode=share_mode,
            ),
            identity=FileIdentity(17, 19),
            filesystem=filesystem,
            kind=EntryKind.DIRECTORY,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.CALLER,
            share_policy=policy,
            created=True,
            path_hint=Path("C:/root"),
        )

    def test_reopen_policy_matrix_uses_actual_authority_and_share_profiles(
        self,
    ) -> None:
        common = FILE_SHARE_READ | FILE_SHARE_WRITE
        shared = common | FILE_SHARE_DELETE
        profiles = (
            (
                SharePolicy.SCAN,
                _directory_access(SharePolicy.SCAN, relative_target=False),
                shared,
            ),
            (
                SharePolicy.PINNED,
                _directory_access(SharePolicy.PINNED, relative_target=False),
                common,
            ),
            (
                SharePolicy.MUTATION,
                _directory_access(SharePolicy.MUTATION, relative_target=False),
                shared,
            ),
        )
        expected = {
            (SharePolicy.SCAN, SharePolicy.SCAN): True,
            (SharePolicy.SCAN, SharePolicy.PINNED): False,
            (SharePolicy.SCAN, SharePolicy.MUTATION): False,
            (SharePolicy.PINNED, SharePolicy.SCAN): False,
            (SharePolicy.PINNED, SharePolicy.PINNED): True,
            (SharePolicy.PINNED, SharePolicy.MUTATION): False,
            (SharePolicy.MUTATION, SharePolicy.SCAN): True,
            (SharePolicy.MUTATION, SharePolicy.PINNED): False,
            (SharePolicy.MUTATION, SharePolicy.MUTATION): True,
        }
        for source_policy, actual_access, actual_share in profiles:
            for requested_policy in SharePolicy:
                api = _DuplicateApi()
                metadata = windows_native._Metadata(
                    FileIdentity(17, 19),
                    FilesystemIdentity(17, 255, 0x4006),
                    EntryKind.DIRECTORY,
                    0,
                    0,
                )
                backend = _ReopenBackend(api, metadata)
                source = self._source(
                    backend, source_policy, actual_access, actual_share
                )
                allowed = expected[(source_policy, requested_policy)]
                with self.subTest(
                    source=source_policy, requested=requested_policy
                ):
                    if not allowed:
                        with self.assertRaises(ValueError):
                            backend.reopen_directory(source, requested_policy)
                        self.assertEqual(api.duplicates, 0)
                    else:
                        duplicate = backend.reopen_directory(
                            source, requested_policy
                        )
                        self.assertIs(duplicate.share_policy, requested_policy)
                        self.assertFalse(duplicate.created)
                        resource = backend._resource(duplicate)
                        self.assertEqual(resource.desired_access, actual_access)
                        self.assertEqual(resource.share_mode, actual_share)
                        self.assertEqual(duplicate.identity, source.identity)
                        duplicate.close()
                        self.assertEqual(api.duplicates, 1)
                    source.close()

    def test_policy_name_cannot_hide_reduced_actual_authority(self) -> None:
        api = _DuplicateApi()
        metadata = windows_native._Metadata(
            FileIdentity(17, 19),
            FilesystemIdentity(17, 255, 0x4006),
            EntryKind.DIRECTORY,
            0,
            0,
        )
        backend = _ReopenBackend(api, metadata)
        source = self._source(
            backend,
            SharePolicy.MUTATION,
            _directory_access(SharePolicy.SCAN, relative_target=False),
            _share_mode(SharePolicy.MUTATION),
        )

        with self.assertRaises(ValueError):
            backend.reopen_directory(source, SharePolicy.MUTATION)

        self.assertEqual(api.duplicates, 0)
        source.close()


class _PathCapacityApi:
    def __init__(
        self,
        *,
        dos_path: str = "\\\\?\\C:\\root",
        guid_path: str = (
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\root"
        ),
        volume_root: str = (
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\"
        ),
        volume_information: list[tuple[int, int, int]] | None = None,
        available: int | BaseException = 123_456,
        allocation: tuple[int, int] | BaseException = (8, 4_096),
        raw_final_results: list[int] | None = None,
        events: list[str] | None = None,
    ) -> None:
        self.dos_path = dos_path
        self.guid_path = guid_path
        self.volume_root = volume_root
        self.volume_information = volume_information or [
            (0x9ABC_DEF0, 255, 0x4006),
            (0x9ABC_DEF0, 255, 0x4006),
        ]
        self.available = available
        self.allocation = allocation
        self.raw_final_results = raw_final_results
        self.events = [] if events is None else events
        self.final_sizes: list[int] = []
        self.final_flags: list[int] = []
        self.closed: list[int] = []
        self._last_error = 5

    def GetFinalPathNameByHandleW(
        self,
        handle: int,
        buffer: Any,
        buffer_size: int,
        flags: int,
    ) -> int:
        import ctypes

        self.events.append(f"final:{flags}")
        self.final_sizes.append(buffer_size)
        self.final_flags.append(flags)
        if self.raw_final_results is not None:
            return self.raw_final_results.pop(0)
        path = self.dos_path if flags == windows_native.VOLUME_NAME_DOS else self.guid_path
        required = len(path) + 1
        if buffer_size < required:
            return required
        units = ctypes.cast(buffer, ctypes.POINTER(windows_native.WCHAR))
        encoded = path.encode("utf-16-le")
        for index in range(len(path)):
            units[index] = int.from_bytes(encoded[index * 2 : index * 2 + 2], "little")
        units[len(path)] = 0
        return len(path)

    def get_volume_path(self, path: str) -> str:
        self.events.append("volume-path")
        return self.volume_root

    def get_volume_information(self, root: str) -> tuple[int, int, int]:
        self.events.append("volume-info")
        return self.volume_information.pop(0)

    def get_disk_free_space_ex(self, path: str) -> int:
        self.events.append("free-space")
        if isinstance(self.available, BaseException):
            raise self.available
        return self.available

    def get_disk_free_space(self, root: str) -> tuple[int, int]:
        self.events.append("allocation")
        if isinstance(self.allocation, BaseException):
            raise self.allocation
        return self.allocation

    def CloseHandle(self, handle: int) -> bool:
        self.closed.append(handle)
        return True

    def last_error(self) -> int:
        return self._last_error


class _CapacityBackend(WindowsFilesystemBackend):
    def __init__(
        self,
        api: _PathCapacityApi,
        metadata: list[Any],
        events: list[str] | None = None,
    ) -> None:
        super().__init__(api=api, osfhandle_opener=lambda _handle, _flags: 0)
        self._capacity_metadata = metadata
        self._events = api.events if events is None else events

    def _metadata(self, handle: int, component: object) -> Any:
        self._events.append("metadata")
        return self._capacity_metadata.pop(0)


def _capacity_metadata(
    *,
    identity: FileIdentity = FileIdentity(0x1234_5678_9ABC_DEF0, 31),
    filesystem: FilesystemIdentity = FilesystemIdentity(
        0x1234_5678_9ABC_DEF0, 255, 0x4006
    ),
) -> Any:
    return windows_native._Metadata(
        identity, filesystem, EntryKind.DIRECTORY, 0, 0
    )


def _capacity_capability(
    backend: WindowsFilesystemBackend,
) -> DirectoryCapability:
    filesystem = FilesystemIdentity(0x1234_5678_9ABC_DEF0, 255, 0x4006)
    return DirectoryCapability(
        backend,
        _WindowsResource(91, None, None, False),
        identity=FileIdentity(filesystem.volume, 31),
        filesystem=filesystem,
        kind=EntryKind.DIRECTORY,
        logical_size=0,
        modified_ns=0,
        security_domain=SecurityDomain.CALLER,
        share_policy=SharePolicy.SCAN,
        created=False,
        path_hint=Path("C:/root"),
    )


class WindowsFinalPathTests(unittest.TestCase):
    def test_final_path_grows_once_at_the_512_unit_boundary(self) -> None:
        prefix = "\\\\?\\C:\\"
        path = prefix + "a" * (512 - len(prefix))
        api = _PathCapacityApi(dos_path=path)
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        directory = _capacity_capability(backend)

        recovered = backend.final_path(directory)

        self.assertEqual(str(recovered), path)
        self.assertEqual(api.final_sizes, [512, 513])
        directory.close()

    def test_final_path_rejects_nonprogress_and_need_above_16k(self) -> None:
        cases = (
            ("nonprogress", [513, 513]),
            ("above-cap", [16 * 1024 + 1]),
        )
        for label, results in cases:
            api = _PathCapacityApi(raw_final_results=list(results))
            backend = WindowsFilesystemBackend(
                api=api, osfhandle_opener=lambda _handle, _flags: 0
            )
            directory = _capacity_capability(backend)
            with self.subTest(label=label), self.assertRaises(OSError):
                backend.final_path(directory)
            directory.close()

    def test_final_path_preserves_native_failure(self) -> None:
        api = _PathCapacityApi(raw_final_results=[0])
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        directory = _capacity_capability(backend)

        with self.assertRaises(PermissionError):
            backend.final_path(directory)

        directory.close()

    def test_public_final_path_rejects_malformed_dos_forms(self) -> None:
        malformed = (
            "C:\\root",
            "\\\\server\\share\\root",
            "\\\\?\\relative",
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\root",
            "\\\\?\\UNC\\server",
        )
        for path in malformed:
            api = _PathCapacityApi(dos_path=path)
            backend = WindowsFilesystemBackend(
                api=api, osfhandle_opener=lambda _handle, _flags: 0
            )
            directory = _capacity_capability(backend)
            with self.subTest(path=path), self.assertRaises(OSError):
                backend.final_path(directory)
            directory.close()

    def test_public_final_path_accepts_absolute_drive_and_unc_dos_forms(self) -> None:
        paths = (
            "\\\\?\\C:\\root",
            "\\\\?\\UNC\\server\\share\\root",
        )
        for path in paths:
            api = _PathCapacityApi(dos_path=path)
            backend = WindowsFilesystemBackend(
                api=api, osfhandle_opener=lambda _handle, _flags: 0
            )
            directory = _capacity_capability(backend)
            with self.subTest(path=path):
                self.assertEqual(str(backend.final_path(directory)), path)
            directory.close()


class WindowsCapacityTests(unittest.TestCase):
    def _backend(
        self,
        api: _PathCapacityApi,
        metadata: list[Any] | None = None,
    ) -> tuple[_CapacityBackend, DirectoryCapability]:
        backend = _CapacityBackend(
            api,
            metadata or [_capacity_metadata(), _capacity_metadata()],
        )
        return backend, _capacity_capability(backend)

    def test_available_bytes_uses_guid_path_and_revalidates_in_order(self) -> None:
        events: list[str] = []
        api = _PathCapacityApi(events=events)
        backend, directory = self._backend(api)

        available = backend.available_bytes(directory)

        self.assertEqual(available, 123_456)
        self.assertEqual(
            events,
            [
                "metadata",
                f"final:{windows_native.VOLUME_NAME_GUID}",
                "volume-path",
                "volume-info",
                "free-space",
                "volume-info",
                "metadata",
            ],
        )
        self.assertEqual(api.final_flags, [windows_native.VOLUME_NAME_GUID])
        directory.close()

    def test_allocation_unit_uses_volume_root_and_revalidates_in_order(self) -> None:
        events: list[str] = []
        api = _PathCapacityApi(events=events, allocation=(8, 4_096))
        backend, directory = self._backend(api)

        allocation_unit = backend.allocation_unit(directory)

        self.assertEqual(allocation_unit, 32_768)
        self.assertEqual(
            events,
            [
                "metadata",
                f"final:{windows_native.VOLUME_NAME_GUID}",
                "volume-path",
                "volume-info",
                "allocation",
                "volume-info",
                "metadata",
            ],
        )
        directory.close()

    def test_capacity_rejects_malformed_guid_without_dos_fallback(self) -> None:
        malformed = (
            "\\\\?\\C:\\root",
            "\\\\?\\Volume{not-a-guid}\\root",
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}",
            "\\\\?\\UNC\\server\\share\\root",
        )
        for path in malformed:
            api = _PathCapacityApi(guid_path=path)
            backend, directory = self._backend(api)
            with self.subTest(path=path), self.assertRaises(OSError):
                backend.available_bytes(directory)
            self.assertEqual(api.final_flags, [windows_native.VOLUME_NAME_GUID])
            directory.close()

    def test_capacity_rejects_wrong_or_unterminated_volume_root(self) -> None:
        roots = (
            "C:\\",
            "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}",
            "\\\\?\\Volume{fedcba98-7654-3210-fedc-ba9876543210}\\",
        )
        for root in roots:
            api = _PathCapacityApi(volume_root=root)
            backend, directory = self._backend(api)
            with self.subTest(root=root), self.assertRaises(OSError):
                backend.available_bytes(directory)
            directory.close()

    def test_capacity_rejects_path_volume_discriminator_changes(self) -> None:
        expected = (0x9ABC_DEF0, 255, 0x4006)
        changed = (
            (0x9ABC_DEF1, 255, 0x4006),
            (0x9ABC_DEF0, 256, 0x4006),
            (0x9ABC_DEF0, 255, 0x4007),
        )
        for reported in changed:
            api = _PathCapacityApi(volume_information=[expected, reported])
            backend, directory = self._backend(api)
            with self.subTest(reported=reported), self.assertRaises(OSError):
                backend.available_bytes(directory)
            directory.close()

    def test_capacity_rejects_handle_object_or_filesystem_changes(self) -> None:
        cases = (
            (
                "object",
                _capacity_metadata(identity=FileIdentity(0x1234_5678_9ABC_DEF0, 32)),
            ),
            (
                "filesystem",
                _capacity_metadata(
                    filesystem=FilesystemIdentity(
                        0x1234_5678_9ABC_DEF0, 256, 0x4006
                    )
                ),
            ),
        )
        for label, after in cases:
            api = _PathCapacityApi()
            backend, directory = self._backend(
                api, [_capacity_metadata(), after]
            )
            with self.subTest(label=label), self.assertRaisesRegex(
                OSError, "identity changed"
            ):
                backend.available_bytes(directory)
            directory.close()

    def test_available_bytes_rejects_negative_and_api_failure(self) -> None:
        for label, available in (
            ("negative", -1),
            ("failure", OSError("free-space failure")),
        ):
            api = _PathCapacityApi(available=available)
            backend, directory = self._backend(api)
            with self.subTest(label=label), self.assertRaises(OSError):
                backend.available_bytes(directory)
            directory.close()

    def test_allocation_unit_rejects_nonpositive_and_overflow_values(self) -> None:
        values = (
            (0, 4_096),
            (8, 0),
            (-1, 4_096),
            (1 << 63, 3),
        )
        for allocation in values:
            api = _PathCapacityApi(allocation=allocation)
            backend, directory = self._backend(api)
            with self.subTest(allocation=allocation), self.assertRaises(OSError):
                backend.allocation_unit(directory)
            directory.close()

    def test_allocation_unit_preserves_api_failure(self) -> None:
        api = _PathCapacityApi(allocation=OSError("allocation failure"))
        backend, directory = self._backend(api)

        with self.assertRaisesRegex(OSError, "allocation failure"):
            backend.allocation_unit(directory)

        directory.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_native_final_path_and_capacity_are_handle_tied(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            directory = backend.open_root(Path(raw), SharePolicy.SCAN)
            try:
                self.assertTrue(str(backend.final_path(directory)).startswith("\\\\?\\"))
                self.assertGreaterEqual(backend.available_bytes(directory), 0)
                self.assertGreater(backend.allocation_unit(directory), 0)
            finally:
                directory.close()


@unittest.skipUnless(os.name == "nt", "requires Windows native handles")
class WindowsTask4BindingTests(unittest.TestCase):
    def test_capacity_and_final_path_bindings_have_exact_signatures(self) -> None:
        import ctypes

        api = _WindowsApi()
        self.assertEqual(
            api.GetFinalPathNameByHandleW.argtypes,
            (
                windows_native.HANDLE,
                ctypes.POINTER(windows_native.WCHAR),
                windows_native.ULONG,
                windows_native.ULONG,
            ),
        )
        self.assertIs(api.GetFinalPathNameByHandleW.restype, windows_native.ULONG)
        self.assertEqual(
            api.GetVolumePathNameW.argtypes,
            (
                ctypes.c_wchar_p,
                ctypes.POINTER(windows_native.WCHAR),
                windows_native.ULONG,
            ),
        )
        self.assertIs(api.GetVolumePathNameW.restype, windows_native.BOOL)
        self.assertEqual(len(api.GetVolumeInformationW.argtypes), 8)
        self.assertIs(api.GetVolumeInformationW.restype, windows_native.BOOL)
        self.assertEqual(len(api.GetDiskFreeSpaceExW.argtypes), 4)
        self.assertIs(api.GetDiskFreeSpaceExW.restype, windows_native.BOOL)
        self.assertEqual(len(api.GetDiskFreeSpaceW.argtypes), 5)
        self.assertIs(api.GetDiskFreeSpaceW.restype, windows_native.BOOL)


@unittest.skipUnless(os.name == "nt", "requires Windows native handles")
class WindowsEnumerationTests(unittest.TestCase):
    def _junction(self, link: Path, target: Path) -> None:
        subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(link), str(target)],
            check=True,
            capture_output=True,
            text=True,
        )

    def test_enumeration_matches_direct_no_follow_metadata(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "file").write_bytes(b"payload")
            (root_path / "directory").mkdir()
            self._junction(root_path / "junction", root_path / "directory")
            root = backend.open_root(root_path, SharePolicy.SCAN)
            try:
                listed = {entry.name: entry for entry in backend.entries(root)}
                self.assertEqual(
                    set(listed), {"file", "directory", "junction"}
                )
                for name, entry in listed.items():
                    with self.subTest(name=name):
                        direct = backend.entry(root, name)
                        self.assertIsNotNone(direct)
                        assert direct is not None
                        self.assertEqual(entry, direct)
                        self.assertEqual(entry.filesystem, root.filesystem)
                self.assertIs(listed["file"].kind, EntryKind.REGULAR)
                self.assertEqual(listed["file"].logical_size, 7)
                self.assertIs(listed["directory"].kind, EntryKind.DIRECTORY)
                self.assertIs(listed["junction"].kind, EntryKind.REPARSE)
            finally:
                root.close()

    def test_scan_enumeration_may_retain_or_omit_concurrently_deleted_record(
        self,
    ) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            candidate = root_path / "candidate"
            candidate.write_bytes(b"candidate")
            (root_path / "stable").write_bytes(b"stable")
            root = backend.open_root(root_path, SharePolicy.SCAN)
            try:
                observed = backend.entry(root, "candidate")
                self.assertIsNotNone(observed)
                iterator = backend.entries(root)
                records = [next(iterator)]
                candidate.unlink()
                records.extend(iterator)
                by_name = {entry.name: entry for entry in records}
                self.assertIn("stable", by_name)
                if "candidate" in by_name:
                    self.assertEqual(by_name["candidate"], observed)
            finally:
                root.close()

    def test_scan_allows_directory_rename_while_pinned_blocks_until_close(
        self,
    ) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            container = Path(raw)
            scan_path = container / "scan"
            scan_moved = container / "scan-moved"
            scan_path.mkdir()
            scan = backend.open_root(scan_path, SharePolicy.SCAN)
            scan_path.rename(scan_moved)
            scan.close()

            pinned_path = container / "pinned"
            pinned_moved = container / "pinned-moved"
            pinned_path.mkdir()
            pinned = backend.open_root(pinned_path, SharePolicy.PINNED)
            try:
                with self.assertRaises(OSError):
                    pinned_path.rename(pinned_moved)
            finally:
                pinned.close()
            pinned_path.rename(pinned_moved)

    def test_depth_128_frames_and_retained_anchor_have_separate_counts(self) -> None:
        class CountingBackend(WindowsFilesystemBackend):
            def __init__(self) -> None:
                super().__init__()
                self.active_handles: set[int] = set()
                self.max_active = 0

            def _directory_capability(
                self,
                handle: int,
                metadata: Any,
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
                capability = super()._directory_capability(
                    handle,
                    metadata,
                    parent=parent,
                    name=name,
                    share_policy=share_policy,
                    security_domain=security_domain,
                    created=created,
                    path_hint=path_hint,
                    desired_access=desired_access,
                    share_mode=share_mode,
                )
                handle = self._resource(capability).handle
                self.active_handles.add(handle)
                self.max_active = max(self.max_active, len(self.active_handles))
                return capability

            def close_resource(self, resource: object) -> None:
                native = self._checked_resource(resource)
                handle = native.handle
                super().close_resource(resource)
                self.active_handles.discard(handle)

        backend = CountingBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            created_paths: list[Path] = []
            builder: DirectoryCapability | None = None
            current: DirectoryCapability | None = None
            anchor: DirectoryCapability | None = None
            frames: list[Any] = []
            try:
                builder = backend.open_root(root_path, SharePolicy.MUTATION)
                current = builder
                for _depth in range(128):
                    child = backend.create_directory(
                        current, "d", SharePolicy.MUTATION
                    )
                    created_paths.append(child.path_hint)
                    if current is not builder:
                        current.close()
                    current = child
                current.close()
                builder.close()
                self.assertEqual(backend.active_handles, set())
                backend.max_active = 0

                anchor = backend.open_root(root_path, SharePolicy.SCAN)
                frames = [
                    backend.entries_owned(backend.reopen_directory(anchor))
                ]
                for _depth in range(128):
                    record = next(frames[-1])
                    self.assertEqual(record.name, "d")
                    child = backend.open_directory(
                        frames[-1].directory, "d", SharePolicy.SCAN
                    )
                    frames.append(backend.entries_owned(child))
                frame_count = sum(frame.directory.is_open for frame in frames)
                total_count = frame_count + int(anchor.is_open)
                self.assertEqual(frame_count, 129)
                self.assertEqual(total_count, 130)
                self.assertEqual(backend.max_active, 130)
            finally:
                for frame in reversed(frames):
                    frame.close()
                if anchor is not None:
                    anchor.close()
                if current is not None:
                    current.close()
                if builder is not None:
                    builder.close()
                for created in reversed(created_paths):
                    os.rmdir("\\\\?\\" + str(created))
            self.assertEqual(backend.active_handles, set())

class WindowsLayoutTests(unittest.TestCase):
    def test_windows_native_libraries_are_loaded_for_native_bindings(self) -> None:
        if os.name == "nt":
            self.assertIsNotNone(windows_native._KERNEL32)
            self.assertIsNotNone(windows_native._ADVAPI32)
            self.assertIsNotNone(windows_native._NTDLL)
        else:
            self.assertIsNone(windows_native._KERNEL32)
            self.assertIsNone(windows_native._ADVAPI32)
            self.assertIsNone(windows_native._NTDLL)

    def test_native_layouts_use_exact_width_fields(self) -> None:
        import ctypes

        if ctypes.sizeof(ctypes.c_void_p) == 8:
            self.assertEqual(ctypes.sizeof(UNICODE_STRING), 16)
            self.assertEqual(ctypes.sizeof(OBJECT_ATTRIBUTES), 48)
            self.assertEqual(IO_STATUS_BLOCK.Information.offset, 8)
        else:
            self.assertEqual(ctypes.sizeof(UNICODE_STRING), 8)
            self.assertEqual(ctypes.sizeof(OBJECT_ATTRIBUTES), 24)
            self.assertEqual(IO_STATUS_BLOCK.Information.offset, 4)
        self.assertEqual(FILE_ID_INFO.FileId.offset, 8)
        self.assertEqual(FILE_ID_EXTD_DIR_INFO.FileName.offset, 88)
        self.assertIs(
            windows_native.FILE_DISPOSITION_INFO._fields_[0][1],
            ctypes.c_uint8,
        )
        self.assertEqual(
            ctypes.sizeof(windows_native.FILE_DISPOSITION_INFO), 1
        )

class WindowsPureContractTests(unittest.TestCase):
    def test_attribute_kind_mapping_checks_reparse_before_directory(self) -> None:
        self.assertIs(
            windows_native._entry_kind_from_attributes(0x410), EntryKind.REPARSE
        )
        self.assertIs(
            windows_native._entry_kind_from_attributes(0x010), EntryKind.DIRECTORY
        )
        self.assertIs(
            windows_native._entry_kind_from_attributes(0x040), EntryKind.OTHER
        )
        self.assertIs(
            windows_native._entry_kind_from_attributes(0x080), EntryKind.REGULAR
        )

    def test_filetime_conversion_uses_exact_windows_epoch_and_units(self) -> None:
        epoch = 116_444_736_000_000_000
        self.assertEqual(windows_native._filetime_to_unix_ns(epoch), 0)
        self.assertEqual(windows_native._filetime_to_unix_ns(epoch + 12_345), 1_234_500)

    def test_only_leaf_not_found_becomes_absence(self) -> None:
        missing = _error_from_win32(2, "observe child", "item", leaf=True)
        missing_root = _error_from_win32(2, "open root", r"C:\missing")
        missing_parent = _error_from_win32(3, "observe child", "item")
        denied = _error_from_win32(5, "observe child", "item")
        sharing = _error_from_win32(32, "observe child", "item")

        self.assertIs(type(missing), FileNotFoundError)
        self.assertIs(type(missing_root), OSError)
        self.assertIs(type(missing_parent), OSError)
        self.assertIs(type(denied), PermissionError)
        self.assertIs(type(sharing), OSError)
        self.assertEqual(missing.winerror, 2)
        self.assertEqual(missing_root.winerror, 2)
        self.assertEqual(missing_parent.winerror, 3)
        self.assertEqual(denied.winerror, 5)
        self.assertEqual(sharing.winerror, 32)

    def test_error_evidence_is_bounded_before_construction(self) -> None:
        error = _error_from_win32(123, "open child", "x" * 20_000)

        self.assertLessEqual(len(error.filename.encode("utf-8")), 4_096)
        self.assertEqual(error.winerror, 123)

    def test_windows_component_encoder_accepts_one_strict_component(self) -> None:
        self.assertEqual(
            _encode_windows_component("safe-file.01"),
            "safe-file.01".encode("utf-16-le"),
        )

    def test_windows_component_encoder_rejects_invalid_names(self) -> None:
        invalid = (
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "nul\0char",
            "stream:name",
            "control\x1f",
            'angle<',
            'angle>',
            'quote"',
            "pipe|",
            "question?",
            "star*",
            "trailing.",
            "trailing ",
            "CON",
            "con.txt",
            "PRN",
            "AUX",
            "NUL",
            "CONIN$",
            "CONOUT$.log",
            "COM1",
            "LPT9.log",
            "COM¹.txt",
            "LPT³",
            "\ud800",
            "x" * 32_768,
        )
        for name in invalid:
            with self.subTest(name=repr(name)):
                with self.assertRaises(ValueError):
                    _encode_windows_component(name)

    def test_share_modes_match_pinning_policy(self) -> None:
        common = FILE_SHARE_READ | FILE_SHARE_WRITE
        self.assertEqual(_share_mode(SharePolicy.SCAN), common | FILE_SHARE_DELETE)
        self.assertEqual(_share_mode(SharePolicy.PINNED), common)
        self.assertEqual(
            _share_mode(SharePolicy.MUTATION), common | FILE_SHARE_DELETE
        )

    def test_directory_authority_matches_policy_and_context(self) -> None:
        expected = {
            (SharePolicy.SCAN, False): 0x0012_00A1,
            (SharePolicy.SCAN, True): 0x0012_00A1,
            (SharePolicy.PINNED, False): 0x0012_00E7,
            (SharePolicy.PINNED, True): 0x0013_00E7,
            (SharePolicy.MUTATION, False): 0x0013_01E7,
            (SharePolicy.MUTATION, True): 0x0013_01E7,
        }
        for key, desired_access in expected.items():
            with self.subTest(policy=key[0], relative=key[1]):
                self.assertEqual(
                    _directory_access(key[0], relative_target=key[1]),
                    desired_access,
                )

    def test_typed_file_and_deletion_only_authority_are_distinct(self) -> None:
        expected_files = {
            (FileAccess.READ, SharePolicy.SCAN): 0x0012_0081,
            (FileAccess.WRITE, SharePolicy.PINNED): 0x0012_0182,
            (FileAccess.READ_WRITE, SharePolicy.MUTATION): 0x0013_0183,
        }
        for key, desired_access in expected_files.items():
            with self.subTest(access=key[0], policy=key[1]):
                self.assertEqual(_file_access(*key), desired_access)
        self.assertEqual(_entry_access(SharePolicy.SCAN), 0x0012_0080)
        self.assertEqual(_entry_access(SharePolicy.PINNED), 0x0013_0080)
        self.assertEqual(_entry_access(SharePolicy.MUTATION), 0x0013_0080)
        self.assertFalse(_file_access(FileAccess.READ, SharePolicy.PINNED) & DELETE)


class _RecordingCloseApi:
    def __init__(self, outcomes: list[bool]) -> None:
        self.outcomes = outcomes
        self.closed: list[int] = []

    def CloseHandle(self, handle: int) -> bool:
        self.closed.append(handle)
        return self.outcomes.pop(0)

    def last_error(self) -> int:
        return 6


class WindowsResourceOwnershipTests(unittest.TestCase):
    def test_partially_initialized_backend_finalizer_is_safe(self) -> None:
        backend = WindowsFilesystemBackend.__new__(WindowsFilesystemBackend)

        backend.__del__()

    def _capability(
        self,
        backend: WindowsFilesystemBackend,
        resource: _WindowsResource,
    ) -> FileCapability:
        return FileCapability(
            backend,
            resource,
            identity=FileIdentity(1, 2),
            filesystem=FilesystemIdentity(1),
            kind=EntryKind.REGULAR,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.CALLER,
            share_policy=SharePolicy.MUTATION,
            created=True,
            path_hint=Path("item"),
        )

    def test_failed_close_retains_handle_and_capability_ownership(self) -> None:
        api = _RecordingCloseApi([False, True])
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        resource = _WindowsResource(41, None, "item", True)
        capability = self._capability(backend, resource)

        with self.assertRaises(OSError) as raised:
            capability.close()
        self.assertEqual(raised.exception.winerror, 6)
        self.assertTrue(capability.is_open)
        self.assertEqual(resource.handle, 41)

        capability.close()
        self.assertTrue(capability.closed)
        self.assertEqual(resource.handle, INVALID_OWNED_HANDLE)
        self.assertEqual(api.closed, [41, 41])

    def test_failed_crt_transfer_retains_the_only_native_owner(self) -> None:
        api = _RecordingCloseApi([True])
        opener = mock.Mock(side_effect=OSError("conversion failed"))
        backend = WindowsFilesystemBackend(api=api, osfhandle_opener=opener)
        resource = _WindowsResource(43, None, "item", True)
        capability = self._capability(backend, resource)

        with self.assertRaisesRegex(OSError, "conversion failed"):
            capability.detach_to_fd(os.O_RDONLY)

        self.assertTrue(capability.is_open)
        self.assertEqual(resource.handle, 43)
        capability.close()
        self.assertEqual(api.closed, [43])

    def test_successful_crt_transfer_invalidates_the_native_owner_once(self) -> None:
        api = _RecordingCloseApi([])
        opener = mock.Mock(return_value=17)
        backend = WindowsFilesystemBackend(api=api, osfhandle_opener=opener)
        resource = _WindowsResource(47, None, "item", True)
        capability = self._capability(backend, resource)

        descriptor = capability.detach_to_fd(os.O_RDONLY)

        self.assertEqual(descriptor, 17)
        self.assertTrue(capability.detached)
        self.assertEqual(resource.handle, INVALID_OWNED_HANDLE)
        capability.close()
        self.assertEqual(api.closed, [])
        self.assertTrue(opener.call_args.args[1] & getattr(os, "O_NOINHERIT", 0))


@unittest.skipUnless(os.name == "nt", "requires Windows native handles")
class WindowsOpenTests(unittest.TestCase):
    class _ApiProxy:
        def __init__(
            self,
            *,
            before: object | None = None,
            after: object | None = None,
        ) -> None:
            self._native = _WindowsApi()
            self._before = before
            self._after = after
            self.nt_create_calls = 0
            self.nt_create_records: list[dict[str, int | str]] = []
            self.disposition_calls: list[tuple[int, int, int]] = []
            self.fail_next_disposition = False
            self.fail_next_close = False
            self.fail_handle: int | None = None
            self.last_opened_handle: int | None = None
            self._last_error = 0

        def __getattr__(self, name: str) -> object:
            return getattr(self._native, name)

        def NtCreateFile(self, *args: object) -> int:
            self.nt_create_calls += 1
            attributes = ctypes.cast(
                cast(Any, args[2]), ctypes.POINTER(OBJECT_ATTRIBUTES)
            ).contents
            unicode_name = attributes.ObjectName.contents
            encoded_name = ctypes.string_at(
                unicode_name.Buffer, int(unicode_name.Length)
            )
            self.nt_create_records.append(
                {
                    "name": encoded_name.decode("utf-16-le", errors="strict"),
                    "desired_access": int(cast(Any, args[1])),
                    "disposition": int(cast(Any, args[7])),
                    "security_descriptor": int(attributes.SecurityDescriptor or 0),
                }
            )
            status = self._native.NtCreateFile(*args)
            if status >= 0:
                pointer = ctypes.cast(
                    cast(Any, args[0]), ctypes.POINTER(ctypes.c_void_p)
                )
                self.last_opened_handle = int(pointer.contents.value or 0)
            return status

        def SetFileInformationByHandle(self, *args: object) -> bool:
            handle = int(cast(Any, args[0]))
            information_class = int(cast(Any, args[1]))
            buffer_size = int(cast(Any, args[3]))
            self.disposition_calls.append((handle, information_class, buffer_size))
            if self.fail_next_disposition:
                self.fail_next_disposition = False
                self._last_error = windows_native.ERROR_ACCESS_DENIED
                return False
            return bool(self._native.SetFileInformationByHandle(*args))

        def CloseHandle(self, handle: int) -> bool:
            if self.fail_next_close or handle == self.fail_handle:
                self.fail_next_close = False
                self.fail_handle = None
                self._last_error = 6
                return False
            return bool(self._native.CloseHandle(handle))

        def last_error(self) -> int:
            return self._last_error or self._native.last_error()

        def before_relative_open(self) -> None:
            callback = self._before
            self._before = None
            if callable(callback):
                callback()

        def after_relative_open(self) -> None:
            callback = self._after
            self._after = None
            if callable(callback):
                callback()

    def test_root_handle_stays_on_original_after_path_replacement(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            container = Path(raw)
            original = container / "root"
            moved = container / "moved"
            original.mkdir()
            (original / "anchor").write_bytes(b"original")
            root = backend.open_root(original, SharePolicy.SCAN)
            try:
                original.rename(moved)
                original.mkdir()
                (original / "replacement").write_bytes(b"replacement")

                self.assertIsNotNone(backend.entry(root, "anchor"))
                self.assertIsNone(backend.entry(root, "replacement"))
                replacement = backend.open_root(original, SharePolicy.SCAN)
                try:
                    self.assertNotEqual(root.identity, replacement.identity)
                finally:
                    replacement.close()
            finally:
                root.close()

    def test_relative_file_open_keeps_the_enumerated_identity(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "item").write_bytes(b"payload")
            root = backend.open_root(root_path, SharePolicy.SCAN)
            listed = backend.entry(root, "item")
            self.assertIsNotNone(listed)
            opened = backend.open_file(
                root,
                "item",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.SCAN,
            )
            assert listed is not None
            self.assertEqual(opened.identity, listed.identity)
            opened.close()
            root.close()

    def test_crt_transfer_has_one_owner(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            file = backend.open_file(
                root,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            descriptor = file.detach_to_fd(os.O_RDWR | os.O_BINARY)
            self.assertFalse(
                os.get_handle_inheritable(msvcrt.get_osfhandle(descriptor))
            )
            os.write(descriptor, b"payload")
            os.close(descriptor)
            file.close()
            self.assertEqual((Path(raw) / "item").read_bytes(), b"payload")
            root.close()

    def test_create_disposition_uses_native_created_result(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            try:
                created = backend.open_file(
                    root,
                    "created",
                    access=FileAccess.READ_WRITE,
                    disposition=CreateDisposition.CREATE_NEW,
                )
                self.assertTrue(created.created)
                created.close()
                opened = backend.open_file(
                    root,
                    "created",
                    access=FileAccess.READ,
                    disposition=CreateDisposition.OPEN_OR_CREATE,
                )
                self.assertFalse(opened.created)
                opened.close()
                created_if_missing = backend.open_file(
                    root,
                    "created-if-missing",
                    access=FileAccess.READ_WRITE,
                    disposition=CreateDisposition.OPEN_OR_CREATE,
                )
                self.assertTrue(created_if_missing.created)
                created_if_missing.close()
            finally:
                root.close()

    def test_relative_directory_create_and_open_keep_identity(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            try:
                created = backend.create_directory(
                    root, "child", SharePolicy.MUTATION
                )
                self.assertTrue(created.created)
                identity = created.identity
                created.close()
                opened = backend.open_directory(
                    root, "child", SharePolicy.SCAN
                )
                try:
                    self.assertFalse(opened.created)
                    self.assertEqual(opened.identity, identity)
                finally:
                    opened.close()
            finally:
                root.close()

    def test_reopen_directory_duplicates_the_same_open_object(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            root = backend.open_root(root_path, SharePolicy.SCAN)
            duplicate = backend.reopen_directory(root)
            try:
                self.assertEqual(duplicate.identity, root.identity)
                self.assertEqual(duplicate.filesystem, root.filesystem)
                self.assertFalse(duplicate.created)
                self.assertTrue(root.is_open)
                duplicate.close()
                self.assertTrue(root.is_open)
            finally:
                duplicate.close()
                root.close()

    def test_native_resources_record_actual_delete_authority(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "directory").mkdir()
            (root_path / "file").write_bytes(b"payload")
            root = backend.open_root(root_path, SharePolicy.PINNED)
            pinned_file = backend.open_file(
                root,
                "file",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.PINNED,
            )
            pinned_directory = backend.open_directory(
                root, "directory", SharePolicy.PINNED
            )
            self.assertFalse(backend._resource(root).delete_authority)
            self.assertFalse(backend._resource(pinned_file).delete_authority)
            self.assertTrue(backend._resource(pinned_directory).delete_authority)
            pinned_file.close()
            deletion_entry = backend.open_entry(
                root, "file", SharePolicy.PINNED
            )
            try:
                self.assertTrue(backend._resource(deletion_entry).delete_authority)
            finally:
                deletion_entry.close()
                pinned_directory.close()
                root.close()

    def test_invalid_component_never_reaches_ntcreatefile(self) -> None:
        api = self._ApiProxy()
        backend = WindowsFilesystemBackend(api=api)
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.SCAN)
            try:
                invalid_names = (
                    "",
                    ".",
                    "..",
                    "a/b",
                    "a\\b",
                    "nul\0char",
                    "stream:name",
                    "control\x1f",
                    'angle<',
                    'angle>',
                    'quote"',
                    "pipe|",
                    "question?",
                    "star*",
                    "trailing.",
                    "trailing ",
                    "CON",
                    "con.txt",
                    "PRN",
                    "AUX",
                    "NUL",
                    "CONIN$",
                    "CONOUT$.log",
                    "COM1",
                    "LPT9.log",
                    "COM¹.txt",
                    "LPT³",
                    "\ud800",
                    "x" * 32_768,
                )
                for name in invalid_names:
                    with self.subTest(name=name):
                        before = api.nt_create_calls
                        with self.assertRaises(ValueError):
                            backend.open_file(
                                root,
                                name,
                                access=FileAccess.READ,
                                disposition=CreateDisposition.OPEN_EXISTING,
                                share_policy=SharePolicy.SCAN,
                            )
                        self.assertEqual(api.nt_create_calls, before)
            finally:
                root.close()

    def test_existing_open_rejects_replacement_between_observation_and_open(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            item = root_path / "item"
            old = root_path / "old"
            item.write_bytes(b"original")

            def replace() -> None:
                item.rename(old)
                item.write_bytes(b"replacement")

            backend = WindowsFilesystemBackend(api=self._ApiProxy(before=replace))
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            try:
                with self.assertRaisesRegex(OSError, "identity changed"):
                    backend.open_file(
                        root,
                        "item",
                        access=FileAccess.READ,
                        disposition=CreateDisposition.OPEN_EXISTING,
                    )
                self.assertEqual(old.read_bytes(), b"original")
                self.assertEqual(item.read_bytes(), b"replacement")
            finally:
                root.close()

    def test_create_rejects_replacement_before_post_open_observation(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            item = root_path / "item"
            old = root_path / "old"

            def replace() -> None:
                item.rename(old)
                item.write_bytes(b"replacement")

            backend = WindowsFilesystemBackend(api=self._ApiProxy(after=replace))
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            try:
                with self.assertRaisesRegex(OSError, "identity changed"):
                    backend.open_file(
                        root,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.CREATE_NEW,
                    )
                self.assertFalse(old.exists())
                self.assertEqual(item.read_bytes(), b"replacement")
            finally:
                root.close()

    def test_validation_close_failure_is_secondary_and_retains_owner(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            item = root_path / "item"
            old = root_path / "old"
            proxy = self._ApiProxy()

            def replace_and_fail_close() -> None:
                item.rename(old)
                item.write_bytes(b"replacement")
                proxy.fail_handle = proxy.last_opened_handle

            proxy._after = replace_and_fail_close
            backend = WindowsFilesystemBackend(api=proxy)
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            try:
                with self.assertRaisesRegex(OSError, "identity changed") as raised:
                    backend.open_file(
                        root,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.CREATE_NEW,
                    )
                self.assertEqual(len(backend._failed_closes), 1)
                notes = list(getattr(raised.exception, "__notes__", ()))
                self.assertEqual(len(notes), 1)
                self.assertIn("close failed", notes[0])
                self.assertLessEqual(len(notes[0].encode("utf-8")), 4_096)
                backend.close_resource(backend._failed_closes.pop())
            finally:
                root.close()

    def test_entry_close_failure_is_not_reported_as_a_success(self) -> None:
        proxy = self._ApiProxy()
        backend = WindowsFilesystemBackend(api=proxy)
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "item").write_bytes(b"payload")
            root = backend.open_root(root_path, SharePolicy.SCAN)
            try:
                proxy.fail_next_close = True
                with self.assertRaises(OSError) as raised:
                    backend.entry(root, "item")
                self.assertEqual(raised.exception.winerror, 6)
                self.assertEqual(len(backend._failed_closes), 1)
                backend.close_resource(backend._failed_closes.pop())
            finally:
                root.close()

    def test_typed_opens_reject_wrong_kind_and_reparse_without_following(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "directory").mkdir()
            (root_path / "directory" / "sentinel").write_bytes(b"untouched")
            (root_path / "file").write_bytes(b"file")
            subprocess.run(
                [
                    "cmd",
                    "/c",
                    "mklink",
                    "/J",
                    str(root_path / "link"),
                    str(root_path / "directory"),
                ],
                check=True,
                capture_output=True,
                text=True,
            )
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            try:
                with self.assertRaises(OSError):
                    backend.open_file(
                        root,
                        "directory",
                        access=FileAccess.READ,
                        disposition=CreateDisposition.OPEN_EXISTING,
                    )
                with self.assertRaises(OSError):
                    backend.open_directory(root, "file", SharePolicy.SCAN)
                with self.assertRaises(OSError):
                    backend.open_directory(root, "link", SharePolicy.SCAN)
                link = backend.open_entry(root, "link", SharePolicy.PINNED)
                try:
                    self.assertIs(link.kind, EntryKind.REPARSE)
                    self.assertEqual(
                        (root_path / "directory" / "sentinel").read_bytes(),
                        b"untouched",
                    )
                finally:
                    link.close()
            finally:
                root.close()
            with self.assertRaises(OSError):
                backend.open_root(root_path / "link", SharePolicy.SCAN)

    def test_scan_file_allows_delete_while_handle_retains_original(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            item = root_path / "item"
            item.write_bytes(b"payload")
            root = backend.open_root(root_path, SharePolicy.SCAN)
            opened = backend.open_file(
                root,
                "item",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.SCAN,
            )
            descriptor = opened.detach_to_fd(os.O_RDONLY | os.O_BINARY)
            try:
                item.unlink()
                self.assertEqual(os.read(descriptor, 7), b"payload")
            finally:
                os.close(descriptor)
                root.close()

    def test_pinned_root_blocks_rename_until_close(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            container = Path(raw)
            root_path = container / "root"
            moved = container / "moved"
            root_path.mkdir()
            root = backend.open_root(root_path, SharePolicy.PINNED)
            try:
                with self.assertRaises(OSError):
                    root_path.rename(moved)
            finally:
                root.close()
            root_path.rename(moved)


class _SecurityPolicyApi:
    def __init__(self, *, owner_matches: bool = True) -> None:
        self.owner_matches = owner_matches
        self.events: list[str] = []
        self.security_writes: list[tuple[object, ...]] = []
        self.local_free_results: list[int] = []

    def EqualSid(self, first: object, second: object) -> bool:
        self.events.append("equal-owner")
        return self.owner_matches

    def SetSecurityInfo(self, *args: object) -> int:
        self.events.append("set-dacl")
        self.security_writes.append(args)
        return 0

    def LocalFree(self, pointer: object) -> int:
        self.events.append("local-free")
        if self.local_free_results:
            return self.local_free_results.pop(0)
        return 0

    def CloseHandle(self, _handle: int) -> bool:
        return True

    def last_error(self) -> int:
        return 5


def _security_capability(
    backend: WindowsFilesystemBackend,
    *,
    kind: EntryKind = EntryKind.REGULAR,
) -> FileCapability | DirectoryCapability:
    resource = _WindowsResource(211, None, "item", True)
    if kind is EntryKind.DIRECTORY:
        return DirectoryCapability(
            backend,
            resource,
            kind=EntryKind.DIRECTORY,
            identity=FileIdentity(7, 11),
            filesystem=FilesystemIdentity(7, 255, 0x4006),
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.MANAGED,
            share_policy=SharePolicy.MUTATION,
            created=False,
            path_hint=Path("C:/managed/item"),
        )
    return FileCapability(
        backend,
        resource,
        kind=kind,
        identity=FileIdentity(7, 11),
        filesystem=FilesystemIdentity(7, 255, 0x4006),
        logical_size=0,
        modified_ns=0,
        security_domain=SecurityDomain.MANAGED,
        share_policy=SharePolicy.MUTATION,
        created=False,
        path_hint=Path("C:/managed/item"),
    )


def _security_material_for_tests() -> types.SimpleNamespace:
    return types.SimpleNamespace(
        sid=ctypes.c_void_p(101),
        descriptor=ctypes.c_void_p(102),
        dacl=ctypes.c_void_p(103),
        dacl_bytes=b"expected-dacl",
        close=mock.Mock(),
    )


def _security_snapshot_for_tests(
    *,
    dacl_bytes: bytes = b"expected-dacl",
    control: int | None = None,
) -> types.SimpleNamespace:
    if control is None:
        control = windows_native.SE_DACL_PROTECTED
    return types.SimpleNamespace(
        owner=ctypes.c_void_p(201),
        dacl=ctypes.c_void_p(202),
        dacl_present=True,
        control=control,
        dacl_bytes=dacl_bytes,
        close=mock.Mock(),
    )


def _token_user_sid_for_tests() -> str:
    api = _WindowsApi()
    token = windows_native.HANDLE()
    if not api.OpenProcessToken(
        api.GetCurrentProcess(),
        windows_native.TOKEN_QUERY,
        ctypes.byref(token),
    ):
        raise OSError(api.last_error(), "OpenProcessToken failed")
    try:
        needed = windows_native.ULONG()
        first = api.GetTokenInformation(
            token,
            windows_native.TokenUser,
            None,
            0,
            ctypes.byref(needed),
        )
        if first or api.last_error() != windows_native.ERROR_INSUFFICIENT_BUFFER:
            raise OSError("unexpected TOKEN_USER size-query result")
        if needed.value == 0 or needed.value > windows_native._MAX_TOKEN_USER_BYTES:
            raise OSError("invalid TOKEN_USER buffer size")
        buffer = ctypes.create_string_buffer(int(needed.value))
        if not api.GetTokenInformation(
            token,
            windows_native.TokenUser,
            buffer,
            needed,
            ctypes.byref(needed),
        ):
            raise OSError(api.last_error(), "GetTokenInformation failed")
        token_user = ctypes.cast(
            buffer, ctypes.POINTER(windows_native.TOKEN_USER)
        ).contents
        string_sid = ctypes.c_wchar_p()
        if not api.ConvertSidToStringSidW(
            token_user.User.Sid, ctypes.byref(string_sid)
        ):
            raise OSError(api.last_error(), "ConvertSidToStringSidW failed")
        try:
            if not string_sid.value:
                raise OSError("token SID conversion returned null")
            return string_sid.value
        finally:
            if api.LocalFree(ctypes.cast(string_sid, ctypes.c_void_p)):
                raise OSError(api.last_error(), "LocalFree SID string failed")
    finally:
        if not api.CloseHandle(token):
            raise OSError(api.last_error(), "CloseHandle token failed")


class _TOKEN_OWNER_FOR_TESTS(ctypes.Structure):
    _fields_ = [("Owner", ctypes.c_void_p)]


def _token_owner_sid_for_tests() -> str:
    api = _WindowsApi()
    token = windows_native.HANDLE()
    if not api.OpenProcessToken(
        api.GetCurrentProcess(),
        windows_native.TOKEN_QUERY,
        ctypes.byref(token),
    ):
        raise OSError(api.last_error(), "OpenProcessToken failed")
    try:
        needed = windows_native.ULONG()
        first = api.GetTokenInformation(
            token,
            4,  # TOKEN_INFORMATION_CLASS::TokenOwner
            None,
            0,
            ctypes.byref(needed),
        )
        if first or api.last_error() != windows_native.ERROR_INSUFFICIENT_BUFFER:
            raise OSError("unexpected TOKEN_OWNER size-query result")
        if needed.value == 0 or needed.value > windows_native._MAX_TOKEN_USER_BYTES:
            raise OSError("invalid TOKEN_OWNER buffer size")
        buffer = ctypes.create_string_buffer(int(needed.value))
        if not api.GetTokenInformation(
            token,
            4,
            buffer,
            needed,
            ctypes.byref(needed),
        ):
            raise OSError(api.last_error(), "GetTokenInformation failed")
        token_owner = ctypes.cast(
            buffer, ctypes.POINTER(_TOKEN_OWNER_FOR_TESTS)
        ).contents
        string_sid = ctypes.c_wchar_p()
        if not api.ConvertSidToStringSidW(
            token_owner.Owner, ctypes.byref(string_sid)
        ):
            raise OSError(api.last_error(), "ConvertSidToStringSidW failed")
        try:
            if not string_sid.value:
                raise OSError("token owner SID conversion returned null")
            return string_sid.value
        finally:
            if api.LocalFree(ctypes.cast(string_sid, ctypes.c_void_p)):
                raise OSError(api.last_error(), "LocalFree SID string failed")
    finally:
        if not api.CloseHandle(token):
            raise OSError(api.last_error(), "CloseHandle token failed")


def _token_is_administrator_for_tests() -> bool:
    advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
    create_well_known_sid = advapi32.CreateWellKnownSid
    create_well_known_sid.argtypes = (
        ctypes.c_int,
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_uint32),
    )
    create_well_known_sid.restype = ctypes.c_int
    check_token_membership = advapi32.CheckTokenMembership
    check_token_membership.argtypes = (
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_int),
    )
    check_token_membership.restype = ctypes.c_int

    sid = ctypes.create_string_buffer(68)
    sid_size = ctypes.c_uint32(len(sid))
    if not create_well_known_sid(
        26,  # WELL_KNOWN_SID_TYPE::WinBuiltinAdministratorsSid
        None,
        sid,
        ctypes.byref(sid_size),
    ):
        raise OSError(ctypes.get_last_error(), "CreateWellKnownSid failed")
    is_member = ctypes.c_int()
    if not check_token_membership(None, sid, ctypes.byref(is_member)):
        raise OSError(ctypes.get_last_error(), "CheckTokenMembership failed")
    return bool(is_member.value)


def _security_descriptor_bytes_for_tests(path: Path) -> bytes:
    api = _WindowsApi()
    owner = ctypes.c_void_p()
    group = ctypes.c_void_p()
    dacl = ctypes.c_void_p()
    descriptor = ctypes.c_void_p()
    result = int(
        api.GetNamedSecurityInfoW(
            str(path),
            windows_native.SE_FILE_OBJECT,
            windows_native.OWNER_SECURITY_INFORMATION
            | windows_native.GROUP_SECURITY_INFORMATION
            | windows_native.DACL_SECURITY_INFORMATION,
            ctypes.byref(owner),
            ctypes.byref(group),
            ctypes.byref(dacl),
            None,
            ctypes.byref(descriptor),
        )
    )
    if result != 0:
        raise OSError(result, "GetNamedSecurityInfoW failed")
    try:
        if not descriptor.value:
            raise OSError("named security descriptor is null")
        length = int(api.GetSecurityDescriptorLength(descriptor))
        if length <= 0:
            raise OSError("named security descriptor has zero length")
        return ctypes.string_at(descriptor, length)
    finally:
        if descriptor.value and api.LocalFree(descriptor):
            raise OSError(api.last_error(), "LocalFree descriptor failed")


def _named_owner_dacl_for_tests(path: Path) -> tuple[str, int, bytes]:
    api = _WindowsApi()
    owner = ctypes.c_void_p()
    dacl = ctypes.c_void_p()
    descriptor = ctypes.c_void_p()
    result = int(
        api.GetNamedSecurityInfoW(
            str(path),
            windows_native.SE_FILE_OBJECT,
            windows_native.OWNER_SECURITY_INFORMATION
            | windows_native.DACL_SECURITY_INFORMATION,
            ctypes.byref(owner),
            None,
            ctypes.byref(dacl),
            None,
            ctypes.byref(descriptor),
        )
    )
    if result != 0:
        raise OSError(result, "GetNamedSecurityInfoW failed")
    string_sid = ctypes.c_wchar_p()
    try:
        if not owner.value or not dacl.value or not descriptor.value:
            raise OSError("named owner or DACL is null")
        if not api.ConvertSidToStringSidW(owner, ctypes.byref(string_sid)):
            raise OSError(api.last_error(), "ConvertSidToStringSidW failed")
        control = windows_native.SECURITY_DESCRIPTOR_CONTROL()
        revision = windows_native.ULONG()
        if not api.GetSecurityDescriptorControl(
            descriptor, ctypes.byref(control), ctypes.byref(revision)
        ):
            raise OSError(api.last_error(), "GetSecurityDescriptorControl failed")
        acl = ctypes.cast(dacl, ctypes.POINTER(windows_native.ACL)).contents
        if acl.AclSize < ctypes.sizeof(windows_native.ACL):
            raise OSError("named DACL size is invalid")
        assert string_sid.value is not None
        return (
            string_sid.value,
            int(control.value),
            ctypes.string_at(dacl, int(acl.AclSize)),
        )
    finally:
        if string_sid.value and api.LocalFree(ctypes.cast(string_sid, ctypes.c_void_p)):
            raise OSError(api.last_error(), "LocalFree SID string failed")
        if descriptor.value and api.LocalFree(descriptor):
            raise OSError(api.last_error(), "LocalFree descriptor failed")


def _expected_managed_dacl_for_tests(sid: str, *, directory: bool) -> bytes:
    api = _WindowsApi()
    inheritance = "OICI" if directory else ""
    sddl = (
        f"O:{sid}D:P(A;{inheritance};FA;;;{sid})"
        f"(A;{inheritance};FA;;;SY)(A;{inheritance};FA;;;BA)"
    )
    descriptor = ctypes.c_void_p()
    length = windows_native.ULONG()
    if not api.ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl,
        windows_native.SDDL_REVISION_1,
        ctypes.byref(descriptor),
        ctypes.byref(length),
    ):
        raise OSError(api.last_error(), "SDDL conversion failed")
    try:
        present = windows_native.BOOL()
        defaulted = windows_native.BOOL()
        dacl = ctypes.c_void_p()
        if not api.GetSecurityDescriptorDacl(
            descriptor,
            ctypes.byref(present),
            ctypes.byref(dacl),
            ctypes.byref(defaulted),
        ):
            raise OSError(api.last_error(), "GetSecurityDescriptorDacl failed")
        if not present.value or not dacl.value:
            raise OSError("expected DACL is absent")
        acl = ctypes.cast(dacl, ctypes.POINTER(windows_native.ACL)).contents
        return ctypes.string_at(dacl, int(acl.AclSize))
    finally:
        if descriptor.value and api.LocalFree(descriptor):
            raise OSError(api.last_error(), "LocalFree descriptor failed")


class WindowsSecurityTests(unittest.TestCase):
    def test_managed_sddl_has_exact_token_owner_and_dacl(self) -> None:
        sid = "S-1-5-21-123"
        self.assertEqual(
            windows_native._managed_security_sddl(sid, directory=True),
            "O:S-1-5-21-123D:P(A;OICI;FA;;;S-1-5-21-123)"
            "(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)",
        )
        self.assertEqual(
            windows_native._managed_security_sddl(sid, directory=False),
            "O:S-1-5-21-123D:P(A;;FA;;;S-1-5-21-123)"
            "(A;;FA;;;SY)(A;;FA;;;BA)",
        )

    def test_local_allocation_release_failure_retains_pointer_for_retry(self) -> None:
        api = _SecurityPolicyApi()
        api.local_free_results = [123, 0]
        allocation = windows_native._LocalAllocation(
            api, ctypes.c_void_p(123), "test allocation"
        )
        with self.assertRaises(OSError):
            allocation.close()
        self.assertTrue(allocation.is_owned)
        allocation.close()
        allocation.close()
        self.assertFalse(allocation.is_owned)
        self.assertEqual(api.events, ["local-free", "local-free"])

    def test_existing_owner_mismatch_never_writes_owner_or_dacl(self) -> None:
        api = _SecurityPolicyApi(owner_matches=False)
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        capability = _security_capability(backend)
        material = _security_material_for_tests()
        snapshot = _security_snapshot_for_tests(dacl_bytes=b"wrong")
        with (
            mock.patch.object(
                backend, "_managed_security_material", return_value=material
            ),
            mock.patch.object(
                backend, "_read_security_snapshot", return_value=snapshot
            ),
            self.assertRaises(PermissionError),
        ):
            backend.verify_managed_security(capability, repair_dacl=True)
        self.assertEqual(api.events, ["equal-owner"])
        self.assertEqual(api.security_writes, [])
        material.close.assert_called_once_with()
        snapshot.close.assert_called_once_with()
        capability.close()

    def test_dacl_mismatch_is_read_only_without_repair(self) -> None:
        api = _SecurityPolicyApi(owner_matches=True)
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        capability = _security_capability(backend)
        material = _security_material_for_tests()
        snapshot = _security_snapshot_for_tests(dacl_bytes=b"wrong")
        with (
            mock.patch.object(
                backend, "_managed_security_material", return_value=material
            ),
            mock.patch.object(
                backend, "_read_security_snapshot", return_value=snapshot
            ),
            self.assertRaises(PermissionError),
        ):
            backend.verify_managed_security(capability, repair_dacl=False)
        self.assertEqual(api.security_writes, [])
        capability.close()

    def test_owner_verified_repair_writes_only_dacl_then_reverifies(self) -> None:
        api = _SecurityPolicyApi(owner_matches=True)
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        capability = _security_capability(backend)
        material = _security_material_for_tests()
        first = _security_snapshot_for_tests(dacl_bytes=b"wrong")
        second = _security_snapshot_for_tests()
        with (
            mock.patch.object(
                backend, "_managed_security_material", return_value=material
            ),
            mock.patch.object(
                backend,
                "_read_security_snapshot",
                side_effect=(first, second),
            ),
        ):
            backend.verify_managed_security(capability, repair_dacl=True)
        self.assertEqual(api.events, ["equal-owner", "set-dacl", "equal-owner"])
        self.assertEqual(len(api.security_writes), 1)
        write = api.security_writes[0]
        self.assertEqual(int(cast(Any, write[2])), windows_native.DACL_SECURITY_INFORMATION | windows_native.PROTECTED_DACL_SECURITY_INFORMATION)
        self.assertFalse(cast(Any, write[3]))
        self.assertFalse(cast(Any, write[4]))
        self.assertEqual(cast(Any, write[5]).value, material.dacl.value)
        capability.close()

    def test_managed_creation_failure_phases_keep_exact_owners(self) -> None:
        metadata = windows_native._Metadata(
            FileIdentity(7, 12),
            FilesystemIdentity(7, 255, 0x4006),
            EntryKind.REGULAR,
            0,
            0,
        )
        cases = ("descriptor", "native", "verification", "material-close")
        for phase in cases:
            with self.subTest(phase=phase):
                api = _SecurityPolicyApi()
                backend = WindowsFilesystemBackend(
                    api=api, osfhandle_opener=lambda _handle, _flags: 0
                )
                parent = _security_capability(
                    backend, kind=EntryKind.DIRECTORY
                )
                assert isinstance(parent, DirectoryCapability)
                material = _security_material_for_tests()
                native_open = mock.Mock(return_value=(313, windows_native.FILE_CREATED))
                finish = mock.Mock(return_value=(metadata, True))
                verify = mock.Mock()
                rollback = mock.Mock()
                if phase == "descriptor":
                    material_factory = mock.Mock(
                        side_effect=OSError("descriptor construction failed")
                    )
                else:
                    material_factory = mock.Mock(return_value=material)
                if phase == "native":
                    native_open.side_effect = OSError("native create failed")
                if phase == "verification":
                    verify.side_effect = OSError("security verification failed")
                if phase == "material-close":
                    material.close.side_effect = (
                        OSError("material close failed"),
                        None,
                    )
                with (
                    mock.patch.object(
                        backend,
                        "_managed_security_material",
                        material_factory,
                    ),
                    mock.patch.object(
                        backend, "_native_relative_open", native_open
                    ),
                    mock.patch.object(
                        backend, "_finish_relative_open", finish
                    ),
                    mock.patch.object(
                        backend,
                        "_verify_managed_security_resource",
                        verify,
                    ),
                    mock.patch.object(
                        backend, "_set_delete_disposition", rollback
                    ),
                    self.assertRaisesRegex(OSError, phase.split("-")[0]),
                ):
                    backend.open_file(
                        parent,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.CREATE_NEW,
                    )
                if phase == "descriptor":
                    native_open.assert_not_called()
                else:
                    material.close.assert_called()
                if phase in {"verification", "material-close"}:
                    rollback.assert_called_once()
                else:
                    rollback.assert_not_called()
                if phase == "material-close":
                    self.assertEqual(backend._failed_security_owners, [material])
                    backend._close_security_owner(
                        backend._failed_security_owners.pop()
                    )
                parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows security APIs")
    def test_security_bindings_have_exact_signatures(self) -> None:
        api = _WindowsApi()
        self.assertEqual(
            api.OpenProcessToken.argtypes,
            (
                windows_native.HANDLE,
                windows_native.ULONG,
                ctypes.POINTER(windows_native.HANDLE),
            ),
        )
        self.assertIs(api.OpenProcessToken.restype, windows_native.BOOL)
        self.assertEqual(
            api.GetTokenInformation.argtypes,
            (
                windows_native.HANDLE,
                windows_native.ULONG,
                ctypes.c_void_p,
                windows_native.ULONG,
                ctypes.POINTER(windows_native.ULONG),
            ),
        )
        self.assertIs(api.GetTokenInformation.restype, windows_native.BOOL)
        self.assertEqual(
            api.ConvertSidToStringSidW.argtypes,
            (
                ctypes.c_void_p,
                ctypes.POINTER(ctypes.c_wchar_p),
            ),
        )
        self.assertIs(api.ConvertSidToStringSidW.restype, windows_native.BOOL)
        self.assertEqual(
            api.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes,
            (
                ctypes.c_wchar_p,
                windows_native.ULONG,
                ctypes.POINTER(ctypes.c_void_p),
                ctypes.POINTER(windows_native.ULONG),
            ),
        )
        self.assertIs(
            api.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype,
            windows_native.BOOL,
        )
        security_query_args = (
            windows_native.HANDLE,
            windows_native.ULONG,
            windows_native.ULONG,
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.POINTER(ctypes.c_void_p),
        )
        self.assertEqual(api.GetSecurityInfo.argtypes, security_query_args)
        self.assertIs(api.GetSecurityInfo.restype, windows_native.ULONG)
        self.assertEqual(
            api.SetSecurityInfo.argtypes,
            (
                windows_native.HANDLE,
                windows_native.ULONG,
                windows_native.ULONG,
                ctypes.c_void_p,
                ctypes.c_void_p,
                ctypes.c_void_p,
                ctypes.c_void_p,
            ),
        )
        self.assertIs(api.SetSecurityInfo.restype, windows_native.ULONG)
        self.assertEqual(
            api.GetNamedSecurityInfoW.argtypes,
            (ctypes.c_wchar_p, *security_query_args[1:]),
        )
        self.assertIs(api.GetNamedSecurityInfoW.restype, windows_native.ULONG)
        self.assertEqual(api.GetSecurityDescriptorLength.argtypes, (ctypes.c_void_p,))
        self.assertIs(api.GetSecurityDescriptorLength.restype, windows_native.ULONG)
        self.assertEqual(
            api.GetSecurityDescriptorControl.argtypes,
            (
                ctypes.c_void_p,
                ctypes.POINTER(windows_native.SECURITY_DESCRIPTOR_CONTROL),
                ctypes.POINTER(windows_native.ULONG),
            ),
        )
        self.assertIs(
            api.GetSecurityDescriptorControl.restype, windows_native.BOOL
        )
        self.assertEqual(
            api.GetSecurityDescriptorDacl.argtypes,
            (
                ctypes.c_void_p,
                ctypes.POINTER(windows_native.BOOL),
                ctypes.POINTER(ctypes.c_void_p),
                ctypes.POINTER(windows_native.BOOL),
            ),
        )
        self.assertIs(api.GetSecurityDescriptorDacl.restype, windows_native.BOOL)
        self.assertEqual(
            api.EqualSid.argtypes,
            (ctypes.c_void_p, ctypes.c_void_p),
        )
        self.assertIs(api.EqualSid.restype, windows_native.BOOL)
        self.assertEqual(api.LocalFree.argtypes, (ctypes.c_void_p,))
        self.assertIs(api.LocalFree.restype, ctypes.c_void_p)
        self.assertEqual(
            api.NtSetInformationFile.argtypes,
            (
                windows_native.HANDLE,
                ctypes.POINTER(windows_native.IO_STATUS_BLOCK),
                ctypes.c_void_p,
                windows_native.ULONG,
                windows_native.ULONG,
            ),
        )
        self.assertIs(api.NtSetInformationFile.restype, windows_native.NTSTATUS)
        self.assertEqual(
            api.SetFileInformationByHandle.argtypes,
            (
                windows_native.HANDLE,
                ctypes.c_int32,
                ctypes.c_void_p,
                windows_native.ULONG,
            ),
        )
        self.assertIs(
            api.SetFileInformationByHandle.restype, windows_native.BOOL
        )

    @unittest.skipUnless(os.name == "nt", "requires Windows security APIs")
    def test_caller_root_security_descriptor_is_byte_identical_after_open(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw)
            before = _security_descriptor_bytes_for_tests(path)
            root = backend.open_root(
                path, SharePolicy.PINNED, SecurityDomain.CALLER
            )
            root.close()
            after = _security_descriptor_bytes_for_tests(path)
        self.assertEqual(after, before)

    @unittest.skipUnless(os.name == "nt", "requires Windows security APIs")
    def test_managed_root_and_children_have_token_owner_and_exact_dacl(self) -> None:
        proxy = WindowsOpenTests._ApiProxy()
        backend = WindowsFilesystemBackend(api=proxy)
        with tempfile.TemporaryDirectory() as raw:
            parent_path = Path(raw)
            parent = backend.open_root(parent_path, SharePolicy.MUTATION)
            caller_file = backend.open_file(
                parent,
                "caller-file",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            caller_file.close()
            managed = backend.create_secure_root(parent, "managed")
            directory = backend.create_directory(
                managed, "directory", SharePolicy.MUTATION
            )
            file = backend.open_file(
                managed,
                "file",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            try:
                self.assertTrue(managed.created)
                self.assertTrue(directory.created)
                self.assertTrue(file.created)
                sid = _token_user_sid_for_tests()
                for path, capability, is_directory in (
                    (parent_path / "managed", managed, True),
                    (parent_path / "managed" / "directory", directory, True),
                    (parent_path / "managed" / "file", file, False),
                ):
                    with self.subTest(path=path):
                        owner, control, dacl = _named_owner_dacl_for_tests(path)
                        self.assertEqual(owner, sid)
                        self.assertTrue(control & windows_native.SE_DACL_PROTECTED)
                        self.assertEqual(
                            dacl,
                            _expected_managed_dacl_for_tests(
                                sid, directory=is_directory
                            ),
                        )
                        backend.verify_managed_security(
                            capability, repair_dacl=False
                        )

                creation_records = {
                    cast(str, record["name"]): record
                    for record in proxy.nt_create_records
                    if record["disposition"]
                    in {windows_native.FILE_CREATE, windows_native.FILE_OPEN_IF}
                }
                self.assertEqual(
                    creation_records["caller-file"]["security_descriptor"], 0
                )
                for name in ("managed", "directory", "file"):
                    self.assertNotEqual(
                        creation_records[name]["security_descriptor"], 0
                    )
            finally:
                file.close()
                directory.close()
                managed.close()
                parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows security APIs")
    def test_managed_publication_uses_token_user_owner_for_every_object(
        self,
    ) -> None:
        from tools.focused_mutation_support import lease as lease_module
        from tools.focused_mutation_support.lease import ManagedScratch

        backend = WindowsFilesystemBackend()
        run_id = "00000000-0000-4000-8000-000000000704"
        with tempfile.TemporaryDirectory() as raw:
            with mock.patch.object(
                lease_module, "reclaim_abandoned", return_value=[]
            ):
                scratch = ManagedScratch.create(
                    Path(raw),
                    run_id=run_id,
                    backend=backend,
                )
            try:
                token_user = _token_user_sid_for_tests()
                token_owner = _token_owner_sid_for_tests()
                objects = (
                    (scratch.managed_root, True),
                    (scratch.managed_root / ".hoimin-coordinator", False),
                    (scratch.path, True),
                    (scratch.path / ".hoimin-lease.json", False),
                    (scratch.path / ".hoimin-heartbeat.json", False),
                )
                observed_owners: dict[Path, str] = {}
                for path, is_directory in objects:
                    with self.subTest(path=path):
                        owner, control, dacl = _named_owner_dacl_for_tests(path)
                        observed_owners[path] = owner
                        self.assertEqual(owner, token_user)
                        self.assertTrue(
                            control & windows_native.SE_DACL_PROTECTED
                        )
                        self.assertEqual(
                            dacl,
                            _expected_managed_dacl_for_tests(
                                token_user, directory=is_directory
                            ),
                        )

                if (
                    _token_is_administrator_for_tests()
                    and token_owner != token_user
                ):
                    for path, owner in observed_owners.items():
                        with self.subTest(
                            path=path, invariant="not-token-owner"
                        ):
                            self.assertNotEqual(owner, token_owner)
            finally:
                self.assertEqual(scratch.close_capabilities(), ())

    @unittest.skipUnless(os.name == "nt", "requires Windows security APIs")
    def test_secure_root_and_managed_open_or_create_use_native_result(self) -> None:
        proxy = WindowsOpenTests._ApiProxy()
        backend = WindowsFilesystemBackend(api=proxy)
        with tempfile.TemporaryDirectory() as raw:
            parent = backend.open_root(Path(raw), SharePolicy.MUTATION)
            first = backend.create_secure_root(parent, "managed")
            self.assertTrue(first.created)
            first.close()
            managed = backend.create_secure_root(parent, "managed")
            self.assertFalse(managed.created)
            created = backend.open_file(
                managed,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
            )
            self.assertTrue(created.created)
            created.close()
            opened = backend.open_file(
                managed,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
            )
            self.assertFalse(opened.created)
            opened.close()
            ordinary = backend.open_file(
                managed,
                "item",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
            )
            last = proxy.nt_create_records[-1]
            self.assertEqual(last["security_descriptor"], 0)
            self.assertFalse(int(last["desired_access"]) & windows_native.WRITE_DAC)
            ordinary.close()
            managed.close()
            parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows security APIs")
    def test_post_create_failure_rolls_back_exact_handle_not_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            item = root_path / "item"
            old = root_path / "old"

            def replace() -> None:
                item.rename(old)
                item.write_bytes(b"replacement")

            proxy = WindowsOpenTests._ApiProxy(after=replace)
            backend = WindowsFilesystemBackend(api=proxy)
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            try:
                with self.assertRaisesRegex(OSError, "identity changed"):
                    backend.open_file(
                        root,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.CREATE_NEW,
                    )
                self.assertFalse(old.exists())
                self.assertEqual(item.read_bytes(), b"replacement")
                self.assertEqual(len(proxy.disposition_calls), 1)
            finally:
                root.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows security APIs")
    def test_rollback_failure_and_close_failure_stay_secondary(self) -> None:
        for failure in ("rollback", "close"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as raw:
                root_path = Path(raw)
                item = root_path / "item"
                old = root_path / "old"
                proxy = WindowsOpenTests._ApiProxy()

                def replace() -> None:
                    item.rename(old)
                    item.write_bytes(b"replacement")
                    if failure == "rollback":
                        proxy.fail_next_disposition = True
                    else:
                        proxy.fail_handle = proxy.last_opened_handle

                proxy._after = replace
                backend = WindowsFilesystemBackend(api=proxy)
                root = backend.open_root(root_path, SharePolicy.MUTATION)
                try:
                    with self.assertRaisesRegex(OSError, "identity changed") as caught:
                        backend.open_file(
                            root,
                            "item",
                            access=FileAccess.READ_WRITE,
                            disposition=CreateDisposition.CREATE_NEW,
                        )
                    notes = " ".join(getattr(caught.exception, "__notes__", ()))
                    if failure == "rollback":
                        self.assertIn("rollback", notes)
                        self.assertTrue(old.exists())
                    else:
                        self.assertIn("close failed", notes)
                        self.assertEqual(len(backend._failed_closes), 1)
                        backend.close_resource(backend._failed_closes.pop())
                        self.assertFalse(old.exists())
                    self.assertEqual(item.read_bytes(), b"replacement")
                finally:
                    root.close()


class _MutationApi:
    def __init__(self) -> None:
        self.events: list[str] = []
        self.rename_calls: list[dict[str, int | str]] = []
        self.disposition_calls: list[dict[str, int]] = []
        self.disposition_outcomes: list[tuple[bool, int]] = []
        self.close_outcomes: list[bool] = []
        self.set_time_outcome = True
        self.flush_outcome = True
        self._last_error = windows_native.ERROR_ACCESS_DENIED

    def NtSetInformationFile(self, *args: object) -> int:
        handle = int(cast(Any, args[0]))
        information = ctypes.cast(
            cast(Any, args[2]), ctypes.POINTER(FILE_RENAME_INFORMATION)
        ).contents
        encoded = ctypes.string_at(
            ctypes.addressof(information) + FILE_RENAME_INFORMATION.FileName.offset,
            int(information.FileNameLength),
        )
        self.events.append("rename")
        self.rename_calls.append(
            {
                "handle": handle,
                "root": int(information.RootDirectory or 0),
                "replace": int(information.ReplaceIfExists),
                "name_length": int(information.FileNameLength),
                "name": encoded.decode("utf-16-le", errors="strict"),
                "buffer_length": int(cast(Any, args[3])),
                "information_class": int(cast(Any, args[4])),
            }
        )
        return 0

    def SetFileInformationByHandle(self, *args: object) -> bool:
        information_class = int(cast(Any, args[1]))
        record = {
            "handle": int(cast(Any, args[0])),
            "information_class": information_class,
            "buffer_length": int(cast(Any, args[3])),
        }
        if information_class == windows_native.FILE_DISPOSITION_INFO_EX_CLASS:
            record["flags"] = int(
                ctypes.cast(
                    cast(Any, args[2]),
                    ctypes.POINTER(windows_native.FILE_DISPOSITION_INFO_EX),
                ).contents.Flags
            )
        else:
            record["delete"] = int(
                ctypes.cast(
                    cast(Any, args[2]),
                    ctypes.POINTER(windows_native.FILE_DISPOSITION_INFO),
                ).contents.DeleteFile
            )
        self.events.append("disposition")
        self.disposition_calls.append(record)
        if self.disposition_outcomes:
            succeeded, code = self.disposition_outcomes.pop(0)
            self._last_error = code
            return succeeded
        return True

    def GetSystemTimeAsFileTime(self, pointer: object) -> None:
        value = ctypes.cast(
            cast(Any, pointer), ctypes.POINTER(windows_native.FILETIME)
        ).contents
        value.dwLowDateTime = 0x89AB_CDEF
        value.dwHighDateTime = 0x0123_4567
        self.events.append("clock")

    def SetFileTime(self, *args: object) -> bool:
        self.events.append("touch")
        self.touch_call = args
        return self.set_time_outcome

    def FlushFileBuffers(self, handle: int) -> bool:
        self.events.append("flush")
        self.flush_handle = handle
        return self.flush_outcome

    def CloseHandle(self, handle: int) -> bool:
        self.events.append("close")
        self.closed_handle = handle
        if self.close_outcomes:
            return self.close_outcomes.pop(0)
        return True

    def RtlNtStatusToDosError(self, status: int) -> int:
        return windows_native.ERROR_ACCESS_DENIED

    def last_error(self) -> int:
        return self._last_error


def _mutation_parent(
    backend: WindowsFilesystemBackend,
    *,
    handle: int,
    identity: FileIdentity,
    path: str,
    security_domain: SecurityDomain = SecurityDomain.CALLER,
) -> DirectoryCapability:
    access = _directory_access(SharePolicy.MUTATION, relative_target=True)
    return DirectoryCapability(
        backend,
        _WindowsResource(
            handle,
            None,
            Path(path).name,
            bool(access & DELETE),
            access,
            _share_mode(SharePolicy.MUTATION),
        ),
        identity=identity,
        filesystem=FilesystemIdentity(31, 255, 0x4006),
        kind=EntryKind.DIRECTORY,
        logical_size=0,
        modified_ns=0,
        security_domain=security_domain,
        share_policy=SharePolicy.MUTATION,
        created=False,
        path_hint=Path(path),
    )


def _mutation_source(
    backend: WindowsFilesystemBackend,
    parent: DirectoryCapability | None,
    *,
    handle: int = 303,
    name: str | None = "source",
    policy: SharePolicy = SharePolicy.PINNED,
    delete_authority: bool = True,
    kind: EntryKind = EntryKind.REGULAR,
    security_domain: SecurityDomain = SecurityDomain.CALLER,
    created: bool = False,
    secure_root_creation: bool = False,
) -> FileCapability | DirectoryCapability:
    path_hint = (
        parent.path_hint / name
        if parent is not None and name
        else Path("C:/source")
    )
    resource = _WindowsResource(
        handle,
        parent,
        name,
        delete_authority,
        DELETE | windows_native.READ_CONTROL,
        _share_mode(policy),
        secure_root_creation=secure_root_creation,
    )
    if kind is EntryKind.DIRECTORY:
        return DirectoryCapability(
            backend,
            resource,
            kind=EntryKind.DIRECTORY,
            identity=FileIdentity(31, 41),
            filesystem=FilesystemIdentity(31, 255, 0x4006),
            logical_size=0,
            modified_ns=0,
            security_domain=security_domain,
            share_policy=policy,
            created=created,
            path_hint=path_hint,
        )
    return FileCapability(
        backend,
        resource,
        kind=kind,
        identity=FileIdentity(31, 41),
        filesystem=FilesystemIdentity(31, 255, 0x4006),
        logical_size=0,
        modified_ns=0,
        security_domain=security_domain,
        share_policy=policy,
        created=created,
        path_hint=path_hint,
    )


def _mutation_metadata(
    capability: FileCapability | DirectoryCapability,
) -> Any:
    return windows_native._Metadata(
        capability.identity,
        capability.filesystem,
        capability.kind,
        capability.logical_size,
        capability.modified_ns,
    )


def _mutation_entry(
    capability: FileCapability | DirectoryCapability,
    name: str,
) -> Any:
    return windows_native.DirectoryEntry(
        name,
        capability.kind,
        capability.identity,
        capability.filesystem,
        capability.logical_size,
        capability.modified_ns,
    )


class WindowsMutationTests(unittest.TestCase):
    def _fixture(
        self,
    ) -> tuple[
        _MutationApi,
        WindowsFilesystemBackend,
        DirectoryCapability,
        DirectoryCapability,
        FileCapability | DirectoryCapability,
    ]:
        api = _MutationApi()
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        source_parent = _mutation_parent(
            backend, handle=301, identity=FileIdentity(31, 51), path="C:/source-parent"
        )
        destination_parent = _mutation_parent(
            backend,
            handle=302,
            identity=FileIdentity(31, 52),
            path="C:/destination-parent",
        )
        source = _mutation_source(backend, source_parent)
        return api, backend, source_parent, destination_parent, source

    def test_rename_is_destination_anchored_and_updates_evidence_after_checks(self) -> None:
        api, backend, source_parent, destination_parent, source = self._fixture()
        metadata = _mutation_metadata(source)
        old_entry = _mutation_entry(source, "source")
        destination_entry = _mutation_entry(source, "x")

        def observe(parent: DirectoryCapability, name: str) -> Any:
            if not api.rename_calls:
                self.assertIs(parent, source_parent)
                self.assertEqual(name, "source")
                return old_entry
            if parent is destination_parent and name == "x":
                return destination_entry
            if parent is source_parent and name == "source":
                return None
            raise AssertionError((parent, name))

        with (
            mock.patch.object(backend, "_metadata", side_effect=(metadata, metadata)),
            mock.patch.object(backend, "entry", side_effect=observe),
        ):
            backend.rename(source, destination_parent, "x", replace=True)

        self.assertEqual(len(api.rename_calls), 1)
        call = api.rename_calls[0]
        self.assertEqual(call["handle"], 303)
        self.assertEqual(call["root"], 302)
        self.assertEqual(call["replace"], 1)
        self.assertEqual(call["name"], "x")
        self.assertEqual(call["name_length"], len("x".encode("utf-16-le")))
        self.assertEqual(
            call["buffer_length"],
            max(
                FILE_RENAME_INFORMATION.FileName.offset
                + len("x".encode("utf-16-le")),
                ctypes.sizeof(FILE_RENAME_INFORMATION),
            ),
        )
        self.assertEqual(
            call["information_class"],
            windows_native.FILE_RENAME_INFORMATION_CLASS,
        )
        resource = backend._resource(source)
        self.assertIs(resource.parent, destination_parent)
        self.assertEqual(resource.name, "x")
        self.assertEqual(source.path_hint, destination_parent.path_hint / "x")
        source.close()
        source_parent.close()
        destination_parent.close()

    def test_rename_rejects_policy_authority_parent_and_name_before_api(self) -> None:
        cases = ("policy", "authority", "absolute", "invalid-name")
        for case in cases:
            with self.subTest(case=case):
                api, backend, source_parent, destination_parent, source = self._fixture()
                if case == "policy":
                    source._share_policy = SharePolicy.MUTATION
                elif case == "authority":
                    backend._resource(source).delete_authority = False
                elif case == "absolute":
                    backend._resource(source).parent = None
                destination_name = "a/b" if case == "invalid-name" else "renamed"
                with (
                    mock.patch.object(backend, "_metadata") as metadata,
                    mock.patch.object(backend, "entry") as entry,
                    self.assertRaises((ValueError, RuntimeError)),
                ):
                    backend.rename(
                        source, destination_parent, destination_name, replace=False
                    )
                self.assertEqual(api.rename_calls, [])
                metadata.assert_not_called()
                entry.assert_not_called()
                source.close()
                source_parent.close()
                destination_parent.close()

    def test_rename_precheck_and_each_postcheck_fail_closed(self) -> None:
        cases = ("handle-before", "entry-before", "handle-after", "destination", "old")
        for case in cases:
            with self.subTest(case=case):
                api, backend, source_parent, destination_parent, source = self._fixture()
                matching = _mutation_metadata(source)
                changed = windows_native._Metadata(
                    FileIdentity(31, 999),
                    source.filesystem,
                    source.kind,
                    0,
                    0,
                )
                metadata_values = (
                    [changed]
                    if case == "handle-before"
                    else [matching, changed]
                    if case == "handle-after"
                    else [matching, matching]
                )

                def observe(parent: DirectoryCapability, name: str) -> Any:
                    if not api.rename_calls:
                        if case == "entry-before":
                            return None
                        return _mutation_entry(source, "source")
                    if parent is destination_parent:
                        if case == "destination":
                            return None
                        return _mutation_entry(source, "renamed")
                    if case == "old":
                        return _mutation_entry(source, "source")
                    return None

                with (
                    mock.patch.object(
                        backend, "_metadata", side_effect=metadata_values
                    ),
                    mock.patch.object(backend, "entry", side_effect=observe),
                    self.assertRaises(OSError),
                ):
                    backend.rename(source, destination_parent, "renamed", replace=False)
                if case in {"handle-before", "entry-before"}:
                    self.assertEqual(api.rename_calls, [])
                else:
                    self.assertEqual(len(api.rename_calls), 1)
                resource = backend._resource(source)
                self.assertIs(resource.parent, source_parent)
                self.assertEqual(resource.name, "source")
                self.assertEqual(source.path_hint, source_parent.path_hint / "source")
                source.close()
                source_parent.close()
                destination_parent.close()

    def test_rename_postcheck_rejects_kind_only_source_change(self) -> None:
        api, backend, source_parent, destination_parent, source = self._fixture()
        before = _mutation_metadata(source)
        changed = windows_native._Metadata(
            source.identity,
            source.filesystem,
            EntryKind.DIRECTORY,
            source.logical_size,
            source.modified_ns,
        )

        with (
            mock.patch.object(backend, "_metadata", side_effect=(before, changed)),
            mock.patch.object(
                backend,
                "entry",
                return_value=_mutation_entry(source, "source"),
            ),
            self.assertRaisesRegex(OSError, "source handle identity changed"),
        ):
            backend.rename(source, destination_parent, "renamed", replace=False)

        self.assertEqual(len(api.rename_calls), 1)
        self.assertIs(backend._resource(source).parent, source_parent)
        source.close()
        source_parent.close()
        destination_parent.close()

    def test_rename_same_entry_skips_old_source_lookup(self) -> None:
        api, backend, source_parent, destination_parent, source = self._fixture()
        matching = _mutation_metadata(source)
        observations: list[tuple[DirectoryCapability, str]] = []

        def observe(parent: DirectoryCapability, name: str) -> Any:
            observations.append((parent, name))
            if len(observations) > 2:
                raise AssertionError("same-entry rename queried the old name")
            return _mutation_entry(source, name)

        with (
            mock.patch.object(backend, "_metadata", side_effect=(matching, matching)),
            mock.patch.object(backend, "entry", side_effect=observe),
        ):
            backend.rename(source, source_parent, "source", replace=False)

        self.assertEqual(
            observations,
            [(source_parent, "source"), (source_parent, "source")],
        )
        self.assertEqual(len(api.rename_calls), 1)
        source.close()
        source_parent.close()
        destination_parent.close()

    def test_rename_checks_old_source_for_each_non_same_entry_axis(self) -> None:
        for case in ("different-name", "different-parent"):
            with self.subTest(case=case):
                api, backend, source_parent, destination_parent, source = self._fixture()
                matching = _mutation_metadata(source)
                target_parent = (
                    source_parent if case == "different-name" else destination_parent
                )
                target_name = "renamed" if case == "different-name" else "source"
                observations = 0

                def observe(parent: DirectoryCapability, name: str) -> Any:
                    nonlocal observations
                    observations += 1
                    if observations == 1:
                        return _mutation_entry(source, "source")
                    if observations == 2:
                        return _mutation_entry(source, target_name)
                    self.assertIs(parent, source_parent)
                    self.assertEqual(name, "source")
                    return _mutation_entry(source, "source")

                with (
                    mock.patch.object(
                        backend, "_metadata", side_effect=(matching, matching)
                    ),
                    mock.patch.object(backend, "entry", side_effect=observe),
                    self.assertRaisesRegex(OSError, "old source entry remains"),
                ):
                    backend.rename(source, target_parent, target_name, replace=False)

                self.assertEqual(observations, 3)
                self.assertEqual(len(api.rename_calls), 1)
                source.close()
                source_parent.close()
                destination_parent.close()

    def test_delete_uses_extended_exact_handle_then_closes_before_absence(self) -> None:
        api, backend, source_parent, destination_parent, source = self._fixture()
        matching = _mutation_metadata(source)
        observations = 0

        def observe(parent: DirectoryCapability, name: str) -> Any:
            nonlocal observations
            self.assertIs(parent, source_parent)
            self.assertEqual(name, "source")
            observations += 1
            api.events.append(f"entry-{observations}")
            return _mutation_entry(source, "source") if observations == 1 else None

        with (
            mock.patch.object(backend, "_metadata", return_value=matching),
            mock.patch.object(backend, "entry", side_effect=observe),
        ):
            backend.delete(source)
        self.assertTrue(source.closed)
        self.assertEqual(len(api.disposition_calls), 1)
        disposition = api.disposition_calls[0]
        self.assertEqual(disposition["handle"], 303)
        self.assertEqual(
            disposition["information_class"],
            windows_native.FILE_DISPOSITION_INFO_EX_CLASS,
        )
        self.assertEqual(
            disposition["flags"],
            windows_native.FILE_DISPOSITION_FLAG_DELETE
            | windows_native.FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | windows_native.FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        )
        self.assertEqual(api.events, ["entry-1", "disposition", "close", "entry-2"])
        source_parent.close()
        destination_parent.close()

    def test_delete_falls_back_only_for_two_compatibility_errors(self) -> None:
        for code in (
            windows_native.ERROR_INVALID_PARAMETER,
            windows_native.ERROR_NOT_SUPPORTED,
        ):
            api, backend, source_parent, destination_parent, source = self._fixture()
            api.disposition_outcomes = [(False, code), (True, 0)]
            observations = iter((_mutation_entry(source, "source"), None))
            with (
                self.subTest(code=code),
                mock.patch.object(
                    backend, "_metadata", return_value=_mutation_metadata(source)
                ),
                mock.patch.object(backend, "entry", side_effect=lambda *_: next(observations)),
            ):
                backend.delete(source)
            self.assertEqual(
                [call["information_class"] for call in api.disposition_calls],
                [
                    windows_native.FILE_DISPOSITION_INFO_EX_CLASS,
                    windows_native.FILE_DISPOSITION_INFO_CLASS,
                ],
            )
            self.assertEqual({call["handle"] for call in api.disposition_calls}, {303})
            source_parent.close()
            destination_parent.close()

        api, backend, source_parent, destination_parent, source = self._fixture()
        api.disposition_outcomes = [
            (False, windows_native.ERROR_ACCESS_DENIED)
        ]
        with (
            mock.patch.object(backend, "_metadata", return_value=_mutation_metadata(source)),
            mock.patch.object(backend, "entry", return_value=_mutation_entry(source, "source")),
            self.assertRaises(PermissionError),
        ):
            backend.delete(source)
        self.assertEqual(len(api.disposition_calls), 1)
        self.assertTrue(source.is_open)
        source.close()
        source_parent.close()
        destination_parent.close()

    def test_delete_close_failure_retries_without_second_disposition(self) -> None:
        api, backend, source_parent, destination_parent, source = self._fixture()
        api.close_outcomes = [False, True]
        observations = iter((_mutation_entry(source, "source"), None))
        with (
            mock.patch.object(backend, "_metadata", return_value=_mutation_metadata(source)),
            mock.patch.object(backend, "entry", side_effect=lambda *_: next(observations)),
        ):
            with self.assertRaises(OSError):
                backend.delete(source)
            self.assertTrue(source.is_open)
            backend.delete(source)
        self.assertTrue(source.closed)
        self.assertEqual(len(api.disposition_calls), 1)
        self.assertEqual(api.events.count("close"), 2)
        source_parent.close()
        destination_parent.close()

    def test_delete_reports_same_name_replacement_without_mutating_it(self) -> None:
        api, backend, source_parent, destination_parent, source = self._fixture()
        replacement = windows_native.DirectoryEntry(
            "source",
            EntryKind.REGULAR,
            FileIdentity(31, 999),
            source.filesystem,
            0,
            0,
        )
        observations = iter((_mutation_entry(source, "source"), replacement))
        with (
            mock.patch.object(backend, "_metadata", return_value=_mutation_metadata(source)),
            mock.patch.object(backend, "entry", side_effect=lambda *_: next(observations)),
            self.assertRaisesRegex(OSError, "replacement|remains"),
        ):
            backend.delete(source)
        self.assertTrue(source.closed)
        self.assertEqual(len(api.disposition_calls), 1)
        source_parent.close()
        destination_parent.close()

    def test_delete_rejects_wrong_policy_authority_or_parent_before_api(self) -> None:
        for case in ("policy", "authority", "parent"):
            api, backend, source_parent, destination_parent, source = self._fixture()
            if case == "policy":
                source._share_policy = SharePolicy.MUTATION
            elif case == "authority":
                backend._resource(source).delete_authority = False
            else:
                backend._resource(source).parent = None
            with (
                self.subTest(case=case),
                mock.patch.object(backend, "_metadata") as metadata,
                mock.patch.object(backend, "entry") as entry,
                self.assertRaises(RuntimeError),
            ):
                backend.delete(source)
            self.assertEqual(api.disposition_calls, [])
            metadata.assert_not_called()
            entry.assert_not_called()
            source.close()
            source_parent.close()
            destination_parent.close()

    def test_delete_allows_only_created_secure_root_mutation_provenance(self) -> None:
        api, backend, source_parent, destination_parent, ordinary = self._fixture()
        ordinary.close()
        source = _mutation_source(
            backend,
            source_parent,
            policy=SharePolicy.MUTATION,
            kind=EntryKind.DIRECTORY,
            security_domain=SecurityDomain.MANAGED,
            created=True,
            secure_root_creation=True,
        )
        observations = iter((_mutation_entry(source, "source"), None))
        with (
            mock.patch.object(backend, "_metadata", return_value=_mutation_metadata(source)),
            mock.patch.object(backend, "entry", side_effect=lambda *_: next(observations)),
        ):
            backend.delete(source)
        self.assertTrue(source.closed)
        self.assertEqual(len(api.disposition_calls), 1)
        source_parent.close()
        destination_parent.close()

        for case in ("replacement", "existing", "managed-child", "caller"):
            with self.subTest(case=case):
                api, backend, source_parent, destination_parent, ordinary = self._fixture()
                ordinary.close()
                source = _mutation_source(
                    backend,
                    source_parent,
                    policy=SharePolicy.MUTATION,
                    kind=EntryKind.DIRECTORY,
                    security_domain=(
                        SecurityDomain.CALLER
                        if case == "caller"
                        else SecurityDomain.MANAGED
                    ),
                    created=case != "existing",
                    secure_root_creation=case not in {"managed-child", "caller"},
                )
                observed = (
                    windows_native.DirectoryEntry(
                        "source",
                        EntryKind.DIRECTORY,
                        FileIdentity(31, 999),
                        source.filesystem,
                        0,
                        0,
                    )
                    if case == "replacement"
                    else _mutation_entry(source, "source")
                )
                with (
                    mock.patch.object(
                        backend,
                        "_metadata",
                        return_value=_mutation_metadata(source),
                    ) as metadata,
                    mock.patch.object(backend, "entry", return_value=observed) as entry,
                    self.assertRaises((OSError, RuntimeError)),
                ):
                    backend.delete(source)
                self.assertEqual(api.disposition_calls, [])
                if case == "replacement":
                    metadata.assert_called_once()
                    entry.assert_called_once()
                    self.assertTrue(
                        backend._resource(source).secure_root_creation
                    )
                    observations = iter(
                        (_mutation_entry(source, "source"), None)
                    )
                    with (
                        mock.patch.object(
                            backend,
                            "_metadata",
                            return_value=_mutation_metadata(source),
                        ),
                        mock.patch.object(
                            backend,
                            "entry",
                            side_effect=lambda *_: next(observations),
                        ),
                    ):
                        backend.delete(source)
                    self.assertTrue(source.closed)
                    self.assertEqual(len(api.disposition_calls), 1)
                else:
                    metadata.assert_not_called()
                    entry.assert_not_called()
                    source.close()
                source_parent.close()
                destination_parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native mutation")
    def test_native_secure_root_delete_exception_is_exact_and_non_reusable(
        self,
    ) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            parent_path = Path(raw)
            parent = backend.open_root(parent_path, SharePolicy.MUTATION)
            try:
                created = backend.create_secure_root(parent, "created")
                self.assertTrue(created.created)
                backend.delete(created)
                self.assertFalse((parent_path / "created").exists())

                existing = backend.create_secure_root(parent, "existing")
                existing.close()
                reopened = backend.create_secure_root(parent, "existing")
                self.assertFalse(reopened.created)
                with self.assertRaises(RuntimeError):
                    backend.delete(reopened)
                self.assertTrue((parent_path / "existing").is_dir())
                reopened.close()
                (parent_path / "existing").rmdir()

                original = backend.create_secure_root(parent, "original")
                (parent_path / "original").rename(parent_path / "displaced")
                (parent_path / "original").mkdir()
                with self.assertRaisesRegex(OSError, "identity changed"):
                    backend.delete(original)
                self.assertTrue((parent_path / "displaced").is_dir())
                self.assertTrue((parent_path / "original").is_dir())
                original.close()
                (parent_path / "original").rmdir()
                (parent_path / "displaced").rmdir()

                managed = backend.create_secure_root(parent, "managed")
                child = backend.create_directory(
                    managed, "child", SharePolicy.MUTATION
                )
                with self.assertRaises(RuntimeError):
                    backend.delete(child)
                self.assertTrue((parent_path / "managed" / "child").is_dir())
                child.close()
                (parent_path / "managed" / "child").rmdir()
                backend.delete(managed)
            finally:
                parent.close()

    def test_touch_and_flush_use_same_regular_handle_and_recheck_identity(self) -> None:
        api, backend, source_parent, destination_parent, source = self._fixture()
        matching = _mutation_metadata(source)
        with mock.patch.object(
            backend, "_metadata", side_effect=(matching, matching, matching, matching)
        ):
            backend.touch(cast(FileCapability, source))
            backend.flush(cast(FileCapability, source))
        touch_call = api.touch_call
        self.assertEqual(int(cast(Any, touch_call[0])), 303)
        self.assertFalse(cast(Any, touch_call[1]))
        self.assertFalse(cast(Any, touch_call[2]))
        self.assertTrue(cast(Any, touch_call[3]))
        written = ctypes.cast(
            cast(Any, touch_call[3]), ctypes.POINTER(windows_native.FILETIME)
        ).contents
        self.assertEqual(written.dwLowDateTime, 0x89AB_CDEF)
        self.assertEqual(written.dwHighDateTime, 0x0123_4567)
        self.assertEqual(api.flush_handle, 303)
        source.close()
        source_parent.close()
        destination_parent.close()

    def test_touch_and_flush_reject_api_or_identity_changes(self) -> None:
        for operation in ("touch-error", "flush-error", "touch-identity", "flush-identity"):
            api, backend, source_parent, destination_parent, source = self._fixture()
            matching = _mutation_metadata(source)
            changed = windows_native._Metadata(
                FileIdentity(31, 999), source.filesystem, source.kind, 0, 0
            )
            if operation == "touch-error":
                api.set_time_outcome = False
                metadata_values = [matching]
            elif operation == "flush-error":
                api.flush_outcome = False
                metadata_values = [matching]
            else:
                metadata_values = [matching, changed]
            with (
                self.subTest(operation=operation),
                mock.patch.object(backend, "_metadata", side_effect=metadata_values),
                self.assertRaises(OSError),
            ):
                if operation.startswith("touch"):
                    backend.touch(cast(FileCapability, source))
                else:
                    backend.flush(cast(FileCapability, source))
            source.close()
            source_parent.close()
            destination_parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native mutation")
    def test_native_rename_is_pinned_relative_and_supports_replacement(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            destination_path = root_path / "destination"
            destination_path.mkdir()
            (root_path / "source").write_bytes(b"source")
            (destination_path / "sentinel").write_bytes(b"sentinel")
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            destination = backend.open_directory(
                root, "destination", SharePolicy.MUTATION
            )
            source = backend.open_entry(root, "source", SharePolicy.PINNED)
            identity = source.identity
            try:
                with self.assertRaises(OSError):
                    (root_path / "source").rename(root_path / "external")
                backend.rename(source, root, "x", replace=False)
                self.assertIsNone(backend.entry(root, "source"))
                same_parent = backend.entry(root, "x")
                assert same_parent is not None
                self.assertEqual(same_parent.identity, identity)
                backend.rename(source, destination, "published", replace=False)
                self.assertEqual(source.identity, identity)
                self.assertIsNone(backend.entry(root, "x"))
                published = backend.entry(destination, "published")
                assert published is not None
                self.assertEqual(published.identity, identity)

                (destination_path / "target").write_bytes(b"sentinel")
                second = root_path / "second"
                second.write_bytes(b"replacement")
                replacement = backend.open_entry(
                    root, "second", SharePolicy.PINNED
                )
                try:
                    with self.assertRaises(OSError):
                        backend.rename(
                            replacement, destination, "target", replace=False
                        )
                    self.assertEqual(
                        (destination_path / "target").read_bytes(), b"sentinel"
                    )
                    backend.rename(
                        replacement, destination, "target", replace=True
                    )
                    replacement.close()
                    self.assertEqual(
                        (destination_path / "target").read_bytes(), b"replacement"
                    )
                finally:
                    replacement.close()
                self.assertEqual(
                    (destination_path / "sentinel").read_bytes(), b"sentinel"
                )
            finally:
                source.close()
                destination.close()
                root.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native mutation")
    def test_native_delete_handles_readonly_empty_directory_reparse_and_pin(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            readonly = root_path / "readonly"
            readonly.write_bytes(b"payload")
            readonly.chmod(0o444)
            empty = root_path / "empty"
            empty.mkdir()
            nonempty = root_path / "nonempty"
            nonempty.mkdir()
            (nonempty / "sentinel").write_bytes(b"sentinel")
            target = root_path / "target"
            target.mkdir()
            (target / "sentinel").write_bytes(b"outside")
            WindowsEnumerationTests()._junction(root_path / "junction", target)
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            try:
                for name in ("readonly", "empty", "junction"):
                    opened = backend.open_entry(root, name, SharePolicy.PINNED)
                    try:
                        if name == "readonly":
                            with self.assertRaises(OSError):
                                (root_path / name).rename(root_path / "external")
                        backend.delete(opened)
                    finally:
                        opened.close()
                    self.assertFalse((root_path / name).exists())
                self.assertEqual((target / "sentinel").read_bytes(), b"outside")

                directory = backend.open_directory(
                    root, "nonempty", SharePolicy.PINNED
                )
                try:
                    with self.assertRaises(OSError):
                        backend.delete(directory)
                    self.assertTrue(directory.is_open)
                finally:
                    directory.close()
                self.assertEqual(
                    (nonempty / "sentinel").read_bytes(), b"sentinel"
                )
            finally:
                root.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native mutation")
    def test_native_touch_flush_preserve_128_bit_identity_and_advance_time(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "heartbeat").write_bytes(b"heartbeat")
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            heartbeat = backend.open_file(
                root,
                "heartbeat",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_EXISTING,
            )
            identity = heartbeat.identity
            before = backend.entry(root, "heartbeat")
            assert before is not None
            backend.touch(heartbeat)
            backend.flush(heartbeat)
            after = backend.entry(root, "heartbeat")
            assert after is not None
            self.assertEqual(after.identity, identity)
            self.assertEqual(heartbeat.identity, identity)
            self.assertGreaterEqual(after.modified_ns, before.modified_ns)
            heartbeat.close()
            root.close()


class WindowsReviewFixTests(unittest.TestCase):
    def test_win32_error_mapping_preserves_errno_and_narrow_types(self) -> None:
        cases = (
            (2, True, FileNotFoundError, errno.ENOENT),
            (2, False, OSError, errno.ENOENT),
            (3, False, OSError, errno.ENOENT),
            (5, False, PermissionError, errno.EACCES),
            (32, False, OSError, errno.EACCES),
            (80, False, OSError, errno.EEXIST),
            (123, False, OSError, errno.EINVAL),
        )
        for code, leaf, expected_type, expected_errno in cases:
            with self.subTest(code=code, leaf=leaf):
                error = _error_from_win32(
                    code,
                    "review error mapping",
                    "x" * 20_000,
                    leaf=leaf,
                )
                self.assertIs(type(error), expected_type)
                self.assertEqual(error.errno, expected_errno)
                self.assertEqual(error.winerror, code)
                self.assertEqual(
                    isinstance(error, FileNotFoundError), code == 2 and leaf
                )
                self.assertLessEqual(len(error.filename.encode("utf-8")), 4_096)

    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_managed_parent_uses_security_only_for_created_files(self) -> None:
        api = WindowsOpenTests._ApiProxy()
        backend = WindowsFilesystemBackend(api=api)
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            managed = backend.create_secure_root(root, "managed")
            try:
                existing = backend.open_file(
                    managed,
                    "existing",
                    access=FileAccess.READ_WRITE,
                    disposition=CreateDisposition.CREATE_NEW,
                )
                existing.close()
                listed = backend.entry(managed, "existing")
                self.assertIsNotNone(listed)
                opened = backend.open_file(
                    managed,
                    "existing",
                    access=FileAccess.READ,
                    disposition=CreateDisposition.OPEN_EXISTING,
                )
                try:
                    assert listed is not None
                    self.assertEqual(opened.identity, listed.identity)
                    self.assertIs(opened.kind, EntryKind.REGULAR)
                    self.assertIs(opened.security_domain, SecurityDomain.MANAGED)
                    record = api.nt_create_records[-1]
                    self.assertEqual(record["security_descriptor"], 0)
                    self.assertFalse(
                        int(record["desired_access"])
                        & windows_native.WRITE_DAC
                    )
                finally:
                    opened.close()

                for name, disposition in (
                    ("create-new", CreateDisposition.CREATE_NEW),
                    ("open-or-create", CreateDisposition.OPEN_OR_CREATE),
                ):
                    with self.subTest(disposition=disposition):
                        calls_before = api.nt_create_calls
                        created = backend.open_file(
                            managed,
                            name,
                            access=FileAccess.READ_WRITE,
                            disposition=disposition,
                        )
                        try:
                            self.assertTrue(created.created)
                            self.assertGreater(api.nt_create_calls, calls_before)
                            records = api.nt_create_records[calls_before:]
                            secured = [
                                record
                                for record in records
                                if record["security_descriptor"]
                            ]
                            self.assertEqual(len(secured), 1)
                            record = secured[0]
                            self.assertNotEqual(record["security_descriptor"], 0)
                            self.assertEqual(
                                bool(
                                    int(record["desired_access"])
                                    & windows_native.WRITE_DAC
                                ),
                                disposition is CreateDisposition.OPEN_OR_CREATE,
                            )
                        finally:
                            created.close()
            finally:
                managed.close()
                root.close()

    def test_rename_header_has_fixed_pointer_width_abi_offset(self) -> None:
        import ctypes

        expected_offset = 20 if ctypes.sizeof(ctypes.c_void_p) == 8 else 12
        self.assertEqual(FILE_RENAME_INFORMATION.FileName.offset, expected_offset)
        encoded = "renamed".encode("utf-16-le")
        allocation_size = FILE_RENAME_INFORMATION.FileName.offset + len(encoded)
        self.assertEqual(allocation_size, expected_offset + len(encoded))


class _Task5ReviewApi(WindowsOpenTests._ApiProxy):
    def __init__(self) -> None:
        super().__init__()
        self.duplicate_calls: list[dict[str, int | bool]] = []
        self.opened_handles: list[dict[str, int | str]] = []
        self.close_calls: list[int] = []
        self.close_failures: dict[int, int] = {}
        self.duplicate_failure = False
        self.last_duplicate: int | None = None

    def NtCreateFile(self, *args: object) -> int:
        status = super().NtCreateFile(*args)
        if status >= 0:
            pointer = ctypes.cast(
                cast(Any, args[0]), ctypes.POINTER(ctypes.c_void_p)
            )
            record = dict(self.nt_create_records[-1])
            record["handle"] = int(pointer.contents.value or 0)
            self.opened_handles.append(record)
        return status

    def DuplicateHandle(self, *args: object) -> bool:
        record: dict[str, int | bool] = {
            "source": int(cast(Any, args[1])),
            "desired_access": int(cast(Any, args[4])),
            "inherit": bool(cast(Any, args[5])),
            "options": int(cast(Any, args[6])),
        }
        self.duplicate_calls.append(record)
        if self.duplicate_failure:
            self._last_error = windows_native.ERROR_ACCESS_DENIED
            return False
        succeeded = bool(self._native.DuplicateHandle(*args))
        if succeeded:
            pointer = ctypes.cast(
                cast(Any, args[3]), ctypes.POINTER(ctypes.c_void_p)
            )
            self.last_duplicate = int(pointer.contents.value or 0)
            record["duplicate"] = self.last_duplicate
        return succeeded

    def CloseHandle(self, handle: int) -> bool:
        value = int(handle)
        self.close_calls.append(value)
        remaining = self.close_failures.get(value, 0)
        if remaining:
            self.close_failures[value] = remaining - 1
            self._last_error = windows_native.ERROR_ACCESS_DENIED
            return False
        return bool(self._native.CloseHandle(handle))


class _Task5ReviewFakeApi:
    def __init__(self) -> None:
        self.after_error: BaseException | None = None
        self.information = windows_native.FILE_CREATED
        self.create_handle = 701
        self.duplicate_handle = 702
        self.duplicate_calls: list[dict[str, int | bool]] = []
        self.close_calls: list[int] = []
        self.close_failures: dict[int, int] = {}
        self.disposition_calls: list[int] = []
        self.disposition_success = True
        self._last_error = windows_native.ERROR_ACCESS_DENIED

    def before_relative_open(self) -> None:
        return None

    def after_relative_open(self) -> None:
        if self.after_error is not None:
            raise self.after_error

    def NtCreateFile(self, *args: object) -> int:
        handle = ctypes.cast(
            cast(Any, args[0]), ctypes.POINTER(ctypes.c_void_p)
        )
        handle.contents.value = self.create_handle
        status = ctypes.cast(
            cast(Any, args[3]), ctypes.POINTER(IO_STATUS_BLOCK)
        )
        status.contents.Information = self.information
        return 0

    def DuplicateHandle(self, *args: object) -> bool:
        self.duplicate_calls.append(
            {
                "source": int(cast(Any, args[1])),
                "desired_access": int(cast(Any, args[4])),
                "inherit": bool(cast(Any, args[5])),
                "options": int(cast(Any, args[6])),
            }
        )
        duplicate = ctypes.cast(
            cast(Any, args[3]), ctypes.POINTER(ctypes.c_void_p)
        )
        duplicate.contents.value = self.duplicate_handle
        return True

    def GetCurrentProcess(self) -> int:
        return 1

    def SetFileInformationByHandle(self, *args: object) -> bool:
        self.disposition_calls.append(int(cast(Any, args[0])))
        return self.disposition_success

    def CloseHandle(self, handle: int) -> bool:
        value = int(handle)
        self.close_calls.append(value)
        remaining = self.close_failures.get(value, 0)
        if remaining:
            self.close_failures[value] = remaining - 1
            return False
        return True

    def RtlNtStatusToDosError(self, _status: int) -> int:
        return self._last_error

    def last_error(self) -> int:
        return self._last_error


class _Task5Round2FakeApi(_Task5ReviewFakeApi):
    def __init__(self, outcomes: list[tuple[str, int]]) -> None:
        super().__init__()
        self.outcomes = list(outcomes)
        self.nt_create_records: list[dict[str, int]] = []

    def NtCreateFile(self, *args: object) -> int:
        attributes = ctypes.cast(
            cast(Any, args[2]), ctypes.POINTER(OBJECT_ATTRIBUTES)
        ).contents
        self.nt_create_records.append(
            {
                "desired_access": int(cast(Any, args[1])),
                "share_mode": int(cast(Any, args[6])),
                "disposition": int(cast(Any, args[7])),
                "security_descriptor": int(attributes.SecurityDescriptor or 0),
            }
        )
        if not self.outcomes:
            raise AssertionError("unexpected NtCreateFile call")
        outcome, value = self.outcomes.pop(0)
        if outcome == "error":
            self._last_error = value
            return -1
        handle = ctypes.cast(
            cast(Any, args[0]), ctypes.POINTER(ctypes.c_void_p)
        )
        handle.contents.value = self.create_handle
        status = ctypes.cast(
            cast(Any, args[3]), ctypes.POINTER(IO_STATUS_BLOCK)
        )
        status.contents.Information = value
        return 0


class WindowsTask5ReviewFixRound2Tests(unittest.TestCase):
    @staticmethod
    def _created_handle(api: _Task5ReviewApi, name: str) -> int:
        records = [
            record
            for record in api.opened_handles
            if record["name"] == name
            and record["disposition"] == windows_native.FILE_CREATE
        ]
        if len(records) != 1:
            raise AssertionError(records)
        return int(records[0]["handle"])

    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_native_created_capability_keeps_actual_authority_without_duplicate(
        self,
    ) -> None:
        api = _Task5ReviewApi()
        backend = WindowsFilesystemBackend(api=api)
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            cases = (
                (
                    "pinned-file",
                    "file",
                    SharePolicy.PINNED,
                    _file_access(FileAccess.READ_WRITE, SharePolicy.PINNED),
                ),
                (
                    "scan-file",
                    "file",
                    SharePolicy.SCAN,
                    _file_access(FileAccess.READ_WRITE, SharePolicy.SCAN),
                ),
                (
                    "scan-directory",
                    "directory",
                    SharePolicy.SCAN,
                    _directory_access(SharePolicy.SCAN, relative_target=True),
                ),
            )
            try:
                for name, kind, policy, final_access in cases:
                    with self.subTest(name=name):
                        duplicate_before = len(api.duplicate_calls)
                        capability: FileCapability | DirectoryCapability
                        if kind == "file":
                            capability = backend.open_file(
                                root,
                                name,
                                access=FileAccess.READ_WRITE,
                                disposition=CreateDisposition.CREATE_NEW,
                                share_policy=policy,
                            )
                        else:
                            capability = backend.create_directory(
                                root, name, policy
                            )
                        try:
                            resource = backend._resource(capability)
                            self.assertEqual(
                                resource.desired_access, final_access | DELETE
                            )
                            self.assertTrue(resource.delete_authority)
                            self.assertEqual(resource.share_mode, _share_mode(policy))
                            creation = [
                                record
                                for record in api.nt_create_records
                                if record["name"] == name
                                and record["disposition"]
                                == windows_native.FILE_CREATE
                            ]
                            self.assertEqual(len(creation), 1)
                            self.assertEqual(
                                creation[0]["desired_access"],
                                final_access | DELETE,
                            )
                            self.assertEqual(
                                resource.handle,
                                self._created_handle(api, name),
                            )
                            self.assertEqual(
                                api.duplicate_calls[duplicate_before:], []
                            )
                        finally:
                            capability.close()
            finally:
                root.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_native_created_delete_access_obeys_share_oracle(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            pinned = backend.open_file(
                root,
                "pinned",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
                share_policy=SharePolicy.PINNED,
            )
            try:
                pinned_resource = backend._resource(pinned)
                self.assertTrue(pinned_resource.delete_authority)
                self.assertEqual(
                    pinned_resource.share_mode,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                )
                inspector = backend.open_file(
                    root,
                    "pinned",
                    access=FileAccess.READ,
                    disposition=CreateDisposition.OPEN_EXISTING,
                    share_policy=SharePolicy.SCAN,
                )
                inspector.close()
                with self.assertRaises(OSError) as caught:
                    backend.open_file(
                        root,
                        "pinned",
                        access=FileAccess.READ,
                        disposition=CreateDisposition.OPEN_EXISTING,
                        share_policy=SharePolicy.PINNED,
                    )
                self.assertEqual(caught.exception.winerror, 32)
            finally:
                pinned.close()

            first = backend.open_file(
                root,
                "pinned",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.PINNED,
            )
            second = backend.open_file(
                root,
                "pinned",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.PINNED,
            )
            second.close()
            first.close()

            scan = backend.open_file(
                root,
                "scan",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
                share_policy=SharePolicy.SCAN,
            )
            second_scan = backend.open_file(
                root,
                "scan",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
                share_policy=SharePolicy.SCAN,
            )
            self.assertTrue(backend._resource(scan).delete_authority)
            self.assertEqual(
                backend._resource(scan).share_mode,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            )
            second_scan.close()
            scan.close()
            root.close()

    def test_open_or_create_records_distinct_created_and_opened_authority(
        self,
    ) -> None:
        for policy in (SharePolicy.PINNED, SharePolicy.SCAN):
            with self.subTest(policy=policy):
                metadata = windows_native._Metadata(
                    FileIdentity(31, 41),
                    FilesystemIdentity(31, 255, 0x4006),
                    EntryKind.REGULAR,
                    0,
                    0,
                )
                observed = windows_native.DirectoryEntry(
                    "item",
                    EntryKind.REGULAR,
                    metadata.identity,
                    metadata.filesystem,
                    0,
                    0,
                )
                final_access = _file_access(FileAccess.READ_WRITE, policy)

                created_api = _Task5Round2FakeApi(
                    [("success", windows_native.FILE_CREATED)]
                )
                created_backend = WindowsFilesystemBackend(
                    api=created_api,
                    osfhandle_opener=lambda _handle, _flags: 0,
                )
                created_parent = _mutation_parent(
                    created_backend,
                    handle=301,
                    identity=FileIdentity(31, 51),
                    path="C:/parent",
                )
                with mock.patch.object(
                    created_backend,
                    "_finish_relative_open",
                    return_value=(metadata, True),
                ):
                    created = created_backend.open_file(
                        created_parent,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.OPEN_OR_CREATE,
                        share_policy=policy,
                    )
                created_resource = created_backend._resource(created)
                self.assertTrue(created.created)
                self.assertEqual(created_resource.handle, created_api.create_handle)
                self.assertEqual(created_resource.desired_access, final_access | DELETE)
                self.assertTrue(created_resource.delete_authority)
                self.assertEqual(created_api.duplicate_calls, [])
                self.assertEqual(
                    created_api.nt_create_records,
                    [
                        {
                            "desired_access": final_access | DELETE,
                            "share_mode": _share_mode(policy),
                            "disposition": windows_native.FILE_CREATE,
                            "security_descriptor": 0,
                        }
                    ],
                )
                created.close()
                created_parent.close()

                opened_api = _Task5Round2FakeApi(
                    [
                        ("error", 80),
                        ("success", windows_native.FILE_OPENED),
                    ]
                )
                opened_backend = WindowsFilesystemBackend(
                    api=opened_api,
                    osfhandle_opener=lambda _handle, _flags: 0,
                )
                opened_parent = _mutation_parent(
                    opened_backend,
                    handle=301,
                    identity=FileIdentity(31, 51),
                    path="C:/parent",
                )
                with (
                    mock.patch.object(
                        opened_backend, "entry", return_value=observed
                    ),
                    mock.patch.object(
                        opened_backend,
                        "_finish_relative_open",
                        return_value=(metadata, False),
                    ),
                ):
                    opened = opened_backend.open_file(
                        opened_parent,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.OPEN_OR_CREATE,
                        share_policy=policy,
                    )
                opened_resource = opened_backend._resource(opened)
                self.assertFalse(opened.created)
                self.assertEqual(opened_resource.handle, opened_api.create_handle)
                self.assertEqual(opened_resource.desired_access, final_access)
                self.assertFalse(opened_resource.delete_authority)
                self.assertEqual(opened_api.duplicate_calls, [])
                self.assertEqual(
                    [record["disposition"] for record in opened_api.nt_create_records],
                    [windows_native.FILE_CREATE, windows_native.FILE_OPEN],
                )
                self.assertEqual(
                    [record["desired_access"] for record in opened_api.nt_create_records],
                    [final_access | DELETE, final_access],
                )
                opened.close()
                opened_parent.close()

    def test_open_or_create_retries_only_collision_disappearance_for_eight_cycles(
        self,
    ) -> None:
        outcomes = [("error", 80), ("error", 2)] * 8
        api = _Task5Round2FakeApi(outcomes)
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        parent = _mutation_parent(
            backend,
            handle=301,
            identity=FileIdentity(31, 51),
            path="C:/parent",
        )
        observed = windows_native.DirectoryEntry(
            "item",
            EntryKind.REGULAR,
            FileIdentity(31, 41),
            parent.filesystem,
            0,
            0,
        )
        with (
            mock.patch.object(backend, "entry", return_value=observed) as entry,
            self.assertRaisesRegex(
                OSError, "^open-or-create entry did not stabilize$"
            ),
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
                share_policy=SharePolicy.PINNED,
            )
        self.assertEqual(len(api.nt_create_records), 16)
        self.assertEqual(
            [record["disposition"] for record in api.nt_create_records],
            [windows_native.FILE_CREATE, windows_native.FILE_OPEN] * 8,
        )
        self.assertEqual(entry.call_count, 8)
        self.assertEqual(api.outcomes, [])
        self.assertEqual(api.disposition_calls, [])
        self.assertEqual(api.close_calls, [])
        parent.close()

    def test_open_or_create_retries_when_collision_entry_disappears(self) -> None:
        api = _Task5Round2FakeApi(
            [("error", 80), ("success", windows_native.FILE_CREATED)]
        )
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        parent = _mutation_parent(
            backend,
            handle=301,
            identity=FileIdentity(31, 51),
            path="C:/parent",
        )
        metadata = windows_native._Metadata(
            FileIdentity(31, 41),
            parent.filesystem,
            EntryKind.REGULAR,
            0,
            0,
        )
        with (
            mock.patch.object(backend, "entry", return_value=None) as entry,
            mock.patch.object(
                backend,
                "_finish_relative_open",
                return_value=(metadata, True),
            ),
        ):
            created = backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
                share_policy=SharePolicy.PINNED,
            )
        self.assertTrue(created.created)
        self.assertEqual(entry.call_count, 1)
        self.assertEqual(
            [record["disposition"] for record in api.nt_create_records],
            [windows_native.FILE_CREATE, windows_native.FILE_CREATE],
        )
        created.close()
        parent.close()

    def test_open_or_create_does_not_retry_other_create_or_open_errors(self) -> None:
        cases = (
            ("create-access-denied", [("error", 5)], 5, 1),
            (
                "open-sharing-violation",
                [("error", 183), ("error", 32)],
                32,
                2,
            ),
        )
        for label, outcomes, expected_error, expected_calls in cases:
            with self.subTest(label=label):
                api = _Task5Round2FakeApi(outcomes)
                backend = WindowsFilesystemBackend(
                    api=api, osfhandle_opener=lambda _handle, _flags: 0
                )
                parent = _mutation_parent(
                    backend,
                    handle=301,
                    identity=FileIdentity(31, 51),
                    path="C:/parent",
                )
                observed = windows_native.DirectoryEntry(
                    "item",
                    EntryKind.REGULAR,
                    FileIdentity(31, 41),
                    parent.filesystem,
                    0,
                    0,
                )
                with (
                    mock.patch.object(backend, "entry", return_value=observed),
                    self.assertRaises(OSError) as caught,
                ):
                    backend.open_file(
                        parent,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.OPEN_OR_CREATE,
                        share_policy=SharePolicy.PINNED,
                    )
                self.assertEqual(caught.exception.winerror, expected_error)
                self.assertEqual(len(api.nt_create_records), expected_calls)
                self.assertEqual(
                    api.nt_create_records[0]["disposition"],
                    windows_native.FILE_CREATE,
                )
                parent.close()

    def test_open_or_create_disappearance_with_close_failure_does_not_retry(
        self,
    ) -> None:
        api = _Task5Round2FakeApi(
            [
                ("error", 80),
                ("success", windows_native.FILE_OPENED),
                ("success", windows_native.FILE_CREATED),
            ]
        )
        api.close_failures[api.create_handle] = 1
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        parent = _mutation_parent(
            backend,
            handle=301,
            identity=FileIdentity(31, 51),
            path="C:/parent",
        )
        metadata = windows_native._Metadata(
            FileIdentity(31, 41),
            parent.filesystem,
            EntryKind.REGULAR,
            0,
            0,
        )
        observed = windows_native.DirectoryEntry(
            "item",
            EntryKind.REGULAR,
            metadata.identity,
            metadata.filesystem,
            0,
            0,
        )
        with (
            mock.patch.object(
                backend, "entry", side_effect=(observed, None)
            ),
            mock.patch.object(backend, "_metadata", return_value=metadata),
            self.assertRaises(FileNotFoundError) as caught,
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
                share_policy=SharePolicy.PINNED,
            )
        self.assertEqual(caught.exception.winerror, 2)
        self.assertEqual(len(api.nt_create_records), 2)
        self.assertEqual(len(api.outcomes), 1)
        self.assertEqual(len(backend._failed_closes), 1)
        notes = list(getattr(caught.exception, "__notes__", ()))
        self.assertEqual(sum("close failed" in note for note in notes), 1)
        backend.close_resource(backend._failed_closes.pop())
        parent.close()

    def test_managed_open_or_create_uses_descriptor_only_for_create_attempt(
        self,
    ) -> None:
        api = _Task5Round2FakeApi(
            [("error", 80), ("success", windows_native.FILE_OPENED)]
        )
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        parent = _mutation_parent(
            backend,
            handle=301,
            identity=FileIdentity(31, 51),
            path="C:/managed",
            security_domain=SecurityDomain.MANAGED,
        )
        metadata = windows_native._Metadata(
            FileIdentity(31, 41),
            parent.filesystem,
            EntryKind.REGULAR,
            0,
            0,
        )
        observed = windows_native.DirectoryEntry(
            "item",
            EntryKind.REGULAR,
            metadata.identity,
            metadata.filesystem,
            0,
            0,
        )
        create_material = _security_material_for_tests()
        open_material = _security_material_for_tests()
        create_material.descriptor = ctypes.c_void_p(111)
        open_material.descriptor = ctypes.c_void_p(222)
        events: list[str] = []
        create_material.close.side_effect = lambda: events.append(
            "create-material-close"
        )
        open_material.close.side_effect = lambda: events.append(
            "open-material-close"
        )

        def verify(*_args: object, **kwargs: object) -> None:
            self.assertTrue(kwargs["repair_dacl"])
            events.append("verify-opened")

        def post_metadata(*_args: object) -> Any:
            events.append("post-metadata")
            return metadata

        with (
            mock.patch.object(
                backend,
                "_managed_security_material",
                side_effect=[create_material, open_material],
            ),
            mock.patch.object(
                backend,
                "_finish_relative_open",
                return_value=(metadata, False),
            ),
            mock.patch.object(
                backend,
                "_verify_managed_security_resource",
                side_effect=verify,
            ),
            mock.patch.object(backend, "_metadata", side_effect=post_metadata),
            mock.patch.object(backend, "entry", return_value=observed),
        ):
            opened = backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
                share_policy=SharePolicy.PINNED,
            )
        self.assertFalse(opened.created)
        self.assertEqual(
            [record["disposition"] for record in api.nt_create_records],
            [windows_native.FILE_CREATE, windows_native.FILE_OPEN],
        )
        self.assertEqual(
            [record["security_descriptor"] for record in api.nt_create_records],
            [111, 0],
        )
        self.assertEqual(
            events,
            [
                "create-material-close",
                "verify-opened",
                "open-material-close",
                "post-metadata",
            ],
        )
        resource = backend._resource(opened)
        self.assertFalse(resource.delete_authority)
        self.assertEqual(
            resource.desired_access,
            _file_access(FileAccess.READ_WRITE, SharePolicy.PINNED)
            | windows_native.WRITE_DAC,
        )
        opened.close()
        parent.close()


    def test_open_or_create_created_failures_rollback_exact_single_owner(
        self,
    ) -> None:
        metadata = windows_native._Metadata(
            FileIdentity(31, 41),
            FilesystemIdentity(31, 255, 0x4006),
            EntryKind.REGULAR,
            0,
            0,
        )
        for domain in (SecurityDomain.CALLER, SecurityDomain.MANAGED):
            with self.subTest(domain=domain):
                api = _Task5Round2FakeApi(
                    [("success", windows_native.FILE_CREATED)]
                )
                backend = WindowsFilesystemBackend(
                    api=api, osfhandle_opener=lambda _handle, _flags: 0
                )
                parent = _mutation_parent(
                    backend,
                    handle=301,
                    identity=FileIdentity(31, 51),
                    path="C:/parent",
                    security_domain=domain,
                )
                entry = mock.Mock(return_value=None)
                finish = mock.patch.object(
                    backend,
                    "_finish_relative_open",
                    return_value=(metadata, True),
                )
                observe = mock.patch.object(backend, "entry", entry)
                material = _security_material_for_tests()
                if domain is SecurityDomain.MANAGED:
                    with (
                        finish,
                        observe,
                        mock.patch.object(
                            backend,
                            "_managed_security_material",
                            return_value=material,
                        ),
                        mock.patch.object(
                            backend,
                            "_verify_managed_security_resource",
                            side_effect=OSError(
                                "managed post-create primary"
                            ),
                        ),
                        self.assertRaisesRegex(
                            OSError, "post-create primary"
                        ),
                    ):
                        backend.open_file(
                            parent,
                            "item",
                            access=FileAccess.READ_WRITE,
                            disposition=CreateDisposition.OPEN_OR_CREATE,
                            share_policy=SharePolicy.PINNED,
                        )
                    material.close.assert_called_once_with()
                else:
                    with (
                        finish,
                        observe,
                        mock.patch.object(
                            windows_native,
                            "FileCapability",
                            side_effect=OSError(
                                "caller post-create primary"
                            ),
                        ),
                        self.assertRaisesRegex(
                            OSError, "post-create primary"
                        ),
                    ):
                        backend.open_file(
                            parent,
                            "item",
                            access=FileAccess.READ_WRITE,
                            disposition=CreateDisposition.OPEN_OR_CREATE,
                            share_policy=SharePolicy.PINNED,
                        )
                self.assertEqual(api.disposition_calls, [api.create_handle])
                self.assertEqual(api.close_calls, [api.create_handle])
                self.assertEqual(api.duplicate_calls, [])
                self.assertEqual(entry.call_count, 1)
                parent.close()


class WindowsTask5ReviewFixRound3Tests(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_native_open_or_create_opened_results_have_final_share_profile(
        self,
    ) -> None:
        read_write_access = (
            windows_native.SYNCHRONIZE
            | windows_native.READ_CONTROL
            | windows_native.FILE_READ_ATTRIBUTES
            | windows_native.FILE_READ_DATA
            | windows_native.FILE_WRITE_DATA
            | windows_native.FILE_WRITE_ATTRIBUTES
        )
        read_access = (
            windows_native.SYNCHRONIZE
            | windows_native.READ_CONTROL
            | windows_native.FILE_READ_ATTRIBUTES
            | windows_native.FILE_READ_DATA
        )
        cases = (
            (
                SecurityDomain.CALLER,
                SharePolicy.PINNED,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            ),
            (
                SecurityDomain.CALLER,
                SharePolicy.SCAN,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ),
            (
                SecurityDomain.MANAGED,
                SharePolicy.PINNED,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            ),
            (
                SecurityDomain.MANAGED,
                SharePolicy.SCAN,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ),
        )
        for domain, policy, expected_share in cases:
            with (
                self.subTest(domain=domain, policy=policy),
                tempfile.TemporaryDirectory() as raw,
            ):
                api = _Task5ReviewApi()
                backend = WindowsFilesystemBackend(api=api)
                root = backend.open_root(Path(raw), SharePolicy.MUTATION)
                managed: DirectoryCapability | None = None
                opened: FileCapability | None = None
                second: FileCapability | None = None
                try:
                    parent = root
                    if domain is SecurityDomain.MANAGED:
                        managed = backend.create_secure_root(root, "managed")
                        parent = managed

                    initial = backend.open_file(
                        parent,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.CREATE_NEW,
                        share_policy=policy,
                    )
                    try:
                        expected_identity = initial.identity
                        expected_kind = initial.kind
                        expected_filesystem = initial.filesystem
                    finally:
                        initial.close()

                    calls_before = len(api.nt_create_records)
                    original_verify = backend._verify_managed_security_resource
                    with mock.patch.object(
                        backend,
                        "_verify_managed_security_resource",
                        wraps=original_verify,
                    ) as verify:
                        opened = backend.open_file(
                            parent,
                            "item",
                            access=FileAccess.READ_WRITE,
                            disposition=CreateDisposition.OPEN_OR_CREATE,
                            share_policy=policy,
                        )

                    expected_open_access = read_write_access
                    if domain is SecurityDomain.MANAGED:
                        expected_open_access |= windows_native.WRITE_DAC
                    opened_records = [
                        record
                        for record in api.nt_create_records[calls_before:]
                        if record["disposition"] == windows_native.FILE_CREATE
                        or (
                            record["disposition"] == windows_native.FILE_OPEN
                            and record["desired_access"]
                            == expected_open_access
                        )
                    ]
                    self.assertEqual(
                        [record["disposition"] for record in opened_records],
                        [windows_native.FILE_CREATE, windows_native.FILE_OPEN],
                    )
                    self.assertEqual(
                        int(opened_records[1]["security_descriptor"]), 0
                    )
                    if domain is SecurityDomain.MANAGED:
                        self.assertNotEqual(
                            int(opened_records[0]["security_descriptor"]), 0
                        )
                        verify.assert_called_once()
                        self.assertTrue(verify.call_args.kwargs["repair_dacl"])
                        self.assertFalse(verify.call_args.kwargs["directory"])
                    else:
                        self.assertEqual(
                            int(opened_records[0]["security_descriptor"]), 0
                        )
                        verify.assert_not_called()

                    opened_resource = backend._resource(opened)
                    self.assertFalse(opened.created)
                    self.assertEqual(opened.identity, expected_identity)
                    self.assertIs(opened.kind, expected_kind)
                    self.assertEqual(opened.filesystem, expected_filesystem)
                    self.assertFalse(opened_resource.delete_authority)
                    self.assertEqual(
                        opened_resource.desired_access, expected_open_access
                    )
                    self.assertEqual(
                        opened_resource.share_mode, expected_share
                    )

                    second = backend.open_file(
                        parent,
                        "item",
                        access=FileAccess.READ,
                        disposition=CreateDisposition.OPEN_EXISTING,
                        share_policy=SharePolicy.PINNED,
                    )
                    second_resource = backend._resource(second)
                    self.assertFalse(second.created)
                    self.assertEqual(second.identity, expected_identity)
                    self.assertIs(second.kind, expected_kind)
                    self.assertEqual(second.filesystem, expected_filesystem)
                    self.assertFalse(second_resource.delete_authority)
                    self.assertEqual(second_resource.desired_access, read_access)
                    self.assertEqual(
                        second_resource.share_mode,
                        FILE_SHARE_READ | FILE_SHARE_WRITE,
                    )

                    second.close()
                    second = None
                    opened.close()
                    opened = None
                finally:
                    if second is not None:
                        second.close()
                    if opened is not None:
                        opened.close()
                    if managed is not None:
                        managed.close()
                    root.close()


class WindowsTask5ReviewFixTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_native_pinned_and_scan_failures_remove_only_created_identity(self) -> None:
        cases = (
            ("pinned-file-metadata", "file", SharePolicy.PINNED, "metadata"),
            ("pinned-file-constructor", "file", SharePolicy.PINNED, "constructor"),
            ("scan-file-metadata", "file", SharePolicy.SCAN, "metadata"),
            ("scan-file-constructor", "file", SharePolicy.SCAN, "constructor"),
            ("scan-directory-metadata", "directory", SharePolicy.SCAN, "metadata"),
            (
                "scan-directory-constructor",
                "directory",
                SharePolicy.SCAN,
                "constructor",
            ),
        )
        for name, kind, policy, failure in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as raw:
                path = Path(raw) / name
                backend = WindowsFilesystemBackend()
                root = backend.open_root(Path(raw), SharePolicy.MUTATION)
                try:
                    if failure == "metadata":
                        patcher = mock.patch.object(
                            backend,
                            "_metadata",
                            side_effect=OSError("post-create metadata failed"),
                        )
                    elif kind == "file":
                        patcher = mock.patch.object(
                            windows_native,
                            "FileCapability",
                            side_effect=OSError("capability construction failed"),
                        )
                    else:
                        patcher = mock.patch.object(
                            windows_native,
                            "DirectoryCapability",
                            side_effect=OSError("capability construction failed"),
                        )
                    with patcher, self.assertRaises(OSError):
                        if kind == "file":
                            backend.open_file(
                                root,
                                name,
                                access=FileAccess.READ_WRITE,
                                disposition=CreateDisposition.CREATE_NEW,
                                share_policy=policy,
                            )
                        else:
                            backend.create_directory(root, name, policy)
                    self.assertFalse(path.exists())
                finally:
                    root.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_native_managed_security_failures_remove_scan_and_pinned_creates(self) -> None:
        cases = (
            ("pinned-file", "file", SharePolicy.PINNED),
            ("scan-file", "file", SharePolicy.SCAN),
            ("scan-directory", "directory", SharePolicy.SCAN),
        )
        for name, kind, policy in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as raw:
                backend = WindowsFilesystemBackend()
                parent = backend.open_root(Path(raw), SharePolicy.MUTATION)
                managed = backend.create_secure_root(parent, "managed")
                path = Path(raw) / "managed" / name
                try:
                    with (
                        mock.patch.object(
                            backend,
                            "_verify_managed_security_resource",
                            side_effect=OSError("managed security failed"),
                        ),
                        self.assertRaisesRegex(OSError, "managed security"),
                    ):
                        if kind == "file":
                            backend.open_file(
                                managed,
                                name,
                                access=FileAccess.READ_WRITE,
                                disposition=CreateDisposition.CREATE_NEW,
                                share_policy=policy,
                            )
                        else:
                            backend.create_directory(managed, name, policy)
                    self.assertFalse(path.exists())
                finally:
                    managed.close()
                    parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native handles")
    def test_after_open_hook_uses_created_information_for_every_policy(self) -> None:
        for policy in (
            SharePolicy.MUTATION,
            SharePolicy.PINNED,
            SharePolicy.SCAN,
        ):
            with self.subTest(policy=policy), tempfile.TemporaryDirectory() as raw:
                api = _Task5ReviewApi()
                api._after = mock.Mock(side_effect=OSError("after hook failed"))
                backend = WindowsFilesystemBackend(api=api)
                root = backend.open_root(Path(raw), SharePolicy.MUTATION)
                path = Path(raw) / policy.value
                try:
                    with self.assertRaisesRegex(OSError, "after hook failed"):
                        backend.open_file(
                            root,
                            policy.value,
                            access=FileAccess.READ_WRITE,
                            disposition=CreateDisposition.CREATE_NEW,
                            share_policy=policy,
                        )
                    self.assertFalse(path.exists())
                finally:
                    root.close()

    def test_created_cleanup_absence_results_are_secondary_once(self) -> None:
        cases: tuple[tuple[str, object, int], ...] = (
            ("absent", None, 0),
            ("observation-error", OSError("absence observation failed"), 1),
            (
                "surviving-original",
                windows_native.DirectoryEntry(
                    "item",
                    EntryKind.REGULAR,
                    FileIdentity(31, 41),
                    FilesystemIdentity(31, 255, 0x4006),
                    0,
                    0,
                ),
                1,
            ),
            (
                "replacement",
                windows_native.DirectoryEntry(
                    "item",
                    EntryKind.REGULAR,
                    FileIdentity(31, 999),
                    FilesystemIdentity(31, 255, 0x4006),
                    0,
                    0,
                ),
                1,
            ),
        )
        for label, outcome, note_count in cases:
            with self.subTest(label=label):
                api = _Task5ReviewFakeApi()
                primary = OSError("after hook primary")
                api.after_error = primary
                backend = WindowsFilesystemBackend(
                    api=api, osfhandle_opener=lambda _handle, _flags: 0
                )
                parent = _mutation_parent(
                    backend,
                    handle=301,
                    identity=FileIdentity(31, 51),
                    path="C:/parent",
                )
                entry = mock.Mock(
                    side_effect=outcome if isinstance(outcome, BaseException) else None,
                    return_value=None if isinstance(outcome, BaseException) else outcome,
                )
                with (
                    mock.patch.object(backend, "entry", entry),
                    self.assertRaisesRegex(OSError, "after hook primary") as caught,
                ):
                    backend.open_file(
                        parent,
                        "item",
                        access=FileAccess.READ_WRITE,
                        disposition=CreateDisposition.CREATE_NEW,
                    )
                self.assertIs(caught.exception, primary)
                self.assertEqual(api.disposition_calls, [api.create_handle])
                self.assertEqual(entry.call_count, 1)
                notes = list(getattr(primary, "__notes__", ()))
                self.assertEqual(len(notes), note_count)
                if notes:
                    self.assertLessEqual(len(notes[0].encode("utf-8")), 4_096)
                parent.close()

    def test_single_owner_close_failure_defers_one_absence_check_until_retry(
        self,
    ) -> None:
        api = _Task5ReviewFakeApi()
        api.close_failures[api.create_handle] = 1
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        parent = _mutation_parent(
            backend,
            handle=301,
            identity=FileIdentity(31, 51),
            path="C:/parent",
        )
        metadata = windows_native._Metadata(
            FileIdentity(31, 41),
            parent.filesystem,
            EntryKind.REGULAR,
            0,
            0,
        )
        entry = mock.Mock(return_value=None)
        with (
            mock.patch.object(
                backend,
                "_finish_relative_open",
                return_value=(metadata, True),
            ),
            mock.patch.object(backend, "entry", entry),
            mock.patch.object(
                windows_native,
                "FileCapability",
                side_effect=OSError("constructor primary"),
            ),
            self.assertRaisesRegex(OSError, "constructor primary") as caught,
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
                share_policy=SharePolicy.PINNED,
            )
        self.assertEqual(entry.call_count, 0)
        with mock.patch.object(backend, "entry", entry):
            backend.__del__()
        self.assertEqual(entry.call_count, 1)
        self.assertEqual(api.close_calls.count(api.create_handle), 2)
        self.assertEqual(api.disposition_calls, [api.create_handle])
        notes = list(getattr(caught.exception, "__notes__", ()))
        self.assertEqual(sum("close failed" in note for note in notes), 1)
        parent.close()

    def test_disposition_failure_closes_owner_without_absence_lookup(self) -> None:
        api = _Task5ReviewFakeApi()
        api.disposition_success = False
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        parent = _mutation_parent(
            backend,
            handle=301,
            identity=FileIdentity(31, 51),
            path="C:/parent",
        )
        metadata = windows_native._Metadata(
            FileIdentity(31, 41),
            parent.filesystem,
            EntryKind.REGULAR,
            0,
            0,
        )
        entry = mock.Mock(return_value=None)
        with (
            mock.patch.object(
                backend, "_finish_relative_open", return_value=(metadata, True)
            ),
            mock.patch.object(backend, "entry", entry),
            mock.patch.object(
                windows_native,
                "FileCapability",
                side_effect=OSError("constructor primary"),
            ),
            self.assertRaisesRegex(OSError, "constructor primary") as caught,
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
                share_policy=SharePolicy.PINNED,
            )
        self.assertEqual(api.disposition_calls, [api.create_handle])
        self.assertEqual(api.close_calls, [api.create_handle])
        self.assertEqual(entry.call_count, 0)
        notes = list(getattr(caught.exception, "__notes__", ()))
        self.assertEqual(sum("rollback failed" in note for note in notes), 1)
        parent.close()

    def test_file_opened_failure_never_sets_creation_disposition(self) -> None:
        api = _Task5Round2FakeApi(
            [("error", 80), ("success", windows_native.FILE_OPENED)]
        )
        backend = WindowsFilesystemBackend(
            api=api, osfhandle_opener=lambda _handle, _flags: 0
        )
        parent = _mutation_parent(
            backend,
            handle=301,
            identity=FileIdentity(31, 51),
            path="C:/parent",
        )
        metadata = windows_native._Metadata(
            FileIdentity(31, 41),
            parent.filesystem,
            EntryKind.REGULAR,
            0,
            0,
        )
        observed = windows_native.DirectoryEntry(
            "item",
            EntryKind.REGULAR,
            metadata.identity,
            metadata.filesystem,
            0,
            0,
        )
        entry = mock.Mock(return_value=observed)
        with (
            mock.patch.object(
                backend, "_finish_relative_open", return_value=(metadata, False)
            ),
            mock.patch.object(backend, "entry", entry),
            mock.patch.object(
                windows_native,
                "FileCapability",
                side_effect=OSError("constructor primary"),
            ),
            self.assertRaisesRegex(OSError, "constructor primary"),
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
                share_policy=SharePolicy.PINNED,
            )
        self.assertEqual(api.disposition_calls, [])
        self.assertEqual(api.close_calls, [api.create_handle])
        self.assertEqual(entry.call_count, 1)
        parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native security")
    def test_managed_security_namespace_replacement_never_returns_stale_evidence(self) -> None:
        cases = (
            ("secure-root-created", "root", CreateDisposition.CREATE_NEW, True),
            ("secure-root-opened", "root", CreateDisposition.OPEN_EXISTING, False),
            ("create-new-created", "file", CreateDisposition.CREATE_NEW, True),
            (
                "open-or-create-created",
                "file",
                CreateDisposition.OPEN_OR_CREATE,
                True,
            ),
            (
                "open-or-create-opened",
                "file",
                CreateDisposition.OPEN_OR_CREATE,
                False,
            ),
        )
        for label, kind, disposition, created_by_call in cases:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as raw:
                backend = WindowsFilesystemBackend()
                root_path = Path(raw)
                parent = backend.open_root(root_path, SharePolicy.MUTATION)
                managed: DirectoryCapability | None = None
                if kind == "root":
                    target = root_path / "managed"
                    if not created_by_call:
                        existing = backend.create_secure_root(parent, "managed")
                        existing.close()
                else:
                    managed = backend.create_secure_root(parent, "managed")
                    target = root_path / "managed" / "item"
                    if not created_by_call:
                        existing_file = backend.open_file(
                            managed,
                            "item",
                            access=FileAccess.READ_WRITE,
                            disposition=CreateDisposition.CREATE_NEW,
                        )
                        existing_file.close()
                        target.write_bytes(b"original")
                old = target.with_name(target.name + "-old")
                original_verify = backend._verify_managed_security_resource

                def verify_then_replace(
                    resource: _WindowsResource,
                    *,
                    directory: bool,
                    component: object,
                    repair_dacl: bool,
                    material: Any | None = None,
                ) -> None:
                    original_verify(
                        resource,
                        directory=directory,
                        component=component,
                        repair_dacl=repair_dacl,
                        material=material,
                    )
                    target.rename(old)
                    if kind == "root":
                        target.mkdir()
                    else:
                        target.write_bytes(b"replacement")

                try:
                    with (
                        mock.patch.object(
                            backend,
                            "_verify_managed_security_resource",
                            side_effect=verify_then_replace,
                        ),
                        self.assertRaises(OSError),
                    ):
                        if kind == "root":
                            backend.create_secure_root(parent, "managed")
                        else:
                            assert managed is not None
                            backend.open_file(
                                managed,
                                "item",
                                access=FileAccess.READ_WRITE,
                                disposition=disposition,
                            )
                    self.assertTrue(target.exists())
                    if kind == "file":
                        self.assertEqual(target.read_bytes(), b"replacement")
                    self.assertEqual(old.exists(), not created_by_call)
                finally:
                    if managed is not None:
                        managed.close()
                    parent.close()

    def test_managed_postsecurity_handle_changes_fail_after_material_cleanup(self) -> None:
        changes = (
            ("identity", FileIdentity(31, 999), FilesystemIdentity(31, 255, 0x4006), EntryKind.REGULAR),
            ("filesystem", FileIdentity(31, 41), FilesystemIdentity(32, 255, 0x4006), EntryKind.REGULAR),
            ("kind", FileIdentity(31, 41), FilesystemIdentity(31, 255, 0x4006), EntryKind.DIRECTORY),
        )
        for label, identity, filesystem, kind in changes:
            with self.subTest(label=label):
                api = _SecurityPolicyApi()
                backend = WindowsFilesystemBackend(
                    api=api, osfhandle_opener=lambda _handle, _flags: 0
                )
                parent = _mutation_parent(
                    backend,
                    handle=301,
                    identity=FileIdentity(31, 51),
                    path="C:/parent",
                )
                before = windows_native._Metadata(
                    FileIdentity(31, 41),
                    FilesystemIdentity(31, 255, 0x4006),
                    EntryKind.REGULAR,
                    0,
                    0,
                )
                after = windows_native._Metadata(
                    identity, filesystem, kind, 0, 0
                )
                material = _security_material_for_tests()
                events: list[str] = []
                material.close.side_effect = lambda: events.append("material-close")

                def post_metadata(*_args: object) -> Any:
                    events.append("post-metadata")
                    return after

                with (
                    mock.patch.object(
                        backend,
                        "_managed_security_material",
                        return_value=material,
                    ),
                    mock.patch.object(
                        backend,
                        "_native_relative_open",
                        return_value=(701, windows_native.FILE_OPENED),
                    ),
                    mock.patch.object(
                        backend,
                        "_finish_relative_open",
                        return_value=(before, False),
                    ),
                    mock.patch.object(
                        backend, "_verify_managed_security_resource"
                    ),
                    mock.patch.object(
                        backend, "_metadata", side_effect=post_metadata
                    ),
                    self.assertRaisesRegex(OSError, "identity|filesystem|kind"),
                ):
                    backend._managed_relative_open(
                        parent=parent,
                        name="item",
                        desired_access=_file_access(
                            FileAccess.READ_WRITE, SharePolicy.MUTATION
                        ),
                        share_policy=SharePolicy.MUTATION,
                        disposition=windows_native.FILE_OPEN_IF,
                        create_options=windows_native.FILE_OPEN_REPARSE_POINT
                        | windows_native.FILE_NON_DIRECTORY_FILE,
                        file_attributes=windows_native.FILE_ATTRIBUTE_NORMAL,
                        expected=None,
                        expected_kind=EntryKind.REGULAR,
                        allowed_information={windows_native.FILE_OPENED},
                    )
                self.assertEqual(events[:2], ["material-close", "post-metadata"])
                self.assertEqual(api.security_writes, [])
                parent.close()

    @unittest.skipUnless(os.name == "nt", "requires Windows native security")
    def test_existing_typed_opens_still_skip_automatic_managed_repair(self) -> None:
        backend = WindowsFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            parent = backend.open_root(Path(raw), SharePolicy.MUTATION)
            managed = backend.create_secure_root(parent, "managed")
            file = backend.open_file(
                managed,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            file.close()
            directory = backend.create_directory(
                managed, "payload", SharePolicy.MUTATION
            )
            directory.close()
            with mock.patch.object(
                backend, "_verify_managed_security_resource"
            ) as verify:
                opened_file = backend.open_file(
                    managed,
                    "item",
                    access=FileAccess.READ,
                    disposition=CreateDisposition.OPEN_EXISTING,
                )
                opened_directory = backend.open_directory(
                    managed, "payload", SharePolicy.SCAN
                )
            verify.assert_not_called()
            opened_file.close()
            opened_directory.close()
            managed.close()
            parent.close()


if os.name == "nt":
    import msvcrt
