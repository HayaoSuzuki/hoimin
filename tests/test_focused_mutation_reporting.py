import json
from dataclasses import replace
from datetime import datetime, timezone
import inspect
import io
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
from collections.abc import Sequence

from tools.focused_mutation_support.model import (
    Candidate,
    CandidateState,
    CommandRecord,
    RankingReason,
    RunRecord,
    RunState,
)
from tools.focused_mutation_support.disk import (
    DiskFailure,
    DiskGuard,
    DiskObservation,
    DiskPolicy,
    DiskSecondary,
    DiskStopReason,
)
from tools.focused_mutation_support.reporting import render_markdown
from tools.focused_mutation_support.runner import (
    CommandDiskStopped,
    CommandDrainFailed,
    CommandRunner,
    CommandTimedOut,
)
from tools.focused_mutation_support.store import OwnedOutput, RunStore
from tools.focused_mutation_support.lease import (
    JanitorDiagnostic,
    ManagedScratch,
    ScratchCleanupRecord,
    ScratchCleanupStatus,
)
from tools.focused_mutation import (
    Dependencies,
    Options,
    SubprocessProbe,
    _attach_candidate_diagnostic,
    _candidate_package,
    _parser,
    _record_disk_failure,
    _record_disk_failures_for_observation,
    _scoped_signal_handlers,
    main,
    run_workflow,
)
from tools.focused_mutation_support.mutation import (
    build_baseline_command,
    build_list_command,
    build_mutation_command,
    classify_mutation_output,
    parse_list_json,
    validate_cargo_mutants_version,
)


LIST_JSON_27_1_0 = json.dumps(
    [
        {
            "diff": "--- src/lib.rs\n+++ replace add -> u64 with 0\n",
            "file": "src/lib.rs",
            "function": {
                "function_name": "add",
                "return_type": "-> u64",
                "span": {
                    "end": {"column": 2, "line": 3},
                    "start": {"column": 1, "line": 1},
                },
            },
            "genre": "FnValue",
            "name": "src/lib.rs:2:5: replace add -> u64 with 0",
            "package": "task4_fixture",
            "replacement": "0",
            "span": {
                "end": {"column": 17, "line": 2},
                "start": {"column": 5, "line": 2},
            },
        }
    ]
)

WORKFLOW_LIST_JSON = json.dumps(
    [
        {
            "file": "crates/hoimin-core/src/machine.rs",
            "name": f"machine.rs:1: replace {symbol}",
            "function": {"function_name": symbol},
        }
        for symbol in ("a", "b")
    ]
)


def fixture_candidate(
    symbol: str,
    state: CandidateState,
    *,
    not_run_reason: str | None = None,
) -> Candidate:
    return Candidate(
        path="crates/hoimin-core/src/machine.rs",
        symbol=symbol,
        mutant_name=f"machine.rs:1: replace {symbol}",
        score=100,
        reasons=[RankingReason("fixture", 100, "test fixture")],
        state=state,
        not_run_reason=not_run_reason,
    )


def fixture_record(
    *, candidates: list[Candidate], state: RunState
) -> RunRecord:
    record = RunRecord.new(total_budget_seconds=1_800.0)
    record.candidates = candidates
    record.state = state
    record.repository = {"head": "abc", "dirty": False}
    record.elapsed_seconds = 12.5
    return record


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0

    def __call__(self) -> float:
        return self.now


class WorkflowProbe:
    def __init__(
        self,
        root: Path,
        *,
        dirty: bool = False,
        timeout_command: str | None = None,
    ) -> None:
        self.root = root
        self.dirty = dirty
        self.timeout_command = timeout_command

    def text(self, argv: list[str], timeout: float) -> str:
        if self.timeout_command in argv:
            raise subprocess.TimeoutExpired(argv, timeout)
        replies = {
            ("git", "rev-parse", "--show-toplevel"): f"{self.root}\n",
            ("git", "rev-parse", "HEAD"): "abc\n",
            ("git", "branch", "--show-current"): "feature\n",
            (
                "git",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
            ): (
                " M crates/hoimin-core/src/machine.rs\0"
                if self.dirty
                else ""
            ),
            (
                "git",
                "diff",
                "--name-only",
                "-z",
                "origin/main...HEAD",
                "--",
                "*.rs",
            ): "crates/hoimin-core/src/machine.rs\0",
            (
                "git",
                "log",
                "--first-parent",
                "-20",
                "--name-only",
                "--format=",
            ): "",
        }
        return replies[tuple(argv)]


class WorkflowRunner:
    def __init__(
        self,
        output: Path,
        *,
        fail_label: str | None = None,
        timeout_label: str | None = None,
        interrupt_label: str | None = None,
        drain_label: str | None = None,
        malformed_inventory: bool = False,
        mutation_result: str = "caught",
        after_baseline: object | None = None,
        version_output: str = "cargo-mutants 27.1.0\n",
    ) -> None:
        self.output = output
        self.fail_label = fail_label
        self.timeout_label = timeout_label
        self.interrupt_label = interrupt_label
        self.drain_label = drain_label
        self.malformed_inventory = malformed_inventory
        self.mutation_result = mutation_result
        self.after_baseline = after_baseline
        self.version_output = version_output
        self.interrupted = False
        self.calls: list[tuple[list[str], Path, float, str]] = []
        self.checkpoint_command_counts: list[int] = []
        self.previous_candidate_absence: list[bool] = []

    def _record(
        self, argv: list[str], cwd: Path, label: str, sequence: int
    ) -> CommandRecord:
        stdout = self.output / "commands" / f"{sequence:04d}.stdout"
        stderr = self.output / "commands" / f"{sequence:04d}.stderr"
        if label == "cargo-mutants-version":
            value = self.version_output
        elif label == "inventory":
            value = "{" if self.malformed_inventory else WORKFLOW_LIST_JSON
        else:
            value = ""
        stdout.write_text(value, encoding="utf-8")
        stderr.write_text("", encoding="utf-8")
        return CommandRecord(
            sequence,
            label,
            argv,
            str(cwd),
            "2026-07-26T00:00:00+00:00",
            "2026-07-26T00:00:01+00:00",
            1.0,
            1 if label == self.fail_label else 0,
            stdout_path=str(stdout),
            stderr_path=str(stderr),
        )

    def run(
        self, argv: list[str], cwd: Path, timeout: float, label: str
    ) -> CommandRecord:
        saved = self.output / "run.json"
        if saved.is_file():
            self.checkpoint_command_counts.append(
                len(json.loads(saved.read_text())["commands"])
            )
        self.calls.append((argv, cwd, timeout, label))
        record = self._record(argv, cwd, label, len(self.calls))
        if label == self.timeout_label:
            record.timed_out = True
            record.exit_code = -15
            raise CommandTimedOut(record)
        if label == self.interrupt_label:
            raise KeyboardInterrupt
        if label == self.drain_label:
            record.cleanup_errors.append("stdout output drain did not settle")
            raise CommandDrainFailed(record)
        if label.startswith("baseline-") and self.after_baseline is not None:
            self.after_baseline()
        if label.startswith("mutation-"):
            candidate_number = int(label.removeprefix("mutation-"))
            if candidate_number > 1:
                previous = cwd.parent / f"candidate-{candidate_number - 1:04d}"
                self.previous_candidate_absence.append(not previous.exists())
            results = cwd / "mutants.out"
            results.mkdir(parents=True)
            selector = argv[argv.index("--re") + 1]
            exact = re.sub(r"\\(.)", r"\1", selector[1:-1]) + "\n"
            if self.mutation_result != "missing":
                (results / f"{self.mutation_result}.txt").write_text(
                    exact, encoding="utf-8"
                )
                outcome_name = {
                    "caught": "CaughtMutant",
                    "missed": "MissedMutant",
                    "timeout": "Timeout",
                    "unviable": "Unviable",
                }[self.mutation_result]
                (results / "outcomes.json").write_text(
                    json.dumps(
                        {
                            "total_mutants": 1,
                            "outcomes": [
                                {
                                    "scenario": "UnmutatedBaseline",
                                    "summary": "Success",
                                },
                                {
                                    "mutant": exact.rstrip("\n"),
                                    "summary": outcome_name,
                                }
                            ],
                        }
                    ),
                    encoding="utf-8",
                )
        return record


def workflow_fixture(
    directory: str,
    *,
    clock: FakeClock | None = None,
    dirty: bool = False,
    **runner_options: object,
) -> tuple[Options, Dependencies, WorkflowRunner]:
    root = Path(directory) / "repo"
    output = Path(directory) / "output"
    source = root / "crates/hoimin-core/src/machine.rs"
    source.parent.mkdir(parents=True)
    source.write_text("fn a() {}\nfn b() {}\n", encoding="utf-8")
    actual_clock = clock or FakeClock()
    runner = WorkflowRunner(output, **runner_options)
    options = Options(
        root,
        output,
        1_800.0,
        "origin/main",
        (),
        (),
        False,
        None,
        DiskPolicy(scratch_root=Path(directory)),
    )
    dependencies = Dependencies(
        actual_clock,
        lambda: datetime(2026, 7, 26, tzinfo=timezone.utc),
        WorkflowProbe(root, dirty=dirty),
        runner,
    )
    return options, dependencies, runner


def command_record(*, exit_code: int = 0) -> CommandRecord:
    return CommandRecord(
        sequence=1,
        label="mutation",
        argv=["cargo", "mutants"],
        cwd="/output/candidate",
        started_at="2026-07-26T00:00:00+00:00",
        ended_at="2026-07-26T00:00:01+00:00",
        elapsed_seconds=1.0,
        exit_code=exit_code,
    )


def write_outcomes_json(
    output: Path, mutant_name: str, summary: str
) -> None:
    (output / "outcomes.json").write_text(
        json.dumps(
            {
                "total_mutants": 1,
                "outcomes": [
                    {"scenario": "UnmutatedBaseline", "summary": "Success"},
                    {"mutant": mutant_name, "summary": summary},
                ],
            }
        ),
        encoding="utf-8",
    )


class FocusedMutationReportingTests(unittest.TestCase):
    def test_run_record_compatibility_dict_excludes_private_spool_capability(
        self,
    ) -> None:
        record = RunRecord.new(total_budget_seconds=1.0)
        command = command_record()
        command._spool = object()
        record.commands.append(command)

        encoded = record.to_dict()

        self.assertNotIn("_spool", encoded["commands"][0])
        json.dumps(encoded)

    def test_janitor_diagnostics_are_separate_from_cleanup_statuses(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_create = ManagedScratch.create

            def create_with_diagnostic(
                parent: Path,
                **kwargs: object,
            ) -> ManagedScratch:
                diagnostics = kwargs.get("stale_diagnostics")
                assert isinstance(diagnostics, list)
                diagnostics.append(
                    JanitorDiagnostic(("injected selection diagnostic",))
                )
                return real_create(parent, **kwargs)

            with mock.patch.object(
                ManagedScratch,
                "create",
                side_effect=create_with_diagnostic,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.stale_cleanup, [])
            self.assertEqual(
                record.stale_cleanup_diagnostics,
                ["injected selection diagnostic"],
            )

    def test_combined_disk_observation_preserves_raw_and_derived_failures(self) -> None:
        record = RunRecord.new(60.0)
        prior = DiskFailure(
            code="disk.measurement.failed",
            reason=DiskStopReason.MEASUREMENT_FAILED,
            message="monitor failed",
        )
        raw = DiskFailure(
            code="filesystem.reserve.reached",
            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
        )
        observation = DiskObservation(
            owned_bytes=100,
            available_bytes=20,
            root_owned_bytes={"owned:scratch": 70, "owned:output": 30},
        )
        policy = DiskPolicy(
            max_disk_bytes=100,
            min_free_bytes=20,
            scratch_root=Path("/tmp"),
        )

        selected = _record_disk_failures_for_observation(
            record,
            policy,
            observation,
            prior,
            raw,
        )

        self.assertIs(selected, prior)
        self.assertEqual(record.disk_stop["code"], prior.code)
        self.assertTrue(
            any(item.get("code") == raw.code for item in record.secondary_errors)
        )
        self.assertTrue(
            any(
                item.get("code") == "workspace.size.exceeded"
                and item.get("observation", {}).get("owned_bytes") == 100
                for item in record.secondary_errors
            )
        )

    def test_later_disk_failure_and_threshold_secondary_are_preserved(self) -> None:
        record = RunRecord.new(60.0)
        first = DiskFailure(
            code="filesystem.reserve.reached",
            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
        )
        later = DiskFailure(
            code="disk.measurement.failed",
            reason=DiskStopReason.MEASUREMENT_FAILED,
            message="post-clean meter failed",
            secondary=(
                DiskSecondary(
                    reason=DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
                ),
            ),
        )

        _record_disk_failure(record, first)
        _record_disk_failure(record, later)

        self.assertEqual(record.disk_stop["code"], first.code)
        self.assertTrue(
            any(
                item.get("code") == later.code
                and item.get("message") == "post-clean meter failed"
                for item in record.secondary_errors
            )
        )
        self.assertTrue(
            any(
                item.get("code") == "workspace.size.exceeded"
                and item.get("message") == "workspace_size_exceeded"
                for item in record.secondary_errors
            )
        )

    def test_final_report_boundary_observation_is_in_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            record = run_workflow(options, dependencies)

            self.assertTrue(
                any(
                    item.get("phase") == "report_boundary"
                    for item in record.disk_observations
                )
            )
            self.assertGreater(record.disk_summary["sample_count"], 0)

    def test_final_report_error_preserves_exception_notes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            error = OSError("injected report primary")
            error.add_note("injected report rollback secondary")

            with mock.patch.object(
                RunStore,
                "write_markdown",
                side_effect=error,
            ):
                record = run_workflow(options, dependencies)

            self.assertIn("injected report primary", record.report_error)
            self.assertIn("injected report rollback secondary", record.report_error)

    def test_final_report_retry_preserves_exception_notes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_checkpoint = RunStore.checkpoint
            report_failed = False

            def fail_report(store: RunStore, record: RunRecord) -> None:
                del store, record
                nonlocal report_failed
                report_failed = True
                raise OSError("injected initial report failure")

            def fail_retry(store: RunStore, record: RunRecord) -> None:
                if report_failed:
                    error = OSError("injected retry primary")
                    error.add_note("injected retry rollback secondary")
                    raise error
                real_checkpoint(store, record)

            with (
                mock.patch.object(RunStore, "write_markdown", fail_report),
                mock.patch.object(RunStore, "checkpoint", fail_retry),
            ):
                record = run_workflow(options, dependencies)

            self.assertIn("injected initial report failure", record.report_error)
            self.assertIn("injected retry primary", record.report_error)
            self.assertIn("injected retry rollback secondary", record.report_error)

    def test_candidate_diagnostic_caps_final_utf8_and_uses_observed_counts(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stdout = root / "stdout"
            stderr = root / "stderr"
            stdout.write_bytes(b"\xff" * (16 * 1024))
            stderr.write_bytes(b"\xfe" * (16 * 1024))
            command = command_record()
            command.stdout_path = str(stdout)
            command.stderr_path = str(stderr)
            command.stdout_observed_bytes = 512 * 1024
            command.stderr_observed_bytes = 512 * 1024
            candidate = Candidate("crates/a/src/lib.rs", "f", "mutant")

            remaining = _attach_candidate_diagnostic(
                candidate, command, 16 * 1024
            )

            encoded = (candidate.diagnostic or "").encode("utf-8")
            self.assertLessEqual(len(encoded), 16 * 1024)
            self.assertEqual(candidate.diagnostic_retained_bytes, len(encoded))
            self.assertEqual(candidate.diagnostic_observed_bytes, 1024 * 1024)
            self.assertTrue(candidate.diagnostic_truncated)
            self.assertEqual(remaining, 16 * 1024 - len(encoded))

    def test_previous_candidate_directory_is_absent_before_next_dispatch(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)

            record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMPLETED)
            self.assertEqual(runner.previous_candidate_absence, [True])
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertTrue(
                any(
                    item.get("phase") == "post_cleanup"
                    for item in persisted["disk_observations"]
                )
            )
            self.assertIn("start_free_bytes", persisted["disk_summary"])
            self.assertIn("end_free_bytes", persisted["disk_summary"])
            self.assertIn("free_byte_delta", persisted["disk_summary"])
            self.assertGreater(persisted["disk_summary"]["sample_count"], 0)
            self.assertEqual(
                persisted["scratch"]["cleanup"]["status"], "clean"
            )

    @unittest.skipIf(os.name == "nt", "requires POSIX signals")
    def test_real_sigterm_interrupts_and_reaps_the_active_command(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "output"
            output.mkdir()
            child_pid_file = root / "child.pid"
            program = "\n".join(
                (
                    "from pathlib import Path",
                    "import sys, threading",
                    "from tools.focused_mutation import _scoped_signal_handlers",
                    "from tools.focused_mutation_support.runner import CommandInterrupted, CommandRunner",
                    "from tools.focused_mutation_support.store import RunStore",
                    f"root = Path({str(root)!r})",
                    f"output = Path({str(output)!r})",
                    f"pid_file = Path({str(child_pid_file)!r})",
                    "store = RunStore(output)",
                    "store.commands.mkdir()",
                    "cancel = threading.Event()",
                    "runner = CommandRunner(store, cancellation_event=cancel)",
                    "child = 'import os,sys,time; from pathlib import Path; Path(sys.argv[1]).write_text(str(os.getpid()), encoding=\"utf-8\"); time.sleep(30)'",
                    "try:",
                    "    with _scoped_signal_handlers(cancel):",
                    "        runner.run([sys.executable, '-c', child, str(pid_file)], cwd=root, timeout=30.0, label='signal-child')",
                    "except CommandInterrupted:",
                    "    raise SystemExit(130)",
                    "raise SystemExit(0)",
                )
            )
            process = subprocess.Popen(
                [sys.executable, "-c", program],
                cwd=Path.cwd(),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            child_pid: int | None = None
            try:
                deadline = time.monotonic() + 5.0
                while time.monotonic() < deadline:
                    try:
                        child_pid = int(
                            child_pid_file.read_text(encoding="utf-8")
                        )
                        break
                    except (FileNotFoundError, ValueError):
                        time.sleep(0.01)
                self.assertIsNotNone(child_pid)
                os.kill(process.pid, signal.SIGTERM)
                stdout, stderr = process.communicate(timeout=5.0)
                self.assertEqual(
                    process.returncode,
                    130,
                    f"stdout={stdout!r} stderr={stderr!r}",
                )
                assert child_pid is not None
                exit_deadline = time.monotonic() + 2.0
                while time.monotonic() < exit_deadline:
                    try:
                        os.kill(child_pid, 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.01)
                else:
                    self.fail(f"signal child {child_pid} survived wrapper exit")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=2.0)
                if child_pid is not None:
                    try:
                        os.kill(child_pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_scoped_signal_handler_requests_cancellation_without_raising(self) -> None:
        cancellation = mock.Mock()
        cancellation.set = mock.Mock()

        with _scoped_signal_handlers(cancellation):
            handler = signal.getsignal(signal.SIGTERM)
            self.assertTrue(callable(handler))
            handler(signal.SIGTERM, None)  # type: ignore[misc]

        cancellation.set.assert_called_once_with()

    def test_setup_failure_rolls_back_output_marker_and_scratch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            scratch_parent = Path(directory) / "scratch-parent"
            scratch_parent.mkdir()
            options = replace(
                options,
                disk_policy=DiskPolicy(
                    max_disk_bytes=8 * 1024**3,
                    min_free_bytes=1,
                    scratch_root=scratch_parent,
                ),
            )
            with (
                mock.patch.object(
                    RunStore,
                    "initialize",
                    side_effect=OSError("injected checkpoint failure"),
                ),
                self.assertRaisesRegex(OSError, "injected checkpoint"),
            ):
                run_workflow(options, dependencies)

            self.assertFalse((options.output / ".hoimin-output-owner").exists())
            managed = scratch_parent / "hoimin-focused-v1"
            self.assertFalse(
                any(
                    child.name.startswith(("run-", ".deleting-", ".staging-"))
                    for child in managed.iterdir()
                )
            )

    def test_monitor_thread_start_failure_preserves_primary_and_rolls_back(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            with (
                mock.patch(
                    "tools.focused_mutation_support.disk.threading.Thread.start",
                    side_effect=RuntimeError("injected monitor start failure"),
                ),
                self.assertRaisesRegex(
                    RuntimeError, "injected monitor start failure"
                ),
            ):
                run_workflow(options, dependencies)

            self.assertFalse(
                (options.output / ".hoimin-output-owner").exists()
            )
            managed = options.disk_policy.scratch_root / "hoimin-focused-v1"
            self.assertFalse(
                any(
                    child.name.startswith(("run-", ".staging-", ".deleting-"))
                    for child in managed.iterdir()
                )
            )

    def test_post_create_capacity_error_explicitly_removes_owner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            with (
                mock.patch.object(
                    OwnedOutput,
                    "available_bytes",
                    side_effect=OSError("injected capacity failure"),
                ),
                self.assertRaisesRegex(OSError, "injected capacity failure"),
            ):
                run_workflow(options, dependencies)

            self.assertFalse(
                (options.output / ".hoimin-output-owner").exists()
            )

    def test_missing_effective_cargo_home_monitors_nearest_existing_ancestor(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            missing = Path(directory) / "missing-cargo-home"

            with mock.patch.dict(os.environ, {"CARGO_HOME": str(missing)}):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMPLETED)
            self.assertTrue(runner.calls)
            self.assertTrue((options.output / ".hoimin-output-owner").is_file())

    def test_missing_exact_mutation_outcome_keeps_run_incomplete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory, mutation_result="missing"
            )

            record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertEqual(record.candidates[0].state, CandidateState.ERROR)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.COMMAND_FAILED.value)

    def test_setup_failure_preserves_root_when_monitor_does_not_join(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            scratch_parent = Path(directory) / "scratch-parent"
            scratch_parent.mkdir()
            options = replace(
                options,
                disk_policy=DiskPolicy(
                    max_disk_bytes=8 * 1024**3,
                    min_free_bytes=1,
                    scratch_root=scratch_parent,
                ),
            )
            with (
                mock.patch.object(
                    RunStore,
                    "initialize",
                    side_effect=OSError("injected checkpoint failure"),
                ),
                mock.patch(
                    "tools.focused_mutation_support.disk.DiskGuard.stop_and_join",
                    return_value=False,
                ),
                self.assertRaisesRegex(OSError, "injected checkpoint"),
            ):
                run_workflow(options, dependencies)

            managed = scratch_parent / "hoimin-focused-v1"
            roots = [
                child
                for child in managed.iterdir()
                if child.name.startswith("run-")
            ]
            self.assertEqual(len(roots), 1)
            self.assertFalse((roots[0] / ".hoimin-cleanup-ready.json").exists())
            self.assertTrue((roots[0] / ".hoimin-lease.json").is_file())
            self.assertTrue((options.output / ".hoimin-output-owner").is_file())

    def test_setup_failure_preserves_clean_cleanup_close_details_as_notes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_cleanup = ManagedScratch.cleanup

            def clean_with_detail(scratch: ManagedScratch) -> ScratchCleanupRecord:
                return replace(
                    real_cleanup(scratch),
                    details=("injected setup cleanup close failure",),
                )

            with (
                mock.patch.object(
                    ManagedScratch,
                    "cleanup",
                    autospec=True,
                    side_effect=clean_with_detail,
                ),
                mock.patch.object(
                    RunStore,
                    "initialize",
                    side_effect=OSError("injected setup primary"),
                ),
                self.assertRaisesRegex(
                    OSError, "setup primary"
                ) as caught,
            ):
                run_workflow(options, dependencies)

            self.assertTrue(
                any(
                    "setup cleanup close failure" in note
                    for note in caught.exception.__notes__
                )
            )

    def test_final_monitor_join_timeout_makes_run_incomplete_and_defers_root(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            with mock.patch(
                "tools.focused_mutation_support.disk.DiskGuard.stop_and_join",
                return_value=False,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertIn("monitor", record.error)
            self.assertEqual(record.cleanup["status"], "deferred")
            remaining = record.cleanup["remaining_root"]
            self.assertIsInstance(remaining, str)
            self.assertTrue(Path(remaining).is_dir())
            self.assertFalse(
                (Path(remaining) / ".hoimin-retain.json").exists()
            )
            self.assertEqual(record.disk_stop["code"], "disk.measurement.failed")
            self.assertIn("monitor did not join", record.disk_stop["message"])
            post_cleanup = next(
                item
                for item in record.disk_observations
                if item.get("phase") == "post_cleanup"
            )
            self.assertEqual(
                post_cleanup["owned_bytes"],
                sum(post_cleanup["root_owned_bytes"].values()),
            )
            self.assertIn("owned:scratch", post_cleanup["root_owned_bytes"])
            self.assertIn("owned:output", post_cleanup["root_owned_bytes"])

    def test_monitor_join_timeout_is_secondary_to_existing_disk_stop(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)

            def timeout_after_threshold(guard: DiskGuard, timeout: float) -> bool:
                del timeout
                failure = DiskFailure(
                    code="workspace.size.exceeded",
                    reason=DiskStopReason.WORKSPACE_SIZE_EXCEEDED,
                )
                guard.failure = failure
                guard.latest_failure = failure
                return False

            with mock.patch(
                "tools.focused_mutation_support.disk.DiskGuard.stop_and_join",
                autospec=True,
                side_effect=timeout_after_threshold,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.disk_stop["code"], "workspace.size.exceeded")
            self.assertTrue(
                any(
                    item.get("code") == "disk.measurement.failed"
                    and "monitor did not join" in str(item.get("message"))
                    for item in record.secondary_errors
                )
            )

    def test_join_timeout_deduplicates_cross_root_hardlink(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            captured: dict[str, DiskObservation] = {}

            def timeout_after_link(guard: DiskGuard, timeout: float) -> bool:
                del timeout
                roots = {
                    root.enforcement: root.path
                    for root in guard.roots
                }
                scratch_file = roots["owned:scratch"] / "shared-hardlink"
                scratch_file.write_bytes(b"x" * 32)
                os.link(scratch_file, roots["owned:output"] / "shared-hardlink")
                guard.sample()
                captured["prior"] = guard.observations[-1]
                return False

            with mock.patch(
                "tools.focused_mutation_support.disk.DiskGuard.stop_and_join",
                autospec=True,
                side_effect=timeout_after_link,
            ):
                record = run_workflow(options, dependencies)

            post_cleanup = next(
                item
                for item in record.disk_observations
                if item.get("phase") == "post_cleanup"
            )
            prior = captured["prior"]
            self.assertEqual(
                post_cleanup["root_owned_bytes"]["owned:scratch"],
                prior.root_owned_bytes["owned:scratch"] - 32,
            )
            self.assertEqual(
                post_cleanup["owned_bytes"],
                sum(post_cleanup["root_owned_bytes"].values()),
            )

    def test_output_must_not_own_the_managed_scratch_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            scratch_parent = Path(directory) / "nested-output"
            scratch_parent.mkdir()
            (scratch_parent / "scratch").mkdir()
            options = replace(
                options,
                output=scratch_parent,
                disk_policy=replace(
                    options.disk_policy,
                    scratch_root=scratch_parent / "scratch",
                ),
            )

            with self.assertRaisesRegex(
                ValueError, "overlap managed scratch"
            ):
                run_workflow(options, dependencies)

    def test_clean_cleanup_close_error_becomes_terminal_report_secondary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_cleanup = ManagedScratch.cleanup

            def clean_with_close_error(scratch: ManagedScratch) -> ScratchCleanupRecord:
                result = real_cleanup(scratch)
                return replace(
                    result,
                    details=("managed lease close failed: injected",),
                )

            with mock.patch.object(
                ManagedScratch,
                "cleanup",
                autospec=True,
                side_effect=clean_with_close_error,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.cleanup["status"], "clean")
            self.assertIsNone(record.cleanup["remaining_root"])
            self.assertEqual(record.state, RunState.REPORT_FAILED)
            self.assertTrue(
                any(
                    item.get("code") == "report.delivery.failed"
                    and "managed lease close failed" in str(item.get("message"))
                    for item in record.secondary_errors
                )
            )

    def test_unverifiable_deferred_cleanup_keeps_scratch_bytes_charged(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)

            with mock.patch.object(
                ManagedScratch,
                "cleanup",
                autospec=True,
                return_value=ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    0,
                    0,
                    ("injected owned-root identity lookup failure",),
                    None,
                ),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.cleanup["status"], "deferred")
            self.assertIsNone(record.cleanup["remaining_root"])
            post_cleanup = next(
                item
                for item in record.disk_observations
                if item.get("phase") == "post_cleanup"
            )
            self.assertGreater(
                post_cleanup["root_owned_bytes"]["owned:scratch"], 0
            )
            self.assertEqual(
                post_cleanup["owned_bytes"],
                sum(post_cleanup["root_owned_bytes"].values()),
            )

    def test_preclean_meter_failure_uses_last_guard_scratch_floor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_sample = DiskGuard.sample

            def fail_only_preclean_meter(
                guard: DiskGuard,
            ) -> DiskFailure | None:
                if [root.enforcement for root in guard.roots] == [
                    "owned:scratch"
                ]:
                    failure = DiskFailure(
                        code="disk.measurement.failed",
                        reason=DiskStopReason.MEASUREMENT_FAILED,
                        message="injected pre-clean meter failure",
                    )
                    guard.failure = failure
                    guard.latest_failure = failure
                    return failure
                return real_sample(guard)

            with (
                mock.patch.object(
                    DiskGuard,
                    "sample",
                    autospec=True,
                    side_effect=fail_only_preclean_meter,
                ),
                mock.patch.object(
                    ManagedScratch,
                    "cleanup",
                    autospec=True,
                    return_value=ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        0,
                        0,
                        ("injected identity-integrity failure",),
                        None,
                    ),
                ),
            ):
                record = run_workflow(options, dependencies)

            post_cleanup = next(
                item
                for item in record.disk_observations
                if item.get("phase") == "post_cleanup"
            )
            self.assertGreater(
                post_cleanup["root_owned_bytes"]["owned:scratch"], 0
            )
            self.assertEqual(
                post_cleanup["owned_bytes"],
                sum(post_cleanup["root_owned_bytes"].values()),
            )

    def test_final_meter_failure_still_emits_preclean_scratch_floor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_sample = DiskGuard.sample

            def fail_only_final_meter(
                guard: DiskGuard,
            ) -> DiskFailure | None:
                labels = [root.enforcement for root in guard.roots]
                if labels == ["owned:output", "capacity_only:cargo_home"]:
                    failure = DiskFailure(
                        code="disk.measurement.failed",
                        reason=DiskStopReason.MEASUREMENT_FAILED,
                        message="injected final meter failure",
                    )
                    guard.failure = failure
                    guard.latest_failure = failure
                    return failure
                return real_sample(guard)

            with (
                mock.patch.object(
                    DiskGuard,
                    "sample",
                    autospec=True,
                    side_effect=fail_only_final_meter,
                ),
                mock.patch.object(
                    ManagedScratch,
                    "cleanup",
                    autospec=True,
                    return_value=ScratchCleanupRecord(
                        ScratchCleanupStatus.DEFERRED,
                        0,
                        0,
                        ("injected identity-integrity failure",),
                        None,
                    ),
                ),
            ):
                record = run_workflow(options, dependencies)

            post_cleanup = next(
                item
                for item in record.disk_observations
                if item.get("phase") == "post_cleanup"
            )
            self.assertGreater(
                post_cleanup["root_owned_bytes"]["owned:scratch"], 0
            )
            self.assertTrue(
                any(
                    item.get("code") == "disk.measurement.failed"
                    and "final meter failure" in str(item.get("message"))
                    for item in [record.disk_stop, *record.secondary_errors]
                )
            )

    def test_failed_cleanup_path_replacement_keeps_preclean_floor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_sample = DiskGuard.sample

            def seed_preclean_payload(guard: DiskGuard) -> DiskFailure | None:
                if [root.enforcement for root in guard.roots] == [
                    "owned:scratch"
                ]:
                    (guard.roots[0].path / "preclean-payload").write_bytes(
                        b"x" * 4_096
                    )
                return real_sample(guard)

            def replace_after_validation(
                scratch: ManagedScratch,
            ) -> ScratchCleanupRecord:
                validated = scratch.path.with_name(
                    f"{scratch.path.name}.validated"
                )
                escaped = Path(directory) / "escaped-owned-root"
                scratch.path.rename(validated)
                scratch.path = validated
                validated.rename(escaped)
                validated.mkdir()
                (validated / "replacement").write_bytes(b"y")
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    0,
                    0,
                    ("injected post-validation replacement",),
                    str(validated),
                )

            with (
                mock.patch.object(
                    DiskGuard,
                    "sample",
                    autospec=True,
                    side_effect=seed_preclean_payload,
                ),
                mock.patch.object(
                    ManagedScratch,
                    "cleanup",
                    autospec=True,
                    side_effect=replace_after_validation,
                ),
            ):
                record = run_workflow(options, dependencies)

            post_cleanup = next(
                item
                for item in record.disk_observations
                if item.get("phase") == "post_cleanup"
            )
            self.assertGreaterEqual(
                post_cleanup["root_owned_bytes"]["owned:scratch"], 4_096
            )
            self.assertEqual(
                post_cleanup["owned_bytes"],
                sum(post_cleanup["root_owned_bytes"].values()),
            )

    def test_deferred_same_inode_shrink_keeps_preclean_byte_floor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_sample = DiskGuard.sample
            cleanup_started = False
            identity = (101, 202)

            def sample_with_shrunk_identity(
                guard: DiskGuard,
            ) -> DiskFailure | None:
                labels = [root.enforcement for root in guard.roots]
                if labels == ["owned:scratch"]:
                    guard.observations.append(
                        DiskObservation(
                            owned_bytes=4_096,
                            available_bytes=1 << 40,
                            root_owned_bytes={"owned:scratch": 4_096},
                            identity_bytes={identity: 4_096},
                            root_owned_identities={
                                "owned:scratch": frozenset({identity})
                            },
                        )
                    )
                    return None
                if cleanup_started and "owned:scratch" in labels:
                    guard.observations.append(
                        DiskObservation(
                            owned_bytes=1,
                            available_bytes=1 << 40,
                            root_owned_bytes={
                                "owned:scratch": 1,
                                "owned:output": 0,
                            },
                            identity_bytes={identity: 1},
                            root_owned_identities={
                                "owned:scratch": frozenset({identity}),
                                "owned:output": frozenset(),
                            },
                        )
                    )
                    return None
                return real_sample(guard)

            def defer_cleanup(
                scratch: ManagedScratch,
            ) -> ScratchCleanupRecord:
                del scratch
                nonlocal cleanup_started
                cleanup_started = True
                return ScratchCleanupRecord(
                    ScratchCleanupStatus.DEFERRED,
                    0,
                    0,
                    ("injected same-inode shrink",),
                    str(options.disk_policy.scratch_root),
                )

            with (
                mock.patch.object(
                    DiskGuard,
                    "sample",
                    autospec=True,
                    side_effect=sample_with_shrunk_identity,
                ),
                mock.patch.object(
                    ManagedScratch,
                    "cleanup",
                    autospec=True,
                    side_effect=defer_cleanup,
                ),
            ):
                record = run_workflow(options, dependencies)

            post_cleanup = next(
                item
                for item in record.disk_observations
                if item.get("phase") == "post_cleanup"
            )
            self.assertEqual(
                post_cleanup["root_owned_bytes"]["owned:scratch"], 4_096
            )
            self.assertEqual(
                post_cleanup["owned_bytes"],
                sum(post_cleanup["root_owned_bytes"].values()),
            )

    def test_final_published_disk_failure_changes_completed_run_to_disk_limit(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)

            def publish_failure(guard: object, timeout: float) -> bool:
                guard.failure = DiskFailure(  # type: ignore[attr-defined]
                    code="filesystem.reserve.reached",
                    reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
                )
                return True

            with mock.patch(
                "tools.focused_mutation_support.disk.DiskGuard.stop_and_join",
                autospec=True,
                side_effect=publish_failure,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.DISK_LIMIT)
            self.assertEqual(record.error, "filesystem.reserve.reached")

    def test_clean_cleanup_does_not_restat_removed_scratch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            real_cleanup = ManagedScratch.cleanup
            real_exists = Path.exists
            cleanup_finished = False

            def observe_cleanup(
                scratch: ManagedScratch, *args: object, **kwargs: object
            ) -> ScratchCleanupRecord:
                nonlocal cleanup_finished
                result = real_cleanup(scratch, *args, **kwargs)
                cleanup_finished = True
                return result

            def forbid_removed_scratch_lookup(path: Path) -> bool:
                if cleanup_finished and path.name.startswith(".deleting-"):
                    raise AssertionError(
                        "removed scratch was restatted after clean cleanup"
                    )
                return real_exists(path)

            with (
                mock.patch.object(
                    ManagedScratch,
                    "cleanup",
                    autospec=True,
                    side_effect=observe_cleanup,
                ),
                mock.patch.object(
                    Path,
                    "exists",
                    autospec=True,
                    side_effect=forbid_removed_scratch_lookup,
                ),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMPLETED)
            self.assertIsNotNone(
                record.disk_summary[
                    "absence_verified_removed_logical_bytes"
                ]
            )

    def test_final_published_disk_failure_wins_over_baseline_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory, fail_label="baseline-hoimin-core"
            )

            def publish_failure(guard: object, timeout: float) -> bool:
                del timeout
                guard.failure = DiskFailure(  # type: ignore[attr-defined]
                    code="filesystem.reserve.reached",
                    reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
                )
                return True

            with mock.patch(
                "tools.focused_mutation_support.disk.DiskGuard.stop_and_join",
                autospec=True,
                side_effect=publish_failure,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.DISK_LIMIT)
            self.assertEqual(record.error, "filesystem.reserve.reached")
            self.assertIn(
                {
                    "kind": "outcome",
                    "code": RunState.BASELINE_FAILED.value,
                    "message": RunState.BASELINE_FAILED.value,
                },
                record.secondary_errors,
            )

    def test_repository_report_path_is_validated_before_commands(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            focused_module = __import__(
                "tools.focused_mutation", fromlist=["validate_reported_path"]
            )
            real_validate = __import__(
                "tools.focused_mutation_support.lease",
                fromlist=["validate_reported_path"],
            ).validate_reported_path

            def reject_repository(path: Path) -> str:
                if path == options.repository:
                    raise ValueError("injected repository report path rejection")
                return real_validate(path)

            with mock.patch.object(
                focused_module,
                "validate_reported_path",
                side_effect=reject_repository,
                create=True,
            ):
                with self.assertRaisesRegex(
                    ValueError, "repository report path rejection"
                ):
                    run_workflow(options, dependencies)

            self.assertEqual(runner.calls, [])

    def test_monitor_join_records_distinct_latest_failure_after_sticky_stop(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            sticky = DiskFailure(
                code="filesystem.reserve.reached",
                reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
            )
            later = DiskFailure(
                code="disk.measurement.failed",
                reason=DiskStopReason.MEASUREMENT_FAILED,
                message="injected in-flight monitor failure",
            )

            def publish_both(guard: DiskGuard, timeout: float) -> bool:
                del timeout
                guard.failure = sticky
                guard.latest_failure = later
                return True

            with mock.patch(
                "tools.focused_mutation_support.disk.DiskGuard.stop_and_join",
                autospec=True,
                side_effect=publish_both,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.disk_stop["code"], sticky.code)
            self.assertTrue(
                any(
                    item.get("code") == later.code
                    and "in-flight" in str(item.get("message"))
                    for item in record.secondary_errors
                )
            )

    def test_initial_disk_stop_is_typed_and_launches_no_command(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)

            def fail_initial_sample(guard: object) -> None:
                guard.failure = DiskFailure(  # type: ignore[attr-defined]
                    code="filesystem.reserve.reached",
                    reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
                )

            with mock.patch(
                "tools.focused_mutation_support.disk.DiskGuard.start",
                autospec=True,
                side_effect=fail_initial_sample,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.DISK_LIMIT)
            self.assertEqual(record.error, "filesystem.reserve.reached")
            self.assertEqual(runner.calls, [])
            self.assertEqual(record.cleanup["status"], "clean")
            self.assertEqual(record.cleanup["omitted_detail_count"], 0)
            self.assertEqual(
                record.disk_stop,
                {
                    "code": "filesystem.reserve.reached",
                    "reason": "filesystem_reserve_reached",
                    "message": None,
                    "observation": None,
                },
            )
            self.assertEqual(
                set(record.disk_summary),
                {
                    "start_free_bytes",
                    "end_free_bytes",
                    "free_byte_delta",
                    "peak_owned_bytes",
                    "minimum_free_bytes",
                    "sample_count",
                    "maximum_measurement_seconds",
                    "absence_verified_removed_logical_bytes",
                },
            )

    def test_disk_stop_survives_its_immediate_checkpoint_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            command_runner = CommandRunner(RunStore(options.output))
            dependencies = replace(dependencies, runner=command_runner)
            failure = DiskFailure(
                code="filesystem.reserve.reached",
                reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
            )
            command = command_record()
            command.label = "cargo-mutants-version"
            original_checkpoint = RunStore.checkpoint
            checkpoint_calls = 0

            def fail_second_checkpoint(
                store: RunStore, record: RunRecord
            ) -> None:
                nonlocal checkpoint_calls
                checkpoint_calls += 1
                if checkpoint_calls == 4:
                    raise OSError("injected checkpoint failure")
                original_checkpoint(store, record)

            with (
                mock.patch.object(
                    CommandRunner,
                    "run",
                    side_effect=CommandDiskStopped(command, failure),
                ),
                mock.patch.object(
                    RunStore,
                    "checkpoint",
                    autospec=True,
                    side_effect=fail_second_checkpoint,
                ),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.DISK_LIMIT)
            self.assertEqual(record.error, "filesystem.reserve.reached")

    def test_report_failure_does_not_replace_interruption(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory, interrupt_label="inventory"
            )

            with mock.patch.object(
                RunStore,
                "write_markdown",
                side_effect=OSError("injected final report failure"),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.INTERRUPTED)
            self.assertNotEqual(record.error, "injected final report failure")
            self.assertEqual(
                record.report_error, "injected final report failure"
            )

    def test_output_marker_close_failure_is_terminal_report_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            original_release = OwnedOutput.release_marker

            def release_with_error(
                owner: OwnedOutput, *, remove_marker: bool = False
            ) -> tuple[str, ...]:
                original_release(owner, remove_marker=remove_marker)
                return ("injected output marker close failure",)

            with mock.patch.object(
                OwnedOutput,
                "release_marker",
                autospec=True,
                side_effect=release_with_error,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.REPORT_FAILED)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.COMPLETED.value)
            self.assertIn("output marker close failure", record.report_error)

    def test_actual_meter_close_failure_is_terminal_and_nonzero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)

            original_close = DiskGuard.close

            def close_with_error(guard: DiskGuard) -> tuple[str, ...]:
                errors = original_close(guard)
                if any(root.enforcement == "owned:output" for root in guard.roots):
                    return (*errors, "injected capability close failure")
                return errors

            with mock.patch.object(
                DiskGuard,
                "close",
                autospec=True,
                side_effect=close_with_error,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.REPORT_FAILED)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.COMPLETED.value)
            self.assertIn("capability close failure", record.report_error)

    def test_actual_output_ownership_close_failure_is_terminal(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            original_close = OwnedOutput.close_directory

            def close_with_error(owner: OwnedOutput) -> tuple[str, ...]:
                errors = original_close(owner)
                return (*errors, "injected output ownership close failure")

            with mock.patch.object(
                OwnedOutput,
                "close_directory",
                autospec=True,
                side_effect=close_with_error,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.REPORT_FAILED)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.COMPLETED.value)
            self.assertIn(
                "output ownership close failure", record.report_error
            )

    def test_initial_stop_keeps_output_lock_through_final_checkpoint(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            original_checkpoint = RunStore.checkpoint
            competing_attempted = False

            def fail_initial_sample(guard: DiskGuard) -> None:
                guard.failure = DiskFailure(
                    code="filesystem.reserve.reached",
                    reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
                )

            def checkpoint_while_competing(
                store: RunStore, record: RunRecord
            ) -> None:
                nonlocal competing_attempted
                if not competing_attempted:
                    competing_attempted = True
                    with self.assertRaises(ValueError):
                        OwnedOutput.create(options.output, "competing-run")
                original_checkpoint(store, record)

            with (
                mock.patch.object(
                    DiskGuard,
                    "start",
                    autospec=True,
                    side_effect=fail_initial_sample,
                ),
                mock.patch.object(
                    RunStore,
                    "checkpoint",
                    autospec=True,
                    side_effect=checkpoint_while_competing,
                ),
            ):
                record = run_workflow(options, dependencies)

            self.assertTrue(competing_attempted)
            self.assertEqual(record.state, RunState.DISK_LIMIT)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.DISK_LIMIT.value)

    def test_pre_cleanup_meter_failure_prevents_completed_report(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            original_sample = DiskGuard.sample

            def fail_execution_meter(guard: DiskGuard) -> DiskFailure | None:
                if [root.enforcement for root in guard.roots] == ["owned:scratch"]:
                    failure = DiskFailure(
                        code="disk.measurement.failed",
                        reason=DiskStopReason.MEASUREMENT_FAILED,
                        message="injected pre-clean measurement failure",
                    )
                    guard.failure = failure
                    return failure
                return original_sample(guard)

            with mock.patch.object(
                DiskGuard,
                "sample",
                autospec=True,
                side_effect=fail_execution_meter,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.DISK_LIMIT)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.DISK_LIMIT.value)
            self.assertIn("pre-clean measurement failure", persisted["report_error"])

    def test_repository_probe_cleanup_failure_is_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            stdout = Path(directory) / "stdout"
            stderr = Path(directory) / "stderr"
            stdout.write_text("probe result", encoding="utf-8")
            stderr.write_text("", encoding="utf-8")
            command = command_record()
            command.stdout_path = str(stdout)
            command.stderr_path = str(stderr)
            runner = mock.Mock()
            runner.run.return_value = command
            probe = SubprocessProbe(Path(directory), runner)

            with (
                mock.patch(
                    "tools.focused_mutation._discard_command_spool",
                    return_value=False,
                ),
                self.assertRaisesRegex(OSError, "spool cleanup"),
            ):
                probe.text(["git", "status"], 1.0)

    def test_repository_probes_use_workflow_command_dispatch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, base_runner = workflow_fixture(directory)
            probe_responses = WorkflowProbe(options.repository)

            class ProbeCommandRunner(CommandRunner):
                def __init__(self) -> None:
                    super().__init__(RunStore(options.output))
                    self.delegate = WorkflowRunner(base_runner.output)
                    self.dispatched: list[dict[str, object]] = []

                def run(
                    self,
                    argv: Sequence[str],
                    cwd: Path,
                    timeout: float,
                    label: str,
                    **kwargs: object,
                ) -> CommandRecord:
                    self.dispatched.append({"label": label, **kwargs})
                    (self.delegate.output / "commands").mkdir(exist_ok=True)
                    command = self.delegate.run(
                        list(argv), cwd, timeout, label
                    )
                    if label.startswith("probe-"):
                        Path(command.stdout_path).write_text(
                            probe_responses.text(list(argv), timeout),
                            encoding="utf-8",
                        )
                    return command

            runner = ProbeCommandRunner()
            dependencies = Dependencies(
                dependencies.monotonic,
                dependencies.utc_now,
                SubprocessProbe(options.repository, runner),
                runner,
            )

            record = run_workflow(options, dependencies)

            probe_commands = [
                command
                for command in record.commands
                if command.label.startswith("probe-")
            ]
            self.assertGreaterEqual(
                len(probe_commands),
                1,
                (record.state, record.error, runner.dispatched),
            )
            probe_dispatches = [
                item
                for item in runner.dispatched
                if str(item["label"]).startswith("probe-")
            ]
            self.assertEqual(len(probe_dispatches), len(probe_commands))
            self.assertTrue(
                all(isinstance(item.get("disk_guard"), DiskGuard) for item in probe_dispatches)
            )
            self.assertTrue(
                all(item.get("max_log_bytes") == 64 * 1024 for item in probe_dispatches)
            )
            for item in probe_dispatches:
                environment = item.get("environment")
                self.assertIsInstance(environment, dict)
                assert isinstance(environment, dict)
                self.assertIn("TMPDIR", environment)
                self.assertIn("CARGO_TARGET_DIR", environment)
            self.assertTrue(
                all(
                    not Path(command.stdout_path).exists()
                    and not Path(command.stderr_path).exists()
                    for command in probe_commands
                )
            )

    def test_version_spool_cleanup_failure_stops_before_inventory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            real_discard = __import__(
                "tools.focused_mutation",
                fromlist=["_discard_command_spool"],
            )._discard_command_spool

            def fail_version_cleanup(command: CommandRecord) -> bool:
                clean = real_discard(command)
                if command.label == "cargo-mutants-version":
                    command.cleanup_errors.append(
                        "injected version spool cleanup failure"
                    )
                    return False
                return clean

            with mock.patch(
                "tools.focused_mutation._discard_command_spool",
                side_effect=fail_version_cleanup,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertIn("spool cleanup", record.error)
            self.assertNotIn(
                "inventory", [label for _argv, _cwd, _timeout, label in runner.calls]
            )

    def test_version_failure_remains_primary_when_spool_cleanup_also_fails(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(
                directory,
                fail_label="cargo-mutants-version",
            )
            real_discard = __import__(
                "tools.focused_mutation",
                fromlist=["_discard_command_spool"],
            )._discard_command_spool

            def fail_version_cleanup(command: CommandRecord) -> bool:
                clean = real_discard(command)
                if command.label == "cargo-mutants-version":
                    command.cleanup_errors.append(
                        "injected version spool cleanup failure"
                    )
                    return False
                return clean

            with mock.patch(
                "tools.focused_mutation._discard_command_spool",
                side_effect=fail_version_cleanup,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.TOOL_UNAVAILABLE)
            self.assertEqual(record.error, "cargo-mutants --version failed")
            self.assertTrue(
                any(
                    item.get("code") == "command.spool.cleanup.failed"
                    and "version" in str(item.get("message"))
                    for item in record.secondary_errors
                )
            )
            self.assertNotIn(
                "inventory", [label for _argv, _cwd, _timeout, label in runner.calls]
            )

    def test_baseline_failure_remains_primary_when_spool_cleanup_also_fails(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(
                directory,
                fail_label="baseline-hoimin-core",
            )
            real_discard = __import__(
                "tools.focused_mutation",
                fromlist=["_discard_command_spool"],
            )._discard_command_spool

            def fail_baseline_cleanup(command: CommandRecord) -> bool:
                clean = real_discard(command)
                if command.label == "baseline-hoimin-core":
                    command.cleanup_errors.append(
                        "injected baseline spool cleanup failure"
                    )
                    return False
                return clean

            with mock.patch(
                "tools.focused_mutation._discard_command_spool",
                side_effect=fail_baseline_cleanup,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.BASELINE_FAILED)
            self.assertTrue(
                all(
                    item.not_run_reason == "baseline_failed"
                    for item in record.candidates
                )
            )
            self.assertTrue(
                any(
                    item.get("code") == "command.spool.cleanup.failed"
                    and "baseline" in str(item.get("message"))
                    for item in record.secondary_errors
                )
            )
            self.assertFalse(
                any(label.startswith("mutation-") for *_, label in runner.calls)
            )

    def test_repository_probe_primary_keeps_spool_cleanup_secondary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            primary = OSError("injected repository probe primary")
            primary.add_note("injected repository probe spool cleanup failure")
            dependencies.probe.text = mock.Mock(side_effect=primary)

            record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.TOOL_UNAVAILABLE)
            self.assertIn("injected repository probe primary", record.error)
            self.assertIn(
                "injected repository probe spool cleanup failure", record.error
            )

    def test_version_stderr_truncation_stops_before_inventory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            original_record = runner._record

            def truncated_version(
                argv: list[str], cwd: Path, label: str, sequence: int
            ) -> CommandRecord:
                command = original_record(argv, cwd, label, sequence)
                if label == "cargo-mutants-version":
                    command.stderr_truncated = True
                return command

            runner._record = truncated_version

            record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertIn("truncated", record.error)
            self.assertNotIn(
                "inventory", [label for _argv, _cwd, _timeout, label in runner.calls]
            )

    def test_version_stdout_read_failure_still_discards_spool(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            discarded: list[str] = []
            real_discard = __import__(
                "tools.focused_mutation",
                fromlist=["_discard_command_spool"],
            )._discard_command_spool

            def observe_discard(command: CommandRecord) -> bool:
                discarded.append(command.label)
                return real_discard(command)

            with (
                mock.patch(
                    "tools.focused_mutation._read_stdout",
                    side_effect=UnicodeDecodeError(
                        "utf-8", b"\xff", 0, 1, "invalid start byte"
                    ),
                ),
                mock.patch(
                    "tools.focused_mutation._discard_command_spool",
                    side_effect=observe_discard,
                ),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertEqual(discarded, ["cargo-mutants-version"])

    def test_inventory_spool_cleanup_failure_stops_before_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            real_discard = __import__(
                "tools.focused_mutation",
                fromlist=["_discard_command_spool"],
            )._discard_command_spool

            def fail_inventory_cleanup(command: CommandRecord) -> bool:
                clean = real_discard(command)
                if command.label == "inventory":
                    command.cleanup_errors.append(
                        "injected inventory spool cleanup failure"
                    )
                    return False
                return clean

            with mock.patch(
                "tools.focused_mutation._discard_command_spool",
                side_effect=fail_inventory_cleanup,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertIn("spool cleanup", record.error)
            self.assertFalse(
                any(
                    label.startswith("baseline-")
                    for _argv, _cwd, _timeout, label in runner.calls
                )
            )

    def test_main_prints_bounded_setup_cleanup_notes(self) -> None:
        primary = OSError("injected setup failure")
        primary.add_note(
            "setup rollback left managed scratch: status=deferred; "
            "remaining_root='/tmp/managed-run'"
        )
        stderr = io.StringIO()
        with (
            mock.patch(
                "tools.focused_mutation.run_workflow", side_effect=primary
            ),
            mock.patch("tools.focused_mutation.sys.stderr", stderr),
            self.assertRaises(SystemExit) as raised,
        ):
            main(["--output", "/tmp/focused-output"])

        self.assertEqual(raised.exception.code, 2)
        self.assertIn("remaining_root='/tmp/managed-run'", stderr.getvalue())
        self.assertLessEqual(len(stderr.getvalue().encode("utf-8")), 24 * 1024)

    def test_main_treats_signal_before_handler_restore_as_interruption(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"

            def finish_after_signal(
                _options: Options,
                dependencies: Dependencies,
                **_kwargs: object,
            ) -> RunRecord:
                event = dependencies.runner._cancellation_event  # type: ignore[attr-defined]
                self.assertIsNotNone(event)
                event.set()
                return fixture_record(candidates=[], state=RunState.COMPLETED)

            with mock.patch(
                "tools.focused_mutation.run_workflow",
                side_effect=finish_after_signal,
            ):
                exit_code = main(["--output", str(output)])

            self.assertEqual(exit_code, 130)

    def test_signal_during_final_report_persists_interrupted_run_json(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            real_write_markdown = RunStore.write_markdown

            def interrupt_before_markdown(
                store: RunStore, record: RunRecord
            ) -> None:
                runner.interrupted = True
                real_write_markdown(store, record)

            with mock.patch.object(
                RunStore,
                "write_markdown",
                autospec=True,
                side_effect=interrupt_before_markdown,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.INTERRUPTED)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.INTERRUPTED.value)
            self.assertNotEqual(persisted["state"], RunState.COMPLETED.value)

    def test_signal_at_handler_restore_persists_without_cancel_guard(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)

            def interrupt_while_restoring() -> None:
                runner.interrupted = True

            dependencies = replace(
                dependencies,
                restore_signal_handlers=interrupt_while_restoring,
            )
            record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.INTERRUPTED)
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.INTERRUPTED.value)

    def test_signal_restore_checkpoint_failure_marks_report_failed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            restored = False
            original_checkpoint = RunStore.checkpoint

            def interrupt_while_restoring() -> None:
                nonlocal restored
                restored = True
                runner.interrupted = True

            def fail_after_restore(
                store: RunStore, record: RunRecord
            ) -> None:
                if restored:
                    raise OSError("injected restore checkpoint failure")
                original_checkpoint(store, record)

            dependencies = replace(
                dependencies,
                restore_signal_handlers=interrupt_while_restoring,
            )
            with mock.patch.object(
                RunStore,
                "checkpoint",
                autospec=True,
                side_effect=fail_after_restore,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.INTERRUPTED)
            self.assertIn("restore checkpoint failure", record.report_error)

    def test_cleanup_ready_failure_defers_root_for_janitor_and_delivers_evidence(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            with mock.patch(
                "tools.focused_mutation_support.lease.ManagedScratch.mark_cleanup_ready",
                side_effect=OSError("injected cleanup-ready failure"),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertIn("cleanup-ready", record.error)
            self.assertEqual(record.cleanup["status"], "deferred")
            self.assertTrue((options.output / "run.json").is_file())
            self.assertTrue((options.output / "report.md").is_file())

    def test_candidate_package_accepts_shortest_workspace_member_path(
        self,
    ) -> None:
        self.assertEqual(_candidate_package("crates/example/lib.rs"), "example")

    def test_command_cleanup_errors_are_json_serializable(self) -> None:
        record = fixture_record(candidates=[], state=RunState.COMMAND_FAILED)
        command = command_record()
        command.cleanup_errors.append(
            "stderr log was not delete-ready within 2.0 seconds"
        )
        record.commands.append(command)

        encoded = json.loads(json.dumps(record.to_dict()))

        self.assertEqual(
            encoded["commands"][0]["cleanup_errors"],
            ["stderr log was not delete-ready within 2.0 seconds"],
        )
        self.assertEqual(CommandRecord(
            sequence=2,
            label="empty",
            argv=[],
            cwd=".",
            started_at="2026-07-28T00:00:00+00:00",
        ).cleanup_errors, [])

    def test_report_exposes_unknown_exit_after_lifecycle_cleanup_failure(
        self,
    ) -> None:
        record = fixture_record(candidates=[], state=RunState.COMMAND_FAILED)
        command = command_record()
        command.label = "mutation-0001"
        command.exit_code = None
        command.cleanup_errors.append(
            "process lifecycle cleanup failed: "
            "root process 12345 was not reaped after forced kill"
        )
        record.commands.append(command)

        encoded = json.loads(json.dumps(record.to_dict()))
        markdown = render_markdown(record)

        self.assertIsNone(encoded["commands"][0]["exit_code"])
        self.assertEqual(
            encoded["commands"][0]["cleanup_errors"],
            [
                "process lifecycle cleanup failed: "
                "root process 12345 was not reaped after forced kill"
            ],
        )
        self.assertIn("## Command cleanup failures", markdown)
        self.assertIn("`mutation-0001` — exit `unknown`", markdown)
        self.assertIn("not reaped after forced kill", markdown)

    def test_report_preserves_verified_unverified_and_next_order(self) -> None:
        record = fixture_record(
            candidates=[
                fixture_candidate("a", CandidateState.SURVIVED),
                fixture_candidate(
                    "b",
                    CandidateState.NOT_RUN,
                    not_run_reason="reporting_reserve",
                ),
            ],
            state=RunState.BUDGET_EXHAUSTED,
        )
        markdown = render_markdown(record)
        self.assertIn("State: `budget_exhausted`", markdown)
        self.assertIn("`a` — survived", markdown)
        self.assertIn("`b` — reporting_reserve", markdown)
        self.assertLess(markdown.index("`a`"), markdown.index("`b`"))

    def test_timeout_and_error_are_unverified_and_recommended_in_record_order(self) -> None:
        record = fixture_record(
            candidates=[
                fixture_candidate("timed", CandidateState.TIMEOUT),
                fixture_candidate("broken", CandidateState.ERROR),
                fixture_candidate("killed", CandidateState.KILLED),
            ],
            state=RunState.COMPLETED,
        )

        markdown = render_markdown(record)
        verified = markdown.split("## Verified candidates", 1)[1].split(
            "## Investigation results", 1
        )[0]
        investigation = markdown.split("## Investigation results", 1)[1].split(
            "## Unverified candidates", 1
        )[0]
        unverified = markdown.split("## Unverified candidates", 1)[1].split(
            "## Next recommended order", 1
        )[0]
        recommended = markdown.split("## Next recommended order", 1)[1].split(
            "## Manual classification", 1
        )[0]

        self.assertIn("`killed` — killed", verified)
        self.assertNotIn("`timed`", verified)
        self.assertNotIn("`broken`", verified)
        self.assertIn("`timed` — timeout", investigation)
        self.assertIn("`broken` — error", investigation)
        self.assertIn("`timed` — timeout", unverified)
        self.assertIn("`broken` — error", unverified)
        self.assertIn("1. `timed`", recommended)
        self.assertIn("2. `broken`", recommended)
        self.assertLess(recommended.index("`timed`"), recommended.index("`broken`"))
        encoded = json.loads(json.dumps(record.to_dict()))
        self.assertEqual(
            [item["state"] for item in encoded["candidates"]],
            ["timeout", "error", "killed"],
        )

    def test_every_candidate_state_has_documented_report_membership(self) -> None:
        record = fixture_record(
            candidates=[
                fixture_candidate(state.value, state)
                for state in CandidateState
            ],
            state=RunState.COMPLETED,
        )

        markdown = render_markdown(record)
        verified = markdown.split("## Verified candidates", 1)[1].split(
            "## Investigation results", 1
        )[0]
        investigation = markdown.split("## Investigation results", 1)[1].split(
            "## Unverified candidates", 1
        )[0]
        unverified = markdown.split("## Unverified candidates", 1)[1].split(
            "## Next recommended order", 1
        )[0]
        recommended = markdown.split("## Next recommended order", 1)[1].split(
            "## Manual classification", 1
        )[0]

        self.assertIn("- Unverified candidates: `5`", markdown)
        for symbol in ("killed", "survived"):
            self.assertIn(f"`{symbol}`", verified)
            self.assertNotIn(f"`{symbol}`", unverified)
        for symbol in ("pending", "timeout", "unviable", "not_run", "error"):
            self.assertNotIn(f"`{symbol}`", verified)
            self.assertIn(f"`{symbol}`", unverified)
        for symbol in ("survived", "timeout", "unviable", "error"):
            self.assertIn(f"`{symbol}`", investigation)
        for symbol in ("pending", "killed", "not_run"):
            self.assertNotIn(f"`{symbol}`", investigation)
        self.assertEqual(
            [
                line.split("`", 2)[1]
                for line in recommended.splitlines()
                if line[:1].isdigit()
            ],
            ["pending", "timeout", "unviable", "not_run", "error"],
        )

    def test_baseline_failure_checkpoints_and_skips_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(
                directory, fail_label="baseline-hoimin-core"
            )
            record = run_workflow(options, dependencies)
            self.assertEqual(record.state, RunState.BASELINE_FAILED)
            self.assertFalse(
                any(label.startswith("mutation-") for *_, label in runner.calls)
            )
            self.assertTrue((options.output / "run.json").is_file())
            self.assertTrue(
                all(
                    item.not_run_reason == "baseline_failed"
                    for item in record.candidates
                )
            )

    def test_discovery_timeout_discards_its_command_spool(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(
                directory, timeout_label="cargo-mutants-version"
            )

            record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            version = next(
                command
                for command in record.commands
                if command.label == "cargo-mutants-version"
            )
            self.assertFalse(Path(version.stdout_path).exists())
            self.assertFalse(Path(version.stderr_path).exists())

    def test_next_package_baseline_disk_stop_does_not_mutate_prior_candidate(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            discovered = [
                Candidate(
                    "crates/hoimin-core/src/machine.rs",
                    "a",
                    "machine.rs:1: replace a",
                ),
                Candidate(
                    "crates/hoimin-cli/src/main.rs",
                    "b",
                    "main.rs:1: replace b",
                ),
            ]
            original_run = runner.run

            def stop_second_baseline(
                argv: list[str], cwd: Path, timeout: float, label: str
            ) -> CommandRecord:
                command = original_run(argv, cwd, timeout, label)
                if label == "baseline-hoimin-cli":
                    raise CommandDiskStopped(
                        command,
                        DiskFailure(
                            code="filesystem.reserve.reached",
                            reason=DiskStopReason.FILESYSTEM_RESERVE_REACHED,
                        ),
                    )
                return command

            runner.run = stop_second_baseline
            with (
                mock.patch(
                    "tools.focused_mutation.discover_candidates",
                    return_value=discovered,
                ),
                mock.patch(
                    "tools.focused_mutation.parse_list_json",
                    return_value=discovered,
                ),
            ):
                record = run_workflow(options, dependencies)

            first = next(item for item in record.candidates if item.symbol == "a")
            stopped = next(
                item
                for item in record.commands
                if item.label == "baseline-hoimin-cli"
            )
            self.assertEqual(record.state, RunState.DISK_LIMIT)
            self.assertNotIn(stopped.sequence, first.command_sequences)

    def test_qualified_inventory_method_matches_bare_discovery_symbol(
        self,
    ) -> None:
        inventory = json.dumps(
            [
                {
                    "file": "crates/hoimin-core/src/machine.rs",
                    "name": "machine.rs:1: replace accept_completion",
                    "function": {
                        "function_name": "<hoimin_core::machine::RunState as StateMachine>::accept_completion"
                    },
                }
            ]
        )
        selected = [
            Candidate(
                "crates/hoimin-core/src/machine.rs",
                "accept_completion",
                None,
            )
        ]
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            options = replace(options, symbols=("accept_completion",))
            with (
                mock.patch(
                    "tools.focused_mutation.discover_candidates",
                    return_value=selected,
                ),
                mock.patch(f"{__name__}.WORKFLOW_LIST_JSON", inventory),
            ):
                record = run_workflow(options, dependencies)

        self.assertEqual(len(record.candidates), 1)
        candidate = record.candidates[0]
        self.assertEqual(
            candidate.symbol,
            "<hoimin_core::machine::RunState as StateMachine>::accept_completion",
        )
        self.assertIn(
            "explicit_symbol", [reason.code for reason in candidate.reasons]
        )

    def test_empty_preliminary_selection_fails_without_workspace_inventory(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            clock = FakeClock()
            options, dependencies, runner = workflow_fixture(
                directory, clock=clock
            )

            def finish_discovery(
                *_: object, **__: object
            ) -> list[Candidate]:
                clock.now = 650.0
                return []

            with mock.patch(
                "tools.focused_mutation.discover_candidates",
                side_effect=finish_discovery,
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertIn("no focused mutation candidates", record.error)
            self.assertFalse(
                any(call[3] in {"inventory", "baseline-hoimin-core"} for call in runner.calls)
            )

    def test_candidates_outside_workspace_members_are_skipped_and_checkpointed(
        self,
    ) -> None:
        inventory = json.dumps(
            [
                {
                    "file": "build.rs",
                    "name": "build.rs:1: replace main with ()",
                    "function": {"function_name": "main"},
                },
                {
                    "file": "crates/hoimin-core",
                    "name": "hoimin-core: replace package entry",
                    "function": {"function_name": "package"},
                },
                {
                    "file": "tools/helper.py",
                    "name": "helper.py:1: replace helper with ()",
                    "function": {"function_name": "helper"},
                },
                {
                    "file": "crates/hoimin-core/src/machine.rs",
                    "name": "machine.rs:1: replace a",
                    "function": {"function_name": "a"},
                },
            ]
        )
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            with (
                mock.patch(
                    "tools.focused_mutation.discover_candidates",
                    return_value=[
                        Candidate(
                            "crates/hoimin-core/src/machine.rs", "a", None
                        )
                    ],
                ),
                mock.patch(
                    "tools.focused_mutation.rank_candidates",
                    side_effect=lambda candidates, *_: candidates,
                ),
                mock.patch(f"{__name__}.WORKFLOW_LIST_JSON", inventory),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMPLETED)
            self.assertEqual(len(record.candidates), 1)
            by_path = {candidate.path: candidate for candidate in record.candidates}
            self.assertNotIn("build.rs", by_path)
            self.assertNotIn("crates/hoimin-core", by_path)
            self.assertNotIn("tools/helper.py", by_path)
            self.assertNotEqual(
                by_path["crates/hoimin-core/src/machine.rs"].state,
                CandidateState.NOT_RUN,
            )
            self.assertTrue(
                any(
                    label.startswith(("baseline-", "mutation-"))
                    for *_, label in runner.calls
                )
            )
            persisted = json.loads((options.output / "run.json").read_text())
            self.assertEqual(persisted["state"], RunState.COMPLETED.value)

    def test_outside_workspace_candidate_is_classified_before_budget_stops_run(
        self,
    ) -> None:
        inventory = json.dumps(
            [
                {
                    "file": "build.rs",
                    "name": "build.rs:1: replace main with ()",
                    "function": {"function_name": "main"},
                },
                {
                    "file": "crates/hoimin-core/src/machine.rs",
                    "name": "machine.rs:1: replace a",
                    "function": {"function_name": "a"},
                },
            ]
        )
        with tempfile.TemporaryDirectory() as directory:
            clock = FakeClock()

            def enter_reserve() -> None:
                clock.now = 1_500.0

            options, dependencies, _ = workflow_fixture(
                directory, clock=clock, after_baseline=enter_reserve
            )
            with (
                mock.patch(
                    "tools.focused_mutation.discover_candidates",
                    return_value=[
                        Candidate(
                            "crates/hoimin-core/src/machine.rs", "a", None
                        )
                    ],
                ),
                mock.patch(f"{__name__}.WORKFLOW_LIST_JSON", inventory),
            ):
                record = run_workflow(options, dependencies)

            by_path = {candidate.path: candidate for candidate in record.candidates}
            self.assertEqual(record.state, RunState.BUDGET_EXHAUSTED)
            self.assertNotIn("build.rs", by_path)

    def test_reporting_reserve_is_rechecked_after_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            clock = FakeClock()

            def enter_reserve() -> None:
                clock.now = 1_500.0

            options, dependencies, runner = workflow_fixture(
                directory, clock=clock, after_baseline=enter_reserve
            )
            record = run_workflow(options, dependencies)
            self.assertEqual(record.state, RunState.BUDGET_EXHAUSTED)
            self.assertFalse(
                any(label.startswith("mutation-") for *_, label in runner.calls)
            )
            self.assertTrue(
                all(
                    item.not_run_reason == "reporting_reserve"
                    for item in record.candidates
                )
            )

    def test_tool_absence_is_infrastructure_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            runner.run = mock.Mock(side_effect=FileNotFoundError("cargo"))
            record = run_workflow(options, dependencies)
            self.assertEqual(record.state, RunState.TOOL_UNAVAILABLE)
            self.assertTrue((options.output / "run.json").is_file())
            self.assertFalse(
                any(item.state is CandidateState.TIMEOUT for item in record.candidates)
            )

    def test_output_drain_failure_is_a_reported_command_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory,
                drain_label="cargo-mutants-version",
            )

            record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertIn("output drain", record.error)
            self.assertTrue((options.output / "run.json").is_file())

    def test_in_band_discovery_failures_mark_candidates_not_run(self) -> None:
        cases = (
            (
                "version command",
                {"fail_label": "cargo-mutants-version"},
                RunState.TOOL_UNAVAILABLE,
            ),
            (
                "unsupported version",
                {"version_output": "cargo-mutants 27.2.0\n"},
                RunState.TOOL_UNAVAILABLE,
            ),
            (
                "inventory command",
                {"fail_label": "inventory"},
                RunState.COMMAND_FAILED,
            ),
        )
        for name, runner_options, expected_state in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                options, dependencies, runner = workflow_fixture(
                    directory, **runner_options
                )

                discovered = [
                    Candidate("build.rs", "main", "build.rs:1: replace main"),
                    Candidate(
                        "crates/hoimin-core/src/machine.rs",
                        "a",
                        "machine.rs:1: replace a",
                    ),
                ]
                with mock.patch(
                    "tools.focused_mutation.discover_candidates",
                    return_value=discovered,
                ):
                    record = run_workflow(options, dependencies)

                self.assertEqual(record.state, expected_state)
                self.assertTrue(record.candidates)
                self.assertTrue(
                    all(
                        candidate.state is CandidateState.NOT_RUN
                        and candidate.not_run_reason == expected_state.value
                        for candidate in record.candidates
                    )
                )
                self.assertFalse(
                    any(
                        label.startswith(("baseline-", "mutation-"))
                        for *_, label in runner.calls
                    )
                )
                persisted = json.loads(
                    (options.output / "run.json").read_text(encoding="utf-8")
                )
                self.assertTrue(
                    all(
                        candidate["state"] == CandidateState.NOT_RUN.value
                        and candidate["not_run_reason"] == expected_state.value
                        for candidate in persisted["candidates"]
                    )
                )
                self.assertEqual(persisted["state"], expected_state.value)
                self.assertIn(
                    f"- `main` — {expected_state.value}",
                    (options.output / "report.md").read_text(encoding="utf-8"),
                )

    def test_malformed_inventory_is_command_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory, malformed_inventory=True
            )
            record = run_workflow(options, dependencies)
            self.assertEqual(record.state, RunState.COMMAND_FAILED)
            self.assertTrue((options.output / "run.json").is_file())

    def test_discovery_command_timeout_does_not_timeout_candidate(self) -> None:
        for label in ("cargo-mutants-version", "inventory"):
            with self.subTest(label=label), tempfile.TemporaryDirectory() as directory:
                options, dependencies, _ = workflow_fixture(
                    directory, timeout_label=label
                )
                record = run_workflow(options, dependencies)
                self.assertEqual(record.state, RunState.COMMAND_FAILED)
                self.assertFalse(
                    any(
                        item.state is CandidateState.TIMEOUT
                        for item in record.candidates
                    )
                )

    def test_git_probe_timeout_is_command_failure_without_candidate_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            dependencies = Dependencies(
                dependencies.monotonic,
                dependencies.utc_now,
                WorkflowProbe(options.repository, timeout_command="status"),
                dependencies.runner,
            )

            record = run_workflow(options, dependencies)

        self.assertEqual(record.state, RunState.COMMAND_FAILED)
        self.assertFalse(
            any(item.state is CandidateState.TIMEOUT for item in record.candidates)
        )

    def test_initial_repository_validation_uses_overall_budget_deadline(self) -> None:
        clock = mock.Mock(return_value=100.0)
        record = fixture_record(candidates=[], state=RunState.COMPLETED)
        with (
            mock.patch("tools.focused_mutation.time.monotonic", clock),
            mock.patch(
                "tools.focused_mutation.run_workflow",
                return_value=record,
            ) as workflow,
        ):
            self.assertEqual(main(["--budget", "3m", "--output", "/tmp/out"]), 0)

        self.assertEqual(workflow.call_args.kwargs["budget"].started, 100.0)

    def test_git_probe_uses_utf8_independently_of_the_windows_locale(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "output"
            output.mkdir()
            store = RunStore(output)
            store.commands.mkdir()
            probe = SubprocessProbe(root, CommandRunner(store))
            output_text = probe.text(
                [sys.executable, "-c", "print('日本語.py')"], 12.0
            )

        self.assertEqual(output_text, "日本語.py\n")

    def test_initial_repository_validation_timeout_exits_two(self) -> None:
        with (
            mock.patch("tools.focused_mutation.Path.cwd", return_value=Path("/")),
            self.assertRaises(SystemExit) as raised,
        ):
            main(["--budget", "3s", "--output", "/tmp/out"])

        self.assertEqual(raised.exception.code, 2)

    def test_baseline_timeout_is_baseline_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory, timeout_label="baseline-hoimin-core"
            )
            record = run_workflow(options, dependencies)
            self.assertEqual(record.state, RunState.BASELINE_FAILED)
            self.assertTrue(
                all(
                    item.not_run_reason == "baseline_failed"
                    for item in record.candidates
                )
            )

    def test_mutation_timeout_is_attributed_to_active_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory, timeout_label="mutation-0001"
            )
            record = run_workflow(options, dependencies)
            self.assertEqual(record.candidates[0].state, CandidateState.TIMEOUT)
            self.assertEqual(record.candidates[0].command_sequences, [4])
            self.assertNotEqual(record.candidates[1].state, CandidateState.TIMEOUT)

    def test_keyboard_interrupt_checkpoints_partial_run(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(
                directory, interrupt_label="mutation-0001"
            )
            record = run_workflow(options, dependencies)
            self.assertEqual(record.state, RunState.INTERRUPTED)
            self.assertTrue((options.output / "run.json").is_file())
            self.assertEqual(
                json.loads((options.output / "run.json").read_text())["state"],
                "interrupted",
            )

    def test_dirty_repository_metadata_is_persisted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory, dirty=True)
            record = run_workflow(options, dependencies)
            self.assertTrue(record.repository["dirty"])
            self.assertEqual(
                record.repository["dirty_paths"],
                ["crates/hoimin-core/src/machine.rs"],
            )

    def test_each_command_observes_previous_command_checkpoint(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            run_workflow(options, dependencies)
            self.assertEqual(
                runner.checkpoint_command_counts,
                list(range(len(runner.calls))),
            )

    def test_cli_preserves_repeated_file_and_symbol_selectors(self) -> None:
        arguments = _parser().parse_args(
            [
                "--output",
                "/tmp/out",
                "--file",
                "a.rs",
                "--file",
                "b.rs",
                "--symbol",
                "a",
                "--symbol",
                "b",
            ]
        )
        self.assertEqual(arguments.file, ["a.rs", "b.rs"])
        self.assertEqual(arguments.symbol, ["a", "b"])

    def test_cli_maps_run_states_to_exit_codes(self) -> None:
        expected = {
            RunState.COMPLETED: 0,
            RunState.BUDGET_EXHAUSTED: 3,
            RunState.DISK_LIMIT: 2,
            RunState.INTERRUPTED: 130,
            RunState.COMMAND_FAILED: 2,
            RunState.BASELINE_FAILED: 2,
            RunState.TOOL_UNAVAILABLE: 2,
        }
        repository_result = subprocess.CompletedProcess(
            ["git"], 0, stdout=str(Path.cwd()), stderr=""
        )
        for state, exit_code in expected.items():
            with self.subTest(state=state):
                record = fixture_record(candidates=[], state=state)
                with (
                    mock.patch(
                        "tools.focused_mutation.subprocess.run",
                        return_value=repository_result,
                    ),
                    mock.patch(
                        "tools.focused_mutation.run_workflow",
                        return_value=record,
                    ),
                ):
                    self.assertEqual(
                        main(["--output", "/tmp/focused-cli-test"]),
                        exit_code,
                    )

    def test_output_path_rejection_happens_before_commands(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, runner = workflow_fixture(directory)
            options = Options(
                options.repository,
                Path(directory) / "mutants.out-focused",
                options.budget_seconds,
                options.base,
                options.files,
                options.symbols,
                options.iterate,
                options.prior_inventory,
                options.disk_policy,
            )
            with self.assertRaisesRegex(ValueError, "mutants.out"):
                run_workflow(options, dependencies)
            self.assertEqual(runner.calls, [])

    def test_prior_inventory_comparison_is_recorded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            options, dependencies, _ = workflow_fixture(directory)
            prior = Path(directory) / "prior.json"
            prior.write_text(
                json.dumps({"candidates": [{}, {}, {}, {}]}),
                encoding="utf-8",
            )
            options = Options(
                options.repository,
                options.output,
                options.budget_seconds,
                options.base,
                options.files,
                options.symbols,
                options.iterate,
                prior,
                options.disk_policy,
            )
            record = run_workflow(options, dependencies)
            self.assertEqual(record.comparison["full_candidates"], 4)
            self.assertEqual(record.comparison["focused_candidates"], 2)
            self.assertEqual(record.comparison["reduction_ratio"], 0.5)

    def test_list_command_is_workspace_json_and_narrow_files(self) -> None:
        argv = build_list_command(
            Path("/repo"),
            ["crates/hoimin-core/src/machine.rs"],
        )
        self.assertEqual(
            argv,
            [
                "cargo",
                "mutants",
                "--workspace",
                "--list",
                "--json",
                "--manifest-path",
                str(Path("/repo") / "Cargo.toml"),
                "--file",
                "crates/hoimin-core/src/machine.rs",
            ],
        )

    def test_focused_command_uses_exact_anchored_mutant_name(self) -> None:
        self.assertIn(
            "output_directory",
            inspect.signature(build_mutation_command).parameters,
            "focused mutations must explicitly isolate cargo-mutants output",
        )
        candidate = Candidate(
            path="crates/hoimin-core/src/machine.rs",
            symbol="RunState::accept_completion",
            mutant_name=(
                "crates/hoimin-core/src/machine.rs:324: replace guard"
            ),
        )
        argv = build_mutation_command(
            Path("/repo"), Path("/evidence/cargo-mutants/0001"), candidate,
            iterate=False,
            jobs=1,
        )
        self.assertEqual(
            argv[argv.index("--output") + 1],
            str(Path("/evidence/cargo-mutants/0001")),
        )
        self.assertIn(
            (
                "^crates/hoimin\\-core/src/machine\\.rs:324:"
                "\\ replace\\ guard$"
            ),
            argv,
        )
        self.assertNotIn("--iterate", argv)
        self.assertEqual(argv[argv.index("--jobs") + 1], "1")

    def test_cli_commands_skip_depth_fixture_that_trips_the_outer_guard(
        self,
    ) -> None:
        candidate = Candidate(
            "crates/hoimin-cli/src/workspace/disk.rs",
            "measure_owned_tree",
            "crates/hoimin-cli/src/workspace/disk.rs:1: replace function",
        )
        expected_test_args = [
            "--",
            "--skip",
            "workspace::disk::tests::rejects_a_tree_deeper_than_the_bound",
        ]

        self.assertEqual(
            build_baseline_command(candidate)[-3:], expected_test_args
        )
        self.assertEqual(
            build_mutation_command(
                Path("/repo"),
                Path("/evidence/run"),
                candidate,
                iterate=False,
                jobs=1,
            )[-3:],
            expected_test_args,
        )

    def test_focused_command_enables_reuse_only_when_requested(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "name")
        self.assertEqual(
            build_mutation_command(
                Path("/repo"), Path("/evidence/run"), candidate, iterate=True, jobs=4
            )[-1],
            "--iterate",
        )

    def test_focused_command_requires_inventory_name(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", None)
        with self.assertRaisesRegex(ValueError, "mutant name"):
            build_mutation_command(
                Path("/repo"),
                Path("/evidence/run"),
                candidate,
                iterate=False,
                jobs=1,
            )

    def test_baseline_is_scoped_to_the_candidate_crate(self) -> None:
        core = Candidate(
            "crates/hoimin-core/src/machine.rs", "transition", None
        )
        cli = Candidate(
            "crates/hoimin-cli/src/process/mod.rs", "cancel", None
        )
        self.assertEqual(
            build_baseline_command(core),
            ["cargo", "test", "-p", "hoimin-core"],
        )
        self.assertEqual(
            build_baseline_command(cli),
            [
                "cargo",
                "test",
                "-p",
                "hoimin-cli",
                "--",
                "--skip",
                "workspace::disk::tests::rejects_a_tree_deeper_than_the_bound",
            ],
        )

    def test_baseline_rejects_path_outside_workspace_member(self) -> None:
        with self.assertRaisesRegex(ValueError, "workspace member"):
            build_baseline_command(Candidate("src/lib.rs", "f", None))

    def test_real_27_1_0_list_shape_becomes_candidates(self) -> None:
        self.assertEqual(
            parse_list_json(LIST_JSON_27_1_0),
            [
                Candidate(
                    path="src/lib.rs",
                    symbol="add",
                    mutant_name=(
                        "src/lib.rs:2:5: replace add -> u64 with 0"
                    ),
                )
            ],
        )

    def test_list_parser_ignores_non_function_mutants(self) -> None:
        inventory = json.loads(LIST_JSON_27_1_0)
        inventory.insert(
            0,
            {
                "file": "src/lib.rs",
                "function": None,
                "name": "src/lib.rs:1:20: replace * with +",
            },
        )

        try:
            candidates = parse_list_json(json.dumps(inventory))
        except ValueError as error:
            self.fail(f"valid non-function mutant was rejected: {error}")

        self.assertEqual(
            candidates,
            [
                Candidate(
                    path="src/lib.rs",
                    symbol="add",
                    mutant_name=(
                        "src/lib.rs:2:5: replace add -> u64 with 0"
                    ),
                )
            ],
        )

    def test_list_parser_rejects_incomplete_entries(self) -> None:
        with self.assertRaisesRegex(ValueError, "name"):
            parse_list_json('[{"file": "src/lib.rs"}]')

    def test_list_parser_rejects_candidate_path_that_breaks_markdown(self) -> None:
        value = json.loads(LIST_JSON_27_1_0)
        value[0]["file"] = "src/unsafe\nname.rs"

        with self.assertRaisesRegex(ValueError, "Markdown"):
            parse_list_json(json.dumps(value))

    def test_supported_cargo_mutants_version_is_accepted(self) -> None:
        self.assertEqual(
            validate_cargo_mutants_version("cargo-mutants 27.1.0\n"),
            (None, None),
        )

    def test_other_cargo_mutants_minor_is_tool_unavailable(self) -> None:
        state, message = validate_cargo_mutants_version(
            "cargo-mutants 27.2.0\n"
        )
        self.assertEqual(state, RunState.TOOL_UNAVAILABLE)
        self.assertIn("27.1.0", message or "")

    def test_real_workspace_list_is_safe_from_per_candidate_directory(
        self,
    ) -> None:
        if shutil.which("cargo-mutants") is None:
            self.skipTest("cargo-mutants 27.1.0 is not installed")

        repository = Path(__file__).resolve().parents[1]
        before = self._root_mutants_artifacts(repository)
        version = subprocess.run(
            ["cargo", "mutants", "--version"],
            cwd=repository,
            check=True,
            capture_output=True,
            text=True,
        )
        state, message = validate_cargo_mutants_version(version.stdout)
        self.assertIsNone(state, message)

        with tempfile.TemporaryDirectory() as tmp:
            listed = subprocess.run(
                build_list_command(
                    repository,
                    ["crates/hoimin-core/src/machine.rs"],
                ),
                cwd=tmp,
                check=True,
                capture_output=True,
                text=True,
            )
        self.assertIsInstance(json.loads(listed.stdout), list)
        self.assertEqual(
            self._root_mutants_artifacts(repository),
            before,
            "cargo-mutants --list altered repository-root mutants.out*",
        )

    def test_exact_results_map_to_candidate_states(self) -> None:
        expected = {
            "caught.txt": (CandidateState.KILLED, "CaughtMutant"),
            "missed.txt": (CandidateState.SURVIVED, "MissedMutant"),
            "timeout.txt": (CandidateState.TIMEOUT, "Timeout"),
            "unviable.txt": (CandidateState.UNVIABLE, "Unviable"),
        }
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        for filename, (state, summary) in expected.items():
            with self.subTest(filename=filename), tempfile.TemporaryDirectory() as tmp:
                output = Path(tmp) / "mutants.out"
                output.mkdir()
                (output / filename).write_text(
                    "exact mutant\n", encoding="utf-8"
                )
                write_outcomes_json(output, "exact mutant", summary)
                self.assertEqual(
                    classify_mutation_output(
                        Path(tmp), command_record(), candidate
                    ),
                    state,
                )

    def test_other_mutant_result_is_not_accepted(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            (output / "caught.txt").write_text(
                "exact mutant plus suffix\n", encoding="utf-8"
            )
            self.assertEqual(
                classify_mutation_output(
                    Path(tmp), command_record(), candidate
                ),
                CandidateState.ERROR,
            )

    def test_duplicate_exact_result_is_error(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            for filename in ("caught.txt", "timeout.txt"):
                (output / filename).write_text(
                    "exact mutant\n", encoding="utf-8"
                )
            self.assertEqual(
                classify_mutation_output(
                    Path(tmp), command_record(), candidate
                ),
                CandidateState.ERROR,
            )

    def test_duplicate_exact_result_in_one_category_is_error(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            (output / "caught.txt").write_text(
                "exact mutant\nexact mutant\n", encoding="utf-8"
            )
            write_outcomes_json(output, "exact mutant", "CaughtMutant")
            self.assertEqual(
                classify_mutation_output(
                    Path(tmp), command_record(), candidate
                ),
                CandidateState.ERROR,
            )

    def test_disjoint_outcome_scalars_do_not_credit_mutant(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            (output / "caught.txt").write_text(
                "exact mutant\n", encoding="utf-8"
            )
            (output / "outcomes.json").write_text(
                json.dumps(
                    {
                        "total_mutants": 1,
                        "outcomes": [
                            {"scenario": "UnmutatedBaseline", "summary": "Success"},
                            {"mutant": "exact mutant", "summary": "MissedMutant"},
                            {"mutant": "different", "summary": "CaughtMutant"},
                        ],
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(
                classify_mutation_output(
                    Path(tmp), command_record(), candidate
                ),
                CandidateState.ERROR,
            )

    def test_total_mutants_requires_exact_integer_one(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        for malformed in (True, 1.0):
            with self.subTest(total_mutants=malformed), tempfile.TemporaryDirectory() as tmp:
                output = Path(tmp) / "mutants.out"
                output.mkdir()
                (output / "caught.txt").write_text(
                    "exact mutant\n", encoding="utf-8"
                )
                write_outcomes_json(output, "exact mutant", "CaughtMutant")
                payload = json.loads((output / "outcomes.json").read_text())
                payload["total_mutants"] = malformed
                (output / "outcomes.json").write_text(
                    json.dumps(payload), encoding="utf-8"
                )
                self.assertEqual(
                    classify_mutation_output(
                        Path(tmp), command_record(), candidate
                    ),
                    CandidateState.ERROR,
                )

    def test_extra_mutant_outcome_does_not_credit_mutant(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            (output / "caught.txt").write_text(
                "exact mutant\nother mutant\n", encoding="utf-8"
            )
            (output / "outcomes.json").write_text(
                json.dumps(
                    {
                        "total_mutants": 1,
                        "outcomes": [
                            {"scenario": "UnmutatedBaseline", "summary": "Success"},
                            {"mutant": "exact mutant", "summary": "CaughtMutant"},
                            {"mutant": "other mutant", "summary": "CaughtMutant"},
                        ],
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(
                classify_mutation_output(
                    Path(tmp), command_record(), candidate
                ),
                CandidateState.ERROR,
            )

    def test_missing_results_are_error(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(
                classify_mutation_output(
                    Path(tmp), command_record(), candidate
                ),
                CandidateState.ERROR,
            )

    def test_failed_command_with_no_exact_result_is_error(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            self.assertEqual(
                classify_mutation_output(
                    Path(tmp), command_record(exit_code=1), candidate
                ),
                CandidateState.ERROR,
            )

    def test_incomplete_command_does_not_accept_stale_result(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        record = command_record()
        record.exit_code = None
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            (output / "caught.txt").write_text(
                "exact mutant\n", encoding="utf-8"
            )
            self.assertEqual(
                classify_mutation_output(Path(tmp), record, candidate),
                CandidateState.ERROR,
            )

    def test_timed_out_command_does_not_accept_stale_result(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        record = command_record(exit_code=-15)
        record.timed_out = True
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "mutants.out"
            output.mkdir()
            (output / "caught.txt").write_text(
                "exact mutant\n", encoding="utf-8"
            )
            self.assertEqual(
                classify_mutation_output(Path(tmp), record, candidate),
                CandidateState.ERROR,
            )

    @staticmethod
    def _root_mutants_artifacts(
        repository: Path,
    ) -> dict[str, tuple[int, int, bytes]]:
        snapshot: dict[str, tuple[int, int, bytes]] = {}
        for root in repository.glob("mutants.out*"):
            paths = [root, *root.rglob("*")] if root.is_dir() else [root]
            for path in paths:
                snapshot[str(path.relative_to(repository))] = (
                    path.stat().st_mtime_ns,
                    path.stat().st_size,
                    path.read_bytes() if path.is_file() else b"",
                )
        return snapshot


if __name__ == "__main__":
    unittest.main()
