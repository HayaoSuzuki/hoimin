from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


class FocusedMutationDocumentationTests(unittest.TestCase):
    def test_development_guide_documents_bounded_workflow(self) -> None:
        text = (ROOT / "docs" / "development.md").read_text(encoding="utf-8")
        for required in (
            "tools/focused_mutation.py",
            "--budget 30m",
            "--output /tmp/hoimin-focused-run",
            "run.json",
            "report.md",
            "budget_exhausted",
            "survivor is not proof of a bug",
            "cargo mutants --workspace",
            "do not use `--iterate` for the required final inventory",
        ):
            with self.subTest(required=required):
                self.assertIn(required, text)

    def test_development_guide_documents_artifact_lifecycle(self) -> None:
        text = (ROOT / "docs" / "development.md").read_text(encoding="utf-8")
        normalized = " ".join(text.split())
        for required in (
            "`run.json` is checkpointed throughout",
            "`report.md` is generated or refreshed during finalization",
            "may be absent after an abrupt unhandled process termination",
        ):
            with self.subTest(required=required):
                self.assertIn(required, normalized)
