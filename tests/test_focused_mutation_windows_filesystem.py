from __future__ import annotations

import errno
import gc
import os
import struct
import subprocess
import tempfile
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
            self.fail_next_close = False
            self.fail_handle: int | None = None
            self.last_opened_handle: int | None = None
            self._last_error = 0

        def __getattr__(self, name: str) -> object:
            return getattr(self._native, name)

        def NtCreateFile(self, *args: object) -> int:
            import ctypes

            self.nt_create_calls += 1
            status = self._native.NtCreateFile(*args)
            if status >= 0:
                pointer = ctypes.cast(
                    cast(Any, args[0]), ctypes.POINTER(ctypes.c_void_p)
                )
                self.last_opened_handle = int(pointer.contents.value or 0)
            return status

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
                self.assertEqual(old.read_bytes(), b"")
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
    def test_managed_parent_allows_only_existing_file_open(self) -> None:
        api = WindowsOpenTests._ApiProxy()
        backend = WindowsFilesystemBackend(api=api)
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            (root_path / "existing").write_bytes(b"payload")
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            managed = backend.reopen_directory(root)
            managed._security_domain = SecurityDomain.MANAGED
            try:
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
                finally:
                    opened.close()

                for name, disposition in (
                    ("create-new", CreateDisposition.CREATE_NEW),
                    ("open-or-create", CreateDisposition.OPEN_OR_CREATE),
                ):
                    with self.subTest(disposition=disposition):
                        calls_before = api.nt_create_calls
                        with self.assertRaises(NotImplementedError):
                            backend.open_file(
                                managed,
                                name,
                                access=FileAccess.READ_WRITE,
                                disposition=disposition,
                            )
                        self.assertEqual(api.nt_create_calls, calls_before)
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


if os.name == "nt":
    import msvcrt
