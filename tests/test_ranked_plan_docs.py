import unittest
from pathlib import Path


class RankedPlanDocumentationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.readme = (Path(__file__).resolve().parents[1] / "README.md").read_text(
            encoding="utf-8"
        )

    def test_documents_ranked_two_command_workflow(self) -> None:
        for text in [
            "hoimin verify PLAN.json --top 10",
            "ranking_reasons",
            "ordering heuristics",
            "mutually exclusive",
            "Version-1 manifests",
            "top N among retained candidates",
        ]:
            with self.subTest(text=text):
                self.assertIn(text, self.readme)

    def test_documents_saved_rank_and_oversized_top_semantics(self) -> None:
        for text in [
            "never re-ranks",
            "lower-ranked candidates remain valid",
            "selects every retained candidate",
            "reports the actual selected count",
        ]:
            with self.subTest(text=text):
                self.assertIn(text, self.readme)


if __name__ == "__main__":
    unittest.main()
