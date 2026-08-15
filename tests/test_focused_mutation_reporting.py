import json
from dataclasses import replace
from datetime import datetime, timezone
import inspect
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from tools.focused_mutation_support.model import (
    Candidate,
    CandidateState,
    CommandRecord,
    RankingReason,
    RunRecord,
    RunState,
)
from tools.focused_mutation_support.reporting import render_markdown
from tools.focused_mutation_support.runner import CommandTimedOut
from tools.focused_mutation import (
    Dependencies,
    Options,
    SubprocessProbe,
    _candidate_package,
    _parser,
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
        malformed_inventory: bool = False,
        after_baseline: object | None = None,
        version_output: str = "cargo-mutants 27.1.0\n",
    ) -> None:
        self.output = output
        self.fail_label = fail_label
        self.timeout_label = timeout_label
        self.interrupt_label = interrupt_label
        self.malformed_inventory = malformed_inventory
        self.after_baseline = after_baseline
        self.version_output = version_output
        self.calls: list[tuple[list[str], Path, float, str]] = []
        self.checkpoint_command_counts: list[int] = []

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
        if label.startswith("baseline-") and self.after_baseline is not None:
            self.after_baseline()
        if label.startswith("mutation-"):
            results = cwd / "mutants.out"
            results.mkdir(parents=True)
            (results / "caught.txt").write_text(
                f"machine.rs:1: replace {label.removeprefix('mutation-') == '0001' and 'a' or 'b'}\n",
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
        root, output, 1_800.0, "origin/main", (), (), False, None
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


class FocusedMutationReportingTests(unittest.TestCase):
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

    def test_baseline_uses_mutation_deadline_after_discovery_window(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            clock = FakeClock()
            options, dependencies, runner = workflow_fixture(
                directory, clock=clock
            )

            def finish_discovery(*_: object) -> list[Candidate]:
                clock.now = 650.0
                return []

            with mock.patch(
                "tools.focused_mutation.discover_candidates",
                side_effect=finish_discovery,
            ):
                run_workflow(options, dependencies)

            baseline_call = next(
                call
                for call in runner.calls
                if call[3] == "baseline-hoimin-core"
            )
            self.assertEqual(baseline_call[2], 850.0)

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
                    "tools.focused_mutation.discover_candidates", return_value=[]
                ),
                mock.patch(
                    "tools.focused_mutation.rank_candidates",
                    side_effect=lambda candidates, *_: candidates,
                ),
                mock.patch(f"{__name__}.WORKFLOW_LIST_JSON", inventory),
            ):
                record = run_workflow(options, dependencies)

            self.assertEqual(record.state, RunState.COMPLETED)
            self.assertEqual(len(record.candidates), 4)
            by_path = {candidate.path: candidate for candidate in record.candidates}
            self.assertTrue(
                all(
                    by_path[path].state is CandidateState.NOT_RUN
                    and by_path[path].not_run_reason
                    == "outside_workspace_member"
                    for path in ("build.rs", "crates/hoimin-core", "tools/helper.py")
                )
            )
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
                    "tools.focused_mutation.discover_candidates", return_value=[]
                ),
                mock.patch(f"{__name__}.WORKFLOW_LIST_JSON", inventory),
            ):
                record = run_workflow(options, dependencies)

            by_path = {candidate.path: candidate for candidate in record.candidates}
            self.assertEqual(record.state, RunState.BUDGET_EXHAUSTED)
            self.assertEqual(
                by_path["build.rs"].not_run_reason,
                "outside_workspace_member",
            )

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
        repository_result = subprocess.CompletedProcess(
            ["git"], 0, stdout=str(Path.cwd()), stderr=""
        )
        clock = mock.Mock(side_effect=[100.0, 130.0])
        record = fixture_record(candidates=[], state=RunState.COMPLETED)
        with (
            mock.patch("tools.focused_mutation.time.monotonic", clock),
            mock.patch(
                "tools.focused_mutation.subprocess.run",
                return_value=repository_result,
            ) as run,
            mock.patch(
                "tools.focused_mutation.run_workflow",
                return_value=record,
            ) as workflow,
        ):
            self.assertEqual(main(["--budget", "3m", "--output", "/tmp/out"]), 0)

        self.assertEqual(run.call_args.kwargs["timeout"], 30.0)
        self.assertEqual(
            run.call_args.args[0],
            ["git", "rev-parse", "--show-toplevel"],
        )
        self.assertFalse(run.call_args.kwargs["shell"])
        self.assertTrue(run.call_args.kwargs["text"])
        self.assertEqual(run.call_args.kwargs["encoding"], "utf-8")
        self.assertEqual(run.call_args.kwargs["errors"], "surrogateescape")
        self.assertEqual(workflow.call_args.kwargs["budget"].started, 100.0)

    def test_git_probe_uses_utf8_independently_of_the_windows_locale(self) -> None:
        repository = Path("C:/日本語のリポジトリ")
        result = subprocess.CompletedProcess(
            ["git"], 0, stdout="日本語.py\n", stderr=""
        )
        with mock.patch(
            "tools.focused_mutation.subprocess.run", return_value=result
        ) as run:
            output = SubprocessProbe(repository).text(
                ["git", "status", "--porcelain=v1", "-z"], 12.0
            )

        self.assertEqual(output, "日本語.py\n")
        self.assertEqual(run.call_args.kwargs["encoding"], "utf-8")
        self.assertEqual(run.call_args.kwargs["errors"], "surrogateescape")
        self.assertTrue(run.call_args.kwargs["text"])
        self.assertFalse(run.call_args.kwargs["shell"])

    def test_initial_repository_validation_timeout_exits_two(self) -> None:
        with (
            mock.patch(
                "tools.focused_mutation.subprocess.run",
                side_effect=subprocess.TimeoutExpired(["git"], 1.0),
            ),
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
            RunState.BUDGET_EXHAUSTED: 0,
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

    def test_focused_command_enables_reuse_only_when_requested(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", "name")
        self.assertEqual(
            build_mutation_command(
                Path("/repo"), Path("/evidence/run"), candidate, iterate=True
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
            ["cargo", "test", "-p", "hoimin-cli"],
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

    def test_list_parser_rejects_incomplete_entries(self) -> None:
        with self.assertRaisesRegex(ValueError, "name"):
            parse_list_json('[{"file": "src/lib.rs"}]')

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
            "caught.txt": CandidateState.KILLED,
            "missed.txt": CandidateState.SURVIVED,
            "timeout.txt": CandidateState.TIMEOUT,
            "unviable.txt": CandidateState.UNVIABLE,
        }
        candidate = Candidate("crates/a/src/lib.rs", "f", "exact mutant")
        for filename, state in expected.items():
            with self.subTest(filename=filename), tempfile.TemporaryDirectory() as tmp:
                output = Path(tmp) / "mutants.out"
                output.mkdir()
                (output / filename).write_text(
                    "exact mutant\n", encoding="utf-8"
                )
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
