#!/usr/bin/env python3
from __future__ import annotations

import argparse
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import time

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from tools.focused_mutation_support.budget import RunBudget, parse_duration
from tools.focused_mutation_support.discovery import (
    CommandProbe,
    discover_candidates,
    discover_repository,
)
from tools.focused_mutation_support.model import (
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
    validate_cargo_mutants_version,
)
from tools.focused_mutation_support.ranking import rank_candidates
from tools.focused_mutation_support.reporting import render_markdown
from tools.focused_mutation_support.runner import (
    CommandInterrupted,
    CommandRunner,
    CommandTimedOut,
)
from tools.focused_mutation_support.store import RunStore


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


@dataclass(frozen=True)
class Dependencies:
    monotonic: Callable[[], float]
    utc_now: Callable[[], datetime]
    probe: CommandProbe
    runner: CommandRunner


class SubprocessProbe:
    def __init__(self, cwd: Path) -> None:
        self.cwd = cwd

    def text(self, argv: list[str], timeout: float) -> str:
        return subprocess.run(
            argv,
            cwd=self.cwd,
            check=True,
            capture_output=True,
            text=True,
            shell=False,
            timeout=timeout,
        ).stdout


def _read_stdout(command: CommandRecord) -> str:
    return Path(command.stdout_path).read_text(encoding="utf-8")


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
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict) or not isinstance(value.get("candidates"), list):
        raise ValueError("prior inventory must contain a candidates array")
    full = len(value["candidates"])
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
    store = RunStore(options.output)
    store.initialize(record)
    command_stage = "discovery"
    active_candidate = None
    active_package = None

    def checkpoint() -> None:
        record.elapsed_seconds = dependencies.monotonic() - started
        store.checkpoint(record)

    def run_command(
        argv: Sequence[str], cwd: Path, timeout: float, label: str
    ):
        try:
            command = dependencies.runner.run(argv, cwd, timeout, label)
        except (CommandTimedOut, CommandInterrupted) as error:
            record.commands.append(error.record)
            checkpoint()
            raise
        record.commands.append(command)
        checkpoint()
        return command

    try:
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
        )
        selected = rank_candidates(
            selected, snapshot, options.files, options.symbols
        )
        record.candidates = selected
        checkpoint()

        inventory_dir = options.output / "cargo-mutants" / "inventory"
        inventory_dir.mkdir(parents=True, exist_ok=True)
        command_stage = "discovery"
        version = run_command(
            ["cargo", "mutants", "--version"],
            inventory_dir,
            budget.discovery_timeout(dependencies.monotonic()),
            "cargo-mutants-version",
        )
        if version.exit_code != 0:
            _stop_before_mutations(
                record,
                RunState.TOOL_UNAVAILABLE,
                "cargo-mutants --version failed",
            )
        else:
            state, message = validate_cargo_mutants_version(_read_stdout(version))
            if state is not None:
                _stop_before_mutations(record, state, message)
            else:
                record.tools["cargo-mutants"] = _read_stdout(version).strip()

        if record.state is RunState.RUNNING:
            command_stage = "discovery"
            list_command = run_command(
                build_list_command(options.repository, [item.path for item in selected]),
                inventory_dir,
                budget.discovery_timeout(dependencies.monotonic()),
                "inventory",
            )
            if list_command.exit_code != 0:
                _stop_before_mutations(
                    record,
                    RunState.COMMAND_FAILED,
                    "cargo-mutants inventory failed",
                )
            else:
                inventory = parse_list_json(_read_stdout(list_command))
                wanted = {(item.path, item.symbol) for item in selected}
                candidates = [
                    item for item in inventory if not wanted or (item.path, item.symbol) in wanted
                ]
                record.candidates = rank_candidates(
                    candidates, snapshot, options.files, options.symbols
                )
                record.comparison = _comparison(
                    options.prior_inventory, len(record.candidates)
                )
                checkpoint()

        for candidate in record.candidates:
            package = _candidate_package(candidate.path)
            if package is None:
                candidate.state = CandidateState.NOT_RUN
                candidate.not_run_reason = "outside_workspace_member"
                checkpoint()

        baseline_by_package: dict[str, bool] = {}
        for index, candidate in enumerate(record.candidates):
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
                baseline_by_package[package] = baseline.exit_code == 0
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
            run_directory = options.output / "cargo-mutants" / f"{index + 1:04d}"
            run_directory.mkdir(parents=True, exist_ok=True)
            command_stage = "mutation"
            active_candidate = candidate
            mutation = run_command(
                build_mutation_command(
                    options.repository,
                    run_directory,
                    candidate,
                    options.iterate,
                ),
                run_directory,
                budget.mutation_timeout(dependencies.monotonic()),
                f"mutation-{index + 1:04d}",
            )
            candidate.command_sequences.append(mutation.sequence)
            candidate.state = classify_mutation_output(
                run_directory, mutation, candidate
            )
            checkpoint()
        if record.state is RunState.RUNNING:
            record.state = RunState.COMPLETED
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
            record.state = RunState.BUDGET_EXHAUSTED
            _mark_pending(record, "command_timeout")
    except (CommandInterrupted, KeyboardInterrupt):
        record.state = RunState.INTERRUPTED
        _mark_pending(record, "interrupted")
    except subprocess.TimeoutExpired as error:
        record.state = RunState.COMMAND_FAILED
        record.error = f"{error.cmd} timed out"
        _mark_pending(record, "command_failed")
    except (OSError, subprocess.SubprocessError) as error:
        record.state = RunState.TOOL_UNAVAILABLE
        record.error = str(error)
        _mark_pending(record, "tool_unavailable")
    except (ValueError, json.JSONDecodeError) as error:
        record.state = RunState.COMMAND_FAILED
        record.error = str(error)
        _mark_pending(record, "command_failed")
    finally:
        record.ended_at = dependencies.utc_now().isoformat()
        record.elapsed_seconds = dependencies.monotonic() - started
        try:
            store.checkpoint(record)
            (options.output / "report.md").write_text(
                render_markdown(record), encoding="utf-8"
            )
        except OSError as error:
            record.state = RunState.REPORT_FAILED
            record.error = str(error)
            try:
                store.checkpoint(record)
            except OSError:
                pass
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
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    try:
        budget_seconds = parse_duration(arguments.budget)
        budget = RunBudget.start(budget_seconds, time.monotonic())
        repository = Path(
            subprocess.run(
                ["git", "rev-parse", "--show-toplevel"],
                check=True,
                capture_output=True,
                text=True,
                shell=False,
                timeout=budget.discovery_timeout(time.monotonic()),
            ).stdout.strip()
        ).resolve()
        options = Options(
            repository,
            arguments.output.resolve(),
            budget_seconds,
            arguments.base,
            tuple(arguments.file),
            tuple(arguments.symbol),
            arguments.iterate,
            arguments.prior_inventory,
        )
        store = RunStore(options.output)
        runner = CommandRunner(store)
        record = run_workflow(
            options,
            Dependencies(
                time.monotonic,
                lambda: datetime.now(timezone.utc),
                SubprocessProbe(repository),
                runner,
            ),
            budget=budget,
        )
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        _parser().error(str(error))
    if record.state in {RunState.COMPLETED, RunState.BUDGET_EXHAUSTED}:
        return 0
    if record.state is RunState.INTERRUPTED:
        return 130
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
