from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
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

        self.assertIn("cargo test --workspace", job_block(workflow, "rust"))
        self.assertIn("  rust-shuffle:\n", workflow)
        shuffle = job_block(workflow, "rust-shuffle")
        self.assertIn("needs: quality", shuffle)
        self.assertIn("runs-on: ubuntu-latest", shuffle)
        self.assertIn("nightly-2026-07-27", shuffle)
        self.assertIn("uv sync --frozen", shuffle)
        self.assertIn(
            "cargo +nightly-2026-07-27 test --workspace -- "
            "-Z unstable-options --shuffle",
            shuffle,
        )

    def test_development_guide_documents_seed_replay(self) -> None:
        guide = DEVELOPMENT_GUIDE.read_text(encoding="utf-8")

        self.assertIn("--shuffle", guide)
        self.assertIn("--shuffle-seed <SEED>", guide)
        self.assertIn("nightly-2026-07-27", guide)
