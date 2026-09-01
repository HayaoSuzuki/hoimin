from __future__ import annotations

import os
import sys
import types
import unittest
from dataclasses import FrozenInstanceError
from pathlib import Path
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


class RecordingOwner:
    def __init__(self) -> None:
        self.closed: list[int] = []
        self.detached: list[tuple[int, int]] = []
        self.fail_close = False
        self.fail_detach = False

    def close_resource(self, resource: object) -> None:
        if self.fail_close:
            raise OSError("close failed")
        self.closed.append(int(resource))

    def detach_file_resource(self, resource: object, flags: int) -> int:
        if self.fail_detach:
            raise OSError("detach failed")
        self.detached.append((int(resource), flags))
        return int(resource) + 100


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
        invalid_cases = [
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
