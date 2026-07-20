from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SKILLS = {
    "hoimin-mutation-testing": {
        "description": (
            "Use when developing or changing Python code and automated tests, "
            "and hoimin mutation testing can expose missing behavioral coverage."
        ),
        "phrases": (
            "--profile focused --format json",
            "--source <dir> --changed",
            "`--file <path>`",
            "production code",
            "`1`",
            "survivor",
        ),
    },
    "hoimin-mutation-improvement": {
        "description": (
            "Use when iterating on hoimin mutation-test survivors and test improvements "
            "until the current target's progress is saturated or complete."
        ),
        "phrases": (
            "hoimin progress --format json",
            "latest.state",
            "`improving`",
            "`stalled`",
            "`saturated`",
            "`regressing`",
            "`indeterminate`",
            "終了コードではなく",
        ),
    },
}


class SkillContractTests(unittest.TestCase):
    def test_skill_mirrors_and_required_workflows(self) -> None:
        for name, contract in SKILLS.items():
            codex = ROOT / ".agents" / "skills" / name / "SKILL.md"
            claude = ROOT / ".claude" / "skills" / name / "SKILL.md"
            self.assertTrue(codex.is_file(), codex)
            self.assertTrue(claude.is_file(), claude)
            self.assertEqual(codex.read_bytes(), claude.read_bytes(), name)

            lines = codex.read_text(encoding="utf-8").splitlines()
            self.assertGreaterEqual(len(lines), 4, name)
            self.assertEqual(lines[0], "---", name)
            closing = lines.index("---", 1)
            frontmatter = dict(line.split(": ", 1) for line in lines[1:closing])
            self.assertEqual(
                frontmatter,
                {"name": name, "description": contract["description"]},
                name,
            )
            self.assertTrue(frontmatter["description"].startswith("Use when"), name)
            body = "\n".join(lines[closing + 1 :])
            for phrase in contract["phrases"]:
                self.assertIn(phrase, body, f"{name}: {phrase}")


if __name__ == "__main__":
    unittest.main()
