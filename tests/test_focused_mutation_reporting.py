import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from tools.focused_mutation_support.model import (
    Candidate,
    CandidateState,
    CommandRecord,
    RankingReason,
    RunRecord,
    RunState,
)
from tools.focused_mutation_support.reporting import render_markdown
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
                "/repo/Cargo.toml",
                "--file",
                "crates/hoimin-core/src/machine.rs",
            ],
        )

    def test_focused_command_uses_exact_anchored_mutant_name(self) -> None:
        candidate = Candidate(
            path="crates/hoimin-core/src/machine.rs",
            symbol="RunState::accept_completion",
            mutant_name=(
                "crates/hoimin-core/src/machine.rs:324: replace guard"
            ),
        )
        argv = build_mutation_command(Path("/repo"), candidate, iterate=False)
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
            build_mutation_command(Path("/repo"), candidate, iterate=True)[-1],
            "--iterate",
        )

    def test_focused_command_requires_inventory_name(self) -> None:
        candidate = Candidate("crates/a/src/lib.rs", "f", None)
        with self.assertRaisesRegex(ValueError, "mutant name"):
            build_mutation_command(Path("/repo"), candidate, iterate=False)

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
