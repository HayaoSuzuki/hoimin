from pathlib import Path
import os
import subprocess
import tempfile
import unittest
from unittest import mock

from tools.focused_mutation_support.discovery import (
    RepositorySnapshot,
    _normalize_path,
    discover_candidates,
    discover_repository,
)
from tools.focused_mutation_support.model import Candidate
from tools.focused_mutation_support.ranking import rank_candidates


class FakeProbe:
    def __init__(self, replies: dict[tuple[str, ...], str]):
        self.replies = replies
        self.calls: list[tuple[tuple[str, ...], float]] = []

    def text(self, argv: list[str], timeout: float) -> str:
        key = tuple(argv)
        self.calls.append((key, timeout))
        return self.replies[key]


class LocalGitProbe:
    def __init__(self, root: Path):
        self.root = root

    def text(self, argv: list[str], timeout: float) -> str:
        return subprocess.run(
            argv,
            cwd=self.root,
            check=True,
            capture_output=True,
            encoding="utf-8",
            timeout=timeout,
        ).stdout


class DiscoveryTests(unittest.TestCase):
    def test_recent_git_paths_reach_fallback_selection_and_ranking(self) -> None:
        names = ["日本.rs", "with space.rs"]
        if os.name != "nt":
            names.extend(['with"quote.rs', "with\\backslash.rs"])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            probe = LocalGitProbe(root)
            probe.text(["git", "init", "--quiet"], 10.0)
            probe.text(["git", "config", "core.quotepath", "true"], 10.0)
            for name in names:
                source = root / "crates/core/src" / name
                source.parent.mkdir(parents=True, exist_ok=True)
                for revision in range(2):
                    source.write_text(
                        f"fn selected() {{ let revision = {revision}; }}\n",
                        encoding="utf-8",
                    )
                    probe.text(["git", "add", "--", str(source)], 10.0)
                    probe.text(
                        [
                            "git", "-c", "user.name=Test", "-c",
                            "user.email=test@example.invalid", "-c",
                            "commit.gpgsign=false", "-c", "core.hooksPath=",
                            "commit", "--quiet", "-m", "Update source",
                        ],
                        10.0,
                    )

            snapshot = discover_repository(root, "HEAD", probe, lambda: 10.0)
            self.assertEqual(snapshot.dirty_paths, ())
            self.assertEqual(snapshot.base_paths, ())
            self.assertEqual(
                snapshot.recent_paths,
                tuple(f"crates/core/src/{name}" for name in reversed(names)),
            )
            candidates = discover_candidates(
                snapshot, (), (), probe, lambda: 10.0
            )
            self.assertEqual(
                {(item.path, item.symbol) for item in candidates},
                {(f"crates/core/src/{name}", "selected") for name in names},
            )
            ranked = rank_candidates(candidates, snapshot)
            for candidate in ranked:
                self.assertEqual(
                    [reason.code for reason in candidate.reasons],
                    ["recent_change"],
                )

    def test_repository_path_rejects_markdown_control_characters(self) -> None:
        with self.assertRaisesRegex(ValueError, "Markdown"):
            _normalize_path("crates/core/src/unsafe\nname.rs")

    def test_symbol_only_selection_uses_inventory_to_find_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "crates/core/src/machine.rs"
            target.parent.mkdir(parents=True)
            target.write_text(
                "fn ordinary() {}\npub fn cancel() {}\n", encoding="utf-8"
            )
            probe = FakeProbe(
                {
                    (
                        "rg",
                        "--files",
                        "--glob",
                        "*.rs",
                        str(root),
                    ): f"{target}\n"
                }
            )
            snapshot = RepositorySnapshot(root, "abc", "feature", (), (), ())

            candidates = discover_candidates(
                snapshot, (), ("cancel",), probe, lambda: 17.0
            )

        self.assertEqual(
            [(item.path, item.symbol) for item in candidates],
            [("crates/core/src/machine.rs", "cancel")],
        )
        self.assertEqual(
            probe.calls,
            [(("rg", "--files", "--glob", "*.rs", str(root)), 17.0)],
        )

    def test_symbol_selector_excludes_unrelated_changed_functions_before_cap(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            changed = root / "crates/core/src/changed.rs"
            target = root / "crates/core/src/target.rs"
            changed.parent.mkdir(parents=True)
            changed.write_text(
                "fn unrelated_first() {}\nfn unrelated_second() {}\n",
                encoding="utf-8",
            )
            target.write_text("fn selected() {}\n", encoding="utf-8")
            snapshot = RepositorySnapshot(
                root,
                "abc",
                "feature",
                (),
                ("crates/core/src/changed.rs",),
                (),
            )
            probe = FakeProbe(
                {
                    (
                        "rg",
                        "--files",
                        "--glob",
                        "*.rs",
                        str(root),
                    ): f"{changed}\n{target}\n"
                }
            )

            try:
                candidates = discover_candidates(
                    snapshot,
                    (),
                    ("selected",),
                    probe,
                    lambda: 17.0,
                    max_candidates=1,
                )
            except ValueError as error:
                self.fail(f"symbol selection included unrelated functions: {error}")

        self.assertEqual(
            [(item.path, item.symbol) for item in candidates],
            [("crates/core/src/target.rs", "selected")],
        )

    def test_explicit_rust_file_overrides_implicit_path_exclusions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "tests/generated/fixture.rs"
            target.parent.mkdir(parents=True)
            target.write_text("fn explicit_fixture() {}\n", encoding="utf-8")
            snapshot = RepositorySnapshot(
                root,
                "abc",
                "feature",
                ("tests/generated/fixture.rs",),
                (),
                (),
            )

            candidates = discover_candidates(
                snapshot,
                ("tests/generated/fixture.rs",),
                (),
                FakeProbe({}),
                lambda: 10.0,
            )

        self.assertEqual(
            [(item.path, item.symbol) for item in candidates],
            [("tests/generated/fixture.rs", "explicit_fixture")],
        )

    def test_recent_history_stops_while_adding_tenth_unique_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            current = root / "crates/core/src/current.rs"
            recent = root / "crates/core/src/recent.rs"
            current.parent.mkdir(parents=True)
            current.write_text(
                "\n".join(f"fn current_{number}() {{}}" for number in range(9)),
                encoding="utf-8",
            )
            recent.write_text(
                "fn recent_first() {}\nfn recent_second() {}\n",
                encoding="utf-8",
            )
            snapshot = RepositorySnapshot(
                root,
                "abc",
                "feature",
                ("crates/core/src/current.rs",),
                (),
                ("crates/core/src/recent.rs",),
            )

            candidates = discover_candidates(
                snapshot, (), (), FakeProbe({}), lambda: 10.0
            )

        self.assertEqual(len(candidates), 10)
        self.assertEqual(candidates[-1].symbol, "recent_first")

    def test_generated_components_are_excluded_from_implicit_discovery(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = (
                "crates/core/generated/output.rs",
                "crates/core/gen/output.rs",
                "crates/core/src/generated_name.rs",
            )
            for path in paths:
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("fn candidate() {}\n", encoding="utf-8")
            snapshot = RepositorySnapshot(
                root, "abc", "feature", paths, (), ()
            )

            candidates = discover_candidates(
                snapshot, (), (), FakeProbe({}), lambda: 10.0
            )

        self.assertEqual(
            [item.path for item in candidates],
            ["crates/core/src/generated_name.rs"],
        )

    def test_oversized_rust_source_is_rejected_before_materialization(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "crates/core/src/oversized.rs"
            target.parent.mkdir(parents=True)
            with target.open("wb") as stream:
                stream.truncate(8 * 1024**2 + 1)
            snapshot = RepositorySnapshot(
                root,
                "abc",
                "feature",
                ("crates/core/src/oversized.rs",),
                (),
                (),
            )

            with mock.patch.object(
                Path,
                "read_text",
                side_effect=AssertionError("unbounded source read"),
            ):
                candidates = discover_candidates(
                    snapshot, (), (), FakeProbe({}), lambda: 10.0
                )

        self.assertEqual(candidates, [])

    def test_oversized_function_symbol_is_rejected_before_candidate_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "crates/core/src/oversized_symbol.rs"
            target.parent.mkdir(parents=True)
            target.write_text(
                "fn " + "x" * (16 * 1024 + 1) + "() {}\n",
                encoding="utf-8",
            )
            snapshot = RepositorySnapshot(
                root,
                "abc",
                "feature",
                ("crates/core/src/oversized_symbol.rs",),
                (),
                (),
            )

            with self.assertRaisesRegex(
                ValueError, "function symbol exceeds 16384 bytes"
            ):
                discover_candidates(
                    snapshot, (), (), FakeProbe({}), lambda: 10.0
                )

    def test_changed_and_explicit_targets_rank_deterministically(self) -> None:
        snapshot = RepositorySnapshot(
            root=Path("/repo"),
            head="abc",
            branch="feature",
            dirty_paths=("crates/hoimin-core/src/machine.rs",),
            base_paths=("crates/hoimin-cli/src/process/mod.rs",),
            recent_paths=(),
        )
        candidates = [
            Candidate("crates/hoimin-cli/src/process/mod.rs", "cancel", None),
            Candidate("crates/hoimin-core/src/machine.rs", "transition", None),
        ]

        ranked = rank_candidates(
            candidates, snapshot, explicit_symbols=("cancel",)
        )

        self.assertEqual(
            [item.symbol for item in ranked], ["cancel", "transition"]
        )
        self.assertEqual(
            [reason.code for reason in ranked[0].reasons],
            ["explicit_symbol", "changed_since_base", "risk_cancellation"],
        )

    def test_qualified_methods_and_free_functions_receive_symbol_priority(
        self,
    ) -> None:
        snapshot = RepositorySnapshot(
            root=Path("/repo"),
            head="abc",
            branch="feature",
            dirty_paths=(),
            base_paths=(),
            recent_paths=(),
        )
        candidates = [
            Candidate(
                "crates/hoimin-core/src/machine.rs",
                "<hoimin_core::machine::RunState as StateMachine>::accept_completion",
                None,
            ),
            Candidate(
                "crates/hoimin-core/src/lib.rs", "free_function", None
            ),
        ]

        ranked = rank_candidates(
            candidates,
            snapshot,
            explicit_symbols=(
                "accept_completion",
                "free_function",
            ),
        )

        self.assertEqual(
            {item.symbol for item in ranked},
            {
                "<hoimin_core::machine::RunState as StateMachine>::accept_completion",
                "free_function",
            },
        )
        self.assertTrue(
            all(
                "explicit_symbol" in [reason.code for reason in item.reasons]
                for item in ranked
            )
        )

    def test_repository_discovery_uses_bounded_read_only_git_commands(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            repository = Path(directory).resolve()
            probe = FakeProbe(
                {
                    ("git", "rev-parse", "--show-toplevel"): f"{repository}\n",
                    ("git", "rev-parse", "HEAD"): "abc\n",
                    ("git", "branch", "--show-current"): "feature\n",
                    (
                        "git",
                        "status",
                        "--porcelain=v1",
                        "-z",
                        "--untracked-files=all",
                    ): " M crates/core/src/dirty.rs\0D  crates/core/src/gone.rs\0",
                    (
                        "git",
                        "diff",
                        "--name-only",
                        "-z",
                        "origin/main...HEAD",
                        "--",
                        "*.rs",
                    ): "crates/core/src/base.rs\0",
                    (
                        "git",
                        "log",
                        "--first-parent",
                        "-20",
                        "--name-only",
                        "-z",
                        "--format=",
                    ): "crates/core/src/recent.rs\0",
                }
            )

            timeouts = iter((12.0, 11.0, 10.0, 9.0, 8.0, 7.0))
            snapshot = discover_repository(
                repository, "origin/main", probe, lambda: next(timeouts)
            )

            self.assertEqual(snapshot.root, repository)
            self.assertEqual(snapshot.dirty_paths, ("crates/core/src/dirty.rs",))
            self.assertEqual(snapshot.base_paths, ("crates/core/src/base.rs",))
            self.assertEqual(snapshot.recent_paths, ("crates/core/src/recent.rs",))
            self.assertEqual(
                probe.calls,
                [
                    (("git", "rev-parse", "--show-toplevel"), 12.0),
                    (("git", "rev-parse", "HEAD"), 11.0),
                    (("git", "branch", "--show-current"), 10.0),
                    ((
                        "git", "status", "--porcelain=v1", "-z",
                        "--untracked-files=all",
                    ), 9.0),
                    ((
                        "git", "diff", "--name-only", "-z",
                        "origin/main...HEAD", "--", "*.rs",
                    ), 8.0),
                    ((
                        "git", "log", "--first-parent", "-20",
                        "--name-only", "-z", "--format=",
                    ), 7.0),
                ],
            )

    def test_candidate_discovery_filters_and_deduplicates_before_ten(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            valid_paths = [
                "crates/core/src/explicit.rs",
                "crates/core/src/dirty.rs",
                "crates/core/src/base.rs",
                *[
                    f"crates/core/src/recent_{number}.rs"
                    for number in range(1, 12)
                ],
            ]
            for path in valid_paths:
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(
                    "pub fn shared() {}\nfn unique_name() {}\n",
                    encoding="utf-8",
                )
            snapshot = RepositorySnapshot(
                root=root,
                head="abc",
                branch="feature",
                dirty_paths=(
                    "crates/core/src/dirty.rs",
                    "tests/ignored.rs",
                    "target/generated.rs",
                    ".worktrees/other.rs",
                    ".idea/settings.rs",
                    "crates/core/src/not_rust.txt",
                ),
                base_paths=("crates/core/src/base.rs",),
                recent_paths=tuple(
                    f"crates/core/src/recent_{number}.rs"
                    for number in range(1, 12)
                ),
            )

            candidates = discover_candidates(
                snapshot,
                explicit_files=("crates/core/src/explicit.rs",),
                explicit_symbols=(),
                probe=FakeProbe({}),
                timeout=lambda: 10.0,
            )

        self.assertEqual(len(candidates), 6)
        self.assertEqual(candidates[0].path, "crates/core/src/explicit.rs")
        self.assertFalse(
            any(item.path.startswith("crates/core/src/recent_") for item in candidates)
        )
        self.assertEqual(
            len(
                {
                    (item.path, item.symbol, item.mutant_name)
                    for item in candidates
                }
            ),
            len(candidates),
        )

    def test_invalid_repository_relative_paths_are_rejected(self) -> None:
        snapshot = RepositorySnapshot(
            root=Path("/repo"),
            head="abc",
            branch="feature",
            dirty_paths=("../escape.rs", "/absolute.rs"),
            base_paths=(),
            recent_paths=(),
        )

        with self.assertRaises(ValueError):
            discover_candidates(
                snapshot, (), (), FakeProbe({}), lambda: 10.0
            )

    def test_explicit_symbol_must_resolve_to_exactly_one_function(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("a.rs", "b.rs"):
                path = root / "crates/core/src" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fn duplicate() {}\n", encoding="utf-8")
            snapshot = RepositorySnapshot(root, "abc", "feature", (), (), ())
            inventory = "\n".join(
                str(root / "crates/core/src" / name)
                for name in ("a.rs", "b.rs")
            )
            probe = FakeProbe({
                (
                    "rg", "--files", "--glob", "*.rs", str(root)
                ): inventory
            })

            with self.assertRaisesRegex(ValueError, "resolved to 2 functions"):
                discover_candidates(
                    snapshot, (), ("duplicate",), probe, lambda: 10.0
                )
            with self.assertRaisesRegex(ValueError, "resolved to 0 functions"):
                discover_candidates(
                    snapshot, (), ("missing",), probe, lambda: 10.0
                )

    def test_preliminary_selection_stops_at_configured_cap(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "crates/core/src/lib.rs"
            path.parent.mkdir(parents=True)
            path.write_text("fn a() {}\nfn b() {}\n", encoding="utf-8")
            snapshot = RepositorySnapshot(
                root, "abc", "feature", ("crates/core/src/lib.rs",), (), ()
            )

            with self.assertRaisesRegex(ValueError, "exceed 1"):
                discover_candidates(
                    snapshot,
                    (),
                    (),
                    FakeProbe({}),
                    lambda: 10.0,
                    max_candidates=1,
                )

    def test_ranking_uses_explicit_risk_signals_and_stable_ties(self) -> None:
        snapshot = RepositorySnapshot(
            root=Path("/repo"),
            head="abc",
            branch="feature",
            dirty_paths=("crates/core/src/z.rs",),
            base_paths=(),
            recent_paths=("crates/core/src/a.rs",),
        )
        candidates = [
            Candidate("crates/core/src/z.rs", "ordinary", "if error"),
            Candidate("crates/core/src/a.rs", "resume_timeout", None),
            Candidate("crates/core/src/b.rs", "resource_limit", None),
        ]

        ranked = rank_candidates(
            candidates,
            snapshot,
            explicit_files=("crates/core/src/b.rs",),
        )

        self.assertEqual(
            [item.path for item in ranked],
            [
                "crates/core/src/b.rs",
                "crates/core/src/z.rs",
                "crates/core/src/a.rs",
            ],
        )
        self.assertEqual(
            [reason.code for reason in ranked[1].reasons],
            ["dirty_worktree", "conditional_or_error_path"],
        )
        self.assertEqual(ranked[1].score, 530)


if __name__ == "__main__":
    unittest.main()
