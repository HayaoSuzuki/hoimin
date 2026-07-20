from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SKILLS = {
    "hoimin-mutation-testing": {
        "description": (
            "Use when developing or changing Python code and automated tests, "
            "and hoimin mutation testing can expose missing behavioral coverage."
        ),
        "required_blocks": (
            "2. Run the normal test command first. If it fails, repair or report "
            "the failure before mutation testing.",
            "3. Target production code, never the test module. Use `--source <dir> "
            "--changed` when a source root is known; otherwise use `--file <path>`. "
            "Narrow a large target with `--line` or `--symbol`.",
            "hoimin run --root . --source <dir> --changed --profile focused "
            "--format json -- python -m pytest -q",
            "| `1` | Complete; survivor exists | Keep the report and investigate a "
            "survivor. |\n"
            "| `2` | Configuration or infrastructure error | Fix or report it; do "
            "not add tests yet. |\n"
            "| `3` | Baseline failed | Repair the normal test failure first. |\n"
            "| `4` | Incomplete run | Resolve the limit, timeout, or interruption "
            "first. |\n"
            "| `130` | Cancelled | Report cancellation; do not interpret partial data. |",
            "For exit codes `2`, `3`, `4`, or `130`, stop the mutation-test "
            "workflow and diagnose or resolve the condition before adding tests "
            "or scoring results.",
        ),
    },
    "hoimin-mutation-improvement": {
        "description": (
            "Use when iterating on hoimin mutation-test survivors and test improvements "
            "until the current target's progress is saturated or complete."
        ),
        "required_blocks": (
            "2. Create a temporary directory outside the repository. Save every "
            "complete `hoimin run --format json` result there in oldest-to-newest "
            "order.",
            "3. Run the normal test command before each mutation run. If it fails, "
            "repair the normal test failure before collecting or scoring another "
            "mutation report. Do not use a report whose baseline failed or whose run "
            "is incomplete in the progress history.",
            "4. Default to `--profile focused`. Do not use persistent reports or "
            "`--session` / `--resume` unless the user requests them.",
            "hoimin progress --format json report-001.json report-002.json",
            "Read `latest.state` and `latest.consecutive_stalls` from the JSON "
            "result. Decide from `latest.state`, **終了コードではなく**. Use "
            "`latest.consecutive_stalls` to report and confirm the default patience; "
            "`latest.state` remains the control signal.",
            "| `improving` | Progress reset the stall count. Select one remaining "
            "survivor and continue. |\n"
            "| `stalled` | Try one more focused behavioral-test improvement; it has "
            "not yet reached patience. |\n"
            "| `saturated` | Stop. Confirm `latest.consecutive_stalls` has reached "
            "the default three comparable stalls; report residual survivors, "
            "attempted contracts, and this stop reason. |\n"
            "| `regressing` | Stop and diagnose the previous test change or target drift. "
            "Do not hide regression by adding another test. |\n"
            "| `indeterminate` | Repair the baseline, incomplete run, or changed "
            "selection and rebuild a comparable history. Do not count it as a stall. |",
            "State the production target, test argv, report count, final "
            "`latest.state` and `latest.consecutive_stalls`, tests added or "
            "strengthened, and unresolved survivors with their rationale.",
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
            for block in contract["required_blocks"]:
                with self.subTest(name=name, block=block):
                    self.assertIn(block, body)


if __name__ == "__main__":
    unittest.main()
