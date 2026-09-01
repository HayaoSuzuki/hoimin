from __future__ import annotations

import builtins
import os
import stat
import sys
import tempfile
import types
import unittest
from collections.abc import Mapping, Sequence
from dataclasses import FrozenInstanceError
from pathlib import Path
from typing import Any, cast
from unittest import mock

from tools.focused_mutation_support.filesystem import (
    CreateDisposition,
    DirectoryCapability,
    DirectoryEntry,
    DirectoryIterator,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemIdentity,
    FilesystemBackend,
    SecurityDomain,
    SharePolicy,
    _reset_default_filesystem_backend_for_tests,
    default_filesystem_backend,
    validate_component,
)
from tools.focused_mutation_support import filesystem as filesystem_module
from tools.focused_mutation_support import posix_filesystem as posix_filesystem_module
from tools.focused_mutation_support.posix_filesystem import (
    PosixFilesystemBackend,
    _PosixResource,
)


def _set_backend_hook(
    backend: PosixFilesystemBackend, name: str, value: object
) -> None:
    setattr(cast(Any, backend), name, value)


class RecordingOwner:
    def __init__(self) -> None:
        self.closed: list[int] = []
        self.detached: list[tuple[int, int]] = []
        self.fail_close = False
        self.fail_detach = False

    def close_resource(self, resource: object) -> None:
        if self.fail_close:
            raise OSError("close failed")
        self.closed.append(int(cast(Any, resource)))

    def detach_file_resource(self, resource: object, flags: int) -> int:
        if self.fail_detach:
            raise OSError("detach failed")
        self.detached.append((int(cast(Any, resource)), flags))
        return int(cast(Any, resource)) + 100


def _test_file_capability(
    owner: object,
    resource: object,
    *,
    identity: FileIdentity = FileIdentity(1, 2),
    filesystem: FilesystemIdentity = FilesystemIdentity(1),
    kind: EntryKind = EntryKind.REGULAR,
    logical_size: int = 3,
) -> FileCapability:
    return FileCapability(
        owner,
        resource,
        identity=identity,
        filesystem=filesystem,
        kind=kind,
        logical_size=logical_size,
        modified_ns=4,
        security_domain=SecurityDomain.MANAGED,
        share_policy=SharePolicy.PINNED,
        created=False,
        path_hint=Path("fixture-file"),
    )


def _test_directory_capability(
    owner: object,
    resource: object,
    *,
    identity: FileIdentity = FileIdentity(1, 2),
    filesystem: FilesystemIdentity = FilesystemIdentity(1),
    kind: EntryKind = EntryKind.DIRECTORY,
    logical_size: int = 0,
) -> DirectoryCapability:
    return DirectoryCapability(
        owner,
        resource,
        identity=identity,
        filesystem=filesystem,
        kind=kind,
        logical_size=logical_size,
        modified_ns=4,
        security_domain=SecurityDomain.MANAGED,
        share_policy=SharePolicy.PINNED,
        created=True,
        path_hint=Path("fixture-directory"),
    )


class FilesystemValueTests(unittest.TestCase):
    def test_identity_values_are_hashable_and_do_not_alias(self) -> None:
        self.assertNotEqual(FileIdentity(7, 11), FileIdentity(7, 12))
        self.assertEqual(
            {FilesystemIdentity(7, 1, 2), FilesystemIdentity(7, 1, 2)},
            {FilesystemIdentity(7, 1, 2)},
        )

    def test_child_component_rejects_namespace_escape(self) -> None:
        invalid = ["", ".", "..", f"a{os.sep}b", "a\0b"]
        if os.altsep is not None:
            invalid.append(f"a{os.altsep}b")
        for value in invalid:
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_component(value)
        self.assertEqual(validate_component(".hoimin-lease.json"), ".hoimin-lease.json")
        if os.name == "posix":
            self.assertEqual(validate_component("a\\b"), "a\\b")

    def test_public_values_are_immutable_and_slotted(self) -> None:
        entry = DirectoryEntry(
            name="child",
            kind=EntryKind.REGULAR,
            identity=FileIdentity(1, 2),
            filesystem=FilesystemIdentity(1),
            logical_size=3,
            modified_ns=4,
        )
        with self.assertRaises(FrozenInstanceError):
            entry.name = "replacement"  # type: ignore[misc]
        with self.assertRaises(AttributeError):
            entry.extra = "unbounded"  # type: ignore[attr-defined]
        self.assertFalse(hasattr(entry, "__dict__"))

    def test_enum_values_match_the_backend_contract(self) -> None:
        self.assertEqual(
            [member.value for member in EntryKind],
            ["directory", "regular", "reparse", "other"],
        )
        self.assertEqual(
            [member.value for member in SharePolicy],
            ["scan", "pinned", "mutation"],
        )
        self.assertEqual(
            [member.value for member in SecurityDomain],
            ["caller", "managed"],
        )
        self.assertEqual(
            [member.value for member in FileAccess],
            ["read", "write", "read_write"],
        )
        self.assertEqual(
            [member.value for member in CreateDisposition],
            ["open_existing", "create_new", "open_or_create"],
        )


class CapabilityOwnershipTests(unittest.TestCase):
    def test_failed_close_keeps_ownership_for_retry(self) -> None:
        owner = RecordingOwner()
        capability = _test_file_capability(owner, 7)
        owner.fail_close = True
        with self.assertRaisesRegex(OSError, "close failed"):
            capability.close()
        self.assertTrue(capability.is_open)
        self.assertFalse(capability.closed)
        owner.fail_close = False
        capability.close()
        capability.close()
        self.assertTrue(capability.closed)
        self.assertFalse(capability.is_open)
        self.assertEqual(owner.closed, [7])

    def test_detach_transfers_once_and_failure_keeps_ownership(self) -> None:
        owner = RecordingOwner()
        capability = _test_file_capability(owner, 9)
        owner.fail_detach = True
        with self.assertRaisesRegex(OSError, "detach failed"):
            capability.detach_to_fd(3)
        self.assertTrue(capability.is_open)
        self.assertFalse(capability.detached)
        owner.fail_detach = False
        self.assertEqual(capability.detach_to_fd(3), 109)
        self.assertTrue(capability.detached)
        self.assertFalse(capability.is_open)
        with self.assertRaises(RuntimeError):
            capability.detach_to_fd(3)
        with self.assertRaises(RuntimeError):
            capability._resource_for(owner)
        capability.close()
        self.assertEqual(owner.closed, [])

    def test_non_regular_capability_cannot_detach(self) -> None:
        for resource, kind in (
            (10, EntryKind.REPARSE),
            (11, EntryKind.OTHER),
        ):
            with self.subTest(kind=kind):
                owner = RecordingOwner()
                capability = _test_file_capability(owner, resource, kind=kind)
                with self.assertRaisesRegex(
                    RuntimeError,
                    "only regular file capabilities can detach to a descriptor",
                ):
                    capability.detach_to_fd(3)
                self.assertEqual(owner.detached, [])
                self.assertTrue(capability.is_open)
                self.assertTrue(capability.owned_by(owner))
                capability.close()
                capability.close()
                self.assertEqual(owner.closed, [resource])

    def test_directory_move_invalidates_source_and_closes_replacement_once(self) -> None:
        owner = RecordingOwner()
        source = _test_directory_capability(owner, 13)
        self.assertTrue(source.owned_by(owner))
        self.assertFalse(source.owned_by(RecordingOwner()))
        moved = source._move_for(owner)
        self.assertTrue(source.transferred)
        self.assertFalse(source.is_open)
        with self.assertRaises(RuntimeError):
            source._resource_for(owner)
        source.close()
        self.assertEqual(owner.closed, [])
        moved.close()
        moved.close()
        self.assertEqual(owner.closed, [13])

    def test_directory_move_constructor_failure_leaves_source_open(self) -> None:
        owner = RecordingOwner()
        source = _test_directory_capability(owner, 17)
        with mock.patch(
            "tools.focused_mutation_support.filesystem.DirectoryCapability",
            side_effect=MemoryError("replacement allocation failed"),
        ):
            with self.assertRaisesRegex(MemoryError, "replacement allocation failed"):
                source._move_for(owner)
        self.assertTrue(source.is_open)
        self.assertFalse(source.transferred)
        self.assertEqual(source._resource_for(owner), 17)
        source.close()
        self.assertEqual(owner.closed, [17])

    def test_wrong_owner_cannot_access_or_move_resource(self) -> None:
        class EqualOwner(RecordingOwner):
            def __eq__(self, other: object) -> bool:
                return True

        owner = EqualOwner()
        equal_but_distinct_owner = EqualOwner()
        capability = _test_directory_capability(owner, 19)
        self.assertFalse(capability.owned_by(equal_but_distinct_owner))
        with self.assertRaises(RuntimeError):
            capability._resource_for(equal_but_distinct_owner)
        with self.assertRaises(RuntimeError):
            capability._move_for(equal_but_distinct_owner)
        self.assertTrue(capability.is_open)
        capability.close()

    def test_metadata_is_read_only_and_no_raw_resource_is_public(self) -> None:
        owner = RecordingOwner()
        capability = _test_file_capability(owner, 23)
        self.assertEqual(capability.identity, FileIdentity(1, 2))
        self.assertEqual(capability.filesystem, FilesystemIdentity(1))
        self.assertIs(capability.kind, EntryKind.REGULAR)
        self.assertEqual(capability.logical_size, 3)
        self.assertEqual(capability.modified_ns, 4)
        self.assertIs(capability.security_domain, SecurityDomain.MANAGED)
        self.assertIs(capability.share_policy, SharePolicy.PINNED)
        self.assertFalse(capability.created)
        self.assertEqual(capability.path_hint, Path("fixture-file"))
        with self.assertRaises(AttributeError):
            capability.path_hint = Path("replacement")  # type: ignore[misc]
        with self.assertRaises(AttributeError):
            _ = capability.resource  # type: ignore[attr-defined]
        capability.close()

    def test_constructor_rejects_invalid_identity_size_and_kind(self) -> None:
        owner = RecordingOwner()
        invalid_cases: list[dict[str, Any]] = [
            {"identity": FileIdentity(0, 2)},
            {"identity": FileIdentity(1, 0)},
            {"filesystem": FilesystemIdentity(0)},
            {"logical_size": -1},
            {"kind": EntryKind.DIRECTORY},
        ]
        for overrides in invalid_cases:
            with self.subTest(overrides=overrides), self.assertRaises(ValueError):
                _test_file_capability(owner, 29, **overrides)
        with self.assertRaises(ValueError):
            _test_directory_capability(owner, 31, kind=EntryKind.REGULAR)
        self.assertEqual(owner.closed, [])

    def test_context_manager_requires_open_capability(self) -> None:
        owner = RecordingOwner()
        capability = _test_file_capability(owner, 37)
        with capability as entered:
            self.assertIs(entered, capability)
        self.assertTrue(capability.closed)
        with self.assertRaises(RuntimeError):
            capability.__enter__()
        self.assertEqual(owner.closed, [37])


class FilesystemProtocolTests(unittest.TestCase):
    def test_protocols_publish_the_complete_structural_contract(self) -> None:
        self.assertEqual(
            {
                name
                for name in DirectoryIterator.__dict__
                if not name.startswith("_") or name in {"__iter__", "__next__"}
            },
            {"directory", "__iter__", "__next__", "close"},
        )
        self.assertEqual(
            {name for name in FilesystemBackend.__dict__ if not name.startswith("_")},
            {
                "directory_rename_requires_closed_descendants",
                "open_root",
                "create_secure_root",
                "reopen_directory",
                "open_directory",
                "create_directory",
                "open_file",
                "open_entry",
                "entry",
                "entries",
                "entries_owned",
                "rename",
                "delete",
                "available_bytes",
                "allocation_unit",
                "touch",
                "flush",
                "final_path",
                "verify_managed_security",
            },
        )

    def test_default_backend_selection_is_lazy_and_cached(self) -> None:
        cases = [
            ("posix", "posix_filesystem", "PosixFilesystemBackend"),
            ("nt", "windows_filesystem", "WindowsFilesystemBackend"),
        ]
        for platform_name, module_name, class_name in cases:
            with self.subTest(platform_name=platform_name):
                qualified_name = (
                    f"tools.focused_mutation_support.{module_name}"
                )
                fake_module = types.ModuleType(qualified_name)

                class FakeBackend:
                    constructions = 0

                    def __init__(self) -> None:
                        type(self).constructions += 1

                setattr(fake_module, class_name, FakeBackend)
                _reset_default_filesystem_backend_for_tests()
                try:
                    with (
                        mock.patch.dict(sys.modules, {qualified_name: fake_module}),
                        mock.patch.object(
                            filesystem_module,
                            "_platform_name",
                            return_value=platform_name,
                        ),
                    ):
                        first = default_filesystem_backend()
                        second = default_filesystem_backend()
                        self.assertIs(first, second)
                        self.assertIsInstance(first, FakeBackend)
                        self.assertEqual(FakeBackend.constructions, 1)
                finally:
                    _reset_default_filesystem_backend_for_tests()

    def test_posix_factory_selection_never_imports_windows_backend(self) -> None:
        original_import = builtins.__import__

        def guarded_import(
            name: str,
            globals: Mapping[str, object] | None = None,
            locals: Mapping[str, object] | None = None,
            fromlist: Sequence[str] = (),
            level: int = 0,
        ) -> types.ModuleType:
            if name.endswith("windows_filesystem"):
                raise AssertionError("POSIX selection imported Windows DLL bindings")
            return original_import(name, globals, locals, fromlist, level)

        _reset_default_filesystem_backend_for_tests()
        try:
            with (
                mock.patch.object(filesystem_module, "_platform_name", return_value="posix"),
                mock.patch.object(builtins, "__import__", side_effect=guarded_import),
            ):
                backend = default_filesystem_backend()
                self.assertIsInstance(backend, PosixFilesystemBackend)
        finally:
            _reset_default_filesystem_backend_for_tests()

    @unittest.skipUnless(os.name == "nt", "requires the native Windows backend")
    def test_windows_factory_selection_constructs_real_backend_lazily(self) -> None:
        _reset_default_filesystem_backend_for_tests()
        try:
            with mock.patch.object(
                filesystem_module, "_platform_name", return_value="nt"
            ):
                first = default_filesystem_backend()
                second = default_filesystem_backend()
                self.assertIs(first, second)
                self.assertEqual(
                    first.__class__.__module__,
                    "tools.focused_mutation_support.windows_filesystem",
                )
                self.assertEqual(first.__class__.__name__, "WindowsFilesystemBackend")
        finally:
            _reset_default_filesystem_backend_for_tests()


@unittest.skipUnless(os.name == "posix", "requires POSIX descriptor primitives")
class PosixBackendTests(unittest.TestCase):
    def test_relative_lifecycle_preserves_identity_and_domain(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            managed = backend.create_secure_root(root, "managed")
            child = backend.create_directory(managed, "child", SharePolicy.MUTATION)
            created = backend.open_file(
                child,
                "marker",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            self.assertTrue(created.created)
            identity = created.identity
            descriptor = created.detach_to_fd(os.O_RDWR)
            os.write(descriptor, b"x")
            os.fsync(descriptor)
            os.close(descriptor)
            reopened = backend.open_entry(child, "marker", SharePolicy.PINNED)
            self.assertFalse(reopened.created)
            self.assertEqual(reopened.identity, identity)
            self.assertIs(reopened.kind, EntryKind.REGULAR)
            self.assertEqual(reopened.security_domain, SecurityDomain.MANAGED)
            assert isinstance(reopened, FileCapability)
            backend.touch(reopened)
            backend.flush(reopened)
            self.assertGreater(backend.allocation_unit(root), 0)
            self.assertGreaterEqual(backend.available_bytes(root), 0)
            self.assertEqual(backend.final_path(root), Path(raw).resolve())
            backend.rename(reopened, child, "renamed", replace=False)
            backend.delete(reopened)
            self.assertIsNone(backend.entry(child, "renamed"))
            child.close()
            managed.close()
            root.close()

    def test_entries_owned_moves_the_source_and_closes_once(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.SCAN)
            source = backend.reopen_directory(root)
            iterator = backend.entries_owned(source)
            self.assertTrue(source.transferred)
            with self.assertRaises(RuntimeError):
                backend.entry(source, "unused")
            self.assertEqual(list(iterator), [])
            iterator.close()
            self.assertTrue(iterator.directory.closed)
            root.close()

    def test_backslash_payload_round_trips_as_one_posix_component(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            created = backend.open_file(
                root,
                r"back\slash",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
            created.close()
            entries = backend.entries(root)
            try:
                self.assertIn(r"back\slash", {entry.name for entry in entries})
            finally:
                entries.close()
            source = backend.open_entry(root, r"back\slash", SharePolicy.PINNED)
            backend.rename(source, root, r"renamed\payload", replace=False)
            backend.delete(source)
            self.assertIsNone(backend.entry(root, r"renamed\payload"))
            root.close()

    def test_open_or_create_reports_native_create_result(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root = backend.open_root(Path(raw), SharePolicy.MUTATION)
            created = backend.open_file(
                root,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
            )
            self.assertTrue(created.created)
            created.close()
            opened = backend.open_file(
                root,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
            )
            self.assertFalse(opened.created)
            opened.close()
            root.close()

    def test_posix_open_rejects_entry_replaced_between_stat_and_open(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            original = root_path / "item"
            displaced = root_path / "displaced"
            original.write_bytes(b"original")
            root = backend.open_root(root_path, SharePolicy.MUTATION)

            def replace(_parent: DirectoryCapability, _name: str) -> None:
                original.rename(displaced)
                original.write_bytes(b"replacement")

            backend._before_relative_open = replace
            with self.assertRaisesRegex(OSError, "identity changed"):
                backend.open_file(
                    root,
                    "item",
                    access=FileAccess.READ,
                    disposition=CreateDisposition.OPEN_EXISTING,
                )
            self.assertEqual(displaced.read_bytes(), b"original")
            self.assertEqual(original.read_bytes(), b"replacement")
            root.close()

    def test_posix_delete_rejects_same_name_replacement(self) -> None:
        backend = PosixFilesystemBackend()
        with tempfile.TemporaryDirectory() as raw:
            root_path = Path(raw)
            original = root_path / "item"
            displaced = root_path / "displaced"
            original.write_bytes(b"original")
            root = backend.open_root(root_path, SharePolicy.MUTATION)
            opened = backend.open_entry(root, "item", SharePolicy.PINNED)
            original.rename(displaced)
            original.write_bytes(b"replacement")
            with self.assertRaisesRegex(OSError, "identity changed"):
                backend.delete(opened)
            self.assertEqual(displaced.read_bytes(), b"original")
            self.assertEqual(original.read_bytes(), b"replacement")
            opened.close()
            root.close()


class PosixBackendSeamTests(unittest.TestCase):
    def _directory(
        self, backend: PosixFilesystemBackend, fd: int
    ) -> DirectoryCapability:
        return DirectoryCapability(
            backend,
            _PosixResource(fd=fd, parent=None, name=None, access=None),
            identity=FileIdentity(1, 2),
            filesystem=FilesystemIdentity(1),
            kind=EntryKind.DIRECTORY,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.MANAGED,
            share_policy=SharePolicy.MUTATION,
            created=False,
            path_hint=Path("fixture"),
        )

    def test_posix_fdopendir_failure_moves_source_and_closes_fd_once(self) -> None:
        backend = PosixFilesystemBackend()
        source = self._directory(backend, 41)
        backend._stream_factory = mock.Mock(side_effect=OSError("fdopendir failed"))
        with mock.patch("os.close") as close:
            with self.assertRaisesRegex(OSError, "fdopendir failed"):
                backend.entries_owned(source)
        self.assertTrue(source.transferred)
        close.assert_called_once_with(41)

    def test_posix_detach_transfers_descriptor_to_caller_close_only(self) -> None:
        backend = PosixFilesystemBackend()
        capability = FileCapability(
            backend,
            _PosixResource(
                fd=42,
                parent=None,
                name=None,
                access=FileAccess.READ_WRITE,
            ),
            identity=FileIdentity(1, 2),
            filesystem=FilesystemIdentity(1),
            kind=EntryKind.REGULAR,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.MANAGED,
            share_policy=SharePolicy.PINNED,
            created=False,
            path_hint=Path("fixture"),
        )
        with mock.patch("os.close") as close:
            descriptor = capability.detach_to_fd(os.O_RDWR)
            capability.close()
            close.assert_not_called()
            os.close(descriptor)
        close.assert_called_once_with(42)

    def test_posix_entries_close_uses_closedir_without_os_close(self) -> None:
        class Stream:
            fd = 43

            def __init__(self) -> None:
                self.close_calls = 0

            def next_name(self) -> str | None:
                return None

            def close(self) -> None:
                self.close_calls += 1

        backend = PosixFilesystemBackend()
        source = self._directory(backend, 43)
        stream = Stream()
        backend._stream_factory = lambda _fd: stream
        with mock.patch("os.close") as close:
            iterator = backend.entries_owned(source)
            self.assertEqual(list(iterator), [])
            iterator.close()
        self.assertTrue(source.transferred)
        self.assertEqual(stream.close_calls, 1)
        close.assert_not_called()

    def test_posix_iterator_construction_failure_closes_moved_stream(self) -> None:
        class Stream:
            fd = 45

            def __init__(self) -> None:
                self.close_calls = 0

            def next_name(self) -> str | None:
                return None

            def close(self) -> None:
                self.close_calls += 1

        backend = PosixFilesystemBackend()
        source = self._directory(backend, 45)
        stream = Stream()
        backend._stream_factory = lambda _fd: stream
        with (
            mock.patch("os.close") as close,
            mock.patch.object(
                posix_filesystem_module,
                "_PosixEntries",
                side_effect=MemoryError("iterator allocation failed"),
            ),
            self.assertRaisesRegex(MemoryError, "iterator allocation failed"),
        ):
            backend.entries_owned(source)
        self.assertTrue(source.transferred)
        self.assertEqual(stream.close_calls, 1)
        close.assert_not_called()

    def test_posix_entries_close_reopened_descriptor_on_decode_error(self) -> None:
        class Stream:
            fd = 47

            def __init__(self) -> None:
                self.close_calls = 0

            def next_name(self) -> str | None:
                raise UnicodeDecodeError("utf-8", b"\xff", 0, 1, "invalid")

            def close(self) -> None:
                self.close_calls += 1

        backend = PosixFilesystemBackend()
        source = self._directory(backend, 47)
        stream = Stream()
        backend._stream_factory = lambda _fd: stream
        with mock.patch("os.close") as close:
            iterator = backend.entries_owned(source)
            with self.assertRaises(UnicodeDecodeError):
                next(iterator)
            iterator.close()
        self.assertEqual(stream.close_calls, 1)
        close.assert_not_called()

    def test_posix_stream_close_failure_retains_retryable_ownership(self) -> None:
        class Stream:
            fd = 53

            def __init__(self) -> None:
                self.close_calls = 0

            def next_name(self) -> str | None:
                return None

            def close(self) -> None:
                self.close_calls += 1
                if self.close_calls == 1:
                    raise OSError("closedir failed")

        backend = PosixFilesystemBackend()
        source = self._directory(backend, 53)
        stream = Stream()
        backend._stream_factory = lambda _fd: stream
        with mock.patch("os.close") as close:
            iterator = backend.entries_owned(source)
            with self.assertRaisesRegex(OSError, "closedir failed"):
                iterator.close()
            self.assertTrue(iterator.directory.is_open)
            iterator.close()
        self.assertEqual(stream.close_calls, 2)
        close.assert_not_called()

    def test_posix_open_or_create_stops_after_eight_unstable_cycles(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 59)
        observed = DirectoryEntry(
            name="item",
            kind=EntryKind.REGULAR,
            identity=FileIdentity(1, 3),
            filesystem=FilesystemIdentity(1),
            logical_size=0,
            modified_ns=0,
        )
        create_attempts = 0

        def unstable_open(
            _parent_fd: int, _name: str, flags: int, _mode: int = 0
        ) -> int:
            nonlocal create_attempts
            if flags & os.O_EXCL:
                create_attempts += 1
                raise FileExistsError("exists")
            raise FileNotFoundError("vanished")

        _set_backend_hook(backend, "_native_open_relative", unstable_open)
        _set_backend_hook(
            backend, "entry", lambda _parent, _name: observed
        )
        with (
            mock.patch("os.close"),
            self.assertRaisesRegex(OSError, "did not stabilize"),
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
            )
        with mock.patch("os.close"):
            parent.close()
        self.assertEqual(create_attempts, 8)

    def test_posix_managed_security_refuses_wrong_owner_before_chmod(self) -> None:
        backend = PosixFilesystemBackend()
        capability = FileCapability(
            backend,
            _PosixResource(
                fd=61,
                parent=None,
                name=None,
                access=FileAccess.READ_WRITE,
            ),
            identity=FileIdentity(1, 2),
            filesystem=FilesystemIdentity(1),
            kind=EntryKind.REGULAR,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.MANAGED,
            share_policy=SharePolicy.PINNED,
            created=False,
            path_hint=Path("fixture"),
        )
        chmod_calls: list[tuple[int, int]] = []
        backend._effective_uid = lambda: 1000
        backend._chmod_resource = lambda resource, mode: chmod_calls.append(
            (resource.fd, mode)
        )
        metadata = types.SimpleNamespace(st_uid=1001, st_mode=stat.S_IFREG | 0o600)
        with mock.patch("os.fstat", return_value=metadata), mock.patch("os.close"):
            with self.assertRaises(PermissionError):
                backend.verify_managed_security(capability, repair_dacl=True)
            capability.close()
        self.assertEqual(chmod_calls, [])


class PosixBackendReviewFixTests(unittest.TestCase):
    def _directory(
        self,
        backend: PosixFilesystemBackend,
        fd: int,
        *,
        identity: FileIdentity = FileIdentity(1, 2),
        parent: DirectoryCapability | None = None,
        name: str | None = None,
        created: bool = False,
    ) -> DirectoryCapability:
        return DirectoryCapability(
            backend,
            _PosixResource(fd=fd, parent=parent, name=name, access=None),
            identity=identity,
            filesystem=FilesystemIdentity(1),
            kind=EntryKind.DIRECTORY,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.MANAGED,
            share_policy=SharePolicy.PINNED,
            created=created,
            path_hint=Path(name or "fixture-directory"),
        )

    def _non_follow_file(
        self,
        backend: PosixFilesystemBackend,
        fd: int,
        parent: DirectoryCapability,
        kind: EntryKind,
        *,
        name: str = "item",
        identity: FileIdentity = FileIdentity(1, 77),
    ) -> FileCapability:
        return FileCapability(
            backend,
            _PosixResource(fd=fd, parent=parent, name=name, access=None),
            identity=identity,
            filesystem=FilesystemIdentity(1),
            kind=kind,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.MANAGED,
            share_policy=SharePolicy.PINNED,
            created=False,
            path_hint=Path(name),
        )

    def _entry(
        self,
        kind: EntryKind,
        *,
        name: str = "item",
        identity: FileIdentity = FileIdentity(1, 77),
    ) -> DirectoryEntry:
        return DirectoryEntry(
            name=name,
            kind=kind,
            identity=identity,
            filesystem=FilesystemIdentity(1),
            logical_size=0,
            modified_ns=0,
        )

    def test_detach_rejects_append_and_truncate_without_transfer(self) -> None:
        for index, unsupported in enumerate((os.O_APPEND, os.O_TRUNC), start=1):
            with self.subTest(unsupported=unsupported):
                backend = PosixFilesystemBackend()
                resource = _PosixResource(
                    fd=70 + index,
                    parent=None,
                    name=None,
                    access=FileAccess.READ_WRITE,
                )
                capability = FileCapability(
                    backend,
                    resource,
                    identity=FileIdentity(1, 10 + index),
                    filesystem=FilesystemIdentity(1),
                    kind=EntryKind.REGULAR,
                    logical_size=0,
                    modified_ns=0,
                    security_domain=SecurityDomain.MANAGED,
                    share_policy=SharePolicy.PINNED,
                    created=False,
                    path_hint=Path("fixture-file"),
                )
                with mock.patch("os.close") as close:
                    with self.assertRaisesRegex(ValueError, "unsupported"):
                        capability.detach_to_fd(os.O_RDWR | unsupported)
                    self.assertTrue(capability.is_open)
                    self.assertEqual(resource.fd, 70 + index)
                    capability.close()
                close.assert_called_once_with(70 + index)

    def test_wrong_observed_file_kinds_never_reach_native_open(self) -> None:
        for kind in (EntryKind.DIRECTORY, EntryKind.REPARSE, EntryKind.OTHER):
            with self.subTest(kind=kind):
                backend = PosixFilesystemBackend()
                parent = self._directory(backend, 81)
                _set_backend_hook(
                    backend,
                    "entry",
                    lambda _parent, _name: self._entry(kind),
                )
                native_calls: list[int] = []
                hook_calls: list[str] = []
                _set_backend_hook(
                    backend,
                    "_before_relative_open",
                    lambda _parent, name: hook_calls.append(name),
                )

                def native_open(
                    _fd: int, _name: str, flags: int, _mode: int = 0
                ) -> int:
                    native_calls.append(flags)
                    return 82

                _set_backend_hook(
                    backend, "_native_open_relative", native_open
                )
                _set_backend_hook(
                    backend,
                    "_new_file_capability",
                    lambda *_args, **_kwargs: object(),
                )
                with (
                    mock.patch("os.close"),
                    self.assertRaisesRegex(OSError, "not a regular file"),
                ):
                    backend.open_file(
                        parent,
                        "item",
                        access=FileAccess.READ,
                        disposition=CreateDisposition.OPEN_EXISTING,
                    )
                self.assertEqual(native_calls, [])
                self.assertEqual(hook_calls, [])
                with mock.patch("os.close"):
                    parent.close()

    def test_open_or_create_wrong_kind_makes_only_exclusive_attempt(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 83)
        _set_backend_hook(
            backend,
            "entry",
            lambda _parent, _name: self._entry(EntryKind.REPARSE),
        )
        native_calls: list[int] = []

        def native_open(_fd: int, _name: str, flags: int, _mode: int = 0) -> int:
            native_calls.append(flags)
            if flags & os.O_EXCL:
                raise FileExistsError("exists")
            return 84

        _set_backend_hook(backend, "_native_open_relative", native_open)
        _set_backend_hook(
            backend,
            "_new_file_capability",
            lambda *_args, **_kwargs: object(),
        )
        with (
            mock.patch("os.close"),
            self.assertRaisesRegex(OSError, "not a regular file"),
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.OPEN_OR_CREATE,
            )
        self.assertEqual(len(native_calls), 1)
        self.assertTrue(native_calls[0] & os.O_EXCL)
        with mock.patch("os.close"):
            parent.close()

    def test_regular_to_fifo_race_opens_nonblocking_then_rejects(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 85)
        _set_backend_hook(
            backend,
            "entry",
            lambda _parent, _name: self._entry(EntryKind.REGULAR),
        )
        hook_calls: list[str] = []
        native_flags: list[int] = []
        _set_backend_hook(
            backend,
            "_before_relative_open",
            lambda _parent, name: hook_calls.append(name),
        )

        def native_open(_fd: int, _name: str, flags: int, _mode: int = 0) -> int:
            native_flags.append(flags)
            return 86

        _set_backend_hook(backend, "_native_open_relative", native_open)
        fifo_metadata = (
            FileIdentity(1, 78),
            FilesystemIdentity(1),
            stat.S_IFIFO | 0o600,
            0,
            0,
        )
        with (
            mock.patch("os.close") as close,
            mock.patch.object(
                posix_filesystem_module,
                "_PROVISIONAL_NONBLOCK",
                0x400000,
                create=True,
            ),
            mock.patch.object(
                posix_filesystem_module,
                "_metadata",
                return_value=fifo_metadata,
            ),
            self.assertRaisesRegex(OSError, "not a regular file"),
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ,
                disposition=CreateDisposition.OPEN_EXISTING,
            )
        self.assertEqual(hook_calls, ["item"])
        self.assertEqual(len(native_flags), 1)
        self.assertTrue(native_flags[0] & 0x400000)
        close.assert_called_once_with(86)
        with mock.patch("os.close"):
            parent.close()

    def test_created_secure_root_failure_never_removes_first_observation(
        self,
    ) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 90)
        identity = FileIdentity(1, 91)
        observed = self._entry(
            EntryKind.DIRECTORY, name="managed", identity=identity
        )
        entry_mock = mock.Mock(return_value=observed)
        _set_backend_hook(backend, "entry", entry_mock)
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 91
        )
        _set_backend_hook(
            backend,
            "verify_managed_security",
            mock.Mock(side_effect=OSError("primary root security failure")),
        )
        metadata = (identity, FilesystemIdentity(1), stat.S_IFDIR | 0o700, 0, 0)
        with (
            mock.patch("os.mkdir"),
            mock.patch("os.rmdir") as rmdir,
            mock.patch("os.close"),
            mock.patch.object(
                posix_filesystem_module, "_metadata", return_value=metadata
            ),
        ):
            with self.assertRaisesRegex(
                OSError, "primary root security failure"
            ) as caught:
                backend.create_secure_root(parent, "managed")
        rmdir.assert_not_called()
        self.assertEqual(entry_mock.call_count, 1)
        self.assertIn(
            "directory creation identity was not atomically bound; "
            "rollback unavailable",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            parent.close()

    def test_created_directory_failure_never_removes_first_observation(
        self,
    ) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 92)
        identity = FileIdentity(1, 93)
        observed = self._entry(
            EntryKind.DIRECTORY, name="child", identity=identity
        )
        entry_mock = mock.Mock(return_value=observed)
        _set_backend_hook(backend, "entry", entry_mock)
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 93
        )
        metadata = (identity, FilesystemIdentity(1), stat.S_IFDIR | 0o700, 0, 0)
        with (
            mock.patch("os.mkdir"),
            mock.patch("os.close"),
            mock.patch("os.rmdir") as rmdir,
            mock.patch.object(
                posix_filesystem_module, "_metadata", return_value=metadata
            ),
            mock.patch.object(
                posix_filesystem_module,
                "DirectoryCapability",
                side_effect=OSError("primary directory construction failure"),
            ),
        ):
            with self.assertRaisesRegex(
                OSError, "primary directory construction failure"
            ) as caught:
                backend.create_directory(parent, "child", SharePolicy.MUTATION)
        rmdir.assert_not_called()
        self.assertEqual(entry_mock.call_count, 1)
        self.assertIn(
            "directory creation identity was not atomically bound; "
            "rollback unavailable",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            parent.close()

    def test_created_file_constructor_failure_rolls_back_and_verifies(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 94)
        identity = FileIdentity(1, 95)
        observed = self._entry(EntryKind.REGULAR, identity=identity)
        entry_mock = mock.Mock(side_effect=(observed, None))
        _set_backend_hook(backend, "entry", entry_mock)
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 95
        )
        raw_metadata = types.SimpleNamespace(
            st_dev=1,
            st_ino=95,
            st_mode=stat.S_IFREG | 0o600,
            st_size=0,
            st_mtime_ns=0,
        )
        with (
            mock.patch("os.close"),
            mock.patch("os.fstat", return_value=raw_metadata),
            mock.patch("os.unlink") as unlink,
            mock.patch.object(
                posix_filesystem_module,
                "_filesystem_identity",
                return_value=FilesystemIdentity(1),
            ),
            mock.patch.object(
                posix_filesystem_module,
                "FileCapability",
                side_effect=OSError("primary file construction failure"),
            ),
            self.assertRaisesRegex(OSError, "primary file construction failure"),
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
        unlink.assert_called_once_with("item", dir_fd=94)
        self.assertEqual(entry_mock.call_count, 2)
        with mock.patch("os.close"):
            parent.close()

    def test_create_rollback_failure_keeps_primary_before_secondary(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 96)
        identity = FileIdentity(1, 97)
        observed = self._entry(EntryKind.REGULAR, identity=identity)
        _set_backend_hook(backend, "entry", mock.Mock(return_value=observed))
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 97
        )
        raw_metadata = types.SimpleNamespace(
            st_dev=1,
            st_ino=97,
            st_mode=stat.S_IFREG | 0o600,
            st_size=0,
            st_mtime_ns=0,
        )
        with (
            mock.patch("os.close"),
            mock.patch("os.fstat", return_value=raw_metadata),
            mock.patch("os.unlink", side_effect=OSError("secondary unlink failure")),
            mock.patch.object(
                posix_filesystem_module,
                "_filesystem_identity",
                return_value=FilesystemIdentity(1),
            ),
            mock.patch.object(
                posix_filesystem_module,
                "FileCapability",
                side_effect=OSError("primary construction failure"),
            ),
        ):
            with self.assertRaisesRegex(
                OSError, "primary construction failure"
            ) as caught:
                backend.open_file(
                    parent,
                    "item",
                    access=FileAccess.READ_WRITE,
                    disposition=CreateDisposition.CREATE_NEW,
                )
        self.assertIn(
            "secondary unlink failure",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            parent.close()

    def test_raw_fstat_failure_makes_create_identity_unavailable(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 98)
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 99
        )
        entry_mock = mock.Mock()
        _set_backend_hook(backend, "entry", entry_mock)
        with (
            mock.patch("os.close"),
            mock.patch("os.unlink") as unlink,
            mock.patch("os.fstat", side_effect=OSError("primary identity unavailable")),
        ):
            with self.assertRaisesRegex(
                OSError, "primary identity unavailable"
            ) as caught:
                backend.open_file(
                    parent,
                    "item",
                    access=FileAccess.READ_WRITE,
                    disposition=CreateDisposition.CREATE_NEW,
                )
        unlink.assert_not_called()
        entry_mock.assert_not_called()
        self.assertIn(
            "identity unavailable for rollback",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            parent.close()

    def test_raw_identity_survives_later_filesystem_query_failure(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 105)
        identity = FileIdentity(1, 106)
        matching = self._entry(EntryKind.REGULAR, identity=identity)
        entry_mock = mock.Mock(side_effect=(matching, None))
        _set_backend_hook(backend, "entry", entry_mock)
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 106
        )
        raw_metadata = types.SimpleNamespace(
            st_dev=1,
            st_ino=106,
            st_mode=stat.S_IFREG | 0o600,
            st_size=0,
            st_mtime_ns=0,
        )
        with (
            mock.patch("os.fstat", return_value=raw_metadata),
            mock.patch("os.close"),
            mock.patch("os.unlink") as unlink,
            mock.patch.object(
                posix_filesystem_module,
                "_filesystem_identity",
                side_effect=OSError("later filesystem query failure"),
            ),
            self.assertRaisesRegex(OSError, "later filesystem query failure"),
        ):
            backend.open_file(
                parent,
                "item",
                access=FileAccess.READ_WRITE,
                disposition=CreateDisposition.CREATE_NEW,
            )
        unlink.assert_called_once_with("item", dir_fd=105)
        self.assertEqual(entry_mock.call_count, 2)
        with mock.patch("os.close"):
            parent.close()

    def test_created_file_filesystem_mismatch_preserves_entry(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 107)
        identity = FileIdentity(1, 108)
        mismatch = DirectoryEntry(
            name="item",
            kind=EntryKind.REGULAR,
            identity=identity,
            filesystem=FilesystemIdentity(999),
            logical_size=0,
            modified_ns=0,
        )
        _set_backend_hook(backend, "entry", mock.Mock(return_value=mismatch))
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 108
        )
        raw_metadata = types.SimpleNamespace(
            st_dev=1,
            st_ino=108,
            st_mode=stat.S_IFREG | 0o600,
            st_size=0,
            st_mtime_ns=0,
        )
        with (
            mock.patch("os.fstat", return_value=raw_metadata),
            mock.patch("os.close"),
            mock.patch("os.unlink") as unlink,
            mock.patch.object(
                posix_filesystem_module,
                "_filesystem_identity",
                return_value=FilesystemIdentity(1),
            ),
            mock.patch.object(
                posix_filesystem_module,
                "FileCapability",
                side_effect=OSError("primary construction failure"),
            ),
        ):
            with self.assertRaisesRegex(
                OSError, "primary construction failure"
            ) as caught:
                backend.open_file(
                    parent,
                    "item",
                    access=FileAccess.READ_WRITE,
                    disposition=CreateDisposition.CREATE_NEW,
                )
        unlink.assert_not_called()
        self.assertIn(
            "rollback refused changed creation evidence",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            parent.close()

    def test_created_cleanup_baseexceptions_remain_secondary(self) -> None:
        class CleanupSignal(BaseException):
            pass

        cases = ("reobserve", "removal", "absence")
        for offset, phase in enumerate(cases):
            with self.subTest(phase=phase):
                backend = PosixFilesystemBackend()
                parent_fd = 160 + offset * 10
                file_fd = parent_fd + 1
                parent = self._directory(backend, parent_fd)
                identity = FileIdentity(1, file_fd)
                matching = self._entry(EntryKind.REGULAR, identity=identity)
                signal = CleanupSignal(f"{phase} cleanup baseexception")
                if phase == "reobserve":
                    entry_hook = mock.Mock(side_effect=signal)
                elif phase == "removal":
                    entry_hook = mock.Mock(return_value=matching)
                else:
                    entry_hook = mock.Mock(side_effect=(matching, signal))
                _set_backend_hook(backend, "entry", entry_hook)
                _set_backend_hook(
                    backend,
                    "_native_open_relative",
                    lambda *_args, value=file_fd, **_kwargs: value,
                )
                raw_metadata = types.SimpleNamespace(
                    st_dev=1,
                    st_ino=file_fd,
                    st_mode=stat.S_IFREG | 0o600,
                    st_size=0,
                    st_mtime_ns=0,
                )
                unlink_effect = signal if phase == "removal" else None
                with (
                    mock.patch("os.fstat", return_value=raw_metadata),
                    mock.patch("os.close"),
                    mock.patch("os.unlink", side_effect=unlink_effect),
                    mock.patch.object(
                        posix_filesystem_module,
                        "_filesystem_identity",
                        return_value=FilesystemIdentity(1),
                    ),
                    mock.patch.object(
                        posix_filesystem_module,
                        "FileCapability",
                        side_effect=OSError("primary construction failure"),
                    ),
                ):
                    with self.assertRaisesRegex(
                        OSError, "primary construction failure"
                    ) as caught:
                        backend.open_file(
                            parent,
                            "item",
                            access=FileAccess.READ_WRITE,
                            disposition=CreateDisposition.CREATE_NEW,
                        )
                self.assertIn(
                    f"{phase} cleanup baseexception",
                    " ".join(getattr(caught.exception, "__notes__", ())),
                )
                with mock.patch("os.close"):
                    parent.close()

    def test_created_owner_close_baseexception_remains_secondary(self) -> None:
        class CleanupSignal(BaseException):
            pass

        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 190)
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 191
        )
        raw_metadata = types.SimpleNamespace(
            st_dev=1,
            st_ino=191,
            st_mode=stat.S_IFREG | 0o600,
            st_size=0,
            st_mtime_ns=0,
        )

        def close_created(fd: int) -> None:
            if fd == 191:
                raise CleanupSignal("close cleanup baseexception")

        with (
            mock.patch("os.fstat", return_value=raw_metadata),
            mock.patch("os.close", side_effect=close_created),
            mock.patch("os.unlink") as unlink,
            mock.patch.object(
                posix_filesystem_module,
                "_filesystem_identity",
                return_value=FilesystemIdentity(1),
            ),
            mock.patch.object(
                posix_filesystem_module,
                "FileCapability",
                side_effect=OSError("primary construction failure"),
            ),
        ):
            with self.assertRaisesRegex(
                OSError, "primary construction failure"
            ) as caught:
                backend.open_file(
                    parent,
                    "item",
                    access=FileAccess.READ_WRITE,
                    disposition=CreateDisposition.CREATE_NEW,
                )
        unlink.assert_not_called()
        self.assertIn(
            "close cleanup baseexception",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            parent.close()

    def test_existing_capability_close_baseexception_remains_secondary(self) -> None:
        class CleanupSignal(BaseException):
            pass

        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 192)
        existing = self._entry(
            EntryKind.DIRECTORY,
            name="managed",
            identity=FileIdentity(1, 193),
        )
        opened = self._directory(
            backend,
            193,
            identity=existing.identity,
            parent=parent,
            name="managed",
        )
        _set_backend_hook(backend, "entry", mock.Mock(return_value=existing))
        _set_backend_hook(
            backend,
            "_open_observed_directory",
            mock.Mock(return_value=opened),
        )
        _set_backend_hook(
            backend,
            "verify_managed_security",
            mock.Mock(side_effect=OSError("primary security failure")),
        )

        def close_opened(fd: int) -> None:
            if fd == 193:
                raise CleanupSignal("capability close baseexception")

        with (
            mock.patch("os.mkdir", side_effect=FileExistsError("exists")),
            mock.patch("os.close", side_effect=close_opened),
        ):
            with self.assertRaisesRegex(OSError, "primary security failure") as caught:
                backend.create_secure_root(parent, "managed")
        self.assertIn(
            "capability close baseexception",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            opened.close()
            parent.close()

    def test_replacement_before_first_directory_observation_is_not_removed(
        self,
    ) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 100)
        replacement = self._entry(
            EntryKind.DIRECTORY,
            name="child",
            identity=FileIdentity(1, 102),
        )
        _set_backend_hook(
            backend, "entry", mock.Mock(return_value=replacement)
        )
        _set_backend_hook(
            backend, "_native_open_relative", lambda *_args, **_kwargs: 101
        )
        metadata = (
            replacement.identity,
            FilesystemIdentity(1),
            stat.S_IFDIR | 0o700,
            0,
            0,
        )
        with (
            mock.patch("os.mkdir"),
            mock.patch("os.close"),
            mock.patch("os.rmdir") as rmdir,
            mock.patch.object(
                posix_filesystem_module, "_metadata", return_value=metadata
            ),
            mock.patch.object(
                posix_filesystem_module,
                "DirectoryCapability",
                side_effect=OSError("primary directory construction failure"),
            ),
        ):
            with self.assertRaisesRegex(
                OSError, "primary directory construction failure"
            ) as caught:
                backend.create_directory(parent, "child", SharePolicy.MUTATION)
        rmdir.assert_not_called()
        self.assertIn(
            "directory creation identity was not atomically bound; "
            "rollback unavailable",
            " ".join(getattr(caught.exception, "__notes__", ())),
        )
        with mock.patch("os.close"):
            parent.close()

    def test_existing_secure_root_failure_is_never_removed(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 103)
        identity = FileIdentity(1, 104)
        existing = self._entry(
            EntryKind.DIRECTORY, name="managed", identity=identity
        )
        entry_mock = mock.Mock(return_value=existing)
        _set_backend_hook(backend, "entry", entry_mock)
        _set_backend_hook(
            backend,
            "_open_observed_directory",
            mock.Mock(side_effect=OSError("existing root open failure")),
        )
        with (
            mock.patch("os.mkdir", side_effect=FileExistsError("exists")),
            mock.patch("os.rmdir") as rmdir,
            mock.patch("os.close"),
            self.assertRaisesRegex(OSError, "existing root open failure"),
        ):
            backend.create_secure_root(parent, "managed")
        rmdir.assert_not_called()
        self.assertEqual(entry_mock.call_count, 1)
        with mock.patch("os.close"):
            parent.close()

    def test_non_follow_delete_uses_owned_parent_duplicate(self) -> None:
        cases = (
            (EntryKind.REPARSE, stat.S_IFLNK),
            (EntryKind.OTHER, stat.S_IFIFO),
        )
        for offset, (kind, mode) in enumerate(cases):
            with self.subTest(kind=kind):
                backend = PosixFilesystemBackend()
                parent_fd = 110 + offset * 10
                surrogate_fd = parent_fd + 1
                parent = self._directory(backend, parent_fd)
                source = self._non_follow_file(
                    backend, surrogate_fd, parent, kind
                )
                stat_fds: list[int] = []

                def stat_entry(
                    _name: str, *, dir_fd: int, follow_symlinks: bool
                ) -> object:
                    self.assertFalse(follow_symlinks)
                    stat_fds.append(dir_fd)
                    if len(stat_fds) == 1:
                        return types.SimpleNamespace(
                            st_dev=1,
                            st_ino=77,
                            st_mode=mode,
                            st_size=0,
                            st_mtime_ns=0,
                        )
                    raise FileNotFoundError("absent")

                with (
                    mock.patch("os.stat", side_effect=stat_entry),
                    mock.patch("os.unlink") as unlink,
                    mock.patch("os.close"),
                    mock.patch.object(
                        posix_filesystem_module,
                        "_metadata",
                        return_value=(
                            parent.identity,
                            parent.filesystem,
                            stat.S_IFDIR | 0o700,
                            0,
                            0,
                        ),
                    ),
                ):
                    backend.delete(source)
                    unlink.assert_called_once_with("item", dir_fd=surrogate_fd)
                    self.assertEqual(stat_fds, [surrogate_fd, parent_fd])
                    parent.close()

    def test_dead_original_parent_prevents_non_follow_namespace_syscall(self) -> None:
        backend = PosixFilesystemBackend()
        parent = self._directory(backend, 130)
        source = self._non_follow_file(
            backend, 131, parent, EntryKind.REPARSE
        )
        with mock.patch("os.close"):
            parent.close()
            with (
                mock.patch("os.stat") as stat_entry,
                mock.patch("os.unlink") as unlink,
                self.assertRaises(RuntimeError),
            ):
                backend.delete(source)
            stat_entry.assert_not_called()
            unlink.assert_not_called()
            source.close()

    def test_non_follow_rename_directly_refreshes_owned_surrogate(self) -> None:
        backend = PosixFilesystemBackend()
        source_parent = self._directory(backend, 140)
        destination_parent = self._directory(
            backend, 141, identity=FileIdentity(1, 20)
        )
        source = self._non_follow_file(
            backend, 142, source_parent, EntryKind.REPARSE
        )
        stat_fds: list[int] = []

        def stat_entry(
            _name: str, *, dir_fd: int, follow_symlinks: bool
        ) -> object:
            self.assertFalse(follow_symlinks)
            stat_fds.append(dir_fd)
            return types.SimpleNamespace(
                st_dev=1,
                st_ino=77,
                st_mode=stat.S_IFLNK,
                st_size=0,
                st_mtime_ns=0,
            )

        def parent_metadata(fd: int) -> tuple[object, ...]:
            identity = (
                source_parent.identity if fd == 142 else destination_parent.identity
            )
            return (
                identity,
                FilesystemIdentity(1),
                stat.S_IFDIR | 0o700,
                0,
                0,
            )

        with (
            mock.patch("os.stat", side_effect=stat_entry),
            mock.patch("os.rename") as rename,
            mock.patch("os.open") as reopen,
            mock.patch("os.dup2") as duplicate,
            mock.patch("os.close") as close,
            mock.patch.object(
                posix_filesystem_module,
                "_metadata",
                side_effect=parent_metadata,
            ),
        ):
            backend.rename(
                source,
                destination_parent,
                "renamed",
                replace=False,
            )
            resource = source._resource_for(backend)
            self.assertIsInstance(resource, _PosixResource)
            assert isinstance(resource, _PosixResource)
            self.assertEqual(resource.fd, 142)
            self.assertIs(resource.parent, destination_parent)
            self.assertEqual(resource.name, "renamed")
            self.assertEqual(stat_fds, [142])
            rename.assert_called_once_with(
                "item",
                "renamed",
                src_dir_fd=142,
                dst_dir_fd=141,
            )
            reopen.assert_not_called()
            duplicate.assert_called_once_with(141, 142, inheritable=False)
            close.assert_not_called()
            source.close()
            source_parent.close()
            destination_parent.close()

    def test_dup2_failure_blocks_later_non_follow_mutation(self) -> None:
        for offset, later_operation in enumerate(("delete", "rename")):
            with self.subTest(later_operation=later_operation):
                backend = PosixFilesystemBackend()
                source_parent_fd = 150 + offset * 10
                destination_parent_fd = source_parent_fd + 1
                surrogate_fd = source_parent_fd + 2
                source_parent = self._directory(backend, source_parent_fd)
                destination_parent = self._directory(
                    backend,
                    destination_parent_fd,
                    identity=FileIdentity(1, 30 + offset),
                )
                source = self._non_follow_file(
                    backend, surrogate_fd, source_parent, EntryKind.OTHER
                )
                source_metadata = types.SimpleNamespace(
                    st_dev=1,
                    st_ino=77,
                    st_mode=stat.S_IFIFO,
                    st_size=0,
                    st_mtime_ns=0,
                )

                def parent_metadata(fd: int) -> tuple[object, ...]:
                    identity = (
                        source_parent.identity
                        if fd == surrogate_fd
                        else destination_parent.identity
                    )
                    return (
                        identity,
                        FilesystemIdentity(1),
                        stat.S_IFDIR | 0o700,
                        0,
                        0,
                    )

                with (
                    mock.patch("os.stat", return_value=source_metadata) as stat_entry,
                    mock.patch("os.rename") as rename,
                    mock.patch("os.unlink") as unlink,
                    mock.patch("os.open") as reopen,
                    mock.patch(
                        "os.dup2", side_effect=OSError("surrogate refresh failed")
                    ) as duplicate,
                    mock.patch("os.close") as close,
                    mock.patch.object(
                        posix_filesystem_module,
                        "_metadata",
                        side_effect=parent_metadata,
                    ),
                ):
                    with self.assertRaisesRegex(
                        OSError, "surrogate refresh failed"
                    ):
                        backend.rename(
                            source,
                            destination_parent,
                            "renamed",
                            replace=False,
                        )
                    resource = source._resource_for(backend)
                    self.assertIsInstance(resource, _PosixResource)
                    assert isinstance(resource, _PosixResource)
                    self.assertEqual(resource.fd, surrogate_fd)
                    self.assertIs(resource.parent, destination_parent)
                    self.assertEqual(resource.name, "renamed")
                    reopen.assert_not_called()
                    duplicate.assert_called_once_with(
                        destination_parent_fd,
                        surrogate_fd,
                        inheritable=False,
                    )
                    close.assert_not_called()

                    stat_entry.reset_mock()
                    rename.reset_mock()
                    unlink.reset_mock()
                    if later_operation == "delete":
                        with self.assertRaisesRegex(
                            OSError, "parent surrogate identity changed"
                        ):
                            backend.delete(source)
                    else:
                        with self.assertRaisesRegex(
                            OSError, "parent surrogate identity changed"
                        ):
                            backend.rename(
                                source,
                                destination_parent,
                                "again",
                                replace=False,
                            )
                    stat_entry.assert_not_called()
                    rename.assert_not_called()
                    unlink.assert_not_called()
                    source.close()
                    source_parent.close()
                    destination_parent.close()
