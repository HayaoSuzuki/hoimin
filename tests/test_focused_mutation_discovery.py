from pathlib import Path
import tempfile
import unittest

from tools.focused_mutation_support.discovery import (
    RepositorySnapshot,
    discover_candidates,
    discover_repository,
)
from tools.focused_mutation_support.model import Candidate
from tools.focused_mutation_support.ranking import rank_candidates


class FakeProbe:
    def __init__(self, replies: dict[tuple[str, ...], str]):
        self.replies = replies
        self.calls: list[tuple[str, ...]] = []

    def text(self, argv: list[str]) -> str:
        key = tuple(argv)
        self.calls.append(key)
        return self.replies[key]


class DiscoveryTests(unittest.TestCase):
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

    def test_repository_discovery_uses_bounded_read_only_git_commands(self) -> None:
        probe = FakeProbe(
            {
                ("git", "rev-parse", "--show-toplevel"): "/repo\n",
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
                    "--format=",
                ): "crates/core/src/recent.rs\n",
            }
        )

        snapshot = discover_repository(Path("/repo"), "origin/main", probe)

        self.assertEqual(snapshot.root, Path("/repo"))
        self.assertEqual(snapshot.dirty_paths, ("crates/core/src/dirty.rs",))
        self.assertEqual(snapshot.base_paths, ("crates/core/src/base.rs",))
        self.assertEqual(snapshot.recent_paths, ("crates/core/src/recent.rs",))
        self.assertEqual(
            probe.calls,
            [
                ("git", "rev-parse", "--show-toplevel"),
                ("git", "rev-parse", "HEAD"),
                ("git", "branch", "--show-current"),
                (
                    "git",
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                ),
                (
                    "git",
                    "diff",
                    "--name-only",
                    "-z",
                    "origin/main...HEAD",
                    "--",
                    "*.rs",
                ),
                (
                    "git",
                    "log",
                    "--first-parent",
                    "-20",
                    "--name-only",
                    "--format=",
                ),
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
            )

        self.assertEqual(len(candidates), 10)
        self.assertEqual(candidates[0].path, "crates/core/src/explicit.rs")
        self.assertNotIn(
            "crates/core/src/recent_3.rs", {item.path for item in candidates}
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
            discover_candidates(snapshot, (), (), FakeProbe({}))

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
