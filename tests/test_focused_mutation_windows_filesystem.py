from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Any, cast
from unittest import mock

from tools.focused_mutation_support.filesystem import (
    CreateDisposition,
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

    def test_rename_allocation_uses_the_ctypes_filename_offset(self) -> None:
        import ctypes

        encoded = "renamed".encode("utf-16-le")
        allocation_size = FILE_RENAME_INFORMATION.FileName.offset + len(encoded)
        value = ctypes.create_string_buffer(allocation_size)
        self.assertEqual(len(value), allocation_size)


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


if os.name == "nt":
    import msvcrt
