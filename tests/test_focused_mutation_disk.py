from pathlib import Path
import argparse
import errno
from contextlib import contextmanager, nullcontext
from collections.abc import Callable
import gc
import inspect
import json
import os
import stat
import sys
import tempfile
import threading
import time
from types import ModuleType
import unittest
import uuid
from unittest import mock
from typing import BinaryIO, Protocol, cast
import zlib

from tools.focused_mutation import Options, _parser, options_from_arguments
from tools.focused_mutation_support.disk import (
    MAX_COMMAND_LOG_BYTES,
    MAX_DIAGNOSTIC_DETAIL_BYTES,
    MAX_DIAGNOSTIC_DETAILS,
    MAX_LOG_BYTES,
    CleanupOutcome,
    ComponentState,
    DiskLifecycle,
    DiskLifecycleEvent,
    DiskFailure,
    DiskObservation,
    DiskPolicy,
    DiskGuard,
    MeterRoot,
    DiskRootId,
    DiskSecondary,
    DiskStopReason,
    DISK_MEASUREMENT_FAILED,
    apply_disk_lifecycle_event,
    canonical_scratch_root,
    evaluate_disk_policy,
    parse_byte_size,
)
from tools.focused_mutation_support.filesystem import (
    CreateDisposition,
    DirectoryCapability,
    DirectoryEntry,
    DirectoryIterator,
    EntryKind,
    FileAccess,
    FileCapability,
    FileIdentity,
    FilesystemBackend,
    FilesystemIdentity,
    SecurityDomain,
    SharePolicy,
)
from tools.focused_mutation_support.runner import CommandDiskStopped, CommandRunner
from tools.focused_mutation_support.model import RunRecord
from tools.focused_mutation_support.lease import (
    JanitorDiagnostic,
    ManagedScratch,
    LeaseLock,
    ScratchCleanupRecord,
    ScratchCleanupStatus,
    _MarkerRollbackOwners,
    _OwnedDescriptor,
    _bound_cleanup_records,
    _close_lease_lock_all,
    _open_coordinator,
    reclaim_abandoned,
    validate_reported_path,
)
from tools.focused_mutation_support.mutation import (
    parse_list_json,
    read_bounded_regular,
    read_bounded_regular_tail,
)
from tools.focused_mutation_support.store import (
    BoundedTextWriter,
    OwnedOutput,
    ReportTooLarge,
    RunStore,
)


class _WindowsJunctionHelper(Protocol):
    def _junction(self, link: Path, target: Path) -> None: ...


def _cleanup_records_only(
    records: list[ScratchCleanupRecord | JanitorDiagnostic],
) -> list[ScratchCleanupRecord]:
    assert all(isinstance(record, ScratchCleanupRecord) for record in records)
    return cast(list[ScratchCleanupRecord], records)


def _write_text(text: str) -> Callable[[BoundedTextWriter], None]:
    def write(writer: BoundedTextWriter) -> None:
        writer.write(text)

    return write


class DiskPolicyParserTests(unittest.TestCase):
    def options(self, namespace: argparse.Namespace) -> Options:
        parser_os = mock.Mock(wraps=os)
        parser_os.name = "posix"
        parser_os.path = os.path
        with mock.patch("tools.focused_mutation.os", parser_os):
            return options_from_arguments(namespace, Path("/repo"))

    def parse(self, *arguments: str) -> DiskPolicy:
        namespace = _parser().parse_args(["--output", "/tmp/out", *arguments])
        return self.options(namespace).disk_policy

    def test_exact_disk_safety_defaults(self) -> None:
        policy = self.parse()
        self.assertEqual(policy.max_disk_bytes, 8 * 1024**3)
        self.assertEqual(policy.min_free_bytes, 10 * 1024**3)
        self.assertEqual(policy.jobs, 1)
        self.assertEqual(policy.max_log_bytes, 16 * 1024**2)
        self.assertEqual(policy.max_command_log_bytes, 64 * 1024**2)
        self.assertEqual(policy.max_tool_json_bytes, 8 * 1024**2)
        self.assertEqual(policy.max_selector_count, 1_000)
        self.assertEqual(policy.max_selector_bytes, 16 * 1024)
        self.assertEqual(policy.max_candidate_diagnostic_bytes, 16 * 1024)
        self.assertEqual(policy.max_run_diagnostic_bytes, 16 * 1024**2)
        self.assertEqual(policy.max_report_bytes, 32 * 1024**2)
        self.assertEqual(policy.max_reported_path_bytes, 16 * 1024)
        self.assertEqual(policy.sample_interval_seconds, 0.250)

    def test_byte_parser_accepts_binary_units(self) -> None:
        self.assertEqual(parse_byte_size("1"), 1)
        self.assertEqual(parse_byte_size("16MiB"), 16 * 1024**2)
        self.assertEqual(parse_byte_size("8GiB"), 8 * 1024**3)

    def test_byte_parser_rejects_nonpositive_or_ambiguous_values(self) -> None:
        for value in ("0", "-1", "1MB", "1.5GiB", "nan"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                parse_byte_size(value)

    def test_cli_accepts_explicit_bounded_policy(self) -> None:
        policy = self.parse(
            "--max-disk",
            "12GiB",
            "--min-free-space",
            "11GiB",
            "--jobs",
            "4",
            "--max-log-size",
            "32MiB",
        )
        self.assertEqual(policy.max_disk_bytes, 12 * 1024**3)
        self.assertEqual(policy.min_free_bytes, 11 * 1024**3)
        self.assertEqual(policy.jobs, 4)
        self.assertEqual(policy.max_log_bytes, 32 * 1024**2)

    def test_cli_rejects_unsafe_jobs_and_log_sizes(self) -> None:
        for arguments in (
            ("--jobs", "0"),
            ("--jobs", "5"),
            ("--max-log-size", "0"),
            ("--max-log-size", "65MiB"),
        ):
            with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                self.parse(*arguments)
        self.assertEqual(MAX_LOG_BYTES, 16 * 1024**2)
        self.assertEqual(MAX_COMMAND_LOG_BYTES, 64 * 1024**2)

    def test_scratch_root_must_resolve_to_a_real_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scratch = root / "scratch"
            scratch.mkdir()
            policy = self.parse("--scratch-root", str(scratch))
            self.assertEqual(policy.scratch_root, scratch.resolve())
            file_path = root / "file"
            file_path.write_text("not a directory", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "scratch root"):
                self.parse("--scratch-root", str(file_path))

    @unittest.skipIf(
        os.name == "nt",
        "Task 8 OwnedOutput activation and symlink policy are intentionally unmigrated",
    )
    def test_cli_preserves_output_symlink_for_no_follow_rejection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target"
            target.mkdir()
            output = root / "output"
            output.symlink_to(target, target_is_directory=True)
            namespace = _parser().parse_args(
                ["--output", str(output), "--min-free-space", "1"]
            )

            options = self.options(namespace)

            self.assertEqual(options.output, output.absolute())
            self.assertNotEqual(options.output, target)
            with self.assertRaises(OSError):
                OwnedOutput.create(
                    options.output,
                    "00000000-0000-4000-8000-000000000011",
                )

    def test_windows_fails_closed_before_disk_safe_runtime_setup(self) -> None:
        namespace = _parser().parse_args(["--output", r"C:\out"])

        with (
            mock.patch("tools.focused_mutation.os.name", "nt"),
            self.assertRaisesRegex(ValueError, "Windows native adapter"),
        ):
            options_from_arguments(namespace, Path(r"C:\repo"))

    def test_keep_scratch_is_recorded_without_disabling_monitoring(self) -> None:
        policy = self.parse("--keep-scratch")
        self.assertTrue(policy.keep_scratch)
        self.assertEqual(policy.sample_interval_seconds, 0.250)

    def test_selector_count_and_encoded_size_are_bounded(self) -> None:
        too_many = ["--output", "/tmp/out"]
        for index in range(1_001):
            too_many.extend(["--file", f"file-{index}.rs"])
        with self.assertRaisesRegex(ValueError, "selectors exceed 1000"):
            namespace = _parser().parse_args(too_many)
            self.options(namespace)

        namespace = _parser().parse_args(
            ["--output", "/tmp/out", "--symbol", "x" * (16 * 1024 + 1)]
        )
        with self.assertRaisesRegex(ValueError, "selector exceeds 16 KiB"):
            self.options(namespace)


class DiskPolicyDecisionTests(unittest.TestCase):
    def policy(self) -> DiskPolicy:
        return DiskPolicy(max_disk_bytes=100, min_free_bytes=20)

    def test_thresholds_are_inclusive_and_reserve_has_precedence(self) -> None:
        self.assertIsNone(
            evaluate_disk_policy(
                self.policy(), DiskObservation(owned_bytes=99, available_bytes=21)
            )
        )
        size = evaluate_disk_policy(
            self.policy(), DiskObservation(owned_bytes=100, available_bytes=21)
        )
        assert size is not None
        self.assertEqual(size.reason, DiskStopReason.WORKSPACE_SIZE_EXCEEDED)
        reserve = evaluate_disk_policy(
            self.policy(), DiskObservation(owned_bytes=100, available_bytes=20)
        )
        assert reserve is not None
        self.assertEqual(reserve.reason, DiskStopReason.FILESYSTEM_RESERVE_REACHED)
        self.assertEqual(
            [item.reason for item in reserve.secondary],
            [DiskStopReason.WORKSPACE_SIZE_EXCEEDED],
        )

    def test_cleanup_waits_for_reap_drains_and_monitor(self) -> None:
        lifecycle = DiskLifecycle([DiskRootId.EXECUTION])
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle, DiskLifecycleEvent.dispatch_requested()
            )
        )
        self.assertFalse(
            apply_disk_lifecycle_event(
                lifecycle,
                DiskLifecycleEvent.cleanup_requested(DiskRootId.EXECUTION),
            )
        )
        for event in (
            DiskLifecycleEvent.process_drain_succeeded(),
            DiskLifecycleEvent.output_drain_succeeded(),
            DiskLifecycleEvent.monitor_join_succeeded(),
        ):
            self.assertTrue(apply_disk_lifecycle_event(lifecycle, event))
        self.assertFalse(
            apply_disk_lifecycle_event(
                lifecycle, DiskLifecycleEvent.dispatch_requested()
            )
        )
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle,
                DiskLifecycleEvent.cleanup_requested(DiskRootId.EXECUTION),
            )
        )
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle,
                DiskLifecycleEvent.cleanup_completed(
                    DiskRootId.EXECUTION, CleanupOutcome.CLEAN
                ),
            )
        )
        self.assertEqual(lifecycle.process_drain, ComponentState.SUCCEEDED)

    def test_failed_component_permits_deferred_but_not_clean(self) -> None:
        lifecycle = DiskLifecycle([DiskRootId.EXECUTION])
        for event in (
            DiskLifecycleEvent.process_drain_failed(),
            DiskLifecycleEvent.output_drain_succeeded(),
            DiskLifecycleEvent.monitor_join_succeeded(),
        ):
            self.assertTrue(apply_disk_lifecycle_event(lifecycle, event))
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle,
                DiskLifecycleEvent.cleanup_requested(DiskRootId.EXECUTION),
            )
        )
        self.assertFalse(
            apply_disk_lifecycle_event(
                lifecycle,
                DiskLifecycleEvent.cleanup_completed(
                    DiskRootId.EXECUTION, CleanupOutcome.CLEAN
                ),
            )
        )
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle,
                DiskLifecycleEvent.cleanup_completed(
                    DiskRootId.EXECUTION,
                    CleanupOutcome.DEFERRED,
                    "still active",
                ),
            )
        )

    def test_same_reason_keeps_distinct_evidence_and_deduplicates_exact_repeats(
        self,
    ) -> None:
        lifecycle = DiskLifecycle([])
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle, DiskLifecycleEvent.measurement_failed("first")
            )
        )
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle, DiskLifecycleEvent.measurement_failed("second")
            )
        )
        self.assertTrue(
            apply_disk_lifecycle_event(
                lifecycle, DiskLifecycleEvent.measurement_failed("second")
            )
        )

        self.assertEqual(
            lifecycle.secondary,
            [
                DiskSecondary(
                    reason=DiskStopReason.MEASUREMENT_FAILED,
                    code=DISK_MEASUREMENT_FAILED,
                    message="second",
                )
            ],
        )

        threshold = DiskLifecycle([])
        value = DiskObservation(owned_bytes=100, available_bytes=20)
        event = DiskLifecycleEvent.observation(self.policy(), value)
        self.assertTrue(apply_disk_lifecycle_event(threshold, event))
        self.assertTrue(apply_disk_lifecycle_event(threshold, event))
        assert threshold.stop is not None
        self.assertEqual(len(threshold.stop.secondary), 1)
        self.assertEqual(threshold.secondary, [])

        cleanup = DiskLifecycle(
            [DiskRootId.EXECUTION, DiskRootId.DELIVERY]
        )
        for event in (
            DiskLifecycleEvent.process_drain_succeeded(),
            DiskLifecycleEvent.output_drain_succeeded(),
            DiskLifecycleEvent.monitor_join_succeeded(),
            DiskLifecycleEvent.report_succeeded(),
        ):
            self.assertTrue(apply_disk_lifecycle_event(cleanup, event))
        for root in (DiskRootId.EXECUTION, DiskRootId.DELIVERY):
            self.assertTrue(
                apply_disk_lifecycle_event(
                    cleanup, DiskLifecycleEvent.cleanup_requested(root)
                )
            )
            self.assertTrue(
                apply_disk_lifecycle_event(
                    cleanup,
                    DiskLifecycleEvent.cleanup_completed(
                        root, CleanupOutcome.FAILED, "same cleanup failure"
                    ),
                )
            )
        self.assertEqual(
            cleanup.secondary,
            [
                DiskSecondary(
                    code="workspace.cleanup.failed",
                    message="same cleanup failure",
                )
            ],
        )


class BoundedCommandDrainTests(unittest.TestCase):
    def test_concurrent_stdout_and_stderr_are_drained_and_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "output"
            output.mkdir()
            store = RunStore(output)
            store.commands.mkdir()
            script = root / "noisy.py"
            script.write_text(
                "import os, threading\n"
                "def write(fd, byte):\n"
                "    for _ in range(16): os.write(fd, byte * 4096)\n"
                "threads = [threading.Thread(target=write, args=(1, b'A')), "
                "threading.Thread(target=write, args=(2, b'B'))]\n"
                "[thread.start() for thread in threads]\n"
                "[thread.join() for thread in threads]\n",
                encoding="utf-8",
            )
            record = CommandRunner(store).run(
                [sys.executable, str(script)],
                cwd=root,
                timeout=5.0,
                label="noisy",
                max_log_bytes=8 * 1024,
            )
            self.assertEqual(record.exit_code, 0)
            self.assertEqual(record.stdout_observed_bytes, 64 * 1024)
            self.assertEqual(record.stderr_observed_bytes, 64 * 1024)
            self.assertEqual(record.stdout_retained_bytes, 4 * 1024)
            self.assertEqual(record.stderr_retained_bytes, 4 * 1024)
            self.assertTrue(record.stdout_truncated)
            self.assertTrue(record.stderr_truncated)
            self.assertLessEqual(
                Path(record.stdout_path).stat().st_size
                + Path(record.stderr_path).stat().st_size,
                8 * 1024,
            )

    def test_tool_controlled_regular_reads_are_capped_before_decode(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            oversized = root / "oversized.json"
            oversized.write_bytes(b" " * 1025)
            with self.assertRaisesRegex(ValueError, "exceeds 1024 bytes"):
                read_bounded_regular(oversized, 1024)

            target = root / "target.json"
            target.write_text("[]", encoding="utf-8")
            link = root / "link.json"
            link.symlink_to(target)
            with self.assertRaises(OSError):
                read_bounded_regular(link, 1024)

    def test_inventory_entry_count_is_bounded_before_entry_validation(self) -> None:
        inventory = "[" + ",".join("{}" for _ in range(10_001)) + "]"
        with self.assertRaisesRegex(ValueError, "exceeds 10000 entries"):
            parse_list_json(inventory)

    def test_inventory_string_fields_are_bounded_before_candidate_creation(self) -> None:
        inventory = json.dumps(
            [
                {
                    "file": "src/lib.rs",
                    "name": "x" * (16 * 1024 + 1),
                    "function": {"function_name": "f"},
                }
            ]
        )

        with self.assertRaisesRegex(ValueError, "name exceeds 16384 bytes"):
            parse_list_json(inventory)

    def test_sparse_multi_gibibyte_diagnostic_reads_only_the_tail_window(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "compiler.log"
            with path.open("wb") as stream:
                stream.truncate(3 * 1024**3)
                stream.seek(-4, 2)
                stream.write(b"TAIL")

            tail, observed = read_bounded_regular_tail(path, 16 * 1024)

            self.assertEqual(observed, 3 * 1024**3)
            self.assertEqual(len(tail), 16 * 1024)
            self.assertTrue(tail.endswith(b"TAIL"))


class _RecordedNode:
    def __init__(
        self,
        identity: FileIdentity,
        filesystem: FilesystemIdentity,
        *,
        kind: EntryKind = EntryKind.DIRECTORY,
        logical_size: int = 0,
    ) -> None:
        self.identity = identity
        self.filesystem = filesystem
        self.kind = kind
        self.logical_size = logical_size
        self.children: dict[str, _RecordedNode] = {}

    def add(self, name: str, node: "_RecordedNode") -> "_RecordedNode":
        self.children[name] = node
        return node

    def evidence(self, name: str) -> DirectoryEntry:
        return DirectoryEntry(
            name=name,
            kind=self.kind,
            identity=self.identity,
            filesystem=self.filesystem,
            logical_size=self.logical_size,
            modified_ns=0,
        )


class _RecordedResource:
    def __init__(
        self,
        node: _RecordedNode,
        path_hint: Path,
        *,
        close_failures: int = 0,
    ) -> None:
        self.node = node
        self.path_hint = path_hint
        self.close_failures = close_failures
        self.closed = False
        self.walker_owned = False


class _RecordedIterator:
    def __init__(
        self,
        backend: "_RecordingFilesystemBackend",
        directory: DirectoryCapability,
        entries: list[tuple[str, _RecordedNode]],
        *,
        fail_after: int | None,
        virtual_entry: _RecordedNode | None = None,
        virtual_count: int = 0,
    ) -> None:
        self._backend = backend
        self._directory = directory
        self._entries = entries
        self._index = 0
        self._fail_after = fail_after
        self._virtual_entry = virtual_entry
        self._virtual_count = virtual_count
        self._closed = False

    @property
    def directory(self) -> DirectoryCapability:
        return self._directory

    def __iter__(self) -> "_RecordedIterator":
        return self

    def __next__(self) -> DirectoryEntry:
        self._backend.operations.append("iterator.next")
        if self._closed:
            raise StopIteration
        if self._fail_after is not None and self._index == self._fail_after:
            error = OSError("injected iteration failure")
            try:
                self.close()
            except BaseException as close_error:
                error.add_note(f"recording iterator cleanup failed: {close_error}")
            raise error
        if self._virtual_entry is not None and self._index < self._virtual_count:
            name = f"entry-{self._index}"
            self._index += 1
            result = self._virtual_entry.evidence(name)
            self._backend.after_iterator_next()
            return result
        if self._index == len(self._entries) + self._virtual_count:
            self.close()
            raise StopIteration
        name, node = self._entries[self._index - self._virtual_count]
        self._index += 1
        result = node.evidence(name)
        self._backend.after_iterator_next()
        return result

    def close(self) -> None:
        self._backend.operations.append("iterator.close")
        if self._closed:
            return
        self._directory.close()
        self._closed = True


class _RecordingFilesystemBackend(FilesystemBackend):
    def __init__(self) -> None:
        self.roots: dict[Path, _RecordedNode] = {}
        self.open_root_events: dict[Path, list[object]] = {}
        self.close_failures: dict[FileIdentity, int] = {}
        self.iterator_fail_after: dict[FileIdentity, int] = {}
        self.virtual_entries: dict[FileIdentity, tuple[_RecordedNode, int]] = {}
        self.iterator_construction_errors: dict[FileIdentity, BaseException] = {}
        self.before_open: Callable[
            [DirectoryCapability, str, EntryKind], None
        ] = lambda _parent, _name, _kind: None
        self.after_reopen: Callable[[], None] = lambda: None
        self.after_entries_owned: Callable[[], None] = lambda: None
        self.after_iterator_next: Callable[[], None] = lambda: None
        self.after_available: Callable[[], None] = lambda: None
        self.open_overrides: dict[tuple[EntryKind, str], _RecordedNode] = {}
        self.available: dict[FilesystemIdentity, int] = {}
        self.units: dict[FilesystemIdentity, int] = {}
        self.available_error: BaseException | None = None
        self.allocation_error: BaseException | None = None
        self.operations: list[str] = []
        self.open_root_calls: list[Path] = []
        self.available_calls: list[DirectoryCapability] = []
        self.allocation_calls: list[DirectoryCapability] = []
        self.active_resources = 0
        self.max_active_resources = 0
        self.walker_resources = 0
        self.max_walker_resources = 0
        self.closed_identities: list[FileIdentity] = []
        self.issued_iterators: list[_RecordedIterator] = []

    def register(self, path: Path, node: _RecordedNode) -> None:
        self.roots[path] = node
        self.available.setdefault(node.filesystem, 1_000_000)
        self.units.setdefault(node.filesystem, 1)

    def _resource(
        self, capability: DirectoryCapability | FileCapability
    ) -> _RecordedResource:
        resource = capability._resource_for(self)
        if not isinstance(resource, _RecordedResource):
            raise RuntimeError("invalid recording filesystem resource")
        return resource

    def _record_open(self, resource: _RecordedResource) -> None:
        self.active_resources += 1
        self.max_active_resources = max(
            self.max_active_resources, self.active_resources
        )

    def directory_capability(
        self,
        node: _RecordedNode,
        path_hint: Path,
        *,
        close_failures: int | None = None,
    ) -> DirectoryCapability:
        resource = _RecordedResource(
            node,
            path_hint,
            close_failures=(
                self.close_failures.get(node.identity, 0)
                if close_failures is None
                else close_failures
            ),
        )
        self._record_open(resource)
        return DirectoryCapability(
            self,
            resource,
            identity=node.identity,
            filesystem=node.filesystem,
            kind=EntryKind.DIRECTORY,
            logical_size=node.logical_size,
            modified_ns=0,
            security_domain=SecurityDomain.CALLER,
            share_policy=SharePolicy.SCAN,
            created=False,
            path_hint=path_hint,
        )

    def file_capability(
        self, node: _RecordedNode, path_hint: Path
    ) -> FileCapability:
        resource = _RecordedResource(
            node,
            path_hint,
            close_failures=self.close_failures.get(node.identity, 0),
        )
        self._record_open(resource)
        return FileCapability(
            self,
            resource,
            identity=node.identity,
            filesystem=node.filesystem,
            kind=node.kind,
            logical_size=node.logical_size,
            modified_ns=0,
            security_domain=SecurityDomain.CALLER,
            share_policy=SharePolicy.SCAN,
            created=False,
            path_hint=path_hint,
        )

    def close_resource(self, value: object) -> None:
        self.operations.append("capability.close")
        if not isinstance(value, _RecordedResource) or value.closed:
            raise RuntimeError("recording resource is not open")
        if value.close_failures:
            value.close_failures -= 1
            raise OSError("injected capability close failure")
        value.closed = True
        self.active_resources -= 1
        if value.walker_owned:
            self.walker_resources -= 1
        self.closed_identities.append(value.node.identity)

    def open_root(
        self,
        path: Path,
        share_policy: SharePolicy,
        security_domain: SecurityDomain = SecurityDomain.CALLER,
    ) -> DirectoryCapability:
        self.operations.append("open_root")
        self.open_root_calls.append(path)
        events = self.open_root_events.get(path)
        if events:
            selected = events.pop(0)
            if isinstance(selected, BaseException):
                raise selected
            if isinstance(selected, (DirectoryCapability, FileCapability)):
                return cast(DirectoryCapability, selected)
            assert isinstance(selected, _RecordedNode)
            node = selected
        else:
            try:
                node = self.roots[path]
            except KeyError as error:
                raise FileNotFoundError(path) from error
        return self.directory_capability(node, path)

    def reopen_directory(
        self,
        directory: DirectoryCapability,
        share_policy: SharePolicy | None = None,
    ) -> DirectoryCapability:
        self.operations.append("reopen_directory")
        resource = self._resource(directory)
        reopened = self.directory_capability(resource.node, directory.path_hint)
        self.after_reopen()
        return reopened

    def open_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        self.operations.append("open_directory")
        self.before_open(parent, name, EntryKind.DIRECTORY)
        resource = self._resource(parent)
        child = self.open_overrides.get((EntryKind.DIRECTORY, name))
        if child is None:
            try:
                child = resource.node.children[name]
            except KeyError as error:
                raise FileNotFoundError(name) from error
        return self.directory_capability(child, parent.path_hint / name)

    def create_secure_root(
        self,
        parent: DirectoryCapability,
        name: str,
    ) -> DirectoryCapability:
        raise AssertionError("recording backend does not create secure roots")

    def _prepare_secure_root_commit(
        self, directory: DirectoryCapability
    ) -> None:
        raise AssertionError("recording backend does not commit secure roots")

    def _directory_creation_rollback_available(
        self, directory: DirectoryCapability
    ) -> bool:
        raise AssertionError("recording backend does not create directories")

    def _commit_secure_root(self, directory: DirectoryCapability) -> None:
        raise AssertionError("recording backend does not commit secure roots")

    def create_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        raise AssertionError("recording backend does not create directories")

    def open_file(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        access: FileAccess,
        disposition: CreateDisposition,
        share_policy: SharePolicy = SharePolicy.MUTATION,
    ) -> FileCapability:
        self.operations.append("open_file")
        self.before_open(parent, name, EntryKind.REGULAR)
        resource = self._resource(parent)
        child = self.open_overrides.get((EntryKind.REGULAR, name))
        if child is None:
            try:
                child = resource.node.children[name]
            except KeyError as error:
                raise FileNotFoundError(name) from error
        return self.file_capability(child, parent.path_hint / name)

    def open_entry(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> FileCapability | DirectoryCapability:
        resource = self._resource(parent)
        child = resource.node.children[name]
        if child.kind is EntryKind.DIRECTORY:
            return self.directory_capability(child, parent.path_hint / name)
        return self.file_capability(child, parent.path_hint / name)

    def entry(
        self,
        parent: DirectoryCapability,
        name: str,
    ) -> DirectoryEntry | None:
        resource = self._resource(parent)
        child = resource.node.children.get(name)
        return None if child is None else child.evidence(name)

    def entries_owned(self, parent: DirectoryCapability) -> _RecordedIterator:
        self.operations.append("entries_owned")
        source_resource = self._resource(parent)
        moved = parent._move_for(self)
        resource = self._resource(moved)
        construction_error = self.iterator_construction_errors.get(
            resource.node.identity
        )
        if construction_error is not None:
            try:
                moved.close()
            except BaseException as close_error:
                construction_error.add_note(
                    f"recording iterator cleanup failed: {close_error}"
                )
                if moved.is_open:
                    try:
                        moved.close()
                    except BaseException as retry_error:
                        construction_error.add_note(
                            "recording iterator final cleanup failed: "
                            f"{retry_error}"
                        )
            raise construction_error
        source_resource.walker_owned = True
        self.walker_resources += 1
        self.max_walker_resources = max(
            self.max_walker_resources, self.walker_resources
        )
        virtual_entry, virtual_count = self.virtual_entries.get(
            resource.node.identity,
            (None, 0),
        )
        iterator = _RecordedIterator(
            self,
            moved,
            list(resource.node.children.items()),
            fail_after=self.iterator_fail_after.get(resource.node.identity),
            virtual_entry=virtual_entry,
            virtual_count=virtual_count,
        )
        self.issued_iterators.append(iterator)
        self.after_entries_owned()
        return iterator

    def entries(self, parent: DirectoryCapability) -> _RecordedIterator:
        reopened = self.reopen_directory(parent)
        try:
            return self.entries_owned(reopened)
        except BaseException:
            if reopened.is_open:
                reopened.close()
            raise

    def available_bytes(self, directory: DirectoryCapability) -> int:
        self.operations.append("available_bytes")
        self._resource(directory)
        self.available_calls.append(directory)
        if self.available_error is not None:
            raise self.available_error
        result = self.available[directory.filesystem]
        self.after_available()
        return result

    def allocation_unit(self, directory: DirectoryCapability) -> int:
        self.operations.append("allocation_unit")
        self._resource(directory)
        self.allocation_calls.append(directory)
        if self.allocation_error is not None:
            raise self.allocation_error
        return self.units[directory.filesystem]

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        raise AssertionError("recording backend does not rename")

    def delete(
        self,
        capability: FileCapability | DirectoryCapability,
    ) -> None:
        raise AssertionError("recording backend does not delete")

    def touch(self, file: FileCapability) -> None:
        raise AssertionError("recording backend does not touch")

    def flush(self, file: FileCapability) -> None:
        raise AssertionError("recording backend does not flush")

    def final_path(self, directory: DirectoryCapability) -> Path:
        return directory.path_hint

    def verify_managed_security(
        self,
        capability: FileCapability | DirectoryCapability,
        *,
        repair_dacl: bool,
    ) -> None:
        raise AssertionError("recording backend has no managed security")


class _ManagedRecordedNode:
    def __init__(
        self,
        identity: FileIdentity,
        filesystem: FilesystemIdentity,
        *,
        kind: EntryKind,
        security_domain: SecurityDomain,
        parent: "_ManagedRecordedNode | None" = None,
        name: str = "",
        backing: BinaryIO | None = None,
        modified_ns: int = 0,
    ) -> None:
        self.identity = identity
        self.filesystem = filesystem
        self.kind = kind
        self.security_domain = security_domain
        self.parent = parent
        self.name = name
        self.backing = backing
        self.modified_ns = modified_ns
        self.children: dict[str, _ManagedRecordedNode] = {}

    def evidence(self) -> DirectoryEntry:
        size = 0
        if self.backing is not None:
            size = os.fstat(self.backing.fileno()).st_size
        return DirectoryEntry(
            self.name,
            self.kind,
            self.identity,
            self.filesystem,
            size,
            self.modified_ns,
        )


class _ManagedRecordedResource:
    def __init__(
        self,
        node: _ManagedRecordedNode,
        *,
        descriptor: int = -1,
        secure_root_creation: bool = False,
    ) -> None:
        self.node = node
        self.descriptor = descriptor
        self.close_failures = 0
        self.closed = False
        self.secure_root_creation = secure_root_creation


class _ManagedRecordedIterator:
    def __init__(
        self,
        backend: "_ManagedRecordingBackend",
        directory: DirectoryCapability,
    ) -> None:
        self._backend = backend
        self._directory = directory
        resource = backend._resource(directory)
        self._entries = iter(tuple(resource.node.children.values()))
        self._failure = backend.iterator_failure
        backend.iterator_failure = None
        self._closed = False

    @property
    def directory(self) -> DirectoryCapability:
        return self._directory

    def __iter__(self) -> "_ManagedRecordedIterator":
        return self

    def __next__(self) -> DirectoryEntry:
        if self._closed:
            raise StopIteration
        if self._failure is not None:
            failure = self._failure
            self._failure = None
            raise failure
        try:
            return next(self._entries).evidence()
        except StopIteration:
            self.close()
            raise

    def close(self) -> None:
        if self._closed:
            return
        self._directory.close()
        self._closed = True


class _ManagedRecordingBackend(FilesystemBackend):
    """Backend-neutral managed-publication oracle with real detached fds."""

    _filesystem = FilesystemIdentity(0xA11CE, 255, 0x4006)

    def __init__(
        self,
        parent_path: Path,
        *,
        rename_requires_closed_descendants: bool,
    ) -> None:
        self.parent_path = parent_path
        self._rename_requires_closed_descendants = (
            rename_requires_closed_descendants
        )
        self._next_identity = 100
        self.events: list[str] = []
        self.live_resources: set[_ManagedRecordedResource] = set()
        self.detached: dict[int, _ManagedRecordedNode] = {}
        self.backings: list[BinaryIO] = []
        self.failures: dict[str, BaseException] = {}
        self.after_event: Callable[[str], None] = lambda _event: None
        self.iterator_failure: BaseException | None = None
        self.coordinator: LeaseLock | None = None
        self.last_parent_capability: DirectoryCapability | None = None
        self.parent = self._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.CALLER,
            parent=None,
            name=parent_path.name,
        )

    @property
    def directory_rename_requires_closed_descendants(self) -> bool:
        return self._rename_requires_closed_descendants

    def _emit(self, event: str) -> None:
        self.events.append(event)
        failure = self.failures.pop(event, None)
        if failure is not None:
            raise failure
        self.after_event(event)

    def _new_node(
        self,
        kind: EntryKind,
        security_domain: SecurityDomain,
        *,
        parent: _ManagedRecordedNode | None,
        name: str,
        modified_ns: int = 0,
    ) -> _ManagedRecordedNode:
        self._next_identity += 1
        backing: BinaryIO | None = None
        if kind is EntryKind.REGULAR:
            self._prune_detached()
            backing = cast(BinaryIO, tempfile.TemporaryFile())
            self.backings.append(backing)
        node = _ManagedRecordedNode(
            FileIdentity(self._filesystem.volume, self._next_identity),
            self._filesystem,
            kind=kind,
            security_domain=security_domain,
            parent=parent,
            name=name,
            backing=backing,
            modified_ns=modified_ns,
        )
        if parent is not None:
            parent.children[name] = node
        return node

    @staticmethod
    def _path(node: _ManagedRecordedNode) -> Path:
        components: list[str] = []
        current: _ManagedRecordedNode | None = node
        while current is not None:
            components.append(current.name)
            current = current.parent
        return Path("C:/recorded").joinpath(*reversed(components))

    def _resource(
        self, capability: DirectoryCapability | FileCapability
    ) -> _ManagedRecordedResource:
        resource = capability._resource_for(self)
        if not isinstance(resource, _ManagedRecordedResource):
            raise RuntimeError("invalid managed recording resource")
        return resource

    def _directory_capability(
        self,
        node: _ManagedRecordedNode,
        *,
        share_policy: SharePolicy,
        created: bool,
        secure_root_creation: bool = False,
    ) -> DirectoryCapability:
        resource = _ManagedRecordedResource(
            node, secure_root_creation=secure_root_creation
        )
        self.live_resources.add(resource)
        return DirectoryCapability(
            self,
            resource,
            identity=node.identity,
            filesystem=node.filesystem,
            kind=EntryKind.DIRECTORY,
            logical_size=0,
            modified_ns=0,
            security_domain=node.security_domain,
            share_policy=share_policy,
            created=created,
            path_hint=self._path(node),
        )

    def _file_capability(
        self,
        node: _ManagedRecordedNode,
        *,
        share_policy: SharePolicy,
        created: bool,
    ) -> FileCapability:
        descriptor = -1
        if node.kind is EntryKind.REGULAR:
            if node.backing is None:
                raise RuntimeError(
                    "recorded regular file has no backing descriptor"
                )
            self._prune_detached()
            descriptor = os.dup(node.backing.fileno())
            os.lseek(descriptor, 0, os.SEEK_SET)
        elif node.kind not in {EntryKind.REPARSE, EntryKind.OTHER}:
            raise RuntimeError("recorded file capability has an invalid kind")
        resource = _ManagedRecordedResource(node, descriptor=descriptor)
        self.live_resources.add(resource)
        return FileCapability(
            self,
            resource,
            identity=node.identity,
            filesystem=node.filesystem,
            kind=node.kind,
            logical_size=(
                os.fstat(descriptor).st_size if descriptor >= 0 else 0
            ),
            modified_ns=0,
            security_domain=node.security_domain,
            share_policy=share_policy,
            created=created,
            path_hint=self._path(node),
        )

    def close_resource(self, value: object) -> None:
        if not isinstance(value, _ManagedRecordedResource) or value.closed:
            raise RuntimeError("managed recording resource is not open")
        self._emit(f"close:{value.node.name}")
        if value.close_failures:
            value.close_failures -= 1
            raise OSError(f"injected close failure for {value.node.name}")
        if value.descriptor >= 0:
            os.close(value.descriptor)
            value.descriptor = -1
        value.closed = True
        self.live_resources.discard(value)

    def detach_file_resource(self, value: object, flags: int) -> int:
        del flags
        if not isinstance(value, _ManagedRecordedResource):
            raise RuntimeError("invalid managed recording resource")
        if value.descriptor < 0 or value.closed:
            raise RuntimeError("managed recording file is not detachable")
        descriptor = value.descriptor
        value.descriptor = -1
        value.closed = True
        self.live_resources.discard(value)
        self.detached[descriptor] = value.node
        self._emit(f"detach:{value.node.name}")
        return descriptor

    def open_root(
        self,
        path: Path,
        share_policy: SharePolicy,
        security_domain: SecurityDomain = SecurityDomain.CALLER,
    ) -> DirectoryCapability:
        if path != self.parent_path:
            raise FileNotFoundError(path)
        if security_domain is not SecurityDomain.CALLER:
            raise AssertionError("parent root must remain caller-owned")
        self._emit(f"open-parent:{share_policy.value}")
        capability = self._directory_capability(
            self.parent, share_policy=share_policy, created=False
        )
        self.last_parent_capability = capability
        return capability

    def create_secure_root(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryCapability:
        parent_node = self._resource(parent).node
        self._emit(f"create-secure-root:{name}")
        node = parent_node.children.get(name)
        created = node is None
        if node is None:
            node = self._new_node(
                EntryKind.DIRECTORY,
                SecurityDomain.MANAGED,
                parent=parent_node,
                name=name,
            )
        if node.kind is not EntryKind.DIRECTORY:
            raise OSError("secure root is not a directory")
        return self._directory_capability(
            node,
            share_policy=SharePolicy.MUTATION,
            created=created,
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
            or resource.node.parent is None
        ):
            raise RuntimeError("recorded secure root commit is invalid")
        if not directory.created:
            if resource.secure_root_creation:
                raise RuntimeError("recorded existing root has creation provenance")
            return
        if not resource.secure_root_creation:
            raise RuntimeError("recorded created root has no rollback provenance")

    def _directory_creation_rollback_available(
        self, directory: DirectoryCapability
    ) -> bool:
        resource = self._resource(directory)
        node = resource.node
        parent = node.parent
        return (
            directory.created
            and directory.kind is EntryKind.DIRECTORY
            and parent is not None
            and parent.children.get(node.name) is node
        )

    def _commit_secure_root(self, directory: DirectoryCapability) -> None:
        resource = cast(_ManagedRecordedResource, directory._resource)
        resource.secure_root_creation = False
        self.events.append(f"commit-secure-root:{resource.node.name}")

    def reopen_directory(
        self,
        directory: DirectoryCapability,
        share_policy: SharePolicy | None = None,
    ) -> DirectoryCapability:
        self._emit(
            "reopen-directory:"
            + ("preserve" if share_policy is None else share_policy.value)
        )
        node = self._resource(directory).node
        return self._directory_capability(
            node,
            share_policy=(
                directory.share_policy if share_policy is None else share_policy
            ),
            created=False,
        )

    def open_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        self._emit(f"open-directory:{name}:{share_policy.value}")
        node = self._resource(parent).node.children.get(name)
        if node is None:
            raise FileNotFoundError(name)
        if node.kind is not EntryKind.DIRECTORY:
            raise OSError("recorded entry is not a directory")
        return self._directory_capability(
            node, share_policy=share_policy, created=False
        )

    def create_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        self._emit(f"create-directory:{name}:{share_policy.value}")
        parent_node = self._resource(parent).node
        if name in parent_node.children:
            raise FileExistsError(name)
        node = self._new_node(
            EntryKind.DIRECTORY,
            parent.security_domain,
            parent=parent_node,
            name=name,
        )
        return self._directory_capability(
            node, share_policy=share_policy, created=True
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
        parent_node = self._resource(parent).node
        self._emit(
            f"{disposition.value}:{name}:{access.value}:{share_policy.value}"
        )
        node = parent_node.children.get(name)
        created = False
        if disposition is CreateDisposition.CREATE_NEW:
            if node is not None:
                raise FileExistsError(name)
            node = self._new_node(
                EntryKind.REGULAR,
                parent.security_domain,
                parent=parent_node,
                name=name,
            )
            created = True
        elif disposition is CreateDisposition.OPEN_OR_CREATE and node is None:
            node = self._new_node(
                EntryKind.REGULAR,
                parent.security_domain,
                parent=parent_node,
                name=name,
            )
            created = True
        elif node is None:
            raise FileNotFoundError(name)
        if node.kind is not EntryKind.REGULAR:
            raise OSError("recorded entry is not a regular file")
        return self._file_capability(
            node, share_policy=share_policy, created=created
        )

    def open_entry(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> FileCapability | DirectoryCapability:
        self._emit(f"open-entry:{name}:{share_policy.value}")
        node = self._resource(parent).node.children.get(name)
        if node is None:
            raise FileNotFoundError(name)
        if node.kind is EntryKind.DIRECTORY:
            return self._directory_capability(
                node, share_policy=share_policy, created=False
            )
        return self._file_capability(
            node, share_policy=share_policy, created=False
        )

    def entry(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryEntry | None:
        self._emit(f"entry:{name}")
        node = self._resource(parent).node.children.get(name)
        return None if node is None else node.evidence()

    def entries(self, parent: DirectoryCapability) -> _ManagedRecordedIterator:
        return _ManagedRecordedIterator(self, self.reopen_directory(parent))

    def entries_owned(
        self, parent: DirectoryCapability
    ) -> DirectoryIterator:
        moved = parent._move_for(self)
        return _ManagedRecordedIterator(self, moved)

    @staticmethod
    def _is_descendant(
        node: _ManagedRecordedNode, parent: _ManagedRecordedNode
    ) -> bool:
        current: _ManagedRecordedNode | None = node.parent
        while current is not None:
            if current is parent:
                return True
            current = current.parent
        return False

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        source_resource = self._resource(source)
        node = source_resource.node
        destination_node = self._resource(destination_parent).node
        self._emit(f"rename:{node.name}->{destination_name}:{replace}")
        coordinator = self.coordinator
        if coordinator is None or not coordinator.locked:
            raise AssertionError("coordinator was not locked through rename")
        if self.directory_rename_requires_closed_descendants:
            for resource in self.live_resources:
                if resource is source_resource:
                    continue
                if self._is_descendant(resource.node, node):
                    raise AssertionError(
                        f"descendant capability remained open: {resource.node.name}"
                    )
            for descriptor, descendant in self.detached.items():
                if not self._is_descendant(descendant, node):
                    continue
                try:
                    os.fstat(descriptor)
                except OSError:
                    continue
                raise AssertionError(
                    f"descendant descriptor remained open: {descendant.name}"
                )
        if not replace and destination_name in destination_node.children:
            raise FileExistsError(destination_name)
        old_parent = node.parent
        if old_parent is None or old_parent.children.get(node.name) is not node:
            raise OSError("rename source identity changed")
        del old_parent.children[node.name]
        node.parent = destination_node
        node.name = destination_name
        destination_node.children[destination_name] = node
        source._path_hint = self._path(node)

    def delete(
        self, capability: FileCapability | DirectoryCapability
    ) -> None:
        resource = self._resource(capability)
        created_secure_root = (
            isinstance(capability, DirectoryCapability)
            and capability.created
            and capability.security_domain is SecurityDomain.MANAGED
            and capability.share_policy is SharePolicy.MUTATION
            and resource.secure_root_creation
        )
        if (
            capability.share_policy is not SharePolicy.PINNED
            and not created_secure_root
        ):
            raise RuntimeError("recorded delete requires exact mutation authority")
        node = resource.node
        self._emit(f"delete:{node.name}")
        if node.kind is EntryKind.DIRECTORY and node.children:
            raise OSError("recorded directory is not empty")
        parent = node.parent
        if parent is None or parent.children.get(node.name) is not node:
            raise OSError("delete target identity changed")
        del parent.children[node.name]
        capability.close()

    def available_bytes(self, directory: DirectoryCapability) -> int:
        self._resource(directory)
        return 1_000_000

    def allocation_unit(self, directory: DirectoryCapability) -> int:
        self._resource(directory)
        return 4_096

    def touch(self, file: FileCapability) -> None:
        resource = self._resource(file)
        self._emit(f"touch:{resource.node.name}")

    def flush(self, file: FileCapability) -> None:
        resource = self._resource(file)
        self._emit(f"flush:{resource.node.name}")
        os.fsync(resource.descriptor)

    def final_path(self, directory: DirectoryCapability) -> Path:
        return self._path(self._resource(directory).node)

    def verify_managed_security(
        self,
        capability: FileCapability | DirectoryCapability,
        *,
        repair_dacl: bool,
    ) -> None:
        resource = self._resource(capability)
        if capability.security_domain is not SecurityDomain.MANAGED:
            raise PermissionError("recorded capability is not managed")
        self._emit(
            f"verify-managed:{resource.node.name}:repair={str(repair_dacl).lower()}"
        )

    def close_backings(self) -> None:
        self._prune_detached()
        for descriptor in tuple(self.detached):
            try:
                os.close(descriptor)
            except OSError:
                pass
            self.detached.pop(descriptor, None)
        for backing in self.backings:
            try:
                backing.close()
            except OSError:
                pass

    def _prune_detached(self) -> None:
        for descriptor in tuple(self.detached):
            try:
                os.fstat(descriptor)
            except OSError:
                self.detached.pop(descriptor, None)


class _Task8ManagedIterator:
    def __init__(
        self,
        backend: "_Task8ManagedRecordingBackend",
        directory: DirectoryCapability,
    ) -> None:
        self._backend = backend
        self._directory = directory
        resource = backend._resource(directory)
        node = resource.node
        self._name = node.name
        self._entries = tuple(node.children.values())
        self._index = 0
        self._virtual_count = backend.virtual_entry_counts.get(node.identity, 0)
        self._failure = backend.iterator_failure
        backend.iterator_failure = None
        resource.close_failures = (
            backend.iterator_close_failures_by_identity.get(
                node.identity, resource.close_failures
            )
        )
        self._closed = False

    @property
    def directory(self) -> DirectoryCapability:
        return self._directory

    def __iter__(self) -> "_Task8ManagedIterator":
        return self

    def __next__(self) -> DirectoryEntry:
        self._backend._cleanup_operation(f"iterator.next:{self._name}")
        if self._closed:
            raise StopIteration
        if self._failure is not None:
            failure = self._failure
            self._failure = None
            raise failure
        if self._index < self._virtual_count:
            index = self._index
            self._index += 1
            return DirectoryEntry(
                f"virtual-{index:05d}",
                EntryKind.REGULAR,
                FileIdentity(
                    self._backend._filesystem.volume,
                    1_000_000 + index,
                ),
                self._backend._filesystem,
                0,
                0,
            )
        concrete_index = self._index - self._virtual_count
        if concrete_index >= len(self._entries):
            if self._backend.iterator_auto_close:
                self.close()
            raise StopIteration
        self._index += 1
        return self._entries[concrete_index].evidence()

    def close(self) -> None:
        self._backend._cleanup_operation(f"iterator.close:{self._name}")
        if self._closed:
            return
        self._directory.close()
        self._closed = True


class _Task8CoordinatorLease(LeaseLock):
    """Deterministic in-memory coordinator for Task 8 recording tests."""

    def __init__(self, gate: threading.Lock) -> None:
        super().__init__(os.open(os.devnull, os.O_RDONLY))
        self._gate = gate
        self.locked = True

    def release(self) -> None:
        if not self.locked:
            return
        self._gate.release()
        self.locked = False


class _Task8ManagedRecordingBackend(_ManagedRecordingBackend):
    """Capability cleanup oracle kept separate from Task 7 event fixtures."""

    def __init__(
        self,
        parent_path: Path,
        *,
        rename_requires_closed_descendants: bool,
    ) -> None:
        super().__init__(
            parent_path,
            rename_requires_closed_descendants=(
                rename_requires_closed_descendants
            ),
        )
        self.cleanup_operations: list[str] = []
        self.before_cleanup_operation: Callable[[str], None] = (
            lambda _operation: None
        )
        self.after_cleanup_operation: Callable[[str], None] = (
            lambda _operation: None
        )
        self.virtual_entry_counts: dict[FileIdentity, int] = {}
        self.close_failures_by_identity: dict[FileIdentity, int] = {}
        self.iterator_close_failures_by_identity: dict[
            FileIdentity, int
        ] = {}
        self.iterator_auto_close = True
        self.close_counts: dict[FileIdentity, int] = {}
        self.track_directory_resources = False
        self.max_directory_resources = 0
        self.coordinator_gate = threading.Lock()

    def _cleanup_operation(self, operation: str) -> None:
        self.before_cleanup_operation(operation)
        self.cleanup_operations.append(operation)
        self.after_cleanup_operation(operation)

    def _record_directory_resources(self) -> None:
        if not self.track_directory_resources:
            return
        count = sum(
            1
            for resource in self.live_resources
            if not resource.closed
            and resource.node.kind is EntryKind.DIRECTORY
        )
        self.max_directory_resources = max(
            self.max_directory_resources, count
        )

    def _directory_capability(
        self,
        node: _ManagedRecordedNode,
        *,
        share_policy: SharePolicy,
        created: bool,
        secure_root_creation: bool = False,
    ) -> DirectoryCapability:
        capability = super()._directory_capability(
            node,
            share_policy=share_policy,
            created=created,
            secure_root_creation=secure_root_creation,
        )
        self._resource(capability).close_failures = (
            self.close_failures_by_identity.get(node.identity, 0)
        )
        self._record_directory_resources()
        return capability

    def _file_capability(
        self,
        node: _ManagedRecordedNode,
        *,
        share_policy: SharePolicy,
        created: bool,
    ) -> FileCapability:
        capability = super()._file_capability(
            node,
            share_policy=share_policy,
            created=created,
        )
        self._resource(capability).close_failures = (
            self.close_failures_by_identity.get(node.identity, 0)
        )
        return capability

    def close_resource(self, value: object) -> None:
        if isinstance(value, _ManagedRecordedResource):
            identity = value.node.identity
            self.close_counts[identity] = self.close_counts.get(identity, 0) + 1
            self._cleanup_operation(f"close:{value.node.name}")
        super().close_resource(value)
        self._record_directory_resources()

    def open_directory(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> DirectoryCapability:
        self._cleanup_operation(f"open_directory:{name}")
        return super().open_directory(parent, name, share_policy)

    def open_file(
        self,
        parent: DirectoryCapability,
        name: str,
        *,
        access: FileAccess,
        disposition: CreateDisposition,
        share_policy: SharePolicy = SharePolicy.MUTATION,
    ) -> FileCapability:
        self._cleanup_operation(f"open_file:{name}")
        return super().open_file(
            parent,
            name,
            access=access,
            disposition=disposition,
            share_policy=share_policy,
        )

    def open_entry(
        self,
        parent: DirectoryCapability,
        name: str,
        share_policy: SharePolicy,
    ) -> FileCapability | DirectoryCapability:
        self._cleanup_operation(f"open_entry:{name}")
        if name.startswith("virtual-"):
            raise FileNotFoundError(name)
        return super().open_entry(parent, name, share_policy)

    def entry(
        self, parent: DirectoryCapability, name: str
    ) -> DirectoryEntry | None:
        self._cleanup_operation(f"entry:{name}")
        return super().entry(parent, name)

    def entries_owned(
        self, parent: DirectoryCapability
    ) -> _Task8ManagedIterator:
        self._cleanup_operation("entries_owned")
        moved = parent._move_for(self)
        iterator = _Task8ManagedIterator(self, moved)
        self._record_directory_resources()
        return iterator

    def rename(
        self,
        source: FileCapability | DirectoryCapability,
        destination_parent: DirectoryCapability,
        destination_name: str,
        *,
        replace: bool,
    ) -> None:
        source_name = self._resource(source).node.name
        self._cleanup_operation(f"rename:{source_name}->{destination_name}")
        super().rename(
            source,
            destination_parent,
            destination_name,
            replace=replace,
        )

    def delete(
        self, capability: FileCapability | DirectoryCapability
    ) -> None:
        name = self._resource(capability).node.name
        self._cleanup_operation(f"delete:{name}")
        super().delete(capability)

    def final_path(self, directory: DirectoryCapability) -> Path:
        self._cleanup_operation("final_path")
        return super().final_path(directory)


class ManagedPublicationCapabilityTests(unittest.TestCase):
    run_id = "00000000-0000-4000-8000-000000000701"

    def _backend(
        self, *, rename_requires_closed_descendants: bool
    ) -> _ManagedRecordingBackend:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        backend = _ManagedRecordingBackend(
            Path(temporary.name).resolve(),
            rename_requires_closed_descendants=(
                rename_requires_closed_descendants
            ),
        )
        self.addCleanup(backend.close_backings)
        return backend

    def _create(
        self,
        backend: _ManagedRecordingBackend,
        *,
        acquire_failure_call: int | None = None,
        lease_allocation_failure_call: int | None = None,
        allocated_locks: list[LeaseLock] | None = None,
    ) -> ManagedScratch:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_open_coordinator"],
        )
        real_open_coordinator = lease_module._open_coordinator
        real_acquire = LeaseLock.acquire

        def observe_coordinator(*args: object, **kwargs: object) -> LeaseLock:
            coordinator = real_open_coordinator(*args, **kwargs)
            backend.coordinator = coordinator
            return coordinator

        acquire_calls = 0
        allocation_calls = 0

        def allocate_effect(descriptor: int) -> LeaseLock:
            nonlocal allocation_calls
            allocation_calls += 1
            if allocation_calls == lease_allocation_failure_call:
                raise MemoryError("injected lease owner allocation failure")
            lock = LeaseLock(descriptor)
            if allocated_locks is not None:
                allocated_locks.append(lock)
            return lock

        def acquire_effect(lock: LeaseLock, *, blocking: bool) -> None:
            nonlocal acquire_calls
            acquire_calls += 1
            if acquire_calls == acquire_failure_call:
                raise OSError("injected lease relock failure")
            real_acquire(lock, blocking=blocking)

        with (
            mock.patch.object(
                lease_module,
                "reclaim_abandoned",
                return_value=[],
            ),
            mock.patch.object(
                lease_module,
                "_open_coordinator",
                side_effect=observe_coordinator,
            ),
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=acquire_effect,
            ),
            mock.patch.object(
                lease_module,
                "LeaseLock",
                side_effect=allocate_effect,
            ),
        ):
            scratch = ManagedScratch.create(
                backend.parent_path,
                run_id=self.run_id,
                backend=backend,
            )
        self.addCleanup(scratch.close_capabilities)
        return scratch

    @staticmethod
    def _managed_node(
        backend: _ManagedRecordingBackend,
    ) -> _ManagedRecordedNode:
        return backend.parent.children["hoimin-focused-v1"]

    def test_backends_expose_only_the_semantic_directory_rename_feature(
        self,
    ) -> None:
        from tools.focused_mutation_support.posix_filesystem import (
            PosixFilesystemBackend,
        )
        from tools.focused_mutation_support.windows_filesystem import (
            WindowsFilesystemBackend,
        )

        self.assertFalse(
            PosixFilesystemBackend().directory_rename_requires_closed_descendants
        )
        self.assertTrue(
            WindowsFilesystemBackend().directory_rename_requires_closed_descendants
        )

    def test_posix_created_secure_root_has_no_destructive_delete_exception(
        self,
    ) -> None:
        from tools.focused_mutation_support import posix_filesystem as posix_module
        from tools.focused_mutation_support.posix_filesystem import (
            PosixFilesystemBackend,
            _PosixResource,
        )

        backend = PosixFilesystemBackend()
        filesystem = FilesystemIdentity(71)
        parent = DirectoryCapability(
            backend,
            _PosixResource(901, None, None, None),
            identity=FileIdentity(71, 10),
            filesystem=filesystem,
            kind=EntryKind.DIRECTORY,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.CALLER,
            share_policy=SharePolicy.MUTATION,
            created=False,
            path_hint=Path("/recorded-parent"),
        )
        created_root = DirectoryCapability(
            backend,
            _PosixResource(902, parent, "managed", None),
            identity=FileIdentity(71, 20),
            filesystem=filesystem,
            kind=EntryKind.DIRECTORY,
            logical_size=0,
            modified_ns=0,
            security_domain=SecurityDomain.MANAGED,
            share_policy=SharePolicy.MUTATION,
            created=True,
            path_hint=Path("/recorded-parent/managed"),
        )
        with (
            mock.patch.object(posix_module.os, "stat") as observe,
            mock.patch.object(posix_module.os, "rmdir") as rmdir,
            mock.patch.object(posix_module.os, "close"),
            self.assertRaises(ValueError),
        ):
            backend.delete(created_root)
        self.assertFalse(
            backend._directory_creation_rollback_available(created_root)
        )
        observe.assert_not_called()
        rmdir.assert_not_called()
        self.assertTrue(created_root.is_open)
        with mock.patch.object(posix_module.os, "close"):
            created_root.close()
            parent.close()

    def _assert_posix_unbound_directory_rollback_is_non_destructive(
        self,
        *,
        role: str,
        secure_root: bool,
    ) -> None:
        from tools.focused_mutation_support import posix_filesystem as posix_module
        from tools.focused_mutation_support import lease as lease_module
        from tools.focused_mutation_support.posix_filesystem import (
            PosixFilesystemBackend,
            _PosixResource,
        )

        backend = PosixFilesystemBackend()
        filesystem = FilesystemIdentity(81)
        parent = DirectoryCapability(
            backend,
            _PosixResource(911, None, None, None),
            identity=FileIdentity(81, 10),
            filesystem=filesystem,
            kind=EntryKind.DIRECTORY,
            logical_size=0,
            modified_ns=0,
            security_domain=(
                SecurityDomain.CALLER
                if secure_root
                else SecurityDomain.MANAGED
            ),
            share_policy=SharePolicy.MUTATION,
            created=False,
            path_hint=Path("/recorded-parent"),
        )
        name = "hoimin-focused-v1" if secure_root else role
        replacement = DirectoryEntry(
            name,
            EntryKind.DIRECTORY,
            FileIdentity(81, 999),
            filesystem,
            0,
            0,
        )
        namespace = {name: replacement}

        def finish_created(
            owner: DirectoryCapability,
            component: str,
            expected: DirectoryEntry,
            share_policy: SharePolicy,
            *,
            security_domain: SecurityDomain,
            verify_security: bool,
        ) -> DirectoryCapability:
            del verify_security
            return DirectoryCapability(
                backend,
                _PosixResource(912, owner, component, None),
                identity=expected.identity,
                filesystem=expected.filesystem,
                kind=EntryKind.DIRECTORY,
                logical_size=0,
                modified_ns=0,
                security_domain=security_domain,
                share_policy=share_policy,
                created=True,
                path_hint=owner.path_hint / component,
            )

        def remove_replacement(component: str, *, dir_fd: int) -> None:
            self.assertEqual(dir_fd, 911)
            namespace.pop(component)

        with (
            mock.patch.object(posix_module.os, "mkdir") as mkdir,
            mock.patch.object(posix_module.os, "close"),
            mock.patch.object(
                backend,
                "entry",
                side_effect=lambda _parent, component: namespace.get(component),
            ) as entry,
            mock.patch.object(
                backend,
                "_entry_at_fd",
                side_effect=lambda _parent, component, _fd: namespace.get(
                    component
                ),
            ) as entry_at_fd,
            mock.patch.object(
                backend,
                "_finish_created_directory",
                side_effect=finish_created,
            ),
            mock.patch.object(
                posix_module.os,
                "rmdir",
                side_effect=remove_replacement,
            ) as rmdir,
        ):
            if secure_root:
                directory = backend.create_secure_root(parent, name)
            else:
                directory = backend.create_directory(
                    parent, name, SharePolicy.PINNED
                )
            primary = OSError(f"injected {role} post-create primary")
            if secure_root:
                rollback_errors = lease_module._rollback_created_managed_root(
                    directory, parent, backend
                )
            else:
                rollback_errors = lease_module._delete_owned_directory(
                    parent,
                    name,
                    directory,
                    backend,
                    label=f"managed {role} rollback",
                )
            for rollback_error in rollback_errors:
                primary.add_note(rollback_error)
            parent.close()

        self.assertEqual(str(primary), f"injected {role} post-create primary")
        notes = tuple(getattr(primary, "__notes__", ()))
        self.assertEqual(
            sum("rollback unavailable" in note for note in notes), 1
        )
        self.assertTrue(
            all(len(note.encode("utf-8")) <= 1_024 for note in notes)
        )
        self.assertIs(namespace[name], replacement)
        mkdir.assert_called_once()
        rmdir.assert_not_called()
        entry_at_fd.assert_not_called()
        self.assertEqual(entry.call_count, 1)

    def test_posix_unbound_secure_root_rollback_preserves_replacement(
        self,
    ) -> None:
        self._assert_posix_unbound_directory_rollback_is_non_destructive(
            role="secure root", secure_root=True
        )

    def test_posix_unbound_staging_rollback_preserves_replacement(self) -> None:
        self._assert_posix_unbound_directory_rollback_is_non_destructive(
            role="staging", secure_root=False
        )

    def test_posix_unbound_child_rollback_preserves_replacement(self) -> None:
        self._assert_posix_unbound_directory_rollback_is_non_destructive(
            role="child", secure_root=False
        )

    def test_successful_managed_root_handoff_disarms_rollback_exception(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_ensure_managed_root"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)

        root_path, managed = lease_module._ensure_managed_root(
            backend.parent_path,
            backend=backend,
        )

        self.assertEqual(root_path, backend.parent_path / "hoimin-focused-v1")
        with self.assertRaises(RuntimeError):
            backend.delete(managed)
        self.assertIn("hoimin-focused-v1", backend.parent.children)
        parent_close = f"close:{backend.parent.name}"
        parent_close_index = backend.events.index(parent_close)
        self.assertEqual(
            backend.events[parent_close_index:],
            [parent_close, "commit-secure-root:hoimin-focused-v1"],
        )
        managed.close()

    def test_parent_close_failure_rolls_back_before_one_parent_retry(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_ensure_managed_root"],
        )
        for persistent in (False, True):
            with self.subTest(persistent=persistent):
                backend = self._backend(
                    rename_requires_closed_descendants=False
                )
                armed = False
                replacement: _ManagedRecordedNode | None = None
                parent_close_event = f"close:{backend.parent.name}"

                def inject_parent_close(event: str) -> None:
                    nonlocal armed, replacement
                    if event == "entry:hoimin-focused-v1" and not armed:
                        armed = True
                        for resource in backend.live_resources:
                            if resource.node is backend.parent:
                                resource.close_failures = 2 if persistent else 1
                    if (
                        persistent
                        and event == parent_close_event
                        and replacement is None
                    ):
                        original = backend.parent.children.pop(
                            "hoimin-focused-v1"
                        )
                        backend.parent.children["displaced-original"] = original
                        replacement = backend._new_node(
                            EntryKind.DIRECTORY,
                            SecurityDomain.MANAGED,
                            parent=backend.parent,
                            name="hoimin-focused-v1",
                        )

                backend.after_event = inject_parent_close
                with self.assertRaisesRegex(
                    OSError, "injected close failure"
                ) as caught:
                    lease_module._ensure_managed_root(
                        backend.parent_path,
                        backend=backend,
                    )

                self.assertEqual(
                    backend.events.count(parent_close_event),
                    2,
                )
                notes: tuple[str, ...] = tuple(
                    getattr(caught.exception, "__notes__", ())
                )
                self.assertTrue(
                    all(len(note.encode("utf-8")) <= 1_024 for note in notes)
                )
                if persistent:
                    self.assertIs(
                        backend.parent.children["hoimin-focused-v1"],
                        replacement,
                    )
                    self.assertIn("displaced-original", backend.parent.children)
                    self.assertTrue(
                        any("rollback unavailable" in note for note in notes)
                    )
                    self.assertTrue(
                        any("parent close failed" in note for note in notes)
                    )
                    self.assertNotIn(
                        "delete:hoimin-focused-v1", backend.events
                    )
                else:
                    self.assertNotIn(
                        "hoimin-focused-v1", backend.parent.children
                    )
                    self.assertEqual(len(backend.live_resources), 0)
                retained_parent = backend.last_parent_capability
                assert retained_parent is not None
                if retained_parent.is_open:
                    retained_parent.close()

    def test_bootstrap_graph_is_capability_relative_and_coordinator_serialized(
        self,
    ) -> None:
        for requires_close in (False, True):
            with self.subTest(requires_close=requires_close):
                backend = self._backend(
                    rename_requires_closed_descendants=requires_close
                )
                coordinator_checked_events: list[str] = []

                def require_coordinator(event: str) -> None:
                    if event.startswith(("rename:", "entry:run-")):
                        coordinator = backend.coordinator
                        self.assertIsNotNone(coordinator)
                        assert coordinator is not None
                        self.assertTrue(coordinator.locked)
                        coordinator_checked_events.append(event)

                backend.after_event = require_coordinator
                scratch = self._create(backend)

                expected_order = [
                    "open-parent:mutation",
                    "create-secure-root:hoimin-focused-v1",
                    "verify-managed:hoimin-focused-v1:repair=true",
                    "open_or_create:.hoimin-coordinator:read_write:pinned",
                    "verify-managed:.hoimin-coordinator:repair=true",
                    "detach:.hoimin-coordinator",
                    f"create-directory:.staging-{self.run_id}:pinned",
                    f"verify-managed:.staging-{self.run_id}:repair=false",
                    "create_new:.hoimin-lease.json:read_write:pinned",
                    "verify-managed:.hoimin-lease.json:repair=false",
                    "create_new:.hoimin-heartbeat.json:read_write:pinned",
                    "verify-managed:.hoimin-heartbeat.json:repair=false",
                    f"rename:.staging-{self.run_id}->run-{self.run_id}:False",
                    f"entry:run-{self.run_id}",
                ]
                cursor = 0
                for expected in expected_order:
                    cursor = backend.events.index(expected, cursor) + 1

                self.assertTrue(coordinator_checked_events)
                self.assertIsNotNone(backend.coordinator)
                assert backend.coordinator is not None
                self.assertFalse(backend.coordinator.locked)
                self.assertEqual(backend.coordinator.fd, -1)
                self.assertIsInstance(scratch._managed_root_capability, DirectoryCapability)
                self.assertIsInstance(scratch._root, DirectoryCapability)
                self.assertIsInstance(scratch._heartbeat, FileCapability)
                lease = scratch._lease
                managed_root_capability = scratch._managed_root_capability
                root = scratch._root
                heartbeat = scratch._heartbeat
                assert lease is not None
                assert managed_root_capability is not None
                assert root is not None
                assert heartbeat is not None
                self.assertTrue(lease.locked)
                self.assertTrue(managed_root_capability.is_open)
                self.assertTrue(root.is_open)
                self.assertTrue(heartbeat.is_open)
                self.assertTrue(lease.fd >= 0)
                self.assertNotIn("open-parent:pinned", backend.events)

                self.assertEqual(scratch.close_capabilities(), ())
                self.assertEqual(len(backend.live_resources), 0)

    def test_constructor_prechecks_active_and_never_calls_namespace_rename(
        self,
    ) -> None:
        backend = self._backend(rename_requires_closed_descendants=False)
        managed = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=backend.parent,
            name="hoimin-focused-v1",
        )
        active_name = f"run-{self.run_id}"
        active = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=managed,
            name=active_name,
        )
        original_identity = active.identity
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["reclaim_abandoned"],
        )

        with (
            mock.patch.object(lease_module, "reclaim_abandoned", return_value=[]),
            self.assertRaises(FileExistsError),
        ):
            ManagedScratch.create(
                backend.parent_path,
                run_id=self.run_id,
                backend=backend,
            )

        self.assertEqual(managed.children[active_name].identity, original_identity)
        self.assertFalse(any(event.startswith("rename:") for event in backend.events))
        self.assertNotIn(f".staging-{self.run_id}", managed.children)
        self.assertEqual(len(backend.live_resources), 0)

    def test_marker_failure_rolls_back_only_the_exact_created_identity(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_create_marker"],
        )
        for replacement in (False, True):
            with self.subTest(replacement=replacement):
                backend = self._backend(
                    rename_requires_closed_descendants=False
                )
                parent = backend.open_root(
                    backend.parent_path, SharePolicy.MUTATION
                )
                managed = backend.create_secure_root(parent, "hoimin-focused-v1")
                parent.close()
                marker_name = ".hoimin-lease.json"
                marker_value = {
                    "schema_version": 1,
                    "run_id": self.run_id,
                    "owner_kind": "focused_python",
                    "lease_id": "00000000-0000-4000-8000-000000000702",
                }
                replacement_node: _ManagedRecordedNode | None = None

                def fail_verification(event: str) -> None:
                    nonlocal replacement_node
                    if event != f"verify-managed:{marker_name}:repair=false":
                        return
                    if replacement:
                        managed_node = backend._resource(managed).node
                        original = managed_node.children.pop(marker_name)
                        original.name = f"{marker_name}.original"
                        managed_node.children[original.name] = original
                        replacement_node = backend._new_node(
                            EntryKind.REGULAR,
                            SecurityDomain.MANAGED,
                            parent=managed_node,
                            name=marker_name,
                        )
                    raise OSError("injected marker verification failure")

                backend.after_event = fail_verification
                with self.assertRaisesRegex(OSError, "verification failure"):
                    lease_module._create_marker(
                        managed,
                        marker_name,
                        marker_value,
                        backend,
                    )

                current = backend._resource(managed).node.children.get(marker_name)
                if replacement:
                    self.assertIs(current, replacement_node)
                else:
                    self.assertIsNone(current)
                self.assertEqual(len(backend.live_resources), 1)
                managed.close()

    def test_marker_result_allocation_precedes_descriptor_handoff(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_create_marker"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)
        parent = backend.open_root(backend.parent_path, SharePolicy.MUTATION)
        managed = backend.create_secure_root(parent, "hoimin-focused-v1")
        parent.close()
        marker_name = ".hoimin-lease.json"

        with (
            mock.patch.object(
                lease_module,
                "_marker_result",
                side_effect=MemoryError("injected marker result allocation failure"),
            ),
            self.assertRaisesRegex(MemoryError, "result allocation failure"),
        ):
            lease_module._create_marker(
                managed,
                marker_name,
                {
                    "schema_version": 1,
                    "run_id": self.run_id,
                    "owner_kind": "focused_python",
                    "lease_id": "00000000-0000-4000-8000-000000000708",
                },
                backend,
            )

        self.assertNotIn(
            marker_name, backend._resource(managed).node.children
        )
        for descriptor in backend.detached:
            with self.assertRaises(OSError):
                os.fstat(descriptor)
        self.assertEqual(len(backend.live_resources), 1)
        managed.close()

    def test_marker_read_returns_none_only_for_absence_and_rejects_overflow(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_create_marker", "_read_marker"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)
        parent = backend.open_root(backend.parent_path, SharePolicy.MUTATION)
        managed = backend.create_secure_root(parent, "hoimin-focused-v1")
        parent.close()
        lease_id = "00000000-0000-4000-8000-000000000703"
        value = {
            "schema_version": 1,
            "run_id": self.run_id,
            "owner_kind": "focused_python",
            "lease_id": lease_id,
        }
        identity, descriptor = lease_module._create_marker(
            managed,
            ".hoimin-lease.json",
            value,
            backend,
        )
        os.close(descriptor)

        self.assertIsNone(
            lease_module._read_marker(
                managed,
                ".hoimin-heartbeat.json",
                backend,
                expected_run_id=self.run_id,
                expected_lease_id=lease_id,
            )
        )
        self.assertEqual(
            lease_module._read_marker(
                managed,
                ".hoimin-lease.json",
                backend,
                expected_run_id=self.run_id,
                expected_lease_id=lease_id,
            ),
            (identity, value),
        )

        node = backend._resource(managed).node.children[".hoimin-lease.json"]
        assert node.backing is not None
        node.backing.seek(0)
        node.backing.truncate(0)
        node.backing.write(b"x" * (lease_module.MARKER_CAPACITY + 1))
        node.backing.flush()
        with self.assertRaisesRegex(OSError, "capacity"):
            lease_module._read_marker(
                managed,
                ".hoimin-lease.json",
                backend,
                expected_run_id=self.run_id,
                expected_lease_id=lease_id,
            )
        managed.close()

    def test_marker_fd_close_precedes_rollback_and_retries_same_owner(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_create_marker"],
        )
        value = {
            "schema_version": 1,
            "run_id": self.run_id,
            "owner_kind": "focused_python",
            "lease_id": "00000000-0000-4000-8000-000000000705",
        }
        for close_failures in (1, 2):
            with self.subTest(close_failures=close_failures):
                backend = self._backend(
                    rename_requires_closed_descendants=False
                )
                parent = backend.open_root(
                    backend.parent_path, SharePolicy.MUTATION
                )
                managed = backend.create_secure_root(
                    parent, "hoimin-focused-v1"
                )
                parent.close()
                timeline: list[str] = []
                real_close = lease_module.os.close
                close_calls = 0

                def fail_write(_fd: int, _value: bytes) -> int:
                    timeline.append("write")
                    raise OSError("injected marker write failure")

                def close_with_failures(descriptor: int) -> None:
                    nonlocal close_calls
                    close_calls += 1
                    timeline.append(f"close:{close_calls}")
                    if close_calls <= close_failures:
                        raise OSError("injected marker close failure")
                    real_close(descriptor)

                backend.after_event = timeline.append
                with (
                    mock.patch.object(
                        lease_module.os, "write", side_effect=fail_write
                    ),
                    mock.patch.object(
                        lease_module.os,
                        "close",
                        side_effect=close_with_failures,
                    ),
                    self.assertRaisesRegex(
                        OSError, "marker write failure"
                    ) as caught,
                ):
                    lease_module._create_marker(
                        managed,
                        ".hoimin-lease.json",
                        value,
                        backend,
                    )

                self.assertGreaterEqual(close_calls, 2)
                self.assertTrue(
                    all(
                        len(note.encode("utf-8")) <= 1_024
                        for note in getattr(caught.exception, "__notes__", ())
                    )
                )
                marker = backend._resource(managed).node.children.get(
                    ".hoimin-lease.json"
                )
                if close_failures == 1:
                    self.assertIsNone(marker)
                    self.assertGreater(
                        timeline.index(
                            "open-entry:.hoimin-lease.json:pinned"
                        ),
                        timeline.index("close:2"),
                    )
                else:
                    self.assertIsNotNone(marker)
                    self.assertNotIn(
                        "open-entry:.hoimin-lease.json:pinned", timeline
                    )
                    self.assertFalse(
                        any(event.startswith("delete:") for event in timeline)
                    )
                managed.close()

    def test_marker_cleanup_baseexception_stays_secondary_to_primary(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_create_marker"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)
        parent = backend.open_root(backend.parent_path, SharePolicy.MUTATION)
        managed = backend.create_secure_root(parent, "hoimin-focused-v1")
        parent.close()
        marker_name = ".hoimin-lease.json"
        backend.failures[f"close:{marker_name}"] = MemoryError(
            "injected cleanup allocation failure"
        )

        def fail_verification(event: str) -> None:
            if event == f"verify-managed:{marker_name}:repair=false":
                raise OSError("injected verification primary")

        backend.after_event = fail_verification
        with self.assertRaisesRegex(
            OSError, "verification primary"
        ) as caught:
            lease_module._create_marker(
                managed,
                marker_name,
                {
                    "schema_version": 1,
                    "run_id": self.run_id,
                    "owner_kind": "focused_python",
                    "lease_id": "00000000-0000-4000-8000-000000000706",
                },
                backend,
            )

        notes = getattr(caught.exception, "__notes__", ())
        self.assertTrue(any("MemoryError" in note for note in notes))
        self.assertTrue(
            all(len(note.encode("utf-8")) <= 1_024 for note in notes)
        )
        self.assertNotIn(
            marker_name, backend._resource(managed).node.children
        )
        managed.close()

    def test_marker_delete_waits_for_consuming_close_before_absence_check(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_create_marker", "_delete_exact_marker"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)
        parent = backend.open_root(backend.parent_path, SharePolicy.MUTATION)
        managed = backend.create_secure_root(parent, "hoimin-focused-v1")
        parent.close()
        marker_name = ".hoimin-lease.json"
        identity, descriptor = lease_module._create_marker(
            managed,
            marker_name,
            {
                "schema_version": 1,
                "run_id": self.run_id,
                "owner_kind": "focused_python",
                "lease_id": "00000000-0000-4000-8000-000000000707",
            },
            backend,
        )
        os.close(descriptor)
        marker_node = backend._resource(managed).node.children[marker_name]

        close_attempts = 0
        rollback_capabilities: list[FileCapability | DirectoryCapability] = []
        target_resource: _ManagedRecordedResource | None = None
        real_open_entry = backend.open_entry
        real_close_resource = backend.close_resource
        inject_close_failure = True

        def open_entry(
            owner: DirectoryCapability,
            name: str,
            share_policy: SharePolicy,
        ) -> FileCapability | DirectoryCapability:
            nonlocal target_resource
            capability = real_open_entry(owner, name, share_policy)
            if name == marker_name:
                rollback_capabilities.append(capability)
                target_resource = backend._resource(capability)
            return capability

        def close_resource(value: object) -> None:
            nonlocal close_attempts
            if value is target_resource:
                close_attempts += 1
                if inject_close_failure:
                    assert isinstance(value, _ManagedRecordedResource)
                    value.close_failures = 1
            real_close_resource(value)

        start = len(backend.events)
        with (
            mock.patch.object(
                backend, "open_entry", side_effect=open_entry
            ),
            mock.patch.object(
                backend, "close_resource", side_effect=close_resource
            ),
        ):
            errors = lease_module._delete_exact_marker(
                managed,
                marker_name,
                identity,
                backend,
            )

        cleanup_events = backend.events[start:]
        self.assertEqual(close_attempts, 2)
        self.assertTrue(any("close failed" in error for error in errors))
        self.assertNotIn(f"entry:{marker_name}", cleanup_events)
        self.assertEqual(len(rollback_capabilities), 1)
        retained = rollback_capabilities[0]
        self.assertTrue(retained.is_open)
        self.assertIsNotNone(target_resource)
        assert target_resource is not None
        self.assertFalse(target_resource.closed)

        inject_close_failure = False
        finalizer_start = len(backend.events)
        retained.__del__()
        self.assertEqual(
            backend.events[finalizer_start:], [f"close:{marker_name}"]
        )
        self.assertTrue(retained.closed)
        self.assertTrue(target_resource.closed)
        self.assertNotIn(marker_name, backend._resource(managed).node.children)
        self.assertIs(marker_node.parent, backend._resource(managed).node)
        managed.close()

    def test_marker_rollback_owner_is_registered_before_outer_cleanup(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_MarkerRollbackOwners", "_close_locked_coordinator_once"],
        )
        active_name = f"run-{self.run_id}"
        staging_name = f".staging-{self.run_id}"
        marker_name = ".hoimin-heartbeat.json"
        namespace_prefixes = ("rename:", "delete:", "entry:", "open-entry:")

        for phase in ("pre-publication", "post-publication"):
            for marker_case in ("matching", "replacement"):
                for persistent in (False, True):
                    with self.subTest(
                        phase=phase,
                        marker_case=marker_case,
                        persistent=persistent,
                    ):
                        backend = self._backend(
                            rename_requires_closed_descendants=True
                        )
                        real_registry = lease_module._MarkerRollbackOwners
                        real_open_entry = backend.open_entry
                        real_close_resource = backend.close_resource
                        real_close_coordinator = (
                            lease_module._close_locked_coordinator_once
                        )
                        registries: list[_MarkerRollbackOwners] = []
                        rollback_capabilities: list[
                            FileCapability | DirectoryCapability
                        ] = []
                        target_resource: _ManagedRecordedResource | None = None
                        replacement: _ManagedRecordedNode | None = None
                        close_attempts = 0
                        target_close_indices: list[int] = []
                        active_entries = 0
                        inject_close_failure = True

                        def allocate_registry() -> _MarkerRollbackOwners:
                            registry = real_registry()
                            registries.append(registry)
                            return registry

                        def inject_primary(event: str) -> None:
                            nonlocal active_entries
                            if event != f"entry:{active_name}":
                                return
                            active_entries += 1
                            expected = 2 if phase == "pre-publication" else 3
                            if active_entries == expected:
                                raise OSError(
                                    f"injected {phase} publication primary"
                                )

                        def open_entry(
                            owner: DirectoryCapability,
                            name: str,
                            share_policy: SharePolicy,
                        ) -> FileCapability | DirectoryCapability:
                            nonlocal target_resource, replacement
                            if name == marker_name and not rollback_capabilities:
                                parent_node = backend._resource(owner).node
                                if marker_case == "replacement":
                                    displaced = parent_node.children.pop(name)
                                    displaced.parent = None
                                    replacement = backend._new_node(
                                        EntryKind.REGULAR,
                                        SecurityDomain.MANAGED,
                                        parent=parent_node,
                                        name=name,
                                    )
                            capability = real_open_entry(
                                owner, name, share_policy
                            )
                            if name == marker_name and not rollback_capabilities:
                                rollback_capabilities.append(capability)
                                target_resource = backend._resource(capability)
                            return capability

                        def close_resource(value: object) -> None:
                            nonlocal close_attempts
                            is_target = value is target_resource
                            if is_target:
                                close_attempts += 1
                                if inject_close_failure and (
                                    persistent or close_attempts == 1
                                ):
                                    assert isinstance(
                                        value, _ManagedRecordedResource
                                    )
                                    value.close_failures = 1
                            try:
                                real_close_resource(value)
                            finally:
                                if is_target:
                                    target_close_indices.append(
                                        len(backend.events) - 1
                                    )

                        def close_coordinator(
                            lock: LeaseLock, label: str
                        ) -> tuple[str, ...]:
                            errors = real_close_coordinator(lock, label)
                            if lock is backend.coordinator and lock.fd < 0:
                                backend.events.append("close-coordinator-lock")
                            return errors

                        backend.after_event = inject_primary
                        with (
                            mock.patch.object(
                                lease_module,
                                "_MarkerRollbackOwners",
                                side_effect=allocate_registry,
                            ),
                            mock.patch.object(
                                backend, "open_entry", side_effect=open_entry
                            ),
                            mock.patch.object(
                                backend,
                                "close_resource",
                                side_effect=close_resource,
                            ),
                            mock.patch.object(
                                lease_module,
                                "_close_locked_coordinator_once",
                                side_effect=close_coordinator,
                            ),
                            self.assertRaisesRegex(
                                OSError, f"injected {phase} publication primary"
                            ) as caught,
                        ):
                            self._create(backend)

                        self.assertEqual(len(registries), 1)
                        self.assertEqual(len(rollback_capabilities), 1)
                        retained = rollback_capabilities[0]
                        observed_open = retained.is_open
                        observed_attempts = close_attempts
                        notes = tuple(
                            getattr(caught.exception, "__notes__", ())
                        )
                        self.assertGreaterEqual(len(target_close_indices), 2)
                        after_second_close = backend.events[
                            target_close_indices[1] + 1 :
                        ]
                        managed = self._managed_node(backend)
                        if persistent:
                            registry = registries[0]
                            slot = registry.slot(marker_name)
                            self.assertIs(slot.capability, retained)
                            self.assertTrue(slot.has_open_owner())
                            self.assertEqual(observed_attempts, 2)
                            self.assertTrue(observed_open)
                            self.assertEqual(
                                [
                                    event
                                    for event in after_second_close
                                    if event.startswith(namespace_prefixes)
                                ],
                                [],
                            )
                            self.assertEqual(
                                after_second_close,
                                [
                                    f"close:{staging_name}",
                                    "close:hoimin-focused-v1",
                                    "close-coordinator-lock",
                                ],
                            )
                            self.assertEqual(
                                sum(
                                    "cleanup unavailable" in note
                                    for note in notes
                                ),
                                1,
                            )
                            self.assertTrue(
                                all(
                                    len(note.encode("utf-8")) <= 1_024
                                    for note in notes
                                )
                            )
                            self.assertIn(staging_name, managed.children)
                        else:
                            self.assertEqual(observed_attempts, 2)
                            self.assertFalse(observed_open)
                            self.assertFalse(
                                any(
                                    "cleanup unavailable" in note
                                    for note in notes
                                )
                            )
                            if marker_case == "matching":
                                self.assertNotIn(staging_name, managed.children)
                            else:
                                self.assertIn(staging_name, managed.children)

                        if marker_case == "replacement":
                            self.assertIsNotNone(replacement)
                            assert replacement is not None
                            self.assertIs(
                                managed.children[staging_name].children[
                                    marker_name
                                ],
                                replacement,
                            )

                        inject_close_failure = False
                        finalizer_start = len(backend.events)
                        retained.__del__()
                        finalizer_events = backend.events[finalizer_start:]
                        if persistent:
                            self.assertEqual(
                                finalizer_events, [f"close:{marker_name}"]
                            )
                            self.assertTrue(retained.closed)
                            self.assertIsNotNone(target_resource)
                            assert target_resource is not None
                            self.assertTrue(target_resource.closed)
                        else:
                            self.assertEqual(finalizer_events, [])

    def test_marker_creation_rollback_propagates_retained_owner_state(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_MarkerRollbackOwners", "_close_locked_coordinator_once"],
        )
        staging_name = f".staging-{self.run_id}"
        marker_name = ".hoimin-lease.json"
        real_registry = lease_module._MarkerRollbackOwners
        backend = self._backend(rename_requires_closed_descendants=True)
        real_open_entry = backend.open_entry
        real_close_resource = backend.close_resource
        real_close_coordinator = lease_module._close_locked_coordinator_once
        registries: list[_MarkerRollbackOwners] = []
        retained: list[FileCapability | DirectoryCapability] = []
        target_resource: _ManagedRecordedResource | None = None
        close_attempts = 0
        target_close_indices: list[int] = []
        inject_close_failure = True

        def allocate_registry() -> _MarkerRollbackOwners:
            registry = real_registry()
            registries.append(registry)
            return registry

        def inject_primary(event: str) -> None:
            if event == f"verify-managed:{marker_name}:repair=false":
                raise OSError("injected marker creation verification primary")

        def open_entry(
            owner: DirectoryCapability,
            name: str,
            share_policy: SharePolicy,
        ) -> FileCapability | DirectoryCapability:
            nonlocal target_resource
            capability = real_open_entry(owner, name, share_policy)
            if name == marker_name:
                retained.append(capability)
                target_resource = backend._resource(capability)
            return capability

        def close_resource(value: object) -> None:
            nonlocal close_attempts
            is_target = value is target_resource
            if is_target:
                close_attempts += 1
                if inject_close_failure:
                    assert isinstance(value, _ManagedRecordedResource)
                    value.close_failures = 1
            try:
                real_close_resource(value)
            finally:
                if is_target:
                    target_close_indices.append(len(backend.events) - 1)

        def close_coordinator(
            lock: LeaseLock, label: str
        ) -> tuple[str, ...]:
            errors = real_close_coordinator(lock, label)
            if lock is backend.coordinator and lock.fd < 0:
                backend.events.append("close-coordinator-lock")
            return errors

        backend.after_event = inject_primary
        with (
            mock.patch.object(
                lease_module,
                "_MarkerRollbackOwners",
                side_effect=allocate_registry,
            ),
            mock.patch.object(
                backend, "open_entry", side_effect=open_entry
            ),
            mock.patch.object(
                backend, "close_resource", side_effect=close_resource
            ),
            mock.patch.object(
                lease_module,
                "_close_locked_coordinator_once",
                side_effect=close_coordinator,
            ),
            self.assertRaisesRegex(
                OSError, "marker creation verification primary"
            ) as caught,
        ):
            self._create(backend)

        self.assertEqual(len(registries), 1)
        self.assertEqual(len(retained), 1)
        capability = retained[0]
        self.assertEqual(close_attempts, 2)
        self.assertTrue(capability.is_open)
        registry = registries[0]
        self.assertIs(registry.slot(marker_name).capability, capability)
        self.assertTrue(registry.has_open_owner())
        self.assertEqual(len(target_close_indices), 2)
        after_second_rollback_close = backend.events[
            target_close_indices[1] + 1 :
        ]
        self.assertEqual(
            after_second_rollback_close,
            [
                f"close:{staging_name}",
                "close:hoimin-focused-v1",
                "close-coordinator-lock",
            ],
        )
        notes = tuple(getattr(caught.exception, "__notes__", ()))
        self.assertEqual(
            sum("cleanup unavailable" in note for note in notes), 1
        )
        self.assertTrue(
            all(len(note.encode("utf-8")) <= 1_024 for note in notes)
        )
        self.assertIn(staging_name, self._managed_node(backend).children)

        inject_close_failure = False
        finalizer_start = len(backend.events)
        capability.__del__()
        self.assertEqual(
            backend.events[finalizer_start:], [f"close:{marker_name}"]
        )
        self.assertTrue(capability.closed)

    def test_windows_handoff_failures_unpublish_and_release_every_owner(self) -> None:
        stages = (
            "lease-read",
            "lease-content",
            "lease-reopen",
            "lease-relock",
            "heartbeat-reopen",
            "post-rename-root-identity",
        )
        for stage in stages:
            with self.subTest(stage=stage):
                backend = self._backend(
                    rename_requires_closed_descendants=True
                )
                event_counts: dict[str, int] = {}

                def inject_event(event: str) -> None:
                    event_counts[event] = event_counts.get(event, 0) + 1
                    if stage == "lease-read" and event == (
                        "open_existing:.hoimin-lease.json:read:pinned"
                    ):
                        raise OSError("injected lease read failure")
                    if stage == "lease-content" and event == (
                        "open_existing:.hoimin-lease.json:read:pinned"
                    ):
                        managed = self._managed_node(backend)
                        active = managed.children[f"run-{self.run_id}"]
                        lease = active.children[".hoimin-lease.json"]
                        assert lease.backing is not None
                        lease.backing.seek(0)
                        lease.backing.truncate(0)
                        lease.backing.write(b"{}\n")
                        lease.backing.flush()
                    if stage == "lease-reopen" and event == (
                        "open_existing:.hoimin-lease.json:read_write:pinned"
                    ):
                        raise OSError("injected lease reopen failure")
                    if stage == "heartbeat-reopen" and event == (
                        "open_existing:.hoimin-heartbeat.json:write:pinned"
                    ) and event_counts[event] == 2:
                        raise OSError("injected heartbeat reopen failure")
                    if stage == "post-rename-root-identity" and event == (
                        f"entry:run-{self.run_id}"
                    ):
                        raise OSError("injected root identity failure")

                backend.after_event = inject_event
                with self.assertRaises(OSError):
                    self._create(
                        backend,
                        acquire_failure_call=(
                            3 if stage == "lease-relock" else None
                        ),
                    )

                managed = self._managed_node(backend)
                self.assertFalse(
                    any(
                        name.startswith(("run-", ".staging-", ".deleting-"))
                        for name in managed.children
                    )
                )
                self.assertEqual(
                    set(managed.children), {".hoimin-coordinator"}
                )
                coordinator = backend.coordinator
                self.assertIsNotNone(coordinator)
                assert coordinator is not None
                self.assertEqual(coordinator.fd, -1)
                self.assertFalse(coordinator.locked)
                self.assertEqual(len(backend.live_resources), 0)
                for descriptor in backend.detached:
                    with self.assertRaises(OSError):
                        os.fstat(descriptor)

    def test_windows_pre_rename_close_failures_retry_once_and_retain_owner(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_locked_coordinator_once"],
        )
        active_name = f"run-{self.run_id}"
        staging_name = f".staging-{self.run_id}"
        retained_owner_sets: list[
            tuple[
                str,
                bool,
                list[FileCapability],
                list[LeaseLock],
            ]
        ] = []
        for boundary in ("heartbeat", "lease"):
            for persistent in (False, True):
                with self.subTest(boundary=boundary, persistent=persistent):
                    backend = self._backend(
                        rename_requires_closed_descendants=True
                    )
                    allocated_locks: list[LeaseLock] = []
                    close_attempts = 0
                    target_descriptor: int | None = None
                    target_heartbeat: _ManagedRecordedResource | None = None
                    retained_heartbeat_capabilities: list[FileCapability] = []
                    retained_owner_sets.append(
                        (
                            boundary,
                            persistent,
                            retained_heartbeat_capabilities,
                            allocated_locks,
                        )
                    )
                    replacement: _ManagedRecordedNode | None = None
                    rollback_lock_states: list[bool] = []
                    rollback_started = False
                    owner_unavailable = False
                    namespace_after_owner_unavailable: list[str] = []
                    real_close_resource = backend.close_resource
                    real_open_file = backend.open_file
                    real_close_descriptor = lease_module.os.close
                    real_close_coordinator = (
                        lease_module._close_locked_coordinator_once
                    )

                    def install_replacement() -> None:
                        nonlocal replacement
                        if replacement is not None:
                            return
                        replacement = backend._new_node(
                            EntryKind.DIRECTORY,
                            SecurityDomain.MANAGED,
                            parent=self._managed_node(backend),
                            name=active_name,
                        )

                    def close_resource(value: object) -> None:
                        nonlocal close_attempts, target_heartbeat
                        nonlocal rollback_started, owner_unavailable
                        if (
                            boundary == "heartbeat"
                            and isinstance(value, _ManagedRecordedResource)
                            and value.node.name == ".hoimin-heartbeat.json"
                        ):
                            if target_heartbeat is None:
                                target_heartbeat = value
                            if value is target_heartbeat:
                                close_attempts += 1
                                rollback_started = True
                                install_replacement()
                                if persistent and close_attempts == 2:
                                    owner_unavailable = True
                                if persistent or close_attempts == 1:
                                    value.close_failures = 1
                        real_close_resource(value)

                    def open_file(
                        parent: DirectoryCapability,
                        name: str,
                        *,
                        access: FileAccess,
                        disposition: CreateDisposition,
                        share_policy: SharePolicy = SharePolicy.MUTATION,
                    ) -> FileCapability:
                        capability = real_open_file(
                            parent,
                            name,
                            access=access,
                            disposition=disposition,
                            share_policy=share_policy,
                        )
                        if (
                            boundary == "heartbeat"
                            and name == ".hoimin-heartbeat.json"
                            and access is FileAccess.WRITE
                            and disposition is CreateDisposition.OPEN_EXISTING
                        ):
                            retained_heartbeat_capabilities.append(capability)
                        return capability

                    def close_descriptor(descriptor: int) -> None:
                        nonlocal close_attempts, target_descriptor
                        nonlocal rollback_started, owner_unavailable
                        managed_lock = next(
                            (
                                lock
                                for lock in allocated_locks
                                if lock is not backend.coordinator
                                and lock.fd == descriptor
                            ),
                            None,
                        )
                        if (
                            boundary == "lease"
                            and managed_lock is not None
                            and (
                                descriptor == target_descriptor
                                or target_descriptor is None
                            )
                        ):
                            target_descriptor = descriptor
                            close_attempts += 1
                            rollback_started = True
                            install_replacement()
                            if persistent and close_attempts == 2:
                                owner_unavailable = True
                            if persistent or close_attempts == 1:
                                raise OSError("injected managed lease close failure")
                        real_close_descriptor(descriptor)

                    def observe_rollback(event: str) -> None:
                        if rollback_started and event.startswith(
                            ("close:", "delete:", "entry:", "open-entry:")
                        ):
                            coordinator = backend.coordinator
                            rollback_lock_states.append(
                                coordinator is not None and coordinator.locked
                            )
                        if owner_unavailable and event.startswith(
                            ("rename:", "delete:", "entry:", "open-entry:")
                        ):
                            namespace_after_owner_unavailable.append(event)

                    def close_coordinator(
                        lock: LeaseLock, label: str
                    ) -> tuple[str, ...]:
                        errors = real_close_coordinator(lock, label)
                        if lock is backend.coordinator and lock.fd < 0:
                            backend.events.append("close-coordinator-lock")
                        return errors

                    def close_retained_owners(
                        capabilities: list[FileCapability],
                        locks: list[LeaseLock],
                    ) -> None:
                        for capability in capabilities:
                            if capability.is_open:
                                capability.__del__()
                        for lock in locks:
                            if lock.fd >= 0:
                                lock.__del__()

                    backend.after_event = observe_rollback
                    self.addCleanup(
                        close_retained_owners,
                        retained_heartbeat_capabilities,
                        allocated_locks,
                    )
                    with (
                        mock.patch.object(
                            backend,
                            "close_resource",
                            side_effect=close_resource,
                        ),
                        mock.patch.object(
                            backend,
                            "open_file",
                            side_effect=open_file,
                        ),
                        mock.patch.object(
                            lease_module.os,
                            "close",
                            side_effect=close_descriptor,
                        ),
                        mock.patch.object(
                            lease_module,
                            "_close_locked_coordinator_once",
                            side_effect=close_coordinator,
                        ),
                        self.assertRaisesRegex(
                            OSError, "close failure"
                        ) as caught,
                    ):
                        self._create(
                            backend,
                            allocated_locks=allocated_locks,
                        )

                    self.assertEqual(close_attempts, 2)
                    self.assertTrue(rollback_lock_states)
                    self.assertTrue(all(rollback_lock_states))
                    self.assertEqual(
                        backend.events[-1], "close-coordinator-lock"
                    )
                    self.assertFalse(
                        any(
                            event.startswith(f"rename:{staging_name}->")
                            for event in backend.events
                        )
                    )
                    managed = self._managed_node(backend)
                    self.assertIs(managed.children[active_name], replacement)
                    if persistent:
                        self.assertEqual(
                            namespace_after_owner_unavailable, []
                        )
                        staging = managed.children[staging_name]
                        retained_marker = (
                            ".hoimin-heartbeat.json"
                            if boundary == "heartbeat"
                            else ".hoimin-lease.json"
                        )
                        self.assertEqual(
                            set(staging.children),
                            {
                                ".hoimin-lease.json",
                                ".hoimin-heartbeat.json",
                            },
                        )
                        self.assertTrue(
                            sum(
                                "cleanup unavailable" in note
                                for note in getattr(
                                    caught.exception, "__notes__", ()
                                )
                            )
                            == 1
                        )
                    else:
                        self.assertNotIn(staging_name, managed.children)
                    coordinator = backend.coordinator
                    self.assertIsNotNone(coordinator)
                    assert coordinator is not None
                    self.assertEqual(coordinator.fd, -1)
                    self.assertFalse(coordinator.locked)
                    if boundary == "heartbeat" and persistent:
                        self.assertIsNotNone(target_heartbeat)
                        assert target_heartbeat is not None
                        self.assertFalse(target_heartbeat.closed)
                        self.assertEqual(
                            backend.live_resources, {target_heartbeat}
                        )
                    else:
                        self.assertEqual(len(backend.live_resources), 0)
                    managed_lease_locks = [
                        lock
                        for lock in allocated_locks
                        if lock is not coordinator
                    ]
                    if boundary == "lease" and persistent:
                        self.assertEqual(
                            sum(lock.fd >= 0 for lock in managed_lease_locks),
                            1,
                        )
                    else:
                        self.assertTrue(
                            all(lock.fd < 0 for lock in managed_lease_locks)
                        )

        self.doCleanups()
        for boundary, persistent, capabilities, locks in retained_owner_sets:
            with self.subTest(
                cleanup_boundary=boundary,
                cleanup_persistent=persistent,
            ):
                self.assertTrue(
                    all(not capability.is_open for capability in capabilities)
                )
                self.assertTrue(all(lock.fd < 0 for lock in locks))

    def test_windows_handoff_allocates_lease_owner_before_opening_capability(
        self,
    ) -> None:
        backend = self._backend(rename_requires_closed_descendants=True)

        with self.assertRaisesRegex(MemoryError, "owner allocation failure"):
            self._create(backend, lease_allocation_failure_call=3)

        lease_reopens = [
            index
            for index, event in enumerate(backend.events)
            if event == "open_existing:.hoimin-lease.json:read_write:pinned"
        ]
        self.assertEqual(len(lease_reopens), 1)
        rename_back = backend.events.index(
            f"rename:run-{self.run_id}->.staging-{self.run_id}:False"
        )
        self.assertGreater(lease_reopens[0], rename_back)
        self.assertEqual(len(backend.live_resources), 0)

    def test_windows_restore_allocates_lease_owner_before_reopen(self) -> None:
        backend = self._backend(rename_requires_closed_descendants=True)
        heartbeat_reopens = 0

        def fail_published_heartbeat_reopen(event: str) -> None:
            nonlocal heartbeat_reopens
            if event != "open_existing:.hoimin-heartbeat.json:write:pinned":
                return
            heartbeat_reopens += 1
            if heartbeat_reopens == 2:
                raise OSError("injected heartbeat reopen primary")

        backend.after_event = fail_published_heartbeat_reopen
        with self.assertRaisesRegex(
            OSError, "heartbeat reopen primary"
        ) as caught:
            self._create(backend, lease_allocation_failure_call=4)

        self.assertEqual(
            backend.events.count(
                "open_existing:.hoimin-lease.json:read_write:pinned"
            ),
            1,
        )
        self.assertTrue(
            any(
                "MemoryError" in note
                for note in getattr(caught.exception, "__notes__", ())
            )
        )

    def test_windows_restored_owner_close_failure_blocks_namespace_cleanup(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_locked_coordinator_once"],
        )
        active_name = f"run-{self.run_id}"
        staging_name = f".staging-{self.run_id}"
        retained_owner_sets: list[
            tuple[str, list[FileCapability], list[LeaseLock]]
        ] = []
        for boundary in ("lease", "heartbeat"):
            with self.subTest(boundary=boundary):
                backend = self._backend(rename_requires_closed_descendants=True)
                allocated_locks: list[LeaseLock] = []
                retained_capabilities: list[FileCapability] = []
                retained_owner_sets.append(
                    (boundary, retained_capabilities, allocated_locks)
                )
                event_counts: dict[str, int] = {}
                close_attempts = 0
                restore_started = False
                rollback_started = False
                owner_unavailable = False
                replacement: _ManagedRecordedNode | None = None
                target_heartbeat: _ManagedRecordedResource | None = None
                target_descriptor: int | None = None
                rollback_lock_states: list[bool] = []
                namespace_after_owner_failure: list[str] = []
                real_open_file = backend.open_file
                real_close_resource = backend.close_resource
                real_close_descriptor = lease_module.os.close
                real_close_coordinator = (
                    lease_module._close_locked_coordinator_once
                )

                def inject_primary_and_observe(event: str) -> None:
                    nonlocal restore_started, rollback_started, replacement
                    event_counts[event] = event_counts.get(event, 0) + 1
                    if event == (
                        "open_existing:.hoimin-heartbeat.json:write:pinned"
                    ) and event_counts[event] == 2:
                        rollback_started = True
                        raise OSError("injected published heartbeat reopen failure")
                    if event == (
                        "open_existing:.hoimin-lease.json:read:pinned"
                    ) and event_counts[event] == 2:
                        restore_started = True
                        managed = self._managed_node(backend)
                        replacement = backend._new_node(
                            EntryKind.DIRECTORY,
                            SecurityDomain.MANAGED,
                            parent=managed,
                            name=active_name,
                        )
                    if rollback_started and event.startswith(
                        ("close:", "delete:", "entry:", "open-entry:")
                    ):
                        coordinator = backend.coordinator
                        rollback_lock_states.append(
                            coordinator is not None and coordinator.locked
                        )
                    if owner_unavailable and event.startswith(
                        ("rename:", "delete:", "entry:", "open-entry:")
                    ):
                        namespace_after_owner_failure.append(event)

                def open_file(
                    parent: DirectoryCapability,
                    name: str,
                    *,
                    access: FileAccess,
                    disposition: CreateDisposition,
                    share_policy: SharePolicy = SharePolicy.MUTATION,
                ) -> FileCapability:
                    nonlocal target_heartbeat
                    capability = real_open_file(
                        parent,
                        name,
                        access=access,
                        disposition=disposition,
                        share_policy=share_policy,
                    )
                    if (
                        restore_started
                        and name == ".hoimin-heartbeat.json"
                        and access is FileAccess.WRITE
                        and disposition is CreateDisposition.OPEN_EXISTING
                    ):
                        retained_capabilities.append(capability)
                        target_heartbeat = backend._resource(capability)
                    return capability

                def close_resource(value: object) -> None:
                    nonlocal close_attempts, owner_unavailable
                    if boundary == "heartbeat" and value is target_heartbeat:
                        close_attempts += 1
                        if close_attempts == 2:
                            owner_unavailable = True
                        assert isinstance(value, _ManagedRecordedResource)
                        value.close_failures = 1
                    real_close_resource(value)

                def close_descriptor(descriptor: int) -> None:
                    nonlocal close_attempts
                    nonlocal target_descriptor, owner_unavailable
                    managed_lock = next(
                        (
                            lock
                            for lock in allocated_locks
                            if restore_started
                            and lock is not backend.coordinator
                            and lock.fd == descriptor
                        ),
                        None,
                    )
                    if boundary == "lease" and managed_lock is not None:
                        if target_descriptor is None:
                            target_descriptor = descriptor
                        if descriptor == target_descriptor:
                            close_attempts += 1
                            if close_attempts == 2:
                                owner_unavailable = True
                            raise OSError(
                                "injected restored lease close failure"
                            )
                    real_close_descriptor(descriptor)

                def close_coordinator(
                    lock: LeaseLock, label: str
                ) -> tuple[str, ...]:
                    errors = real_close_coordinator(lock, label)
                    if lock is backend.coordinator and lock.fd < 0:
                        backend.events.append("close-coordinator-lock")
                    return errors

                def close_retained_owners(
                    capabilities: list[FileCapability],
                    locks: list[LeaseLock],
                ) -> None:
                    for capability in capabilities:
                        if capability.is_open:
                            capability.__del__()
                    for lock in locks:
                        if lock.fd >= 0:
                            lock.__del__()

                backend.after_event = inject_primary_and_observe
                self.addCleanup(
                    close_retained_owners,
                    retained_capabilities,
                    allocated_locks,
                )
                with (
                    mock.patch.object(
                        backend, "open_file", side_effect=open_file
                    ),
                    mock.patch.object(
                        backend, "close_resource", side_effect=close_resource
                    ),
                    mock.patch.object(
                        lease_module.os,
                        "close",
                        side_effect=close_descriptor,
                    ),
                    mock.patch.object(
                        lease_module,
                        "_close_locked_coordinator_once",
                        side_effect=close_coordinator,
                    ),
                    self.assertRaisesRegex(
                        OSError, "published heartbeat reopen failure"
                    ) as caught,
                ):
                    self._create(
                        backend,
                        allocated_locks=allocated_locks,
                    )

                self.assertEqual(close_attempts, 2)
                self.assertEqual(namespace_after_owner_failure, [])
                managed = self._managed_node(backend)
                self.assertIs(managed.children[active_name], replacement)
                staging = managed.children[staging_name]
                self.assertEqual(
                    set(staging.children),
                    {".hoimin-lease.json", ".hoimin-heartbeat.json"},
                )
                self.assertTrue(rollback_lock_states)
                self.assertTrue(all(rollback_lock_states))
                self.assertEqual(
                    backend.events[-1], "close-coordinator-lock"
                )
                self.assertTrue(
                    sum(
                        "cleanup unavailable" in note
                        for note in getattr(caught.exception, "__notes__", ())
                    )
                    == 1
                )
                coordinator = backend.coordinator
                self.assertIsNotNone(coordinator)
                assert coordinator is not None
                if boundary == "lease":
                    self.assertEqual(
                        sum(
                            lock.fd >= 0
                            for lock in allocated_locks
                            if lock is not coordinator
                        ),
                        1,
                    )
                else:
                    self.assertIsNotNone(target_heartbeat)
                    assert target_heartbeat is not None
                    self.assertFalse(target_heartbeat.closed)

        self.doCleanups()
        for boundary, capabilities, locks in retained_owner_sets:
            with self.subTest(cleanup_boundary=boundary):
                self.assertTrue(
                    all(not capability.is_open for capability in capabilities)
                )
                self.assertTrue(all(lock.fd < 0 for lock in locks))

    def test_hidden_helper_owner_blocks_marker_and_staging_rollback(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_OwnedDescriptor", "_close_locked_coordinator_once"],
        )
        active_name = f"run-{self.run_id}"
        staging_name = f".staging-{self.run_id}"
        lease_name = ".hoimin-lease.json"
        retained_owner_sets: list[
            tuple[
                str,
                list[FileCapability],
                list[_OwnedDescriptor],
                list[LeaseLock],
            ]
        ] = []
        for stage in ("create-capability", "read-descriptor", "lease-capability"):
            with self.subTest(stage=stage):
                backend = self._backend(rename_requires_closed_descendants=True)
                allocated_locks: list[LeaseLock] = []
                descriptor_owners: list[_OwnedDescriptor] = []
                retained_capabilities: list[FileCapability] = []
                retained_owner_sets.append(
                    (
                        stage,
                        retained_capabilities,
                        descriptor_owners,
                        allocated_locks,
                    )
                )
                target_resource: _ManagedRecordedResource | None = None
                target_descriptor: int | None = None
                close_attempts = 0
                rollback_started = False
                owner_unavailable = False
                replacement: _ManagedRecordedNode | None = None
                rollback_lock_states: list[bool] = []
                namespace_after_owner_failure: list[str] = []
                real_owned_descriptor = _OwnedDescriptor
                real_open_file = backend.open_file
                real_close_resource = backend.close_resource
                real_detach_resource = backend.detach_file_resource
                real_read = lease_module.os.read
                real_close_descriptor = lease_module.os.close
                real_close_coordinator = (
                    lease_module._close_locked_coordinator_once
                )

                def allocate_descriptor_owner() -> _OwnedDescriptor:
                    owner = real_owned_descriptor()
                    descriptor_owners.append(owner)
                    return owner

                def maybe_install_replacement() -> None:
                    nonlocal replacement
                    if stage != "create-capability":
                        return
                    managed = backend.parent.children.get("hoimin-focused-v1")
                    if managed is None:
                        return
                    if replacement is None and active_name not in managed.children:
                        replacement = backend._new_node(
                            EntryKind.DIRECTORY,
                            SecurityDomain.MANAGED,
                            parent=managed,
                            name=active_name,
                        )

                def observe_and_inject(event: str) -> None:
                    nonlocal rollback_started
                    if (
                        stage == "create-capability"
                        and not rollback_started
                        and event == f"verify-managed:{lease_name}:repair=false"
                    ):
                        rollback_started = True
                        maybe_install_replacement()
                        raise OSError("injected marker creation verification")
                    if rollback_started:
                        maybe_install_replacement()
                    if rollback_started and event.startswith(
                        ("close:", "delete:", "entry:", "open-entry:")
                    ):
                        coordinator = backend.coordinator
                        rollback_lock_states.append(
                            coordinator is not None and coordinator.locked
                        )
                    if owner_unavailable and event.startswith(
                        ("rename:", "delete:", "entry:", "open-entry:")
                    ):
                        namespace_after_owner_failure.append(event)

                def open_file(
                    parent: DirectoryCapability,
                    name: str,
                    *,
                    access: FileAccess,
                    disposition: CreateDisposition,
                    share_policy: SharePolicy = SharePolicy.MUTATION,
                ) -> FileCapability:
                    nonlocal target_resource
                    capability = real_open_file(
                        parent,
                        name,
                        access=access,
                        disposition=disposition,
                        share_policy=share_policy,
                    )
                    is_target = (
                        stage == "create-capability"
                        and name == lease_name
                        and disposition is CreateDisposition.CREATE_NEW
                    ) or (
                        stage == "lease-capability"
                        and name == lease_name
                        and access is FileAccess.READ_WRITE
                        and disposition is CreateDisposition.OPEN_EXISTING
                    )
                    if is_target:
                        retained_capabilities.append(capability)
                        target_resource = backend._resource(capability)
                    return capability

                def close_resource(value: object) -> None:
                    nonlocal close_attempts, owner_unavailable
                    if value is target_resource:
                        close_attempts += 1
                        if close_attempts == 2:
                            owner_unavailable = True
                        assert isinstance(value, _ManagedRecordedResource)
                        value.close_failures = 1
                    real_close_resource(value)

                def detach_resource(value: object, flags: int) -> int:
                    nonlocal rollback_started
                    if stage == "lease-capability" and value is target_resource:
                        rollback_started = True
                        raise OSError("injected published lease detach failure")
                    return real_detach_resource(value, flags)

                def read_descriptor(descriptor: int, size: int) -> bytes:
                    nonlocal rollback_started, target_descriptor
                    node = backend.detached.get(descriptor)
                    if (
                        stage == "read-descriptor"
                        and target_descriptor is None
                        and node is not None
                        and node.name == lease_name
                    ):
                        target_descriptor = descriptor
                        rollback_started = True
                        raise OSError("injected marker read failure")
                    return real_read(descriptor, size)

                def close_descriptor(descriptor: int) -> None:
                    nonlocal close_attempts, owner_unavailable
                    if descriptor == target_descriptor:
                        close_attempts += 1
                        if close_attempts == 2:
                            owner_unavailable = True
                        raise OSError("injected marker descriptor close failure")
                    real_close_descriptor(descriptor)

                def close_coordinator(
                    lock: LeaseLock, label: str
                ) -> tuple[str, ...]:
                    errors = real_close_coordinator(lock, label)
                    if lock is backend.coordinator and lock.fd < 0:
                        backend.events.append("close-coordinator-lock")
                    return errors

                def close_retained_owners(
                    capabilities: list[FileCapability],
                    owners: list[_OwnedDescriptor],
                    locks: list[LeaseLock],
                ) -> None:
                    for capability in capabilities:
                        if capability.is_open:
                            capability.__del__()
                    for owner in owners:
                        if owner.fd >= 0:
                            owner.__del__()
                    for lock in locks:
                        if lock.fd >= 0:
                            lock.__del__()

                backend.after_event = observe_and_inject
                self.addCleanup(
                    close_retained_owners,
                    retained_capabilities,
                    descriptor_owners,
                    allocated_locks,
                )
                with (
                    mock.patch.object(
                        lease_module,
                        "_OwnedDescriptor",
                        side_effect=allocate_descriptor_owner,
                    ),
                    mock.patch.object(
                        backend, "open_file", side_effect=open_file
                    ),
                    mock.patch.object(
                        backend, "close_resource", side_effect=close_resource
                    ),
                    mock.patch.object(
                        backend,
                        "detach_file_resource",
                        side_effect=detach_resource,
                    ),
                    mock.patch.object(
                        lease_module.os, "read", side_effect=read_descriptor
                    ),
                    mock.patch.object(
                        lease_module.os,
                        "close",
                        side_effect=close_descriptor,
                    ),
                    mock.patch.object(
                        lease_module,
                        "_close_locked_coordinator_once",
                        side_effect=close_coordinator,
                    ),
                    self.assertRaises(OSError) as caught,
                ):
                    self._create(
                        backend,
                        allocated_locks=allocated_locks,
                    )

                self.assertEqual(close_attempts, 2)
                self.assertEqual(namespace_after_owner_failure, [])
                managed = self._managed_node(backend)
                if stage == "create-capability":
                    self.assertIs(managed.children[active_name], replacement)
                    retained_root = managed.children[staging_name]
                    self.assertEqual(set(retained_root.children), {lease_name})
                else:
                    self.assertIsNone(replacement)
                    self.assertNotIn(staging_name, managed.children)
                    retained_root = managed.children[active_name]
                    self.assertEqual(
                        set(retained_root.children),
                        {lease_name, ".hoimin-heartbeat.json"},
                    )
                self.assertTrue(rollback_lock_states)
                self.assertTrue(all(rollback_lock_states))
                self.assertEqual(
                    backend.events[-1], "close-coordinator-lock"
                )
                self.assertTrue(
                    sum(
                        "cleanup unavailable" in note
                        for note in getattr(caught.exception, "__notes__", ())
                    )
                    == 1
                )
                if stage == "read-descriptor":
                    retained_descriptors = [
                        owner
                        for owner in descriptor_owners
                        if owner.fd >= 0
                    ]
                    self.assertEqual(len(retained_descriptors), 1)
                else:
                    self.assertIsNotNone(target_resource)
                    assert target_resource is not None
                    self.assertFalse(target_resource.closed)

        self.doCleanups()
        for stage, capabilities, descriptors, locks in retained_owner_sets:
            with self.subTest(cleanup_stage=stage):
                self.assertTrue(
                    all(not capability.is_open for capability in capabilities)
                )
                self.assertTrue(all(owner.fd < 0 for owner in descriptors))
                self.assertTrue(all(lock.fd < 0 for lock in locks))

    def test_staging_rollback_checks_absence_only_after_owner_close(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_locked_coordinator_once"],
        )
        staging_name = f".staging-{self.run_id}"
        for close_failures in (1, 2):
            with self.subTest(close_failures=close_failures):
                backend = self._backend(
                    rename_requires_closed_descendants=False
                )
                staging_resource: _ManagedRecordedResource | None = None
                retained_staging: list[DirectoryCapability] = []
                real_create_directory = backend.create_directory

                def create_directory(
                    parent: DirectoryCapability,
                    name: str,
                    share_policy: SharePolicy,
                ) -> DirectoryCapability:
                    capability = real_create_directory(
                        parent, name, share_policy
                    )
                    if name == staging_name:
                        retained_staging.append(capability)
                    return capability

                def close_retained_staging() -> None:
                    for capability in retained_staging:
                        if capability.is_open:
                            capability.close()

                self.addCleanup(close_retained_staging)

                def fail_staging_verification(event: str) -> None:
                    nonlocal staging_resource
                    if event == f"delete:{staging_name}":
                        staging_resource = next(
                            resource
                            for resource in backend.live_resources
                            if resource.node.name == staging_name
                        )
                        staging_resource.close_failures = close_failures
                    if event == (
                        f"verify-managed:{staging_name}:repair=false"
                    ):
                        raise OSError("injected staging verification primary")

                backend.after_event = fail_staging_verification
                real_close_all = lease_module._close_locked_coordinator_once

                def close_and_observe(
                    lock: LeaseLock, label: str
                ) -> tuple[str, ...]:
                    errors = real_close_all(lock, label)
                    if lock is backend.coordinator and lock.fd < 0:
                        backend.events.append("close-coordinator-lock")
                    return errors

                with (
                    mock.patch.object(
                        lease_module,
                        "_close_locked_coordinator_once",
                        side_effect=close_and_observe,
                    ),
                    mock.patch.object(
                        backend,
                        "create_directory",
                        side_effect=create_directory,
                    ),
                    self.assertRaisesRegex(
                        OSError, "staging verification primary"
                    ),
                ):
                    self._create(backend)

                delete_index = backend.events.index(f"delete:{staging_name}")
                rollback_events = backend.events[delete_index:]
                expected_absence_checks = 1 if close_failures == 1 else 0
                self.assertEqual(
                    rollback_events.count(f"entry:{staging_name}"),
                    expected_absence_checks,
                )
                self.assertEqual(
                    rollback_events.count(f"close:{staging_name}"), 2
                )
                if close_failures == 1:
                    self.assertLess(
                        rollback_events.index("close:hoimin-focused-v1"),
                        rollback_events.index("close-coordinator-lock"),
                    )
                    self.assertEqual(
                        rollback_events[-1], "close-coordinator-lock"
                    )
                    self.assertIsNotNone(staging_resource)
                    assert staging_resource is not None
                    self.assertTrue(staging_resource.closed)
                else:
                    self.assertIsNotNone(staging_resource)
                    assert staging_resource is not None
                    self.assertFalse(staging_resource.closed)

    def test_directory_rollback_never_deletes_a_non_created_capability(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_delete_owned_directory"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)
        parent = backend.open_root(backend.parent_path, SharePolicy.MUTATION)
        initial = backend.create_directory(parent, "existing", SharePolicy.PINNED)
        initial.close()
        existing = backend.open_directory(parent, "existing", SharePolicy.PINNED)
        backend.events.clear()

        errors = lease_module._delete_owned_directory(
            parent,
            "existing",
            existing,
            backend,
            label="managed staging rollback",
        )

        self.assertEqual(
            sum("rollback unavailable" in error for error in errors), 1
        )
        self.assertTrue(existing.closed)
        self.assertIn("existing", backend.parent.children)
        self.assertFalse(
            any(
                event.startswith(("delete:", "entry:"))
                for event in backend.events
            )
        )
        parent.close()

    def test_directory_rollback_uses_one_lifetime_close_budget(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=[
                "_close_capability_retry",
                "_delete_owned_directory",
                "_rollback_created_managed_root",
            ],
        )

        with self.subTest(case="secure-root-persistent-consuming-close"):
            backend = self._backend(
                rename_requires_closed_descendants=False
            )
            parent = backend.open_root(
                backend.parent_path, SharePolicy.MUTATION
            )
            root = backend.create_secure_root(parent, "hoimin-focused-v1")
            target = backend._resource(root)
            real_close_resource = backend.close_resource
            close_attempts = 0
            inject_close_failure = True

            def close_secure_root(value: object) -> None:
                nonlocal close_attempts
                if value is target:
                    close_attempts += 1
                    if inject_close_failure:
                        target.close_failures = 1
                real_close_resource(value)

            start = len(backend.events)
            with mock.patch.object(
                backend,
                "close_resource",
                side_effect=close_secure_root,
            ):
                errors = lease_module._rollback_created_managed_root(
                    root, parent, backend
                )
            rollback_events = backend.events[start:]
            observed_open = root.is_open
            observed_attempts = close_attempts
            inject_close_failure = False
            finalizer_start = len(backend.events)
            root.__del__()
            finalizer_events = backend.events[finalizer_start:]
            parent.close()

            self.assertEqual(observed_attempts, 2)
            self.assertEqual(
                rollback_events.count("delete:hoimin-focused-v1"), 1
            )
            self.assertTrue(observed_open)
            self.assertNotIn("entry:hoimin-focused-v1", rollback_events)
            self.assertEqual(
                finalizer_events, ["close:hoimin-focused-v1"]
            )
            self.assertTrue(root.closed)
            self.assertTrue(
                all(len(error.encode("utf-8")) <= 1_024 for error in errors)
            )

        for persistent in (False, True):
            with self.subTest(
                case="delete-fails-before-close", persistent=persistent
            ):
                backend = self._backend(
                    rename_requires_closed_descendants=False
                )
                parent = backend.open_root(
                    backend.parent_path, SharePolicy.MUTATION
                )
                name = "rollback-before-close"
                directory = backend.create_directory(
                    parent, name, SharePolicy.PINNED
                )
                target = backend._resource(directory)
                real_close_resource = backend.close_resource
                close_attempts = 0
                target_close_indices: list[int] = []
                inject_close_failure = True

                def close_after_delete_failure(value: object) -> None:
                    nonlocal close_attempts
                    is_target = value is target
                    if is_target:
                        close_attempts += 1
                        if inject_close_failure and (
                            persistent or close_attempts == 1
                        ):
                            target.close_failures = 1
                    try:
                        real_close_resource(value)
                    finally:
                        if is_target:
                            target_close_indices.append(
                                len(backend.events) - 1
                            )

                backend.failures[f"delete:{name}"] = OSError(
                    "injected delete failure before owner close"
                )
                start = len(backend.events)
                with mock.patch.object(
                    backend,
                    "close_resource",
                    side_effect=close_after_delete_failure,
                ):
                    errors = lease_module._delete_owned_directory(
                        parent,
                        name,
                        directory,
                        backend,
                        label="managed directory rollback",
                    )
                rollback_events = backend.events[start:]
                observed_open = directory.is_open
                observed_attempts = close_attempts
                inject_close_failure = False
                finalizer_start = len(backend.events)
                directory.__del__()
                finalizer_events = backend.events[finalizer_start:]
                parent.close()

                self.assertEqual(observed_attempts, 2)
                self.assertEqual(rollback_events.count(f"delete:{name}"), 1)
                self.assertEqual(len(target_close_indices), 2)
                absence_events = [
                    event
                    for event in backend.events[
                        target_close_indices[1] + 1 : finalizer_start
                    ]
                    if event == f"entry:{name}"
                ]
                self.assertEqual(absence_events, [] if persistent else [f"entry:{name}"])
                self.assertEqual(observed_open, persistent)
                self.assertIn(name, backend.parent.children)
                self.assertTrue(
                    all(
                        len(error.encode("utf-8")) <= 1_024
                        for error in errors
                    )
                )
                if persistent:
                    self.assertEqual(finalizer_events, [f"close:{name}"])
                else:
                    self.assertEqual(finalizer_events, [])

        with self.subTest(case="directory-move-transfers-close-attempts"):
            backend = self._backend(
                rename_requires_closed_descendants=False
            )
            parent = backend.open_root(
                backend.parent_path, SharePolicy.MUTATION
            )
            name = "moved-owner"
            original = backend.create_directory(
                parent, name, SharePolicy.PINNED
            )
            target = backend._resource(original)
            real_close_resource = backend.close_resource
            close_attempts = 0
            inject_close_failure = True

            def close_moved_owner(value: object) -> None:
                nonlocal close_attempts
                if value is target:
                    close_attempts += 1
                    if inject_close_failure:
                        target.close_failures = 1
                real_close_resource(value)

            with mock.patch.object(
                backend,
                "close_resource",
                side_effect=close_moved_owner,
            ):
                with self.assertRaisesRegex(OSError, "injected close failure"):
                    original.close()
                moved = original._move_for(backend)
                errors = lease_module._close_capability_retry(
                    moved, "moved managed directory"
                )
            observed_attempts = close_attempts
            observed_open = moved.is_open
            inject_close_failure = False
            finalizer_start = len(backend.events)
            moved.__del__()
            finalizer_events = backend.events[finalizer_start:]
            parent.close()

            self.assertTrue(original.transferred)
            self.assertEqual(observed_attempts, 2)
            self.assertTrue(observed_open)
            self.assertEqual(finalizer_events, [f"close:{name}"])
            self.assertTrue(
                all(len(error.encode("utf-8")) <= 1_024 for error in errors)
            )

        with self.subTest(case="closed-close-is-idempotent"):
            backend = self._backend(
                rename_requires_closed_descendants=False
            )
            parent = backend.open_root(
                backend.parent_path, SharePolicy.MUTATION
            )
            directory = backend.create_directory(
                parent, "idempotent-owner", SharePolicy.PINNED
            )
            start = len(backend.events)
            directory.close()
            directory.close()
            self.assertEqual(
                backend.events[start:].count("close:idempotent-owner"), 1
            )
            parent.close()

    def test_staging_and_child_rollback_use_actual_close_progress(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_locked_coordinator_once"],
        )
        staging_name = f".staging-{self.run_id}"

        with self.subTest(case="staging-before-mainline-close"):
            backend = self._backend(
                rename_requires_closed_descendants=False
            )
            real_create_directory = backend.create_directory
            real_close_resource = backend.close_resource
            real_close_coordinator = (
                lease_module._close_locked_coordinator_once
            )
            staging_retained: list[DirectoryCapability] = []
            staging_target: _ManagedRecordedResource | None = None
            close_attempts = 0
            staging_close_indices: list[int] = []
            inject_close_failure = True

            def create_directory(
                parent: DirectoryCapability,
                name: str,
                share_policy: SharePolicy,
            ) -> DirectoryCapability:
                nonlocal staging_target
                capability = real_create_directory(parent, name, share_policy)
                if name == staging_name:
                    staging_retained.append(capability)
                    staging_target = backend._resource(capability)
                return capability

            def close_staging(value: object) -> None:
                nonlocal close_attempts
                is_target = value is staging_target
                if is_target:
                    close_attempts += 1
                    if inject_close_failure:
                        assert staging_target is not None
                        staging_target.close_failures = 1
                try:
                    real_close_resource(value)
                finally:
                    if is_target:
                        staging_close_indices.append(len(backend.events) - 1)

            def inject_primary(event: str) -> None:
                if event == f"verify-managed:{staging_name}:repair=false":
                    raise OSError("injected staging pre-close primary")

            def close_coordinator(
                lock: LeaseLock, label: str
            ) -> tuple[str, ...]:
                errors = real_close_coordinator(lock, label)
                if lock is backend.coordinator and lock.fd < 0:
                    backend.events.append("close-coordinator-lock")
                return errors

            backend.failures[f"delete:{staging_name}"] = OSError(
                "injected staging delete before close"
            )
            backend.after_event = inject_primary
            with (
                mock.patch.object(
                    backend,
                    "create_directory",
                    side_effect=create_directory,
                ),
                mock.patch.object(
                    backend,
                    "close_resource",
                    side_effect=close_staging,
                ),
                mock.patch.object(
                    lease_module,
                    "_close_locked_coordinator_once",
                    side_effect=close_coordinator,
                ),
                self.assertRaisesRegex(
                    OSError, "staging pre-close primary"
                ) as caught,
            ):
                self._create(backend)

            self.assertEqual(len(staging_retained), 1)
            capability = staging_retained[0]
            observed_open = capability.is_open
            observed_attempts = close_attempts
            self.assertEqual(len(staging_close_indices), 2)
            after_second_close = backend.events[
                staging_close_indices[1] + 1 :
            ]
            inject_close_failure = False
            finalizer_start = len(backend.events)
            capability.__del__()
            finalizer_events = backend.events[finalizer_start:]

            self.assertEqual(observed_attempts, 2)
            self.assertTrue(observed_open)
            self.assertEqual(
                [
                    event
                    for event in after_second_close
                    if event.startswith(
                        ("rename:", "delete:", "entry:", "open-entry:")
                    )
                ],
                [],
            )
            self.assertEqual(
                after_second_close,
                ["close:hoimin-focused-v1", "close-coordinator-lock"],
            )
            self.assertEqual(finalizer_events, [f"close:{staging_name}"])
            self.assertEqual(
                str(caught.exception), "injected staging pre-close primary"
            )
            self.assertTrue(
                all(
                    len(note.encode("utf-8")) <= 1_024
                    for note in getattr(caught.exception, "__notes__", ())
                )
            )

        child_name = "candidate-review-fix"
        with self.subTest(case="child-before-mainline-close"):
            backend = self._backend(
                rename_requires_closed_descendants=False
            )
            scratch = self._create(backend)
            real_create_directory = backend.create_directory
            real_close_resource = backend.close_resource
            pre_child_retained: list[DirectoryCapability] = []
            pre_child_target: _ManagedRecordedResource | None = None
            close_attempts = 0
            pre_child_close_indices: list[int] = []
            primary_raised = False
            inject_close_failure = True

            def create_child(
                parent: DirectoryCapability,
                name: str,
                share_policy: SharePolicy,
            ) -> DirectoryCapability:
                nonlocal pre_child_target
                capability = real_create_directory(parent, name, share_policy)
                if name == child_name:
                    pre_child_retained.append(capability)
                    pre_child_target = backend._resource(capability)
                return capability

            def close_child(value: object) -> None:
                nonlocal close_attempts
                is_target = value is pre_child_target
                if is_target:
                    close_attempts += 1
                    if inject_close_failure and close_attempts == 1:
                        assert pre_child_target is not None
                        pre_child_target.close_failures = 1
                try:
                    real_close_resource(value)
                finally:
                    if is_target:
                        pre_child_close_indices.append(
                            len(backend.events) - 1
                        )

            def inject_child_primary(event: str) -> None:
                nonlocal primary_raised
                if event == f"entry:{child_name}" and not primary_raised:
                    primary_raised = True
                    raise OSError("injected child pre-close primary")

            backend.failures[f"delete:{child_name}"] = OSError(
                "injected child delete before close"
            )
            backend.after_event = inject_child_primary
            with (
                mock.patch.object(
                    backend,
                    "create_directory",
                    side_effect=create_child,
                ),
                mock.patch.object(
                    backend, "close_resource", side_effect=close_child
                ),
                self.assertRaisesRegex(
                    OSError, "child pre-close primary"
                ) as caught,
            ):
                scratch.create_child(child_name)

            self.assertEqual(len(pre_child_retained), 1)
            capability = pre_child_retained[0]
            observed_open = capability.is_open
            observed_attempts = close_attempts
            inject_close_failure = False
            finalizer_start = len(backend.events)
            capability.__del__()
            finalizer_events = backend.events[finalizer_start:]

            self.assertEqual(observed_attempts, 2)
            self.assertFalse(observed_open)
            self.assertEqual(len(pre_child_close_indices), 2)
            self.assertIn(
                f"entry:{child_name}",
                backend.events[
                    pre_child_close_indices[1] + 1 : finalizer_start
                ],
            )
            self.assertEqual(finalizer_events, [])
            self.assertEqual(
                str(caught.exception), "injected child pre-close primary"
            )
            pre_child_root = scratch._root
            assert pre_child_root is not None
            self.assertIn(
                child_name, backend._resource(pre_child_root).node.children
            )

        for persistent in (False, True):
            with self.subTest(
                case="child-mainline-close", persistent=persistent
            ):
                backend = self._backend(
                    rename_requires_closed_descendants=False
                )
                scratch = self._create(backend)
                real_create_directory = backend.create_directory
                real_close_resource = backend.close_resource
                mainline_retained: list[DirectoryCapability] = []
                mainline_target: _ManagedRecordedResource | None = None
                close_attempts = 0
                inject_close_failure = True

                def create_child(
                    parent: DirectoryCapability,
                    name: str,
                    share_policy: SharePolicy,
                ) -> DirectoryCapability:
                    nonlocal mainline_target
                    capability = real_create_directory(
                        parent, name, share_policy
                    )
                    if name == child_name:
                        mainline_retained.append(capability)
                        mainline_target = backend._resource(capability)
                    return capability

                def close_child(value: object) -> None:
                    nonlocal close_attempts
                    if value is mainline_target:
                        close_attempts += 1
                        if inject_close_failure and (
                            persistent or close_attempts == 1
                        ):
                            assert mainline_target is not None
                            mainline_target.close_failures = 1
                    real_close_resource(value)

                with (
                    mock.patch.object(
                        backend,
                        "create_directory",
                        side_effect=create_child,
                    ),
                    mock.patch.object(
                        backend, "close_resource", side_effect=close_child
                    ),
                    self.assertRaisesRegex(
                        OSError, "injected close failure"
                    ),
                ):
                    scratch.create_child(child_name)

                self.assertEqual(len(mainline_retained), 1)
                capability = mainline_retained[0]
                observed_open = capability.is_open
                observed_attempts = close_attempts
                inject_close_failure = False
                finalizer_start = len(backend.events)
                capability.__del__()
                finalizer_events = backend.events[finalizer_start:]

                self.assertEqual(observed_attempts, 2)
                self.assertEqual(observed_open, persistent)
                mainline_root = scratch._root
                assert mainline_root is not None
                self.assertNotIn(
                    child_name,
                    backend._resource(mainline_root).node.children,
                )
                if persistent:
                    self.assertEqual(
                        finalizer_events, [f"close:{child_name}"]
                    )
                else:
                    self.assertEqual(finalizer_events, [])

    def test_post_coordinator_allocation_failure_closes_every_owner_in_order(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_locked_coordinator_once"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)
        real_close = lease_module._close_locked_coordinator_once
        root_close_lock_states: list[bool] = []

        def observe_root_cleanup(event: str) -> None:
            if event == "close:hoimin-focused-v1":
                coordinator = backend.coordinator
                if coordinator is not None and coordinator.locked:
                    root_close_lock_states.append(True)

        def close_and_observe(lock: LeaseLock, label: str) -> tuple[str, ...]:
            errors = real_close(lock, label)
            if lock is backend.coordinator and lock.fd < 0:
                backend.events.append("close-coordinator-lock")
            return errors

        def close_leaked_coordinator() -> None:
            coordinator = backend.coordinator
            if coordinator is not None and coordinator.fd >= 0:
                coordinator.close()

        backend.after_event = observe_root_cleanup
        self.addCleanup(close_leaked_coordinator)
        with (
            mock.patch.object(
                lease_module.uuid,
                "uuid4",
                side_effect=MemoryError("injected post-coordinator allocation"),
            ),
            mock.patch.object(
                lease_module,
                "_close_locked_coordinator_once",
                side_effect=close_and_observe,
            ),
            self.assertRaisesRegex(
                MemoryError, "post-coordinator allocation"
            ),
        ):
            self._create(backend)

        self.assertEqual(root_close_lock_states, [True])
        root_close = backend.events.index("close:hoimin-focused-v1")
        coordinator_close = backend.events.index("close-coordinator-lock")
        self.assertLess(root_close, coordinator_close)
        self.assertEqual(backend.events[-1], "close-coordinator-lock")
        coordinator = backend.coordinator
        self.assertIsNotNone(coordinator)
        assert coordinator is not None
        self.assertEqual(coordinator.fd, -1)
        self.assertFalse(coordinator.locked)
        self.assertEqual(len(backend.live_resources), 0)

    def test_marker_owner_registry_precedes_transaction_helper_acquisition(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_MarkerRollbackOwners"],
        )
        backend = self._backend(rename_requires_closed_descendants=False)
        real_close = lease_module._close_locked_coordinator_once

        def close_and_observe(lock: LeaseLock, label: str) -> tuple[str, ...]:
            errors = real_close(lock, label)
            if lock is backend.coordinator and lock.fd < 0:
                backend.events.append("close-coordinator-lock")
            return errors

        with (
            mock.patch.object(
                lease_module,
                "_MarkerRollbackOwners",
                side_effect=MemoryError("injected owner registry allocation"),
            ),
            mock.patch.object(
                lease_module,
                "_close_locked_coordinator_once",
                side_effect=close_and_observe,
            ),
            self.assertRaisesRegex(
                MemoryError, "owner registry allocation"
            ),
        ):
            self._create(backend)

        self.assertFalse(
            any(
                event.startswith(
                    (
                        f"entry:run-{self.run_id}",
                        f"entry:.staging-{self.run_id}",
                        f"create-directory:.staging-{self.run_id}",
                        "create_new:.hoimin-",
                    )
                )
                for event in backend.events
            )
        )
        self.assertEqual(
            backend.events[-2:],
            ["close:hoimin-focused-v1", "close-coordinator-lock"],
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_coordinator_close_retries_only_after_locked_rollback(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_locked_coordinator_once"],
        )
        for persistent in (False, True):
            with self.subTest(persistent=persistent):
                backend = self._backend(
                    rename_requires_closed_descendants=True
                )
                real_close = lease_module.os.close
                coordinator_close_calls = 0
                coordinator_close_lock_states: list[bool] = []
                rollback_lock_states: list[bool] = []

                def fail_coordinator_close(descriptor: int) -> None:
                    nonlocal coordinator_close_calls
                    coordinator = backend.coordinator
                    if coordinator is None or descriptor != coordinator.fd:
                        real_close(descriptor)
                        return
                    coordinator_close_calls += 1
                    coordinator_close_lock_states.append(coordinator.locked)
                    backend.events.append(
                        f"close-coordinator-lock:{coordinator_close_calls}"
                    )
                    if coordinator_close_calls == 1 or persistent:
                        raise OSError(
                            "injected coordinator close failure "
                            f"{coordinator_close_calls}"
                        )
                    real_close(descriptor)

                def observe_rollback_lock(event: str) -> None:
                    if event.startswith(
                        ("rename:run-", "open-entry:", "delete:")
                    ):
                        coordinator = backend.coordinator
                        rollback_lock_states.append(
                            coordinator is not None and coordinator.locked
                        )

                backend.after_event = observe_rollback_lock
                with (
                    mock.patch.object(
                        lease_module.os,
                        "close",
                        side_effect=fail_coordinator_close,
                    ),
                    self.assertRaisesRegex(
                        OSError, "coordinator close failure 1"
                    ) as caught,
                ):
                    self._create(backend)

                self.assertEqual(coordinator_close_calls, 2)
                self.assertEqual(
                    coordinator_close_lock_states, [True, True]
                )
                first_close = backend.events.index("close-coordinator-lock:1")
                second_close = backend.events.index("close-coordinator-lock:2")
                rename_back = backend.events.index(
                    f"rename:run-{self.run_id}->.staging-{self.run_id}:False"
                )
                self.assertLess(first_close, rename_back)
                self.assertGreater(second_close, rename_back)
                self.assertTrue(rollback_lock_states)
                self.assertTrue(all(rollback_lock_states))
                self.assertEqual(
                    backend.events[-1], "close-coordinator-lock:2"
                )
                notes: tuple[str, ...] = tuple(
                    getattr(caught.exception, "__notes__", ())
                )
                self.assertTrue(
                    all(len(note.encode("utf-8")) <= 1_024 for note in notes)
                )
                if persistent:
                    self.assertIn("close failure 2", notes[-1])
                else:
                    self.assertFalse(
                        any("close failure 2" in note for note in notes)
                    )
                self.assertEqual(
                    set(self._managed_node(backend).children),
                    {".hoimin-coordinator"},
                    f"notes={notes!r}; tail={backend.events[-30:]!r}",
                )
                self.assertEqual(len(backend.live_resources), 0)
                coordinator = backend.coordinator
                assert coordinator is not None
                if persistent:
                    descriptor = coordinator.fd
                    self.assertGreaterEqual(descriptor, 0)
                    backend.coordinator = None
                    del coordinator
                    gc.collect()
                    try:
                        with self.assertRaises(OSError):
                            os.fstat(descriptor)
                    finally:
                        try:
                            os.close(descriptor)
                        except OSError:
                            pass
                else:
                    self.assertEqual(coordinator.fd, -1)

    def test_locked_coordinator_close_refuses_an_unlocked_descriptor(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_locked_coordinator_once"],
        )
        backing = tempfile.TemporaryFile()
        self.addCleanup(backing.close)
        descriptor = os.dup(backing.fileno())
        lock = LeaseLock(descriptor)

        errors = lease_module._close_locked_coordinator_once(
            lock, "managed coordinator"
        )

        self.assertTrue(any("held lock" in error for error in errors))
        self.assertEqual(lock.fd, descriptor)
        self.assertFalse(lock.locked)
        os.fstat(descriptor)
        os.close(descriptor)
        lock.fd = -1

    def test_child_rollback_checks_absence_only_after_owner_close(self) -> None:
        child_name = "candidate-0001"
        for close_failures in (1, 2):
            with self.subTest(close_failures=close_failures):
                backend = self._backend(
                    rename_requires_closed_descendants=False
                )
                scratch = self._create(backend)
                child_resource: _ManagedRecordedResource | None = None
                retained_children: list[DirectoryCapability] = []
                armed = False
                real_create_directory = backend.create_directory

                def create_directory(
                    parent: DirectoryCapability,
                    name: str,
                    share_policy: SharePolicy,
                ) -> DirectoryCapability:
                    capability = real_create_directory(
                        parent, name, share_policy
                    )
                    if name == child_name:
                        retained_children.append(capability)
                    return capability

                def close_retained_children() -> None:
                    for capability in retained_children:
                        if capability.is_open:
                            capability.close()

                self.addCleanup(close_retained_children)

                def arm_child_close(event: str) -> None:
                    nonlocal armed, child_resource
                    if event != f"entry:{child_name}" or armed:
                        return
                    armed = True
                    child_resource = next(
                        resource
                        for resource in backend.live_resources
                        if resource.node.name == child_name
                    )
                    child_resource.close_failures = close_failures

                backend.after_event = arm_child_close
                start = len(backend.events)
                with (
                    mock.patch.object(
                        backend,
                        "create_directory",
                        side_effect=create_directory,
                    ),
                    self.assertRaisesRegex(OSError, "injected close failure"),
                ):
                    scratch.create_child(child_name)

                rollback_events = backend.events[start:]
                self.assertEqual(
                    rollback_events.count(f"entry:{child_name}"),
                    1,
                )
                self.assertEqual(
                    rollback_events.count(f"close:{child_name}"), 2
                )
                self.assertIsNotNone(child_resource)
                assert child_resource is not None
                if close_failures == 1:
                    self.assertTrue(child_resource.closed)
                else:
                    self.assertFalse(child_resource.closed)

    def test_child_meter_heartbeat_and_close_preserve_structural_ownership(
        self,
    ) -> None:
        backend = self._backend(rename_requires_closed_descendants=False)
        scratch = self._create(backend)

        child_path = scratch.create_child("candidate-0001")
        self.assertEqual(child_path, scratch.path / "candidate-0001")
        self.assertIn(
            "create-directory:candidate-0001:pinned", backend.events
        )
        child = scratch.open_child("candidate-0001", SharePolicy.SCAN)
        self.assertTrue(child.is_open)
        child.close()
        meter = scratch.reopen_for_meter()
        self.assertTrue(meter.is_open)
        meter.close()
        self.assertIn("reopen-directory:preserve", backend.events)

        root = scratch._root
        assert root is not None
        root_node = backend._resource(root).node
        original = root_node.children.pop(".hoimin-heartbeat.json")
        original.name = ".hoimin-heartbeat.original"
        root_node.children[original.name] = original
        backend._new_node(
            EntryKind.REGULAR,
            SecurityDomain.MANAGED,
            parent=root_node,
            name=".hoimin-heartbeat.json",
        )
        touch_count = backend.events.count("touch:.hoimin-heartbeat.json")
        with self.assertRaisesRegex(OSError, "heartbeat identity"):
            scratch.refresh_heartbeat()
        self.assertEqual(
            backend.events.count("touch:.hoimin-heartbeat.json"), touch_count
        )

        heartbeat = scratch._heartbeat
        assert heartbeat is not None
        heartbeat_resource = backend._resource(heartbeat)
        heartbeat_resource.close_failures = 1
        first_errors = scratch.close_capabilities()
        self.assertTrue(any("heartbeat" in error for error in first_errors))
        self.assertIs(scratch._heartbeat, heartbeat)
        self.assertTrue(heartbeat.is_open)
        self.assertIsNone(scratch._lease)
        self.assertIsNone(scratch._root)
        self.assertIsNone(scratch._managed_root_capability)
        self.assertEqual(scratch.close_capabilities(), ())
        self.assertIsNone(scratch._heartbeat)

    def test_child_iterator_cleanup_preserves_iteration_primary(self) -> None:
        backend = self._backend(rename_requires_closed_descendants=False)
        scratch = self._create(backend)
        backend.iterator_failure = ValueError("injected iteration primary")
        backend.failures[f"close:run-{self.run_id}"] = MemoryError(
            "injected iterator close failure"
        )

        with self.assertRaisesRegex(
            ValueError, "iteration primary"
        ) as caught:
            scratch.create_child("candidate-0001")

        notes = getattr(caught.exception, "__notes__", ())
        self.assertTrue(any("MemoryError" in note for note in notes))
        root = scratch._root
        assert root is not None
        self.assertNotIn(
            "candidate-0001", backend._resource(root).node.children
        )
        self.assertEqual(len(backend.live_resources), 3)

    def test_child_replacement_is_rejected_and_deadline_stops_before_flush(
        self,
    ) -> None:
        backend = self._backend(rename_requires_closed_descendants=False)
        scratch = self._create(backend)
        scratch.create_child("candidate-0001")
        root = scratch._root
        assert root is not None
        root_node = backend._resource(root).node
        original = root_node.children.pop("candidate-0001")
        original.name = "candidate-original"
        root_node.children[original.name] = original
        replacement = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=root_node,
            name="candidate-0001",
        )
        with self.assertRaisesRegex(OSError, "child identity"):
            scratch.open_child("candidate-0001", SharePolicy.SCAN)
        self.assertIs(root_node.children["candidate-0001"], replacement)

        before_flush = backend.events.count("flush:.hoimin-heartbeat.json")
        with self.assertRaisesRegex(TimeoutError, "heartbeat"):
            scratch.refresh_heartbeat(
                deadline=5.0,
                monotonic=mock.Mock(side_effect=[0.0, 6.0]),
            )
        self.assertIn("touch:.hoimin-heartbeat.json", backend.events)
        self.assertEqual(
            backend.events.count("flush:.hoimin-heartbeat.json"), before_flush
        )

    def test_finalizer_performs_close_only(self) -> None:
        backend = self._backend(rename_requires_closed_descendants=False)
        scratch = self._create(backend)
        before = len(backend.events)

        scratch.__del__()

        tail = backend.events[before:]
        self.assertTrue(tail)
        self.assertTrue(all(event.startswith("close:") for event in tail))
        self.assertFalse(any(event.startswith("rename:") for event in tail))
        self.assertFalse(any(event.startswith("delete:") for event in tail))


class AnchoredDiskGuardTests(unittest.TestCase):
    def setUp(self) -> None:
        self._identity_counter = 0

    def _node(
        self,
        *,
        filesystem: FilesystemIdentity | None = None,
        kind: EntryKind = EntryKind.DIRECTORY,
        logical_size: int = 0,
        identity: FileIdentity | None = None,
    ) -> _RecordedNode:
        selected_filesystem = filesystem or FilesystemIdentity(17, 19, 23)
        self._identity_counter += 1
        selected_identity = identity or FileIdentity(
            selected_filesystem.volume,
            self._identity_counter,
        )
        return _RecordedNode(
            selected_identity,
            selected_filesystem,
            kind=kind,
            logical_size=logical_size,
        )

    def _policy(
        self,
        root: Path,
        *,
        max_disk_bytes: int = 1_000_000,
        min_free_bytes: int = 1,
    ) -> DiskPolicy:
        with mock.patch(
            "tools.focused_mutation_support.disk.canonical_scratch_root",
            return_value=root,
        ):
            return DiskPolicy(
                max_disk_bytes=max_disk_bytes,
                min_free_bytes=min_free_bytes,
                scratch_root=root,
            )

    def _recording_guard(
        self,
        path: Path,
        node: _RecordedNode,
        *,
        backend: _RecordingFilesystemBackend | None = None,
        meter_root: MeterRoot | None = None,
        monotonic: Callable[[], float] = lambda: 0.0,
        max_disk_bytes: int = 1_000_000,
        min_free_bytes: int = 1,
    ) -> tuple[DiskGuard, _RecordingFilesystemBackend]:
        selected = backend or _RecordingFilesystemBackend()
        selected.register(path, node)
        guard = DiskGuard(
            self._policy(
                path,
                max_disk_bytes=max_disk_bytes,
                min_free_bytes=min_free_bytes,
            ),
            [meter_root or MeterRoot(path, enforcement="owned:test")],
            monotonic=monotonic,
            backend=selected,
        )
        return guard, selected

    def _reservation_guard(
        self,
        observation: DiskObservation,
        *,
        max_disk_bytes: int = 1_000,
        min_free_bytes: int = 10,
    ) -> tuple[
        DiskGuard,
        _RecordingFilesystemBackend,
        DirectoryCapability,
    ]:
        path = Path("C:/recorded/owned")
        filesystem = FilesystemIdentity(47, 53, 59)
        root = self._node(filesystem=filesystem)
        backend = _RecordingFilesystemBackend()
        guard, _ = self._recording_guard(
            path,
            root,
            backend=backend,
            max_disk_bytes=max_disk_bytes,
            min_free_bytes=min_free_bytes,
        )
        guard.observations.append(observation)
        borrowed = backend.directory_capability(
            self._node(filesystem=filesystem),
            Path("C:/recorded/spool"),
        )
        self.addCleanup(borrowed.close)
        self.addCleanup(guard.close)
        return guard, backend, borrowed

    def test_canonical_validation_uses_selected_backend_and_never_statvfs(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            backend = _RecordingFilesystemBackend()
            backend.register(root, self._node())

            with mock.patch.object(
                os,
                "statvfs",
                side_effect=AssertionError("raw POSIX capacity call"),
                create=True,
            ):
                selected = canonical_scratch_root(root, backend=backend)

            self.assertEqual(selected, root)
            self.assertEqual(backend.open_root_calls, [root])
            self.assertEqual(len(backend.available_calls), 1)
            self.assertEqual(backend.active_resources, 0)

    def test_canonical_validation_keeps_capacity_primary_and_close_note(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            backend = _RecordingFilesystemBackend()
            node = self._node()
            backend.register(root, node)
            backend.available_error = OSError("injected capacity primary")
            backend.close_failures[node.identity] = 1

            with self.assertRaisesRegex(
                ValueError,
                "scratch root free space cannot be queried",
            ) as caught:
                canonical_scratch_root(root, backend=backend)

            self.assertIn("injected capacity primary", str(caught.exception))
            self.assertTrue(
                any(
                    "close" in note and "injected capability close failure" in note
                    for note in getattr(caught.exception, "__notes__", ())
                )
            )
            self.assertEqual(backend.active_resources, 0)

    def test_canonical_close_retry_failure_is_bounded_secondary(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            backend = _RecordingFilesystemBackend()
            node = self._node()
            backend.register(root, node)
            backend.available_error = OSError("injected capacity primary")
            backend.close_failures[node.identity] = 2

            with self.assertRaisesRegex(
                ValueError,
                "injected capacity primary",
            ) as caught:
                canonical_scratch_root(root, backend=backend)

            notes = getattr(caught.exception, "__notes__", ())
            self.assertEqual(len(notes), 2)
            self.assertTrue(all(len(note.encode("utf-8")) <= 512 for note in notes))
            self.assertEqual(backend.operations.count("capability.close"), 2)
            backend.available_calls[0].close()
            self.assertEqual(backend.active_resources, 0)

    def test_canonical_non_directory_is_classified_by_selected_backend(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = (Path(raw) / "not-a-directory").resolve()
            root.write_text("payload", encoding="utf-8")
            backend = _RecordingFilesystemBackend()
            backend.open_root_events[root] = [NotADirectoryError(root)]

            with self.assertRaisesRegex(
                ValueError,
                "scratch root is not a directory",
            ):
                canonical_scratch_root(root, backend=backend)

            self.assertEqual(backend.open_root_calls, [root])

    @unittest.skipUnless(os.name == "nt", "requires Windows filesystem backend")
    def test_windows_canonical_non_directory_keeps_public_error(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "not-a-directory"
            path.write_text("payload", encoding="utf-8")

            with self.assertRaisesRegex(
                ValueError,
                "scratch root is not a directory",
            ):
                canonical_scratch_root(path)

    def test_factory_is_called_once_and_retains_replaced_root_object(self) -> None:
        path = Path("C:/recorded/owned")
        backend = _RecordingFilesystemBackend()
        original = self._node()
        original.add(
            "payload",
            self._node(kind=EntryKind.REGULAR, logical_size=4),
        )
        replacement = self._node()
        replacement.add(
            "replacement",
            self._node(kind=EntryKind.REGULAR, logical_size=100),
        )
        backend.register(path, original)
        factory_calls = 0

        def factory() -> DirectoryCapability:
            nonlocal factory_calls
            factory_calls += 1
            return backend.directory_capability(original, path)

        guard = DiskGuard(
            self._policy(path),
            [
                MeterRoot(
                    path,
                    enforcement="owned:test",
                    capability_factory=factory,
                )
            ],
            backend=backend,
        )
        backend.roots[path] = replacement

        self.assertIsNone(guard.sample())
        self.assertEqual(guard.observations[-1].owned_bytes, 4)
        self.assertEqual(factory_calls, 1)
        self.assertEqual(backend.open_root_calls, [])
        self.assertEqual(guard.close(), ())
        self.assertEqual(backend.closed_identities.count(original.identity), 2)
        self.assertEqual(backend.active_resources, 0)

    def test_factory_capability_validation_rolls_back_every_owner(self) -> None:
        path = Path("C:/recorded/owned")
        backend = _RecordingFilesystemBackend()
        first_node = self._node()
        invalid_node = self._node()
        first = backend.directory_capability(first_node, path)
        invalid = backend.directory_capability(
            invalid_node,
            Path("C:/recorded/wrong"),
        )

        with self.assertRaisesRegex(RuntimeError, "path"):
            DiskGuard(
                self._policy(path),
                [
                    MeterRoot(path, capability_factory=lambda: first),
                    MeterRoot(path, capability_factory=lambda: invalid),
                ],
                backend=backend,
            )

        self.assertTrue(first.closed)
        self.assertTrue(invalid.closed)
        self.assertEqual(backend.active_resources, 0)

    def test_constructor_rollback_finally_retries_each_failed_close(self) -> None:
        path = Path("C:/recorded/owned")
        backend = _RecordingFilesystemBackend()
        first = backend.directory_capability(
            self._node(),
            path,
            close_failures=1,
        )
        invalid = backend.directory_capability(
            self._node(),
            Path("C:/recorded/wrong"),
            close_failures=1,
        )

        with self.assertRaisesRegex(RuntimeError, "path") as caught:
            DiskGuard(
                self._policy(path),
                [
                    MeterRoot(path, capability_factory=lambda: first),
                    MeterRoot(path, capability_factory=lambda: invalid),
                ],
                backend=backend,
            )

        self.assertTrue(first.closed)
        self.assertTrue(invalid.closed)
        self.assertEqual(backend.active_resources, 0)
        self.assertEqual(len(getattr(caught.exception, "__notes__", ())), 2)

    def test_constructor_state_registration_failure_closes_new_owner(self) -> None:
        path = Path("C:/recorded/owned")
        backend = _RecordingFilesystemBackend()
        capability = backend.directory_capability(self._node(), path)

        with (
            mock.patch(
                "tools.focused_mutation_support.disk._RootCapabilityState",
                side_effect=MemoryError("injected state allocation failure"),
            ),
            self.assertRaisesRegex(MemoryError, "state allocation failure"),
        ):
            DiskGuard(
                self._policy(path),
                [MeterRoot(path, capability_factory=lambda: capability)],
                backend=backend,
            )

        self.assertTrue(capability.closed)
        self.assertEqual(backend.active_resources, 0)

    def test_acquisition_error_state_failure_rolls_back_prior_owner(self) -> None:
        first_path = Path("C:/recorded/first")
        missing_path = Path("C:/recorded/missing")
        backend = _RecordingFilesystemBackend()
        backend.register(first_path, self._node())
        disk_module = __import__(
            "tools.focused_mutation_support.disk",
            fromlist=["_RootCapabilityState"],
        )
        real_state = disk_module._RootCapabilityState
        calls = 0

        def allocate_state(*args: object, **kwargs: object) -> object:
            nonlocal calls
            calls += 1
            if calls == 2:
                raise MemoryError("injected error-state allocation failure")
            return real_state(*args, **kwargs)

        with (
            mock.patch(
                "tools.focused_mutation_support.disk._RootCapabilityState",
                side_effect=allocate_state,
            ),
            self.assertRaisesRegex(MemoryError, "error-state allocation failure"),
        ):
            DiskGuard(
                self._policy(first_path),
                [MeterRoot(first_path), MeterRoot(missing_path)],
                backend=backend,
            )

        self.assertEqual(backend.active_resources, 0)

    def test_factory_rejects_wrong_owner_closed_and_non_directory_before_sample(
        self,
    ) -> None:
        path = Path("C:/recorded/owned")
        for case in ("owner", "closed", "kind"):
            with self.subTest(case=case):
                backend = _RecordingFilesystemBackend()
                node = self._node()
                if case == "owner":
                    other = _RecordingFilesystemBackend()
                    capability: object = other.directory_capability(node, path)
                elif case == "closed":
                    capability = backend.directory_capability(node, path)
                    capability.close()
                else:
                    file_node = self._node(
                        kind=EntryKind.REGULAR,
                        logical_size=1,
                    )
                    capability = backend.file_capability(file_node, path)

                def factory() -> DirectoryCapability:
                    return cast(DirectoryCapability, capability)

                with self.assertRaisesRegex(RuntimeError, case):
                    DiskGuard(
                        self._policy(path),
                        [
                            MeterRoot(
                                path,
                                capability_factory=factory,
                            )
                        ],
                        backend=backend,
                    )

                if isinstance(capability, (DirectoryCapability, FileCapability)):
                    self.assertFalse(capability.is_open)

    def test_exact_path_missing_then_records_and_closes_transient_owner(self) -> None:
        path = Path("C:/recorded/owned")
        exact_path = Path("C:/recorded/exact")
        backend = _RecordingFilesystemBackend()
        retained = self._node()
        exact = self._node(filesystem=retained.filesystem)
        backend.register(path, retained)
        backend.open_root_events[exact_path] = [FileNotFoundError(exact_path), exact]
        guard, _ = self._recording_guard(
            path,
            retained,
            backend=backend,
            meter_root=MeterRoot(
                path,
                charge_owned_bytes=False,
                enforcement="capacity_only:test",
                exact_path=exact_path,
            ),
        )

        self.assertIsNone(guard.sample())
        self.assertIsNone(guard.sample())
        self.assertEqual(backend.open_root_calls, [path, exact_path, exact_path])
        self.assertEqual(backend.closed_identities.count(exact.identity), 1)
        self.assertEqual(backend.active_resources, 1)
        self.assertEqual(guard.close(), ())

    def test_exact_path_close_failure_gets_a_final_cleanup_attempt(self) -> None:
        path = Path("C:/recorded/owned")
        exact_path = Path("C:/recorded/exact")
        backend = _RecordingFilesystemBackend()
        retained = self._node()
        exact = self._node(filesystem=retained.filesystem)
        backend.register(path, retained)
        backend.open_root_events[exact_path] = [exact]
        backend.close_failures[exact.identity] = 1
        guard, _ = self._recording_guard(
            path,
            retained,
            backend=backend,
            meter_root=MeterRoot(
                path,
                charge_owned_bytes=False,
                enforcement="capacity_only:test",
                exact_path=exact_path,
            ),
        )
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        assert failure.message is not None
        self.assertIn("injected capability close failure", failure.message)
        self.assertEqual(backend.active_resources, 1)

    def test_rejected_exact_path_owner_gets_a_final_cleanup_attempt(self) -> None:
        path = Path("C:/recorded/owned")
        exact_path = Path("C:/recorded/exact")
        cases = (
            ("owner", "disk root capability owner does not match backend"),
            ("kind", "disk root capability kind is not a directory"),
            ("path", "disk root capability path does not match MeterRoot"),
        )
        for case, primary_message in cases:
            with self.subTest(case=case):
                backend = _RecordingFilesystemBackend()
                retained = self._node()
                backend.register(path, retained)
                invalid: DirectoryCapability | FileCapability
                if case == "owner":
                    invalid_backend = _RecordingFilesystemBackend()
                    invalid = invalid_backend.directory_capability(
                        self._node(filesystem=retained.filesystem),
                        exact_path,
                        close_failures=1,
                    )
                elif case == "kind":
                    invalid_backend = backend
                    invalid_node = self._node(
                        filesystem=retained.filesystem,
                        kind=EntryKind.REGULAR,
                    )
                    backend.close_failures[invalid_node.identity] = 1
                    invalid = backend.file_capability(invalid_node, exact_path)
                else:
                    invalid_backend = backend
                    invalid = backend.directory_capability(
                        self._node(filesystem=retained.filesystem),
                        Path("C:/recorded/wrong"),
                        close_failures=1,
                    )
                backend.open_root_events[exact_path] = [invalid]
                guard, _ = self._recording_guard(
                    path,
                    retained,
                    backend=backend,
                    meter_root=MeterRoot(
                        path,
                        charge_owned_bytes=False,
                        enforcement=f"capacity_only:{case}",
                        exact_path=exact_path,
                    ),
                )
                self.addCleanup(guard.close)
                self.addCleanup(invalid.close)

                failure = guard.sample()

                self.assertIsNotNone(failure)
                assert failure is not None
                self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
                assert failure.message is not None
                self.assertTrue(
                    failure.message.startswith(
                        f"RuntimeError: {primary_message}"
                    )
                )
                self.assertEqual(
                    failure.message.count("filesystem capability close failed"),
                    1,
                )
                self.assertIn("injected capability close failure", failure.message)
                self.assertLessEqual(len(failure.message.encode("utf-8")), 1_024)
                self.assertEqual(
                    (
                        invalid.closed,
                        invalid_backend.operations.count("capability.close"),
                        invalid_backend.closed_identities.count(invalid.identity),
                        backend.active_resources,
                        invalid_backend.active_resources,
                    ),
                    (
                        True,
                        2,
                        1,
                        1,
                        1 if invalid_backend is backend else 0,
                    ),
                )
                self.assertEqual(guard.close(), ())
                self.assertEqual(backend.active_resources, 0)
                self.assertEqual(
                    invalid_backend.closed_identities.count(invalid.identity),
                    1,
                )

    def test_exact_path_replacement_and_later_disappearance_are_typed(self) -> None:
        path = Path("C:/recorded/owned")
        exact_path = Path("C:/recorded/exact")
        for case in ("replacement", "disappearance"):
            with self.subTest(case=case):
                backend = _RecordingFilesystemBackend()
                retained = self._node()
                exact = self._node(filesystem=retained.filesystem)
                backend.register(path, retained)
                events: list[object] = [exact]
                if case == "replacement":
                    events.append(self._node(filesystem=retained.filesystem))
                else:
                    events.append(FileNotFoundError(exact_path))
                backend.open_root_events[exact_path] = events
                guard, _ = self._recording_guard(
                    path,
                    retained,
                    backend=backend,
                    meter_root=MeterRoot(
                        path,
                        charge_owned_bytes=False,
                        enforcement=f"capacity_only:{case}",
                        exact_path=exact_path,
                    ),
                )
                self.addCleanup(guard.close)

                self.assertIsNone(guard.sample())
                failure = guard.sample()

                self.assertIsNotNone(failure)
                assert failure is not None
                self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
                assert failure.message is not None
                self.assertIn(f"capacity_only:{case}", failure.message)

    def test_generic_dfs_deduplicates_hard_links_and_capacity(self) -> None:
        filesystem = FilesystemIdentity(31, 37, 41)
        backend = _RecordingFilesystemBackend()
        first_path = Path("C:/recorded/first")
        second_path = Path("C:/recorded/second")
        first_root = self._node(filesystem=filesystem)
        second_root = self._node(filesystem=filesystem)
        payload = self._node(
            filesystem=filesystem,
            kind=EntryKind.REGULAR,
            logical_size=32,
        )
        first_root.add("payload", payload)
        second_root.add("payload-link", payload)
        backend.register(first_path, first_root)
        backend.register(second_path, second_root)
        guard = DiskGuard(
            self._policy(first_path),
            [
                MeterRoot(first_path, enforcement="owned:first"),
                MeterRoot(second_path, enforcement="owned:second"),
            ],
            backend=backend,
        )
        self.addCleanup(guard.close)

        self.assertIsNone(guard.sample())

        observation = guard.observations[-1]
        self.assertEqual(observation.owned_bytes, 32)
        self.assertEqual(len(observation.identity_bytes), 1)
        self.assertEqual(len(backend.available_calls), 1)
        self.assertEqual(
            observation.root_owned_bytes,
            {"owned:first": 32, "owned:second": 0},
        )

    def test_generic_dfs_ignores_only_vanished_child_and_skips_unfollowable(
        self,
    ) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        vanished = self._node(kind=EntryKind.REGULAR, logical_size=5)
        root.add("vanished", vanished)
        root.add("link", self._node(kind=EntryKind.REPARSE))
        root.add("device", self._node(kind=EntryKind.OTHER))
        backend = _RecordingFilesystemBackend()

        def vanish(
            _parent: DirectoryCapability,
            name: str,
            _kind: EntryKind,
        ) -> None:
            if name == "vanished":
                root.children.pop(name, None)

        backend.before_open = vanish
        guard, _ = self._recording_guard(path, root, backend=backend)
        self.addCleanup(guard.close)

        self.assertIsNone(guard.sample())
        self.assertEqual(guard.observations[-1].owned_bytes, 0)
        self.assertEqual(backend.operations.count("open_file"), 1)
        self.assertEqual(backend.operations.count("open_directory"), 0)

    def test_generic_dfs_rejects_opened_identity_kind_and_filesystem_changes(
        self,
    ) -> None:
        path = Path("C:/recorded/owned")
        for case in ("identity", "kind", "filesystem"):
            with self.subTest(case=case):
                root = self._node()
                original = self._node(
                    kind=(
                        EntryKind.REGULAR
                        if case in {"identity", "kind"}
                        else EntryKind.DIRECTORY
                    ),
                    logical_size=7,
                )
                root.add("child", original)
                backend = _RecordingFilesystemBackend()

                def replace_child(
                    _parent: DirectoryCapability,
                    name: str,
                    _kind: EntryKind,
                    *,
                    selected: str = case,
                ) -> None:
                    if selected == "kind":
                        backend.open_overrides[(EntryKind.REGULAR, name)] = (
                            self._node(kind=EntryKind.REPARSE)
                        )
                    elif selected == "filesystem":
                        backend.open_overrides[(EntryKind.DIRECTORY, name)] = (
                            self._node(filesystem=FilesystemIdentity(43))
                        )
                    else:
                        backend.open_overrides[(EntryKind.REGULAR, name)] = (
                            self._node(
                                kind=EntryKind.REGULAR,
                                logical_size=7,
                            )
                        )

                backend.before_open = replace_child
                guard, _ = self._recording_guard(path, root, backend=backend)
                self.addCleanup(guard.close)

                failure = guard.sample()

                self.assertIsNotNone(failure)
                assert failure is not None
                self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
                self.assertEqual(backend.active_resources, 1)

    def test_entry_limit_accepts_exact_boundary_and_rejects_next(self) -> None:
        path = Path("C:/recorded/owned")
        for count, stopped in ((250_000, False), (250_001, True)):
            with self.subTest(count=count):
                root = self._node()
                virtual = self._node(kind=EntryKind.REPARSE)
                backend = _RecordingFilesystemBackend()
                backend.virtual_entries[root.identity] = (virtual, count)
                guard, _ = self._recording_guard(path, root, backend=backend)
                self.addCleanup(guard.close)

                failure = guard.sample()

                if stopped:
                    self.assertIsNotNone(failure)
                    assert failure is not None
                    assert failure.message is not None
                    self.assertIn("250000", failure.message)
                else:
                    self.assertIsNone(failure)

    def test_depth_boundary_has_129_walker_and_130_total_capabilities(self) -> None:
        path = Path("C:/recorded/owned")
        for child_count, stopped in ((128, False), (129, True)):
            with self.subTest(child_count=child_count):
                root = self._node()
                current = root
                for index in range(child_count):
                    child = self._node()
                    current.add(f"d{index}", child)
                    current = child
                backend = _RecordingFilesystemBackend()
                guard, _ = self._recording_guard(path, root, backend=backend)
                self.addCleanup(guard.close)

                failure = guard.sample()

                if stopped:
                    self.assertIsNotNone(failure)
                    assert failure is not None
                    assert failure.message is not None
                    self.assertIn("depth exceeds 128", failure.message)
                else:
                    self.assertIsNone(failure)
                self.assertEqual(backend.max_walker_resources, 129)
                self.assertEqual(backend.max_active_resources, 130)
                self.assertEqual(backend.active_resources, 1)

    def test_deadline_after_child_open_prevents_iterator_transfer(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        root.add("child", self._node())
        backend = _RecordingFilesystemBackend()
        backend.close_failures[root.children["child"].identity] = 1
        clock = [0.0]

        def expire(
            _parent: DirectoryCapability,
            _name: str,
            _kind: EntryKind,
        ) -> None:
            clock[0] = 5.0

        backend.before_open = expire
        guard, _ = self._recording_guard(
            path,
            root,
            backend=backend,
            monotonic=lambda: clock[0],
        )
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        assert failure.message is not None
        self.assertIn("exceeded five seconds", failure.message)
        self.assertEqual(backend.operations.count("entries_owned"), 1)
        self.assertEqual(backend.active_resources, 1)

    def test_deadline_after_root_reopen_retries_the_untransferred_owner(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        backend = _RecordingFilesystemBackend()
        clock = [0.0]
        guard, _ = self._recording_guard(
            path,
            root,
            backend=backend,
            monotonic=lambda: clock[0],
        )
        backend.close_failures[root.identity] = 1
        backend.after_reopen = lambda: clock.__setitem__(0, 5.0)
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        self.assertEqual(backend.active_resources, 1)

    def test_child_iterator_construction_failure_retries_transferred_owner(
        self,
    ) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        child = self._node()
        root.add("child", child)
        backend = _RecordingFilesystemBackend()
        backend.iterator_construction_errors[child.identity] = OSError(
            "injected child iterator construction failure"
        )
        backend.close_failures[child.identity] = 1
        guard, _ = self._recording_guard(path, root, backend=backend)
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        assert failure.message is not None
        self.assertIn("child iterator construction failure", failure.message)
        self.assertEqual(backend.active_resources, 1)
        self.assertEqual(backend.walker_resources, 0)

    def test_deadline_after_iterator_transfer_unwinds_the_new_owner(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        backend = _RecordingFilesystemBackend()
        clock = [0.0]
        backend.after_entries_owned = lambda: clock.__setitem__(0, 5.0)
        guard, _ = self._recording_guard(
            path,
            root,
            backend=backend,
            monotonic=lambda: clock[0],
        )
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        assert failure.message is not None
        self.assertIn("exceeded five seconds", failure.message)
        self.assertEqual(backend.walker_resources, 0)
        self.assertEqual(backend.active_resources, 1)

    def test_frame_registration_failure_closes_the_transferred_iterator(
        self,
    ) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        backend = _RecordingFilesystemBackend()
        guard, _ = self._recording_guard(path, root, backend=backend)
        self.addCleanup(guard.close)

        with mock.patch(
            "tools.focused_mutation_support.disk._MeterFrame",
            side_effect=MemoryError("injected frame allocation failure"),
        ):
            failure = guard.sample()

        self.assertIsNotNone(failure)
        self.assertEqual(backend.walker_resources, 0)
        self.assertEqual(backend.active_resources, 1)

    def test_iterator_construction_and_unwind_close_failures_do_not_leak(
        self,
    ) -> None:
        path = Path("C:/recorded/owned")
        for case in ("construction", "iteration"):
            with self.subTest(case=case):
                root = self._node()
                backend = _RecordingFilesystemBackend()
                guard, _ = self._recording_guard(path, root, backend=backend)
                if case == "construction":
                    backend.iterator_construction_errors[root.identity] = OSError(
                        "injected iterator construction failure"
                    )
                else:
                    backend.iterator_fail_after[root.identity] = 0
                    backend.close_failures[root.identity] = 1
                self.addCleanup(guard.close)

                failure = guard.sample()

                self.assertIsNotNone(failure)
                assert failure is not None
                expected = (
                    "injected iterator construction failure"
                    if case == "construction"
                    else "injected iteration failure"
                )
                assert failure.message is not None
                self.assertIn(expected, failure.message)
                self.assertEqual(backend.active_resources, 1)
                self.assertEqual(backend.walker_resources, 0)

    def test_regular_file_close_failure_gets_a_final_cleanup_attempt(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        payload = self._node(kind=EntryKind.REGULAR, logical_size=1)
        root.add("payload", payload)
        backend = _RecordingFilesystemBackend()
        backend.close_failures[payload.identity] = 1
        guard, _ = self._recording_guard(path, root, backend=backend)
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        assert failure.message is not None
        self.assertIn("injected capability close failure", failure.message)
        self.assertEqual(backend.active_resources, 1)

    @unittest.skipUnless(os.name == "nt", "requires Windows capacity API")
    def test_windows_guard_samples_a_real_root(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / "payload").write_bytes(b"1234")
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1024,
                    min_free_bytes=1,
                    scratch_root=root,
                ),
                [MeterRoot(root, enforcement="owned:test")],
            )

            self.assertIsNone(guard.sample())
            self.assertEqual(guard.observations[-1].owned_bytes, 4)
            self.assertEqual(guard.close(), ())

    def test_directory_stream_checks_deadline_immediately_after_readdir(
        self,
    ) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        root.add("payload", self._node(kind=EntryKind.REGULAR, logical_size=1))
        backend = _RecordingFilesystemBackend()
        clock = [0.0]
        backend.after_iterator_next = lambda: clock.__setitem__(0, 5.0)
        guard, _ = self._recording_guard(
            path,
            root,
            backend=backend,
            monotonic=lambda: clock[0],
        )
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        assert failure.message is not None
        self.assertIn("exceeded five seconds", failure.message)
        self.assertNotIn("open_file", backend.operations)

    def test_expired_measurement_stops_before_root_stat(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        backend = _RecordingFilesystemBackend()
        ticks = iter((0.0, 5.0, 5.0, 5.0))
        guard, _ = self._recording_guard(
            path,
            root,
            backend=backend,
            monotonic=lambda: next(ticks, 5.0),
        )
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        assert failure.message is not None
        self.assertIn("exceeded five seconds", failure.message)
        self.assertEqual(backend.available_calls, [])
        self.assertNotIn("reopen_directory", backend.operations)

    def test_measurement_does_not_stat_after_readdir_crosses_deadline(self) -> None:
        self.test_directory_stream_checks_deadline_immediately_after_readdir()

    def test_capacity_query_crossing_deadline_stops_before_tree_scan(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        backend = _RecordingFilesystemBackend()
        clock = [0.0]
        backend.after_available = lambda: clock.__setitem__(0, 5.0)
        guard, _ = self._recording_guard(
            path,
            root,
            backend=backend,
            monotonic=lambda: clock[0],
        )
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
        assert failure.message is not None
        self.assertIn("exceeded five seconds", failure.message)
        self.assertNotIn("reopen_directory", backend.operations)

    def test_depth_failure_identifies_a_bounded_relative_prefix(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        current = self._node()
        root.add("diagnostic-anchor", current)
        for index in range(129):
            child = self._node()
            current.add(f"d{index}", child)
            current = child
        guard, _backend = self._recording_guard(path, root)
        self.addCleanup(guard.close)

        failure = guard.sample()

        self.assertIsNotNone(failure)
        assert failure is not None
        self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
        assert failure.message is not None
        self.assertIn("diagnostic-anchor/d0/d1", failure.message)
        self.assertLessEqual(len(failure.message.encode("utf-8")), 1_024)

    def test_reserved_spool_bytes_trip_owned_limit_before_write(self) -> None:
        guard, backend, borrowed = self._reservation_guard(
            DiskObservation(owned_bytes=90, available_bytes=1_000),
            max_disk_bytes=100,
        )
        backend.available[borrowed.filesystem] = 1_000
        backend.units[borrowed.filesystem] = 1

        failure = guard.reserve_additional_bytes(10, filesystem=borrowed)

        self.assertIsNotNone(failure)
        assert failure is not None
        self.assertEqual(failure.reason, DiskStopReason.WORKSPACE_SIZE_EXCEEDED)
        assert failure.observation is not None
        self.assertEqual(failure.observation.owned_bytes, 100)
        self.assertTrue(borrowed.is_open)

    def test_reserved_spool_bytes_trip_free_space_reserve_before_write(
        self,
    ) -> None:
        guard, backend, borrowed = self._reservation_guard(
            DiskObservation(owned_bytes=0, available_bytes=20)
        )
        backend.available[borrowed.filesystem] = 20
        backend.units[borrowed.filesystem] = 1

        failure = guard.reserve_additional_bytes(10, filesystem=borrowed)

        self.assertIsNotNone(failure)
        assert failure is not None
        self.assertEqual(failure.reason, DiskStopReason.FILESYSTEM_RESERVE_REACHED)
        assert failure.observation is not None
        self.assertEqual(failure.observation.available_bytes, 7)

    def test_spool_reservation_includes_block_and_metadata_overhead(self) -> None:
        guard, backend, borrowed = self._reservation_guard(
            DiskObservation(owned_bytes=0, available_bytes=16_388)
        )
        backend.available[borrowed.filesystem] = 16_384
        backend.units[borrowed.filesystem] = 4_096

        failure = guard.reserve_additional_bytes(1, filesystem=borrowed)

        self.assertIsNotNone(failure)
        assert failure is not None
        self.assertEqual(failure.reason, DiskStopReason.FILESYSTEM_RESERVE_REACHED)
        assert failure.observation is not None
        self.assertEqual(failure.observation.available_bytes, 0)

    def test_spool_reservation_debits_only_its_own_filesystem(self) -> None:
        guard, backend, borrowed = self._reservation_guard(
            DiskObservation(
                owned_bytes=0,
                available_bytes=11,
                filesystem_available_bytes={
                    "1:1:1": 11,
                    "47:53:59": 1_000,
                },
            )
        )
        backend.available[borrowed.filesystem] = 1_000
        backend.units[borrowed.filesystem] = 1

        failure = guard.reserve_additional_bytes(2, filesystem=borrowed)

        self.assertIsNone(failure)

    def test_spool_capacity_query_failure_is_typed_measurement_stop(self) -> None:
        guard, backend, borrowed = self._reservation_guard(
            DiskObservation(owned_bytes=0, available_bytes=1_000)
        )
        backend.available_error = OSError("injected spool capacity failure")

        failure = guard.reserve_additional_bytes(1, filesystem=borrowed)

        self.assertIsNotNone(failure)
        assert failure is not None
        self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
        assert failure.message is not None
        self.assertIn("spool capacity failure", failure.message)

    def test_spool_reservation_validates_borrowed_owner_open_and_unit(self) -> None:
        for case in ("owner", "closed", "unit"):
            with self.subTest(case=case):
                guard, backend, borrowed = self._reservation_guard(
                    DiskObservation(owned_bytes=0, available_bytes=1_000)
                )
                selected = borrowed
                if case == "owner":
                    other = _RecordingFilesystemBackend()
                    selected = other.directory_capability(
                        self._node(filesystem=borrowed.filesystem),
                        Path("C:/recorded/foreign"),
                    )
                    self.addCleanup(selected.close)
                elif case == "closed":
                    selected.close()
                else:
                    backend.units[borrowed.filesystem] = 0

                if case in {"owner", "closed"}:
                    with self.assertRaises((RuntimeError, ValueError)):
                        guard.reserve_additional_bytes(1, filesystem=selected)
                    self.assertEqual(backend.available_calls, [])
                    self.assertEqual(backend.allocation_calls, [])
                else:
                    failure = guard.reserve_additional_bytes(
                        1,
                        filesystem=selected,
                    )
                    self.assertIsNotNone(failure)
                    assert failure is not None
                    self.assertEqual(
                        failure.reason,
                        DiskStopReason.MEASUREMENT_FAILED,
                    )

    def test_spool_reservation_sticky_failure_precedes_capability_access(self) -> None:
        guard, backend, borrowed = self._reservation_guard(
            DiskObservation(owned_bytes=0, available_bytes=1_000)
        )
        sticky = DiskFailure(
            code=DISK_MEASUREMENT_FAILED,
            reason=DiskStopReason.MEASUREMENT_FAILED,
            message="sticky",
        )
        guard.failure = sticky
        borrowed.close()

        self.assertIs(
            guard.reserve_additional_bytes(1, filesystem=borrowed),
            sticky,
        )
        self.assertEqual(backend.available_calls, [])
        self.assertEqual(backend.allocation_calls, [])

    def test_only_latest_observation_retains_inode_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            owned = Path(directory) / "owned"
            owned.mkdir()
            (owned / "payload").write_bytes(b"payload")
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1,
                    min_free_bytes=1,
                    scratch_root=Path(directory),
                ),
                [MeterRoot(owned, enforcement="owned:test")],
            )
            self.addCleanup(guard.close)

            guard.sample()
            guard.sample()

            self.assertEqual(len(guard.observations), 2)
            self.assertEqual(guard.observations[0].identity_bytes, {})
            self.assertEqual(guard.observations[0].root_owned_identities, {})
            self.assertTrue(guard.observations[1].identity_bytes)
            self.assertIsNotNone(guard.failure)
            failure = guard.failure
            assert failure is not None
            self.assertIsNotNone(failure.observation)
            observation = failure.observation
            assert observation is not None
            self.assertEqual(observation.identity_bytes, {})

    def test_later_measurement_failure_is_exposed_after_sticky_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            owned = Path(directory) / "owned"
            owned.mkdir()
            (owned / "payload").write_bytes(b"payload")
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1,
                    min_free_bytes=1,
                    scratch_root=Path(directory),
                ),
                [MeterRoot(owned, enforcement="owned:test")],
            )
            self.addCleanup(guard.close)

            first = guard.sample()
            with mock.patch(
                "tools.focused_mutation_support.disk._measure_capability",
                side_effect=OSError("injected later scan failure"),
            ):
                sticky = guard.sample()

            self.assertIs(sticky, first)
            self.assertIsNotNone(guard.latest_failure)
            latest_failure = guard.latest_failure
            assert latest_failure is not None
            self.assertEqual(
                latest_failure.reason,
                DiskStopReason.MEASUREMENT_FAILED,
            )
            assert latest_failure.message is not None
            self.assertIn("later scan failure", latest_failure.message)

    def test_join_timeout_keeps_scratch_lease_until_monitor_exits(self) -> None:
        path = Path("C:/recorded/leased")
        backend = _RecordingFilesystemBackend()
        node = self._node()
        backend.available[node.filesystem] = 1_000_000
        backend.units[node.filesystem] = 1
        retained = backend.directory_capability(node, path)
        with mock.patch(
            "tools.focused_mutation_support.disk.canonical_scratch_root",
            return_value=path,
        ):
            policy = DiskPolicy(
                max_disk_bytes=1024 * 1024,
                min_free_bytes=1,
                sample_interval_seconds=0.001,
                scratch_root=path,
            )
        guard = DiskGuard(
            policy,
            [MeterRoot(path, capability_factory=lambda: retained)],
            backend=backend,
        )
        entered = threading.Event()
        release = threading.Event()
        original_sample = guard.sample
        calls = 0

        def blocking_sample() -> DiskFailure | None:
            nonlocal calls
            calls += 1
            if calls > 1:
                entered.set()
                release.wait(timeout=2.0)
            return original_sample()

        with mock.patch.object(guard, "sample", side_effect=blocking_sample):
            guard.start()
            self.assertTrue(entered.wait(timeout=1.0))
            self.assertFalse(guard.stop_and_join(timeout=0.01))
            self.assertTrue(retained.is_open)
            release.set()
            assert guard._thread is not None
            guard._thread.join(timeout=2.0)
            self.assertFalse(guard._thread.is_alive())

        self.assertTrue(retained.closed)
        self.assertEqual(backend.active_resources, 0)

    @unittest.skipIf(os.name == "nt", "surrogateescape names are POSIX-specific")
    def test_invalid_utf8_name_is_measured_without_stopping_monitor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root_fd = os.open(root, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
            try:
                try:
                    file_fd = os.open(
                        b"bad-\xff",
                        os.O_WRONLY | os.O_CREAT,
                        0o600,
                        dir_fd=root_fd,
                    )
                except OSError as error:
                    self.skipTest(f"filesystem rejects non-UTF8 names: {error}")
                os.write(file_fd, b"payload")
                os.close(file_fd)
            finally:
                os.close(root_fd)
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=1024, min_free_bytes=1),
                [MeterRoot(root)],
            )

            failure = guard.sample()

            self.assertIsNone(failure)
            self.assertEqual(guard.observations[-1].owned_bytes, 7)
            guard.close()

    def test_unicode_measurement_error_is_published_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=1024, min_free_bytes=1),
                [MeterRoot(root)],
            )
            with mock.patch(
                "tools.focused_mutation_support.disk._measure_capability",
                side_effect=UnicodeEncodeError("utf-8", "\udcff", 0, 1, "bad"),
            ):
                failure = guard.sample()

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
            guard.close()

    def test_capability_close_failure_is_recorded_without_short_circuit(self) -> None:
        first_path = Path("C:/recorded/first")
        second_path = Path("C:/recorded/second")
        backend = _RecordingFilesystemBackend()
        first = backend.directory_capability(
            self._node(),
            first_path,
            close_failures=1,
        )
        second = backend.directory_capability(self._node(), second_path)
        guard = DiskGuard(
            self._policy(first_path),
            [
                MeterRoot(first_path, capability_factory=lambda: first),
                MeterRoot(second_path, capability_factory=lambda: second),
            ],
            backend=backend,
        )

        errors = guard.close()

        self.assertEqual(len(errors), 1)
        self.assertIn("injected capability close failure", errors[0])
        self.assertTrue(first.is_open)
        self.assertTrue(second.closed)
        self.assertEqual(backend.active_resources, 1)

        guard.close()

        self.assertTrue(first.closed)
        self.assertEqual(backend.active_resources, 0)

    def test_probe_close_retries_duplicate_and_bounds_retry_failure(self) -> None:
        path = Path("C:/recorded/owned")
        for close_failures, expected_errors, expected_active in (
            (1, 1, 1),
            (2, 2, 1),
        ):
            with self.subTest(close_failures=close_failures):
                backend = _RecordingFilesystemBackend()
                root = self._node()
                guard, _ = self._recording_guard(path, root, backend=backend)
                backend.close_failures[root.identity] = close_failures

                errors = guard.probe_close()

                self.assertEqual(len(errors), expected_errors)
                self.assertTrue(
                    all(len(error.encode("utf-8")) <= 512 for error in errors)
                )
                self.assertEqual(backend.active_resources, expected_active)
                guard.close()

    def test_hard_links_across_owned_roots_are_counted_once(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first_root = root / "first-root"
            second_root = root / "second-root"
            first_root.mkdir()
            second_root.mkdir()
            first = first_root / "payload"
            first.write_bytes(b"x" * 32)
            try:
                (second_root / "payload-link").hardlink_to(first)
            except OSError:
                self.skipTest("hard links unavailable")
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=33, min_free_bytes=1),
                [MeterRoot(first_root), MeterRoot(second_root)],
            )

            failure = guard.sample()

            self.assertIsNone(failure)
            self.assertEqual(guard.observations[-1].owned_bytes, 32)
            guard.close()

    def test_counts_owned_regular_files_once_and_deduplicates_capacity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            owned = root / "owned"
            owned.mkdir()
            first = owned / "first"
            first.write_bytes(b"x" * 32)
            second = owned / "second"
            try:
                second.hardlink_to(first)
            except OSError:
                self.skipTest("hard links unavailable")
            capacity_only = root / "cargo-home"
            capacity_only.mkdir()
            (capacity_only / "not-owned").write_bytes(b"y" * 10_000)
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=33, min_free_bytes=1),
                [
                    MeterRoot(owned),
                    MeterRoot(
                        capacity_only,
                        charge_owned_bytes=False,
                        enforcement="capacity_only:cargo_home",
                    ),
                ],
            )

            failure = guard.sample()

            self.assertIsNone(failure)
            self.assertEqual(guard.observations[-1].owned_bytes, 32)
            guard.close()

    def test_measurement_stays_on_the_opened_root_after_path_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            owned = parent / "owned"
            owned.mkdir()
            (owned / "original").write_bytes(b"old")
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=1024, min_free_bytes=1),
                [MeterRoot(owned)],
            )
            moved = parent / "moved"
            owned.rename(moved)
            owned.mkdir()
            (owned / "replacement").write_bytes(b"x" * 100)

            failure = guard.sample()

            self.assertIsNone(failure)
            self.assertEqual(guard.observations[-1].owned_bytes, 3)
            guard.close()

    def test_capacity_only_exact_path_replacement_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            cargo_home = parent / "cargo-home"
            cargo_home.mkdir()
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=1024, min_free_bytes=1),
                [
                    MeterRoot(
                        cargo_home,
                        charge_owned_bytes=False,
                        enforcement="capacity_only:cargo_home",
                        exact_path=cargo_home,
                    )
                ],
            )
            cargo_home.rename(parent / "old-cargo-home")
            cargo_home.mkdir()

            failure = guard.sample()

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
            guard.close()

    def test_capacity_only_exact_path_disappearance_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            cargo_home = parent / "cargo-home"
            cargo_home.mkdir()
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=1024, min_free_bytes=1),
                [
                    MeterRoot(
                        cargo_home,
                        charge_owned_bytes=False,
                        enforcement="capacity_only:cargo_home",
                        exact_path=cargo_home,
                    )
                ],
            )
            self.assertIsNone(guard.sample())
            cargo_home.rename(parent / "removed-cargo-home")

            failure = guard.sample()

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
            guard.close()

    def test_measurement_failure_stops_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "missing"
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=1024, min_free_bytes=1),
                [MeterRoot(missing)],
            )
            failure = guard.sample()
            assert failure is not None
            self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)

    def test_backend_neutral_preflight_stops_before_external_launch(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        root.add(
            "payload",
            self._node(kind=EntryKind.REGULAR, logical_size=2),
        )
        guard, _backend = self._recording_guard(
            path,
            root,
            max_disk_bytes=2,
        )
        self.addCleanup(guard.close)
        launch = mock.Mock()

        failure = guard.sample()
        if failure is None:
            launch()

        self.assertIsNotNone(failure)
        assert failure is not None
        self.assertEqual(
            failure.reason,
            DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
        )
        launch.assert_not_called()

    @unittest.skipIf(
        os.name == "nt",
        "Task 8-10 RunStore activation is intentionally unmigrated on Windows",
    )
    def test_preflight_stop_happens_before_child_launch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            owned = root / "owned"
            owned.mkdir()
            (owned / "payload").write_bytes(b"xx")
            output = root / "output"
            output.mkdir()
            store = RunStore(output)
            store.commands.mkdir()
            guard = DiskGuard(
                DiskPolicy(max_disk_bytes=2, min_free_bytes=1),
                [MeterRoot(owned)],
            )

            with self.assertRaises(CommandDiskStopped) as caught:
                CommandRunner(store).run(
                    [str(root / "must-not-exist")],
                    cwd=root,
                    timeout=1.0,
                    label="preflight",
                    disk_guard=guard,
                )

            self.assertEqual(
                caught.exception.record.disk_stop_code,
                "workspace.size.exceeded",
            )
            self.assertFalse((root / "must-not-exist").exists())

    def test_backend_neutral_periodic_sample_records_size_stop(self) -> None:
        path = Path("C:/recorded/owned")
        root = self._node()
        backend = _RecordingFilesystemBackend()
        backend.available[root.filesystem] = 1_000_000
        backend.units[root.filesystem] = 1
        retained = backend.directory_capability(root, path)
        with mock.patch(
            "tools.focused_mutation_support.disk.canonical_scratch_root",
            return_value=path,
        ):
            policy = DiskPolicy(
                max_disk_bytes=2,
                min_free_bytes=1,
                sample_interval_seconds=0.001,
                scratch_root=path,
            )
        guard = DiskGuard(
            policy,
            [MeterRoot(path, capability_factory=lambda: retained)],
            backend=backend,
        )
        guard.start()
        root.add(
            "payload",
            self._node(kind=EntryKind.REGULAR, logical_size=2),
        )
        deadline = time.monotonic() + 1.0
        while guard.failure is None and time.monotonic() < deadline:
            threading.Event().wait(0.001)

        self.assertIsNotNone(guard.failure)
        assert guard.failure is not None
        self.assertEqual(
            guard.failure.reason,
            DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
        )
        self.assertTrue(guard.stop_and_join(1.0))
        self.assertTrue(retained.closed)

    @unittest.skipIf(
        os.name == "nt",
        "Task 8-10 RunStore activation is intentionally unmigrated on Windows",
    )
    def test_periodic_size_stop_terminates_and_reaps_the_command(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            owned = root / "owned"
            owned.mkdir()
            output = root / "output"
            output.mkdir()
            store = RunStore(output)
            store.commands.mkdir()
            script = root / "grow.py"
            script.write_text(
                "from pathlib import Path\n"
                "import sys, time\n"
                "Path(sys.argv[1]).write_bytes(b'x' * 4096)\n"
                "time.sleep(30)\n",
                encoding="utf-8",
            )
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1024,
                    min_free_bytes=1,
                    sample_interval_seconds=0.01,
                ),
                [MeterRoot(owned)],
            )
            guard.start()
            self.addCleanup(guard.stop_and_join, 2.0)

            with self.assertRaises(CommandDiskStopped) as caught:
                CommandRunner(store).run(
                    [sys.executable, str(script), str(owned / "payload")],
                    cwd=root,
                    timeout=5.0,
                    label="periodic-stop",
                    disk_guard=guard,
                )

            self.assertEqual(
                caught.exception.failure.reason,
                DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
            )
            self.assertIsNotNone(caught.exception.record.exit_code)


class ManagedScratchTests(unittest.TestCase):
    def _task7_backend(
        self, *, rename_requires_closed_descendants: bool = False
    ) -> _ManagedRecordingBackend:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        backend = _ManagedRecordingBackend(
            Path(temporary.name).resolve(),
            rename_requires_closed_descendants=(
                rename_requires_closed_descendants
            ),
        )
        self.addCleanup(backend.close_backings)
        return backend

    def _task7_create(
        self,
        backend: _ManagedRecordingBackend,
        *,
        run_id: str,
    ) -> ManagedScratch:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_open_coordinator"],
        )
        real_open_coordinator = lease_module._open_coordinator

        def observe_coordinator(*args: object, **kwargs: object) -> LeaseLock:
            coordinator = real_open_coordinator(*args, **kwargs)
            backend.coordinator = coordinator
            return coordinator

        with (
            mock.patch.object(
                lease_module, "reclaim_abandoned", return_value=[]
            ),
            mock.patch.object(
                lease_module,
                "_open_coordinator",
                side_effect=observe_coordinator,
            ),
        ):
            scratch = ManagedScratch.create(
                backend.parent_path,
                run_id=run_id,
                backend=backend,
            )
        self.addCleanup(scratch.close_capabilities)
        return scratch

    def _task8_backend(
        self, *, rename_requires_closed_descendants: bool = False
    ) -> _Task8ManagedRecordingBackend:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        backend = _Task8ManagedRecordingBackend(
            Path(temporary.name).resolve(),
            rename_requires_closed_descendants=(
                rename_requires_closed_descendants
            ),
        )
        self.addCleanup(backend.close_backings)
        return backend

    def _task8_create(
        self,
        backend: _Task8ManagedRecordingBackend,
        *,
        run_id: str = "00000000-0000-4000-8000-000000000801",
    ) -> ManagedScratch:
        scratch = self._task7_create(backend, run_id=run_id)
        backend.cleanup_operations.clear()
        backend.close_counts.clear()
        backend.track_directory_resources = True
        backend.max_directory_resources = sum(
            1
            for resource in backend.live_resources
            if not resource.closed
            and resource.node.kind is EntryKind.DIRECTORY
        )
        self.addCleanup(
            setattr,
            backend,
            "after_cleanup_operation",
            lambda _operation: None,
        )
        self.addCleanup(
            setattr,
            backend,
            "before_cleanup_operation",
            lambda _operation: None,
        )
        return scratch

    @staticmethod
    def _task8_root_node(
        backend: _Task8ManagedRecordingBackend,
        scratch: ManagedScratch,
    ) -> _ManagedRecordedNode:
        root = scratch._root
        assert root is not None
        return backend._resource(root).node

    @staticmethod
    def _task8_add_payload(
        backend: _Task8ManagedRecordingBackend,
        parent: _ManagedRecordedNode,
        name: str,
        *,
        kind: EntryKind = EntryKind.REGULAR,
    ) -> _ManagedRecordedNode:
        return backend._new_node(
            kind,
            SecurityDomain.MANAGED,
            parent=parent,
            name=name,
        )

    @staticmethod
    def _task8_cleanup(
        scratch: ManagedScratch,
        backend: _Task8ManagedRecordingBackend,
        *,
        time_budget: float = 60.0,
    ) -> ScratchCleanupRecord:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_DeadlineExceeded", "_open_coordinator"],
        )

        def observe_coordinator(*args: object, **kwargs: object) -> LeaseLock:
            del args
            timeout = float(cast(float, kwargs.get("timeout", 5.0)))
            deadline = cast(float | None, kwargs.get("deadline"))
            wait = timeout
            if deadline is not None:
                wait = min(wait, max(0.0, deadline - time.monotonic()))
            if not backend.coordinator_gate.acquire(timeout=wait):
                raise lease_module._DeadlineExceeded(
                    "managed coordinator lock timed out"
                )
            coordinator = _Task8CoordinatorLease(
                backend.coordinator_gate
            )
            backend.coordinator = coordinator
            return coordinator

        with mock.patch.object(
            lease_module,
            "_open_coordinator",
            side_effect=observe_coordinator,
        ):
            try:
                return scratch.cleanup(time_budget=time_budget)
            except BaseException:
                coordinator = backend.coordinator
                if coordinator is not None:
                    try:
                        coordinator.close()
                    except BaseException:
                        pass
                raise

    @staticmethod
    def _task8_native_cleanup(
        scratch: ManagedScratch,
        *,
        time_budget: float = 60.0,
    ) -> ScratchCleanupRecord:
        """Close a leaked coordinator only when an expected RED escapes."""
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_open_coordinator"],
        )
        delegate = lease_module._open_coordinator
        opened: list[LeaseLock] = []

        def track_coordinator(*args: object, **kwargs: object) -> LeaseLock:
            coordinator = delegate(*args, **kwargs)
            opened.append(coordinator)
            return coordinator

        with mock.patch.object(
            lease_module,
            "_open_coordinator",
            side_effect=track_coordinator,
        ):
            try:
                return scratch.cleanup(time_budget=time_budget)
            except BaseException:
                for coordinator in opened:
                    try:
                        coordinator.close()
                    except BaseException:
                        pass
                raise

    @staticmethod
    def _task8_cleanup_with_real_coordinator(
        scratch: ManagedScratch,
        backend: _Task8ManagedRecordingBackend,
        opened: list[LeaseLock],
        *,
        time_budget: float = 60.0,
    ) -> ScratchCleanupRecord:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_open_coordinator"],
        )
        delegate = lease_module._open_coordinator

        def track_coordinator(*args: object, **kwargs: object) -> LeaseLock:
            coordinator = delegate(*args, **kwargs)
            opened.append(coordinator)
            backend.coordinator = coordinator
            return coordinator

        with mock.patch.object(
            lease_module,
            "_open_coordinator",
            side_effect=track_coordinator,
        ):
            return scratch.cleanup(time_budget=time_budget)

    @staticmethod
    def _task8_same_identity_replacement(
        backend: _Task8ManagedRecordingBackend,
        original: _ManagedRecordedNode,
        *,
        kind: EntryKind | None = None,
    ) -> _ManagedRecordedNode:
        parent = original.parent
        if parent is None:
            raise AssertionError("replacement target has no parent")
        name = original.name
        replacement = backend._new_node(
            original.kind if kind is None else kind,
            SecurityDomain.MANAGED,
            parent=parent,
            name=name,
        )
        replacement.identity = original.identity
        return replacement

    @staticmethod
    def _task7_managed_root(
        backend: _ManagedRecordingBackend,
    ) -> DirectoryCapability:
        parent = backend.open_root(
            backend.parent_path, SharePolicy.MUTATION
        )
        managed = backend.create_secure_root(parent, "hoimin-focused-v1")
        parent.close()
        return managed

    @staticmethod
    def _task9_add_marker(
        backend: _ManagedRecordingBackend,
        root: _ManagedRecordedNode,
        name: str,
        run_id: str,
        lease_id: str,
        *,
        value: dict[str, object] | None = None,
        modified_ns: int = 0,
    ) -> _ManagedRecordedNode:
        marker = backend._new_node(
            EntryKind.REGULAR,
            SecurityDomain.MANAGED,
            parent=root,
            name=name,
            modified_ns=modified_ns,
        )
        assert marker.backing is not None
        encoded = (
            json.dumps(
                {
                    "schema_version": 1,
                    "run_id": run_id,
                    "owner_kind": "focused_python",
                    "lease_id": lease_id,
                }
                if value is None
                else value,
                sort_keys=True,
            )
            + "\n"
        ).encode("utf-8")
        marker.backing.seek(0)
        marker.backing.truncate()
        marker.backing.write(encoded)
        marker.backing.flush()
        return marker

    def _task9_add_candidate(
        self,
        backend: _ManagedRecordingBackend,
        managed: _ManagedRecordedNode,
        *,
        run_id: str,
        prefix: str = "run-",
        lease: bool = True,
        heartbeat: bool = True,
        ready: bool = False,
        retained: bool = False,
        modified_ns: int = 0,
    ) -> _ManagedRecordedNode:
        lease_id = str(uuid.UUID(int=int(uuid.UUID(run_id)) + 10_000))
        root = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=managed,
            name=f"{prefix}{run_id}",
            modified_ns=modified_ns,
        )
        if lease:
            self._task9_add_marker(
                backend, root, ".hoimin-lease.json", run_id, lease_id
            )
        if heartbeat:
            self._task9_add_marker(
                backend,
                root,
                ".hoimin-heartbeat.json",
                run_id,
                lease_id,
                modified_ns=modified_ns,
            )
        if ready:
            self._task9_add_marker(
                backend,
                root,
                ".hoimin-cleanup-ready.json",
                run_id,
                lease_id,
            )
        if retained:
            self._task9_add_marker(
                backend, root, ".hoimin-retain.json", run_id, lease_id
            )
        return root

    def _task9_empty_candidate(
        self,
        run_id: str,
    ) -> tuple[
        _Task8ManagedRecordingBackend,
        DirectoryCapability,
        _ManagedRecordedNode,
        DirectoryCapability,
        object,
    ]:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_JanitorCandidate"],
        )
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        candidate = self._task9_add_candidate(
            backend,
            managed,
            run_id=run_id,
            prefix=".staging-",
            lease=False,
            heartbeat=False,
            modified_ns=0,
        )
        candidate_capability = backend.open_directory(
            managed_capability, candidate.name, SharePolicy.PINNED
        )
        selected = lease_module._JanitorCandidate(
            candidate.name,
            candidate.identity,
            candidate.filesystem,
            run_id,
            candidate.modified_ns,
        )
        return (
            backend,
            managed_capability,
            candidate,
            candidate_capability,
            selected,
        )

    def test_exact_absolute_deadline_is_expired(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_check_absolute_deadline"],
        )

        with self.assertRaisesRegex(TimeoutError, "exact deadline"):
            lease_module._check_absolute_deadline(
                5.0, lambda: 5.0, "exact deadline"
            )

    def test_managed_root_stops_between_filesystem_identity_queries(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_ensure_managed_root"],
        )
        backend = self._task7_backend()
        clock = [0.0]

        def cross_after_root_creation(event: str) -> None:
            if event == "create-secure-root:hoimin-focused-v1":
                clock[0] = 6.0

        backend.after_event = cross_after_root_creation
        with self.assertRaisesRegex(TimeoutError, "managed root deadline"):
            lease_module._ensure_managed_root(
                backend.parent_path,
                backend=backend,
                deadline=5.0,
                monotonic=lambda: clock[0],
            )

        self.assertNotIn(
            "verify-managed:hoimin-focused-v1:repair=true", backend.events
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_directory_reopen_stops_between_identity_queries(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        candidate = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000126",
            lease=False,
            heartbeat=False,
        )
        candidate_capability = backend.open_directory(
            managed_capability, candidate.name, SharePolicy.PINNED
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_bounded_candidate_entries"],
        )
        crossed = False
        real_open = backend.open_directory

        def crossing_open(
            parent: DirectoryCapability,
            name: str,
            share_policy: SharePolicy,
        ) -> DirectoryCapability:
            nonlocal crossed
            opened = real_open(parent, name, share_policy)
            crossed = True
            return opened

        with (
            mock.patch.object(
                backend, "open_directory", side_effect=crossing_open
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: 6.0 if crossed else 0.0,
            ),
            self.assertRaisesRegex(TimeoutError, "bounded directory scan"),
        ):
            lease_module._bounded_candidate_entries(
                managed_capability,
                candidate_capability,
                candidate.name,
                backend,
                deadline=5.0,
            )

        self.assertFalse(
            any(event.startswith("entry:") for event in backend.events[-2:])
        )
        candidate_capability.close()
        managed_capability.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_active_report_path_is_validated_before_marker_publication(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_write_marker_at"],
            )
            real_validate = lease_module.validate_reported_path
            real_write_marker = lease_module._write_marker_at
            real_close_coordinator = (
                lease_module._close_locked_coordinator_once
            )
            coordinator_closes: list[str] = []

            def reject_active(path: Path) -> str:
                if path.name.startswith("run-"):
                    raise ValueError("injected active report path rejection")
                return real_validate(path)

            def observe_close(lock: LeaseLock, label: str) -> tuple[str, ...]:
                coordinator_closes.append(label)
                return real_close_coordinator(lock, label)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.validate_reported_path",
                    side_effect=reject_active,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._write_marker_at",
                    wraps=real_write_marker,
                ) as write_marker,
                mock.patch(
                    "tools.focused_mutation_support.lease."
                    "_close_locked_coordinator_once",
                    side_effect=observe_close,
                ),
                self.assertRaisesRegex(ValueError, "active report path"),
            ):
                ManagedScratch.create(Path(directory))

            write_marker.assert_not_called()
            self.assertIn("managed coordinator", coordinator_closes)

    def test_deleting_report_path_is_validated_before_marker_publication(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_write_marker_at"],
            )
            real_validate = lease_module.validate_reported_path

            def reject_deleting(path: Path) -> str:
                if path.name.startswith(".deleting-"):
                    raise ValueError("injected deleting report path rejection")
                return real_validate(path)

            with (
                mock.patch.object(
                    lease_module,
                    "validate_reported_path",
                    side_effect=reject_deleting,
                ),
                mock.patch.object(
                    lease_module,
                    "_write_marker_at",
                    side_effect=AssertionError("marker published before validation"),
                ),
                self.assertRaisesRegex(ValueError, "deleting report path"),
            ):
                ManagedScratch.create(Path(directory))

    def test_child_report_path_is_validated_before_directory_creation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            self.addCleanup(scratch.close_capabilities)
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["validate_reported_path"],
            )
            real_validate = lease_module.validate_reported_path

            def reject_child(path: Path) -> str:
                if path.name == "candidate-0001":
                    raise ValueError("injected child report path rejection")
                return real_validate(path)

            with (
                mock.patch.object(
                    lease_module,
                    "validate_reported_path",
                    side_effect=reject_child,
                ),
                self.assertRaisesRegex(ValueError, "child report path"),
            ):
                scratch.create_child("candidate-0001")

            self.assertFalse((scratch.path / "candidate-0001").exists())
            self.assertEqual(scratch.close_capabilities(), ())

    def test_cleanup_validates_deleting_report_path_before_rename(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            real_validate = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["validate_reported_path"],
            ).validate_reported_path

            def reject_deleting(path: Path) -> str:
                if path.name.startswith(".deleting-"):
                    raise ValueError("injected deleting report path rejection")
                return real_validate(path)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.validate_reported_path",
                    side_effect=reject_deleting,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.rename",
                    wraps=os.rename,
                ) as rename,
                self.assertRaisesRegex(ValueError, "deleting report path"),
            ):
                scratch.cleanup()

            rename.assert_not_called()
            self.assertTrue(scratch.path.is_dir())
            scratch.close_capabilities()

    def test_reported_path_rejects_oversized_markdown_shell_encoding(
        self,
    ) -> None:
        with mock.patch(
            "tools.focused_mutation_support.lease.shlex.quote",
            return_value="x" * (16 * 1024 + 1),
            create=True,
        ):
            with self.assertRaisesRegex(ValueError, "escaped"):
                validate_reported_path(Path("/tmp/safe"))

    def test_reported_path_rejects_markdown_line_breaks(self) -> None:
        with self.assertRaisesRegex(ValueError, "Markdown"):
            validate_reported_path(Path("/tmp/unsafe\npath"))

    def test_coordinator_validation_preserves_primary_and_attempts_fd_close(self) -> None:
        backend = self._task7_backend()
        managed = self._task7_managed_root(backend)
        with (
            mock.patch(
                "tools.focused_mutation_support.lease._initialize_or_validate_coordinator",
                side_effect=ValueError("injected validation failure"),
            ),
            mock.patch.object(
                LeaseLock,
                "release",
                side_effect=OSError("injected unlock failure"),
            ),
            self.assertRaisesRegex(
                ValueError, "validation failure"
            ) as caught,
        ):
            _open_coordinator(managed, backend)

        self.assertTrue(
            any("unlock failed" in note for note in caught.exception.__notes__)
        )
        managed.close()
    def test_lease_close_always_attempts_fd_close_after_unlock_failure(self) -> None:
        with tempfile.TemporaryFile() as stream:
            descriptor = os.dup(stream.fileno())
            lease = LeaseLock(descriptor)
            lease.locked = True
            with (
                mock.patch.object(
                    lease,
                    "release",
                    side_effect=OSError("injected unlock failure"),
                ),
                self.assertRaisesRegex(OSError, "unlock failure"),
            ):
                lease.close()

            self.assertEqual(lease.fd, -1)
            with self.assertRaises(OSError):
                os.fstat(descriptor)

    def test_descriptor_disposal_attempts_every_close(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_descriptors_all"],
        )
        first_read, first_write = os.pipe()
        second_read, second_write = os.pipe()
        real_close = os.close
        attempted: list[int] = []

        def close_with_first_failure(descriptor: int) -> None:
            attempted.append(descriptor)
            real_close(descriptor)
            if descriptor == first_read:
                raise OSError("injected first descriptor close failure")

        try:
            with mock.patch(
                "tools.focused_mutation_support.lease.os.close",
                side_effect=close_with_first_failure,
            ):
                errors = lease_module._close_descriptors_all(
                    (("root", first_read), ("managed root", second_read))
                )

            self.assertEqual(attempted, [first_read, second_read])
            self.assertIn("first descriptor close failure", "; ".join(errors))
        finally:
            for descriptor in (first_read, second_read, first_write, second_write):
                try:
                    real_close(descriptor)
                except OSError:
                    pass

    def test_deferred_timeout_keeps_primary_when_capability_close_fails(
        self,
    ) -> None:
        backend = self._task8_backend()
        selection_root = self._task7_managed_root(backend)
        managed = backend._resource(selection_root).node
        run_id = "00000000-0000-4000-8000-000000000118"
        deleting = self._task9_add_candidate(
            backend,
            managed,
            run_id=run_id,
            prefix=".deleting-",
            ready=True,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_resume_deferred_cleanup"],
        )
        crossed = False
        real_open = backend.open_directory

        def cross_after_root_open(
            parent: DirectoryCapability,
            name: str,
            share_policy: SharePolicy,
        ) -> DirectoryCapability:
            nonlocal crossed
            capability = real_open(parent, name, share_policy)
            if name == deleting.name:
                crossed = True
                backend._resource(capability).close_failures = 1
            return capability

        with (
            mock.patch.object(
                backend,
                "open_directory",
                side_effect=cross_after_root_open,
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: 31.0 if crossed else 0.0,
            ),
        ):
            result = lease_module._resume_deferred_cleanup(
                selection_root.path_hint,
                selection_root,
                deleting.name,
                deleting.identity,
                deleting.filesystem,
                backend,
                deadline=30.0,
            )

        self.assertIsInstance(result, JanitorDiagnostic)
        assert isinstance(result, JanitorDiagnostic)
        detail = "; ".join(result.details)
        self.assertIn("deferred janitor", detail)
        self.assertIn("injected close failure", detail)
        selection_root.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_lease_close_attempts_fd_close_after_unlock_failure(self) -> None:
        with tempfile.TemporaryFile() as stream:
            descriptor = os.dup(stream.fileno())
            lease = LeaseLock(descriptor)
            lease.locked = True
            with mock.patch.object(
                lease,
                "release",
                side_effect=OSError("injected unlock failure"),
            ):
                errors = _close_lease_lock_all(lease, "fixture lease")

            self.assertTrue(any("unlock failed" in item for item in errors))
            self.assertEqual(lease.fd, -1)
            with self.assertRaises(OSError):
                os.fstat(descriptor)

    def test_janitor_diagnostics_are_bounded_with_omitted_count(self) -> None:
        records: list[ScratchCleanupRecord | JanitorDiagnostic] = [
            ScratchCleanupRecord(
                ScratchCleanupStatus.FAILED,
                0,
                0,
                tuple(f"failure-{index}-{detail}" for detail in range(3)),
            )
            for index in range(128)
        ]

        bounded = _bound_cleanup_records(records)

        self.assertEqual(len(bounded), 128)
        self.assertEqual(sum(len(item.details) for item in bounded), 256)
        self.assertEqual(
            sum(item.omitted_detail_count for item in bounded), 128
        )

    def test_bootstrap_swap_cannot_chmod_or_open_replacement_target(self) -> None:
        backend = self._task7_backend()
        original = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=backend.parent,
            name="hoimin-focused-v1",
        )
        replacement: _ManagedRecordedNode | None = None

        def swap_after_security(event: str) -> None:
            nonlocal replacement
            if event != "verify-managed:hoimin-focused-v1:repair=true":
                return
            backend.parent.children.pop("hoimin-focused-v1")
            original.name = "managed-original"
            backend.parent.children[original.name] = original
            replacement = backend._new_node(
                EntryKind.DIRECTORY,
                SecurityDomain.MANAGED,
                parent=backend.parent,
                name="hoimin-focused-v1",
            )

        backend.after_event = swap_after_security
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_ensure_managed_root"],
        )
        with self.assertRaisesRegex(OSError, "identity changed"):
            lease_module._ensure_managed_root(
                backend.parent_path, backend=backend
            )

        self.assertIs(
            backend.parent.children["hoimin-focused-v1"], replacement
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_janitor_keeps_selected_root_capability_after_path_replacement(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        original = backend._resource(managed_capability).node
        candidate = self._task9_add_candidate(
            backend,
            original,
            run_id="00000000-0000-4000-8000-000000000701",
            ready=True,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_persist_coordinator_cursor"],
        )
        real_persist = lease_module._persist_coordinator_cursor
        replacement: _ManagedRecordedNode | None = None

        def replace_after_selection(*args: object, **kwargs: object) -> None:
            nonlocal replacement
            real_persist(*args, **kwargs)
            backend.parent.children.pop("hoimin-focused-v1")
            original.name = "managed-original"
            backend.parent.children[original.name] = original
            replacement = backend._new_node(
                EntryKind.DIRECTORY,
                SecurityDomain.MANAGED,
                parent=backend.parent,
                name="hoimin-focused-v1",
            )
            backend._new_node(
                EntryKind.REGULAR,
                SecurityDomain.MANAGED,
                parent=replacement,
                name="sentinel",
            )

        with mock.patch.object(
            lease_module,
            "_persist_coordinator_cursor",
            side_effect=replace_after_selection,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertIsNotNone(replacement)
        assert replacement is not None
        self.assertIn("sentinel", replacement.children)
        self.assertNotIn(candidate.name, original.children)
        self.assertTrue(
            any(
                item.status is ScratchCleanupStatus.CLEAN
                for item in _cleanup_records_only(records)
            )
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_zero_progress_lease_marker_write_rolls_back_staging(self) -> None:
        backend = self._task7_backend()
        managed = self._task7_managed_root(backend)
        coordinator = _open_coordinator(managed, backend)
        coordinator.close()
        managed.close()
        with (
            mock.patch(
                "tools.focused_mutation_support.lease.os.write",
                return_value=0,
            ),
            self.assertRaisesRegex(OSError, "made no progress"),
        ):
            ManagedScratch.create(
                backend.parent_path,
                run_id="00000000-0000-4000-8000-000000000716",
                backend=backend,
            )

        managed_node = backend.parent.children["hoimin-focused-v1"]
        self.assertEqual(
            set(managed_node.children), {".hoimin-coordinator"}
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_report_freeze_blocks_child_registration_and_detects_later_change(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))

            with scratch.freeze_registry() as generation:
                self.assertTrue(scratch.registry_generation_is(generation))
                with self.assertRaisesRegex(RuntimeError, "registry is frozen"):
                    scratch.create_child("candidate-0001")

            child = scratch.create_child("candidate-0001")
            self.assertFalse(scratch.registry_generation_is(generation))
            self.assertEqual(child, scratch.path / "candidate-0001")
            self.assertEqual(scratch.close_capabilities(), ())

    def test_coordinator_uses_fixed_dual_crc_slots(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            encoded = (scratch.managed_root / ".hoimin-coordinator").read_bytes()

            self.assertEqual(len(encoded), 1_025)
            self.assertEqual(encoded[0], 0)
            self.assertEqual(encoded[1:513], encoded[513:1025])
            slot = encoded[1:513]
            self.assertEqual(slot[:8], b"HMCUR001")
            self.assertEqual(int.from_bytes(slot[8:12], "little"), 1)
            self.assertEqual(
                int.from_bytes(slot[508:512], "little"),
                zlib.crc32(slot[:508]) & 0xFFFF_FFFF,
            )
            self.assertEqual(scratch.close_capabilities(), ())

    def test_coordinator_contention_has_a_bounded_deadline(self) -> None:
        backend = self._task7_backend()
        managed = self._task7_managed_root(backend)
        with (
            mock.patch(
                "tools.focused_mutation_support.lease.LeaseLock.acquire",
                side_effect=BlockingIOError("busy"),
            ),
            self.assertRaisesRegex(TimeoutError, "coordinator lock"),
        ):
            _open_coordinator(managed, backend, timeout=0.01)
        managed.close()

    def test_coordinator_uses_shorter_timeout_than_total_deadline(self) -> None:
        backend = self._task7_backend()
        managed = self._task7_managed_root(backend)
        now = 0.0

        def clock() -> float:
            return now

        def advance(seconds: float) -> None:
            nonlocal now
            now += seconds

        with (
            mock.patch(
                "tools.focused_mutation_support.lease.LeaseLock.acquire",
                side_effect=BlockingIOError("busy"),
            ),
            self.assertRaisesRegex(TimeoutError, "coordinator lock"),
        ):
            _open_coordinator(
                managed,
                backend,
                timeout=0.02,
                deadline=0.2,
                monotonic=clock,
                sleep=advance,
            )

        self.assertLessEqual(now, 0.021)
        managed.close()

    def test_coordinator_does_not_initialize_after_absolute_deadline(self) -> None:
        backend = self._task7_backend()
        managed = self._task7_managed_root(backend)
        with self.assertRaisesRegex(TimeoutError, "deadline"):
            _open_coordinator(
                managed,
                backend,
                deadline=5.0,
                monotonic=mock.Mock(side_effect=[0.0, 6.0]),
            )
        self.assertNotIn(
            "verify-managed:.hoimin-coordinator:repair=true",
            backend.events,
        )
        managed.close()

    def test_coordinator_parent_close_failure_also_closes_coordinator_fd(
        self,
    ) -> None:
        backend = self._task7_backend()
        managed = self._task7_managed_root(backend)

        def fail_after_open(event: str) -> None:
            if event == "verify-managed:.hoimin-coordinator:repair=true":
                resource = next(
                    resource
                    for resource in backend.live_resources
                    if resource.node.name == ".hoimin-coordinator"
                )
                resource.close_failures = 1
                raise OSError("injected coordinator verification failure")

        backend.after_event = fail_after_open
        with self.assertRaisesRegex(OSError, "verification failure") as caught:
            _open_coordinator(managed, backend)

        self.assertTrue(
            any("close failed" in note for note in caught.exception.__notes__)
        )
        self.assertEqual(len(backend.live_resources), 1)
        managed.close()

    def test_managed_root_bootstrap_stops_after_mkdir_crosses_deadline(
        self,
    ) -> None:
        backend = self._task7_backend()
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_ensure_managed_root"],
        )
        clock = [0.0]
        backend.after_event = lambda event: clock.__setitem__(
            0, 6.0
        ) if event.startswith("create-secure-root:") else None
        with self.assertRaisesRegex(TimeoutError, "deadline"):
            lease_module._ensure_managed_root(
                backend.parent_path,
                backend=backend,
                deadline=5.0,
                monotonic=lambda: clock[0],
            )
        self.assertFalse(
            any(event.startswith("verify-managed:") for event in backend.events)
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_selection_timeout_keeps_descriptor_close_failure_secondary(
        self,
    ) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        coordinator = _open_coordinator(managed_capability, backend)
        coordinator.close()
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_read_coordinator_state", "_close_capability_retry"],
        )
        real_read_state = lease_module._read_coordinator_state
        real_close_retry = lease_module._close_capability_retry
        real_close_lock_retry = lease_module._close_lease_lock_retry
        read_calls = 0

        def fail_second_read(*args: object, **kwargs: object) -> object:
            nonlocal read_calls
            read_calls += 1
            if read_calls == 2:
                raise lease_module._DeadlineExceeded(
                    "injected selection deadline"
                )
            return real_read_state(*args, **kwargs)

        def close_with_secondary(
            capability: DirectoryCapability | FileCapability,
            label: str,
        ) -> tuple[str, ...]:
            errors = real_close_retry(capability, label)
            if label == "janitor managed root":
                return (*errors, "injected selection root close failure")
            return errors

        def close_lock_with_secondary(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            errors = real_close_lock_retry(lock, label)
            if label == "janitor selection coordinator":
                return (*errors, "injected selection coordinator close failure")
            return errors

        with (
            mock.patch.object(
                lease_module,
                "_read_coordinator_state",
                side_effect=fail_second_read,
            ),
            mock.patch.object(
                lease_module,
                "_close_capability_retry",
                side_effect=close_with_secondary,
            ),
            mock.patch.object(
                lease_module,
                "_close_lease_lock_retry",
                side_effect=close_lock_with_secondary,
            ),
            self.assertRaisesRegex(
                TimeoutError, "injected selection deadline"
            ) as caught,
        ):
            reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertEqual(read_calls, 2)
        notes = getattr(caught.exception, "__notes__", ())
        self.assertEqual(
            sum("selection coordinator close failure" in note for note in notes),
            1,
        )
        self.assertEqual(
            sum("selection root close failure" in note for note in notes),
            1,
        )
        self.assertLess(
            next(
                index
                for index, note in enumerate(notes)
                if "selection coordinator close failure" in note
            ),
            next(
                index
                for index, note in enumerate(notes)
                if "selection root close failure" in note
            ),
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_candidate_deadline_keeps_lease_close_failure_secondary(
        self,
    ) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000109",
            ready=True,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_read_locked_marker", "_close_lease_lock_retry"],
        )
        real_close_retry = lease_module._close_lease_lock_retry

        def close_with_secondary(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            errors = real_close_retry(lock, label)
            if label == "janitor candidate lease":
                return (*errors, "injected candidate lease close failure")
            return errors

        with (
            mock.patch.object(
                lease_module,
                "_read_locked_marker",
                side_effect=lease_module._DeadlineExceeded(
                    "injected candidate deadline"
                ),
            ),
            mock.patch.object(
                lease_module,
                "_close_lease_lock_retry",
                side_effect=close_with_secondary,
            ),
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        detail = "; ".join(
            item for record in records for item in record.details
        )
        self.assertIn("injected candidate deadline", detail)
        self.assertIn("injected candidate lease close failure", detail)
        self.assertEqual(len(backend.live_resources), 0)

    def test_invalid_run_id_releases_coordinator_lock(self) -> None:
        backend = self._task7_backend()
        with self.assertRaises(ValueError):
            ManagedScratch.create(
                backend.parent_path,
                run_id="not-a-uuid",
                backend=backend,
            )

        managed = self._task7_managed_root(backend)
        coordinator = _open_coordinator(
            managed,
            backend,
            timeout=0.01,
        )
        coordinator.close()
        managed.close()

    def test_invalid_run_id_preserves_managed_root_close_secondary(self) -> None:
        backend = self._task7_backend()
        backend.failures["close:hoimin-focused-v1"] = OSError(
            "injected managed root close failure"
        )
        with (
            mock.patch(
                "tools.focused_mutation_support.lease.reclaim_abandoned",
                return_value=[],
            ),
            self.assertRaises(ValueError) as caught,
        ):
            ManagedScratch.create(
                backend.parent_path,
                run_id="not-a-uuid",
                backend=backend,
            )

        self.assertTrue(
            any(
                "managed root close failure" in note
                for note in getattr(caught.exception, "__notes__", ())
            )
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_constructor_failure_after_lease_publication_rolls_back_staging(self) -> None:
        backend = self._task7_backend()
        run_id = "00000000-0000-4000-8000-000000000010"
        backend.failures[
            "create_new:.hoimin-heartbeat.json:read_write:pinned"
        ] = OSError("injected heartbeat failure")
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["reclaim_abandoned"],
        )

        with (
            mock.patch.object(
                lease_module, "reclaim_abandoned", return_value=[]
            ),
            self.assertRaisesRegex(OSError, "heartbeat failure"),
        ):
            ManagedScratch.create(
                backend.parent_path,
                run_id=run_id,
                backend=backend,
            )

        managed = backend.parent.children["hoimin-focused-v1"]
        self.assertEqual(set(managed.children), {".hoimin-coordinator"})
        self.assertEqual(len(backend.live_resources), 0)

    def test_constructor_preserves_preexisting_empty_staging(self) -> None:
        backend = self._task7_backend()
        managed = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=backend.parent,
            name="hoimin-focused-v1",
        )
        run_id = "00000000-0000-4000-8000-000000000113"
        staging_name = f".staging-{run_id}"
        staging = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=managed,
            name=staging_name,
            modified_ns=time.time_ns(),
        )

        with self.assertRaises(FileExistsError):
            ManagedScratch.create(
                backend.parent_path,
                run_id=run_id,
                backend=backend,
            )

        self.assertIs(managed.children[staging_name], staging)
        self.assertEqual(len(backend.live_resources), 0)

    def test_constructor_never_replaces_preexisting_empty_active(self) -> None:
        backend = self._task7_backend()
        managed = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=backend.parent,
            name="hoimin-focused-v1",
        )
        run_id = "00000000-0000-4000-8000-000000000114"
        active_name = f"run-{run_id}"
        active = backend._new_node(
            EntryKind.DIRECTORY,
            SecurityDomain.MANAGED,
            parent=managed,
            name=active_name,
        )
        identity = active.identity
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["reclaim_abandoned"],
        )

        with (
            mock.patch.object(
                lease_module, "reclaim_abandoned", return_value=[]
            ),
            self.assertRaises(FileExistsError),
        ):
            ManagedScratch.create(
                backend.parent_path,
                run_id=run_id,
                backend=backend,
            )

        self.assertEqual(managed.children[active_name].identity, identity)
        self.assertNotIn(f".staging-{run_id}", managed.children)
        self.assertFalse(any(event.startswith("rename:") for event in backend.events))
        self.assertEqual(len(backend.live_resources), 0)

    def test_coordinator_close_failure_after_publish_rolls_back_active_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_close_locked_coordinator_once"],
            )
            real_close_coordinator = (
                lease_module._close_locked_coordinator_once
            )

            def fail_coordinator(lock: LeaseLock, label: str) -> tuple[str, ...]:
                errors = real_close_coordinator(lock, label)
                if label == "managed coordinator":
                    return (*errors, "injected coordinator close failure")
                return errors

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease."
                    "_close_locked_coordinator_once",
                    side_effect=fail_coordinator,
                ),
                self.assertRaisesRegex(OSError, "coordinator close failure"),
            ):
                ManagedScratch.create(
                    parent,
                    run_id="00000000-0000-4000-8000-000000000110",
                )

            managed = parent / "hoimin-focused-v1"
            self.assertEqual(
                sorted(path.name for path in managed.iterdir()),
                [".hoimin-coordinator"],
            )

    def test_startup_janitor_preserves_a_live_leased_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            first = ManagedScratch.create(parent)
            first_path = first.path
            second = ManagedScratch.create(parent)

            self.assertTrue(first_path.is_dir())
            first.mark_cleanup_ready()
            second.mark_cleanup_ready()
            self.assertEqual(first.cleanup().status, ScratchCleanupStatus.CLEAN)
            self.assertEqual(second.cleanup().status, ScratchCleanupStatus.CLEAN)

    def test_startup_janitor_reclaims_unlocked_cleanup_ready_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            scratch = ManagedScratch.create(parent)
            abandoned = scratch.path
            scratch.mark_cleanup_ready()
            scratch.__del__()

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(
                [record.status for record in _cleanup_records_only(records)],
                [ScratchCleanupStatus.CLEAN],
            )
            self.assertFalse(abandoned.exists())

    def test_janitor_reports_candidate_lease_disposal_failure(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000110",
            retained=True,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_lease_lock_retry"],
        )
        real_close_retry = lease_module._close_lease_lock_retry

        def fail_candidate_close(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            errors = real_close_retry(lock, label)
            if label == "janitor candidate lease":
                return (*errors, "injected candidate lease close failure")
            return errors

        with mock.patch.object(
            lease_module,
            "_close_lease_lock_retry",
            side_effect=fail_candidate_close,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertTrue(
            any(
                "candidate lease close failure" in "; ".join(record.details)
                for record in records
            )
        )

    def test_janitor_reports_transferred_lease_disposal_failure(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        root = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000111",
            ready=True,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_lease_lock_all"],
        )
        real_close_all = lease_module._close_lease_lock_all
        managed_lease_closes = 0
        deferred = ScratchCleanupRecord(
            ScratchCleanupStatus.DEFERRED,
            1,
            0,
            remaining_root=str(root.name),
        )

        def fail_transferred_close(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            nonlocal managed_lease_closes
            errors = real_close_all(lock, label)
            if label == "managed lease":
                managed_lease_closes += 1
                if managed_lease_closes == 1:
                    return (
                        *errors,
                        "injected transferred lease close failure",
                    )
            return errors

        with (
            mock.patch.object(
                ManagedScratch,
                "cleanup",
                return_value=deferred,
            ),
            mock.patch.object(
                lease_module,
                "_close_lease_lock_all",
                side_effect=fail_transferred_close,
            ),
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        cleanup_records = [
            item
            for item in records
            if isinstance(item, ScratchCleanupRecord)
        ]
        self.assertTrue(cleanup_records)
        self.assertTrue(
            any(
                "transferred lease close failure" in "; ".join(item.details)
                for item in cleanup_records
            )
        )
        self.assertEqual(cleanup_records[0].examined_entries, 2)

    def test_empty_janitor_stops_after_candidate_open_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        candidate = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000121",
            prefix=".staging-",
            lease=False,
            heartbeat=False,
        )
        crossed = False

        def cross_during_candidate_open(event: str) -> None:
            nonlocal crossed
            if event == f"open-directory:{candidate.name}:pinned":
                crossed = True

        backend.after_event = cross_during_candidate_open
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: 31.0 if crossed else 0.0,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                now=24 * 60 * 60 + 1.0,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertTrue(
            any(isinstance(item, JanitorDiagnostic) for item in records)
        )
        self.assertIn(candidate.name, managed.children)
        self.assertFalse(
            any(event == f"verify-managed:{candidate.name}:repair=false"
                for event in backend.events)
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_janitor_selection_lock_uses_only_remaining_deadline(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        observed_timeouts: list[float] = []
        clock_calls = 0

        def clock() -> float:
            nonlocal clock_calls
            clock_calls += 1
            return 0.0 if clock_calls == 1 else 4.0

        def stop_at_coordinator(
            _root: DirectoryCapability,
            _backend: FilesystemBackend,
            *,
            timeout: float = 5.0,
            **_kwargs: object,
        ) -> LeaseLock:
            observed_timeouts.append(timeout)
            raise TimeoutError("injected selection contention")

        with (
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=clock,
            ),
            mock.patch(
                "tools.focused_mutation_support.lease._open_coordinator",
                side_effect=stop_at_coordinator,
            ),
            self.assertRaisesRegex(TimeoutError, "selection contention"),
        ):
            reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertEqual(observed_timeouts, [1.0])
        self.assertEqual(len(backend.live_resources), 0)

    def test_cursor_persist_does_not_fsync_after_write_crosses_deadline(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_persist_coordinator_cursor", "_CoordinatorState"],
        )
        crossed = False

        def crossing_write(*_args: object, **_kwargs: object) -> int:
            nonlocal crossed
            crossed = True
            return 512

        with (
            mock.patch(
                "tools.focused_mutation_support.lease._write_at",
                side_effect=crossing_write,
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.os.fsync",
                side_effect=AssertionError("fsync started after janitor deadline"),
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: 31.0 if crossed else 0.0,
            ),
            self.assertRaisesRegex(TimeoutError, "janitor selection"),
        ):
            lease_module._persist_coordinator_cursor(
                123,
                lease_module._CoordinatorState(0, 0, ""),
                "run-00000000-0000-4000-8000-000000000121",
                deadline=30.0,
            )

    def test_janitor_rechecks_deadline_after_heartbeat_marker_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            scratch.mark_cleanup_ready()
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_read_valid_marker_at", "HEARTBEAT_FILE"],
            )
            real_read = lease_module._read_valid_marker_at
            real_stat = lease_module.os.stat
            crossed = False
            heartbeat_stat_started = False

            def crossing_read(
                root_fd: int,
                marker_name: str,
                *args: object,
                **kwargs: object,
            ) -> object:
                nonlocal crossed
                result = real_read(root_fd, marker_name, *args, **kwargs)
                if marker_name == lease_module.HEARTBEAT_FILE:
                    crossed = True
                return result

            def forbid_heartbeat_stat(
                path: object,
                *args: object,
                **kwargs: object,
            ) -> os.stat_result:
                nonlocal heartbeat_stat_started
                if path == lease_module.HEARTBEAT_FILE and crossed:
                    heartbeat_stat_started = True
                    raise AssertionError("heartbeat stat started after deadline")
                return real_stat(path, *args, **kwargs)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._read_valid_marker_at",
                    side_effect=crossing_read,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.stat",
                    side_effect=forbid_heartbeat_stat,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 31.0 if crossed else 0.0,
                ),
            ):
                records = reclaim_abandoned(managed)

            self.assertFalse(heartbeat_stat_started)
            self.assertTrue(
                any(
                    isinstance(item, JanitorDiagnostic)
                    and "deadline" in "; ".join(item.details)
                    for item in records
                )
            )

    def test_deferred_resume_checks_deadline_between_identity_operations(
        self,
    ) -> None:
        backend = self._task8_backend()
        selection_root = self._task7_managed_root(backend)
        managed = backend._resource(selection_root).node
        deleting = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000119",
            prefix=".deleting-",
            ready=True,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_resume_deferred_cleanup"],
        )
        crossed = False

        def cross_after_root_identity(event: str) -> None:
            nonlocal crossed
            if event == f"open-directory:{deleting.name}:pinned":
                crossed = True

        backend.after_event = cross_after_root_identity
        with (
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: 31.0 if crossed else 0.0,
            ),
            mock.patch.object(
                lease_module,
                "_janitor_candidate_record",
                side_effect=AssertionError(
                    "candidate processing started after deadline"
                ),
            ),
        ):
            result = lease_module._resume_deferred_cleanup(
                selection_root.path_hint,
                selection_root,
                deleting.name,
                deleting.identity,
                deleting.filesystem,
                backend,
                deadline=30.0,
            )

        self.assertIsInstance(result, JanitorDiagnostic)
        selection_root.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_janitor_stops_after_root_identity_crosses_deadline(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        candidate = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000124",
            ready=True,
        )
        crossed = False

        def cross_during_candidate_identity(event: str) -> None:
            nonlocal crossed
            if event == f"open-directory:{candidate.name}:pinned":
                crossed = True

        backend.after_event = cross_during_candidate_identity
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: 31.0 if crossed else 0.0,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertTrue(
            any(isinstance(item, JanitorDiagnostic) for item in records)
        )
        self.assertFalse(
            any(event == f"verify-managed:{candidate.name}:repair=false"
                for event in backend.events)
        )
        self.assertIn(candidate.name, managed.children)
        self.assertEqual(len(backend.live_resources), 0)

    def test_deferred_resume_diagnostic_preserves_first_slice_evidence(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            scratch.mark_cleanup_ready()
            scratch.__del__()
            first_slice = ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                7,
                3,
                ("first slice detail",),
                str(scratch.path),
            )

            with (
                mock.patch.object(
                    ManagedScratch,
                    "cleanup",
                    return_value=first_slice,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._resume_deferred_cleanup",
                    return_value=JanitorDiagnostic(
                        ("deferred identity changed",)
                    ),
                ),
            ):
                records = reclaim_abandoned(managed)

            cleanup_records = [
                item
                for item in records
                if isinstance(item, ScratchCleanupRecord)
            ]
            diagnostics = [
                item for item in records if isinstance(item, JanitorDiagnostic)
            ]
            self.assertEqual(len(cleanup_records), 1)
            self.assertEqual(cleanup_records[0].examined_entries, 7)
            self.assertEqual(cleanup_records[0].removed_entries, 3)
            self.assertIn("first slice detail", cleanup_records[0].details)
            self.assertTrue(
                any(
                    "deferred identity changed" in "; ".join(item.details)
                    for item in diagnostics
                )
            )

    def test_deferred_resume_stops_after_managed_open_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        selection_root = self._task7_managed_root(backend)
        managed = backend._resource(selection_root).node
        deleting = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000120",
            prefix=".deleting-",
            ready=True,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_resume_deferred_cleanup"],
        )
        crossed = False

        def cross_after_managed_open(event: str) -> None:
            nonlocal crossed
            if event == "reopen-directory:mutation":
                crossed = True

        backend.after_event = cross_after_managed_open
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: 31.0 if crossed else 0.0,
        ):
            result = lease_module._resume_deferred_cleanup(
                selection_root.path_hint,
                selection_root,
                deleting.name,
                deleting.identity,
                deleting.filesystem,
                backend,
                deadline=30.0,
            )

        self.assertIsInstance(result, JanitorDiagnostic)
        self.assertFalse(
            any(event == f"open-directory:{deleting.name}:pinned"
                for event in backend.events)
        )
        self.assertIn(deleting.name, managed.children)
        selection_root.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_empty_janitor_removal_stays_clean_after_coordinator_close_error(
        self,
    ) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        candidate = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000111",
            prefix=".staging-",
            lease=False,
            heartbeat=False,
            modified_ns=0,
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_lease_lock_retry"],
        )
        real_close_retry = lease_module._close_lease_lock_retry

        def fail_empty_close(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            errors = real_close_retry(lock, label)
            if label == "empty janitor coordinator":
                return (*errors, "injected empty coordinator close failure")
            return errors

        with mock.patch.object(
            lease_module,
            "_close_lease_lock_retry",
            side_effect=fail_empty_close,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                now=24 * 60 * 60 + 1.0,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertEqual(len(records), 1)
        record = records[0]
        assert isinstance(record, ScratchCleanupRecord)
        self.assertEqual(record.status, ScratchCleanupStatus.CLEAN)
        self.assertIsNone(record.remaining_root)
        self.assertIn("empty coordinator close failure", record.details[0])
        self.assertNotIn(candidate.name, managed.children)

    def test_empty_janitor_early_exit_preserves_coordinator_close_error(
        self,
    ) -> None:
        (
            backend,
            managed_capability,
            candidate,
            candidate_capability,
            selected,
        ) = self._task9_empty_candidate(
            "00000000-0000-4000-8000-000000000112"
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=[
                "_close_lease_lock_retry",
                "_reclaim_empty_unleased_candidate",
            ],
        )
        real_close_retry = lease_module._close_lease_lock_retry
        scans = 0

        def stop_second_scan(*_args: object, **_kwargs: object) -> bool:
            nonlocal scans
            scans += 1
            return scans == 1

        def fail_empty_close(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            errors = real_close_retry(lock, label)
            if label == "empty janitor coordinator":
                return (*errors, "injected empty early close failure")
            return errors

        with (
            mock.patch.object(
                lease_module,
                "_directory_is_empty_at",
                side_effect=stop_second_scan,
            ),
            mock.patch.object(
                lease_module,
                "_close_lease_lock_retry",
                side_effect=fail_empty_close,
            ),
        ):
            record = lease_module._reclaim_empty_unleased_candidate(
                managed_capability.path_hint,
                managed_capability,
                candidate_capability,
                selected,
                backend,
                current_time=24 * 60 * 60 + 1.0,
                deadline=time.monotonic() + 5.0,
            )

        self.assertIsInstance(record, JanitorDiagnostic)
        assert isinstance(record, JanitorDiagnostic)
        self.assertIn("empty early close failure", "; ".join(record.details))
        self.assertIn(candidate.name, backend._resource(managed_capability).node.children)
        candidate_capability.close()
        managed_capability.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_janitor_stops_before_cleanup_when_claim_identity_changes(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000125"
        original = self._task9_add_candidate(
            backend, managed, run_id=run_id, ready=True
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_lease_lock_retry"],
        )
        real_rename = backend.rename
        real_close_retry = lease_module._close_lease_lock_retry
        replacement: _ManagedRecordedNode | None = None

        def replace_after_claim(
            source: FileCapability | DirectoryCapability,
            destination_parent: DirectoryCapability,
            destination_name: str,
            *,
            replace: bool,
        ) -> None:
            nonlocal replacement
            real_rename(
                source,
                destination_parent,
                destination_name,
                replace=replace,
            )
            claimed = managed.children.pop(destination_name)
            claimed.name = f"{destination_name}.original"
            managed.children[claimed.name] = claimed
            replacement = backend._new_node(
                EntryKind.DIRECTORY,
                SecurityDomain.MANAGED,
                parent=managed,
                name=destination_name,
            )

        def fail_claim_close(
            lock: LeaseLock, label: str
        ) -> tuple[str, ...]:
            errors = real_close_retry(lock, label)
            if label == "janitor claim coordinator":
                return (*errors, "injected janitor claim close failure")
            return errors

        with (
            mock.patch.object(
                backend,
                "rename",
                side_effect=replace_after_claim,
            ),
            mock.patch.object(
                lease_module,
                "_close_lease_lock_retry",
                side_effect=fail_claim_close,
            ),
            mock.patch.object(ManagedScratch, "cleanup") as cleanup,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertEqual(len(records), 1)
        self.assertIsInstance(records[0], ScratchCleanupRecord)
        self.assertIn("identity changed", records[0].details[0])
        self.assertEqual(
            "; ".join(records[0].details).count("claim close failure"), 1
        )
        self.assertTrue(
            any(
                "claim close failure" in detail
                for detail in records[0].details[1:]
            )
        )
        self.assertIs(managed.children[f".deleting-{run_id}"], replacement)
        self.assertIn(".hoimin-lease.json", original.children)
        cleanup.assert_not_called()
        self.assertEqual(len(backend.live_resources), 0)

    def test_janitor_cursor_failure_is_reported_and_still_cleans_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            candidate = scratch.path
            scratch.__del__()

            with mock.patch(
                "tools.focused_mutation_support.lease._persist_coordinator_cursor",
                side_effect=OSError("injected cursor failure"),
            ):
                records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(len(records), 2)
            self.assertIsInstance(records[0], JanitorDiagnostic)
            record = records[1]
            assert isinstance(record, ScratchCleanupRecord)
            self.assertEqual(record.status, ScratchCleanupStatus.CLEAN)
            self.assertIn("cursor", records[0].details[0])
            self.assertFalse(candidate.exists())

    def test_janitor_never_combines_old_lease_with_replacement_root(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000702"
        original = self._task9_add_candidate(
            backend, managed, run_id=run_id, ready=True
        )
        swapped = False
        replacement: _ManagedRecordedNode | None = None

        def swap_before_candidate_open(event: str) -> None:
            nonlocal swapped, replacement
            if swapped or event != f"open-directory:run-{run_id}:pinned":
                return
            managed.children.pop(original.name)
            original.name = f"{original.name}.original"
            managed.children[original.name] = original
            replacement = backend._new_node(
                EntryKind.DIRECTORY,
                SecurityDomain.MANAGED,
                parent=managed,
                name=f"run-{run_id}",
            )
            backend._new_node(
                EntryKind.REGULAR,
                SecurityDomain.MANAGED,
                parent=replacement,
                name="sentinel",
            )
            swapped = True

        backend.after_event = swap_before_candidate_open
        records = reclaim_abandoned(
            managed_capability.path_hint,
            backend=backend,
            managed_root_capability=managed_capability,
        )

        self.assertTrue(swapped)
        self.assertEqual(len(records), 1)
        self.assertIsInstance(records[0], JanitorDiagnostic)
        self.assertIn("identity changed", records[0].details[0])
        self.assertIsNotNone(replacement)
        assert replacement is not None
        self.assertIn("sentinel", replacement.children)
        self.assertIn(".hoimin-lease.json", original.children)
        self.assertEqual(len(backend.live_resources), 0)

    def test_janitor_reclaims_empty_deleting_crash_tail_without_lease(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        deleting = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000021",
            prefix=".deleting-",
            lease=False,
            heartbeat=False,
        )

        records = reclaim_abandoned(
            managed_capability.path_hint,
            backend=backend,
            managed_root_capability=managed_capability,
        )

        self.assertEqual(
            [record.status for record in _cleanup_records_only(records)],
            [ScratchCleanupStatus.CLEAN],
            records,
        )
        self.assertNotIn(deleting.name, managed.children)

    def test_janitor_preserves_fresh_run_with_only_an_unlocked_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            scratch.close_capabilities()
            (active / ".hoimin-heartbeat.json").unlink()

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(records, [])
            self.assertTrue(active.is_dir())

    def test_janitor_preserves_fresh_staging_with_only_an_unlocked_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            staging = active.with_name(active.name.replace("run-", ".staging-"))
            scratch.close_capabilities()
            (active / ".hoimin-heartbeat.json").unlink()
            active.rename(staging)

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(records, [])
            self.assertTrue(staging.is_dir())

    def test_janitor_reclaims_old_staging_with_only_an_unlocked_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            staging = active.with_name(active.name.replace("run-", ".staging-"))
            scratch.close_capabilities()
            (active / ".hoimin-heartbeat.json").unlink()
            active.rename(staging)
            os.utime(staging, (1.0, 1.0))

            records = reclaim_abandoned(
                scratch.managed_root, now=24 * 60 * 60 + 2.0
            )

            self.assertEqual(
                [record.status for record in _cleanup_records_only(records)],
                [ScratchCleanupStatus.CLEAN],
            )
            self.assertFalse(staging.exists())

    def test_janitor_preserves_old_staging_with_unknown_payload(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            staging = active.with_name(active.name.replace("run-", ".staging-"))
            scratch.__del__()
            active.rename(staging)
            (staging / "sentinel.bin").write_bytes(b"must survive")
            os.utime(staging, (1.0, 1.0))
            for marker in staging.glob(".hoimin-*.json"):
                os.utime(marker, (1.0, 1.0))

            records = reclaim_abandoned(
                scratch.managed_root, now=24 * 60 * 60 + 2.0
            )

            self.assertEqual(records, [])
            self.assertEqual(
                (staging / "sentinel.bin").read_bytes(), b"must survive"
            )

    def test_janitor_resumes_deleting_tail_with_only_an_unlocked_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            deleting = active.with_name(active.name.replace("run-", ".deleting-"))
            scratch.close_capabilities()
            (active / ".hoimin-heartbeat.json").unlink()
            active.rename(deleting)

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(
                [record.status for record in _cleanup_records_only(records)],
                [ScratchCleanupStatus.CLEAN],
                records,
            )
            self.assertFalse(deleting.exists())

    def test_janitor_reclaims_only_old_empty_prelease_staging(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        old = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000022",
            prefix=".staging-",
            lease=False,
            heartbeat=False,
            modified_ns=1_000_000_000,
        )
        fresh = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000023",
            prefix=".staging-",
            lease=False,
            heartbeat=False,
            modified_ns=(24 * 60 * 60 + 2) * 1_000_000_000,
        )

        records = reclaim_abandoned(
            managed_capability.path_hint,
            now=24 * 60 * 60 + 2.0,
            backend=backend,
            managed_root_capability=managed_capability,
        )

        self.assertEqual(
            [record.status for record in _cleanup_records_only(records)],
            [ScratchCleanupStatus.CLEAN],
            records,
        )
        self.assertNotIn(old.name, managed.children)
        self.assertIn(fresh.name, managed.children)

    def test_janitor_cursor_reaches_the_two_hundred_fifty_seventh_root(self) -> None:
        backend = self._task8_backend()
        first_capability = self._task7_managed_root(backend)
        managed = backend._resource(first_capability).node
        for number in range(1, 258):
            self._task9_add_candidate(
                backend,
                managed,
                run_id=str(uuid.UUID(int=number)),
                ready=True,
            )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_janitor_candidate_record"],
        )
        visited: list[str] = []

        def observe_candidate(
            _managed_path: Path,
            _managed_capability: DirectoryCapability,
            candidate: object,
            _backend: FilesystemBackend,
            **_kwargs: object,
        ) -> ScratchCleanupRecord:
            visited.append(cast(str, getattr(candidate, "name")))
            return ScratchCleanupRecord(ScratchCleanupStatus.CLEAN, 1, 1)

        with mock.patch.object(
            lease_module,
            "_janitor_candidate_record",
            side_effect=observe_candidate,
        ):
            first = reclaim_abandoned(
                first_capability.path_hint,
                backend=backend,
                managed_root_capability=first_capability,
            )
            second_capability = self._task7_managed_root(backend)
            second = reclaim_abandoned(
                second_capability.path_hint,
                backend=backend,
                managed_root_capability=second_capability,
            )

        self.assertEqual(len(first), 256)
        self.assertEqual(len(second), 1)
        self.assertEqual(len(visited), 257)
        self.assertEqual(len(set(visited)), 257)
        self.assertEqual(
            visited[-1], f"run-{uuid.UUID(int=257)}"
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_janitor_cursor_failure_reports_but_still_cleans_selected_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            abandoned = scratch.path
            scratch.mark_cleanup_ready()
            scratch.__del__()

            with mock.patch(
                "tools.focused_mutation_support.lease._persist_coordinator_cursor",
                side_effect=OSError("injected cursor failure"),
            ):
                records = reclaim_abandoned(scratch.managed_root)

            self.assertTrue(
                any(
                    "cursor persistence failed" in " ".join(item.details)
                    for item in records
                )
            )
            self.assertFalse(abandoned.exists())

    def test_janitor_gives_every_candidate_one_slice_before_second_pass(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        for number in (31, 32):
            self._task9_add_candidate(
                backend,
                managed,
                run_id=f"00000000-0000-4000-8000-{number:012d}",
                ready=True,
            )
        visits: list[str] = []

        def two_slice_cleanup(
            scratch: ManagedScratch, *, time_budget: float
        ) -> ScratchCleanupRecord:
            visits.append(scratch.run_id)
            same_root_visits = visits.count(scratch.run_id)
            status = (
                ScratchCleanupStatus.DEFERRED
                if same_root_visits == 1
                else ScratchCleanupStatus.CLEAN
            )
            return ScratchCleanupRecord(
                status,
                1,
                same_root_visits - 1,
                remaining_root=(
                    str(scratch.path)
                    if status is ScratchCleanupStatus.DEFERRED
                    else None
                ),
            )

        with mock.patch.object(
            ManagedScratch,
            "cleanup",
            autospec=True,
            side_effect=two_slice_cleanup,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertEqual(
            visits,
            [
                "00000000-0000-4000-8000-000000000031",
                "00000000-0000-4000-8000-000000000032",
                "00000000-0000-4000-8000-000000000031",
                "00000000-0000-4000-8000-000000000032",
            ],
        )
        self.assertEqual(
            [record.status for record in _cleanup_records_only(records)],
            [ScratchCleanupStatus.CLEAN, ScratchCleanupStatus.CLEAN],
        )

    def test_task9_empty_delete_tracks_committed_absence_and_replacement(
        self,
    ) -> None:
        for replacement_after_delete in (False, True):
            with self.subTest(replacement=replacement_after_delete):
                backend = self._task8_backend()
                managed_capability = self._task7_managed_root(backend)
                managed = backend._resource(managed_capability).node
                run_id = (
                    "00000000-0000-4000-8000-000000000901"
                    if replacement_after_delete
                    else "00000000-0000-4000-8000-000000000900"
                )
                original = self._task9_add_candidate(
                    backend,
                    managed,
                    run_id=run_id,
                    prefix=".deleting-",
                    lease=False,
                    heartbeat=False,
                )
                replacement: _ManagedRecordedNode | None = None
                close_count_before_delete = -1

                def delete_then_fail_close(
                    capability: FileCapability | DirectoryCapability,
                ) -> None:
                    nonlocal close_count_before_delete, replacement
                    close_count_before_delete = backend.close_counts.get(
                        original.identity, 0
                    )
                    resource = backend._resource(capability)
                    node = resource.node
                    parent = node.parent
                    assert parent is not None
                    self.assertIs(parent.children.pop(node.name), node)
                    if replacement_after_delete:
                        replacement = backend._new_node(
                            EntryKind.DIRECTORY,
                            SecurityDomain.MANAGED,
                            parent=parent,
                            name=node.name,
                        )
                        backend._new_node(
                            EntryKind.REGULAR,
                            SecurityDomain.MANAGED,
                            parent=replacement,
                            name="sentinel",
                        )
                    resource.close_failures = 1
                    capability.close()

                with mock.patch.object(
                    backend, "delete", side_effect=delete_then_fail_close
                ):
                    records = reclaim_abandoned(
                        managed_capability.path_hint,
                        backend=backend,
                        managed_root_capability=managed_capability,
                    )

                cleanup = _cleanup_records_only(records)
                self.assertEqual(len(cleanup), 1, records)
                expected_status = (
                    ScratchCleanupStatus.FAILED
                    if replacement_after_delete
                    else ScratchCleanupStatus.CLEAN
                )
                self.assertEqual(cleanup[0].status, expected_status, cleanup[0])
                self.assertEqual(cleanup[0].removed_entries, 1)
                self.assertEqual(
                    backend.close_counts.get(original.identity, 0)
                    - close_count_before_delete,
                    2,
                )
                if replacement_after_delete:
                    assert replacement is not None
                    self.assertIs(managed.children[original.name], replacement)
                    self.assertIn("sentinel", replacement.children)
                else:
                    self.assertNotIn(original.name, managed.children)
                self.assertEqual(len(backend.live_resources), 0)

    def test_task9_empty_delete_deadline_defers_without_reopening_target(
        self,
    ) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        later_managed = backend.reopen_directory(
            managed_capability, SharePolicy.MUTATION
        )
        managed = backend._resource(managed_capability).node
        original = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000902",
            prefix=".deleting-",
            lease=False,
            heartbeat=False,
        )
        clock = [0.0]
        crossed_at = -1
        close_count_before_delete = -1

        def delete_then_cross_deadline(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal close_count_before_delete, crossed_at
            close_count_before_delete = backend.close_counts.get(
                original.identity, 0
            )
            resource = backend._resource(capability)
            node = resource.node
            parent = node.parent
            assert parent is not None
            self.assertIs(parent.children.pop(node.name), node)
            resource.close_failures = 1
            clock[0] = 31.0
            crossed_at = len(backend.cleanup_operations)
            capability.close()

        with (
            mock.patch.object(
                backend, "delete", side_effect=delete_then_cross_deadline
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )
            clock[0] = 0.0
            retry_start = len(backend.cleanup_operations)
            resumed = reclaim_abandoned(
                later_managed.path_hint,
                backend=backend,
                managed_root_capability=later_managed,
            )

        cleanup = _cleanup_records_only(records)
        self.assertEqual(len(cleanup), 1, records)
        self.assertEqual(cleanup[0].status, ScratchCleanupStatus.DEFERRED)
        self.assertEqual(cleanup[0].removed_entries, 1)
        resumed_cleanup = _cleanup_records_only(resumed)
        self.assertEqual(len(resumed_cleanup), 1, resumed)
        self.assertEqual(resumed_cleanup[0].status, ScratchCleanupStatus.CLEAN)
        self.assertEqual(resumed_cleanup[0].removed_entries, 1)
        self.assertNotIn(original.name, managed.children)
        self.assertGreaterEqual(crossed_at, 0)
        self.assertFalse(
            any(
                operation
                in {
                    f"entry:{original.name}",
                    f"open_directory:{original.name}",
                    f"delete:{original.name}",
                }
                for operation in backend.cleanup_operations[
                    crossed_at:retry_start
                ]
            ),
            backend.cleanup_operations[crossed_at:retry_start],
        )
        retry_operations = backend.cleanup_operations[retry_start:]
        self.assertEqual(retry_operations.count(f"entry:{original.name}"), 1)
        self.assertNotIn(f"open_directory:{original.name}", retry_operations)
        self.assertNotIn(f"delete:{original.name}", retry_operations)
        self.assertEqual(
            backend.close_counts.get(original.identity, 0)
            - close_count_before_delete,
            2,
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_preallocates_lease_owners_before_candidate_open(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_JanitorCandidate", "_janitor_candidate_record"],
        )
        for owner_kind in ("descriptor", "lease"):
            with self.subTest(owner=owner_kind):
                backend = self._task8_backend()
                managed_capability = self._task7_managed_root(backend)
                managed = backend._resource(managed_capability).node
                run_id = (
                    "00000000-0000-4000-8000-000000000903"
                    if owner_kind == "descriptor"
                    else "00000000-0000-4000-8000-000000000904"
                )
                candidate = self._task9_add_candidate(
                    backend, managed, run_id=run_id, retained=True
                )
                selected = lease_module._JanitorCandidate(
                    candidate.name,
                    candidate.identity,
                    candidate.filesystem,
                    run_id,
                    candidate.modified_ns,
                )
                candidate_opened = False
                real_descriptor = lease_module._OwnedDescriptor
                real_lock = lease_module.LeaseLock

                def note_candidate_open(operation: str) -> None:
                    nonlocal candidate_opened
                    if operation == f"open_directory:{candidate.name}":
                        candidate_opened = True

                def allocate_descriptor() -> _OwnedDescriptor:
                    if candidate_opened and owner_kind == "descriptor":
                        raise MemoryError(
                            "descriptor owner allocated after candidate open"
                        )
                    return real_descriptor()

                def allocate_lock(descriptor: int) -> LeaseLock:
                    if candidate_opened and owner_kind == "lease":
                        raise MemoryError(
                            "lease owner allocated after candidate open"
                        )
                    return real_lock(descriptor)

                backend.after_cleanup_operation = note_candidate_open
                escaped_detail: str | None = None
                try:
                    with (
                        mock.patch.object(
                            lease_module,
                            "_OwnedDescriptor",
                            side_effect=allocate_descriptor,
                        ),
                        mock.patch.object(
                            lease_module,
                            "LeaseLock",
                            side_effect=allocate_lock,
                        ),
                    ):
                        lease_module._janitor_candidate_record(
                            managed_capability.path_hint,
                            managed_capability,
                            selected,
                            backend,
                            current_time=24 * 60 * 60 + 1.0,
                            deadline=time.monotonic() + 30.0,
                        )
                except BaseException as error:
                    escaped_detail = f"{type(error).__name__}: {error}"
                    error.__traceback__ = None
                finally:
                    if managed_capability.is_open:
                        managed_capability.close()
                    gc.collect()
                self.assertIsNone(escaped_detail, escaped_detail)

    def test_task9_windows_handoff_does_not_allocate_lock_after_rename(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["LeaseLock"],
        )
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000905"
        self._task9_add_candidate(backend, managed, run_id=run_id, ready=True)
        real_lock = lease_module.LeaseLock
        renamed = False

        def note_rename(operation: str) -> None:
            nonlocal renamed
            if operation.startswith("rename:"):
                renamed = True

        def allocate_lock(descriptor: int) -> LeaseLock:
            if renamed and descriptor >= 0:
                raise MemoryError("lease lock allocated after claim rename")
            return real_lock(descriptor)

        backend.after_cleanup_operation = note_rename
        escaped_detail: str | None = None
        try:
            with mock.patch.object(
                lease_module, "LeaseLock", side_effect=allocate_lock
            ):
                reclaim_abandoned(
                    managed_capability.path_hint,
                    backend=backend,
                    managed_root_capability=managed_capability,
                )
        except BaseException as error:
            escaped_detail = f"{type(error).__name__}: {error}"
            error.__traceback__ = None
        finally:
            for descriptor in tuple(backend.detached):
                try:
                    os.close(descriptor)
                except OSError:
                    pass
                backend.detached.pop(descriptor, None)
            if managed_capability.is_open:
                managed_capability.close()
            gc.collect()
        self.assertIsNone(escaped_detail, escaped_detail)

    def test_task9_result_allocation_failures_dispose_acquired_owners(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_JanitorRecordLedger", "ManagedScratch"],
        )
        for failure_point in ("managed-result", "record-ledger"):
            with self.subTest(failure_point=failure_point):
                backend = self._task8_backend(
                    rename_requires_closed_descendants=True
                )
                managed_capability = self._task7_managed_root(backend)
                managed = backend._resource(managed_capability).node
                run_id = (
                    "00000000-0000-4000-8000-000000000923"
                    if failure_point == "managed-result"
                    else "00000000-0000-4000-8000-000000000924"
                )
                self._task9_add_candidate(
                    backend, managed, run_id=run_id, ready=True
                )
                real_add = lease_module._JanitorRecordLedger.add

                def fail_record_add(
                    ledger: object,
                    record: ScratchCleanupRecord | JanitorDiagnostic,
                    secondary: object | None = None,
                ) -> int | None:
                    if isinstance(record, ScratchCleanupRecord):
                        raise MemoryError("injected record ledger allocation")
                    return real_add(ledger, record, secondary)

                managed_patch = (
                    mock.patch.object(
                        lease_module,
                        "ManagedScratch",
                        side_effect=MemoryError(
                            "injected managed result allocation"
                        ),
                    )
                    if failure_point == "managed-result"
                    else nullcontext()
                )
                record_patch = (
                    mock.patch.object(
                        lease_module._JanitorRecordLedger,
                        "add",
                        autospec=True,
                        side_effect=fail_record_add,
                    )
                    if failure_point == "record-ledger"
                    else nullcontext()
                )
                expected = (
                    "managed result allocation"
                    if failure_point == "managed-result"
                    else "record ledger allocation"
                )
                with (
                    managed_patch,
                    record_patch,
                    self.assertRaisesRegex(MemoryError, expected),
                ):
                    reclaim_abandoned(
                        managed_capability.path_hint,
                        backend=backend,
                        managed_root_capability=managed_capability,
                    )

                gc.collect()
                self.assertEqual(len(backend.live_resources), 0)

    def test_task9_handoff_retains_transient_close_details_once(self) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000906"
        self._task9_add_candidate(backend, managed, run_id=run_id, ready=True)
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_capability_retry", "_close_lease_lock_retry"],
        )
        real_capability_close = lease_module._close_capability_retry
        real_lock_close = lease_module._close_lease_lock_retry

        def close_capability_with_detail(
            capability: DirectoryCapability | FileCapability,
            label: str,
        ) -> tuple[str, ...]:
            details = real_capability_close(capability, label)
            if label == "janitor pre-handoff root":
                return (*details, "injected transient root close")
            return details

        def close_lock_with_detail(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            details = real_lock_close(lock, label)
            if label == "janitor candidate lease handoff":
                return (*details, "injected transient lease close")
            return details

        with (
            mock.patch.object(
                lease_module,
                "_close_capability_retry",
                side_effect=close_capability_with_detail,
            ),
            mock.patch.object(
                lease_module,
                "_close_lease_lock_retry",
                side_effect=close_lock_with_detail,
            ),
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        cleanup = _cleanup_records_only(records)
        self.assertEqual(len(cleanup), 1, records)
        joined = "; ".join(cleanup[0].details)
        self.assertEqual(joined.count("transient lease close"), 1)
        self.assertEqual(joined.count("transient root close"), 1)
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_persistent_descendant_close_stops_claim_and_closes_coordinator_last(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000907"
        original = self._task9_add_candidate(
            backend, managed, run_id=run_id, ready=True
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_lease_lock_retry", "os"],
        )
        real_acquire = LeaseLock.acquire
        real_close = os.close
        real_close_retry = lease_module._close_lease_lock_retry
        candidate_lock: LeaseLock | None = None
        close_attempts = 0
        disposal_order: list[str] = []

        def capture_candidate_lock(
            lock: LeaseLock, *, blocking: bool
        ) -> None:
            nonlocal candidate_lock
            real_acquire(lock, blocking=blocking)
            node = backend.detached.get(lock.fd)
            if node is not None and node.name == ".hoimin-lease.json":
                candidate_lock = lock

        def fail_candidate_close(descriptor: int) -> None:
            nonlocal close_attempts
            if candidate_lock is not None and descriptor == candidate_lock.fd:
                close_attempts += 1
                raise OSError(
                    f"injected persistent janitor lease close {close_attempts}"
                )
            real_close(descriptor)

        def observe_disposal(
            lock: LeaseLock,
            label: str,
        ) -> tuple[str, ...]:
            details = real_close_retry(lock, label)
            if label.startswith("janitor"):
                disposal_order.append(label)
            return details

        with (
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=capture_candidate_lock,
            ),
            mock.patch.object(
                lease_module.os, "close", side_effect=fail_candidate_close
            ),
            mock.patch.object(
                lease_module,
                "_close_lease_lock_retry",
                side_effect=observe_disposal,
            ),
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        cleanup = _cleanup_records_only(records)
        self.assertEqual(len(cleanup), 1, records)
        self.assertEqual(cleanup[0].status, ScratchCleanupStatus.FAILED)
        self.assertEqual(close_attempts, 2)
        joined = "; ".join(cleanup[0].details)
        self.assertEqual(joined.count("persistent janitor lease close 1"), 1)
        self.assertEqual(joined.count("persistent janitor lease close 2"), 1)
        self.assertFalse(
            any(
                operation.startswith("rename:")
                for operation in backend.cleanup_operations
            ),
            backend.cleanup_operations,
        )
        self.assertIn(original.name, managed.children)
        self.assertEqual(disposal_order[-1], "janitor claim coordinator")
        assert candidate_lock is not None
        candidate_lock.__del__()
        self.assertEqual(candidate_lock.fd, -1)
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_selection_reports_canonical_non_directory_entries_once(
        self,
    ) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        kinds = (EntryKind.REGULAR, EntryKind.REPARSE, EntryKind.OTHER)
        names: list[str] = []
        for offset, kind in enumerate(kinds):
            run_id = f"00000000-0000-4000-8000-{910 + offset:012d}"
            name = f".deleting-{run_id}"
            names.append(name)
            backend._new_node(
                kind,
                SecurityDomain.MANAGED,
                parent=managed,
                name=name,
            )
        cross_run_id = "00000000-0000-4000-8000-000000000913"
        cross = self._task9_add_candidate(
            backend,
            managed,
            run_id=cross_run_id,
            prefix=".deleting-",
            lease=False,
            heartbeat=False,
        )
        cross.filesystem = FilesystemIdentity(cross.filesystem.volume + 1)
        names.append(cross.name)

        records = reclaim_abandoned(
            managed_capability.path_hint,
            backend=backend,
            managed_root_capability=managed_capability,
        )

        diagnostics = [
            record for record in records if isinstance(record, JanitorDiagnostic)
        ]
        self.assertEqual(len(diagnostics), 4, records)
        joined = "\n".join(
            detail for record in diagnostics for detail in record.details
        )
        for name in names:
            self.assertEqual(joined.count(name), 1, joined)
            self.assertNotIn(f"open_directory:{name}", backend.cleanup_operations)
            self.assertNotIn(f"delete:{name}", backend.cleanup_operations)
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_record_ledger_bounds_peak_at_add_time(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        for number in range(256):
            run_id = f"00000000-0000-4000-8000-{20_000 + number:012d}"
            self._task9_add_candidate(
                backend,
                managed,
                run_id=run_id,
                lease=False,
                heartbeat=False,
            )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_JanitorRecordLedger", "_janitor_candidate_record"],
        )
        real_add = lease_module._JanitorRecordLedger.add
        peak_details = 0
        peak_records = 0
        first_source_details: tuple[str, ...] | None = None

        def candidate_record(*_args: object, **_kwargs: object) -> JanitorDiagnostic:
            nonlocal first_source_details
            details = tuple(
                f"candidate-detail-{index}" for index in range(256)
            )
            if first_source_details is None:
                first_source_details = details
            return JanitorDiagnostic(
                details
            )

        def observe_add(
            ledger: object,
            record: ScratchCleanupRecord | JanitorDiagnostic,
        ) -> int | None:
            nonlocal peak_details, peak_records
            result = real_add(ledger, record)
            ledger_records = cast(
                list[ScratchCleanupRecord | JanitorDiagnostic | None],
                getattr(ledger, "_records"),
            )
            record_count = cast(int, getattr(ledger, "_record_count"))
            retained_records = tuple(
                item
                for item in ledger_records[:record_count]
                if item is not None
            )
            detail_cells = getattr(ledger, "_detail_cells", ())
            peak_details = max(
                peak_details,
                sum(len(item.details) for item in retained_records)
                + sum(item is not None for item in detail_cells),
            )
            peak_records = max(peak_records, len(retained_records))
            return result

        with (
            mock.patch.object(
                lease_module,
                "_janitor_candidate_record",
                side_effect=candidate_record,
            ),
            mock.patch.object(
                lease_module._JanitorRecordLedger,
                "add",
                autospec=True,
                side_effect=observe_add,
            ),
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertLessEqual(peak_records, 256)
        self.assertLessEqual(peak_details, MAX_DIAGNOSTIC_DETAILS)
        self.assertIsNotNone(first_source_details)
        self.assertIs(records[0].details, first_source_details)
        self.assertEqual(sum(len(record.details) for record in records), 256)
        self.assertEqual(
            sum(record.omitted_detail_count for record in records),
            256 * 255,
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_record_ledger_deduplicates_before_retaining_cells(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_JanitorRecordLedger"],
        )
        ledger = lease_module._JanitorRecordLedger()
        repeated = tuple("duplicate detail" for _index in range(256))

        ledger.add(JanitorDiagnostic(repeated))
        records = ledger.records()

        self.assertEqual(records[0].details, ("duplicate detail",))
        self.assertEqual(records[0].omitted_detail_count, 0)
        detail_cells = getattr(ledger, "_detail_cells", ())
        retained = sum(item is not None for item in detail_cells)
        retained += sum(
            len(item.details)
            for item in ledger._records[: ledger._record_count]
            if item is not None
        )
        self.assertEqual(retained, 1)

    def test_task9_keeps_primary_and_owner_details_separate_and_unique(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_FixedDetailLedger", "_attach_janitor_details"],
        )
        near_limit_primary = "p" * (MAX_DIAGNOSTIC_DETAIL_BYTES - 1)
        close_detail = "candidate root close evidence"
        ledger = lease_module._FixedDetailLedger()
        ledger.add(close_detail)
        result = lease_module._attach_janitor_details(
            JanitorDiagnostic((near_limit_primary,)), ledger
        )

        self.assertIsInstance(result, JanitorDiagnostic)
        assert isinstance(result, JanitorDiagnostic)
        self.assertEqual(result.details[0], near_limit_primary)
        self.assertEqual(result.details[1:], (close_detail,))
        self.assertEqual(result.omitted_detail_count, 0)

        duplicate_ledger = lease_module._FixedDetailLedger()
        duplicate_ledger.add("duplicate detail")
        duplicate_ledger.add("owner-only detail")
        duplicate = lease_module._attach_janitor_details(
            JanitorDiagnostic(("primary detail", "duplicate detail")),
            duplicate_ledger,
        )
        self.assertIsInstance(duplicate, JanitorDiagnostic)
        assert isinstance(duplicate, JanitorDiagnostic)
        self.assertEqual(
            duplicate.details,
            ("primary detail", "duplicate detail", "owner-only detail"),
        )
        self.assertEqual(duplicate.omitted_detail_count, 0)

    def test_task9_empty_delete_pending_waits_for_external_owner_close(
        self,
    ) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        live_managed = backend.reopen_directory(
            managed_capability, SharePolicy.MUTATION
        )
        later_managed = backend.reopen_directory(
            managed_capability, SharePolicy.MUTATION
        )
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000930"
        candidate = self._task9_add_candidate(
            backend,
            managed,
            run_id=run_id,
            prefix=".staging-",
            lease=False,
            heartbeat=False,
        )
        real_close_resource = backend.close_resource
        pending_owner: DirectoryCapability | None = None
        pending_resource: _ManagedRecordedResource | None = None
        delete_started = False

        def arm_delete(capability: FileCapability | DirectoryCapability) -> None:
            nonlocal delete_started, pending_owner, pending_resource
            self.assertIsInstance(capability, DirectoryCapability)
            assert isinstance(capability, DirectoryCapability)
            backend._cleanup_operation(f"delete:{candidate.name}")
            delete_started = True
            pending_owner = capability
            pending_resource = backend._resource(capability)
            pending_resource.close_failures = 2

        def close_and_commit(value: object) -> None:
            real_close_resource(value)
            if (
                pending_resource is not None
                and value is pending_resource
                and pending_resource.closed
                and delete_started
            ):
                parent = candidate.parent
                if parent is not None and parent.children.get(candidate.name) is candidate:
                    del parent.children[candidate.name]

        with (
            mock.patch.object(backend, "delete", side_effect=arm_delete),
            mock.patch.object(
                backend, "close_resource", side_effect=close_and_commit
            ),
        ):
            first = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )
            self.assertTrue(delete_started)
            assert pending_owner is not None
            delete_index = backend.cleanup_operations.index(
                f"delete:{candidate.name}"
            )
            first_suffix = backend.cleanup_operations[delete_index + 1 :]
            first_cleanup = _cleanup_records_only(first)
            first_status = first_cleanup[0].status
            close_attempts_before_external = pending_owner._close_attempts
            owner_open_before_external = pending_owner.is_open

            live_retry_start = len(backend.cleanup_operations)
            still_live = reclaim_abandoned(
                live_managed.path_hint,
                backend=backend,
                managed_root_capability=live_managed,
            )
            live_retry_operations = backend.cleanup_operations[live_retry_start:]
            attempts_after_live_retry = pending_owner._close_attempts
            pending_owner.close()
            self.assertNotIn(candidate.name, managed.children)
            retry_start = len(backend.cleanup_operations)
            second = reclaim_abandoned(
                later_managed.path_hint,
                backend=backend,
                managed_root_capability=later_managed,
            )
            retry_operations = backend.cleanup_operations[retry_start:]

        second_cleanup = _cleanup_records_only(second)
        live_cleanup = _cleanup_records_only(still_live)
        self.assertEqual(first_status, ScratchCleanupStatus.DEFERRED, first)
        self.assertTrue(owner_open_before_external)
        self.assertEqual(close_attempts_before_external, 2)
        self.assertEqual(attempts_after_live_retry, 2)
        self.assertNotIn(f"entry:{candidate.name}", first_suffix)
        self.assertFalse(
            any(
                operation.startswith(
                    (f"open_directory:{candidate.name}", f"delete:{candidate.name}")
                )
                for operation in first_suffix
            ),
            first_suffix,
        )
        self.assertEqual(len(live_cleanup), 1, still_live)
        self.assertEqual(live_cleanup[0].status, ScratchCleanupStatus.DEFERRED)
        self.assertNotIn(f"entry:{candidate.name}", live_retry_operations)
        self.assertNotIn(
            f"open_directory:{candidate.name}", live_retry_operations
        )
        self.assertNotIn(f"delete:{candidate.name}", live_retry_operations)
        self.assertEqual(len(second_cleanup), 1, second)
        self.assertEqual(second_cleanup[0].status, ScratchCleanupStatus.CLEAN)
        self.assertEqual(second_cleanup[0].removed_entries, 1)
        self.assertEqual(retry_operations.count(f"entry:{candidate.name}"), 1)
        self.assertNotIn(f"open_directory:{candidate.name}", retry_operations)
        self.assertNotIn(f"delete:{candidate.name}", retry_operations)
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_closed_pending_resolves_replacement_without_retargeting(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_DEFERRED_EMPTY_PENDING", "_PendingAbsenceCell"],
        )
        for outcome in ("same-identity", "replacement"):
            with self.subTest(outcome=outcome):
                backend = self._task8_backend()
                managed_capability = self._task7_managed_root(backend)
                managed = backend._resource(managed_capability).node
                run_id = (
                    "00000000-0000-4000-8000-000000000931"
                    if outcome == "same-identity"
                    else "00000000-0000-4000-8000-000000000932"
                )
                original = self._task9_add_candidate(
                    backend,
                    managed,
                    run_id=run_id,
                    prefix=".deleting-",
                    lease=False,
                    heartbeat=False,
                )
                cell = lease_module._DEFERRED_EMPTY_PENDING.reserve(
                    backend,
                    managed_capability.path_hint,
                    managed_capability,
                )
                self.assertIsNotNone(cell)
                pending = lease_module._PendingAbsenceCell()
                pending.arm(
                    scope="root",
                    name=original.name,
                    identity=original.identity,
                    filesystem=original.filesystem,
                    removed_after=1,
                )
                pending.commit()
                replacement: _ManagedRecordedNode | None = None
                if outcome == "replacement":
                    self.assertIs(managed.children.pop(original.name), original)
                    replacement = backend._new_node(
                        EntryKind.DIRECTORY,
                        SecurityDomain.MANAGED,
                        parent=managed,
                        name=original.name,
                    )
                    backend._new_node(
                        EntryKind.REGULAR,
                        SecurityDomain.MANAGED,
                        parent=replacement,
                        name="sentinel",
                    )
                lease_module._DEFERRED_EMPTY_PENDING.activate(
                    cell, None, pending
                )
                operation_start = len(backend.cleanup_operations)

                records = reclaim_abandoned(
                    managed_capability.path_hint,
                    backend=backend,
                    managed_root_capability=managed_capability,
                )

                operations = backend.cleanup_operations[operation_start:]
                cleanup = _cleanup_records_only(records)
                self.assertEqual(len(cleanup), 1, records)
                self.assertEqual(cleanup[0].status, ScratchCleanupStatus.FAILED)
                self.assertEqual(
                    cleanup[0].removed_entries,
                    0 if outcome == "same-identity" else 1,
                )
                self.assertEqual(operations.count(f"entry:{original.name}"), 1)
                self.assertNotIn(
                    f"open_directory:{original.name}", operations
                )
                self.assertNotIn(f"delete:{original.name}", operations)
                if replacement is not None:
                    self.assertIs(managed.children[original.name], replacement)
                    self.assertIn("sentinel", replacement.children)
                else:
                    self.assertIs(managed.children[original.name], original)
                self.assertEqual(len(backend.live_resources), 0)

    def test_task9_full_pending_registry_stops_before_candidate_open(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_DEFERRED_EMPTY_PENDING"],
        )
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        candidate = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000933",
            prefix=".staging-",
            lease=False,
            heartbeat=False,
        )
        reservations = []
        for _index in range(lease_module.MAX_RECLAIM_CANDIDATES):
            cell = lease_module._DEFERRED_EMPTY_PENDING.reserve(
                backend,
                managed_capability.path_hint,
                managed_capability,
            )
            self.assertIsNotNone(cell)
            reservations.append(cell)
        try:
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )
        finally:
            for cell in reservations:
                lease_module._DEFERRED_EMPTY_PENDING.release(cell)

        joined = "; ".join(
            detail for record in records for detail in record.details
        )
        self.assertIn("pending owner registry is full", joined)
        self.assertNotIn(
            f"open_directory:{candidate.name}", backend.cleanup_operations
        )
        self.assertNotIn(f"delete:{candidate.name}", backend.cleanup_operations)
        self.assertIn(candidate.name, managed.children)
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_pending_result_allocation_keeps_registry_evidence(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_DEFERRED_EMPTY_PENDING", "_JanitorRecordLedger"],
        )
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        later_managed = backend.reopen_directory(
            managed_capability, SharePolicy.MUTATION
        )
        managed = backend._resource(managed_capability).node
        original = self._task9_add_candidate(
            backend,
            managed,
            run_id="00000000-0000-4000-8000-000000000934",
            prefix=".deleting-",
            lease=False,
            heartbeat=False,
        )
        self.assertIs(managed.children.pop(original.name), original)
        cell = lease_module._DEFERRED_EMPTY_PENDING.reserve(
            backend, managed_capability.path_hint, managed_capability
        )
        self.assertIsNotNone(cell)
        pending = lease_module._PendingAbsenceCell()
        pending.arm(
            scope="root",
            name=original.name,
            identity=original.identity,
            filesystem=original.filesystem,
            removed_after=1,
        )
        pending.commit()
        lease_module._DEFERRED_EMPTY_PENDING.activate(cell, None, pending)
        real_add = lease_module._JanitorRecordLedger.add
        failed = False

        def fail_first_result(
            ledger: object,
            record: ScratchCleanupRecord | JanitorDiagnostic,
            secondary: object | None = None,
        ) -> int | None:
            nonlocal failed
            if not failed and isinstance(record, ScratchCleanupRecord):
                failed = True
                raise MemoryError("injected pending result allocation")
            return real_add(ledger, record, secondary)

        with (
            mock.patch.object(
                lease_module._JanitorRecordLedger,
                "add",
                autospec=True,
                side_effect=fail_first_result,
            ),
            self.assertRaisesRegex(MemoryError, "pending result allocation"),
        ):
            reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        self.assertTrue(failed)
        self.assertTrue(cell.active)
        self.assertFalse(cell.resolving)
        retry_start = len(backend.cleanup_operations)
        records = reclaim_abandoned(
            later_managed.path_hint,
            backend=backend,
            managed_root_capability=later_managed,
        )

        cleanup = _cleanup_records_only(records)
        retry_operations = backend.cleanup_operations[retry_start:]
        self.assertEqual(len(cleanup), 1, records)
        self.assertEqual(cleanup[0].status, ScratchCleanupStatus.CLEAN)
        self.assertEqual(cleanup[0].removed_entries, 1)
        self.assertEqual(retry_operations.count(f"entry:{original.name}"), 1)
        self.assertNotIn(f"open_directory:{original.name}", retry_operations)
        self.assertNotIn(f"delete:{original.name}", retry_operations)
        self.assertFalse(cell.active)
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_revalidates_postclaim_marker_content_and_identity(
        self,
    ) -> None:
        for mutation in (
            "heartbeat-content",
            "ready-replacement",
            "retain-appearance",
        ):
            with self.subTest(mutation=mutation):
                backend = self._task8_backend(
                    rename_requires_closed_descendants=True
                )
                managed_capability = self._task7_managed_root(backend)
                managed = backend._resource(managed_capability).node
                run_id = (
                    "00000000-0000-4000-8000-000000000920"
                    if mutation == "heartbeat-content"
                    else (
                        "00000000-0000-4000-8000-000000000921"
                        if mutation == "ready-replacement"
                        else "00000000-0000-4000-8000-000000000925"
                    )
                )
                root = self._task9_add_candidate(
                    backend, managed, run_id=run_id, ready=True
                )
                real_rename = backend.rename
                replacement: _ManagedRecordedNode | None = None

                def mutate_after_claim(
                    source: FileCapability | DirectoryCapability,
                    destination_parent: DirectoryCapability,
                    destination_name: str,
                    *,
                    replace: bool,
                ) -> None:
                    nonlocal replacement
                    real_rename(
                        source,
                        destination_parent,
                        destination_name,
                        replace=replace,
                    )
                    if mutation == "heartbeat-content":
                        marker = root.children[".hoimin-heartbeat.json"]
                        assert marker.backing is not None
                        marker.backing.seek(0)
                        marker.backing.truncate()
                        marker.backing.write(b"{}\n")
                        marker.backing.flush()
                    elif mutation == "ready-replacement":
                        original = root.children.pop(
                            ".hoimin-cleanup-ready.json"
                        )
                        original.name = ".hoimin-cleanup-ready.original"
                        root.children[original.name] = original
                        replacement = self._task9_add_marker(
                            backend,
                            root,
                            ".hoimin-cleanup-ready.json",
                            run_id,
                            str(uuid.uuid4()),
                        )
                    else:
                        replacement = self._task9_add_marker(
                            backend,
                            root,
                            ".hoimin-retain.json",
                            run_id,
                            str(uuid.uuid4()),
                        )

                with (
                    mock.patch.object(
                        backend, "rename", side_effect=mutate_after_claim
                    ),
                    mock.patch.object(
                        ManagedScratch,
                        "cleanup",
                        return_value=ScratchCleanupRecord(
                            ScratchCleanupStatus.CLEAN, 0, 0
                        ),
                    ) as cleanup,
                ):
                    records = reclaim_abandoned(
                        managed_capability.path_hint,
                        backend=backend,
                        managed_root_capability=managed_capability,
                    )

                cleanup.assert_not_called()
                failures = _cleanup_records_only(records)
                self.assertEqual(len(failures), 1, records)
                self.assertEqual(
                    failures[0].status, ScratchCleanupStatus.FAILED
                )
                self.assertIn("marker", "; ".join(failures[0].details))
                if replacement is not None:
                    replacement_name = (
                        ".hoimin-cleanup-ready.json"
                        if mutation == "ready-replacement"
                        else ".hoimin-retain.json"
                    )
                    self.assertIs(root.children[replacement_name], replacement)
                self.assertFalse(
                    any(
                        operation.startswith("delete:")
                        for operation in backend.cleanup_operations
                    ),
                    backend.cleanup_operations,
                )
                self.assertEqual(len(backend.live_resources), 0)

    def test_task9_postclaim_rebind_stops_at_shared_absolute_deadline(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000922"
        self._task9_add_candidate(backend, managed, run_id=run_id, ready=True)
        clock = [0.0]
        heartbeat_opens = 0
        crossed_at = -1

        def cross_on_rebind(event: str) -> None:
            nonlocal crossed_at, heartbeat_opens
            if event == "open_existing:.hoimin-heartbeat.json:read:pinned":
                heartbeat_opens += 1
                if heartbeat_opens == 2:
                    clock[0] = 31.0
                    crossed_at = len(backend.events)

        backend.after_event = cross_on_rebind
        with (
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
            mock.patch.object(ManagedScratch, "cleanup") as cleanup,
        ):
            records = reclaim_abandoned(
                managed_capability.path_hint,
                backend=backend,
                managed_root_capability=managed_capability,
            )

        cleanup.assert_not_called()
        self.assertGreaterEqual(crossed_at, 0)
        after_deadline = backend.events[crossed_at:]
        self.assertFalse(
            any(
                event.startswith(
                    (
                        "verify-managed:.hoimin-heartbeat.json",
                        "reopen-directory:mutation",
                    )
                )
                for event in after_deadline
            ),
            after_deadline,
        )
        self.assertTrue(
            any(
                isinstance(record, JanitorDiagnostic)
                and "deadline" in "; ".join(record.details)
                for record in records
            ),
            records,
        )
        self.assertEqual(len(backend.live_resources), 0)

    def test_task8_posix_claim_handoff_uses_capability_authority(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(backend)
        active_name = scratch.path.name
        deleting_name = f".deleting-{scratch.run_id}"

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertIn(
            f"rename:{active_name}->{deleting_name}",
            backend.cleanup_operations,
        )
        self.assertNotIn(active_name, backend.parent.children)
        managed = backend.parent.children["hoimin-focused-v1"]
        self.assertNotIn(deleting_name, managed.children)
        self.assertLessEqual(backend.max_directory_resources, 3)

    def test_task8_windows_claim_handoff_closes_and_reopens_descendants(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000802",
        )
        active_name = scratch.path.name
        deleting_name = f".deleting-{scratch.run_id}"
        event_start = len(backend.events)

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        events = backend.events[event_start:]
        heartbeat_close = events.index("close:.hoimin-heartbeat.json")
        rename = events.index(
            f"rename:{active_name}->{deleting_name}:False"
        )
        lease_reopen = next(
            index
            for index, event in enumerate(events)
            if index > rename
            and event.startswith("open_existing:.hoimin-lease.json:")
        )
        lease_detach = events.index("detach:.hoimin-lease.json", rename)
        heartbeat_reopen = next(
            index
            for index, event in enumerate(events)
            if index > rename
            and event.startswith("open_existing:.hoimin-heartbeat.json:")
        )
        self.assertLess(heartbeat_close, rename)
        self.assertLess(rename, lease_reopen)
        self.assertLess(lease_reopen, lease_detach)
        self.assertLess(lease_detach, heartbeat_reopen)
        self.assertLessEqual(backend.max_directory_resources, 3)

    def test_task8_windows_partial_restore_reuses_live_lease_on_retry(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000833",
        )
        heartbeat_reopen = (
            "open_existing:.hoimin-heartbeat.json:write:pinned"
        )
        lease_reopen = "open_existing:.hoimin-lease.json:read_write:pinned"
        backend.failures[heartbeat_reopen] = OSError(
            "injected heartbeat owner reopen failure"
        )
        initial_reopen_count = backend.events.count(lease_reopen)

        first = self._task8_cleanup(scratch, backend)

        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        restored_lease = scratch._lease
        self.assertIsNotNone(restored_lease)
        assert restored_lease is not None
        self.assertGreaterEqual(restored_lease.fd, 0)
        self.assertIsNone(scratch._heartbeat)
        first_reopen_count = backend.events.count(lease_reopen)
        self.assertEqual(first_reopen_count - initial_reopen_count, 1)

        second = self._task8_cleanup(scratch, backend)

        self.assertEqual(second.status, ScratchCleanupStatus.CLEAN, second)
        self.assertEqual(backend.events.count(lease_reopen), first_reopen_count)
        self.assertEqual(restored_lease.fd, -1)

    def test_task8_persistent_descendant_close_stops_namespace_mutation(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000803",
        )
        heartbeat = scratch._heartbeat
        assert heartbeat is not None
        backend._resource(heartbeat).close_failures = 2

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(backend.close_counts[heartbeat.identity], 2)
        second_close = max(
            index
            for index, operation in enumerate(backend.cleanup_operations)
            if operation == "close:.hoimin-heartbeat.json"
        )
        unsafe_suffix = backend.cleanup_operations[second_close + 1 :]
        self.assertFalse(
            any(
                operation.startswith(("rename:", "delete:", "open_entry:"))
                for operation in unsafe_suffix
            ),
            unsafe_suffix,
        )
        self.assertEqual(
            "; ".join(cleanup.details).count(
                "managed namespace cleanup unavailable"
            ),
            1,
        )

    def test_task8_payload_resume_reopens_validated_cursor(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000804",
        )
        root = self._task8_root_node(backend, scratch)
        level = self._task8_add_payload(
            backend, root, "level", kind=EntryKind.DIRECTORY
        )
        self._task8_add_payload(backend, level, "payload")
        clock = [0.0]
        crossed = False

        def cross_inside_child(operation: str) -> None:
            nonlocal crossed
            if operation == "iterator.next:level" and not crossed:
                crossed = True
                clock[0] = 5.0

        backend.after_cleanup_operation = cross_inside_child
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertTrue(crossed)
        self.assertGreaterEqual(
            backend.cleanup_operations.count("open_directory:level"), 2
        )
        self.assertGreaterEqual(
            backend.cleanup_operations.count("entries_owned"), 3
        )
        self.assertLessEqual(backend.max_directory_resources, 3)

    def test_task8_entry_limit_accepts_50000_and_defers_before_50001(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000811",
        )
        root = scratch._root
        anchor = scratch._managed_root_capability
        assert root is not None
        assert anchor is not None
        root_node = backend._resource(root).node
        root_identity = root.identity
        backend.virtual_entry_counts[root.identity] = 50_001
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_CleanupCursor", "_remove_payload"],
        )

        result = lease_module._remove_payload(
            backend,
            anchor,
            root,
            root_name=scratch.path.name,
            root_identity=scratch._root_identity,
            root_filesystem=scratch._root_filesystem,
            cursor=lease_module._CleanupCursor(),
            started=0.0,
            absolute_deadline=60.0,
            examined=0,
            removed=0,
            monotonic=lambda: 0.0,
        )
        scratch._root = None

        self.assertFalse(result.complete)
        self.assertEqual(result.examined_entries, 50_000)
        self.assertEqual(result.removed_entries, 0)
        self.assertEqual(
            sum(
                operation.startswith("iterator.next:")
                for operation in backend.cleanup_operations
            ),
            50_000,
        )
        self.assertLessEqual(backend.max_directory_resources, 3)

        backend.virtual_entry_counts[root_identity] = 0
        payload = self._task8_add_payload(backend, root_node, "after-slice")
        resumed = lease_module._remove_payload(
            backend,
            anchor,
            None,
            root_name=scratch.path.name,
            root_identity=scratch._root_identity,
            root_filesystem=scratch._root_filesystem,
            cursor=result.cursor,
            started=0.0,
            absolute_deadline=60.0,
            examined=result.examined_entries,
            removed=result.removed_entries,
            monotonic=lambda: 0.0,
        )
        scratch._root = resumed.root

        self.assertTrue(resumed.complete)
        self.assertEqual(resumed.examined_entries, 50_001)
        self.assertEqual(resumed.removed_entries, 1)
        self.assertNotIn(payload.name, root_node.children)

    def test_task8_depth_and_cursor_boundaries_are_exact(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=[
                "_CleanupComponent",
                "_CleanupCursor",
                "_validate_cleanup_cursor",
            ],
        )
        component = lease_module._CleanupComponent(
            "d",
            FileIdentity(1, 1),
            FilesystemIdentity(1, 2, 3),
        )
        exact_depth = lease_module._CleanupCursor(
            (component,) * 4_096
        )
        too_deep = lease_module._CleanupCursor((component,) * 4_097)
        with mock.patch.object(
            lease_module, "MAX_CLEANUP_CURSOR_BYTES", 1_000_000
        ):
            lease_module._validate_cleanup_cursor(exact_depth)
            with self.assertRaisesRegex(OSError, "depth exceeds 4096"):
                lease_module._validate_cleanup_cursor(too_deep)

        one_component = lease_module._CleanupCursor((component,))
        exact_cursor_bytes = len(one_component.encode())
        with mock.patch.object(
            lease_module,
            "MAX_CLEANUP_CURSOR_BYTES",
            exact_cursor_bytes,
        ):
            lease_module._validate_cleanup_cursor(one_component)
        with mock.patch.object(
            lease_module,
            "MAX_CLEANUP_CURSOR_BYTES",
            exact_cursor_bytes - 1,
        ):
            with self.assertRaisesRegex(OSError, "cursor exceeds"):
                lease_module._validate_cleanup_cursor(one_component)

    def test_task8_replacement_before_consuming_delete_is_preserved(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000805",
        )
        child_path = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        child = root.children[child_path.name]
        original = self._task8_add_payload(backend, child, "payload")
        replacement: _ManagedRecordedNode | None = None

        def replace_before_delete(operation: str) -> None:
            nonlocal replacement
            if operation != "delete:payload" or replacement is not None:
                return
            self.assertIs(child.children["payload"], original)
            del child.children["payload"]
            replacement = self._task8_add_payload(
                backend, child, "payload"
            )

        backend.before_cleanup_operation = replace_before_delete
        cleanup = scratch.remove_child(child_path)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        assert replacement is not None
        self.assertIs(child.children["payload"], replacement)
        self.assertNotEqual(replacement.identity, original.identity)

    def test_task8_close_failure_uses_two_lifetime_attempts(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000806",
        )
        child_path = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        child = root.children[child_path.name]
        payload = self._task8_add_payload(backend, child, "payload")
        backend.close_failures_by_identity[payload.identity] = 2

        cleanup = scratch.remove_child(child_path)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(backend.close_counts[payload.identity], 2)
        self.assertEqual(
            "; ".join(cleanup.details).count(
                "managed namespace cleanup unavailable"
            ),
            1,
        )

    def test_task8_lease_close_budget_is_shared_across_cleanup_paths(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000843",
        )
        lease = scratch._lease
        assert lease is not None
        target = lease.fd
        real_close = os.close
        attempts = 0
        after_second_operation = 0

        def fail_target_close(descriptor: int) -> None:
            nonlocal attempts, after_second_operation
            if descriptor == target:
                attempts += 1
                if attempts == 2:
                    after_second_operation = len(backend.cleanup_operations)
                raise OSError(f"injected lease close failure {attempts}")
            real_close(descriptor)

        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["os"],
        )
        with mock.patch.object(
            lease_module.os, "close", side_effect=fail_target_close
        ):
            cleanup = self._task8_cleanup(scratch, backend)
            attempts_after_cleanup = attempts
            scratch.close_capabilities()
            attempts_after_public_close = attempts

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(attempts_after_cleanup, 2)
        self.assertEqual(attempts_after_public_close, 2)
        self.assertIs(scratch._lease, lease)
        self.assertEqual(lease.fd, target)
        joined = "; ".join(cleanup.details)
        self.assertIn("lease close failure 1", joined)
        self.assertIn("lease close failure 2", joined)
        self.assertEqual(
            joined.count("managed namespace cleanup unavailable"), 1
        )
        unsafe = backend.cleanup_operations[after_second_operation:]
        self.assertFalse(
            any(
                operation.startswith(
                    ("rename:", "delete:", "entry:", "open_entry:")
                )
                for operation in unsafe
            ),
            unsafe,
        )

        lease.__del__()
        self.assertEqual(lease.fd, -1)
        with self.assertRaises(OSError):
            os.fstat(target)

    def test_lease_public_close_spends_one_remaining_attempt_with_details(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000862",
        )
        lease = scratch._lease
        assert lease is not None
        descriptor = lease.fd
        real_close = os.close
        attempts = 0

        def transient_close(target: int) -> None:
            nonlocal attempts
            if target == descriptor:
                attempts += 1
                if attempts == 1:
                    raise OSError("injected first public lease close failure")
            real_close(target)

        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["os"],
        )
        with mock.patch.object(
            lease_module.os, "close", side_effect=transient_close
        ):
            scratch.close_capabilities()
            self.assertEqual(attempts, 1)
            self.assertIs(scratch._lease, lease)
            self.assertEqual(lease.fd, descriptor)

            scratch.close_capabilities()

        self.assertEqual(attempts, 2)
        self.assertIsNone(scratch._lease)
        self.assertEqual(lease.fd, -1)
        self.assertTrue(hasattr(lease, "_close_attempts"))
        self.assertEqual(getattr(lease, "_close_attempts"), 2)
        self.assertTrue(hasattr(lease, "_close_details"))
        details = cast(tuple[str, ...], getattr(lease, "_close_details"))
        self.assertIn("first public lease close failure", "; ".join(details))

    def test_task8_transient_lease_close_detail_survives_success(self) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000844",
        )
        lease = scratch._lease
        assert lease is not None
        target = lease.fd
        real_close = os.close
        attempts = 0

        def transient_close(descriptor: int) -> None:
            nonlocal attempts
            if descriptor == target and lease.fd == target:
                attempts += 1
                if attempts == 1:
                    raise OSError("injected transient lease close failure")
            real_close(descriptor)

        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["os"],
        )
        with mock.patch.object(
            lease_module.os, "close", side_effect=transient_close
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertEqual(attempts, 2)
        self.assertIn(
            "transient lease close failure", "; ".join(cleanup.details)
        )

    def test_task8_owned_descriptor_budget_and_finalizer_are_separate(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_OwnedDescriptor"],
        )
        read_descriptor, write_descriptor = os.pipe()
        owner = lease_module._OwnedDescriptor()
        owner.adopt(read_descriptor)
        real_close = os.close
        attempts = 0

        def fail_owned_descriptor_close(descriptor: int) -> None:
            nonlocal attempts
            if descriptor == read_descriptor:
                attempts += 1
                raise OSError(
                    f"injected owned descriptor close failure {attempts}"
                )
            real_close(descriptor)

        try:
            with mock.patch.object(
                lease_module.os,
                "close",
                side_effect=fail_owned_descriptor_close,
            ):
                transaction_details = owner.close_retry(
                    "managed test descriptor"
                )
                attempts_after_transaction = attempts
                public_details = owner.close_once("managed test descriptor")
                attempts_after_public = attempts

            self.assertEqual(attempts_after_transaction, 2)
            self.assertEqual(attempts_after_public, 2)
            self.assertEqual(
                getattr(owner, "_close_attempts", None), 2
            )
            self.assertEqual(public_details, transaction_details)
            joined = "; ".join(public_details)
            self.assertIn("owned descriptor close failure 1", joined)
            self.assertIn("owned descriptor close failure 2", joined)
            self.assertNotIn("owned descriptor close failure 3", joined)

            owner.__del__()
            self.assertEqual(getattr(owner, "_close_attempts", None), 3)
            self.assertEqual(owner.fd, -1)
            with self.assertRaises(OSError):
                os.fstat(read_descriptor)
        finally:
            for descriptor in (read_descriptor, write_descriptor):
                try:
                    real_close(descriptor)
                except OSError:
                    pass

    def test_task8_public_close_dedupes_iterator_directory_attempts(
        self,
    ) -> None:
        class AliasedIterator:
            def __init__(self, directory: DirectoryCapability) -> None:
                self.directory = directory
                self.close_callbacks = 0

            def __iter__(self) -> DirectoryIterator:
                return self

            def __next__(self) -> DirectoryEntry:
                raise StopIteration

            def close(self) -> None:
                self.close_callbacks += 1
                self.directory.close()

        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000867",
        )
        child = scratch.create_child("candidate-0001")
        state = scratch._children[child.name]
        identity = state.identity
        assert identity is not None
        backend.close_failures_by_identity[identity] = 3
        owner = scratch.open_child(child.name, SharePolicy.PINNED)
        resource = backend._resource(owner)
        iterator = AliasedIterator(owner)
        graph = scratch._ensure_cleanup_graph()
        graph.walker_iterator.owner = iterator
        self.assertTrue(graph.blocked.retain(owner))

        scratch.close_capabilities()
        attempts_after_first = owner._close_attempts
        callbacks_after_first = iterator.close_callbacks
        scratch.close_capabilities()
        attempts_after_second = owner._close_attempts
        callbacks_after_second = iterator.close_callbacks
        scratch.close_capabilities()
        attempts_after_third = owner._close_attempts
        callbacks_after_third = iterator.close_callbacks

        self.assertEqual(attempts_after_first, 1)
        self.assertEqual(callbacks_after_first, 1)
        self.assertEqual(attempts_after_second, 2)
        self.assertEqual(callbacks_after_second, 2)
        self.assertEqual(attempts_after_third, 2)
        self.assertEqual(callbacks_after_third, 2)
        self.assertTrue(owner.is_open)
        resource.close_failures = 0
        del owner
        del iterator
        scratch.__del__()
        self.assertTrue(resource.closed)

    def test_task8_capability_finalizer_has_one_post_budget_attempt(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000868",
        )
        child = scratch.create_child("candidate-0001")
        state = scratch._children[child.name]
        identity = state.identity
        assert identity is not None
        backend.close_failures_by_identity[identity] = 8
        owner = scratch.open_child(child.name, SharePolicy.PINNED)
        resource = backend._resource(owner)
        baseline = backend.close_counts.get(identity, 0)
        graph = scratch._ensure_cleanup_graph()
        graph.walker_entry.owner = owner
        self.assertTrue(graph.blocked.retain(owner))
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_close_capability_retry"],
        )
        lease_module._close_capability_retry(owner, "managed finalizer test")
        self.assertEqual(backend.close_counts.get(identity, 0) - baseline, 2)

        scratch.__del__()
        graph.walker_entry.owner = None
        for index in range(len(graph.blocked._owners)):
            graph.blocked._owners[index] = None
        del owner
        gc.collect()

        self.assertEqual(backend.close_counts.get(identity, 0) - baseline, 3)
        self.assertFalse(resource.closed)
        resource.close_failures = 0
        backend.close_resource(resource)

    def test_task8_descriptor_finalizers_do_not_repeat_after_failure(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_OwnedDescriptor", "_close_lease_lock_retry"],
        )
        for owner_kind in ("lease", "descriptor"):
            with self.subTest(owner_kind=owner_kind):
                read_descriptor, write_descriptor = os.pipe()
                owner: LeaseLock | _OwnedDescriptor
                if owner_kind == "lease":
                    owner = LeaseLock(read_descriptor)
                else:
                    owner = _OwnedDescriptor()
                    owner.adopt(read_descriptor)
                real_close = os.close
                actual_calls = 0

                def fail_finalizer_close(descriptor: int) -> None:
                    nonlocal actual_calls
                    if descriptor == read_descriptor:
                        actual_calls += 1
                        raise OSError(
                            f"injected {owner_kind} finalizer failure"
                        )
                    real_close(descriptor)

                try:
                    with mock.patch.object(
                        lease_module.os,
                        "close",
                        side_effect=fail_finalizer_close,
                    ):
                        if isinstance(owner, LeaseLock):
                            lease_module._close_lease_lock_retry(
                                owner, "managed finalizer test"
                            )
                        else:
                            owner.close_retry("managed finalizer test")
                        attempts_after_transaction = actual_calls
                        owner.__del__()
                        attempts_after_finalizer = actual_calls
                        owner.__del__()
                        attempts_after_repeat = actual_calls

                    self.assertEqual(attempts_after_transaction, 2)
                    self.assertEqual(attempts_after_finalizer, 3)
                    self.assertEqual(attempts_after_repeat, 3)
                    self.assertEqual(owner._close_attempts, 3)
                finally:
                    try:
                        real_close(read_descriptor)
                    except OSError:
                        pass
                    owner.fd = -1
                    try:
                        real_close(write_descriptor)
                    except OSError:
                        pass

    def test_task8_preallocates_restore_lock_before_namespace_mutation(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000845",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["LeaseLock"],
        )
        real_lock = lease_module.LeaseLock
        namespace_started = False
        allocations_before_namespace = 0

        def note_operation(_operation: str) -> None:
            nonlocal namespace_started
            namespace_started = True

        def allocate_lock(descriptor: int) -> LeaseLock:
            nonlocal allocations_before_namespace
            if namespace_started:
                raise MemoryError(
                    "lease owner allocation occurred after namespace work"
                )
            allocations_before_namespace += 1
            return real_lock(descriptor)

        backend.before_cleanup_operation = note_operation
        with mock.patch.object(
            lease_module, "LeaseLock", side_effect=allocate_lock
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertGreater(allocations_before_namespace, 0)
        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)

    def test_task8_marker_read_close_failure_retains_owner_after_budget(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000846",
        )
        backend.close_failures_by_identity[scratch._lease_identity] = 3
        backend.failures[
            "verify-managed:.hoimin-lease.json:repair=false"
        ] = OSError("injected marker verification failure")

        cleanup = self._task8_cleanup(scratch, backend)
        attempts_after_cleanup = backend.close_counts.get(
            scratch._lease_identity, 0
        )
        scratch.close_capabilities()
        attempts_after_public_close = backend.close_counts.get(
            scratch._lease_identity, 0
        )

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(attempts_after_cleanup, 2)
        self.assertEqual(attempts_after_public_close, 2)
        self.assertEqual(
            "; ".join(cleanup.details).count(
                "managed namespace cleanup unavailable"
            ),
            1,
        )
        retained_resources = [
            resource
            for resource in backend.live_resources
            if not resource.closed
            and resource.node.identity == scratch._lease_identity
        ]
        def close_retained_marker_resources() -> None:
            for resource in retained_resources:
                resource.close_failures = 0
            scratch.__del__()
            for resource in retained_resources:
                if not resource.closed:
                    backend.close_resource(resource)

        self.addCleanup(close_retained_marker_resources)
        self.assertEqual(len(retained_resources), 1, retained_resources)
        for resource in retained_resources:
            resource.close_failures = 0
        scratch.__del__()
        gc.collect()
        self.assertTrue(
            all(resource.closed for resource in retained_resources),
            retained_resources,
        )

    def test_task8_walker_owner_registration_never_allocates_after_failure(
        self,
    ) -> None:
        class FailingOwnerRegistry(
            list[FileCapability | DirectoryCapability]
        ):
            def append(
                self, value: FileCapability | DirectoryCapability
            ) -> None:
                del value
                raise MemoryError("injected owner registry allocation failure")

        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000847",
        )
        root = self._task8_root_node(backend, scratch)
        payload = self._task8_add_payload(backend, root, "payload")
        backend.close_failures_by_identity[payload.identity] = 3
        scratch._cleanup_owned_capabilities = FailingOwnerRegistry()

        def close_fixture_resources() -> None:
            for resource in tuple(backend.live_resources):
                resource.close_failures = 0
            scratch.__del__()
            for resource in tuple(backend.live_resources):
                if not resource.closed:
                    backend.close_resource(resource)

        self.addCleanup(close_fixture_resources)
        try:
            cleanup = self._task8_cleanup(scratch, backend)
        except MemoryError as error:
            self.fail(f"cleanup allocated while retaining an owner: {error}")
        attempts_after_cleanup = backend.close_counts.get(payload.identity, 0)
        scratch.close_capabilities()
        attempts_after_public_close = backend.close_counts.get(
            payload.identity, 0
        )

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(attempts_after_cleanup, 2)
        self.assertEqual(attempts_after_public_close, 2)
        retained_resources = [
            resource
            for resource in backend.live_resources
            if not resource.closed and resource.node.identity == payload.identity
        ]
        self.assertEqual(len(retained_resources), 1, retained_resources)
        for resource in retained_resources:
            resource.close_failures = 0
        scratch.__del__()
        self.assertTrue(
            all(resource.closed for resource in retained_resources),
            retained_resources,
        )

    def test_task8_preallocates_real_coordinator_lock_before_open(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000848",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["LeaseLock"],
        )
        real_lock = lease_module.LeaseLock
        operation_started = False
        allocations_before_operation = 0
        opened: list[LeaseLock] = []

        def note_operation(_operation: str) -> None:
            nonlocal operation_started
            operation_started = True

        def allocate_lock(descriptor: int) -> LeaseLock:
            nonlocal allocations_before_operation
            if operation_started:
                raise MemoryError(
                    "coordinator owner allocation occurred after open"
                )
            allocations_before_operation += 1
            return real_lock(descriptor)

        def acquire_fixture(
            lock: LeaseLock, *, blocking: bool
        ) -> None:
            del blocking
            lock.locked = True

        backend.before_cleanup_operation = note_operation
        self.addCleanup(scratch.__del__)
        with (
            mock.patch.object(
                lease_module, "LeaseLock", side_effect=allocate_lock
            ),
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=acquire_fixture,
            ),
        ):
            try:
                cleanup = self._task8_cleanup_with_real_coordinator(
                    scratch, backend, opened
                )
            except MemoryError as error:
                self.fail(
                    "coordinator owner allocation followed namespace open: "
                    f"{error}"
                )

        self.assertGreater(allocations_before_operation, 0)
        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)

    def test_task8_real_coordinator_close_failure_retains_owner(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000849",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["os"],
        )
        real_close = os.close
        opened: list[LeaseLock] = []
        close_attempts = 0

        def acquire_fixture(
            lock: LeaseLock, *, blocking: bool
        ) -> None:
            del blocking
            lock.locked = True

        def fail_coordinator_close(descriptor: int) -> None:
            nonlocal close_attempts
            if opened and descriptor == opened[0].fd:
                close_attempts += 1
                raise OSError(
                    f"injected coordinator close failure {close_attempts}"
                )
            real_close(descriptor)

        with (
            mock.patch.object(
                lease_module.os,
                "close",
                side_effect=fail_coordinator_close,
            ),
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=acquire_fixture,
            ),
        ):
            cleanup = self._task8_cleanup_with_real_coordinator(
                scratch, backend, opened
            )
            attempts_after_cleanup = close_attempts
            scratch.close_capabilities()
            attempts_after_public_close = close_attempts

        self.assertEqual(len(opened), 1, (opened, cleanup))
        coordinator = opened[0]

        def close_coordinator_fixture() -> None:
            backend.coordinator = None
            coordinator.__del__()
            if coordinator.fd >= 0:
                real_close(coordinator.fd)
                coordinator.fd = -1
                coordinator.locked = False

        self.addCleanup(close_coordinator_fixture)
        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(attempts_after_cleanup, 2)
        self.assertEqual(attempts_after_public_close, 2)
        self.assertGreaterEqual(coordinator.fd, 0)
        joined = "; ".join(cleanup.details)
        self.assertIn("coordinator close failure 1", joined)
        self.assertIn("coordinator close failure 2", joined)
        self.assertEqual(
            joined.count("managed namespace cleanup unavailable"), 1
        )

    def test_task8_claimed_root_is_slotted_before_validation_failure(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000863",
        )
        root = scratch._root
        assert root is not None
        root._share_policy = SharePolicy.MUTATION
        backend.close_failures_by_identity[scratch._root_identity] = 3
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_validate_cleanup_directory"],
        )
        real_validate = lease_module._validate_cleanup_directory
        slot_registered = False

        def fail_claimed_root_validation(
            directory: DirectoryCapability,
            component: object,
            *,
            label: str,
        ) -> None:
            nonlocal slot_registered
            if label != "managed claimed root":
                real_validate(directory, component, label=label)
                return
            graph = scratch._cleanup_graph
            assert graph is not None
            slot_registered = graph.walker_current.owner is directory
            raise OSError("injected claimed-root validation failure")

        with mock.patch.object(
            lease_module,
            "_validate_cleanup_directory",
            side_effect=fail_claimed_root_validation,
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertTrue(slot_registered)
        graph = scratch._cleanup_graph
        assert graph is not None
        retained = [
            owner
            for owner in graph.blocked
            if owner.identity == scratch._root_identity and owner.is_open
        ]
        self.assertEqual(len(retained), 1, retained)
        claimed_root = retained[0]
        self.assertEqual(claimed_root._close_attempts, 2)
        scratch.close_capabilities()
        self.assertEqual(claimed_root._close_attempts, 2)
        resource = backend._resource(claimed_root)
        resource.close_failures = 0
        del claimed_root
        retained.clear()
        scratch.__del__()
        gc.collect()
        self.assertTrue(resource.closed)

    def test_task8_preassignment_restored_lock_blocks_namespace_retry(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000864",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_read_locked_marker"],
        )
        real_close = os.close
        close_attempts = 0

        def fail_restored_lock_close(descriptor: int) -> None:
            nonlocal close_attempts
            graph = scratch._cleanup_graph
            restored = None if graph is None else graph.restored_lease.lock
            if (
                restored is not None
                and restored is not scratch._lease
                and restored.fd == descriptor
            ):
                close_attempts += 1
                raise OSError(
                    f"injected restored lease close failure {close_attempts}"
                )
            real_close(descriptor)

        with (
            mock.patch.object(
                lease_module,
                "_read_locked_marker",
                side_effect=OSError(
                    "injected restored lease validation failure"
                ),
            ),
            mock.patch.object(
                lease_module.os,
                "close",
                side_effect=fail_restored_lock_close,
            ),
        ):
            first = self._task8_cleanup(scratch, backend)
            first_boundary = len(backend.cleanup_operations)
            attempts_after_first = close_attempts
            second = self._task8_cleanup(scratch, backend)
            attempts_after_second = close_attempts

        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        self.assertEqual(second.status, ScratchCleanupStatus.FAILED, second)
        self.assertIsNone(scratch._lease)
        self.assertEqual(attempts_after_first, 2)
        self.assertEqual(attempts_after_second, 2)
        unsafe = backend.cleanup_operations[first_boundary:]
        self.assertFalse(
            any(
                operation.startswith(
                    ("rename:", "delete:", "entry:", "open_")
                )
                for operation in unsafe
            ),
            unsafe,
        )
        graph = scratch._cleanup_graph
        assert graph is not None
        restored = graph.restored_lease.lock
        assert restored is not None
        self.assertGreaterEqual(restored.fd, 0)
        restored_descriptor = restored.fd
        del restored
        scratch.__del__()
        gc.collect()
        with self.assertRaises(OSError):
            os.fstat(restored_descriptor)

    def test_task8_transient_coordinator_close_detail_keeps_clean_status(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000865",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["os"],
        )
        real_close = os.close
        opened: list[LeaseLock] = []
        close_attempts = 0

        def acquire_fixture(
            lock: LeaseLock, *, blocking: bool
        ) -> None:
            del blocking
            lock.locked = True

        def transient_coordinator_close(descriptor: int) -> None:
            nonlocal close_attempts
            if opened and descriptor == opened[0].fd:
                close_attempts += 1
                if close_attempts == 1:
                    raise OSError(
                        "injected transient coordinator close failure"
                    )
            real_close(descriptor)

        with (
            mock.patch.object(
                lease_module.os,
                "close",
                side_effect=transient_coordinator_close,
            ),
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=acquire_fixture,
            ),
        ):
            cleanup = self._task8_cleanup_with_real_coordinator(
                scratch, backend, opened
            )

        backend.coordinator = None
        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertEqual(close_attempts, 2)
        self.assertEqual(len(opened), 2, opened)
        self.assertEqual(
            "; ".join(cleanup.details).count(
                "injected transient coordinator close failure"
            ),
            1,
        )

    def test_task8_exhausted_tail_coordinator_is_failed_and_stops_retry(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000866",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["os"],
        )
        real_close = os.close
        opened: list[LeaseLock] = []
        close_attempts = 0

        def acquire_fixture(
            lock: LeaseLock, *, blocking: bool
        ) -> None:
            del blocking
            lock.locked = True

        def fail_tail_coordinator_close(descriptor: int) -> None:
            nonlocal close_attempts
            if len(opened) >= 2 and descriptor == opened[1].fd:
                close_attempts += 1
                raise OSError(
                    f"injected tail coordinator close failure {close_attempts}"
                )
            real_close(descriptor)

        with (
            mock.patch.object(
                lease_module.os,
                "close",
                side_effect=fail_tail_coordinator_close,
            ),
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=acquire_fixture,
            ),
        ):
            first = self._task8_cleanup_with_real_coordinator(
                scratch, backend, opened
            )
            first_boundary = len(backend.cleanup_operations)
            attempts_after_first = close_attempts
            second = self._task8_cleanup_with_real_coordinator(
                scratch, backend, opened
            )
            attempts_after_second = close_attempts

        self.assertEqual(len(opened), 2, (opened, first, second))
        tail = opened[1]
        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        self.assertEqual(second.status, ScratchCleanupStatus.FAILED, second)
        self.assertEqual(attempts_after_first, 2)
        self.assertEqual(attempts_after_second, 2)
        self.assertGreaterEqual(tail.fd, 0)
        self.assertEqual(
            "; ".join(first.details).count(
                "managed namespace cleanup unavailable"
            ),
            1,
        )
        unsafe = backend.cleanup_operations[first_boundary:]
        self.assertFalse(
            any(
                operation.startswith(
                    ("rename:", "delete:", "entry:", "open_")
                )
                for operation in unsafe
            ),
            unsafe,
        )
        backend.coordinator = None
        tail.__del__()
        self.assertEqual(tail.fd, -1)

    def test_task8_child_cleanup_state_is_registered_before_creation(
        self,
    ) -> None:
        class RecordingRegistry(dict[str, object]):
            def __init__(self, timeline: list[str]) -> None:
                super().__init__()
                self._timeline = timeline

            def __setitem__(self, key: str, value: object) -> None:
                self._timeline.append(f"register:{key}")
                super().__setitem__(key, value)

        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000850",
        )
        timeline: list[str] = []
        registry = RecordingRegistry(timeline)
        setattr(scratch, "_children", registry)

        def record_creation(event: str) -> None:
            if event.startswith("create-directory:candidate-0001:"):
                timeline.append("create:candidate-0001")

        backend.after_event = record_creation
        child = scratch.create_child("candidate-0001")

        self.assertEqual(child.name, "candidate-0001")
        self.assertLess(
            timeline.index("register:candidate-0001"),
            timeline.index("create:candidate-0001"),
        )
        state = registry[child.name]
        for attribute in (
            "identity",
            "filesystem",
            "cursor",
            "root_owner",
            "pending_absence",
        ):
            with self.subTest(attribute=attribute):
                self.assertTrue(hasattr(state, attribute), state)

        failing_backend = self._task8_backend()
        failing_scratch = self._task8_create(
            failing_backend,
            run_id="00000000-0000-4000-8000-000000000851",
        )

        class FailingRegistry(RecordingRegistry):
            def __setitem__(self, key: str, value: object) -> None:
                del key, value
                raise MemoryError("injected child-state registry allocation")

        failing_timeline: list[str] = []
        setattr(
            failing_scratch,
            "_children",
            FailingRegistry(failing_timeline),
        )
        with self.assertRaisesRegex(MemoryError, "registry allocation"):
            failing_scratch.create_child("candidate-0002")
        self.assertFalse(
            any(
                event.startswith("create-directory:candidate-0002:")
                for event in failing_backend.events
            )
        )

    def test_task8_completed_directory_close_preserves_absence_primary(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000831",
        )
        root = self._task8_root_node(backend, scratch)
        level = self._task8_add_payload(
            backend, root, "level", kind=EntryKind.DIRECTORY
        )
        self._task8_add_payload(
            backend, level, "empty", kind=EntryKind.DIRECTORY
        )

        def fail_absence_and_arm_parent(operation: str) -> None:
            if operation != "entry:empty":
                return
            parents = [
                resource
                for resource in backend.live_resources
                if not resource.closed and resource.node is level
            ]
            self.assertEqual(len(parents), 1, parents)
            parents[0].close_failures = 2
            raise OSError("injected completed-directory absence primary")

        backend.before_cleanup_operation = fail_absence_and_arm_parent
        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn(
            "completed-directory absence primary", cleanup.details[0]
        )
        retained = [
            owner
            for owner in scratch._cleanup_owned_capabilities
            if owner.is_open and owner.identity == level.identity
        ]
        self.assertEqual(len(retained), 1, retained)
        self.assertEqual(retained[0]._close_attempts, 2)
        close_indices = [
            index
            for index, operation in enumerate(backend.cleanup_operations)
            if operation == "close:level"
        ]
        self.assertGreaterEqual(len(close_indices), 3, close_indices)
        unsafe_suffix = backend.cleanup_operations[close_indices[-1] + 1 :]
        self.assertFalse(
            any(
                operation.startswith(
                    (
                        "rename:",
                        "delete:",
                        "entry:",
                        "open_entry:",
                        "open_directory:",
                        "entries_owned",
                        "iterator.next:",
                    )
                )
                for operation in unsafe_suffix
            ),
            unsafe_suffix,
        )
        self.assertEqual(
            "; ".join(cleanup.details).count(
                "managed namespace cleanup unavailable"
            ),
            1,
        )

    def test_task8_deadline_stops_before_next_backend_operation(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000807",
        )
        clock = [0.0]
        active_name = scratch.path.name

        def cross_after_identity(operation: str) -> None:
            if operation == f"entry:{active_name}":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_after_identity
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup)
        self.assertFalse(
            any(
                operation.startswith("rename:")
                for operation in backend.cleanup_operations
            )
        )

    def test_task8_open_owned_marker_checks_deadline_between_operations(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=[
                "_DeadlineExceeded",
                "_MarkerOwnerSlot",
                "_open_owned_marker",
            ],
        )
        parameters = inspect.signature(
            lease_module._open_owned_marker
        ).parameters
        self.assertIn("deadline", parameters)
        self.assertIn("monotonic", parameters)
        cases = (
            (
                "open_existing:.hoimin-heartbeat.json:write:pinned",
                "verify-managed:.hoimin-heartbeat.json:repair=false",
            ),
            (
                "verify-managed:.hoimin-heartbeat.json:repair=false",
                "entry:.hoimin-heartbeat.json",
            ),
            ("entry:.hoimin-heartbeat.json", None),
        )
        for index, (crossing_event, forbidden_event) in enumerate(cases):
            with self.subTest(crossing_event=crossing_event):
                backend = self._task8_backend()
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{852 + index:012d}"
                    ),
                )
                root = scratch._root
                assert root is not None
                clock = [0.0]
                start = len(backend.events)

                def cross_after_operation(event: str) -> None:
                    if event == crossing_event:
                        clock[0] = 1.0

                backend.after_event = cross_after_operation
                slot = lease_module._MarkerOwnerSlot()
                with self.assertRaises(lease_module._DeadlineExceeded):
                    lease_module._open_owned_marker(
                        root,
                        ".hoimin-heartbeat.json",
                        backend,
                        access=FileAccess.WRITE,
                        identity=scratch._heartbeat_identity,
                        deadline=1.0,
                        monotonic=lambda: clock[0],
                        _owner_slot=slot,
                    )

                suffix = backend.events[start:]
                self.assertIn(crossing_event, suffix)
                if forbidden_event is not None:
                    crossing_index = suffix.index(crossing_event)
                    self.assertNotIn(
                        forbidden_event, suffix[crossing_index + 1 :]
                    )
                self.assertFalse(slot.has_open_owner())

    def test_task8_locked_marker_read_checks_deadline_between_operations(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=[
                "_DeadlineExceeded",
                "_encoded_marker",
                "_marker",
                "_read_locked_marker",
            ],
        )
        parameters = inspect.signature(
            lease_module._read_locked_marker
        ).parameters
        self.assertIn("deadline", parameters)
        self.assertIn("monotonic", parameters)
        run_id = "00000000-0000-4000-8000-000000000855"
        lease_id = "00000000-0000-4000-8000-000000000856"
        encoded = lease_module._encoded_marker(
            lease_module._marker(run_id, lease_id)
        )

        for crossing in ("lseek", "read"):
            with self.subTest(crossing=crossing):
                with tempfile.TemporaryFile() as stream:
                    stream.write(encoded)
                    stream.flush()
                    descriptor = os.dup(stream.fileno())
                    lock = LeaseLock(descriptor)
                    clock = [0.0]
                    read_calls = 0
                    real_lseek = os.lseek
                    real_read = os.read

                    def crossing_lseek(
                        target: int, offset: int, whence: int
                    ) -> int:
                        result = real_lseek(target, offset, whence)
                        if target == descriptor and crossing == "lseek":
                            clock[0] = 1.0
                        return result

                    def crossing_read(target: int, size: int) -> bytes:
                        nonlocal read_calls
                        if target == descriptor:
                            read_calls += 1
                            if crossing == "lseek":
                                raise AssertionError(
                                    "marker read started after deadline"
                                )
                        result = real_read(target, size)
                        if target == descriptor and crossing == "read":
                            clock[0] = 1.0
                        return result

                    with (
                        mock.patch.object(
                            lease_module.os,
                            "lseek",
                            side_effect=crossing_lseek,
                        ),
                        mock.patch.object(
                            lease_module.os,
                            "read",
                            side_effect=crossing_read,
                        ),
                    ):
                        with self.assertRaises(
                            lease_module._DeadlineExceeded
                        ):
                            lease_module._read_locked_marker(
                                lock,
                                expected_run_id=run_id,
                                expected_lease_id=lease_id,
                                deadline=1.0,
                                monotonic=lambda: clock[0],
                            )

                    self.assertEqual(
                        read_calls, 0 if crossing == "lseek" else 1
                    )
                    self.assertEqual(
                        _close_lease_lock_all(lock, "deadline fixture"), ()
                    )

    def test_task8_windows_restore_stops_before_lock_after_detach_deadline(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000857",
        )
        clock = [0.0]
        detach_count = 0

        def cross_after_detach(event: str) -> None:
            nonlocal detach_count
            if event != "detach:.hoimin-lease.json":
                return
            detach_count += 1
            if detach_count == 2:
                clock[0] = 60.0

        def forbid_lock(
            _lock: LeaseLock, *, blocking: bool
        ) -> None:
            del blocking
            raise AssertionError("lease lock started after deadline")

        backend.after_event = cross_after_detach
        with (
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=forbid_lock,
            ) as acquire,
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup)
        acquire.assert_not_called()

    def test_task8_windows_restore_stops_before_read_after_lock_deadline(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000858",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_read_locked_marker"],
        )
        clock = [0.0]
        real_acquire = LeaseLock.acquire

        def cross_after_lock(lock: LeaseLock, *, blocking: bool) -> None:
            real_acquire(lock, blocking=blocking)
            clock[0] = 60.0

        def forbid_read(*_args: object, **_kwargs: object) -> object:
            raise AssertionError("locked marker read started after deadline")

        with (
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
            mock.patch.object(
                LeaseLock,
                "acquire",
                autospec=True,
                side_effect=cross_after_lock,
            ),
            mock.patch.object(
                lease_module,
                "_read_locked_marker",
                side_effect=forbid_read,
            ) as locked_read,
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup)
        locked_read.assert_not_called()

    def test_task8_host_timeout_remains_failed_after_clock_crosses(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000808",
        )
        active_name = scratch.path.name
        clock = [0.0]
        backend.failures[f"entry:{active_name}"] = TimeoutError(
            "filesystem ETIMEDOUT"
        )

        def cross_during_identity(operation: str) -> None:
            if operation == f"entry:{active_name}":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_during_identity
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn("ETIMEDOUT", "; ".join(cleanup.details))

    def test_task8_absence_is_checked_only_after_owner_close(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000809",
        )
        deleting_name = f".deleting-{scratch.run_id}"

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        delete_index = max(
            index
            for index, operation in enumerate(backend.cleanup_operations)
            if operation == f"delete:{deleting_name}"
        )
        close_index = next(
            index
            for index, operation in enumerate(backend.cleanup_operations)
            if index > delete_index
            and operation == f"close:{deleting_name}"
        )
        absence_index = next(
            index
            for index, operation in enumerate(backend.cleanup_operations)
            if index > close_index
            and operation == f"entry:{deleting_name}"
        )
        self.assertLess(delete_index, close_index)
        self.assertLess(close_index, absence_index)

    def test_task8_post_consume_payload_replacement_is_absence_only(
        self,
    ) -> None:
        for index, kind in enumerate(
            (EntryKind.REGULAR, EntryKind.REPARSE, EntryKind.OTHER),
            start=1,
        ):
            with self.subTest(kind=kind):
                backend = self._task8_backend()
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{834 + index:012d}"
                    ),
                )
                root = self._task8_root_node(backend, scratch)
                original = self._task8_add_payload(
                    backend, root, "payload", kind=kind
                )
                real_delete = backend.delete
                replacement: _ManagedRecordedNode | None = None

                def fail_after_consuming_delete(
                    capability: FileCapability | DirectoryCapability,
                ) -> None:
                    nonlocal replacement
                    node = backend._resource(capability).node
                    real_delete(capability)
                    if node is original and replacement is None:
                        replacement = self._task8_same_identity_replacement(
                            backend, original
                        )
                        raise OSError(
                            "injected payload post-consume absence failure"
                        )

                with mock.patch.object(
                    backend,
                    "delete",
                    side_effect=fail_after_consuming_delete,
                ):
                    first = self._task8_cleanup(scratch, backend)
                    retry_start = len(backend.cleanup_operations)
                    second = self._task8_cleanup(scratch, backend)

                self.assertEqual(
                    first.status, ScratchCleanupStatus.FAILED, first
                )
                self.assertEqual(
                    second.status, ScratchCleanupStatus.FAILED, second
                )
                assert replacement is not None
                self.assertIs(root.children["payload"], replacement)
                retry = backend.cleanup_operations[retry_start:]
                self.assertIn("entry:payload", retry)
                self.assertNotIn("open_entry:payload", retry)
                self.assertNotIn("delete:payload", retry)

    def test_task8_post_consume_completed_directory_is_absence_only(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000838",
        )
        root = self._task8_root_node(backend, scratch)
        original = self._task8_add_payload(
            backend, root, "empty", kind=EntryKind.DIRECTORY
        )
        real_delete = backend.delete
        replacement: _ManagedRecordedNode | None = None
        sentinel: _ManagedRecordedNode | None = None

        def fail_after_consuming_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal replacement, sentinel
            node = backend._resource(capability).node
            real_delete(capability)
            if node is original and replacement is None:
                replacement = self._task8_same_identity_replacement(
                    backend, original
                )
                sentinel = self._task8_add_payload(
                    backend, replacement, "sentinel"
                )
                raise OSError(
                    "injected directory post-consume absence failure"
                )

        with mock.patch.object(
            backend, "delete", side_effect=fail_after_consuming_delete
        ):
            first = self._task8_cleanup(scratch, backend)
            retry_start = len(backend.cleanup_operations)
            second = self._task8_cleanup(scratch, backend)

        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        self.assertEqual(second.status, ScratchCleanupStatus.FAILED, second)
        assert replacement is not None
        assert sentinel is not None
        self.assertIs(root.children["empty"], replacement)
        self.assertIs(replacement.children["sentinel"], sentinel)
        retry = backend.cleanup_operations[retry_start:]
        self.assertIn("entry:empty", retry)
        self.assertNotIn("open_directory:empty", retry)
        self.assertNotIn("delete:empty", retry)

    def test_task8_post_consume_child_root_is_absence_only(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000839",
        )
        child_path = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        original = root.children[child_path.name]
        real_delete = backend.delete
        replacement: _ManagedRecordedNode | None = None
        sentinel: _ManagedRecordedNode | None = None

        def fail_after_consuming_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal replacement, sentinel
            node = backend._resource(capability).node
            real_delete(capability)
            if node is original and replacement is None:
                replacement = self._task8_same_identity_replacement(
                    backend, original
                )
                sentinel = self._task8_add_payload(
                    backend, replacement, "sentinel"
                )
                raise OSError(
                    "injected child-root post-consume absence failure"
                )

        with mock.patch.object(
            backend, "delete", side_effect=fail_after_consuming_delete
        ):
            first = scratch.remove_child(child_path)
            retry_start = len(backend.cleanup_operations)
            second = scratch.remove_child(child_path)

        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        self.assertEqual(second.status, ScratchCleanupStatus.FAILED, second)
        assert replacement is not None
        assert sentinel is not None
        self.assertIs(root.children[child_path.name], replacement)
        self.assertIs(replacement.children["sentinel"], sentinel)
        retry = backend.cleanup_operations[retry_start:]
        self.assertIn(f"entry:{child_path.name}", retry)
        self.assertNotIn(f"open_directory:{child_path.name}", retry)
        self.assertNotIn(f"delete:{child_path.name}", retry)

    def test_task8_post_consume_tail_marker_is_absence_only(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000840",
        )
        root_owner = scratch._root
        assert root_owner is not None
        marker_name = ".hoimin-cleanup-ready.json"
        marker_owner = backend.open_file(
            root_owner,
            marker_name,
            access=FileAccess.WRITE,
            disposition=CreateDisposition.CREATE_NEW,
            share_policy=SharePolicy.MUTATION,
        )
        marker_owner.close()
        root = self._task8_root_node(backend, scratch)
        original = root.children[marker_name]
        real_delete = backend.delete
        replacement: _ManagedRecordedNode | None = None

        def fail_after_consuming_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal replacement
            node = backend._resource(capability).node
            real_delete(capability)
            if node is original and replacement is None:
                replacement = self._task8_same_identity_replacement(
                    backend, original
                )
                raise OSError(
                    "injected marker post-consume absence failure"
                )

        with mock.patch.object(
            backend, "delete", side_effect=fail_after_consuming_delete
        ):
            first = self._task8_cleanup(scratch, backend)
            retry_start = len(backend.cleanup_operations)
            second = self._task8_cleanup(scratch, backend)

        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        self.assertEqual(second.status, ScratchCleanupStatus.FAILED, second)
        assert replacement is not None
        self.assertIs(root.children[marker_name], replacement)
        retry = backend.cleanup_operations[retry_start:]
        self.assertIn(f"entry:{marker_name}", retry)
        self.assertNotIn(f"open_entry:{marker_name}", retry)
        self.assertNotIn(f"delete:{marker_name}", retry)

    def test_task8_post_consume_run_root_failure_stays_failed(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000841",
        )
        original = self._task8_root_node(backend, scratch)
        deleting_name = f".deleting-{scratch.run_id}"
        real_delete = backend.delete
        replacement: _ManagedRecordedNode | None = None
        sentinel: _ManagedRecordedNode | None = None

        def fail_after_consuming_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal replacement, sentinel
            node = backend._resource(capability).node
            real_delete(capability)
            if node is original and replacement is None:
                replacement = self._task8_same_identity_replacement(
                    backend, original
                )
                sentinel = self._task8_add_payload(
                    backend, replacement, "sentinel"
                )
                raise OSError(
                    "injected run-root post-consume absence failure"
                )

        with mock.patch.object(
            backend, "delete", side_effect=fail_after_consuming_delete
        ):
            first = self._task8_cleanup(scratch, backend)
            retry_start = len(backend.cleanup_operations)
            second = self._task8_cleanup(scratch, backend)

        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        self.assertEqual(second.status, ScratchCleanupStatus.FAILED, second)
        managed = backend.parent.children["hoimin-focused-v1"]
        assert replacement is not None
        assert sentinel is not None
        self.assertIs(managed.children[deleting_name], replacement)
        self.assertIs(replacement.children["sentinel"], sentinel)
        retry = backend.cleanup_operations[retry_start:]
        self.assertIn(f"entry:{deleting_name}", retry)
        self.assertNotIn(f"open_directory:{deleting_name}", retry)
        self.assertNotIn(f"delete:{deleting_name}", retry)

    def test_task8_windows_pending_control_marker_retry_is_absence_only(
        self,
    ) -> None:
        for index, marker_name in enumerate(
            (
                ".hoimin-heartbeat.json",
                ".hoimin-lease.json",
            )
        ):
            with self.subTest(marker_name=marker_name):
                backend = self._task8_backend(
                    rename_requires_closed_descendants=True
                )
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{870 + index:012d}"
                    ),
                )
                real_delete = backend.delete
                injected = False

                def fail_after_consuming_marker(
                    capability: FileCapability | DirectoryCapability,
                ) -> None:
                    nonlocal injected
                    node = backend._resource(capability).node
                    real_delete(capability)
                    if node.name == marker_name and not injected:
                        injected = True
                        raise OSError(
                            "injected control-marker post-consume failure"
                        )

                with mock.patch.object(
                    backend,
                    "delete",
                    side_effect=fail_after_consuming_marker,
                ):
                    first = self._task8_cleanup(scratch, backend)
                    retry_start = len(backend.cleanup_operations)
                    second = self._task8_cleanup(scratch, backend)

                self.assertTrue(injected)
                self.assertEqual(
                    first.status, ScratchCleanupStatus.FAILED, first
                )
                self.assertEqual(
                    second.status, ScratchCleanupStatus.CLEAN, second
                )
                retry = backend.cleanup_operations[retry_start:]
                self.assertIn(f"entry:{marker_name}", retry)
                self.assertNotIn(f"open_file:{marker_name}", retry)
                self.assertNotIn(f"open_entry:{marker_name}", retry)
                self.assertNotIn(f"delete:{marker_name}", retry)

    def test_task8_windows_removed_control_marker_retry_skips_restore(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_CleanupOwnerGraph"],
        )
        graph_type = lease_module._CleanupOwnerGraph
        real_mark_removed = graph_type.mark_marker_removed
        for index, marker_name in enumerate(
            (
                ".hoimin-heartbeat.json",
                ".hoimin-lease.json",
            )
        ):
            with self.subTest(marker_name=marker_name):
                backend = self._task8_backend(
                    rename_requires_closed_descendants=True
                )
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{872 + index:012d}"
                    ),
                )
                clock = [0.0]
                crossed = False

                def cross_after_removed(
                    graph: object, removed_name: str
                ) -> None:
                    nonlocal crossed
                    real_mark_removed(graph, removed_name)
                    if removed_name == marker_name and not crossed:
                        crossed = True
                        clock[0] = 60.0

                with (
                    mock.patch(
                        "tools.focused_mutation_support.lease.time.monotonic",
                        side_effect=lambda: clock[0],
                    ),
                    mock.patch.object(
                        graph_type,
                        "mark_marker_removed",
                        autospec=True,
                        side_effect=cross_after_removed,
                    ),
                ):
                    first = self._task8_cleanup(scratch, backend)
                    retry_start = len(backend.cleanup_operations)
                    clock[0] = 0.0
                    second = self._task8_cleanup(scratch, backend)

                self.assertTrue(crossed)
                self.assertEqual(
                    first.status, ScratchCleanupStatus.DEFERRED, first
                )
                self.assertEqual(
                    second.status, ScratchCleanupStatus.CLEAN, second
                )
                retry = backend.cleanup_operations[retry_start:]
                self.assertNotIn(f"entry:{marker_name}", retry)
                self.assertNotIn(f"open_file:{marker_name}", retry)
                self.assertNotIn(f"open_entry:{marker_name}", retry)
                self.assertNotIn(f"delete:{marker_name}", retry)

    def test_task8_child_pending_retry_preserves_removed_count(self) -> None:
        for index, replacement_expected in enumerate((False, True)):
            with self.subTest(replacement=replacement_expected):
                backend = self._task8_backend()
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{874 + index:012d}"
                    ),
                )
                child_path = scratch.create_child("candidate-0001")
                root = self._task8_root_node(backend, scratch)
                original = root.children[child_path.name]
                real_delete = backend.delete
                injected = False
                replacement: _ManagedRecordedNode | None = None

                def fail_after_consuming_child(
                    capability: FileCapability | DirectoryCapability,
                ) -> None:
                    nonlocal injected, replacement
                    node = backend._resource(capability).node
                    real_delete(capability)
                    if node is original and not injected:
                        injected = True
                        if replacement_expected:
                            replacement = (
                                self._task8_same_identity_replacement(
                                    backend, original
                                )
                            )
                            self._task8_add_payload(
                                backend, replacement, "sentinel"
                            )
                        raise OSError(
                            "injected child post-consume count failure"
                        )

                with mock.patch.object(
                    backend,
                    "delete",
                    side_effect=fail_after_consuming_child,
                ):
                    first = scratch.remove_child(child_path)
                    retry_start = len(backend.cleanup_operations)
                    second = scratch.remove_child(child_path)

                self.assertTrue(injected)
                self.assertEqual(
                    first.status, ScratchCleanupStatus.FAILED, first
                )
                self.assertEqual(first.removed_entries, 1, first)
                self.assertEqual(
                    second.status,
                    (
                        ScratchCleanupStatus.FAILED
                        if replacement_expected
                        else ScratchCleanupStatus.CLEAN
                    ),
                    second,
                )
                self.assertEqual(second.removed_entries, 1, second)
                retry = backend.cleanup_operations[retry_start:]
                self.assertEqual(
                    retry.count(f"entry:{child_path.name}"), 1, retry
                )
                self.assertNotIn(f"open_directory:{child_path.name}", retry)
                self.assertNotIn(f"delete:{child_path.name}", retry)
                if replacement_expected:
                    assert replacement is not None
                    self.assertIs(root.children[child_path.name], replacement)
                    self.assertIn("sentinel", replacement.children)

    def test_task8_root_pending_retry_preserves_removed_count(self) -> None:
        for index, replacement_expected in enumerate((False, True)):
            with self.subTest(replacement=replacement_expected):
                backend = self._task8_backend()
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{876 + index:012d}"
                    ),
                )
                original = self._task8_root_node(backend, scratch)
                deleting_name = f".deleting-{scratch.run_id}"
                real_delete = backend.delete
                injected = False
                replacement: _ManagedRecordedNode | None = None

                def fail_after_consuming_root(
                    capability: FileCapability | DirectoryCapability,
                ) -> None:
                    nonlocal injected, replacement
                    node = backend._resource(capability).node
                    real_delete(capability)
                    if node is original and not injected:
                        injected = True
                        if replacement_expected:
                            replacement = (
                                self._task8_same_identity_replacement(
                                    backend, original
                                )
                            )
                            self._task8_add_payload(
                                backend, replacement, "sentinel"
                            )
                        raise OSError(
                            "injected root post-consume count failure"
                        )

                with mock.patch.object(
                    backend,
                    "delete",
                    side_effect=fail_after_consuming_root,
                ):
                    first = self._task8_cleanup(scratch, backend)
                    retry_start = len(backend.cleanup_operations)
                    second = self._task8_cleanup(scratch, backend)

                self.assertTrue(injected)
                self.assertEqual(
                    first.status, ScratchCleanupStatus.FAILED, first
                )
                self.assertEqual(first.removed_entries, 3, first)
                self.assertEqual(
                    second.status,
                    (
                        ScratchCleanupStatus.FAILED
                        if replacement_expected
                        else ScratchCleanupStatus.CLEAN
                    ),
                    second,
                )
                self.assertEqual(second.removed_entries, 3, second)
                retry = backend.cleanup_operations[retry_start:]
                self.assertEqual(
                    retry.count(f"entry:{deleting_name}"), 1, retry
                )
                self.assertNotIn(f"open_directory:{deleting_name}", retry)
                self.assertNotIn(f"delete:{deleting_name}", retry)
                if replacement_expected:
                    assert replacement is not None
                    managed = backend.parent.children["hoimin-focused-v1"]
                    self.assertIs(
                        managed.children[deleting_name], replacement
                    )
                    self.assertIn("sentinel", replacement.children)

    def test_task8_bounded_iterator_close_detail_reaches_deferred_record(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000881",
        )
        root = self._task8_root_node(backend, scratch)
        backend.iterator_auto_close = False
        backend.iterator_close_failures_by_identity[root.identity] = 1
        clock = [0.0]

        def cross_after_iterator_open(operation: str) -> None:
            if operation == "entries_owned":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_after_iterator_open
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        close_detail = (
            "cleanup iterator close failed: OSError: "
            f"injected close failure for .deleting-{scratch.run_id}"
        )
        self.assertEqual(
            cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
        )
        self.assertEqual(cleanup.details.count(close_detail), 1, cleanup)

    def test_task8_normal_iterator_exhaustion_close_detail_reaches_root_record(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000882",
        )
        root = self._task8_root_node(backend, scratch)
        backend.iterator_auto_close = False
        backend.iterator_close_failures_by_identity[root.identity] = 1

        cleanup = self._task8_cleanup(scratch, backend)

        close_detail = (
            "cleanup completed iterator close failed: OSError: "
            f"injected close failure for .deleting-{scratch.run_id}"
        )
        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertEqual(cleanup.details.count(close_detail), 1, cleanup)

    def test_task8_normal_iterator_exhaustion_close_detail_reaches_child_record(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000883",
        )
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        child_node = root.children[child.name]
        backend.iterator_auto_close = False
        backend.iterator_close_failures_by_identity[child_node.identity] = 1

        cleanup = scratch.remove_child(child)

        close_detail = (
            "cleanup completed iterator close failed: OSError: "
            f"injected close failure for {child.name}"
        )
        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertEqual(cleanup.details.count(close_detail), 1, cleanup)

    def test_task8_iterator_operational_failure_keeps_transient_close_secondary(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000884",
        )
        root = self._task8_root_node(backend, scratch)
        level = self._task8_add_payload(
            backend, root, "level", kind=EntryKind.DIRECTORY
        )
        backend.iterator_auto_close = False
        backend.iterator_close_failures_by_identity[level.identity] = 1
        real_delete = backend.delete

        def fail_completed_directory_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            if capability.identity == level.identity:
                raise OSError("injected completed-directory delete primary")
            real_delete(capability)

        with mock.patch.object(
            backend,
            "delete",
            side_effect=fail_completed_directory_delete,
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        close_detail = (
            "cleanup completed iterator close failed: OSError: "
            "injected close failure for level"
        )
        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn(
            "injected completed-directory delete primary",
            cleanup.details[0],
        )
        self.assertEqual(cleanup.details[1:].count(close_detail), 1, cleanup)

    def test_task8_child_recovery_deadline_keeps_original_close_note(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000885",
        )
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        child_node = root.children[child.name]
        backend.close_failures_by_identity[child_node.identity] = 1
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_validate_cleanup_directory"],
        )
        real_validate = lease_module._validate_cleanup_directory
        vanished = FileNotFoundError("injected child validation vanished")
        clock = [0.0]

        def fail_managed_child_validation(
            *args: object, **kwargs: object
        ) -> None:
            if kwargs.get("label") == "managed child":
                raise vanished
            real_validate(*args, **kwargs)

        def cross_during_child_close(operation: str) -> None:
            if operation == f"close:{child.name}":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_during_child_close
        with (
            mock.patch.object(
                lease_module,
                "_validate_cleanup_directory",
                side_effect=fail_managed_child_validation,
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
        ):
            cleanup = scratch.remove_child(child)

        close_detail = (
            "managed child close failed: OSError: "
            f"injected close failure for {child.name}"
        )
        self.assertEqual(
            cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
        )
        self.assertEqual(
            cleanup.details[0], "managed child recovery deadline reached"
        )
        self.assertEqual(cleanup.details[1:].count(close_detail), 1, cleanup)

    def test_task8_child_recovery_error_keeps_original_close_note(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000886",
        )
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        child_node = root.children[child.name]
        backend.close_failures_by_identity[child_node.identity] = 1
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_validate_cleanup_directory"],
        )
        real_validate = lease_module._validate_cleanup_directory
        vanished = FileNotFoundError("injected child validation vanished")
        entry_calls = 0

        def fail_managed_child_validation(
            *args: object, **kwargs: object
        ) -> None:
            if kwargs.get("label") == "managed child":
                raise vanished
            real_validate(*args, **kwargs)

        def fail_recovery_evidence(operation: str) -> None:
            nonlocal entry_calls
            if operation != f"entry:{child.name}":
                return
            entry_calls += 1
            if entry_calls == 2:
                raise OSError("injected child recovery evidence failure")

        backend.before_cleanup_operation = fail_recovery_evidence
        with mock.patch.object(
            lease_module,
            "_validate_cleanup_directory",
            side_effect=fail_managed_child_validation,
        ):
            cleanup = scratch.remove_child(child)

        close_detail = (
            "managed child close failed: OSError: "
            f"injected close failure for {child.name}"
        )
        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn(
            "injected child recovery evidence failure", cleanup.details[0]
        )
        self.assertEqual(cleanup.details[1:].count(close_detail), 1, cleanup)

    def test_task8_marker_helper_close_note_reaches_cleanup_record(
        self,
    ) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000878",
        )
        backend.close_failures_by_identity[scratch._lease_identity] = 1
        backend.failures[
            "verify-managed:.hoimin-lease.json:repair=false"
        ] = OSError("injected marker-read primary")

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn("injected marker-read primary", cleanup.details[0])
        joined = "; ".join(cleanup.details[1:])
        self.assertEqual(
            joined.count("injected close failure for .hoimin-lease.json"),
            1,
            cleanup,
        )
        self.assertTrue(
            all(
                len(detail.encode("utf-8"))
                <= MAX_DIAGNOSTIC_DETAIL_BYTES
                for detail in cleanup.details
            ),
            cleanup,
        )
        self.assertEqual(cleanup.omitted_detail_count, 0, cleanup)

    def test_task8_walker_helper_close_note_reaches_cleanup_record(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000879",
        )
        root = self._task8_root_node(backend, scratch)
        payload = self._task8_add_payload(backend, root, "payload")
        backend.close_failures_by_identity[payload.identity] = 1
        real_delete = backend.delete
        primary = OSError("injected walker delete primary")
        close_note = (
            "cleanup entry payload close failed: OSError: "
            "injected close failure for payload"
        )
        primary.add_note(close_note)

        def fail_payload_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            if capability.identity == payload.identity:
                raise primary
            real_delete(capability)

        with mock.patch.object(
            backend, "delete", side_effect=fail_payload_delete
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn("injected walker delete primary", cleanup.details[0])
        self.assertEqual(cleanup.details[1:].count(close_note), 1, cleanup)
        self.assertTrue(
            all(
                len(detail.encode("utf-8"))
                <= MAX_DIAGNOSTIC_DETAIL_BYTES
                for detail in cleanup.details
            ),
            cleanup,
        )
        self.assertEqual(cleanup.omitted_detail_count, 0, cleanup)

    def test_task8_initial_coordinator_helper_close_note_reaches_cleanup_record(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000880",
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_bounded_secondary"],
        )
        managed = backend.parent.children["hoimin-focused-v1"]
        coordinator = managed.children[".hoimin-coordinator"]
        primary = OSError("injected initial coordinator primary")
        long_note = "raw overlong coordinator note: " + (
            "x" * (MAX_DIAGNOSTIC_DETAIL_BYTES * 2)
        )
        close_error = OSError(
            "y" * (MAX_DIAGNOSTIC_DETAIL_BYTES * 2)
        )
        close_note = lease_module._bounded_secondary(
            "managed coordinator close failed", close_error
        )
        primary.add_note(long_note)
        primary.add_note(close_note)
        for note_index in range(MAX_DIAGNOSTIC_DETAILS - 2):
            primary.add_note(f"coordinator secondary {note_index:03d}")
        primary.add_note("coordinator omitted secondary 0")
        primary.add_note("coordinator omitted secondary 1")
        backend.failures[
            "verify-managed:.hoimin-coordinator:repair=true"
        ] = primary
        real_close_resource = backend.close_resource
        close_attempts = 0

        def fail_first_coordinator_close(resource: object) -> None:
            nonlocal close_attempts
            if (
                isinstance(resource, _ManagedRecordedResource)
                and resource.node is coordinator
            ):
                close_attempts += 1
                if close_attempts == 1:
                    raise close_error
            real_close_resource(resource)

        with mock.patch.object(
            backend,
            "close_resource",
            side_effect=fail_first_coordinator_close,
        ):
            cleanup = self._task8_cleanup_with_real_coordinator(
                scratch, backend, []
            )

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(close_attempts, 2)
        self.assertIn(
            "injected initial coordinator primary", cleanup.details[0]
        )
        self.assertEqual(cleanup.details[1:].count(close_note), 1, cleanup)
        self.assertEqual(
            len(cleanup.details), MAX_DIAGNOSTIC_DETAILS, cleanup
        )
        self.assertEqual(cleanup.omitted_detail_count, 3, cleanup)
        self.assertNotIn("coordinator secondary 253", cleanup.details)
        self.assertNotIn("coordinator omitted secondary 0", cleanup.details)
        self.assertNotIn("coordinator omitted secondary 1", cleanup.details)
        self.assertEqual(cleanup.details[-1], "coordinator secondary 252")
        self.assertTrue(
            all(
                len(detail.encode("utf-8"))
                <= MAX_DIAGNOSTIC_DETAIL_BYTES
                for detail in cleanup.details
            ),
            cleanup,
        )
        self.assertTrue(cleanup.details[1].endswith("..."), cleanup)

    def test_task8_full_ledger_counts_continuing_close_omission_once(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000887",
        )
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        child_node = root.children[child.name]
        completed = self._task8_add_payload(
            backend, child_node, "empty", kind=EntryKind.DIRECTORY
        )
        backend.iterator_auto_close = False
        backend.iterator_close_failures_by_identity[completed.identity] = 1
        graph = scratch._ensure_cleanup_graph()
        for detail_index in range(MAX_DIAGNOSTIC_DETAILS):
            graph.details.add(f"preexisting detail {detail_index:03d}")
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_FixedDetailLedger", "_bounded_diagnostic_detail"],
        )
        ledger_type = lease_module._FixedDetailLedger
        real_add = ledger_type.add
        close_detail = (
            "cleanup completed iterator close failed: OSError: "
            "injected close failure for empty"
        )
        close_detail_adds = 0
        clock = [0.0]

        def record_add(ledger: object, detail: str) -> None:
            nonlocal close_detail_adds
            if lease_module._bounded_diagnostic_detail(detail) == close_detail:
                close_detail_adds += 1
            real_add(ledger, detail)

        def cross_after_completed_delete(operation: str) -> None:
            if operation == "delete:empty":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_after_completed_delete
        with (
            mock.patch.object(
                ledger_type,
                "add",
                autospec=True,
                side_effect=record_add,
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
        ):
            cleanup = scratch.remove_child(child)

        self.assertEqual(
            cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
        )
        self.assertEqual(
            cleanup.details[0], "managed child cleanup deadline reached"
        )
        self.assertEqual(len(cleanup.details), MAX_DIAGNOSTIC_DETAILS)
        self.assertNotIn(close_detail, cleanup.details)
        self.assertEqual(close_detail_adds, 1)
        self.assertEqual(cleanup.omitted_detail_count, 2, cleanup)

    def test_task8_child_pending_retry_preserves_traversal_details(
        self,
    ) -> None:
        for index, replacement_expected in enumerate((False, True)):
            with self.subTest(replacement=replacement_expected):
                backend = self._task8_backend()
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{888 + index:012d}"
                    ),
                )
                child = scratch.create_child("candidate-0001")
                root = self._task8_root_node(backend, scratch)
                original = root.children[child.name]
                backend.iterator_auto_close = False
                backend.iterator_close_failures_by_identity[
                    original.identity
                ] = 1
                real_delete = backend.delete
                replacement: _ManagedRecordedNode | None = None

                def fail_after_consuming_child(
                    capability: FileCapability | DirectoryCapability,
                ) -> None:
                    nonlocal replacement
                    node = backend._resource(capability).node
                    real_delete(capability)
                    if node is original:
                        if replacement_expected:
                            replacement = self._task8_same_identity_replacement(
                                backend, original
                            )
                            self._task8_add_payload(
                                backend, replacement, "sentinel"
                            )
                        raise OSError(
                            "injected child post-consume detail failure"
                        )

                with mock.patch.object(
                    backend,
                    "delete",
                    side_effect=fail_after_consuming_child,
                ):
                    first = scratch.remove_child(child)
                    retry_start = len(backend.cleanup_operations)
                    second = scratch.remove_child(child)

                close_detail = (
                    "cleanup completed iterator close failed: OSError: "
                    f"injected close failure for {child.name}"
                )
                self.assertEqual(
                    first.status, ScratchCleanupStatus.FAILED, first
                )
                self.assertEqual(first.removed_entries, 1, first)
                self.assertEqual(first.details[1:].count(close_detail), 1, first)
                self.assertEqual(
                    second.status,
                    (
                        ScratchCleanupStatus.FAILED
                        if replacement_expected
                        else ScratchCleanupStatus.CLEAN
                    ),
                    second,
                )
                self.assertEqual(second.removed_entries, 1, second)
                self.assertEqual(second.omitted_detail_count, 0, second)
                if replacement_expected:
                    self.assertEqual(
                        second.details[0],
                        "managed child was replaced after exact removal",
                    )
                    self.assertEqual(
                        second.details[1:].count(close_detail), 1, second
                    )
                    assert replacement is not None
                    self.assertIs(root.children[child.name], replacement)
                    self.assertIn("sentinel", replacement.children)
                else:
                    self.assertEqual(second.details, (close_detail,), second)
                retry = backend.cleanup_operations[retry_start:]
                self.assertEqual(
                    retry.count(f"entry:{child.name}"), 1, retry
                )
                self.assertNotIn(f"open_directory:{child.name}", retry)
                self.assertNotIn(f"delete:{child.name}", retry)

    def test_task8_child_pending_retry_preserves_consumed_owner_close_detail(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000890",
        )
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        state = scratch._children[child.name]
        identity = state.identity
        filesystem = state.filesystem
        assert identity is not None
        assert filesystem is not None
        backend.close_failures_by_identity[identity] = 1
        consumed_owner = scratch.open_child(child.name, SharePolicy.PINNED)
        state.root_owner.owner = consumed_owner
        state.pending_absence.arm(
            scope="child",
            name=child.name,
            identity=identity,
            filesystem=filesystem,
            removed_after=1,
        )
        state.pending_absence.commit()
        del root.children[child.name]
        baseline_close_count = backend.close_counts.get(identity, 0)

        cleanup = scratch.remove_child(child)

        close_detail = (
            "managed child consumed owner close failed: OSError: "
            f"injected close failure for {child.name}"
        )
        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertEqual(cleanup.removed_entries, 1, cleanup)
        self.assertEqual(cleanup.details, (close_detail,), cleanup)
        self.assertEqual(cleanup.omitted_detail_count, 0, cleanup)
        self.assertEqual(
            backend.close_counts.get(identity, 0) - baseline_close_count, 2
        )
        self.assertNotIn(child.name, scratch._children)

    def test_task8_full_ledger_operational_failure_ingests_close_once(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000891",
        )
        root = self._task8_root_node(backend, scratch)
        level = self._task8_add_payload(
            backend, root, "level", kind=EntryKind.DIRECTORY
        )
        backend.iterator_auto_close = False
        backend.iterator_close_failures_by_identity[level.identity] = 1
        graph = scratch._ensure_cleanup_graph()
        for detail_index in range(MAX_DIAGNOSTIC_DETAILS):
            graph.details.add(f"preexisting detail {detail_index:03d}")
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_FixedDetailLedger", "_bounded_diagnostic_detail"],
        )
        ledger_type = lease_module._FixedDetailLedger
        real_add = ledger_type.add
        close_detail = (
            "cleanup completed iterator close failed: OSError: "
            "injected close failure for level"
        )
        close_detail_adds = 0
        real_delete = backend.delete

        def record_add(ledger: object, detail: str) -> None:
            nonlocal close_detail_adds
            if lease_module._bounded_diagnostic_detail(detail) == close_detail:
                close_detail_adds += 1
            real_add(ledger, detail)

        def fail_completed_directory_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            if capability.identity == level.identity:
                raise OSError("injected saturated directory delete primary")
            real_delete(capability)

        with (
            mock.patch.object(
                ledger_type,
                "add",
                autospec=True,
                side_effect=record_add,
            ),
            mock.patch.object(
                backend,
                "delete",
                side_effect=fail_completed_directory_delete,
            ),
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn("saturated directory delete primary", cleanup.details[0])
        self.assertEqual(len(cleanup.details), MAX_DIAGNOSTIC_DETAILS)
        self.assertNotIn(close_detail, cleanup.details)
        self.assertEqual(close_detail_adds, 1)
        self.assertEqual(cleanup.omitted_detail_count, 2, cleanup)

    def test_task8_full_ledger_blocked_failure_ingests_close_once(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000892",
        )
        root = self._task8_root_node(backend, scratch)
        level = self._task8_add_payload(
            backend, root, "level", kind=EntryKind.DIRECTORY
        )
        backend.iterator_auto_close = False
        backend.iterator_close_failures_by_identity[level.identity] = 1
        backend.close_failures_by_identity[level.identity] = 2
        graph = scratch._ensure_cleanup_graph()
        for detail_index in range(MAX_DIAGNOSTIC_DETAILS):
            graph.details.add(f"preexisting detail {detail_index:03d}")
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_FixedDetailLedger", "_bounded_diagnostic_detail"],
        )
        ledger_type = lease_module._FixedDetailLedger
        real_add = ledger_type.add
        added_details: list[str] = []
        continuing_detail = (
            "cleanup completed iterator close failed: OSError: "
            "injected close failure for level"
        )
        owner_close_detail = (
            "cleanup completed directory close failed: OSError: "
            "injected close failure for level"
        )
        blocked_detail = "cleanup owner remains open"
        unavailable_detail = (
            "managed namespace cleanup unavailable: RuntimeError: "
            "an owned capability remains open"
        )

        def record_add(ledger: object, detail: str) -> None:
            bounded = lease_module._bounded_diagnostic_detail(detail)
            added_details.append(bounded)
            real_add(ledger, detail)

        with mock.patch.object(
            ledger_type,
            "add",
            autospec=True,
            side_effect=record_add,
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(
            cleanup.details[0],
            "cleanup failed: OSError: injected close failure for level",
        )
        self.assertEqual(len(cleanup.details), MAX_DIAGNOSTIC_DETAILS)
        for detail in (
            continuing_detail,
            owner_close_detail,
            blocked_detail,
            unavailable_detail,
        ):
            with self.subTest(detail=detail):
                self.assertEqual(added_details.count(detail), 1, added_details)
                self.assertNotIn(detail, cleanup.details)
        self.assertEqual(cleanup.omitted_detail_count, 5, cleanup)
        retained = [
            owner
            for owner in scratch._cleanup_owned_capabilities
            if owner.is_open and owner.identity == level.identity
        ]
        self.assertEqual(len(retained), 1, retained)
        self.assertEqual(retained[0]._close_attempts, 2)

    def test_task8_full_ledger_dedupes_identical_exception_notes(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000893",
        )
        graph = scratch._ensure_cleanup_graph()
        for detail_index in range(MAX_DIAGNOSTIC_DETAILS):
            graph.details.add(f"preexisting detail {detail_index:03d}")
        duplicate_note = "injected duplicate saturated exception note"
        failure = OSError("injected duplicate-note primary")
        failure.add_note(duplicate_note)
        failure.add_note(duplicate_note)
        backend.iterator_failure = failure
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_FixedDetailLedger", "_bounded_diagnostic_detail"],
        )
        ledger_type = lease_module._FixedDetailLedger
        real_add = ledger_type.add
        note_adds = 0

        def record_add(ledger: object, detail: str) -> None:
            nonlocal note_adds
            if lease_module._bounded_diagnostic_detail(detail) == duplicate_note:
                note_adds += 1
            real_add(ledger, detail)

        with mock.patch.object(
            ledger_type,
            "add",
            autospec=True,
            side_effect=record_add,
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(
            cleanup.details[0], "OSError: injected duplicate-note primary"
        )
        self.assertEqual(len(cleanup.details), MAX_DIAGNOSTIC_DETAILS)
        self.assertNotIn(duplicate_note, cleanup.details)
        self.assertEqual(note_adds, 1)
        self.assertEqual(cleanup.omitted_detail_count, 2, cleanup)

    def test_task8_tail_host_error_stays_failed_when_report_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000842",
        )
        clock = [0.0]
        real_delete = backend.delete

        def fail_root_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            if capability.identity == scratch._root_identity:
                raise TimeoutError("filesystem ETIMEDOUT during root delete")
            real_delete(capability)

        def cross_during_report(operation: str) -> None:
            if operation == "final_path":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_during_report
        with (
            mock.patch.object(
                backend, "delete", side_effect=fail_root_delete
            ),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn("ETIMEDOUT", cleanup.details[0])
        self.assertIn("deadline", "; ".join(cleanup.details))

    def test_task8_child_and_root_cleanup_close_every_owner(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000810",
        )
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        child_node = root.children[child.name]
        payload = self._task8_add_payload(backend, child_node, "payload")
        root_owner = scratch._root
        assert root_owner is not None
        cleanup_ready = backend.open_file(
            root_owner,
            ".hoimin-cleanup-ready.json",
            access=FileAccess.WRITE,
            disposition=CreateDisposition.CREATE_NEW,
            share_policy=SharePolicy.MUTATION,
        )
        cleanup_ready.close()

        child_cleanup = scratch.remove_child(child)
        root_cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(
            child_cleanup.status, ScratchCleanupStatus.CLEAN, child_cleanup
        )
        self.assertEqual(
            root_cleanup.status, ScratchCleanupStatus.CLEAN, root_cleanup
        )
        self.assertGreaterEqual(backend.close_counts[payload.identity], 1)
        marker_delete = backend.cleanup_operations.index(
            "delete:.hoimin-cleanup-ready.json"
        )
        root_delete = backend.cleanup_operations.index(
            f"delete:.deleting-{scratch.run_id}"
        )
        self.assertLess(marker_delete, root_delete)
        self.assertEqual(len(backend.live_resources), 0)
        self.assertLessEqual(backend.max_directory_resources, 3)

    def test_task8_recording_cleanup_deletes_reparse_and_other_exactly(
        self,
    ) -> None:
        for index, kind in enumerate(
            (EntryKind.REPARSE, EntryKind.OTHER), start=1
        ):
            with self.subTest(kind=kind):
                backend = self._task8_backend()
                scratch = self._task8_create(
                    backend,
                    run_id=(
                        "00000000-0000-4000-8000-"
                        f"{858 + index:012d}"
                    ),
                )
                root = self._task8_root_node(backend, scratch)
                name = f"payload-{kind.value}"
                payload = self._task8_add_payload(
                    backend, root, name, kind=kind
                )

                cleanup = self._task8_cleanup(scratch, backend)

                self.assertEqual(
                    cleanup.status, ScratchCleanupStatus.CLEAN, cleanup
                )
                self.assertNotIn(name, root.children)
                self.assertIn(f"open_entry:{name}", backend.cleanup_operations)
                self.assertIn(f"delete:{name}", backend.cleanup_operations)
                self.assertGreaterEqual(
                    backend.close_counts.get(payload.identity, 0), 1
                )

    def test_task8_native_link_or_junction_cleanup_preserves_target_sentinel(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            target = parent / "external-target"
            target.mkdir()
            sentinel = target / "sentinel"
            sentinel.write_text("keep", encoding="utf-8")
            scratch = ManagedScratch.create(
                parent,
                run_id="00000000-0000-4000-8000-000000000861",
            )
            child = scratch.create_child("candidate-0001")
            link = child / "external-link"
            if os.name == "nt":
                windows_tests: ModuleType = __import__(
                    "tests.test_focused_mutation_windows_filesystem",
                    fromlist=["WindowsEnumerationTests"],
                )
                helper_type = cast(
                    type[_WindowsJunctionHelper],
                    getattr(windows_tests, "WindowsEnumerationTests"),
                )
                helper_type()._junction(link, target)
            else:
                link.symlink_to(target, target_is_directory=True)

            try:
                cleanup = self._task8_native_cleanup(scratch)
            finally:
                scratch.close_capabilities()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
            self.assertFalse(link.exists())
            self.assertTrue(target.is_dir())
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_cleanup_removes_only_the_leased_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            sentinel = parent / "sentinel"
            sentinel.write_text("keep", encoding="utf-8")
            scratch = ManagedScratch.create(parent)
            child = scratch.create_child("candidate-0001")
            (child / "payload").write_bytes(b"payload")
            leased = scratch.path

            try:
                cleanup = self._task8_native_cleanup(scratch)
            finally:
                scratch.close_capabilities()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertIsNone(cleanup.remaining_root)
            self.assertFalse(leased.exists())
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_cleanup_never_replaces_preexisting_empty_deleting_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            deleting = active.with_name(f".deleting-{scratch.run_id}")
            deleting.mkdir(mode=0o700)

            cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(active))
            self.assertTrue(active.is_dir())
            self.assertTrue(deleting.is_dir())
            deleting.rmdir()
            scratch.close_capabilities()

    def test_cleanup_does_not_claim_after_identity_check_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000812",
        )
        active = scratch.path
        clock = [0.0]

        def cross_deadline(operation: str) -> None:
            if operation == f"entry:{active.name}":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_deadline
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(
            cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
        )
        self.assertEqual(cleanup.remaining_root, str(active))
        self.assertFalse(
            any(
                operation.startswith("rename:")
                for operation in backend.cleanup_operations
            )
        )

    def test_cleanup_serializes_lease_unlink_through_root_absence(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000813",
        )
        managed = scratch._managed_root_capability
        assert managed is not None
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_open_coordinator"],
        )
        acquired = threading.Event()
        contender: threading.Thread | None = None
        acquired_during_delete = False

        def contend() -> None:
            coordinator = lease_module._open_coordinator(
                managed,
                backend,
                timeout=2.0,
                deadline=time.monotonic() + 2.0,
            )
            acquired.set()
            _close_lease_lock_all(coordinator, "cleanup contender")

        def race_lease_delete(operation: str) -> None:
            nonlocal contender, acquired_during_delete
            if operation != "delete:.hoimin-lease.json" or contender is not None:
                return
            contender = threading.Thread(target=contend)
            contender.start()
            acquired_during_delete = acquired.wait(0.1)

        backend.before_cleanup_operation = race_lease_delete
        cleanup = self._task8_cleanup(scratch, backend)

        assert contender is not None
        contender.join(2.0)
        self.assertFalse(contender.is_alive())
        self.assertFalse(acquired_during_delete)
        self.assertTrue(acquired.is_set())
        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertIsNone(cleanup.remaining_root)

    def test_cleanup_absence_stays_clean_when_capability_close_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            leased = scratch.path

            with mock.patch.object(
                scratch,
                "close_capabilities",
                return_value=("managed lease close failed: injected",),
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertIsNone(cleanup.remaining_root)
            self.assertFalse(leased.exists())
            self.assertIn("managed lease close failed", cleanup.details[0])
            scratch.close_capabilities()

    def test_cleanup_of_deep_tree_keeps_only_three_directory_handles(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000811",
        )
        root = self._task8_root_node(backend, scratch)
        current = root
        for index in range(129):
            current = self._task8_add_payload(
                backend,
                current,
                f"d{index:03d}",
                kind=EntryKind.DIRECTORY,
            )
        self._task8_add_payload(backend, current, "payload")
        backend.cleanup_operations.clear()
        backend.max_directory_resources = sum(
            1
            for resource in backend.live_resources
            if not resource.closed
            and resource.node.kind is EntryKind.DIRECTORY
        )

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN, cleanup)
        self.assertLessEqual(backend.max_directory_resources, 3)

    def test_cleanup_refuses_replacement_root_with_same_managed_name(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000812",
        )
        original = self._task8_root_node(backend, scratch)
        managed = original.parent
        assert managed is not None
        active_name = original.name
        moved_name = f"{active_name}.moved"
        del managed.children[active_name]
        original.name = moved_name
        managed.children[moved_name] = original
        replacement = self._task8_add_payload(
            backend,
            managed,
            active_name,
            kind=EntryKind.DIRECTORY,
        )
        sentinel = self._task8_add_payload(backend, replacement, "sentinel")
        backend.cleanup_operations.clear()

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(
            cleanup.remaining_root,
            str(scratch.path.with_name(moved_name)),
        )
        self.assertIs(managed.children[active_name], replacement)
        self.assertIs(replacement.children["sentinel"], sentinel)
        self.assertIs(managed.children[moved_name], original)

    def test_cleanup_root_mismatch_stays_failed_without_report_recovery(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000832",
        )
        original = self._task8_root_node(backend, scratch)
        managed = original.parent
        assert managed is not None
        active_name = original.name
        del managed.children[active_name]
        original.name = f"{active_name}.moved"
        managed.children[original.name] = original
        replacement = self._task8_add_payload(
            backend,
            managed,
            active_name,
            kind=EntryKind.DIRECTORY,
        )
        sentinel = self._task8_add_payload(backend, replacement, "sentinel")
        backend.cleanup_operations.clear()

        with mock.patch.object(
            backend,
            "final_path",
            side_effect=OSError("injected report-only recovery failure"),
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIsNone(cleanup.remaining_root)
        self.assertIn("identity changed", cleanup.details[0])
        self.assertIn("report-only recovery failure", "; ".join(cleanup.details))
        self.assertIs(managed.children[active_name], replacement)
        self.assertIs(replacement.children["sentinel"], sentinel)
        self.assertFalse(
            any(
                operation.startswith(("rename:", "delete:"))
                for operation in backend.cleanup_operations
            )
        )

    def test_task8_native_root_replacement_before_delete_survives(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(
                Path(directory),
                run_id="00000000-0000-4000-8000-000000000814",
            )
            backend = scratch._backend
            real_delete = backend.delete
            injected = False
            sentinel: Path | None = None

            def replace_root_before_delete(
                capability: FileCapability | DirectoryCapability,
            ) -> None:
                nonlocal injected, sentinel
                if (
                    not injected
                    and isinstance(capability, DirectoryCapability)
                    and capability.identity == scratch._root_identity
                ):
                    parent = scratch._managed_root_capability
                    assert parent is not None and parent.is_open
                    claimed = backend.final_path(capability)
                    backend.rename(
                        capability,
                        parent,
                        f"{claimed.name}.original",
                        replace=False,
                    )
                    replacement = backend.create_directory(
                        parent,
                        claimed.name,
                        SharePolicy.MUTATION,
                    )
                    replacement.close()
                    sentinel = claimed / "sentinel"
                    sentinel.write_text("keep", encoding="utf-8")
                    injected = True
                real_delete(capability)

            try:
                with mock.patch.object(
                    backend,
                    "delete",
                    side_effect=replace_root_before_delete,
                ):
                    cleanup = self._task8_native_cleanup(scratch)

                self.assertTrue(injected)
                self.assertEqual(
                    cleanup.status, ScratchCleanupStatus.FAILED, cleanup
                )
                assert sentinel is not None
                self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")
            finally:
                scratch.close_capabilities()

    def test_cleanup_recovers_owned_path_from_pinned_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            original = scratch.path
            moved = original.with_name(f"{original.name}.moved")
            root = scratch._root
            managed = scratch._managed_root_capability
            heartbeat = scratch._heartbeat
            lease = scratch._lease
            assert root is not None
            assert managed is not None
            assert heartbeat is not None
            assert lease is not None
            heartbeat.close()
            scratch._heartbeat = None
            self.assertEqual(
                _close_lease_lock_all(lease, "path recovery fixture lease"),
                (),
            )
            scratch._lease = None
            scratch._backend.rename(
                root, managed, moved.name, replace=False
            )
            replacement = scratch._backend.create_directory(
                managed, original.name, SharePolicy.MUTATION
            )
            replacement.close()
            try:
                cleanup = scratch.cleanup()
            finally:
                scratch.close_capabilities()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(moved))
            self.assertEqual(scratch.path, moved)
            self.assertTrue(original.is_dir())
            self.assertTrue(moved.is_dir())
            scratch.close_capabilities()

    def test_cleanup_claim_lookup_error_does_not_report_unvalidated_path(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            backend = scratch._backend
            real_entry = backend.entry

            def fail_owned_entry(
                parent: DirectoryCapability, name: str
            ) -> DirectoryEntry | None:
                if name == scratch.path.name:
                    raise OSError("injected owned-entry lookup failure")
                return real_entry(parent, name)

            with (
                mock.patch.object(
                    backend,
                    "entry",
                    side_effect=fail_owned_entry,
                ),
                mock.patch.object(
                    backend,
                    "final_path",
                    side_effect=OSError("injected path recovery failure"),
                ),
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertIsNone(cleanup.remaining_root)
            self.assertIn("lookup failed", "; ".join(cleanup.details))
            scratch.close_capabilities()

    def test_missing_named_root_is_failed_before_exact_delete(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000814",
        )
        managed = backend.parent.children["hoimin-focused-v1"]
        del managed.children[scratch.path.name]

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))
        self.assertFalse(
            any(
                operation.startswith(("rename:", "delete:"))
                for operation in backend.cleanup_operations
            )
        )

    def test_cleanup_tail_lookup_error_does_not_report_unvalidated_path(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000815",
        )
        deleting_name = f".deleting-{scratch.run_id}"
        real_entry = backend.entry
        deleting_checks = 0

        def fail_tail_entry(
            parent: DirectoryCapability, name: str
        ) -> DirectoryEntry | None:
            nonlocal deleting_checks
            if name == deleting_name:
                deleting_checks += 1
                if deleting_checks >= 3:
                    raise OSError("injected deleting-entry lookup failure")
            return real_entry(parent, name)

        with (
            mock.patch.object(backend, "entry", side_effect=fail_tail_entry),
            mock.patch.object(
                backend,
                "final_path",
                side_effect=OSError("injected path recovery failure"),
            ),
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIsNone(cleanup.remaining_root)
        self.assertIn(
            "injected deleting-entry lookup failure",
            "; ".join(cleanup.details),
        )

    def test_post_rmdir_stat_error_never_reports_deleted_path_as_failed(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000816",
        )
        deleting_name = f".deleting-{scratch.run_id}"
        real_entry = backend.entry
        real_delete = backend.delete
        root_deleted = False

        def observe_exact_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal root_deleted
            real_delete(capability)
            if capability.identity == scratch._root_identity:
                root_deleted = True

        def fail_after_exact_delete(
            parent: DirectoryCapability, name: str
        ) -> DirectoryEntry | None:
            if root_deleted and name == deleting_name:
                raise OSError("injected post-delete lookup failure")
            return real_entry(parent, name)

        with (
            mock.patch.object(
                backend, "delete", side_effect=observe_exact_delete
            ),
            mock.patch.object(
                backend, "entry", side_effect=fail_after_exact_delete
            ),
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIsNone(cleanup.remaining_root, cleanup)
        self.assertIn(
            "injected post-delete lookup failure",
            "; ".join(cleanup.details),
        )

    def test_cleanup_stops_after_entry_open_crosses_deadline(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000817",
        )
        root = self._task8_root_node(backend, scratch)
        self._task8_add_payload(backend, root, "payload")
        clock = [0.0]

        def cross_after_open(operation: str) -> None:
            if operation == "open_entry:payload":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_after_open
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(
            cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
        )
        self.assertNotIn("delete:payload", backend.cleanup_operations)

    def test_task8_deferred_owner_cleanup_reopens_saved_cursor(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000833",
        )
        root = self._task8_root_node(backend, scratch)
        payload = self._task8_add_payload(backend, root, "payload")
        clock = [0.0]

        def cross_after_open(operation: str) -> None:
            if operation == "open_entry:payload":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_after_open
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            first = self._task8_cleanup(scratch, backend)

        self.assertEqual(first.status, ScratchCleanupStatus.DEFERRED, first)
        self.assertIsNone(scratch._root)
        self.assertIsNotNone(scratch._cleanup_cursor)
        self.assertIn(payload.name, root.children)

        backend.before_cleanup_operation = lambda _operation: None
        backend.after_cleanup_operation = lambda _operation: None
        clock[0] = 0.0
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            resumed = self._task8_cleanup(scratch, backend)

        self.assertEqual(resumed.status, ScratchCleanupStatus.CLEAN, resumed)
        self.assertNotIn(payload.name, root.children)
        self.assertIsNone(scratch._cleanup_cursor)
        self.assertEqual(len(backend.live_resources), 0)

    def test_cleanup_stops_after_iterator_construction_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000818",
        )
        root = self._task8_root_node(backend, scratch)
        self._task8_add_payload(backend, root, "payload")
        clock = [0.0]

        def cross_after_iterator(operation: str) -> None:
            if operation == "entries_owned":
                clock[0] = 60.0

        def forbid_next(operation: str) -> None:
            if clock[0] >= 60.0 and operation.startswith("iterator.next:"):
                raise AssertionError(
                    "iterator next started after cleanup deadline"
                )

        backend.after_cleanup_operation = cross_after_iterator
        backend.before_cleanup_operation = forbid_next
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(
            cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
        )

    def test_remove_payload_preexpired_budget_does_not_start_filesystem_io(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000819",
        )
        root = scratch._root
        anchor = scratch._managed_root_capability
        assert root is not None
        assert anchor is not None
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_CleanupCursor", "_remove_payload"],
        )
        backend.before_cleanup_operation = lambda operation: (_ for _ in ()).throw(
            AssertionError(f"filesystem operation started: {operation}")
        )

        result = lease_module._remove_payload(
            backend,
            anchor,
            root,
            root_name=scratch.path.name,
            root_identity=scratch._root_identity,
            root_filesystem=scratch._root_filesystem,
            cursor=lease_module._CleanupCursor(),
            started=0.0,
            absolute_deadline=1.0,
            examined=0,
            removed=0,
            monotonic=lambda: 1.0,
        )

        self.assertFalse(result.complete)
        self.assertEqual(result.examined_entries, 0)
        self.assertEqual(result.removed_entries, 0)

    def test_remove_payload_stops_after_iterator_transfer_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000820",
        )
        root = scratch._root
        anchor = scratch._managed_root_capability
        assert root is not None
        assert anchor is not None
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_CleanupCursor", "_remove_payload"],
        )
        clock = [0.0]
        backend.after_cleanup_operation = lambda operation: clock.__setitem__(
            0, 1.0 if operation == "entries_owned" else clock[0]
        )
        backend.before_cleanup_operation = lambda operation: (
            (_ for _ in ()).throw(
                AssertionError("iterator next started after deadline")
            )
            if clock[0] >= 1.0 and operation.startswith("iterator.next:")
            else None
        )

        result = lease_module._remove_payload(
            backend,
            anchor,
            root,
            root_name=scratch.path.name,
            root_identity=scratch._root_identity,
            root_filesystem=scratch._root_filesystem,
            cursor=lease_module._CleanupCursor(),
            started=0.0,
            absolute_deadline=1.0,
            examined=0,
            removed=0,
            monotonic=lambda: clock[0],
        )
        scratch._root = None

        self.assertFalse(result.complete)
        self.assertEqual(result.examined_entries, 0)

    def test_remove_payload_propagates_filesystem_timeout_before_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000821",
        )
        root_node = self._task8_root_node(backend, scratch)
        self._task8_add_payload(
            backend, root_node, "nested", kind=EntryKind.DIRECTORY
        )
        root = scratch._root
        anchor = scratch._managed_root_capability
        assert root is not None
        assert anchor is not None
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_CleanupCursor", "_remove_payload"],
        )
        clock = [0.0]

        def timeout_while_opening(operation: str) -> None:
            if operation == "open_directory:nested":
                clock[0] = 60.0
                raise TimeoutError("filesystem ETIMEDOUT")

        backend.before_cleanup_operation = timeout_while_opening
        with self.assertRaisesRegex(TimeoutError, "ETIMEDOUT"):
            lease_module._remove_payload(
                backend,
                anchor,
                root,
                root_name=scratch.path.name,
                root_identity=scratch._root_identity,
                root_filesystem=scratch._root_filesystem,
                cursor=lease_module._CleanupCursor(),
                started=0.0,
                absolute_deadline=60.0,
                examined=0,
                removed=0,
                monotonic=lambda: clock[0],
            )

    def test_remove_payload_reopen_stops_after_identity_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000822",
        )
        root_node = self._task8_root_node(backend, scratch)
        nested = self._task8_add_payload(
            backend, root_node, "nested", kind=EntryKind.DIRECTORY
        )
        self._task8_add_payload(backend, nested, "payload")
        clock = [0.0]

        def cross_after_open(operation: str) -> None:
            if operation == "open_directory:nested":
                clock[0] = 60.0

        def forbid_transfer(operation: str) -> None:
            if (
                clock[0] >= 60.0
                and operation == "entries_owned"
                and backend.cleanup_operations.count("entries_owned") >= 1
            ):
                raise AssertionError(
                    "iterator transfer started after cleanup deadline"
                )

        backend.after_cleanup_operation = cross_after_open
        backend.before_cleanup_operation = forbid_transfer
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup)
        self.assertGreaterEqual(cleanup.examined_entries, 1)
        self.assertEqual(cleanup.removed_entries, 0)

    def test_owned_root_report_recovery_stops_after_final_path_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000823",
        )
        managed = backend.parent.children["hoimin-focused-v1"]
        active_name = scratch.path.name
        root = managed.children.pop(active_name)
        moved_name = f"{active_name}.moved"
        root.name = moved_name
        managed.children[moved_name] = root
        self._task8_add_payload(
            backend, managed, active_name, kind=EntryKind.DIRECTORY
        )
        clock = [0.0]

        def cross_after_final_path(operation: str) -> None:
            if operation == "final_path":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_after_final_path
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIsNone(cleanup.remaining_root)
        self.assertNotIn(f"entry:{moved_name}", backend.cleanup_operations)
        self.assertIn("deadline", "; ".join(cleanup.details))

    def test_owned_root_report_recovery_stops_after_evidence_lookup_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000824",
        )
        managed = backend.parent.children["hoimin-focused-v1"]
        active_name = scratch.path.name
        root = managed.children.pop(active_name)
        moved_name = f"{active_name}.moved"
        root.name = moved_name
        managed.children[moved_name] = root
        self._task8_add_payload(
            backend, managed, active_name, kind=EntryKind.DIRECTORY
        )
        clock = [0.0]

        def cross_after_evidence(operation: str) -> None:
            if operation == f"entry:{moved_name}":
                clock[0] = 60.0

        backend.after_cleanup_operation = cross_after_evidence
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIsNone(cleanup.remaining_root)
        self.assertFalse(
            any(
                operation.startswith(("rename:", "delete:"))
                for operation in backend.cleanup_operations
            )
        )

    def test_empty_janitor_verifies_rmdir_before_reporting_clean(self) -> None:
        (
            backend,
            managed_capability,
            candidate,
            candidate_capability,
            selected,
        ) = self._task9_empty_candidate(
            "00000000-0000-4000-8000-000000000122"
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_reclaim_empty_unleased_candidate"],
        )
        with mock.patch.object(backend, "delete", return_value=None):
            result = lease_module._reclaim_empty_unleased_candidate(
                managed_capability.path_hint,
                managed_capability,
                candidate_capability,
                selected,
                backend,
                current_time=24 * 60 * 60 + 1.0,
                deadline=time.monotonic() + 30.0,
            )

        self.assertIsInstance(result, ScratchCleanupRecord)
        assert isinstance(result, ScratchCleanupRecord)
        self.assertEqual(result.status, ScratchCleanupStatus.FAILED)
        self.assertEqual(
            result.remaining_root,
            str(managed_capability.path_hint / candidate.name),
        )
        self.assertIn(candidate.name, backend._resource(managed_capability).node.children)
        candidate_capability.close()
        managed_capability.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_empty_janitor_does_not_recover_after_deadline(self) -> None:
        (
            backend,
            managed_capability,
            candidate,
            candidate_capability,
            selected,
        ) = self._task9_empty_candidate(
            "00000000-0000-4000-8000-000000000123"
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_reclaim_empty_unleased_candidate"],
        )
        crossed = False

        def vanish_at_scan(*_args: object, **_kwargs: object) -> bool:
            nonlocal crossed
            crossed = True
            raise FileNotFoundError(candidate.name)

        with (
            mock.patch.object(
                lease_module,
                "_directory_is_empty_at",
                side_effect=vanish_at_scan,
            ),
            mock.patch.object(
                lease_module.time,
                "monotonic",
                side_effect=lambda: 31.0 if crossed else 0.0,
            ),
        ):
            result = lease_module._reclaim_empty_unleased_candidate(
                managed_capability.path_hint,
                managed_capability,
                candidate_capability,
                selected,
                backend,
                current_time=24 * 60 * 60 + 1.0,
                deadline=30.0,
            )

        self.assertIsInstance(result, JanitorDiagnostic)
        self.assertIn(candidate.name, backend._resource(managed_capability).node.children)
        candidate_capability.close()
        managed_capability.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_owned_root_recovery_never_scans_managed_namespace(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000825",
        )
        managed = backend.parent.children["hoimin-focused-v1"]
        active_name = scratch.path.name
        root = managed.children.pop(active_name)
        root.name = f"{active_name}.moved"
        managed.children[root.name] = root
        replacement = self._task8_add_payload(
            backend, managed, active_name, kind=EntryKind.DIRECTORY
        )

        cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIs(managed.children[active_name], replacement)
        self.assertNotIn("entries_owned", backend.cleanup_operations)
        self.assertFalse(
            any(
                operation.startswith(("rename:", "delete:"))
                for operation in backend.cleanup_operations
            )
        )

    def test_cleanup_reports_deleting_path_after_post_rename_verification_failure(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000826",
        )
        deleting = scratch.path.with_name(f".deleting-{scratch.run_id}")
        real_entry = backend.entry
        renamed = False

        def note_rename(operation: str) -> None:
            nonlocal renamed
            if operation.startswith("rename:"):
                renamed = True

        def mismatch_after_rename(
            parent: DirectoryCapability, name: str
        ) -> DirectoryEntry | None:
            entry = real_entry(parent, name)
            if renamed and name == deleting.name and entry is not None:
                return DirectoryEntry(
                    entry.name,
                    entry.kind,
                    FileIdentity(entry.identity.volume, entry.identity.file + 1),
                    entry.filesystem,
                    entry.logical_size,
                    entry.modified_ns,
                )
            return entry

        backend.after_cleanup_operation = note_rename
        with mock.patch.object(
            backend, "entry", side_effect=mismatch_after_rename
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertEqual(cleanup.remaining_root, str(deleting))
        self.assertEqual(scratch.path, deleting)
        managed = backend.parent.children["hoimin-focused-v1"]
        self.assertIn(deleting.name, managed.children)

    def test_cleanup_stops_after_claim_rename_crosses_deadline(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000827",
        )
        deleting = scratch.path.with_name(f".deleting-{scratch.run_id}")
        clock = [0.0]

        def cross_during_rename(operation: str) -> None:
            if operation.startswith("rename:"):
                clock[0] = 60.0

        def forbid_post_deadline(operation: str) -> None:
            if clock[0] >= 60.0 and operation.startswith("entry:"):
                raise AssertionError(
                    "entry lookup started after cleanup deadline"
                )

        backend.after_cleanup_operation = cross_during_rename
        backend.before_cleanup_operation = forbid_post_deadline
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup)
        self.assertEqual(cleanup.remaining_root, str(deleting))
        self.assertEqual(scratch.path, deleting)

    def test_cleanup_stops_after_root_rmdir_crosses_deadline(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000828",
        )
        deleting_name = f".deleting-{scratch.run_id}"
        clock = [0.0]

        def cross_during_delete(operation: str) -> None:
            if operation == f"delete:{deleting_name}":
                clock[0] = 60.0

        def forbid_absence(operation: str) -> None:
            if clock[0] >= 60.0 and operation == f"entry:{deleting_name}":
                raise AssertionError(
                    "absence verification started after cleanup deadline"
                )

        backend.after_cleanup_operation = cross_during_delete
        backend.before_cleanup_operation = forbid_absence
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup)
        self.assertIsNone(cleanup.remaining_root)

    def test_marker_deadline_keeps_close_failure_secondary(self) -> None:
        backend = self._task8_backend()
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000829"
        root_node = self._task9_add_candidate(
            backend, managed, run_id=run_id
        )
        root = backend.open_directory(
            managed_capability, root_node.name, SharePolicy.PINNED
        )
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_read_valid_marker_at", "_close_capability_retry"],
        )
        real_close_retry = lease_module._close_capability_retry
        checks = 0

        def cross_after_open(*_args: object, **_kwargs: object) -> None:
            nonlocal checks
            checks += 1
            if checks == 2:
                raise TimeoutError("injected marker deadline")

        def close_with_secondary(
            capability: DirectoryCapability | FileCapability,
            label: str,
        ) -> tuple[str, ...]:
            errors = real_close_retry(capability, label)
            if label.startswith("managed marker read"):
                return (*errors, "injected marker close failure")
            return errors

        with (
            mock.patch.object(
                lease_module,
                "_check_absolute_deadline",
                side_effect=cross_after_open,
            ),
            mock.patch.object(
                lease_module,
                "_close_capability_retry",
                side_effect=close_with_secondary,
            ),
            self.assertRaisesRegex(
                TimeoutError, "injected marker deadline"
            ) as raised,
        ):
            lease_module._read_valid_marker_at(
                root,
                ".hoimin-lease.json",
                run_id,
                backend,
                deadline=time.monotonic() + 30.0,
            )

        self.assertIn(
            "marker close failure",
            "; ".join(getattr(raised.exception, "__notes__", ())),
        )
        root.close()
        managed_capability.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_cleanup_coordinator_timeout_is_deferred_not_an_exception(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_DeadlineExceeded"],
            )

            with mock.patch(
                "tools.focused_mutation_support.lease._open_coordinator",
                side_effect=lease_module._DeadlineExceeded(
                    "injected contention"
                ),
            ):
                cleanup = scratch.cleanup(time_budget=0.01)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            self.assertTrue(scratch.path.is_dir())
            scratch.close_capabilities()

    def test_cleanup_coordinator_filesystem_timeout_is_failed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))

            with mock.patch(
                "tools.focused_mutation_support.lease._open_coordinator",
                side_effect=TimeoutError("filesystem ETIMEDOUT"),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            self.assertIn("ETIMEDOUT", cleanup.details[0])
            self.assertTrue(scratch.path.is_dir())
            scratch.close_capabilities()

    def test_cleanup_tail_coordinator_filesystem_timeout_is_failed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator"],
            )
            real_open = lease_module._open_coordinator
            coordinator_calls = 0

            def timeout_at_tail(*args: object, **kwargs: object) -> LeaseLock:
                nonlocal coordinator_calls
                coordinator_calls += 1
                if coordinator_calls == 2:
                    raise TimeoutError("filesystem ETIMEDOUT")
                return real_open(*args, **kwargs)

            with mock.patch.object(
                lease_module,
                "_open_coordinator",
                side_effect=timeout_at_tail,
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            self.assertIn("ETIMEDOUT", cleanup.details[0])
            self.assertTrue(scratch.path.is_dir())
            scratch.close_capabilities()

    def test_cleanup_claim_coordinator_close_preserves_failed_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator"],
            )
            real_open = lease_module._open_coordinator
            real_close = os.close
            opened: list[LeaseLock] = []
            close_attempts = 0

            def track_coordinator(
                *args: object, **kwargs: object
            ) -> LeaseLock:
                coordinator = real_open(*args, **kwargs)
                opened.append(coordinator)
                return coordinator

            def fail_claim_close(descriptor: int) -> None:
                nonlocal close_attempts
                if opened and descriptor == opened[0].fd:
                    close_attempts += 1
                    raise OSError(
                        "injected claim coordinator close failure "
                        f"{close_attempts}"
                    )
                real_close(descriptor)

            with (
                mock.patch.object(
                    lease_module,
                    "_open_coordinator",
                    side_effect=track_coordinator,
                ),
                mock.patch.object(
                    lease_module.os,
                    "close",
                    side_effect=fail_claim_close,
                ),
            ):
                cleanup = scratch.cleanup()

            try:
                self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
                self.assertEqual(cleanup.remaining_root, str(scratch.path))
                self.assertTrue(scratch.path.is_dir())
                self.assertEqual(close_attempts, 2)
                joined = "; ".join(cleanup.details)
                self.assertIn("claim coordinator close failure 1", joined)
                self.assertIn("claim coordinator close failure 2", joined)
                self.assertEqual(
                    joined.count("managed namespace cleanup unavailable"), 1
                )
                scratch.close_capabilities()
                self.assertEqual(close_attempts, 2)
            finally:
                for coordinator in opened:
                    coordinator.__del__()
                scratch.__del__()

    def test_cleanup_tail_preserves_primary_and_coordinator_close_secondary(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000829",
        )
        real_delete = backend.delete
        failed_root_owner: DirectoryCapability | None = None
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["os"],
        )
        real_close = os.close
        tail_delete_started = False
        tail_close_attempts = 0

        def fail_tail_identity(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal failed_root_owner, tail_delete_started
            if capability.identity == scratch._root_identity:
                assert isinstance(capability, DirectoryCapability)
                failed_root_owner = capability
                tail_delete_started = True
                raise OSError("managed root identity changed during tail delete")
            real_delete(capability)

        def fail_tail_close(descriptor: int) -> None:
            nonlocal tail_close_attempts
            coordinator = backend.coordinator
            if (
                tail_delete_started
                and coordinator is not None
                and descriptor == coordinator.fd
            ):
                tail_close_attempts += 1
                if tail_close_attempts == 1:
                    raise OSError(
                        "injected tail coordinator close failure"
                    )
            real_close(descriptor)

        with (
            mock.patch.object(
                backend, "delete", side_effect=fail_tail_identity
            ),
            mock.patch.object(
                lease_module.os,
                "close",
                side_effect=fail_tail_close,
            ),
        ):
            cleanup = self._task8_cleanup(scratch, backend)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIn("identity changed", cleanup.details[0])
        self.assertIn("identity changed", "; ".join(cleanup.details))
        self.assertIn(
            "tail coordinator close failure", "; ".join(cleanup.details)
        )
        self.assertEqual(tail_close_attempts, 2)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))
        assert failed_root_owner is not None
        self.assertFalse(failed_root_owner.is_open)
        self.assertIsNone(scratch._root)

    def test_task8_tail_delete_failure_retains_persistent_root_owner(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000867",
        )
        root_identity = scratch._root_identity
        backend.close_failures_by_identity[root_identity] = 3
        real_delete = backend.delete
        failed_root_owner: DirectoryCapability | None = None

        def fail_root_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            nonlocal failed_root_owner
            if capability.identity == root_identity:
                assert isinstance(capability, DirectoryCapability)
                failed_root_owner = capability
                raise OSError("injected tail root delete primary")
            real_delete(capability)

        with mock.patch.object(
            backend, "delete", side_effect=fail_root_delete
        ):
            first = self._task8_cleanup(scratch, backend)
            boundary = len(backend.cleanup_operations)
            second = self._task8_cleanup(scratch, backend)

        self.assertEqual(first.status, ScratchCleanupStatus.FAILED, first)
        self.assertIn("tail root delete primary", first.details[0])
        assert failed_root_owner is not None
        self.assertEqual(failed_root_owner._close_attempts, 2)
        self.assertTrue(failed_root_owner.is_open)
        self.assertIs(scratch._root, failed_root_owner)
        self.assertEqual(second.status, ScratchCleanupStatus.FAILED, second)
        self.assertFalse(
            any(
                operation.startswith(
                    ("entry:", "open_", "rename:", "delete:")
                )
                for operation in backend.cleanup_operations[boundary:]
            ),
            backend.cleanup_operations[boundary:],
        )
        self.assertEqual(
            "; ".join(second.details).count(
                "managed namespace cleanup unavailable"
            ),
            1,
        )
        resource = backend._resource(failed_root_owner)
        resource.close_failures = 0
        del failed_root_owner
        scratch.__del__()
        gc.collect()
        self.assertTrue(resource.closed)

    def test_cleanup_does_not_claim_root_after_total_deadline(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000830",
        )
        coordinator = LeaseLock(os.open(os.devnull, os.O_RDONLY))
        coordinator.locked = True
        first_tick = iter([0.0])

        with (
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: next(first_tick, 1.0),
            ),
            mock.patch(
                "tools.focused_mutation_support.lease._open_coordinator",
                return_value=coordinator,
            ),
            mock.patch.object(backend, "rename") as rename,
        ):
            cleanup = scratch.cleanup(time_budget=1.0)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup)
        rename.assert_not_called()
        self.assertEqual(coordinator.fd, -1)

    def test_remove_child_refuses_replacement_with_same_child_name(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000813",
        )
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        original = root.children.pop(child.name)
        original.name = "candidate-original"
        root.children[original.name] = original
        replacement = self._task8_add_payload(
            backend,
            root,
            child.name,
            kind=EntryKind.DIRECTORY,
        )
        sentinel = self._task8_add_payload(backend, replacement, "sentinel")
        backend.cleanup_operations.clear()

        cleanup = scratch.remove_child(child)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED, cleanup)
        self.assertIs(root.children[child.name], replacement)
        self.assertIs(replacement.children["sentinel"], sentinel)
        self.assertIs(root.children[original.name], original)

    def test_task8_native_child_replacement_before_delete_survives(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(
                Path(directory),
                run_id="00000000-0000-4000-8000-000000000815",
            )
            child = scratch.create_child("candidate-0001")
            child_identity, _child_filesystem = scratch._children[child.name]
            backend = scratch._backend
            real_delete = backend.delete
            injected = False
            sentinel: Path | None = None

            def replace_child_before_delete(
                capability: FileCapability | DirectoryCapability,
            ) -> None:
                nonlocal injected, sentinel
                if (
                    not injected
                    and isinstance(capability, DirectoryCapability)
                    and capability.identity == child_identity
                ):
                    parent = scratch._root
                    assert parent is not None and parent.is_open
                    backend.rename(
                        capability,
                        parent,
                        "candidate-original",
                        replace=False,
                    )
                    replacement = backend.create_directory(
                        parent,
                        child.name,
                        SharePolicy.MUTATION,
                    )
                    replacement.close()
                    sentinel = child / "sentinel"
                    sentinel.write_text("keep", encoding="utf-8")
                    injected = True
                real_delete(capability)

            try:
                with mock.patch.object(
                    backend,
                    "delete",
                    side_effect=replace_child_before_delete,
                ):
                    cleanup = scratch.remove_child(child)

                self.assertTrue(injected)
                self.assertEqual(
                    cleanup.status, ScratchCleanupStatus.FAILED, cleanup
                )
                assert sentinel is not None
                self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")
            finally:
                scratch.close_capabilities()

    def test_remove_child_stops_after_open_vanishes_at_deadline(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(backend)
        child = scratch.create_child("candidate-0001")
        root = self._task8_root_node(backend, scratch)
        clock = [0.0]
        backend.cleanup_operations.clear()

        def vanish_during_open(operation: str) -> None:
            if operation == f"open_directory:{child.name}":
                del root.children[child.name]
                clock[0] = 61.0
                raise FileNotFoundError(child.name)
            if clock[0] >= 60.0 and operation.startswith(
                ("entry:", "open_", "entries_owned", "final_path")
            ):
                raise AssertionError(
                    "namespace recovery started after cleanup deadline"
                )

        backend.before_cleanup_operation = vanish_during_open
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            try:
                cleanup = scratch.remove_child(child)
            finally:
                scratch.close_capabilities()

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))

    def test_remove_child_classifies_open_deadline_as_deferred(self) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(backend)
        child = scratch.create_child("candidate-0001")
        clock = [0.0]
        backend.cleanup_operations.clear()

        def cross_after_open(operation: str) -> None:
            if operation == f"open_directory:{child.name}":
                clock[0] = 61.0

        backend.after_cleanup_operation = cross_after_open
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = scratch.remove_child(child)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))
        self.assertNotIn("entries_owned", backend.cleanup_operations)

    def test_remove_child_classifies_filesystem_timeout_before_deadline_as_failed(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(backend)
        child = scratch.create_child("candidate-0001")
        backend.cleanup_operations.clear()

        def timeout_while_opening(operation: str) -> None:
            if operation == f"open_directory:{child.name}":
                raise TimeoutError("filesystem ETIMEDOUT")

        backend.before_cleanup_operation = timeout_while_opening
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            return_value=0.0,
        ):
            cleanup = scratch.remove_child(child)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))
        self.assertIn("ETIMEDOUT", cleanup.details[0])

    def test_remove_child_keeps_filesystem_timeout_failed_after_deadline_crossing(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(backend)
        child = scratch.create_child("candidate-0001")
        clock = [0.0]
        backend.cleanup_operations.clear()

        def timeout_after_crossing(operation: str) -> None:
            if operation == f"open_directory:{child.name}":
                clock[0] = 61.0
                raise TimeoutError("filesystem ETIMEDOUT")

        backend.before_cleanup_operation = timeout_after_crossing
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = scratch.remove_child(child)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))
        self.assertIn("ETIMEDOUT", cleanup.details[0])

    def test_remove_child_stops_when_recovery_identity_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(backend)
        child = scratch.create_child("candidate-0001")
        clock = [0.0]
        entry_calls = 0
        backend.cleanup_operations.clear()

        def vanish_and_cross_during_recovery(operation: str) -> None:
            nonlocal entry_calls
            if operation == f"open_directory:{child.name}":
                raise FileNotFoundError(child.name)
            if operation == f"entry:{child.name}":
                entry_calls += 1
                if entry_calls == 2:
                    clock[0] = 61.0
            if clock[0] >= 60.0 and operation == "final_path":
                raise AssertionError(
                    "root report recovery started after cleanup deadline"
                )

        backend.after_cleanup_operation = vanish_and_cross_during_recovery
        with mock.patch(
            "tools.focused_mutation_support.lease.time.monotonic",
            side_effect=lambda: clock[0],
        ):
            cleanup = scratch.remove_child(child)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))

    def test_remove_child_stops_when_post_rmdir_stat_crosses_deadline(
        self,
    ) -> None:
        backend = self._task8_backend()
        scratch = self._task8_create(backend)
        child = scratch.create_child("candidate-0001")
        clock = [0.0]
        backend.cleanup_operations.clear()

        def no_op_delete(
            capability: FileCapability | DirectoryCapability,
        ) -> None:
            name = backend._resource(capability).node.name
            backend._cleanup_operation(f"delete:{name}")
            clock[0] = 61.0

        def forbid_post_deadline_recovery(operation: str) -> None:
            if clock[0] >= 60.0 and operation.startswith(
                ("entry:", "open_", "entries_owned", "final_path")
            ):
                raise AssertionError(
                    "recovery started after consuming-delete deadline"
                )

        backend.before_cleanup_operation = forbid_post_deadline_recovery
        with (
            mock.patch.object(backend, "delete", side_effect=no_op_delete),
            mock.patch(
                "tools.focused_mutation_support.lease.time.monotonic",
                side_effect=lambda: clock[0],
            ),
        ):
            cleanup = scratch.remove_child(child)

        self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
        self.assertEqual(cleanup.remaining_root, str(scratch.path))

    def test_heartbeat_refresh_refuses_replaced_marker(self) -> None:
        backend = self._task7_backend()
        scratch = self._task7_create(
            backend,
            run_id="00000000-0000-4000-8000-000000000715",
        )
        root_capability = scratch._root
        assert root_capability is not None
        root = backend._resource(root_capability).node
        heartbeat = root.children.pop(".hoimin-heartbeat.json")
        heartbeat.name = ".hoimin-heartbeat.original"
        root.children[heartbeat.name] = heartbeat
        replacement = backend._new_node(
            EntryKind.REGULAR,
            SecurityDomain.MANAGED,
            parent=root,
            name=".hoimin-heartbeat.json",
        )
        touches = backend.events.count("touch:.hoimin-heartbeat.json")

        with self.assertRaisesRegex(OSError, "heartbeat identity changed"):
            scratch.refresh_heartbeat()

        self.assertIs(root.children[".hoimin-heartbeat.json"], replacement)
        self.assertEqual(
            backend.events.count("touch:.hoimin-heartbeat.json"), touches
        )

    def test_retention_preserves_exact_validated_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            leased = scratch.path

            cleanup = scratch.retain()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.RETAINED)
            self.assertEqual(cleanup.remaining_root, str(leased))
            self.assertTrue(leased.is_dir())
            scratch.mark_cleanup_ready()
            self.assertEqual(
                scratch.cleanup().status,
                ScratchCleanupStatus.CLEAN,
            )

    def test_startup_janitor_preserves_explicitly_retained_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            retained = scratch.path
            scratch.mark_cleanup_ready()
            scratch.retain()
            lease = scratch._lease
            assert lease is not None
            lease.release()

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(records, [])
            self.assertTrue(retained.is_dir())
            self.assertEqual(
                scratch.cleanup().status,
                ScratchCleanupStatus.CLEAN,
            )

    def test_janitor_preserves_noncanonical_cleanup_ready_markers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            extra = ManagedScratch.create(parent)
            reordered = ManagedScratch.create(parent)
            for scratch in (extra, reordered):
                scratch.mark_cleanup_ready()
            extra_marker = extra.path / ".hoimin-cleanup-ready.json"
            extra_value = json.loads(extra_marker.read_text(encoding="utf-8"))
            extra_value["unexpected"] = True
            extra_marker.write_text(
                json.dumps(extra_value, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            reordered_marker = reordered.path / ".hoimin-cleanup-ready.json"
            reordered_value = json.loads(
                reordered_marker.read_text(encoding="utf-8")
            )
            reordered_marker.write_text(
                json.dumps(
                    {
                        "schema_version": reordered_value["schema_version"],
                        "run_id": reordered_value["run_id"],
                        "owner_kind": reordered_value["owner_kind"],
                        "lease_id": reordered_value["lease_id"],
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            extra_path = extra.path
            reordered_path = reordered.path
            extra.__del__()
            reordered.__del__()

            records = reclaim_abandoned(extra.managed_root)

            self.assertEqual(records, [])
            self.assertTrue(extra_path.is_dir())
            self.assertTrue(reordered_path.is_dir())

    def test_janitor_preserves_boolean_schema_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            marker = scratch.path / ".hoimin-cleanup-ready.json"
            value = json.loads(marker.read_text(encoding="utf-8"))
            value["schema_version"] = True
            marker.write_text(
                json.dumps(value, sort_keys=True) + "\n", encoding="utf-8"
            )
            path = scratch.path
            managed_root = scratch.managed_root
            scratch.close_capabilities()

            records = reclaim_abandoned(managed_root)

            self.assertEqual(records, [])
            self.assertTrue(path.is_dir())

    def test_owner_cleanup_propagates_one_absolute_deadline_to_all_helpers(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            (scratch.path / "payload").write_text("payload", encoding="utf-8")
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator", "_remove_payload"],
            )
            real_coordinator = lease_module._open_coordinator
            real_remove_payload = lease_module._remove_payload
            coordinator_deadlines: list[float | None] = []
            payload_deadlines: list[float] = []

            def observe_coordinator(
                *args: object, **kwargs: object
            ) -> LeaseLock:
                coordinator_deadlines.append(kwargs.get("deadline"))  # type: ignore[arg-type]
                return real_coordinator(*args, **kwargs)

            def observe_payload(
                *args: object, **kwargs: object
            ) -> object:
                payload_deadlines.append(
                    cast(float, kwargs["absolute_deadline"])
                )
                return real_remove_payload(*args, **kwargs)

            try:
                with (
                    mock.patch.object(
                        lease_module,
                        "_open_coordinator",
                        side_effect=observe_coordinator,
                    ),
                    mock.patch.object(
                        lease_module,
                        "_remove_payload",
                        side_effect=observe_payload,
                    ),
                ):
                    cleanup = self._task8_native_cleanup(scratch)
            finally:
                scratch.close_capabilities()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertGreaterEqual(len(coordinator_deadlines), 2)
            self.assertTrue(all(item is not None for item in coordinator_deadlines))
            self.assertGreaterEqual(len(payload_deadlines), 1)
            expected = coordinator_deadlines[0]
            self.assertTrue(
                all(item == expected for item in coordinator_deadlines)
            )
            self.assertTrue(all(item == expected for item in payload_deadlines))

    def test_janitor_propagates_one_absolute_deadline_to_all_helpers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            scratch.mark_cleanup_ready()
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator", "_open_owned_marker"],
            )
            real_coordinator = lease_module._open_coordinator
            real_open = lease_module._open_owned_marker
            coordinator_deadlines: list[float | None] = []
            marker_deadlines: list[float | None] = []

            def observe_coordinator(
                *args: object, **kwargs: object
            ) -> LeaseLock:
                coordinator_deadlines.append(kwargs.get("deadline"))  # type: ignore[arg-type]
                return real_coordinator(*args, **kwargs)

            def observe_open(
                *args: object, **kwargs: object
            ) -> object:
                marker_deadlines.append(
                    cast(float | None, kwargs.get("deadline"))
                )
                return real_open(*args, **kwargs)

            with (
                mock.patch.object(
                    lease_module,
                    "_open_coordinator",
                    side_effect=observe_coordinator,
                ),
                mock.patch.object(
                    lease_module,
                    "_open_owned_marker",
                    side_effect=observe_open,
                ),
            ):
                records = reclaim_abandoned(managed)

            self.assertTrue(
                any(
                    isinstance(item, ScratchCleanupRecord)
                    and item.status is ScratchCleanupStatus.CLEAN
                    for item in records
                )
            )
            self.assertTrue(coordinator_deadlines)
            self.assertTrue(all(item is not None for item in coordinator_deadlines))
            self.assertTrue(marker_deadlines)
            self.assertTrue(all(item is not None for item in marker_deadlines))
            self.assertEqual(len(set(marker_deadlines)), 1)

    def test_task9_janitor_surface_is_capability_backed(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=[
                "_read_valid_marker_at",
                "_reclaim_empty_unleased_candidate",
                "_resume_deferred_cleanup",
            ],
        )
        marker_parameters = inspect.signature(
            lease_module._read_valid_marker_at
        ).parameters
        empty_parameters = inspect.signature(
            lease_module._reclaim_empty_unleased_candidate
        ).parameters
        resume_parameters = inspect.signature(
            lease_module._resume_deferred_cleanup
        ).parameters

        self.assertIn("backend", marker_parameters)
        self.assertIn("directory", marker_parameters)
        self.assertNotIn("directory_fd", marker_parameters)
        self.assertIn("managed_root_capability", empty_parameters)
        self.assertNotIn("managed_fd", empty_parameters)
        self.assertIn("selection_root", resume_parameters)
        self.assertNotIn("selection_root_fd", resume_parameters)

    def test_task9_selection_uses_one_bounded_capability_inventory(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_select_janitor_candidates"],
        )
        backend = self._task8_backend()
        managed = self._task7_managed_root(backend)
        managed_node = backend._resource(managed).node
        cursor = "run-00000000-0000-4000-8000-000000000800"
        for index in range(300):
            name = (
                ".deleting-00000000-0000-4000-8000-"
                f"{index:012d}"
            )
            backend._new_node(
                EntryKind.DIRECTORY,
                SecurityDomain.MANAGED,
                parent=managed_node,
                name=name,
            )
        selected, examined = lease_module._select_janitor_candidates(
            managed,
            backend,
            cursor=cursor,
            deadline=time.monotonic() + 5.0,
        )

        self.assertEqual(examined, 300)
        self.assertEqual(len(selected), 256)
        self.assertEqual(len({item.name for item in selected}), 256)
        self.assertTrue(all(item.identity.file > 0 for item in selected))
        self.assertTrue(all(item.filesystem == managed.filesystem for item in selected))
        self.assertEqual(backend.events.count("reopen-directory:scan"), 1)
        self.assertEqual(backend.events.count("reopen-directory:preserve"), 0)
        managed.close()

    def test_task9_selection_accepts_exactly_one_hundred_thousand_children(
        self,
    ) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_select_janitor_candidates"],
        )
        backend = self._task8_backend()
        managed = self._task7_managed_root(backend)
        counts = iter((100_000, 100_001))

        class VirtualIterator:
            def __init__(
                self, directory: DirectoryCapability, count: int
            ) -> None:
                self.directory = directory
                self.remaining = count

            def __iter__(self) -> "VirtualIterator":
                return self

            def __next__(self) -> DirectoryEntry:
                if self.remaining == 0:
                    self.close()
                    raise StopIteration
                self.remaining -= 1
                return DirectoryEntry(
                    f"foreign-{self.remaining}",
                    EntryKind.DIRECTORY,
                    FileIdentity(0xA11CE, self.remaining + 1),
                    managed.filesystem,
                    0,
                    0,
                )

            def close(self) -> None:
                if self.directory.is_open:
                    self.directory.close()

        def virtual_inventory(
            scan: DirectoryCapability,
        ) -> DirectoryIterator:
            return cast(DirectoryIterator, VirtualIterator(scan, next(counts)))

        with mock.patch.object(
            backend, "entries_owned", side_effect=virtual_inventory
        ):
            selected, examined = lease_module._select_janitor_candidates(
                managed,
                backend,
                cursor="",
                deadline=time.monotonic() + 5.0,
            )
            self.assertEqual((selected, examined), ([], 100_000))
            with self.assertRaisesRegex(OSError, "100000"):
                lease_module._select_janitor_candidates(
                    managed,
                    backend,
                    cursor="",
                    deadline=time.monotonic() + 5.0,
                )

        managed.close()
        self.assertEqual(len(backend.live_resources), 0)

    def test_task9_late_marker_sharing_failure_is_not_live_busy(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_is_pin_step_live_owner"],
        )
        error = PermissionError(errno.EACCES, "sharing violation")
        setattr(error, "winerror", 32)
        self.assertTrue(lease_module._is_pin_step_live_owner(error, pin_step=True))
        self.assertFalse(
            lease_module._is_pin_step_live_owner(error, pin_step=False)
        )

    def test_task9_managed_create_activates_default_reclaimer(self) -> None:
        backend = self._task7_backend()
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["reclaim_abandoned"],
        )
        observed: list[tuple[Path, FilesystemBackend, DirectoryCapability]] = []
        real_open_coordinator = lease_module._open_coordinator

        def observe(
            path: Path,
            *,
            backend: FilesystemBackend,
            managed_root_capability: DirectoryCapability,
            **_kwargs: object,
        ) -> list[ScratchCleanupRecord | JanitorDiagnostic]:
            observed.append((path, backend, managed_root_capability))
            self.assertIs(managed_root_capability.share_policy, SharePolicy.MUTATION)
            self.assertTrue(managed_root_capability.is_open)
            return []

        def open_coordinator(*args: object, **kwargs: object) -> LeaseLock:
            coordinator = real_open_coordinator(*args, **kwargs)
            backend.coordinator = coordinator
            return coordinator

        with (
            mock.patch.object(
                lease_module, "reclaim_abandoned", side_effect=observe
            ),
            mock.patch.object(
                lease_module, "_open_coordinator", side_effect=open_coordinator
            ),
        ):
            scratch = ManagedScratch.create(
                backend.parent_path,
                run_id="00000000-0000-4000-8000-000000000909",
                backend=backend,
            )
        self.assertEqual(len(observed), 1)
        self.assertIs(observed[0][1], backend)
        self.assertFalse(observed[0][2].is_open)
        scratch.close_capabilities()

    @unittest.skipUnless(os.name == "nt", "requires native Windows locks")
    def test_task9_native_live_lease_is_preserved_then_reclaimed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            live = ManagedScratch.create(parent)
            live_path = live.path

            observer = ManagedScratch.create(parent)
            self.assertTrue(live_path.is_dir())
            observer.mark_cleanup_ready()
            self.assertEqual(
                observer.cleanup().status, ScratchCleanupStatus.CLEAN
            )

            live.mark_cleanup_ready()
            live.close_capabilities()
            records = reclaim_abandoned(live.managed_root)

            self.assertFalse(live_path.exists())
            self.assertEqual(
                [item.status for item in _cleanup_records_only(records)],
                [ScratchCleanupStatus.CLEAN],
            )

    def test_task9_recording_backend_claim_is_relative_and_identity_bound(self) -> None:
        backend = self._task8_backend(
            rename_requires_closed_descendants=True
        )
        managed_capability = self._task7_managed_root(backend)
        managed = backend._resource(managed_capability).node
        run_id = "00000000-0000-4000-8000-000000000910"
        root = self._task9_add_candidate(
            backend, managed, run_id=run_id, ready=True
        )
        selected_identity = root.identity

        records = reclaim_abandoned(
            managed_capability.path_hint,
            now=24 * 60 * 60 + 1.0,
            backend=backend,
            managed_root_capability=managed_capability,
        )

        self.assertFalse(
            any(node.identity == selected_identity for node in managed.children.values())
        )
        self.assertTrue(
            any(
                isinstance(item, ScratchCleanupRecord)
                and item.status is ScratchCleanupStatus.CLEAN
                for item in records
            ),
            records,
        )
        self.assertTrue(
            any(event.startswith("open-directory:run-") for event in backend.events)
        )
        self.assertTrue(
            any(event.startswith("rename:run-") for event in backend.events)
        )
        self.assertEqual(len(backend.live_resources), 0)


class OwnedOutputTests(unittest.TestCase):
    def test_atomic_report_kind_selects_its_fixed_destination(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            owner = OwnedOutput.create(
                output, "00000000-0000-4000-8000-000000000117"
            )
            sentinel = Path(directory) / "sentinel"
            sentinel.write_text("outside", encoding="utf-8")
            try:
                owner.write_atomic(
                    "json",
                    _write_text("{}\n"),
                )
                with self.assertRaisesRegex(ValueError, "report kind"):
                    owner.write_atomic(
                        "../sentinel",
                        _write_text("overwritten"),
                    )

                self.assertEqual(
                    (output / "run.json").read_text(encoding="utf-8"), "{}\n"
                )
                self.assertEqual(sentinel.read_text(encoding="utf-8"), "outside")
            finally:
                owner.close(remove_marker=True)

    @unittest.skipIf(os.name == "nt", "descriptor close injection is Unix-specific")
    def test_atomic_report_rollback_attempts_unlink_after_fd_close_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            run_id = "00000000-0000-4000-8000-000000000112"
            owner = OwnedOutput.create(output, run_id)
            opened: list[int] = []
            real_close = os.close

            def fail_fdopen(fd: int, *args: object, **kwargs: object) -> object:
                del args, kwargs
                opened.append(fd)
                raise ValueError("injected report primary")

            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.store.os.fdopen",
                        side_effect=fail_fdopen,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.store.os.close",
                        side_effect=OSError("injected temporary close failure"),
                    ),
                    self.assertRaisesRegex(
                        ValueError, "report primary"
                    ) as caught,
                ):
                    owner.write_atomic(
                        "json", _write_text("new")
                    )

                self.assertTrue(
                    any(
                        "temporary close failure" in note
                        for note in caught.exception.__notes__
                    )
                )
                self.assertFalse(
                    (output / f".hoimin-output-{run_id}-json.tmp").exists()
                )
            finally:
                for descriptor in opened:
                    try:
                        real_close(descriptor)
                    except OSError:
                        pass
                owner.close(remove_marker=True)

    @unittest.skipIf(os.name == "nt", "unlink-open-marker semantics are Unix-specific")
    def test_constructor_rollback_attempts_all_closes_after_close_errors(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            opened: list[int] = []
            real_open = os.open
            real_close = os.close

            def record_open(*args: object, **kwargs: object) -> int:
                descriptor = real_open(*args, **kwargs)  # type: ignore[arg-type]
                opened.append(descriptor)
                return descriptor

            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.store.os.open",
                        side_effect=record_open,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.store.os.write",
                        return_value=0,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.store.os.close",
                        side_effect=OSError("injected rollback close failure"),
                    ),
                    self.assertRaisesRegex(OSError, "made no progress") as caught,
                ):
                    OwnedOutput.create(
                        output, "00000000-0000-4000-8000-000000000094"
                    )

                self.assertGreaterEqual(len(caught.exception.__notes__), 2)
                self.assertFalse((output / ".hoimin-output-owner").exists())
            finally:
                for descriptor in opened:
                    try:
                        real_close(descriptor)
                    except OSError:
                        pass

    def test_recovery_preserves_bool_or_extra_field_schema_markers(self) -> None:
        for mutation in ("bool-schema", "extra-field"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "output"
                previous = OwnedOutput.create(
                    output, "00000000-0000-4000-8000-000000000096"
                )
                previous.close()
                marker = output / ".hoimin-output-owner"
                value = json.loads(marker.read_text(encoding="utf-8"))
                if mutation == "bool-schema":
                    value["schema_version"] = True
                else:
                    value["unexpected"] = "preserve"
                marker.write_text(
                    json.dumps(value, sort_keys=True) + "\n", encoding="utf-8"
                )

                with self.assertRaises(ValueError):
                    OwnedOutput.create(
                        output, "00000000-0000-4000-8000-000000000095"
                    )

                self.assertTrue(marker.is_file())

    def test_output_close_attempts_directory_after_marker_close_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            owner = OwnedOutput.create(
                output, "00000000-0000-4000-8000-000000000097"
            )
            marker_fd = owner._marker_fd
            directory_fd = owner._directory_fd
            real_close = os.close

            def close_with_marker_failure(fd: int) -> None:
                if fd == marker_fd:
                    raise OSError("injected marker close failure")
                real_close(fd)

            try:
                with mock.patch(
                    "tools.focused_mutation_support.store.os.close",
                    side_effect=close_with_marker_failure,
                ):
                    errors = owner.close(remove_marker=True)
                self.assertTrue(any("marker close" in item for item in errors))
                with self.assertRaises(OSError):
                    os.fstat(directory_fd)
            finally:
                real_close(marker_fd)

    def test_zero_progress_output_marker_write_leaves_no_owner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            with (
                mock.patch(
                    "tools.focused_mutation_support.store.os.write",
                    return_value=0,
                ),
                self.assertRaisesRegex(OSError, "made no progress"),
            ):
                OwnedOutput.create(
                    output, "00000000-0000-4000-8000-000000000098"
                )

            self.assertFalse((output / ".hoimin-output-owner").exists())

    def test_post_flush_sample_can_reject_atomic_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            owner = OwnedOutput.create(
                output, "00000000-0000-4000-8000-000000000099"
            )
            destination = output / "run.json"
            destination.write_text("old", encoding="utf-8")
            store = RunStore(owner)
            policy = DiskPolicy(
                max_disk_bytes=1024 * 1024,
                min_free_bytes=1,
                scratch_root=Path(directory),
            )
            calls = 0

            def sample() -> tuple[DiskFailure | None, DiskObservation | None]:
                nonlocal calls
                calls += 1
                return None, DiskObservation(0, 1024 * 1024)

            store.configure_report_guard(policy, sample)
            with mock.patch.object(
                owner, "post_flush_available_bytes", return_value=1
            ):
                with self.assertRaisesRegex(ReportTooLarge, "post-flush"):
                    store.checkpoint(RunRecord.new(total_budget_seconds=1.0))

            self.assertEqual(calls, 1)
            self.assertEqual(destination.read_text(encoding="utf-8"), "old")
            owner.close(remove_marker=True)

    def test_report_generation_change_prevents_atomic_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            run_id = "00000000-0000-4000-8000-000000000017"
            owner = OwnedOutput.create(output, run_id)
            destination = output / "run.json"
            destination.write_text("old", encoding="utf-8")
            store = RunStore(owner)
            policy = DiskPolicy(max_disk_bytes=1024 * 1024, min_free_bytes=1)

            @contextmanager
            def invalidated_generation():
                yield lambda: False

            store.configure_report_guard(
                policy,
                lambda: (None, DiskObservation(0, 1024 * 1024)),
                invalidated_generation,
            )

            with self.assertRaisesRegex(ReportTooLarge, "generation changed"):
                store.checkpoint(RunRecord.new(total_budget_seconds=1.0))

            self.assertEqual(destination.read_text(encoding="utf-8"), "old")
            self.assertFalse(
                (output / f".hoimin-output-{run_id}-json.tmp").exists()
            )
            owner.close(remove_marker=True)

    def test_only_one_initializer_can_lock_an_empty_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            first = OwnedOutput.create(
                output, "00000000-0000-4000-8000-000000000001"
            )
            self.addCleanup(first.close)

            with self.assertRaises((FileExistsError, ValueError, OSError)):
                OwnedOutput.create(
                    output, "00000000-0000-4000-8000-000000000002"
                )

    def test_path_replacement_cannot_redirect_atomic_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            output = parent / "output"
            replacement = parent / "replacement"
            replacement.mkdir()
            owner = OwnedOutput.create(
                output, "00000000-0000-4000-8000-000000000003"
            )
            moved = parent / "moved"
            output.rename(moved)
            output.symlink_to(replacement, target_is_directory=True)

            with self.assertRaisesRegex(OSError, "identity changed"):
                owner.write_atomic(
                    "json",
                    _write_text("{}\n"),
                )

            self.assertFalse((replacement / "run.json").exists())
            output.unlink()
            moved.rename(output)
            owner.close(remove_marker=True)

    def test_abandoned_deterministic_temporary_is_recovered(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            old_run = "00000000-0000-4000-8000-000000000004"
            previous = OwnedOutput.create(output, old_run)
            temporary = output / f".hoimin-output-{old_run}-json.tmp"
            temporary.write_text("partial", encoding="utf-8")
            previous.close()

            current = OwnedOutput.create(
                output, "00000000-0000-4000-8000-000000000005"
            )

            self.assertEqual(current.recovered_temporary_count, 1)
            self.assertFalse(temporary.exists())
            current.close(remove_marker=True)

    def test_recovery_precedes_reserve_check_and_fresh_marker_publication(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            old_run = "00000000-0000-4000-8000-000000000014"
            previous = OwnedOutput.create(output, old_run)
            temporary = output / f".hoimin-output-{old_run}-json.tmp"
            temporary.write_text("partial", encoding="utf-8")
            previous.close()
            capacity = mock.Mock(f_bavail=100, f_frsize=1)

            with (
                mock.patch(
                    "tools.focused_mutation_support.store.os.fstatvfs",
                    return_value=capacity,
                ),
                self.assertRaisesRegex(ValueError, "reserve"),
            ):
                OwnedOutput.create(
                    output,
                    "00000000-0000-4000-8000-000000000015",
                    min_free_bytes=100,
                )

            self.assertFalse(temporary.exists())
            self.assertFalse((output / ".hoimin-output-owner").exists())

    def test_recovery_preserves_foreign_neighbor_and_rejects_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            old_run = "00000000-0000-4000-8000-000000000006"
            previous = OwnedOutput.create(output, old_run)
            sentinel = output / "sentinel"
            sentinel.write_text("keep", encoding="utf-8")
            previous.close()

            with self.assertRaisesRegex(ValueError, "empty"):
                OwnedOutput.create(
                    output, "00000000-0000-4000-8000-000000000007"
                )

            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_recovery_rejects_noncanonical_run_id_without_deleting_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            output.mkdir()
            marker = output / ".hoimin-output-owner"
            marker.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "run_id": "not-a-uuid",
                        "owner_kind": "focused_python",
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            temporary = output / ".hoimin-output-not-a-uuid-json.tmp"
            temporary.write_text("foreign", encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "empty"):
                OwnedOutput.create(
                    output, "00000000-0000-4000-8000-000000000009"
                )

            self.assertEqual(temporary.read_text(encoding="utf-8"), "foreign")
            self.assertTrue(marker.is_file())

    def test_recovery_rejects_owner_marker_copied_from_another_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "original"
            old_run = "00000000-0000-4000-8000-000000000012"
            previous = OwnedOutput.create(original, old_run)
            previous.close()

            output = root / "output"
            output.mkdir()
            marker = output / ".hoimin-output-owner"
            marker.write_bytes((original / marker.name).read_bytes())
            temporary = output / f".hoimin-output-{old_run}-json.tmp"
            temporary.write_text("foreign", encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "empty"):
                OwnedOutput.create(
                    output,
                    "00000000-0000-4000-8000-000000000013",
                )

            self.assertEqual(temporary.read_text(encoding="utf-8"), "foreign")
            self.assertTrue(marker.is_file())

    def test_report_cap_removes_partial_temporary_and_preserves_destination(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            run_id = "00000000-0000-4000-8000-000000000008"
            owner = OwnedOutput.create(output, run_id)
            (output / "run.json").write_text("old", encoding="utf-8")

            with self.assertRaises(ReportTooLarge):
                owner.write_atomic(
                    "json",
                    _write_text("x" * 1025),
                    capacity=1024,
                )

            self.assertEqual(
                (output / "run.json").read_text(encoding="utf-8"), "old"
            )
            self.assertFalse(
                (output / f".hoimin-output-{run_id}-json.tmp").exists()
            )
            owner.close(remove_marker=True)

    def test_post_flush_guard_runs_before_atomic_report_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            run_id = "00000000-0000-4000-8000-000000000016"
            owner = OwnedOutput.create(output, run_id)
            destination = output / "run.json"
            destination.write_text("old", encoding="utf-8")

            with self.assertRaisesRegex(ReportTooLarge, "post-flush"):
                owner.write_atomic(
                    "json",
                    _write_text("new"),
                    after_flush=lambda _written: (_ for _ in ()).throw(
                        ReportTooLarge("post-flush reserve reached")
                    ),
                )

            self.assertEqual(destination.read_text(encoding="utf-8"), "old")
            self.assertFalse(
                (output / f".hoimin-output-{run_id}-json.tmp").exists()
            )
            owner.close(remove_marker=True)

    def test_finalizer_never_recursively_removes_scratch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            leased = scratch.path
            managed = scratch.managed_root
            scratch.mark_cleanup_ready()
            scratch.__del__()
            self.assertTrue(leased.is_dir())
            self.assertEqual(
                [
                    record.status
                    for record in _cleanup_records_only(
                        reclaim_abandoned(managed)
                    )
                ],
                [ScratchCleanupStatus.CLEAN],
            )

    def test_finalizer_closes_lease_descriptor_without_deleting_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            leased = scratch.path
            managed = scratch.managed_root
            lease = scratch._lease
            assert lease is not None
            lease_fd = lease.fd
            scratch.mark_cleanup_ready()

            scratch.__del__()

            with self.assertRaises(OSError):
                os.fstat(lease_fd)
            self.assertTrue(leased.is_dir())
            self.assertEqual(
                [
                    record.status
                    for record in _cleanup_records_only(
                        reclaim_abandoned(managed)
                    )
                ],
                [ScratchCleanupStatus.CLEAN],
            )


if __name__ == "__main__":
    unittest.main()
