from pathlib import Path
import argparse
from contextlib import contextmanager
from collections.abc import Callable
import gc
import json
import os
import stat
import sys
import tempfile
import threading
import time
import unittest
import uuid
from unittest import mock
from typing import BinaryIO, cast
import zlib

from tools.focused_mutation import Options, _parser, options_from_arguments
from tools.focused_mutation_support.disk import (
    MAX_COMMAND_LOG_BYTES,
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
    ) -> None:
        self.identity = identity
        self.filesystem = filesystem
        self.kind = kind
        self.security_domain = security_domain
        self.parent = parent
        self.name = name
        self.backing = backing
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
            0,
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
        if node.backing is None:
            raise RuntimeError("recorded regular file has no backing descriptor")
        self._prune_detached()
        descriptor = os.dup(node.backing.fileno())
        os.lseek(descriptor, 0, os.SEEK_SET)
        resource = _ManagedRecordedResource(node, descriptor=descriptor)
        self.live_resources.add(resource)
        return FileCapability(
            self,
            resource,
            identity=node.identity,
            filesystem=node.filesystem,
            kind=EntryKind.REGULAR,
            logical_size=os.fstat(descriptor).st_size,
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
    ) -> _ManagedRecordedIterator:
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

        def fail_consuming_close(event: str) -> None:
            nonlocal close_attempts
            if event != f"close:{marker_name}":
                return
            close_attempts += 1
            if close_attempts > 3:
                return
            for resource in backend.live_resources:
                if resource.node is marker_node:
                    resource.close_failures = 1

        backend.after_event = fail_consuming_close
        start = len(backend.events)
        errors = lease_module._delete_exact_marker(
            managed,
            marker_name,
            identity,
            backend,
        )

        cleanup_events = backend.events[start:]
        self.assertTrue(any("close failed" in error for error in errors))
        self.assertNotIn(f"entry:{marker_name}", cleanup_events)
        managed.close()

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

                    def close_retained_owners() -> None:
                        for capability in retained_heartbeat_capabilities:
                            if capability.is_open:
                                capability.close()
                        for lock in allocated_locks:
                            if lock.fd >= 0:
                                lock.close()

                    backend.after_event = observe_rollback
                    self.addCleanup(close_retained_owners)
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
        for boundary in ("lease", "heartbeat"):
            with self.subTest(boundary=boundary):
                backend = self._backend(rename_requires_closed_descendants=True)
                allocated_locks: list[LeaseLock] = []
                retained_capabilities: list[FileCapability] = []
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

                def close_retained_owners() -> None:
                    for capability in retained_capabilities:
                        if capability.is_open:
                            capability.close()
                    for lock in allocated_locks:
                        if lock.fd >= 0:
                            lock.close()

                backend.after_event = inject_primary_and_observe
                self.addCleanup(close_retained_owners)
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

    def test_hidden_helper_owner_blocks_marker_and_staging_rollback(self) -> None:
        lease_module = __import__(
            "tools.focused_mutation_support.lease",
            fromlist=["_OwnedDescriptor", "_close_locked_coordinator_once"],
        )
        active_name = f"run-{self.run_id}"
        staging_name = f".staging-{self.run_id}"
        lease_name = ".hoimin-lease.json"
        for stage in ("create-capability", "read-descriptor", "lease-capability"):
            with self.subTest(stage=stage):
                backend = self._backend(rename_requires_closed_descendants=True)
                allocated_locks: list[LeaseLock] = []
                descriptor_owners: list[_OwnedDescriptor] = []
                retained_capabilities: list[FileCapability] = []
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

                def close_retained_owners() -> None:
                    for capability in retained_capabilities:
                        if capability.is_open:
                            capability.close()
                    for owner in descriptor_owners:
                        if owner.fd >= 0:
                            owner.close_once("test retained marker")
                    for lock in allocated_locks:
                        if lock.fd >= 0:
                            lock.close()

                backend.after_event = observe_and_inject
                self.addCleanup(close_retained_owners)
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
                root_close_lock_states.append(
                    coordinator is not None and coordinator.locked
                )

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
        with tempfile.TemporaryDirectory() as directory:
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            parent_fd = os.open(directory, os.O_RDONLY)
            self.addCleanup(os.close, parent_fd)
            real_identity = lease_module._directory_identity
            crossed = False
            identity_calls = 0

            def crossing_identity(fd: int) -> tuple[int, int]:
                nonlocal crossed, identity_calls
                identity_calls += 1
                value = real_identity(fd)
                crossed = True
                return value

            with (
                mock.patch.object(
                    lease_module,
                    "_directory_identity",
                    side_effect=crossing_identity,
                ),
                self.assertRaisesRegex(TimeoutError, "directory reopen"),
            ):
                lease_module._open_directory_at(
                    parent_fd,
                    ".",
                    deadline=5.0,
                    monotonic=lambda: 6.0 if crossed else 0.0,
                )

            self.assertEqual(identity_calls, 1)

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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            deleting = managed / f".deleting-{scratch.run_id}"
            identity = scratch._root_identity
            scratch.mark_cleanup_ready()
            scratch.path.rename(deleting)
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_resume_deferred_cleanup"],
            )
            selection_fd = os.open(managed, os.O_RDONLY)
            real_open = lease_module._open_directory_at
            real_close = os.close
            opened: list[int] = []

            def record_open(
                parent_fd: int,
                name: str,
                *,
                deadline: float | None = None,
                monotonic: Callable[[], float] | None = None,
            ) -> int:
                descriptor = real_open(
                    parent_fd,
                    name,
                    deadline=deadline,
                    monotonic=monotonic,
                )
                opened.append(descriptor)
                return descriptor

            def close_with_root_failure(descriptor: int) -> None:
                real_close(descriptor)
                if len(opened) >= 2 and descriptor == opened[1]:
                    raise OSError("injected root capability close failure")

            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.lease._open_directory_at",
                        side_effect=record_open,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease._read_valid_marker_at",
                        side_effect=TimeoutError("injected deferred deadline"),
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease.os.close",
                        side_effect=close_with_root_failure,
                    ),
                ):
                    result = lease_module._resume_deferred_cleanup(
                        managed,
                        selection_fd,
                        deleting.name,
                        identity,
                        deadline=time.monotonic() + 30.0,
                    )
            finally:
                real_close(selection_fd)

            self.assertIsInstance(result, JanitorDiagnostic)
            self.assertIn("injected deferred deadline", "; ".join(result.details))
            self.assertIn(
                "root capability close failure", "; ".join(result.details)
            )
            self.assertEqual(len(opened), 2)
            for descriptor in opened:
                with self.assertRaises(OSError):
                    os.fstat(descriptor)

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
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            scratch = ManagedScratch.create(parent)
            scratch.mark_cleanup_ready()
            candidate = scratch.path
            managed = scratch.managed_root
            moved = parent / "managed-original"
            replacement = managed
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_persist_coordinator_cursor"],
            )
            real_persist = lease_module._persist_coordinator_cursor
            swapped = False

            def replace_after_selection(*args: object, **kwargs: object) -> None:
                nonlocal swapped
                real_persist(*args, **kwargs)
                managed.rename(moved)
                replacement.mkdir(mode=0o700)
                (replacement / "sentinel").write_text("keep", encoding="utf-8")
                swapped = True

            with mock.patch(
                "tools.focused_mutation_support.lease._persist_coordinator_cursor",
                side_effect=replace_after_selection,
            ):
                records = reclaim_abandoned(managed)

            self.assertTrue(swapped)
            self.assertEqual((replacement / "sentinel").read_text(), "keep")
            self.assertFalse((moved / candidate.name).exists())
            self.assertTrue(
                any(
                    item.status is ScratchCleanupStatus.CLEAN
                    for item in _cleanup_records_only(records)
                )
            )

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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_close_descriptors_all"],
            )
            real_close_all = lease_module._close_descriptors_all
            real_read_state = lease_module._read_coordinator_state
            read_calls = 0

            def fail_second_read(*args: object, **kwargs: object) -> object:
                nonlocal read_calls
                read_calls += 1
                if read_calls == 2:
                    raise TimeoutError("injected selection deadline")
                return real_read_state(*args, **kwargs)

            def close_with_secondary(
                descriptors: tuple[tuple[str, int], ...],
            ) -> tuple[str, ...]:
                errors = real_close_all(descriptors)
                if any(label == "janitor selection root" for label, _ in descriptors):
                    return (*errors, "injected selection root close failure")
                return errors

            with (
                mock.patch.object(
                    lease_module,
                    "_read_coordinator_state",
                    side_effect=fail_second_read,
                ),
                mock.patch.object(
                    lease_module,
                    "_close_descriptors_all",
                    side_effect=close_with_secondary,
                ),
                self.assertRaisesRegex(
                    TimeoutError, "injected selection deadline"
                ) as caught,
            ):
                reclaim_abandoned(
                    scratch.managed_root,
                    managed_root_fd=scratch._managed_root_fd,
                )

            self.assertTrue(
                any(
                    "selection root close failure" in note
                    for note in getattr(caught.exception, "__notes__", ())
                )
            )
            scratch.mark_cleanup_ready()
            scratch.cleanup()

    def test_candidate_deadline_keeps_lease_close_failure_secondary(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            managed_root = scratch.managed_root
            scratch.close_capabilities()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_close_descriptors_all"],
            )
            real_open = lease_module.os.open
            real_close_all = lease_module._close_descriptors_all
            real_entry_identity = lease_module._entry_identity
            lease_open_count = 0
            deadline_triggered = False

            def observe_open(name: object, *args: object, **kwargs: object) -> int:
                nonlocal lease_open_count
                descriptor = real_open(name, *args, **kwargs)  # type: ignore[arg-type]
                if name == ".hoimin-lease.json":
                    lease_open_count += 1
                return descriptor

            def deadline_after_lease(_deadline: float, _label: str) -> None:
                nonlocal deadline_triggered
                if lease_open_count >= 2:
                    deadline_triggered = True
                    raise TimeoutError("injected candidate deadline")

            def forbid_post_deadline_identity(
                parent_fd: int, name: str
            ) -> tuple[int, int]:
                if deadline_triggered:
                    raise AssertionError(
                        "identity recovery started after janitor deadline"
                    )
                return real_entry_identity(parent_fd, name)

            def close_with_secondary(
                descriptors: tuple[tuple[str, int], ...],
            ) -> tuple[str, ...]:
                errors = real_close_all(descriptors)
                if any(label == "janitor candidate lease" for label, _ in descriptors):
                    return (*errors, "injected candidate lease close failure")
                return errors

            with (
                mock.patch.object(lease_module.os, "open", side_effect=observe_open),
                mock.patch.object(
                    lease_module,
                    "_check_deadline",
                    side_effect=deadline_after_lease,
                ),
                mock.patch.object(
                    lease_module,
                    "_close_descriptors_all",
                    side_effect=close_with_secondary,
                ),
                mock.patch.object(
                    lease_module,
                    "_entry_identity",
                    side_effect=forbid_post_deadline_identity,
                ),
            ):
                records = reclaim_abandoned(managed_root)

            detail = "; ".join(
                item for record in records for item in record.details
            )
            self.assertIn("injected candidate deadline", detail)
            self.assertIn("injected candidate lease close failure", detail)

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
        with self.assertRaises(ValueError) as caught:
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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.retain()
            managed = scratch.managed_root
            scratch.close_capabilities()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_close_lease_lock_all"],
            )
            real_close_all = lease_module._close_lease_lock_all

            def fail_candidate_close(
                lock: LeaseLock,
                label: str,
            ) -> tuple[str, ...]:
                errors = real_close_all(lock, label)
                if label == "janitor candidate lease":
                    return (*errors, "injected candidate lease close failure")
                return errors

            with mock.patch(
                "tools.focused_mutation_support.lease._close_lease_lock_all",
                side_effect=fail_candidate_close,
            ):
                records = reclaim_abandoned(managed)

            self.assertTrue(
                any(
                    "candidate lease close failure" in "; ".join(record.details)
                    for record in records
                )
            )

    def test_janitor_reports_transferred_lease_disposal_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            scratch.mark_cleanup_ready()
            scratch.__del__()
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
                remaining_root=str(scratch.path),
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
                mock.patch(
                    "tools.focused_mutation_support.lease._close_lease_lock_all",
                    side_effect=fail_transferred_close,
                ),
            ):
                records = reclaim_abandoned(managed)

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
        with tempfile.TemporaryDirectory() as directory:
            seed = ManagedScratch.create(Path(directory))
            managed = seed.managed_root
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            run_id = "00000000-0000-4000-8000-000000000121"
            candidate = managed / f".staging-{run_id}"
            candidate.mkdir(mode=0o700)
            os.utime(candidate, (0.0, 0.0))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_reclaim_empty_unleased_candidate"],
            )
            managed_fd = os.open(managed, os.O_RDONLY)
            real_open = lease_module._open_directory_at
            crossed = False

            def cross_during_candidate_open(
                parent_fd: int,
                name: str,
                *,
                deadline: float | None = None,
                monotonic: Callable[[], float] | None = None,
            ) -> int:
                nonlocal crossed
                fd = real_open(
                    parent_fd,
                    name,
                    deadline=deadline,
                    monotonic=monotonic,
                )
                if name == candidate.name:
                    crossed = True
                return fd

            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.lease._open_directory_at",
                        side_effect=cross_during_candidate_open,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease.time.monotonic",
                        side_effect=lambda: 31.0 if crossed else 0.0,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease.os.fstat",
                        side_effect=AssertionError(
                            "candidate stat started after janitor deadline"
                        ),
                    ),
                ):
                    result = lease_module._reclaim_empty_unleased_candidate(
                        managed,
                        managed_fd,
                        candidate.name,
                        current_time=24 * 60 * 60 + 1.0,
                        deadline=30.0,
                    )
            finally:
                os.close(managed_fd)

            self.assertIsInstance(result, JanitorDiagnostic)
            self.assertTrue(candidate.is_dir())

    def test_janitor_selection_lock_uses_only_remaining_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            seed = ManagedScratch.create(Path(directory))
            managed = seed.managed_root
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            managed_fd = os.open(managed, os.O_RDONLY)
            observed_timeouts: list[float] = []
            current_time = 0.0
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            real_open = lease_module._open_directory_at

            def selection_open(
                parent_fd: int, name: str, **kwargs: object
            ) -> int:
                nonlocal current_time
                descriptor = real_open(parent_fd, name, **kwargs)
                current_time = 4.0
                return descriptor

            def stop_at_coordinator(
                root: Path,
                *,
                root_fd: int | None = None,
                timeout: float = 5.0,
                **_kwargs: object,
            ) -> LeaseLock:
                del root, root_fd
                observed_timeouts.append(timeout)
                raise TimeoutError("injected selection contention")

            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.lease.time.monotonic",
                        side_effect=lambda: current_time,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease._open_directory_at",
                        side_effect=selection_open,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease._open_coordinator",
                        side_effect=stop_at_coordinator,
                    ),
                    self.assertRaisesRegex(TimeoutError, "selection contention"),
                ):
                    reclaim_abandoned(managed, managed_root_fd=managed_fd)
            finally:
                os.close(managed_fd)

            self.assertEqual(observed_timeouts, [1.0])

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
                "tools.focused_mutation_support.lease.os.pwrite",
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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            deleting = managed / f".deleting-{scratch.run_id}"
            identity = scratch._root_identity
            scratch.mark_cleanup_ready()
            scratch.path.rename(deleting)
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_resume_deferred_cleanup"],
            )
            selection_fd = os.open(managed, os.O_RDONLY)
            real_identity = lease_module._directory_identity
            crossed = False
            entry_identity_started = False

            def crossing_identity(descriptor: int) -> tuple[int, int]:
                nonlocal crossed
                result = real_identity(descriptor)
                if result == identity:
                    crossed = True
                return result

            def forbid_entry_identity(
                _parent_fd: int, _name: str
            ) -> tuple[int, int]:
                nonlocal entry_identity_started
                entry_identity_started = True
                raise AssertionError("entry identity started after deadline")

            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.lease._directory_identity",
                        side_effect=crossing_identity,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease._entry_identity",
                        side_effect=forbid_entry_identity,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease.time.monotonic",
                        side_effect=lambda: 31.0 if crossed else 0.0,
                    ),
                ):
                    result = lease_module._resume_deferred_cleanup(
                        managed,
                        selection_fd,
                        deleting.name,
                        identity,
                        deadline=30.0,
                    )
            finally:
                os.close(selection_fd)

            self.assertFalse(entry_identity_started)
            self.assertIsInstance(result, JanitorDiagnostic)

    def test_janitor_stops_after_root_identity_crosses_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            candidate = scratch.path
            candidate_identity = (candidate.stat().st_dev, candidate.stat().st_ino)
            scratch.mark_cleanup_ready()
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_directory_identity"],
            )
            real_identity = lease_module._directory_identity
            real_entry_identity = lease_module._entry_identity
            crossed = False
            post_deadline_entry_started = False

            def cross_during_candidate_identity(fd: int) -> tuple[int, int]:
                nonlocal crossed
                identity = real_identity(fd)
                if identity == candidate_identity:
                    crossed = True
                return identity

            def forbid_post_deadline_entry_identity(
                parent_fd: int, name: str
            ) -> tuple[int, int]:
                nonlocal post_deadline_entry_started
                if crossed:
                    post_deadline_entry_started = True
                return real_entry_identity(parent_fd, name)

            managed_fd = os.open(managed, os.O_RDONLY)
            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.lease._directory_identity",
                        side_effect=cross_during_candidate_identity,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease._entry_identity",
                        side_effect=forbid_post_deadline_entry_identity,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease.time.monotonic",
                        side_effect=lambda: 31.0 if crossed else 0.0,
                    ),
                ):
                    records = reclaim_abandoned(
                        managed, managed_root_fd=managed_fd
                    )
            finally:
                os.close(managed_fd)

            self.assertTrue(
                any(isinstance(item, JanitorDiagnostic) for item in records)
            )
            self.assertFalse(post_deadline_entry_started)
            self.assertTrue(candidate.is_dir())

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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            deleting = managed / f".deleting-{scratch.run_id}"
            identity = scratch._root_identity
            scratch.mark_cleanup_ready()
            scratch.path.rename(deleting)
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_resume_deferred_cleanup"],
            )
            selection_fd = os.open(managed, os.O_RDONLY)
            real_open = lease_module._open_directory_at
            crossed = False
            candidate_open_started = False

            def cross_during_managed_open(
                parent_fd: int,
                name: str,
                *,
                deadline: float | None = None,
                monotonic: Callable[[], float] | None = None,
            ) -> int:
                nonlocal crossed, candidate_open_started
                if crossed:
                    candidate_open_started = True
                fd = real_open(
                    parent_fd,
                    name,
                    deadline=deadline,
                    monotonic=monotonic,
                )
                if name == ".":
                    crossed = True
                return fd

            try:
                with (
                    mock.patch(
                        "tools.focused_mutation_support.lease._open_directory_at",
                        side_effect=cross_during_managed_open,
                    ),
                    mock.patch(
                        "tools.focused_mutation_support.lease.time.monotonic",
                        side_effect=lambda: 31.0 if crossed else 0.0,
                    ),
                ):
                    result = lease_module._resume_deferred_cleanup(
                        managed,
                        selection_fd,
                        deleting.name,
                        identity,
                        deadline=30.0,
                    )
            finally:
                os.close(selection_fd)

            self.assertFalse(candidate_open_started)
            self.assertIsInstance(result, JanitorDiagnostic)
            self.assertTrue(deleting.is_dir())

    def test_empty_janitor_removal_stays_clean_after_coordinator_close_error(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            seed = ManagedScratch.create(parent)
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            run_id = "00000000-0000-4000-8000-000000000111"
            candidate = managed / f".staging-{run_id}"
            candidate.mkdir(mode=0o700)
            os.utime(candidate, (0.0, 0.0))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_close_lease_lock_all"],
            )
            real_close_all = lease_module._close_lease_lock_all

            def fail_empty_close(
                lock: LeaseLock,
                label: str,
            ) -> tuple[str, ...]:
                errors = real_close_all(lock, label)
                if label == "empty janitor coordinator":
                    return (*errors, "injected empty coordinator close failure")
                return errors

            with mock.patch(
                "tools.focused_mutation_support.lease._close_lease_lock_all",
                side_effect=fail_empty_close,
            ):
                records = reclaim_abandoned(managed, now=24 * 60 * 60 + 1.0)

            self.assertEqual(len(records), 1)
            record = records[0]
            assert isinstance(record, ScratchCleanupRecord)
            self.assertEqual(record.status, ScratchCleanupStatus.CLEAN)
            self.assertIsNone(record.remaining_root)
            self.assertIn("empty coordinator close failure", record.details[0])
            self.assertFalse(candidate.exists())

    def test_empty_janitor_early_exit_preserves_coordinator_close_error(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            run_id = "00000000-0000-4000-8000-000000000112"
            candidate = scratch.managed_root / f".staging-{run_id}"
            candidate.mkdir(mode=0o700)
            os.utime(candidate, (0.0, 0.0))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=[
                    "_close_lease_lock_all",
                    "_reclaim_empty_unleased_candidate",
                ],
            )
            real_close_all = lease_module._close_lease_lock_all

            def fail_empty_close(
                lock: LeaseLock,
                label: str,
            ) -> tuple[str, ...]:
                errors = real_close_all(lock, label)
                if label == "empty janitor coordinator":
                    return (*errors, "injected empty early close failure")
                return errors

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._entry_identity",
                    return_value=(-1, -1),
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._close_lease_lock_all",
                    side_effect=fail_empty_close,
                ),
            ):
                record = lease_module._reclaim_empty_unleased_candidate(
                    scratch.managed_root,
                    scratch._managed_root_fd,
                    candidate.name,
                    current_time=24 * 60 * 60 + 1.0,
                    deadline=lease_module.time.monotonic() + 5.0,
                )

            self.assertIsInstance(record, JanitorDiagnostic)
            assert isinstance(record, JanitorDiagnostic)
            self.assertIn("empty early close failure", "; ".join(record.details))
            self.assertTrue(candidate.is_dir())
            candidate.rmdir()
            scratch.mark_cleanup_ready()
            scratch.cleanup()

    def test_janitor_stops_before_cleanup_when_claim_identity_changes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_entry_identity", "_close_lease_lock_all"],
            )
            real_identity = lease_module._entry_identity
            real_close_all = lease_module._close_lease_lock_all

            def replace_after_claim(parent_fd: int, name: str) -> tuple[int, int]:
                identity = real_identity(parent_fd, name)
                if name.startswith(".deleting-"):
                    return identity[0], identity[1] + 1
                return identity

            def fail_claim_close(
                lock: LeaseLock, label: str
            ) -> tuple[str, ...]:
                errors = real_close_all(lock, label)
                if label == "janitor claim coordinator":
                    return (*errors, "injected janitor claim close failure")
                return errors

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._entry_identity",
                    side_effect=replace_after_claim,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._close_lease_lock_all",
                    side_effect=fail_claim_close,
                ),
                mock.patch.object(ManagedScratch, "cleanup") as cleanup,
            ):
                records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(len(records), 1)
            self.assertIsInstance(records[0], JanitorDiagnostic)
            self.assertIn("identity changed", records[0].details[0])
            self.assertIn("claim close failure", records[0].details[0])
            deleting = scratch.managed_root / f".deleting-{scratch.run_id}"
            self.assertTrue(deleting.is_dir())
            cleanup.assert_not_called()

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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            active = scratch.path
            moved = active.with_name(f"{active.name}.original")
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            real_open = lease_module._open_directory_at
            swapped = False
            replacement_sentinel: Path | None = None

            def swap_before_candidate_open(
                parent_fd: int, name: str, **kwargs: object
            ) -> int:
                nonlocal swapped, replacement_sentinel
                if not swapped and name == active.name:
                    active.rename(moved)
                    active.mkdir(mode=0o700)
                    replacement_sentinel = active / "sentinel"
                    replacement_sentinel.write_text("keep", encoding="utf-8")
                    swapped = True
                return real_open(parent_fd, name, **kwargs)

            with mock.patch(
                "tools.focused_mutation_support.lease._open_directory_at",
                side_effect=swap_before_candidate_open,
            ):
                records = reclaim_abandoned(scratch.managed_root)

            self.assertTrue(swapped)
            self.assertEqual(len(records), 1)
            self.assertIsInstance(records[0], JanitorDiagnostic)
            self.assertIn("unknown lease", records[0].details[0])
            self.assertIsNotNone(replacement_sentinel)
            assert replacement_sentinel is not None
            self.assertEqual(
                replacement_sentinel.read_text(encoding="utf-8"), "keep"
            )
            self.assertTrue(moved.is_dir())

    def test_janitor_reclaims_empty_deleting_crash_tail_without_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            seed = ManagedScratch.create(Path(directory))
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            deleting = managed / (
                ".deleting-00000000-0000-4000-8000-000000000021"
            )
            deleting.mkdir(mode=0o700)

            records = reclaim_abandoned(managed)

            self.assertEqual(
                [record.status for record in _cleanup_records_only(records)],
                [ScratchCleanupStatus.CLEAN],
            )
            self.assertFalse(deleting.exists())

    def test_janitor_preserves_fresh_run_with_only_an_unlocked_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            (active / ".hoimin-heartbeat.json").unlink()
            scratch.__del__()

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(records, [])
            self.assertTrue(active.is_dir())

    def test_janitor_preserves_fresh_staging_with_only_an_unlocked_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            staging = active.with_name(active.name.replace("run-", ".staging-"))
            (active / ".hoimin-heartbeat.json").unlink()
            scratch.__del__()
            active.rename(staging)

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(records, [])
            self.assertTrue(staging.is_dir())

    def test_janitor_reclaims_old_staging_with_only_an_unlocked_lease(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            staging = active.with_name(active.name.replace("run-", ".staging-"))
            (active / ".hoimin-heartbeat.json").unlink()
            scratch.__del__()
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
            (active / ".hoimin-heartbeat.json").unlink()
            scratch.__del__()
            active.rename(deleting)

            records = reclaim_abandoned(scratch.managed_root)

            self.assertEqual(
                [record.status for record in _cleanup_records_only(records)],
                [ScratchCleanupStatus.CLEAN],
            )
            self.assertFalse(deleting.exists())

    def test_janitor_reclaims_only_old_empty_prelease_staging(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            seed = ManagedScratch.create(Path(directory))
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            old = managed / ".staging-00000000-0000-4000-8000-000000000022"
            fresh = managed / ".staging-00000000-0000-4000-8000-000000000023"
            old.mkdir(mode=0o700)
            fresh.mkdir(mode=0o700)
            os.utime(old, (1.0, 1.0))

            records = reclaim_abandoned(managed, now=24 * 60 * 60 + 2.0)

            self.assertEqual(
                [record.status for record in _cleanup_records_only(records)],
                [ScratchCleanupStatus.CLEAN],
            )
            self.assertFalse(old.exists())
            self.assertTrue(fresh.exists())

    def test_janitor_cursor_reaches_the_two_hundred_fifty_seventh_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            seed = ManagedScratch.create(parent)
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            for number in range(1, 258):
                run_id = str(uuid.UUID(int=number))
                lease_id = str(uuid.UUID(int=10_000 + number))
                root = managed / f"run-{run_id}"
                root.mkdir(mode=0o700)
                marker = {
                    "schema_version": 1,
                    "run_id": run_id,
                    "owner_kind": "focused_python",
                    "lease_id": lease_id,
                }
                for filename in (
                    ".hoimin-lease.json",
                    ".hoimin-heartbeat.json",
                    ".hoimin-cleanup-ready.json",
                ):
                    (root / filename).write_text(
                        json.dumps(marker, sort_keys=True) + "\n", encoding="utf-8"
                    )

            deferred = ScratchCleanupRecord(
                ScratchCleanupStatus.DEFERRED,
                1,
                0,
                remaining_root="retained",
            )
            with mock.patch.object(
                ManagedScratch, "cleanup", return_value=deferred
            ) as cleanup:
                first = reclaim_abandoned(managed)
                second = reclaim_abandoned(managed)

            self.assertEqual(len(first), 256)
            self.assertEqual(len(second), 1)
            self.assertEqual(cleanup.call_count, 514)
            self.assertFalse(
                any(path.name.startswith("run-") for path in managed.iterdir())
            )

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
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            seed = ManagedScratch.create(parent)
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            for number in (31, 32):
                run_id = f"00000000-0000-4000-8000-{number:012d}"
                lease_id = f"00000000-0000-4000-8001-{number:012d}"
                root = managed / f"run-{run_id}"
                root.mkdir(mode=0o700)
                marker = {
                    "schema_version": 1,
                    "run_id": run_id,
                    "owner_kind": "focused_python",
                    "lease_id": lease_id,
                }
                for filename in (
                    ".hoimin-lease.json",
                    ".hoimin-heartbeat.json",
                    ".hoimin-cleanup-ready.json",
                ):
                    (root / filename).write_text(
                        json.dumps(marker, sort_keys=True) + "\n", encoding="utf-8"
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
                records = reclaim_abandoned(managed)

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

    def test_cleanup_removes_only_the_leased_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            sentinel = parent / "sentinel"
            sentinel.write_text("keep", encoding="utf-8")
            scratch = ManagedScratch.create(parent)
            child = scratch.create_child("candidate-0001")
            (child / "payload").write_bytes(b"payload")
            leased = scratch.path
            scratch.mark_cleanup_ready()

            cleanup = scratch.cleanup()

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
            scratch.mark_cleanup_ready()

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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            active = scratch.path
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_entry_identity"],
            )
            real_identity = lease_module._entry_identity
            identity_checked = False

            def cross_deadline(parent_fd: int, name: str) -> tuple[int, int]:
                nonlocal identity_checked
                identity = real_identity(parent_fd, name)
                if name == active.name:
                    identity_checked = True
                return identity

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._entry_identity",
                    side_effect=cross_deadline,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 61.0 if identity_checked else 0.0,
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(
                cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
            )
            self.assertEqual(cleanup.remaining_root, str(active))
            self.assertTrue(active.is_dir())
            self.assertFalse(
                active.with_name(f".deleting-{scratch.run_id}").exists()
            )
            scratch.close_capabilities()

    def test_cleanup_serializes_lease_unlink_through_root_absence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator"],
            )
            real_open_coordinator = lease_module._open_coordinator
            real_unlink = lease_module.os.unlink
            janitor_acquired = threading.Event()
            janitor_records: list[ScratchCleanupRecord | JanitorDiagnostic] = []
            janitor: threading.Thread | None = None
            janitor_acquired_during_unlink = False

            def observe_coordinator(*args: object, **kwargs: object) -> LeaseLock:
                coordinator = real_open_coordinator(*args, **kwargs)
                if threading.current_thread().name == "cleanup-tail-janitor":
                    janitor_acquired.set()
                return coordinator

            def race_lease_unlink(
                name: str,
                *args: object,
                **kwargs: object,
            ) -> None:
                nonlocal janitor, janitor_acquired_during_unlink
                if name == ".hoimin-lease.json" and janitor is None:
                    janitor = threading.Thread(
                        target=lambda: janitor_records.extend(
                            reclaim_abandoned(scratch.managed_root)
                        ),
                        name="cleanup-tail-janitor",
                    )
                    janitor.start()
                    janitor_acquired_during_unlink = janitor_acquired.wait(0.2)
                real_unlink(name, *args, **kwargs)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._open_coordinator",
                    side_effect=observe_coordinator,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.unlink",
                    side_effect=race_lease_unlink,
                ),
            ):
                cleanup = scratch.cleanup()

            assert janitor is not None
            janitor.join(2.0)
            self.assertFalse(janitor.is_alive())
            self.assertFalse(janitor_acquired_during_unlink)
            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertIsNone(cleanup.remaining_root)
            self.assertFalse(scratch.path.exists())

    def test_cleanup_absence_stays_clean_when_capability_close_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            leased = scratch.path
            scratch.mark_cleanup_ready()

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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            current = child
            for index in range(129):
                current = current / f"d{index:03d}"
                current.mkdir()
            (current / "payload").write_bytes(b"payload")
            observed: list[int] = []

            with mock.patch(
                "tools.focused_mutation_support.lease._cleanup_handle_observer",
                side_effect=observed.append,
            ):
                cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertLessEqual(max(observed), 3)
            scratch.mark_cleanup_ready()
            self.assertEqual(
                scratch.cleanup().status,
                ScratchCleanupStatus.CLEAN,
            )

    def test_cleanup_refuses_replacement_root_with_same_managed_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            original = scratch.path
            moved = original.with_name(f"{original.name}.moved")
            original.rename(moved)
            original.mkdir()
            sentinel = original / "sentinel"
            sentinel.write_text("keep", encoding="utf-8")

            cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(moved))
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")
            self.assertTrue(moved.is_dir())

    def test_cleanup_recovers_owned_path_from_pinned_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            original = scratch.path
            moved = original.with_name(f"{original.name}.moved")
            original.rename(moved)
            original.mkdir()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["os"],
            )
            real_stat = lease_module.os.stat

            def fail_moved_lookup(
                path: object,
                *args: object,
                **kwargs: object,
            ) -> os.stat_result:
                if path == moved.name:
                    raise OSError("injected identity lookup failure")
                return real_stat(path, *args, **kwargs)

            with mock.patch(
                "tools.focused_mutation_support.lease.os.stat",
                side_effect=fail_moved_lookup,
            ):
                cleanup = scratch.cleanup()

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
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_entry_identity"],
            )
            real_identity = lease_module._entry_identity

            def fail_owned_entry(parent_fd: int, name: str) -> tuple[int, int]:
                if name == scratch.path.name:
                    raise OSError("injected owned-entry lookup failure")
                return real_identity(parent_fd, name)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._entry_identity",
                    side_effect=fail_owned_entry,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._directory_path_from_capability",
                    side_effect=OSError("injected path recovery failure"),
                ),
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertIsNone(cleanup.remaining_root)
            self.assertIn("lookup failed", "; ".join(cleanup.details))
            scratch.close_capabilities()

    def test_unlinked_pinned_root_is_reported_clean_not_as_deleted_path(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            for marker in (
                ".hoimin-heartbeat.json",
                ".hoimin-lease.json",
            ):
                os.unlink(marker, dir_fd=scratch._root_fd)
            os.rmdir(scratch.path.name, dir_fd=scratch._managed_root_fd)
            deleted_alias = scratch.path.with_name(
                f"{scratch.path.name} (deleted)"
            )

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._directory_path_from_capability",
                    return_value=deleted_alias,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.fstat",
                    return_value=mock.Mock(st_nlink=0),
                ),
            ):
                cleanup = scratch._failed("injected post-unlink failure")

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertIsNone(cleanup.remaining_root)
            self.assertIn("absent", "; ".join(cleanup.details))

    def test_cleanup_tail_lookup_error_does_not_report_unvalidated_path(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_entry_identity"],
            )
            real_identity = lease_module._entry_identity

            def fail_deleting_entry(
                parent_fd: int, name: str
            ) -> tuple[int, int]:
                if name.startswith(".deleting-"):
                    raise OSError("injected deleting-entry lookup failure")
                return real_identity(parent_fd, name)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._entry_identity",
                    side_effect=fail_deleting_entry,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._directory_path_from_capability",
                    side_effect=OSError("injected path recovery failure"),
                ),
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertIsNone(cleanup.remaining_root)
            self.assertIn("lookup failed", "; ".join(cleanup.details))
            scratch.close_capabilities()

    def test_post_rmdir_stat_error_never_reports_deleted_path_as_failed(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["os"],
            )
            real_rmdir = lease_module.os.rmdir
            real_stat = lease_module.os.stat
            removed = False

            def mark_removed(
                path: object, *args: object, **kwargs: object
            ) -> None:
                nonlocal removed
                real_rmdir(path, *args, **kwargs)
                if str(path).startswith(".deleting-"):
                    removed = True

            def fail_post_remove_stat(
                path: object, *args: object, **kwargs: object
            ) -> os.stat_result:
                if removed and str(path).startswith(".deleting-"):
                    raise OSError("injected post-rmdir lookup failure")
                return real_stat(path, *args, **kwargs)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.os.rmdir",
                    side_effect=mark_removed,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.stat",
                    side_effect=fail_post_remove_stat,
                ),
            ):
                cleanup = scratch.cleanup()

            self.assertNotEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertIsNone(cleanup.remaining_root)
            self.assertFalse(scratch.path.exists())
            scratch.close_capabilities()

    def test_cleanup_stops_after_chmod_crosses_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("payload")
            (child / "file").write_bytes(b"payload")
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["os"],
            )
            real_fchmod = lease_module.os.fchmod
            real_scandir = lease_module.os.scandir
            crossed = False

            def cross_during_chmod(fd: int, mode: int) -> None:
                nonlocal crossed
                real_fchmod(fd, mode)
                crossed = True

            def forbid_post_deadline_scan(*args: object, **kwargs: object) -> object:
                if crossed:
                    raise AssertionError("scandir started after cleanup deadline")
                return real_scandir(*args, **kwargs)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.os.fchmod",
                    side_effect=cross_during_chmod,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.scandir",
                    side_effect=forbid_post_deadline_scan,
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(
                cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
            )
            self.assertTrue(scratch.path.is_dir())
            scratch.close_capabilities()

    def test_cleanup_stops_after_scandir_construction_crosses_deadline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("payload")
            (child / "file").write_bytes(b"payload")
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["os"],
            )
            real_scandir = lease_module.os.scandir
            crossed = False

            class DeadlineCrossingIterator:
                def __init__(self, inner: object) -> None:
                    self.inner = inner

                def __next__(self) -> os.DirEntry[str]:
                    raise AssertionError("readdir started after cleanup deadline")

                def close(self) -> None:
                    self.inner.close()  # type: ignore[attr-defined]

            def cross_during_scandir(
                *args: object, **kwargs: object
            ) -> DeadlineCrossingIterator:
                nonlocal crossed
                inner = real_scandir(*args, **kwargs)
                crossed = True
                return DeadlineCrossingIterator(inner)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.os.scandir",
                    side_effect=cross_during_scandir,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(
                cleanup.status, ScratchCleanupStatus.DEFERRED, cleanup
            )
            self.assertTrue(scratch.path.is_dir())
            scratch.close_capabilities()

    def test_remove_payload_preexpired_budget_does_not_start_filesystem_io(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_remove_payload"],
            )

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    return_value=2.0,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.fstat",
                    side_effect=AssertionError(
                        "fstat started after cleanup deadline"
                    ),
                ),
            ):
                examined, removed, complete = lease_module._remove_payload(
                    scratch._root_fd,
                    started=0.0,
                    absolute_deadline=1.0,
                    examined=0,
                    removed=0,
                )

            self.assertEqual((examined, removed, complete), (0, 0, False))
            scratch.close_capabilities()

    def test_remove_payload_stops_after_root_stat_crosses_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_remove_payload"],
            )
            real_fstat = lease_module.os.fstat
            crossed = False

            def cross_during_fstat(fd: int) -> os.stat_result:
                nonlocal crossed
                result = real_fstat(fd)
                crossed = True
                return result

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 2.0 if crossed else 0.0,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.fstat",
                    side_effect=cross_during_fstat,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._filesystem_identity",
                    side_effect=AssertionError(
                        "filesystem identity started after cleanup deadline"
                    ),
                ),
            ):
                examined, removed, complete = lease_module._remove_payload(
                    scratch._root_fd,
                    started=0.0,
                    absolute_deadline=1.0,
                    examined=0,
                    removed=0,
                )

            self.assertEqual((examined, removed, complete), (0, 0, False))
            scratch.close_capabilities()

    def test_remove_payload_propagates_filesystem_timeout_before_deadline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            (child / "nested").mkdir()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            real_open = lease_module._open_directory_at
            open_calls = 0
            crossed = False

            def timeout_while_opening_child(
                parent_fd: int,
                name: str,
                **kwargs: object,
            ) -> int:
                nonlocal open_calls, crossed
                open_calls += 1
                if open_calls == 2:
                    crossed = True
                    raise TimeoutError("filesystem ETIMEDOUT")
                return real_open(parent_fd, name, **kwargs)

            with (
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    side_effect=timeout_while_opening_child,
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                with self.assertRaisesRegex(TimeoutError, "ETIMEDOUT"):
                    lease_module._remove_payload(
                        scratch._root_fd,
                        started=0.0,
                        absolute_deadline=60.0,
                        examined=0,
                        removed=0,
                    )

            scratch.close_capabilities()

    def test_remove_payload_reopen_stops_after_identity_crosses_deadline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("empty-child")
            (child / "nested").mkdir()
            child_identity = (child.stat().st_dev, child.stat().st_ino)
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_remove_payload"],
            )
            real_identity = lease_module._directory_identity
            real_filesystem = lease_module._filesystem_identity
            child_identity_checks = 0
            crossed = False

            def cross_on_reopened_child(fd: int) -> tuple[int, int]:
                nonlocal child_identity_checks, crossed
                identity = real_identity(fd)
                if identity == child_identity:
                    child_identity_checks += 1
                    if child_identity_checks == 2:
                        crossed = True
                return identity

            def forbid_post_deadline_filesystem(fd: int) -> tuple[int, int, int]:
                if crossed:
                    raise AssertionError(
                        "filesystem identity started after cleanup deadline"
                    )
                return real_filesystem(fd)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._directory_identity",
                    side_effect=cross_on_reopened_child,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._filesystem_identity",
                    side_effect=forbid_post_deadline_filesystem,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                examined, removed, complete = lease_module._remove_payload(
                    scratch._root_fd,
                    started=0.0,
                    absolute_deadline=60.0,
                    examined=0,
                    removed=0,
                )

            self.assertFalse(complete)
            self.assertGreaterEqual(examined, 1)
            self.assertEqual(removed, 0)
            scratch.close_capabilities()

    def test_owned_root_search_stops_before_readdir_after_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            original = scratch.path
            moved = original.with_name(f"{original.name}.moved")
            original.rename(moved)
            original.mkdir()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["os"],
            )
            real_scandir = lease_module.os.scandir
            crossed = False

            class DeadlineCrossingIterator:
                def __init__(self, inner: object) -> None:
                    self.inner = inner

                def __next__(self) -> os.DirEntry[str]:
                    raise AssertionError(
                        "owned-root readdir started after cleanup deadline"
                    )

                def close(self) -> None:
                    self.inner.close()  # type: ignore[attr-defined]

            def cross_during_scandir(
                *args: object, **kwargs: object
            ) -> DeadlineCrossingIterator:
                nonlocal crossed
                inner = real_scandir(*args, **kwargs)
                crossed = True
                return DeadlineCrossingIterator(inner)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._directory_path_from_capability",
                    side_effect=OSError("injected path recovery failure"),
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.scandir",
                    side_effect=cross_during_scandir,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertIsNone(cleanup.remaining_root)
            self.assertIn("deadline", "; ".join(cleanup.details))
            scratch.close_capabilities()

    def test_owned_root_search_stops_after_namespace_open_crosses_deadline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            original = scratch.path
            moved = original.with_name(f"{original.name}.moved")
            original.rename(moved)
            original.mkdir()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            real_open = lease_module._open_directory_at
            crossed = False

            def cross_during_namespace_open(
                parent_fd: int,
                name: str,
                *,
                deadline: float | None = None,
                monotonic: Callable[[], float] | None = None,
            ) -> int:
                nonlocal crossed
                fd = real_open(
                    parent_fd,
                    name,
                    deadline=deadline,
                    monotonic=monotonic,
                )
                if name == ".":
                    crossed = True
                return fd

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._directory_path_from_capability",
                    side_effect=OSError("injected path recovery failure"),
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._open_directory_at",
                    side_effect=cross_during_namespace_open,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.scandir",
                    side_effect=AssertionError(
                        "scandir started after cleanup deadline"
                    ),
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertIsNone(cleanup.remaining_root)
            scratch.close_capabilities()

    def test_empty_janitor_verifies_rmdir_before_reporting_clean(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            seed = ManagedScratch.create(Path(directory))
            managed = seed.managed_root
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            run_id = "00000000-0000-4000-8000-000000000122"
            candidate = managed / f".staging-{run_id}"
            candidate.mkdir(mode=0o700)
            os.utime(candidate, (0.0, 0.0))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_reclaim_empty_unleased_candidate"],
            )
            managed_fd = os.open(managed, os.O_RDONLY)
            try:
                with mock.patch.object(
                    lease_module.os, "rmdir", return_value=None
                ):
                    result = lease_module._reclaim_empty_unleased_candidate(
                        managed,
                        managed_fd,
                        candidate.name,
                        current_time=24 * 60 * 60 + 1.0,
                        deadline=time.monotonic() + 30.0,
                    )
            finally:
                os.close(managed_fd)

            self.assertIsInstance(result, ScratchCleanupRecord)
            assert isinstance(result, ScratchCleanupRecord)
            self.assertEqual(result.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(result.remaining_root, str(candidate))
            self.assertTrue(candidate.is_dir())

    def test_empty_janitor_does_not_recover_after_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            seed = ManagedScratch.create(Path(directory))
            managed = seed.managed_root
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            run_id = "00000000-0000-4000-8000-000000000123"
            candidate = managed / f".staging-{run_id}"
            candidate.mkdir(mode=0o700)
            os.utime(candidate, (0.0, 0.0))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_reclaim_empty_unleased_candidate"],
            )
            managed_fd = os.open(managed, os.O_RDONLY)
            crossed = False

            def vanish_at_scan(*_args: object, **_kwargs: object) -> bool:
                nonlocal crossed
                crossed = True
                raise FileNotFoundError(candidate.name)

            def forbid_recovery_identity(
                _parent_fd: int, _name: str
            ) -> tuple[int, int]:
                if crossed:
                    raise AssertionError(
                        "empty janitor recovery started after deadline"
                    )
                raise AssertionError("unexpected identity call")

            try:
                with (
                    mock.patch.object(
                        lease_module,
                        "_directory_is_empty_at",
                        side_effect=vanish_at_scan,
                    ),
                    mock.patch.object(
                        lease_module,
                        "_entry_identity",
                        side_effect=forbid_recovery_identity,
                    ),
                    mock.patch.object(
                        lease_module.time,
                        "monotonic",
                        side_effect=lambda: 31.0 if crossed else 0.0,
                    ),
                ):
                    result = lease_module._reclaim_empty_unleased_candidate(
                        managed,
                        managed_fd,
                        candidate.name,
                        current_time=24 * 60 * 60 + 1.0,
                        deadline=30.0,
                    )
            finally:
                os.close(managed_fd)

            self.assertIsInstance(result, JanitorDiagnostic)
            self.assertTrue(candidate.is_dir())

    def test_owned_root_search_stops_after_readdir_crosses_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            original = scratch.path
            moved = original.with_name(f"{original.name}.moved")
            original.rename(moved)
            original.mkdir()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["os"],
            )
            real_scandir = lease_module.os.scandir
            real_stat = lease_module.os.stat
            crossed = False

            class CrossingIterator:
                def __init__(self, inner: object) -> None:
                    self.inner = inner

                def __next__(self) -> os.DirEntry[str]:
                    nonlocal crossed
                    entry = next(self.inner)  # type: ignore[call-overload]
                    crossed = True
                    return entry

                def close(self) -> None:
                    self.inner.close()  # type: ignore[attr-defined]

            def crossing_scandir(
                *args: object, **kwargs: object
            ) -> CrossingIterator:
                return CrossingIterator(real_scandir(*args, **kwargs))

            def forbid_post_deadline_stat(
                *args: object, **kwargs: object
            ) -> os.stat_result:
                if crossed:
                    raise AssertionError("stat started after cleanup deadline")
                return real_stat(*args, **kwargs)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._directory_path_from_capability",
                    side_effect=OSError("injected path recovery failure"),
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.scandir",
                    side_effect=crossing_scandir,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.stat",
                    side_effect=forbid_post_deadline_stat,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertIsNone(cleanup.remaining_root)
            scratch.close_capabilities()

    def test_cleanup_reports_deleting_path_after_post_rename_verification_failure(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            original = scratch.path
            deleting = original.with_name(f".deleting-{scratch.run_id}")
            real_identity = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_entry_identity"],
            )._entry_identity
            checked_active = False

            def mismatch_after_rename(parent_fd: int, name: str) -> tuple[int, int]:
                nonlocal checked_active
                identity = real_identity(parent_fd, name)
                if name == original.name:
                    checked_active = True
                if name == deleting.name and checked_active:
                    return identity[0], identity[1] + 1
                return identity

            with mock.patch(
                "tools.focused_mutation_support.lease._entry_identity",
                side_effect=mismatch_after_rename,
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(deleting))
            self.assertEqual(scratch.path, deleting)
            self.assertTrue(deleting.is_dir())
            scratch.close_capabilities()

    def test_cleanup_stops_after_claim_rename_crosses_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            deleting = scratch.path.with_name(f".deleting-{scratch.run_id}")
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["os", "_entry_identity"],
            )
            real_rename = lease_module.os.rename
            real_identity = lease_module._entry_identity
            crossed = False

            def crossing_rename(*args: object, **kwargs: object) -> None:
                nonlocal crossed
                real_rename(*args, **kwargs)  # type: ignore[arg-type]
                crossed = True

            def forbid_post_deadline_identity(
                parent_fd: int, name: str
            ) -> tuple[int, int]:
                if crossed:
                    raise AssertionError(
                        "identity started after cleanup deadline"
                    )
                return real_identity(parent_fd, name)

            with (
                mock.patch.object(
                    lease_module.os, "rename", side_effect=crossing_rename
                ),
                mock.patch.object(
                    lease_module,
                    "_entry_identity",
                    side_effect=forbid_post_deadline_identity,
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertEqual(cleanup.remaining_root, str(deleting))
            scratch.close_capabilities()

    def test_cleanup_stops_after_root_rmdir_crosses_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease", fromlist=["os"]
            )
            real_rmdir = lease_module.os.rmdir
            real_stat = lease_module.os.stat
            crossed = False

            def crossing_rmdir(
                path: object, *args: object, **kwargs: object
            ) -> None:
                nonlocal crossed
                real_rmdir(path, *args, **kwargs)  # type: ignore[arg-type]
                if str(path).startswith(".deleting-"):
                    crossed = True

            def forbid_post_deadline_stat(
                *args: object, **kwargs: object
            ) -> os.stat_result:
                if crossed:
                    raise AssertionError("stat started after cleanup deadline")
                return real_stat(*args, **kwargs)  # type: ignore[arg-type]

            with (
                mock.patch.object(
                    lease_module.os, "rmdir", side_effect=crossing_rmdir
                ),
                mock.patch.object(
                    lease_module.os,
                    "stat",
                    side_effect=forbid_post_deadline_stat,
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.cleanup(time_budget=60.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertIsNone(cleanup.remaining_root)
            scratch.close_capabilities()

    def test_marker_deadline_keeps_close_failure_secondary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_read_valid_marker_at"],
            )
            real_close = lease_module.os.close
            checks = 0

            def cross_after_open(*_args: object, **_kwargs: object) -> None:
                nonlocal checks
                checks += 1
                if checks == 2:
                    raise TimeoutError("injected marker deadline")

            def close_then_fail(descriptor: int) -> None:
                real_close(descriptor)
                raise OSError("injected marker close failure")

            with (
                mock.patch.object(
                    lease_module,
                    "_check_deadline",
                    side_effect=cross_after_open,
                ),
                mock.patch.object(
                    lease_module.os, "close", side_effect=close_then_fail
                ),
                self.assertRaisesRegex(
                    TimeoutError, "injected marker deadline"
                ) as raised,
            ):
                lease_module._read_valid_marker_at(
                    scratch._root_fd,
                    ".hoimin-lease.json",
                    scratch.run_id,
                    scratch.lease_id,
                    deadline=time.monotonic() + 30.0,
                )

            self.assertIn(
                "marker close failure",
                "; ".join(getattr(raised.exception, "__notes__", ())),
            )
            scratch.close_capabilities()

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
                fromlist=["_close_lease_lock_all"],
            )
            real_close_all = lease_module._close_lease_lock_all

            def fail_claim_close(
                lock: LeaseLock,
                label: str,
            ) -> tuple[str, ...]:
                errors = real_close_all(lock, label)
                if label == "cleanup claim coordinator":
                    return (*errors, "injected claim coordinator close failure")
                return errors

            with mock.patch(
                "tools.focused_mutation_support.lease._close_lease_lock_all",
                side_effect=fail_claim_close,
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            self.assertTrue(scratch.path.is_dir())
            self.assertIn("claim coordinator close failure", cleanup.details[0])
            scratch.close_capabilities()

    def test_cleanup_tail_preserves_primary_and_coordinator_close_secondary(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_entry_identity", "_close_lease_lock_all"],
            )
            real_identity = lease_module._entry_identity
            real_close_all = lease_module._close_lease_lock_all
            deleting_checks = 0

            def fail_tail_identity(parent_fd: int, name: str) -> tuple[int, int]:
                nonlocal deleting_checks
                identity = real_identity(parent_fd, name)
                if name.startswith(".deleting-"):
                    deleting_checks += 1
                    if deleting_checks == 2:
                        return identity[0], identity[1] + 1
                return identity

            def fail_tail_close(
                lock: LeaseLock,
                label: str,
            ) -> tuple[str, ...]:
                errors = real_close_all(lock, label)
                if label == "cleanup coordinator":
                    return (*errors, "injected tail coordinator close failure")
                return errors

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._entry_identity",
                    side_effect=fail_tail_identity,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._close_lease_lock_all",
                    side_effect=fail_tail_close,
                ),
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertIn("identity changed", "; ".join(cleanup.details))
            self.assertIn(
                "tail coordinator close failure", "; ".join(cleanup.details)
            )
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            scratch.close_capabilities()

    def test_cleanup_does_not_claim_root_after_total_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            coordinator = LeaseLock(os.open(os.devnull, os.O_RDONLY))
            first_tick = iter([0.0])

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.time.monotonic",
                    side_effect=lambda: next(first_tick, 2.0),
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease._open_coordinator",
                    return_value=coordinator,
                ),
                mock.patch(
                    "tools.focused_mutation_support.lease.os.rename"
                ) as rename,
            ):
                cleanup = scratch.cleanup(time_budget=1.0)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            rename.assert_not_called()
            self.assertEqual(coordinator.fd, -1)
            self.assertTrue(scratch.path.is_dir())

    def test_remove_child_refuses_replacement_with_same_child_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            moved = child.with_name("candidate-original")
            child.rename(moved)
            child.mkdir()
            sentinel = child / "sentinel"
            sentinel.write_text("keep", encoding="utf-8")

            cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")
            self.assertTrue(moved.is_dir())

    def test_remove_child_stops_after_open_vanishes_at_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at", "_entry_identity"],
            )
            real_open = lease_module._open_directory_at
            real_identity = lease_module._entry_identity
            crossed = False

            def vanish_during_open(
                parent_fd: int,
                name: str,
                **kwargs: object,
            ) -> int:
                nonlocal crossed
                if name == child.name:
                    crossed = True
                    raise FileNotFoundError(name)
                return real_open(parent_fd, name, **kwargs)

            def forbid_recovery_identity(
                parent_fd: int, name: str
            ) -> tuple[int, int]:
                if crossed:
                    raise AssertionError(
                        "child identity recovery started after deadline"
                    )
                return real_identity(parent_fd, name)

            with (
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    side_effect=vanish_during_open,
                ),
                mock.patch.object(
                    lease_module,
                    "_entry_identity",
                    side_effect=forbid_recovery_identity,
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            scratch.close_capabilities()

    def test_remove_child_classifies_open_deadline_as_deferred(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")

            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            clock_calls = 0

            def cross_deadline_after_open() -> float:
                nonlocal clock_calls
                clock_calls += 1
                return 61.0 if clock_calls >= 3 else 0.0

            with (
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    side_effect=lease_module._DeadlineExceeded(
                        "injected directory post-open deadline"
                    ),
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=cross_deadline_after_open,
                ),
            ):
                cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            self.assertIn("post-open deadline", cleanup.details[0])
            scratch.close_capabilities()

    def test_remove_child_classifies_filesystem_timeout_before_deadline_as_failed(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )

            with (
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    side_effect=TimeoutError("filesystem ETIMEDOUT"),
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    return_value=0.0,
                ),
            ):
                cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            self.assertIn("ETIMEDOUT", cleanup.details[0])
            scratch.close_capabilities()

    def test_remove_child_keeps_filesystem_timeout_failed_after_deadline_crossing(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            clock_calls = 0

            def cross_deadline_during_filesystem_timeout() -> float:
                nonlocal clock_calls
                clock_calls += 1
                return 61.0 if clock_calls >= 3 else 0.0

            with (
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    side_effect=TimeoutError("filesystem ETIMEDOUT"),
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=cross_deadline_during_filesystem_timeout,
                ),
            ):
                cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.FAILED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            self.assertIn("ETIMEDOUT", cleanup.details[0])
            scratch.close_capabilities()

    def test_remove_child_stops_when_recovery_identity_crosses_deadline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at", "_entry_identity"],
            )
            real_identity = lease_module._entry_identity
            identity_calls = 0
            crossed = False

            def vanish_during_open(
                _parent_fd: int,
                name: str,
                **_kwargs: object,
            ) -> int:
                raise FileNotFoundError(name)

            def crossing_recovery_identity(
                parent_fd: int, name: str
            ) -> tuple[int, int]:
                nonlocal identity_calls, crossed
                identity_calls += 1
                identity = real_identity(parent_fd, name)
                if identity_calls == 2:
                    crossed = True
                return identity

            def forbid_root_recovery(*_args: object, **_kwargs: object) -> str:
                if crossed:
                    raise AssertionError(
                        "root recovery started after child identity deadline"
                    )
                raise AssertionError("unexpected root recovery")

            with (
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    side_effect=vanish_during_open,
                ),
                mock.patch.object(
                    lease_module,
                    "_entry_identity",
                    side_effect=crossing_recovery_identity,
                ),
                mock.patch.object(
                    lease_module,
                    "_directory_path_from_capability",
                    side_effect=forbid_root_recovery,
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            scratch.close_capabilities()

    def test_remove_child_stops_when_post_rmdir_stat_crosses_deadline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            child = scratch.create_child("candidate-0001")
            lease_module = __import__(
                "tools.focused_mutation_support.lease", fromlist=["os"]
            )
            real_stat = lease_module.os.stat
            rmdir_called = False
            crossed = False

            def no_op_rmdir(*_args: object, **_kwargs: object) -> None:
                nonlocal rmdir_called
                rmdir_called = True

            def crossing_stat(
                *args: object, **kwargs: object
            ) -> os.stat_result:
                nonlocal crossed
                result = real_stat(*args, **kwargs)  # type: ignore[arg-type]
                if rmdir_called:
                    crossed = True
                return result

            def forbid_root_recovery(*_args: object, **_kwargs: object) -> str:
                if crossed:
                    raise AssertionError(
                        "root recovery started after post-rmdir deadline"
                    )
                raise AssertionError("unexpected root recovery")

            with (
                mock.patch.object(
                    lease_module.os, "rmdir", side_effect=no_op_rmdir
                ),
                mock.patch.object(
                    lease_module.os, "stat", side_effect=crossing_stat
                ),
                mock.patch.object(
                    lease_module,
                    "_directory_path_from_capability",
                    side_effect=forbid_root_recovery,
                ),
                mock.patch.object(
                    lease_module.time,
                    "monotonic",
                    side_effect=lambda: 61.0 if crossed else 0.0,
                ),
            ):
                cleanup = scratch.remove_child(child)

            self.assertEqual(cleanup.status, ScratchCleanupStatus.DEFERRED)
            self.assertEqual(cleanup.remaining_root, str(scratch.path))
            scratch.close_capabilities()

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
            scratch.mark_cleanup_ready()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator", "_open_directory_at"],
            )
            real_coordinator = lease_module._open_coordinator
            real_open = lease_module._open_directory_at
            coordinator_deadlines: list[float | None] = []
            directory_deadlines: list[float | None] = []

            def observe_coordinator(
                *args: object, **kwargs: object
            ) -> LeaseLock:
                coordinator_deadlines.append(kwargs.get("deadline"))  # type: ignore[arg-type]
                return real_coordinator(*args, **kwargs)

            def observe_open(
                *args: object, **kwargs: object
            ) -> int:
                directory_deadlines.append(kwargs.get("deadline"))  # type: ignore[arg-type]
                return real_open(*args, **kwargs)

            with (
                mock.patch.object(
                    lease_module,
                    "_open_coordinator",
                    side_effect=observe_coordinator,
                ),
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    side_effect=observe_open,
                ),
            ):
                cleanup = scratch.cleanup()

            self.assertEqual(cleanup.status, ScratchCleanupStatus.CLEAN)
            self.assertGreaterEqual(len(coordinator_deadlines), 2)
            self.assertTrue(all(item is not None for item in coordinator_deadlines))
            self.assertTrue(all(item is not None for item in directory_deadlines))

    def test_janitor_propagates_one_absolute_deadline_to_all_helpers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            managed = scratch.managed_root
            scratch.mark_cleanup_ready()
            scratch.__del__()
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator", "_open_directory_at"],
            )
            real_coordinator = lease_module._open_coordinator
            real_open = lease_module._open_directory_at
            coordinator_deadlines: list[float | None] = []
            directory_deadlines: list[float | None] = []

            def observe_coordinator(
                *args: object, **kwargs: object
            ) -> LeaseLock:
                coordinator_deadlines.append(kwargs.get("deadline"))  # type: ignore[arg-type]
                return real_coordinator(*args, **kwargs)

            def observe_open(
                *args: object, **kwargs: object
            ) -> int:
                directory_deadlines.append(kwargs.get("deadline"))  # type: ignore[arg-type]
                return real_open(*args, **kwargs)

            with (
                mock.patch.object(
                    lease_module,
                    "_open_coordinator",
                    side_effect=observe_coordinator,
                ),
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
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
            self.assertTrue(directory_deadlines)
            self.assertTrue(all(item is not None for item in directory_deadlines))


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
