from pathlib import Path
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
import zlib

from tools.focused_mutation import _parser, options_from_arguments
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
    DiskStopReason,
    apply_disk_lifecycle_event,
    evaluate_disk_policy,
    parse_byte_size,
)
from tools.focused_mutation_support.runner import CommandDiskStopped, CommandRunner
from tools.focused_mutation_support.model import RunRecord
from tools.focused_mutation_support.lease import (
    JanitorDiagnostic,
    ManagedScratch,
    LeaseLock,
    ScratchCleanupRecord,
    ScratchCleanupStatus,
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
    OwnedOutput,
    ReportTooLarge,
    RunStore,
)


class DiskPolicyParserTests(unittest.TestCase):
    def parse(self, *arguments: str) -> DiskPolicy:
        namespace = _parser().parse_args(["--output", "/tmp/out", *arguments])
        return options_from_arguments(namespace, Path("/repo")).disk_policy

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

            options = options_from_arguments(namespace, Path("/repo"))

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
            options_from_arguments(namespace, Path("/repo"))

        namespace = _parser().parse_args(
            ["--output", "/tmp/out", "--symbol", "x" * (16 * 1024 + 1)]
        )
        with self.assertRaisesRegex(ValueError, "selector exceeds 16 KiB"):
            options_from_arguments(namespace, Path("/repo"))


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
        self.assertEqual(size.reason, DiskStopReason.WORKSPACE_SIZE_EXCEEDED)
        reserve = evaluate_disk_policy(
            self.policy(), DiskObservation(owned_bytes=100, available_bytes=20)
        )
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


class AnchoredDiskGuardTests(unittest.TestCase):
    def test_reserved_spool_bytes_trip_owned_limit_before_write(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=100,
                    min_free_bytes=10,
                    scratch_root=root,
                ),
                [MeterRoot(root, enforcement="owned:test")],
            )
            self.addCleanup(guard.close)
            guard.observations.append(
                DiskObservation(owned_bytes=90, available_bytes=1_000)
            )

            disk_module = __import__(
                "tools.focused_mutation_support.disk",
                fromlist=["_filesystem_identity"],
            )
            with (
                mock.patch.object(
                    disk_module,
                    "_filesystem_identity",
                    return_value=(1, 2, 3),
                ),
                mock.patch.object(
                    disk_module.os,
                    "fstatvfs",
                    return_value=mock.Mock(f_bavail=1_000, f_frsize=1),
                ),
            ):
                failure = guard.reserve_additional_bytes(
                    10, filesystem_fd=guard._root_capabilities[0][1]
                )

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(
                failure.reason, DiskStopReason.WORKSPACE_SIZE_EXCEEDED
            )
            self.assertEqual(failure.observation.owned_bytes, 100)

    def test_reserved_spool_bytes_trip_free_space_reserve_before_write(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1_000,
                    min_free_bytes=10,
                    scratch_root=root,
                ),
                [MeterRoot(root, enforcement="owned:test")],
            )
            self.addCleanup(guard.close)
            guard.observations.append(
                DiskObservation(owned_bytes=0, available_bytes=20)
            )

            disk_module = __import__(
                "tools.focused_mutation_support.disk",
                fromlist=["_filesystem_identity"],
            )
            with (
                mock.patch.object(
                    disk_module,
                    "_filesystem_identity",
                    return_value=(1, 2, 3),
                ),
                mock.patch.object(
                    disk_module.os,
                    "fstatvfs",
                    return_value=mock.Mock(f_bavail=20, f_frsize=1),
                ),
            ):
                failure = guard.reserve_additional_bytes(
                    10, filesystem_fd=guard._root_capabilities[0][1]
                )

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(
                failure.reason, DiskStopReason.FILESYSTEM_RESERVE_REACHED
            )
            self.assertEqual(failure.observation.available_bytes, 7)

    def test_spool_reservation_includes_block_and_metadata_overhead(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1_000,
                    min_free_bytes=10,
                    scratch_root=root,
                ),
                [MeterRoot(root, enforcement="owned:test")],
            )
            self.addCleanup(guard.close)
            guard.observations.append(
                DiskObservation(owned_bytes=0, available_bytes=16_388)
            )
            disk_module = __import__(
                "tools.focused_mutation_support.disk",
                fromlist=["_filesystem_identity"],
            )
            with (
                mock.patch.object(
                    disk_module,
                    "_filesystem_identity",
                    return_value=(1, 2, 3),
                ),
                mock.patch.object(
                    disk_module.os,
                    "fstatvfs",
                    return_value=mock.Mock(f_bavail=4, f_frsize=4_096),
                ),
            ):
                failure = guard.reserve_additional_bytes(
                    1, filesystem_fd=guard._root_capabilities[0][1]
                )

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(
                failure.reason, DiskStopReason.FILESYSTEM_RESERVE_REACHED
            )
            self.assertEqual(failure.observation.available_bytes, 0)

    def test_spool_reservation_debits_only_its_own_filesystem(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1_000,
                    min_free_bytes=10,
                    scratch_root=root,
                ),
                [MeterRoot(root, enforcement="owned:test")],
            )
            self.addCleanup(guard.close)
            guard.observations.append(
                DiskObservation(
                    owned_bytes=0,
                    available_bytes=11,
                    filesystem_available_bytes={"1:1:1": 11, "2:2:2": 1_000},
                )
            )
            disk_module = __import__(
                "tools.focused_mutation_support.disk",
                fromlist=["_filesystem_identity"],
            )
            with (
                mock.patch.object(
                    disk_module,
                    "_filesystem_identity",
                    return_value=(2, 2, 2),
                ),
                mock.patch.object(
                    disk_module.os,
                    "fstatvfs",
                    return_value=mock.Mock(f_bavail=1_000, f_frsize=1),
                ),
            ):
                failure = guard.reserve_additional_bytes(
                    2, filesystem_fd=guard._root_capabilities[0][1]
                )

            self.assertIsNone(failure)

    def test_spool_capacity_query_failure_is_typed_measurement_stop(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1_000,
                    min_free_bytes=10,
                    scratch_root=root,
                ),
                [MeterRoot(root, enforcement="owned:test")],
            )
            self.addCleanup(guard.close)
            guard.observations.append(
                DiskObservation(owned_bytes=0, available_bytes=1_000)
            )
            disk_module = __import__(
                "tools.focused_mutation_support.disk",
                fromlist=["_filesystem_identity"],
            )
            with mock.patch.object(
                disk_module.os,
                "fstatvfs",
                side_effect=OSError("injected spool capacity failure"),
            ):
                failure = guard.reserve_additional_bytes(
                    1, filesystem_fd=guard._root_capabilities[0][1]
                )

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
            self.assertIn("spool capacity failure", failure.message)

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
            self.assertIsNotNone(guard.failure.observation)
            self.assertEqual(guard.failure.observation.identity_bytes, {})

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
                "tools.focused_mutation_support.disk._measure_fd",
                side_effect=OSError("injected later scan failure"),
            ):
                sticky = guard.sample()

            self.assertIs(sticky, first)
            self.assertIsNotNone(guard.latest_failure)
            self.assertEqual(
                guard.latest_failure.reason,
                DiskStopReason.MEASUREMENT_FAILED,
            )
            self.assertIn("later scan failure", guard.latest_failure.message)

    def test_join_timeout_keeps_scratch_lease_until_monitor_exits(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            scratch = ManagedScratch.create(parent)
            scratch.mark_cleanup_ready()
            managed_root = scratch.managed_root
            leased_path = scratch.path
            entered = threading.Event()
            release = threading.Event()
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1024 * 1024,
                    min_free_bytes=1,
                    sample_interval_seconds=0.001,
                    scratch_root=parent,
                ),
                [MeterRoot(leased_path)],
                heartbeat=scratch.refresh_heartbeat,
            )
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
                del scratch
                self.assertEqual(reclaim_abandoned(managed_root), [])
                self.assertTrue(leased_path.is_dir())
                release.set()
                assert guard._thread is not None
                guard._thread.join(timeout=2.0)
                self.assertFalse(guard._thread.is_alive())

            del blocking_sample
            del original_sample
            del guard
            gc.collect()
            records = reclaim_abandoned(managed_root)
            self.assertTrue(
                any(
                    item.status is ScratchCleanupStatus.CLEAN
                    for item in records
                )
            )
            self.assertFalse(leased_path.exists())

    @unittest.skipIf(os.name == "nt", "surrogateescape names are POSIX-specific")
    def test_invalid_utf8_name_is_measured_without_stopping_monitor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
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
                "tools.focused_mutation_support.disk._measure_fd",
                side_effect=UnicodeEncodeError("utf-8", "\udcff", 0, 1, "bad"),
            ):
                failure = guard.sample()

            self.assertIsNotNone(failure)
            assert failure is not None
            self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)
            guard.close()

    def test_capability_close_failure_is_recorded_without_short_circuit(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guard = DiskGuard(
                DiskPolicy(
                    max_disk_bytes=1024 * 1024,
                    min_free_bytes=1,
                    scratch_root=root,
                ),
                [MeterRoot(root)],
            )
            descriptor = guard._root_capabilities[0][1]
            assert descriptor is not None
            real_close = os.close
            try:
                with mock.patch(
                    "tools.focused_mutation_support.disk.os.close",
                    side_effect=OSError("injected close failure"),
                ):
                    errors = guard.close()
                self.assertEqual(len(errors), 1)
                self.assertIn("injected close failure", errors[0])
            finally:
                real_close(descriptor)

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
            self.assertEqual(failure.reason, DiskStopReason.MEASUREMENT_FAILED)

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
        with tempfile.TemporaryDirectory() as directory:
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_ensure_managed_root"],
            )
            real_identity = lease_module._filesystem_identity
            crossed = False
            identity_calls = 0

            def crossing_identity(fd: int) -> tuple[int, int, int]:
                nonlocal crossed, identity_calls
                identity_calls += 1
                value = real_identity(fd)
                crossed = True
                return value

            with (
                mock.patch.object(
                    lease_module,
                    "_filesystem_identity",
                    side_effect=crossing_identity,
                ),
                self.assertRaisesRegex(TimeoutError, "managed root deadline"),
            ):
                lease_module._ensure_managed_root(
                    Path(directory),
                    deadline=5.0,
                    monotonic=lambda: 6.0 if crossed else 0.0,
                )

            self.assertEqual(identity_calls, 1)

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
            real_close_all = lease_module._close_lease_lock_all
            coordinator_closes: list[str] = []

            def reject_active(path: Path) -> str:
                if path.name.startswith("run-"):
                    raise ValueError("injected active report path rejection")
                return real_validate(path)

            def observe_close(lock: LeaseLock, label: str) -> tuple[str, ...]:
                coordinator_closes.append(label)
                return real_close_all(lock, label)

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
                    "tools.focused_mutation_support.lease._close_lease_lock_all",
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
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            seed = ManagedScratch.create(parent)
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)

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
                _open_coordinator(managed)

            self.assertTrue(
                any("unlock failed" in note for note in caught.exception.__notes__)
            )
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
        records = [
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
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            managed = parent / "hoimin-focused-v1"
            managed.mkdir(mode=0o700)
            moved = parent / "managed-original"
            outside = parent / "outside"
            outside.mkdir(mode=0o755)
            outside_mode = stat.S_IMODE(outside.stat().st_mode)
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_directory_at"],
            )
            real_open = lease_module._open_directory_at
            swapped = False

            def swap_after_open(
                parent_fd: int, name: str, **kwargs: object
            ) -> int:
                nonlocal swapped
                descriptor = real_open(parent_fd, name, **kwargs)
                if name == "hoimin-focused-v1" and not swapped:
                    managed.rename(moved)
                    managed.symlink_to(outside, target_is_directory=True)
                    swapped = True
                return descriptor

            with mock.patch(
                "tools.focused_mutation_support.lease._open_directory_at",
                side_effect=swap_after_open,
            ), self.assertRaises(OSError):
                ManagedScratch.create(parent)

            self.assertTrue(swapped)
            self.assertEqual(stat.S_IMODE(outside.stat().st_mode), outside_mode)

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
                any(item.status is ScratchCleanupStatus.CLEAN for item in records)
            )

    def test_zero_progress_lease_marker_write_rolls_back_staging(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.os.write",
                    return_value=0,
                ),
                self.assertRaisesRegex(OSError, "made no progress"),
            ):
                ManagedScratch.create(parent)

            managed = parent / "hoimin-focused-v1"
            self.assertFalse(
                any(
                    child.name.startswith(("run-", ".staging-", ".deleting-"))
                    for child in managed.iterdir()
                )
            )

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
            self.assertEqual(
                scratch.remove_child(child).status,
                ScratchCleanupStatus.CLEAN,
            )
            scratch.mark_cleanup_ready()
            self.assertEqual(
                scratch.cleanup().status,
                ScratchCleanupStatus.CLEAN,
            )

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
            scratch.mark_cleanup_ready()
            self.assertEqual(
                scratch.cleanup().status,
                ScratchCleanupStatus.CLEAN,
            )

    def test_coordinator_contention_has_a_bounded_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            managed = Path(directory)
            with (
                mock.patch(
                    "tools.focused_mutation_support.lease.LeaseLock.acquire",
                    side_effect=BlockingIOError("busy"),
                ),
                self.assertRaisesRegex(TimeoutError, "coordinator lock"),
            ):
                _open_coordinator(managed, timeout=0.01)

    def test_coordinator_uses_shorter_timeout_than_total_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            managed = Path(directory)
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
                    timeout=0.02,
                    deadline=0.2,
                    monotonic=clock,
                    sleep=advance,
                )

            self.assertLessEqual(now, 0.021)

    def test_coordinator_does_not_initialize_after_absolute_deadline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            managed = Path(directory)
            root_fd = os.open(managed, os.O_RDONLY | os.O_DIRECTORY)
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator"],
            )
            try:
                with (
                    mock.patch.object(
                        lease_module.os,
                        "fstat",
                        wraps=lease_module.os.fstat,
                    ) as fstat,
                    self.assertRaisesRegex(TimeoutError, "deadline"),
                ):
                    _open_coordinator(
                        managed,
                        root_fd=root_fd,
                        deadline=5.0,
                        monotonic=mock.Mock(side_effect=[0.0, 6.0]),
                    )
                fstat.assert_not_called()
            finally:
                os.close(root_fd)

    def test_coordinator_parent_close_failure_also_closes_coordinator_fd(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            managed = Path(directory)
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_open_coordinator"],
            )
            real_open = lease_module.os.open
            real_close = lease_module.os.close
            opened: list[int] = []

            def record_open(*args: object, **kwargs: object) -> int:
                descriptor = real_open(*args, **kwargs)  # type: ignore[arg-type]
                opened.append(descriptor)
                return descriptor

            def close_parent_then_fail(descriptor: int) -> None:
                real_close(descriptor)
                if opened and descriptor == opened[0]:
                    raise OSError("injected coordinator parent close failure")

            with (
                mock.patch.object(lease_module.os, "open", side_effect=record_open),
                mock.patch.object(
                    lease_module.os,
                    "close",
                    side_effect=close_parent_then_fail,
                ),
                self.assertRaisesRegex(OSError, "parent close failure"),
            ):
                _open_coordinator(managed)

            self.assertEqual(len(opened), 2)
            for descriptor in opened:
                with self.assertRaises(OSError):
                    os.fstat(descriptor)

    def test_managed_root_bootstrap_stops_after_mkdir_crosses_deadline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_ensure_managed_root"],
            )
            with (
                mock.patch.object(
                    lease_module,
                    "_open_directory_at",
                    wraps=lease_module._open_directory_at,
                ) as open_directory,
                self.assertRaisesRegex(TimeoutError, "deadline"),
            ):
                lease_module._ensure_managed_root(
                    Path(directory),
                    deadline=5.0,
                    monotonic=mock.Mock(side_effect=[0.0, 0.0, 6.0]),
                )
            open_directory.assert_not_called()

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
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            with self.assertRaises(ValueError):
                ManagedScratch.create(parent, run_id="not-a-uuid")

            coordinator = _open_coordinator(
                parent / "hoimin-focused-v1",
                timeout=0.01,
            )
            coordinator.close()

    def test_invalid_run_id_preserves_managed_root_close_secondary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_close_descriptors_all"],
            )
            real_close_all = lease_module._close_descriptors_all

            def close_with_secondary(
                descriptors: tuple[tuple[str, int], ...],
            ) -> tuple[str, ...]:
                errors = real_close_all(descriptors)
                if any(
                    label == "managed scratch creation root"
                    for label, _descriptor in descriptors
                ):
                    return (*errors, "injected managed root close failure")
                return errors

            with (
                mock.patch.object(
                    lease_module,
                    "_close_descriptors_all",
                    side_effect=close_with_secondary,
                ),
                self.assertRaises(ValueError) as caught,
            ):
                ManagedScratch.create(Path(directory), run_id="not-a-uuid")

            self.assertTrue(
                any(
                    "managed root close failure" in note
                    for note in getattr(caught.exception, "__notes__", ())
                )
            )

    def test_constructor_failure_after_lease_publication_rolls_back_staging(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            original = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_write_marker_at"],
            )._write_marker_at
            calls = 0

            def fail_heartbeat(
                directory_fd: int,
                name: str,
                value: dict[str, object],
            ) -> None:
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise OSError("injected heartbeat failure")
                original(directory_fd, name, value)

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._write_marker_at",
                    side_effect=fail_heartbeat,
                ),
                self.assertRaisesRegex(OSError, "heartbeat failure"),
            ):
                ManagedScratch.create(
                    parent,
                    run_id="00000000-0000-4000-8000-000000000010",
                )

            managed = parent / "hoimin-focused-v1"
            self.assertEqual(
                sorted(path.name for path in managed.iterdir()),
                [".hoimin-coordinator"],
            )

    def test_constructor_preserves_preexisting_empty_staging(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            seed = ManagedScratch.create(parent)
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            run_id = "00000000-0000-4000-8000-000000000113"
            staging = managed / f".staging-{run_id}"
            staging.mkdir(mode=0o700)

            with self.assertRaises(FileExistsError):
                ManagedScratch.create(parent, run_id=run_id)

            self.assertTrue(staging.is_dir())

    def test_constructor_never_replaces_preexisting_empty_active(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            seed = ManagedScratch.create(parent)
            managed = seed.managed_root
            seed.mark_cleanup_ready()
            self.assertEqual(seed.cleanup().status, ScratchCleanupStatus.CLEAN)
            run_id = "00000000-0000-4000-8000-000000000114"
            active = managed / f"run-{run_id}"
            active.mkdir(mode=0o700)
            identity = (active.stat().st_dev, active.stat().st_ino)

            with self.assertRaises(FileExistsError):
                ManagedScratch.create(parent, run_id=run_id)

            self.assertEqual((active.stat().st_dev, active.stat().st_ino), identity)
            self.assertFalse((managed / f".staging-{run_id}").exists())

    def test_coordinator_close_failure_after_publish_rolls_back_active_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            lease_module = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["_close_lease_lock_all"],
            )
            real_close_all = lease_module._close_lease_lock_all

            def fail_coordinator(lock: LeaseLock, label: str) -> tuple[str, ...]:
                errors = real_close_all(lock, label)
                if label == "managed coordinator":
                    return (*errors, "injected coordinator close failure")
                return errors

            with (
                mock.patch(
                    "tools.focused_mutation_support.lease._close_lease_lock_all",
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
                [record.status for record in records],
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
            self.assertEqual(records[0].status, ScratchCleanupStatus.CLEAN)
            self.assertIsNone(records[0].remaining_root)
            self.assertIn("empty coordinator close failure", records[0].details[0])
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
            self.assertEqual(records[1].status, ScratchCleanupStatus.CLEAN)
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
                [record.status for record in records],
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
                [record.status for record in records],
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
                [record.status for record in records],
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
                [record.status for record in records],
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
                [record.status for record in records],
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
            janitor_records: list[ScratchCleanupRecord] = []
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
                    entry = next(self.inner)  # type: ignore[arg-type]
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
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            heartbeat = scratch.path / ".hoimin-heartbeat.json"
            moved = scratch.path / ".hoimin-heartbeat.original"
            heartbeat.rename(moved)
            heartbeat.write_text("replacement", encoding="utf-8")
            replacement_mtime = heartbeat.stat().st_mtime_ns

            with self.assertRaisesRegex(OSError, "heartbeat identity changed"):
                scratch.refresh_heartbeat()

            self.assertEqual(heartbeat.stat().st_mtime_ns, replacement_mtime)

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
            scratch._lease.release()

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
                    lambda writer: writer.write("{}\n"),
                )
                with self.assertRaisesRegex(ValueError, "report kind"):
                    owner.write_atomic(
                        "../sentinel",
                        lambda writer: writer.write("overwritten"),
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
                        "json", lambda writer: writer.write("new")
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
                    lambda writer: writer.write("{}\n"),
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
                    lambda writer: writer.write("x" * 1025),
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
                    lambda writer: writer.write("new"),
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
                [record.status for record in reclaim_abandoned(managed)],
                [ScratchCleanupStatus.CLEAN],
            )

    def test_finalizer_closes_lease_descriptor_without_deleting_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            scratch = ManagedScratch.create(Path(directory))
            leased = scratch.path
            managed = scratch.managed_root
            lease_fd = scratch._lease.fd
            scratch.mark_cleanup_ready()

            scratch.__del__()

            with self.assertRaises(OSError):
                os.fstat(lease_fd)
            self.assertTrue(leased.is_dir())
            self.assertEqual(
                [record.status for record in reclaim_abandoned(managed)],
                [ScratchCleanupStatus.CLEAN],
            )


if __name__ == "__main__":
    unittest.main()
