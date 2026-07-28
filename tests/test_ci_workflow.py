from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
DEVELOPMENT_GUIDE = ROOT / "docs" / "development.md"


def job_block(workflow: str, job_name: str) -> str:
    marker = f"  {job_name}:\n"
    start = workflow.index(marker)
    following = workflow[start + len(marker) :]
    next_job = re.search(r"^  [a-z0-9-]+:\n", following, re.MULTILINE)
    end = len(workflow) if next_job is None else start + len(marker) + next_job.start()
    return workflow[start:end]


class ShuffleWorkflowContractTests(unittest.TestCase):
    def test_shuffle_job_is_pinned_isolated_and_complete(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")

        stable = job_block(workflow, "rust")
        self.assertIn("matrix:\n        os: [ubuntu-latest, windows-latest, macos-14]", stable)
        self.assertRegex(stable, r"(?m)^      - run: cargo test --workspace$")
        self.assertIn("  rust-shuffle:\n", workflow)
        shuffle = job_block(workflow, "rust-shuffle")
        self.assertRegex(shuffle, r"(?m)^    needs: quality$")
        self.assertRegex(shuffle, r"(?m)^    runs-on: ubuntu-latest$")
        self.assertIn("python-version: '3.14'", shuffle)
        self.assertRegex(
            shuffle,
            r"(?m)^        run: rustup toolchain install "
            r"nightly-2026-07-27 --profile minimal$",
        )
        self.assertRegex(shuffle, r"(?m)^      - run: uv sync --frozen$")
        self.assertRegex(
            shuffle,
            r"(?m)^        run: cargo \+nightly-2026-07-27 test --workspace -- "
            r"-Z unstable-options --shuffle$",
        )

    def test_stable_quality_matrix_and_release_workflow_remain_nightly_free(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        quality = job_block(workflow, "quality")
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn(
            "matrix:\n        os: [ubuntu-latest, windows-latest, macos-14]",
            quality,
        )
        self.assertNotIn("nightly-2026-07-27", release)
        self.assertNotIn("--shuffle", release)

    def test_development_guide_documents_seed_replay(self) -> None:
        guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

        self.assertIn(
            "cargo +nightly-2026-07-27 test --workspace -- "
            "-Z unstable-options --shuffle\n",
            guide,
        )
        self.assertIn(
            "cargo +nightly-2026-07-27 test --workspace -- \\\n"
            "  -Z unstable-options --shuffle-seed <SEED>\n",
            guide,
        )
