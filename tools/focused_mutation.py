#!/usr/bin/env python3
from __future__ import annotations

import argparse
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass, field
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import time
import os
import uuid
import tempfile
import signal
import threading
from contextlib import contextmanager
from typing import Any, cast

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from tools.focused_mutation_support.budget import RunBudget, parse_duration
from tools.focused_mutation_support.discovery import (
    CommandProbe,
    _symbol_name,
    discover_candidates,
    discover_repository,
)
from tools.focused_mutation_support.disk import (
    CleanupOutcome,
    DiskFailure,
    DiskGuard,
    DiskLifecycle,
    DiskLifecycleEvent,
    DiskObservation,
    DiskPolicy,
    DiskRootId,
    DiskStopReason,
    MeterRoot,
    apply_disk_lifecycle_event,
    evaluate_disk_policy,
    parse_byte_size,
)
from tools.focused_mutation_support.lease import (
    JanitorDiagnostic,
    ManagedScratch,
    ScratchCleanupRecord,
    ScratchCleanupStatus,
    validate_reported_path,
)
from tools.focused_mutation_support.model import (
    Candidate,
    CandidateState,
    CommandRecord,
    RunRecord,
    RunState,
)
from tools.focused_mutation_support.mutation import (
    build_baseline_command,
    build_list_command,
    build_mutation_command,
    classify_mutation_output,
    parse_list_json,
    read_bounded_regular,
    read_bounded_regular_json,
    read_bounded_regular_tail,
    validate_cargo_mutants_version,
)
from tools.focused_mutation_support.ranking import rank_candidates
from tools.focused_mutation_support.runner import (
    CommandDrainFailed,
    CommandDiskStopped,
    CommandInterrupted,
    CommandLifecycleFailed,
    CommandRunner,
    CommandTimedOut,
)
from tools.focused_mutation_support.store import CommandPaths, OwnedOutput, RunStore


@dataclass(frozen=True)
class Options:
    repository: Path
    output: Path
    budget_seconds: float
    base: str
    files: tuple[str, ...]
    symbols: tuple[str, ...]
    iterate: bool
    prior_inventory: Path | None
    disk_policy: DiskPolicy = field(default_factory=DiskPolicy)


@dataclass(frozen=True)
class Dependencies:
    monotonic: Callable[[], float]
    utc_now: Callable[[], datetime]
    probe: CommandProbe
    runner: CommandRunner
    restore_signal_handlers: Callable[[], None] | None = None


@dataclass(frozen=True)
class WorkflowRuntime:
    scratch: ManagedScratch
    inventory_dir: Path
    store: RunStore
    guard: DiskGuard
    capacity_root: Path
    capacity_exact: Path
    command_environment: dict[str, str]


class _InitialDiskStop(RuntimeError):
    def __init__(self, failure: DiskFailure) -> None:
        super().__init__(failure.code)
        self.failure = failure


def _bounded_detail(value: str, capacity: int = 4 * 1024) -> str:
    encoded = value.encode("utf-8", errors="replace")
    if len(encoded) <= capacity:
        return encoded.decode("utf-8")
    suffix = b" [truncated]"
    return (
        encoded[: capacity - len(suffix)].decode("utf-8", errors="ignore")
        + suffix.decode("ascii")
    )


def _cli_error_detail(error: BaseException, capacity: int = 20 * 1024) -> str:
    parts = [str(error)]
    notes = getattr(error, "__notes__", ())
    for note in notes:
        parts.append(f"secondary: {note}")
    return _bounded_detail("\n".join(parts), capacity)


def _append_report_error(record: RunRecord, details: Sequence[str]) -> None:
    bounded = list(
        dict.fromkeys(_bounded_detail(item) for item in details if item)
    )
    if not bounded:
        return
    detail = "; ".join(bounded)
    record.report_error = (
        f"{record.report_error}; {detail}" if record.report_error else detail
    )
    _record_secondary_error(
        record, "report", "report.delivery.failed", detail
    )
    if record.state is RunState.COMPLETED:
        record.state = RunState.REPORT_FAILED
        record.error = detail


def _record_secondary_error(
    record: RunRecord, kind: str, code: str, message: str
) -> None:
    evidence: dict[str, object] = {
        "kind": kind,
        "code": code,
        "message": _bounded_detail(message),
    }
    if evidence not in record.secondary_errors and len(record.secondary_errors) < 256:
        record.secondary_errors.append(evidence)


def _disk_observation_evidence(
    observation: DiskObservation | None,
) -> dict[str, object] | None:
    if observation is None:
        return None
    return {
        "owned_bytes": observation.owned_bytes,
        "available_bytes": observation.available_bytes,
        "measured_in_seconds": observation.measured_in_seconds,
        "conservative_entries": observation.conservative_entries,
        "filesystem_available_bytes": observation.filesystem_available_bytes,
        "root_owned_bytes": observation.root_owned_bytes,
    }


def _record_disk_failure(record: RunRecord, failure: DiskFailure) -> None:
    failure_evidence: dict[str, object] = {
        "code": failure.code,
        "reason": failure.reason.value,
        "message": failure.message,
        "observation": _disk_observation_evidence(failure.observation),
    }
    if record.disk_stop is None:
        record.disk_stop = failure_evidence
    elif failure_evidence != record.disk_stop:
        later_evidence = {"kind": "disk", **failure_evidence}
        if (
            later_evidence not in record.secondary_errors
            and len(record.secondary_errors) < 256
        ):
            record.secondary_errors.append(later_evidence)
    for secondary in failure.secondary:
        evidence: dict[str, object] = {
            "kind": "disk",
            "code": (
                secondary.code
                if secondary.code is not None
                else (
                    secondary.reason.code
                    if secondary.reason is not None
                    else "disk.secondary"
                )
            ),
            "reason": (
                secondary.reason.value
                if secondary.reason is not None
                else None
            ),
            "message": (
                secondary.message
                if secondary.message is not None
                else (
                    secondary.reason.value
                    if secondary.reason is not None
                    else "secondary disk evidence"
                )
            ),
            "observation": _disk_observation_evidence(secondary.observation),
        }
        if (
            evidence not in record.secondary_errors
            and len(record.secondary_errors) < 256
        ):
            record.secondary_errors.append(evidence)


def _record_disk_failures_for_observation(
    record: RunRecord,
    policy: DiskPolicy,
    observation: DiskObservation | None,
    *failures: DiskFailure | None,
) -> DiskFailure | None:
    """Persist every distinct failure while preserving the first stop reason."""
    ordered = [failure for failure in failures if failure is not None]
    derived = (
        evaluate_disk_policy(policy, observation)
        if observation is not None
        else None
    )
    if derived is not None:
        ordered.append(derived)
    for failure in ordered:
        _record_disk_failure(record, failure)
    return ordered[0] if ordered else None


def _prepare_runtime(
    options: Options,
    dependencies: Dependencies,
    record: RunRecord,
    owned_output: OwnedOutput,
    run_id: str,
) -> WorkflowRuntime:
    scratch: ManagedScratch | None = None
    guard: DiskGuard | None = None
    store: RunStore | None = None
    try:
        stale_cleanup: list[ScratchCleanupRecord] = []
        stale_cleanup_diagnostics: list[JanitorDiagnostic] = []
        scratch = ManagedScratch.create(
            options.disk_policy.scratch_root,
            run_id=run_id,
            stale_cleanup=stale_cleanup,
            stale_diagnostics=stale_cleanup_diagnostics,
        )
        command_root = scratch.create_child("commands")
        inventory_dir = scratch.create_child("inventory")
        cargo_target = scratch.create_child("cargo-target")
        process_tmp = scratch.create_child("tmp")
        store = RunStore(
            owned_output,
            command_root=(
                command_root
                if isinstance(dependencies.runner, CommandRunner)
                else None
            ),
        )
        if isinstance(dependencies.runner, CommandRunner):
            dependencies.runner.set_store(store)
        cargo_home = Path(
            os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))
        ).expanduser()
        capacity_exact = Path(os.path.abspath(cargo_home))
        if capacity_exact.exists():
            capacity_exact = capacity_exact.resolve(strict=True)
        nearest = capacity_exact
        while not nearest.exists() and nearest != nearest.parent:
            nearest = nearest.parent
        capacity_root = nearest.resolve(strict=True)
        if not capacity_root.is_dir():
            raise ValueError(f"Cargo home ancestor is not a directory: {capacity_root}")
        guard = DiskGuard(
            options.disk_policy,
            [
                MeterRoot(scratch.path, enforcement="owned:scratch"),
                MeterRoot(options.output, enforcement="owned:output"),
                MeterRoot(
                    capacity_root,
                    charge_owned_bytes=False,
                    enforcement="capacity_only:cargo_home",
                    exact_path=capacity_exact,
                ),
            ],
            monotonic=dependencies.monotonic,
            heartbeat=scratch.refresh_heartbeat,
        )
        guard.start()

        def report_sample() -> tuple[
            DiskFailure | None, DiskObservation | None
        ]:
            failure = guard.sample()
            observation = guard.observations[-1] if guard.observations else None
            return failure, observation

        record.disk_policy = {
            "max_disk_bytes": options.disk_policy.max_disk_bytes,
            "min_free_bytes": options.disk_policy.min_free_bytes,
            "sample_interval_seconds": options.disk_policy.sample_interval_seconds,
        }
        record.disk_enforcement = [item.enforcement for item in guard.roots]
        record.jobs = options.disk_policy.jobs
        record.scratch = {"path": str(scratch.path), "run_id": scratch.run_id}
        record.output_recovery = {
            "removed_temporary_count": owned_output.recovered_temporary_count,
            "remaining_temporary_names": [],
        }
        record.stale_cleanup = [
            {
                "status": item.status.value,
                "examined_entries": item.examined_entries,
                "removed_entries": item.removed_entries,
                "details": [_bounded_detail(detail) for detail in item.details],
                "omitted_detail_count": item.omitted_detail_count,
                "remaining_root": item.remaining_root,
            }
            for item in stale_cleanup
        ]
        record.stale_cleanup_omitted_count = sum(
            item.omitted_detail_count for item in stale_cleanup
        )
        record.stale_cleanup_diagnostics = [
            _bounded_detail(detail)
            for item in stale_cleanup_diagnostics
            for detail in item.details
        ]
        record.stale_cleanup_diagnostics_omitted_count = sum(
            item.omitted_detail_count for item in stale_cleanup_diagnostics
        )
        store.configure_report_guard(
            options.disk_policy,
            report_sample,
            scratch.report_boundary,
        )
        if guard.failure is None:
            store.initialize(record)
        return WorkflowRuntime(
            scratch=scratch,
            inventory_dir=inventory_dir,
            store=store,
            guard=guard,
            capacity_root=capacity_root,
            capacity_exact=capacity_exact,
            command_environment={
                "TMPDIR": str(process_tmp),
                "TMP": str(process_tmp),
                "TEMP": str(process_tmp),
                "CARGO_TARGET_DIR": str(cargo_target),
                "CARGO_INCREMENTAL": "0",
            },
        )
    except BaseException as primary_error:
        joined = True
        rollback_cleanup: ScratchCleanupRecord | None = None
        if guard is not None:
            joined = guard.stop_and_join(timeout=5.0)
        store_close_errors = (
            store.close_command_root() if store is not None else ()
        )
        scratch_cleanup_safe = joined and not store_close_errors
        if scratch is not None:
            try:
                if scratch_cleanup_safe:
                    scratch.mark_cleanup_ready()
                    rollback_cleanup = scratch.cleanup()
                else:
                    rollback_cleanup = scratch.defer(
                        "setup rollback could not prove all scratch users closed"
                    )
            except OSError as cleanup_error:
                rollback_cleanup = ScratchCleanupRecord(
                    ScratchCleanupStatus.FAILED,
                    0,
                    0,
                    (f"setup rollback cleanup failed: {cleanup_error}",),
                    str(scratch.path),
                )
        scratch_close_errors = (
            scratch.close_capabilities()
            if scratch is not None and scratch_cleanup_safe
            else ()
        )
        output_close_errors = owned_output.close(
            remove_marker=scratch_cleanup_safe
        )
        if rollback_cleanup is not None and (
            rollback_cleanup.status is not ScratchCleanupStatus.CLEAN
        ):
            primary_error.add_note(
                "setup rollback left managed scratch: "
                f"status={rollback_cleanup.status.value}; "
                f"remaining_root={rollback_cleanup.remaining_root!r}; "
                f"details={'; '.join(rollback_cleanup.details)}"
            )
        elif rollback_cleanup is not None and rollback_cleanup.details:
            primary_error.add_note(
                "setup rollback cleanup secondary: "
                + "; ".join(rollback_cleanup.details)
            )
        if output_close_errors:
            primary_error.add_note(
                "setup rollback output close errors: "
                + "; ".join(output_close_errors)
            )
        if scratch_close_errors:
            primary_error.add_note(
                "setup rollback scratch close errors: "
                + "; ".join(scratch_close_errors)
            )
        if store_close_errors:
            primary_error.add_note(
                "setup rollback command spool close errors: "
                + "; ".join(store_close_errors)
            )
        raise


class SubprocessProbe:
    def __init__(self, cwd: Path, runner: CommandRunner) -> None:
        self.cwd = cwd
        self.runner = runner
        self.sequence = 0
        self._dispatch: Callable[..., CommandRecord] | None = None

    def bind_dispatch(
        self, dispatch: Callable[..., CommandRecord]
    ) -> None:
        if self._dispatch is not None:
            raise RuntimeError("repository probe dispatch is already bound")
        self._dispatch = dispatch

    def text(self, argv: list[str], timeout: float) -> str:
        self.sequence += 1
        if self._dispatch is None:
            record = self.runner.run(
                argv,
                cwd=self.cwd,
                timeout=timeout,
                label=f"probe-{self.sequence:04d}",
                max_log_bytes=64 * 1024,
            )
        else:
            record = self._dispatch(
                argv,
                self.cwd,
                timeout,
                f"probe-{self.sequence:04d}",
                max_log_bytes=64 * 1024,
            )
        primary_error: BaseException | None = None
        try:
            if record.exit_code != 0:
                raise subprocess.CalledProcessError(
                    record.exit_code if record.exit_code is not None else -1,
                    argv,
                )
            if record.stdout_truncated or record.stderr_truncated:
                raise ValueError(
                    f"repository probe output was truncated: {argv[0]}"
                )
            probe_stdout = _read_command_stream(
                record, "stdout", 32 * 1024
            )
            assert isinstance(probe_stdout, bytes)
            return probe_stdout.decode("utf-8", errors="strict")
        except BaseException as error:
            primary_error = error
            raise
        finally:
            if not _discard_command_spool(record):
                detail = "; ".join(record.cleanup_errors) or (
                    "command spool cleanup failed"
                )
                if primary_error is not None:
                    primary_error.add_note(detail)
                else:
                    raise OSError(detail)


def _read_stdout(command: CommandRecord) -> str:
    if command.stdout_truncated or command.stderr_truncated:
        raise ValueError(f"command output was truncated: {command.label}")
    stdout = _read_command_stream(command, "stdout", 8 * 1024**2)
    assert isinstance(stdout, bytes)
    return stdout.decode("utf-8", errors="strict")


def _read_command_stream(
    command: CommandRecord,
    stream_name: str,
    capacity: int,
    *,
    tail: bool = False,
) -> bytes | tuple[bytes, int]:
    if isinstance(command._spool, CommandPaths):
        value, observed = command._spool.read(
            stream_name, capacity, tail=tail
        )
        return (value, observed) if tail else value
    path = Path(
        command.stdout_path if stream_name == "stdout" else command.stderr_path
    )
    if tail:
        return read_bounded_regular_tail(path, capacity)
    return read_bounded_regular(path, capacity)


def _discard_command_spool(command: CommandRecord) -> bool:
    if isinstance(command._spool, CommandPaths):
        errors = command._spool.discard()
        command.cleanup_errors.extend(errors)
        return not errors
    clean = True
    for value in (command.stdout_path, command.stderr_path):
        if not value:
            continue
        path = Path(value)
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        except OSError as error:
            clean = False
            command.cleanup_errors.append(
                f"command spool cleanup failed: {type(error).__name__}: {error}"
            )
        if path.exists():
            clean = False
            command.cleanup_errors.append(
                f"command spool cleanup failed: path remains: {path}"
            )
    return clean


def _record_command_spool_cleanup(
    record: RunRecord,
    command: CommandRecord,
) -> str | None:
    if _discard_command_spool(command):
        return None
    detail = "; ".join(command.cleanup_errors) or "unknown cleanup error"
    message = f"{command.label}: {detail}"
    _record_secondary_error(
        record,
        "cleanup",
        "command.spool.cleanup.failed",
        message,
    )
    return message


def _mark_pending(record: RunRecord, reason: str) -> None:
    for candidate in record.candidates:
        if candidate.state is CandidateState.PENDING:
            candidate.state = CandidateState.NOT_RUN
            candidate.not_run_reason = reason


def _stop_before_mutations(
    record: RunRecord, state: RunState, error: str | None
) -> None:
    record.state = state
    record.error = error
    _mark_pending(record, state.value)


def _candidate_package(path: str) -> str | None:
    parts = Path(path).parts
    if len(parts) < 3 or parts[0] != "crates":
        return None
    return parts[1]


def _attach_candidate_diagnostic(
    candidate: Candidate,
    command: CommandRecord,
    allowance: int,
) -> int:
    capacity = min(16 * 1024, allowance)
    stderr_capacity = (capacity + 1) // 2
    try:
        stderr, stderr_observed = _read_command_stream(
            command, "stderr", stderr_capacity, tail=True
        )
    except FileNotFoundError:
        stderr, stderr_observed = b"", command.stderr_observed_bytes
    assert isinstance(stderr, bytes)
    assert isinstance(stderr_observed, int)
    try:
        stdout, stdout_observed = _read_command_stream(
            command, "stdout", capacity - len(stderr), tail=True
        )
    except FileNotFoundError:
        stdout, stdout_observed = b"", command.stdout_observed_bytes
    assert isinstance(stdout, bytes)
    assert isinstance(stdout_observed, int)
    body = stderr + stdout
    diagnostic = body.decode("utf-8", errors="replace").encode("utf-8")
    if len(diagnostic) > capacity:
        diagnostic = diagnostic[:capacity]
        diagnostic = diagnostic.decode("utf-8", errors="ignore").encode("utf-8")
    candidate.diagnostic = diagnostic.decode("utf-8") or None
    observed = max(stderr_observed, command.stderr_observed_bytes) + max(
        stdout_observed, command.stdout_observed_bytes
    )
    candidate.diagnostic_observed_bytes = observed
    candidate.diagnostic_retained_bytes = len(diagnostic)
    candidate.diagnostic_truncated = len(diagnostic) < observed
    return allowance - len(diagnostic)


def _mark_pending_package(
    record: RunRecord, package: str, reason: str
) -> None:
    for candidate in record.candidates:
        if (
            candidate.state is CandidateState.PENDING
            and _candidate_package(candidate.path) == package
        ):
            candidate.state = CandidateState.NOT_RUN
            candidate.not_run_reason = reason


def _comparison(path: Path | None, focused_count: int) -> dict[str, object] | None:
    if path is None:
        return None
    value = read_bounded_regular_json(path, 8 * 1024**2)
    if not isinstance(value, dict) or not isinstance(value.get("candidates"), list):
        raise ValueError("prior inventory must contain a candidates array")
    full = len(value["candidates"])
    if full > 1_000:
        raise ValueError("prior inventory candidates exceed 1000")
    return {
        "focused_candidates": focused_count,
        "full_candidates": full,
        "reduction_ratio": 0.0 if full == 0 else 1.0 - focused_count / full,
    }


def run_workflow(
    options: Options,
    dependencies: Dependencies,
    *,
    budget: RunBudget | None = None,
) -> RunRecord:
    if budget is None:
        budget = RunBudget.start(options.budget_seconds, dependencies.monotonic())
    started = budget.started
    record = RunRecord.new(options.budget_seconds)
    record.started_at = dependencies.utc_now().isoformat()
    validate_reported_path(options.repository)
    if options.output.name.startswith("mutants.out"):
        raise ValueError("output path must not use the mutants.out prefix")
    managed_root = options.disk_policy.scratch_root / "hoimin-focused-v1"
    rust_managed_root = options.disk_policy.scratch_root / "hoimin-workspaces-v1"
    resolved_output = options.output.resolve()
    if any(
        resolved_output == root
        or resolved_output.is_relative_to(root)
        or root.is_relative_to(resolved_output)
        for root in (managed_root, rust_managed_root)
    ):
        raise ValueError("output path must not overlap managed scratch")
    run_id = str(uuid.uuid4())
    owned_output = OwnedOutput.create(
        options.output,
        run_id,
        min_free_bytes=options.disk_policy.min_free_bytes,
    )
    try:
        if owned_output.available_bytes() <= options.disk_policy.min_free_bytes:
            raise ValueError("filesystem reserve reached after output recovery")
    except BaseException as primary_error:
        close_errors = owned_output.close(remove_marker=True)
        if close_errors:
            primary_error.add_note(
                "output ownership rollback errors: " + "; ".join(close_errors)
            )
        raise
    runtime = _prepare_runtime(
        options, dependencies, record, owned_output, run_id
    )
    scratch = runtime.scratch
    inventory_dir = runtime.inventory_dir
    store = runtime.store
    guard = runtime.guard
    capacity_root = runtime.capacity_root
    capacity_exact = runtime.capacity_exact
    command_environment = runtime.command_environment
    disk_lifecycle = DiskLifecycle([DiskRootId.EXECUTION])
    record.disk_policy = {
        "max_disk_bytes": options.disk_policy.max_disk_bytes,
        "min_free_bytes": options.disk_policy.min_free_bytes,
        "sample_interval_seconds": options.disk_policy.sample_interval_seconds,
    }
    record.disk_summary = {
        "start_free_bytes": {},
        "end_free_bytes": {},
        "free_byte_delta": {},
        "peak_owned_bytes": 0,
        "minimum_free_bytes": None,
        "sample_count": 0,
        "maximum_measurement_seconds": 0.0,
        "absence_verified_removed_logical_bytes": None,
    }
    record.disk_enforcement = [item.enforcement for item in guard.roots]
    record.jobs = options.disk_policy.jobs
    record.scratch = {
        "path": str(scratch.path),
        "run_id": scratch.run_id,
    }
    record.output_recovery = {
        "removed_temporary_count": owned_output.recovered_temporary_count,
        "remaining_temporary_names": [],
    }

    command_stage = "discovery"
    active_candidate = None
    active_package = None
    diagnostic_allowance = options.disk_policy.max_run_diagnostic_bytes

    def checkpoint() -> None:
        if (
            isinstance(dependencies.runner, CommandRunner)
            and dependencies.runner.interrupted
        ):
            raise KeyboardInterrupt
        record.elapsed_seconds = dependencies.monotonic() - started
        store.checkpoint(record)

    def run_command(
        argv: Sequence[str],
        cwd: Path,
        timeout: float,
        label: str,
        *,
        max_log_bytes: int | None = None,
    ):
        if isinstance(dependencies.runner, CommandRunner):
            preflight_failure = guard.sample()
            if preflight_failure is not None:
                event = (
                    DiskLifecycleEvent.observation(
                        options.disk_policy, preflight_failure.observation
                    )
                    if preflight_failure.observation is not None
                    else DiskLifecycleEvent.measurement_failed(
                        preflight_failure.message
                    )
                )
                apply_disk_lifecycle_event(disk_lifecycle, event)
            if disk_lifecycle.stop is not None:
                try:
                    return dependencies.runner.run(
                        argv,
                        cwd,
                        timeout,
                        label,
                        environment=command_environment,
                        disk_guard=guard,
                        max_log_bytes=(
                            options.disk_policy.max_log_bytes
                            if max_log_bytes is None
                            else max_log_bytes
                        ),
                    )
                except CommandDiskStopped as error:
                    record.commands.append(error.record)
                    raise
        scratch.note_dispatch()
        if not apply_disk_lifecycle_event(
            disk_lifecycle, DiskLifecycleEvent.dispatch_requested()
        ):
            raise RuntimeError("disk lifecycle rejected command dispatch")
        try:
            if isinstance(dependencies.runner, CommandRunner):
                command = dependencies.runner.run(
                    argv,
                    cwd,
                    timeout,
                    label,
                    environment=command_environment,
                    disk_guard=guard,
                    max_log_bytes=(
                        options.disk_policy.max_log_bytes
                        if max_log_bytes is None
                        else max_log_bytes
                    ),
                )
            else:
                command = dependencies.runner.run(argv, cwd, timeout, label)
        except (
            CommandTimedOut,
            CommandInterrupted,
            CommandDiskStopped,
            CommandDrainFailed,
            CommandLifecycleFailed,
        ) as error:
            if isinstance(error, CommandDiskStopped):
                event = (
                    DiskLifecycleEvent.observation(
                        options.disk_policy, error.failure.observation
                    )
                    if error.failure.observation is not None
                    else DiskLifecycleEvent.measurement_failed(
                        error.failure.message
                    )
                )
                apply_disk_lifecycle_event(disk_lifecycle, event)
            record.commands.append(error.record)
            try:
                checkpoint()
            except OSError:
                # The command's timeout/interruption/disk-stop is primary.
                # Finalization will attempt bounded evidence delivery again.
                pass
            raise
        record.commands.append(command)
        checkpoint()
        return command

    try:
        if guard.failure is not None:
            raise _InitialDiskStop(guard.failure)
        if isinstance(dependencies.probe, SubprocessProbe):
            dependencies.probe.bind_dispatch(run_command)
        discovery_timeout = lambda: budget.discovery_timeout(
            dependencies.monotonic()
        )
        snapshot = discover_repository(
            options.repository,
            options.base,
            dependencies.probe,
            discovery_timeout,
        )
        record.repository = {
            "root": str(snapshot.root),
            "head": snapshot.head,
            "branch": snapshot.branch,
            "dirty": bool(snapshot.dirty_paths),
            "dirty_paths": list(snapshot.dirty_paths),
            "base": options.base,
        }
        checkpoint()
        selected = discover_candidates(
            snapshot,
            options.files,
            options.symbols,
            dependencies.probe,
            discovery_timeout,
            max_candidates=options.disk_policy.max_selector_count,
        )
        selected = rank_candidates(
            selected, snapshot, options.files, options.symbols
        )
        record.candidates = selected
        checkpoint()
        if not selected:
            raise ValueError("no focused mutation candidates were selected")

        command_stage = "discovery"
        version = run_command(
            ["cargo", "mutants", "--version"],
            inventory_dir,
            budget.discovery_timeout(dependencies.monotonic()),
            "cargo-mutants-version",
            max_log_bytes=64 * 1024,
        )
        version_error: BaseException | None = None
        try:
            version_text = (
                _read_stdout(version) if version.exit_code == 0 else ""
            )
            if version.exit_code != 0:
                _stop_before_mutations(
                    record,
                    RunState.TOOL_UNAVAILABLE,
                    "cargo-mutants --version failed",
                )
            else:
                state, message = validate_cargo_mutants_version(version_text)
                if state is not None:
                    _stop_before_mutations(record, state, message)
                else:
                    record.tools["cargo-mutants"] = version_text.strip()
        except BaseException as error:
            version_error = error
            raise
        finally:
            version_cleanup_error = _record_command_spool_cleanup(
                record, version
            )
            if version_cleanup_error is not None:
                if version_error is not None:
                    version_error.add_note(version_cleanup_error)
                elif record.state is RunState.RUNNING:
                    _stop_before_mutations(
                        record,
                        RunState.COMMAND_FAILED,
                        "cargo-mutants version spool cleanup failed: "
                        + version_cleanup_error,
                    )

        if record.state is RunState.RUNNING:
            command_stage = "discovery"
            list_command = run_command(
                build_list_command(options.repository, [item.path for item in selected]),
                inventory_dir,
                budget.discovery_timeout(dependencies.monotonic()),
                "inventory",
                max_log_bytes=min(
                    options.disk_policy.max_log_bytes,
                    options.disk_policy.max_tool_json_bytes * 2,
                ),
            )
            inventory = None
            try:
                if list_command.exit_code != 0:
                    _stop_before_mutations(
                        record,
                        RunState.COMMAND_FAILED,
                        "cargo-mutants inventory failed",
                    )
                else:
                    inventory_text = _read_stdout(list_command)
                    inventory = parse_list_json(inventory_text)
            finally:
                inventory_cleanup_error = _record_command_spool_cleanup(
                    record, list_command
                )
            if (
                inventory_cleanup_error is not None
                and record.state is RunState.RUNNING
            ):
                _stop_before_mutations(
                    record,
                    RunState.COMMAND_FAILED,
                    "cargo-mutants inventory spool cleanup failed: "
                    + inventory_cleanup_error,
                )
            if inventory is not None and record.state is RunState.RUNNING:
                wanted = {
                    (item.path, _symbol_name(item.symbol)) for item in selected
                }
                candidates = [
                    item
                    for item in inventory
                    if not wanted
                    or (item.path, _symbol_name(item.symbol)) in wanted
                ]
                if not candidates:
                    raise ValueError(
                        "cargo-mutants inventory contains no focused candidates"
                    )
                for explicit_file in options.files:
                    normalized_file = explicit_file.replace("\\", "/")
                    if not any(
                        item.path == normalized_file for item in candidates
                    ):
                        raise ValueError(
                            "explicit file resolved to no cargo-mutants "
                            f"candidates: {normalized_file}"
                        )
                for explicit_symbol in options.symbols:
                    normalized_symbol = _symbol_name(explicit_symbol)
                    if not any(
                        _symbol_name(item.symbol) == normalized_symbol
                        for item in candidates
                    ):
                        raise ValueError(
                            "explicit symbol resolved to no cargo-mutants "
                            f"candidates: {explicit_symbol}"
                        )
                if len(candidates) > options.disk_policy.max_selector_count:
                    raise ValueError(
                        "selected mutation candidates exceed "
                        f"{options.disk_policy.max_selector_count}"
                    )
                record.candidates = rank_candidates(
                    candidates, snapshot, options.files, options.symbols
                )
                record.comparison = _comparison(
                    options.prior_inventory, len(record.candidates)
                )
                checkpoint()

        for candidate in record.candidates:
            package = _candidate_package(candidate.path)
            if candidate.state is CandidateState.PENDING and package is None:
                candidate.state = CandidateState.NOT_RUN
                candidate.not_run_reason = "outside_workspace_member"
                checkpoint()

        baseline_by_package: dict[str, bool] = {}
        for index, candidate in enumerate(record.candidates):
            active_candidate = None
            if candidate.state is not CandidateState.PENDING:
                continue
            package = _candidate_package(candidate.path)
            if package is None:
                continue
            if record.state is not RunState.RUNNING:
                break
            if not budget.may_start_mutation(dependencies.monotonic()):
                record.state = RunState.BUDGET_EXHAUSTED
                _mark_pending(record, "reporting_reserve")
                checkpoint()
                break
            if package not in baseline_by_package:
                command_stage = "baseline"
                active_package = package
                baseline = run_command(
                    build_baseline_command(candidate),
                    options.repository,
                    budget.mutation_timeout(dependencies.monotonic()),
                    f"baseline-{package}",
                )
                baseline_succeeded = baseline.exit_code == 0
                baseline_by_package[package] = baseline_succeeded
                baseline_cleanup_error = _record_command_spool_cleanup(
                    record, baseline
                )
                if baseline_succeeded and baseline_cleanup_error is not None:
                    record.state = RunState.COMMAND_FAILED
                    record.error = (
                        "baseline command spool cleanup failed: "
                        + baseline_cleanup_error
                    )
                    _mark_pending(record, "scratch_cleanup_incomplete")
                    break
            if not baseline_by_package[package]:
                _mark_pending_package(record, package, "baseline_failed")
                record.state = RunState.BASELINE_FAILED
                _mark_pending(record, "run_stopped")
                checkpoint()
                break
            if not budget.may_start_mutation(dependencies.monotonic()):
                record.state = RunState.BUDGET_EXHAUSTED
                _mark_pending(record, "reporting_reserve")
                checkpoint()
                break
            run_directory = scratch.create_child(f"candidate-{index + 1:04d}")
            command_stage = "mutation"
            active_candidate = candidate
            mutation = run_command(
                build_mutation_command(
                    options.repository,
                    run_directory,
                    candidate,
                    options.iterate,
                    options.disk_policy.jobs,
                ),
                run_directory,
                budget.mutation_timeout(dependencies.monotonic()),
                f"mutation-{index + 1:04d}",
            )
            candidate.command_sequences.append(mutation.sequence)
            candidate.state = classify_mutation_output(
                run_directory, mutation, candidate
            )
            diagnostic_allowance = _attach_candidate_diagnostic(
                candidate, mutation, diagnostic_allowance
            )
            spool_clean = _discard_command_spool(mutation)
            checkpoint()
            if candidate.state is CandidateState.ERROR:
                record.state = RunState.COMMAND_FAILED
                record.error = (
                    "cargo-mutants did not produce one valid exact outcome "
                    f"for {candidate.mutant_name}"
                )
                _mark_pending(record, "invalid_mutation_outcome")
                break
            if not spool_clean:
                record.state = RunState.COMMAND_FAILED
                record.error = "mutation command spool cleanup failed"
                _mark_pending(record, "scratch_cleanup_incomplete")
                break
            cleanup = scratch.remove_child(run_directory)
            if cleanup.status is not ScratchCleanupStatus.CLEAN:
                record.state = RunState.COMMAND_FAILED
                record.error = (
                    "candidate scratch cleanup did not complete: "
                    f"{cleanup.status.value}"
                )
                _mark_pending(record, "scratch_cleanup_incomplete")
                break
            active_candidate = None
        if record.state is RunState.RUNNING:
            record.state = RunState.COMPLETED
    except _InitialDiskStop as error:
        record.state = RunState.DISK_LIMIT
        record.error = error.failure.code
        _record_disk_failure(record, error.failure)
        event = (
            DiskLifecycleEvent.observation(
                options.disk_policy, error.failure.observation
            )
            if error.failure.observation is not None
            else DiskLifecycleEvent.measurement_failed(error.failure.message)
        )
        apply_disk_lifecycle_event(disk_lifecycle, event)
        _mark_pending(record, "disk_limit")
    except CommandTimedOut as error:
        if command_stage == "discovery":
            record.state = RunState.COMMAND_FAILED
            record.error = f"{error.record.label} timed out"
            _mark_pending(record, "command_failed")
        elif command_stage == "baseline":
            record.state = RunState.BASELINE_FAILED
            record.error = f"{error.record.label} timed out"
            if active_package is not None:
                _mark_pending_package(
                    record, active_package, "baseline_failed"
                )
            _mark_pending(record, "run_stopped")
        else:
            if active_candidate is not None:
                active_candidate.state = CandidateState.TIMEOUT
                active_candidate.command_sequences.append(error.record.sequence)
                diagnostic_allowance = _attach_candidate_diagnostic(
                    active_candidate, error.record, diagnostic_allowance
                )
        _record_command_spool_cleanup(record, error.record)
        if command_stage == "mutation":
            record.state = RunState.BUDGET_EXHAUSTED
            _mark_pending(record, "command_timeout")
    except CommandDiskStopped as error:
        if command_stage == "mutation" and active_candidate is not None:
            active_candidate.command_sequences.append(error.record.sequence)
            diagnostic_allowance = _attach_candidate_diagnostic(
                active_candidate, error.record, diagnostic_allowance
            )
        _record_command_spool_cleanup(record, error.record)
        record.state = RunState.DISK_LIMIT
        record.error = error.failure.code
        _record_disk_failure(record, error.failure)
        _mark_pending(record, "disk_limit")
    except (CommandDrainFailed, CommandLifecycleFailed) as error:
        _record_command_spool_cleanup(record, error.record)
        record.state = RunState.COMMAND_FAILED
        record.error = str(error)
        _mark_pending(record, "command_lifecycle_failed")
    except CommandInterrupted as error:
        _record_command_spool_cleanup(record, error.record)
        record.state = RunState.INTERRUPTED
        _mark_pending(record, "interrupted")
    except KeyboardInterrupt:
        record.state = RunState.INTERRUPTED
        _mark_pending(record, "interrupted")
    except subprocess.TimeoutExpired as error:
        record.state = RunState.COMMAND_FAILED
        record.error = f"{error.cmd} timed out"
        _mark_pending(record, "command_failed")
    except (OSError, subprocess.SubprocessError) as error:
        record.state = RunState.TOOL_UNAVAILABLE
        record.error = _cli_error_detail(error)
        _mark_pending(record, "tool_unavailable")
    except (ValueError, json.JSONDecodeError) as error:
        record.state = RunState.COMMAND_FAILED
        record.error = _cli_error_detail(error)
        _mark_pending(record, "command_failed")
    finally:
        if (
            isinstance(dependencies.runner, CommandRunner)
            and dependencies.runner.interrupted
        ):
            record.state = RunState.INTERRUPTED
            _mark_pending(record, "interrupted")
        record.ended_at = dependencies.utc_now().isoformat()
        record.elapsed_seconds = dependencies.monotonic() - started
        guard.sample()
        joined = guard.stop_and_join(timeout=5.0)
        latest_guard_failure = guard.latest_failure
        join_failure = (
            DiskFailure(
                code="disk.measurement.failed",
                reason=DiskStopReason.MEASUREMENT_FAILED,
                message="disk monitor did not join",
            )
            if not joined
            else None
        )
        if guard.failure is not None or latest_guard_failure is not None:
            _record_disk_failures_for_observation(
                record,
                options.disk_policy,
                None,
                guard.failure,
                latest_guard_failure,
            )
        if join_failure is not None:
            _record_disk_failure(record, join_failure)
        if guard.failure is not None:
            event = (
                DiskLifecycleEvent.observation(
                    options.disk_policy, guard.failure.observation
                )
                if guard.failure.observation is not None
                else DiskLifecycleEvent.measurement_failed(
                    guard.failure.message
                )
            )
            apply_disk_lifecycle_event(disk_lifecycle, event)
        if guard.failure is not None and record.state is not RunState.INTERRUPTED:
            if record.state not in {
                RunState.RUNNING,
                RunState.COMPLETED,
                RunState.DISK_LIMIT,
            }:
                _record_secondary_error(
                    record,
                    "outcome",
                    record.state.value,
                    record.error or record.state.value,
                )
            record.state = RunState.DISK_LIMIT
            record.error = guard.failure.code
            _mark_pending(record, "disk_limit")
        record.disk_observations = [
            {
                "owned_bytes": item.owned_bytes,
                "available_bytes": item.available_bytes,
                "measured_in_seconds": item.measured_in_seconds,
                "conservative_entries": item.conservative_entries,
                "filesystem_available_bytes": item.filesystem_available_bytes,
                "root_owned_bytes": item.root_owned_bytes,
            }
            for item in guard.observations
        ]
        process_drain_safe = (
            not isinstance(dependencies.runner, CommandRunner)
            or dependencies.runner.process_drain_safe
        )
        output_drain_safe = (
            not isinstance(dependencies.runner, CommandRunner)
            or dependencies.runner.output_drain_safe
        )
        command_root_close_errors = store.close_command_root()
        if command_root_close_errors:
            _append_report_error(record, command_root_close_errors)
        for event in (
            DiskLifecycleEvent.process_drain_succeeded()
            if process_drain_safe
            else DiskLifecycleEvent.process_drain_failed(),
            DiskLifecycleEvent.output_drain_succeeded()
            if output_drain_safe
            else DiskLifecycleEvent.output_drain_failed(),
            DiskLifecycleEvent.monitor_join_succeeded()
            if joined
            else DiskLifecycleEvent.monitor_join_failed(),
        ):
            if not apply_disk_lifecycle_event(disk_lifecycle, event):
                raise RuntimeError(
                    f"disk lifecycle rejected finalization event {event.kind}"
                )
        cleanup_requested = apply_disk_lifecycle_event(
            disk_lifecycle,
            DiskLifecycleEvent.cleanup_requested(DiskRootId.EXECUTION),
        )
        cleanup_safe = (
            joined
            and process_drain_safe
            and output_drain_safe
            and not command_root_close_errors
            and cleanup_requested
        )
        execution_observation_before_cleanup: DiskObservation | None = None
        execution_owned_before_cleanup: int | None = None
        if cleanup_safe:
            execution_meter = DiskGuard(
                options.disk_policy,
                [MeterRoot(scratch.path, enforcement="owned:scratch")],
                monotonic=dependencies.monotonic,
            )
            execution_measurement_failure = execution_meter.sample()
            if execution_meter.observations:
                execution_observation_before_cleanup = (
                    execution_meter.observations[-1]
                )
                execution_owned_before_cleanup = (
                    execution_observation_before_cleanup.owned_bytes
                )
            execution_meter_close_errors = execution_meter.close()
            if execution_measurement_failure is not None:
                _record_disk_failure(record, execution_measurement_failure)
                if record.state is RunState.COMPLETED:
                    record.state = RunState.DISK_LIMIT
                    record.error = execution_measurement_failure.code
                    _mark_pending(record, "disk_limit")
                _append_report_error(
                    record,
                    (
                        "pre-clean execution measurement failed: "
                        f"{execution_measurement_failure.code}: "
                        f"{execution_measurement_failure.message or ''}",
                    ),
                )
            _append_report_error(record, execution_meter_close_errors)
        cleanup_error: OSError | None = None
        if cleanup_safe:
            try:
                scratch.mark_cleanup_ready()
                cleanup = (
                    scratch.retain()
                    if options.disk_policy.keep_scratch
                    else scratch.cleanup()
                )
            except OSError as error:
                cleanup_error = error
                try:
                    cleanup = scratch.defer(
                        "cleanup-ready or cleanup failed: "
                        f"{type(error).__name__}: {error}"
                    )
                except OSError as defer_error:
                    cleanup = ScratchCleanupRecord(
                        ScratchCleanupStatus.FAILED,
                        0,
                        0,
                        (
                            f"cleanup failed: {type(error).__name__}: {error}",
                            f"janitor defer failed: {type(defer_error).__name__}: {defer_error}",
                        ),
                        str(scratch.path),
                    )
        else:
            cleanup = scratch.defer(
                "cleanup deferred because process, output, or monitor "
                "quiescence was not proven"
            )
        if not joined:
            if record.state is RunState.COMPLETED:
                record.state = RunState.COMMAND_FAILED
            record.error = record.error or "disk monitor did not join"
        elif not cleanup_safe:
            if record.state is RunState.COMPLETED:
                record.state = RunState.COMMAND_FAILED
            record.error = record.error or "command lifecycle cleanup is unsafe"
        if cleanup_error is not None:
            if record.state is RunState.COMPLETED:
                record.state = RunState.COMMAND_FAILED
            record.error = record.error or (
                f"cleanup-ready or retention failed: {cleanup_error}"
            )
        record.cleanup = {
            "status": cleanup.status.value,
            "examined_entries": cleanup.examined_entries,
            "removed_entries": cleanup.removed_entries,
            "details": [_bounded_detail(item) for item in cleanup.details],
            "omitted_detail_count": cleanup.omitted_detail_count,
            "remaining_root": cleanup.remaining_root,
        }
        if record.scratch is not None:
            record.scratch["cleanup"] = record.cleanup
        cleanup_outcome = {
            ScratchCleanupStatus.CLEAN: CleanupOutcome.CLEAN,
            ScratchCleanupStatus.FAILED: CleanupOutcome.FAILED,
            ScratchCleanupStatus.DEFERRED: CleanupOutcome.DEFERRED,
            ScratchCleanupStatus.RETAINED: CleanupOutcome.RETAINED,
        }[cleanup.status]
        if cleanup_requested and not apply_disk_lifecycle_event(
            disk_lifecycle,
            DiskLifecycleEvent.cleanup_completed(
                DiskRootId.EXECUTION,
                cleanup_outcome,
                "; ".join(cleanup.details) or None,
            ),
        ):
            if record.state is RunState.COMPLETED:
                record.state = RunState.COMMAND_FAILED
            record.error = record.error or "disk lifecycle rejected cleanup outcome"
        if cleanup.status in {
            ScratchCleanupStatus.FAILED,
            ScratchCleanupStatus.DEFERRED,
        }:
            _record_secondary_error(
                record,
                "cleanup",
                (
                    "workspace.cleanup.failed"
                    if cleanup.status is ScratchCleanupStatus.FAILED
                    else "workspace.cleanup.deferred"
                ),
                "; ".join(cleanup.details) or cleanup.status.value,
            )
            if record.state is RunState.COMPLETED:
                record.state = RunState.COMMAND_FAILED
            record.error = record.error or "scratch cleanup incomplete"
        elif cleanup.status is ScratchCleanupStatus.CLEAN and cleanup.details:
            _append_report_error(record, cleanup.details)
        if joined:
            _append_report_error(record, scratch.close_capabilities())
        final_roots = [
            MeterRoot(options.output, enforcement="owned:output"),
            MeterRoot(
                capacity_root,
                charge_owned_bytes=False,
                enforcement="capacity_only:cargo_home",
                exact_path=capacity_exact,
            ),
        ]
        if (
            joined
            and cleanup.remaining_root is not None
            and scratch.path.exists()
        ):
            final_roots.insert(
                0, MeterRoot(scratch.path, enforcement="owned:scratch")
            )
        final_guard = DiskGuard(
            options.disk_policy,
            final_roots,
            monotonic=dependencies.monotonic,
        )
        deferred_observation = None
        if not joined and guard.observations:
            deferred_observation = guard.observations[-1]
        elif (
            cleanup.status
            in {ScratchCleanupStatus.FAILED, ScratchCleanupStatus.DEFERRED}
        ):
            deferred_observation = execution_observation_before_cleanup
            if deferred_observation is None:
                deferred_observation = next(
                    (
                        item
                        for item in reversed(guard.observations)
                        if "owned:scratch" in item.root_owned_bytes
                    ),
                    None,
                )
        deferred_owned_floor = (
            deferred_observation.root_owned_bytes.get("owned:scratch", 0)
            if deferred_observation is not None
            else 0
        )

        def include_deferred_scratch(
            observation: DiskObservation | None,
        ) -> DiskObservation | None:
            if observation is None:
                return (
                    deferred_observation
                    if deferred_owned_floor > 0
                    else None
                )
            if deferred_owned_floor == 0:
                return observation
            deferred_scratch_identities = set(
                deferred_observation.root_owned_identities.get(
                    "owned:scratch", ()
                )
                if deferred_observation is not None
                else ()
            )
            deferred_identity_bytes = (
                deferred_observation.identity_bytes
                if deferred_observation is not None
                else {}
            )
            fresh_identity_bytes = observation.identity_bytes
            deferred_attributed_bytes = sum(
                deferred_identity_bytes.get(identity, 0)
                for identity in deferred_scratch_identities
            )
            deferred_scratch_bytes = max(
                0, deferred_owned_floor - deferred_attributed_bytes
            ) + sum(
                max(
                    0,
                    deferred_identity_bytes.get(identity, 0)
                    - fresh_identity_bytes.get(identity, 0),
                )
                for identity in deferred_scratch_identities
            )
            fresh_scratch_bytes = observation.root_owned_bytes.get(
                "owned:scratch", 0
            )
            fresh_scratch_identities = set(
                observation.root_owned_identities.get("owned:scratch", ())
            )
            merged_filesystems = dict(
                deferred_observation.filesystem_available_bytes
                if deferred_observation is not None
                else {}
            )
            for key, value in observation.filesystem_available_bytes.items():
                merged_filesystems[key] = min(
                    value, merged_filesystems.get(key, value)
                )
            return DiskObservation(
                owned_bytes=(
                    observation.owned_bytes + deferred_scratch_bytes
                ),
                available_bytes=min(
                    observation.available_bytes,
                    deferred_observation.available_bytes
                    if deferred_observation is not None
                    else observation.available_bytes,
                ),
                measured_in_seconds=max(
                    observation.measured_in_seconds,
                    deferred_observation.measured_in_seconds
                    if deferred_observation is not None
                    else 0.0,
                ),
                conservative_entries=(
                    observation.conservative_entries
                    + (
                        deferred_observation.conservative_entries
                        if deferred_observation is not None
                        else 0
                    )
                ),
                filesystem_available_bytes=merged_filesystems,
                root_owned_bytes={
                    **observation.root_owned_bytes,
                    "owned:scratch": (
                        fresh_scratch_bytes + deferred_scratch_bytes
                    ),
                },
                identity_bytes={
                    **observation.identity_bytes,
                    **{
                        identity: max(
                            deferred_identity_bytes.get(identity, 0),
                            fresh_identity_bytes.get(identity, 0),
                        )
                        for identity in deferred_scratch_identities
                    },
                },
                root_owned_identities={
                    **observation.root_owned_identities,
                    "owned:scratch": frozenset(
                        fresh_scratch_identities
                        | deferred_scratch_identities
                    ),
                },
            )

        final_guard.sample()
        raw_post_cleanup_failure = final_guard.latest_failure
        post_cleanup_observation = include_deferred_scratch(
            final_guard.observations[-1]
            if final_guard.observations
            else None
        )
        post_cleanup_failure = _record_disk_failures_for_observation(
            record,
            options.disk_policy,
            post_cleanup_observation,
            guard.failure if not joined else None,
            raw_post_cleanup_failure,
        )
        if post_cleanup_observation is not None:
            post_cleanup_item = {
                "phase": "post_cleanup",
                "owned_bytes": post_cleanup_observation.owned_bytes,
                "available_bytes": post_cleanup_observation.available_bytes,
                "measured_in_seconds": (
                    post_cleanup_observation.measured_in_seconds
                ),
                "conservative_entries": (
                    post_cleanup_observation.conservative_entries
                ),
                "filesystem_available_bytes": (
                    post_cleanup_observation.filesystem_available_bytes
                ),
                "root_owned_bytes": post_cleanup_observation.root_owned_bytes,
            }
            if len(record.disk_observations) >= 256:
                del record.disk_observations[1]
            record.disk_observations.append(post_cleanup_item)
        if guard.sample_count or final_guard.sample_count:
            start_free = dict(guard.start_free_bytes)
            end_free = dict(final_guard.end_free_bytes)
            all_filesystems = sorted(set(start_free) | set(end_free))
            minimum_values = [
                value
                for value in (
                    guard.minimum_free_bytes,
                    final_guard.minimum_free_bytes,
                )
                if value is not None
            ]
            record.disk_summary = {
                "start_free_bytes": start_free,
                "end_free_bytes": end_free,
                "free_byte_delta": {
                    key: (
                        end_free[key] - start_free[key]
                        if key in start_free and key in end_free
                        else None
                    )
                    for key in all_filesystems
                },
                "peak_owned_bytes": max(
                    guard.peak_owned_bytes,
                    final_guard.peak_owned_bytes,
                    (
                        post_cleanup_observation.owned_bytes
                        if post_cleanup_observation is not None
                        else 0
                    ),
                ),
                "minimum_free_bytes": min(minimum_values),
                "sample_count": guard.sample_count + final_guard.sample_count,
                "maximum_measurement_seconds": max(
                    guard.maximum_measurement_seconds,
                    final_guard.maximum_measurement_seconds,
                ),
                "absence_verified_removed_logical_bytes": (
                    execution_owned_before_cleanup
                    if cleanup.status is ScratchCleanupStatus.CLEAN
                    and post_cleanup_observation is not None
                    else None
                ),
            }
        if post_cleanup_failure is not None:
            if record.state is RunState.COMPLETED:
                record.state = RunState.DISK_LIMIT
                record.error = post_cleanup_failure.code
            _mark_pending(record, "disk_limit")

        def finalization_interrupted() -> bool:
            return bool(getattr(dependencies.runner, "interrupted", False))

        def apply_finalization_interrupt() -> None:
            record.state = RunState.INTERRUPTED
            record.error = record.error or "interrupted during finalization"
            _mark_pending(record, "interrupted")

        preclose_errors = [
            *guard.close_errors,
            *final_guard.probe_close(),
            *owned_output.probe_close(),
        ]
        _append_report_error(record, preclose_errors)
        report_boundary_evidence_recorded = False

        def final_report_sample() -> tuple[
            DiskFailure | None, DiskObservation | None
        ]:
            nonlocal report_boundary_evidence_recorded
            final_guard.sample()
            raw_failure = final_guard.latest_failure
            observation = include_deferred_scratch(
                final_guard.observations[-1]
                if final_guard.observations
                else None
            )
            failure = _record_disk_failures_for_observation(
                record,
                options.disk_policy,
                observation,
                guard.failure if not joined else None,
                raw_failure,
            )
            if (
                observation is not None
                and not report_boundary_evidence_recorded
            ):
                report_boundary_evidence_recorded = True
                if len(record.disk_observations) >= 256:
                    del record.disk_observations[1]
                observation_evidence = _disk_observation_evidence(observation)
                assert observation_evidence is not None
                record.disk_observations.append(
                    {
                        "phase": "report_boundary",
                        **observation_evidence,
                    }
                )
                record.disk_summary["end_free_bytes"] = dict(
                    final_guard.end_free_bytes
                )
                summary_start = record.disk_summary.get("start_free_bytes", {})
                summary_end = record.disk_summary["end_free_bytes"]
                if (
                    isinstance(summary_start, dict)
                    and isinstance(summary_end, dict)
                ):
                    record.disk_summary["free_byte_delta"] = {
                        key: (
                            summary_end[key] - summary_start[key]
                            if key in summary_start and key in summary_end
                            and isinstance(summary_start[key], int)
                            and isinstance(summary_end[key], int)
                            else None
                        )
                        for key in sorted(set(summary_start) | set(summary_end))
                    }
                current_peak = record.disk_summary.get("peak_owned_bytes", 0)
                record.disk_summary["peak_owned_bytes"] = max(
                    current_peak if isinstance(current_peak, int) else 0,
                    final_guard.peak_owned_bytes,
                    observation.owned_bytes,
                )
                current_minimum = record.disk_summary.get("minimum_free_bytes")
                if final_guard.minimum_free_bytes is not None:
                    record.disk_summary["minimum_free_bytes"] = (
                        final_guard.minimum_free_bytes
                        if not isinstance(current_minimum, int)
                        else min(current_minimum, final_guard.minimum_free_bytes)
                    )
                record.disk_summary["sample_count"] = (
                    guard.sample_count + final_guard.sample_count
                )
                current_duration = record.disk_summary.get(
                    "maximum_measurement_seconds", 0.0
                )
                record.disk_summary["maximum_measurement_seconds"] = max(
                    (
                        float(current_duration)
                        if isinstance(current_duration, (int, float))
                        else 0.0
                    ),
                    final_guard.maximum_measurement_seconds,
                )
            return failure, observation

        store.configure_report_guard(
            options.disk_policy,
            final_report_sample,
            scratch.report_boundary,
            lambda: bool(getattr(dependencies.runner, "interrupted", False)),
        )

        try:
            if finalization_interrupted():
                apply_finalization_interrupt()
            store.checkpoint(record)
            if finalization_interrupted():
                apply_finalization_interrupt()
                store.configure_report_guard(
                    options.disk_policy,
                    final_report_sample,
                    scratch.report_boundary,
                )
                store.checkpoint(record)
            store.write_markdown(record)
            if finalization_interrupted():
                apply_finalization_interrupt()
                store.configure_report_guard(
                    options.disk_policy,
                    final_report_sample,
                    scratch.report_boundary,
                )
                store.checkpoint(record)
                store.write_markdown(record)
        except OSError as error:
            interrupted = finalization_interrupted()
            _append_report_error(record, (_cli_error_detail(error),))
            if interrupted:
                apply_finalization_interrupt()
            try:
                if interrupted:
                    store.configure_report_guard(
                        options.disk_policy,
                        final_report_sample,
                        scratch.report_boundary,
                    )
                store.checkpoint(record)
            except OSError as retry_error:
                _append_report_error(
                    record,
                    (
                        "final report retry failed: "
                        + _cli_error_detail(retry_error),
                    ),
                )
        finally:
            if dependencies.restore_signal_handlers is not None:
                dependencies.restore_signal_handlers()
            if finalization_interrupted():
                apply_finalization_interrupt()
                try:
                    store.configure_report_guard(
                        options.disk_policy,
                        final_report_sample,
                        scratch.report_boundary,
                    )
                    store.checkpoint(record)
                    store.write_markdown(record)
                except OSError as error:
                    _append_report_error(
                        record,
                        (
                            "interrupted final report persistence failed: "
                            + _cli_error_detail(error),
                        ),
                    )
            trailing_close_errors = [
                *final_guard.close(),
                *owned_output.release_marker(remove_marker=False),
                *owned_output.close_directory(),
            ]
            _append_report_error(record, trailing_close_errors)
            report_event = (
                DiskLifecycleEvent.report_succeeded()
                if record.report_error is None
                else DiskLifecycleEvent.report_failed()
            )
            if not apply_disk_lifecycle_event(disk_lifecycle, report_event):
                raise RuntimeError("disk lifecycle rejected report outcome")
            if record.report_error is None:
                apply_disk_lifecycle_event(
                    disk_lifecycle, DiskLifecycleEvent.finish_requested()
                )
    return record


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--budget", default="30m")
    parser.add_argument("--base", default="origin/main")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--file", action="append", default=[])
    parser.add_argument("--symbol", action="append", default=[])
    parser.add_argument("--iterate", action="store_true")
    parser.add_argument("--prior-inventory", type=Path)
    parser.add_argument("--max-disk", default="8GiB")
    parser.add_argument("--min-free-space", default="10GiB")
    parser.add_argument("--jobs", type=int, default=1)
    parser.add_argument("--max-log-size", default="16MiB")
    parser.add_argument("--scratch-root", type=Path)
    parser.add_argument("--keep-scratch", action="store_true")
    return parser


@contextmanager
def _scoped_signal_handlers(
    cancellation: threading.Event,
) -> Iterator[Callable[[], None]]:
    previous: list[tuple[signal.Signals, object]] = []
    restored = False
    previous_mask: set[int | signal.Signals] | None = None

    def interrupt(_signum: int, _frame: object) -> None:
        cancellation.set()

    def restore() -> None:
        nonlocal restored, previous_mask
        if restored:
            return
        if hasattr(signal, "pthread_sigmask"):
            previous_mask = signal.pthread_sigmask(
                signal.SIG_BLOCK, {signal.SIGINT, signal.SIGTERM}
            )
        for signum, handler in previous:
            signal.signal(signum, cast(Any, handler))
        restored = True

    try:
        for signum in (signal.SIGINT, signal.SIGTERM):
            previous.append((signum, signal.getsignal(signum)))
            signal.signal(signum, interrupt)
        yield restore
    finally:
        restore()
        if previous_mask is not None:
            signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)


def options_from_arguments(
    arguments: argparse.Namespace,
    repository: Path,
    *,
    budget_seconds: float | None = None,
) -> Options:
    if os.name == "nt":
        raise ValueError(
            "disk-safe focused mutation requires the Windows native adapter"
        )
    if budget_seconds is None:
        budget_seconds = parse_duration(arguments.budget)
    selectors = [*arguments.file, *arguments.symbol]
    if len(selectors) > 1_000:
        raise ValueError("combined file and symbol selectors exceed 1000")
    for selector in selectors:
        try:
            encoded = selector.encode("utf-8", errors="strict")
        except UnicodeError as error:
            raise ValueError("selector is not strict UTF-8") from error
        if len(encoded) > 16 * 1024:
            raise ValueError("selector exceeds 16 KiB")
    policy = DiskPolicy(
        max_disk_bytes=parse_byte_size(arguments.max_disk),
        min_free_bytes=parse_byte_size(arguments.min_free_space),
        jobs=arguments.jobs,
        max_log_bytes=parse_byte_size(arguments.max_log_size),
        scratch_root=(
            arguments.scratch_root
            if arguments.scratch_root is not None
            else Path(tempfile.gettempdir())
        ),
        keep_scratch=arguments.keep_scratch,
    )
    return Options(
        repository=repository,
        output=Path(os.path.abspath(arguments.output)),
        budget_seconds=budget_seconds,
        base=arguments.base,
        files=tuple(arguments.file),
        symbols=tuple(arguments.symbol),
        iterate=arguments.iterate,
        prior_inventory=arguments.prior_inventory,
        disk_policy=policy,
    )


def _emit_report_failure(record: RunRecord) -> None:
    remaining = None
    if record.cleanup is not None:
        value = record.cleanup.get("remaining_root")
        if isinstance(value, str):
            remaining = value
    detail = json.dumps(remaining, ensure_ascii=True)
    failure = json.dumps(
        _bounded_detail(record.report_error or "unknown report failure", 16 * 1024),
        ensure_ascii=True,
    )
    line = (
        "focused mutation report failed: report.delivery.failed; "
        f"remaining_root={detail}; detail={failure}\n"
    ).encode("utf-8")
    sys.stderr.buffer.write(line[: 20 * 1024])
    sys.stderr.buffer.flush()


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        budget_seconds = parse_duration(arguments.budget)
        budget = RunBudget.start(budget_seconds, time.monotonic())
        repository = Path.cwd().resolve()
        while repository != repository.parent and not (
            repository / ".git"
        ).exists():
            repository = repository.parent
        if not (repository / ".git").exists():
            raise ValueError("current directory is not inside a Git repository")
        options = options_from_arguments(
            arguments, repository, budget_seconds=budget_seconds
        )
        store = RunStore(options.output)
        cancellation = threading.Event()
        runner = CommandRunner(store, cancellation_event=cancellation)
        with _scoped_signal_handlers(cancellation) as restore_signal_handlers:
            record = run_workflow(
                options,
                Dependencies(
                    time.monotonic,
                    lambda: datetime.now(timezone.utc),
                    SubprocessProbe(repository, runner),
                    runner,
                    restore_signal_handlers,
                ),
                budget=budget,
            )
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        _parser().error(_cli_error_detail(error))
    if cancellation.is_set() and record.state is RunState.COMPLETED:
        record.state = RunState.INTERRUPTED
        record.error = "interrupted before signal handlers were restored"
    if record.report_error is not None:
        _emit_report_failure(record)
    if record.state is RunState.COMPLETED:
        return 0
    if record.state is RunState.BUDGET_EXHAUSTED:
        return 3
    if record.state is RunState.INTERRUPTED:
        return 130
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
