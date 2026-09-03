from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]


class FocusedMutationDocumentationTests(unittest.TestCase):
    def test_disk_guard_oracle_has_a_python_correspondence_test(self) -> None:
        source = (ROOT / "tests/test_focused_mutation_disk_oracle.py").read_text(
            encoding="utf-8"
        )
        self.assertIn("disk-guard-lifecycle.jsonl", source)
        self.assertIn("implementation_targets", source)
        self.assertIn("OracleResultKind.INFRASTRUCTURE", source)

    def test_development_guide_documents_bounded_workflow(self) -> None:
        text = (ROOT / "docs" / "development.md").read_text(encoding="utf-8")
        self.assertNotIn(
            "uv run hoimin run --root . --file crates/hoimin-cli/src/analyzer/mod.rs",
            text,
        )
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
            "`run.json` is checkpointed after setup",
            "`run.json` remains the recoverable machine-readable source of truth",
            "may contain only `.hoimin-output-owner`",
            "Recover from `run.json` only when it exists",
            "`report.md` is generated or refreshed during finalization",
            "may be absent after an abrupt unhandled process termination",
        ):
            with self.subTest(required=required):
                self.assertIn(required, normalized)

    def test_disk_safe_operation_contract_is_documented(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        development = (ROOT / "docs" / "development.md").read_text(
            encoding="utf-8"
        )
        normalized = " ".join(f"{readme}\n{development}".split())
        for required in (
            "`--max-workspace-size` | `8GiB`",
            "`--min-free-space` | `10GiB`",
            "generated workspace bytes",
            "copied source bytes",
            "250 ms post-scan delay",
            "5.25 seconds",
            "one blocking filesystem syscall",
            "`--jobs 1`",
            "maximum is four",
            "`owned:scratch`",
            "`owned:output`",
            "`capacity_only:cargo_home`",
            "250,000 tree entries",
            "10,000 inventory entries",
            "100,000 JSON nodes",
            "1,000 selectors and selected candidates",
            "24-hour heartbeat grace",
            "attempts a heartbeat refresh after 60 elapsed seconds",
            "50,000 entries or 5 seconds",
            "60-second owner cleanup",
            "30-second janitor",
            "64 MiB command-log hard maximum",
            "16 KiB selector, candidate diagnostic, and encoded-path caps",
            "16 MiB run-diagnostic cap",
            "32 MiB report cap",
            "Cargo home is capacity-only",
            "Deferred cleanup is incomplete",
            "shared Cargo home is never traversed or deleted",
            "Windows filesystem safety",
            "pinned Win32 directory handles",
            "NT handle-relative child opens, rename, and delete operations",
            "Hoimin never falls back to pathname-based recursive deletion",
            "current token user plus SYSTEM and Administrators protected ACL",
            "`3` means `budget_exhausted`",
            "deletes command spools after bounded extraction",
            "A spool cleanup failure is terminal",
            "isolated volume with a hard capacity",
            "a separate 10 GiB host reserve",
            "one mutation worker",
            "removal of that exact volume after evidence is copied out",
            "does not currently provide a supported complete-inventory command",
            "Treat complete inventory as unavailable",
        ):
            with self.subTest(required=required):
                self.assertIn(required, normalized)
        for obsolete in (
            "Windows native disk-safety adapter remains unfinished",
            "follow-up handoff",
            "fails closed on Windows before mutation setup",
        ):
            with self.subTest(obsolete=obsolete):
                self.assertNotIn(obsolete, normalized)

    def test_safe_example_uses_a_fresh_output_and_one_worker(self) -> None:
        development = (ROOT / "docs" / "development.md").read_text(
            encoding="utf-8"
        )
        safe_example = """test -x .venv/bin/python
hoimin_python="$(pwd -P)/.venv/bin/python"
hoimin run --file tools/focused_mutation_support/disk.py --allow-best-effort-memory --max-workspace-size 8GiB --min-free-space 10GiB -- "$hoimin_python" -m unittest tests.test_focused_mutation_disk
python3 -c 'import shutil, sys; sys.exit(0 if shutil.disk_usage(".").free > 10 * 1024**3 else 1)'
mutation_output="$(mktemp -d /tmp/hoimin-focused.XXXXXX)"
.venv/bin/python tools/focused_mutation.py \\
  --budget 30m --jobs 1 --max-disk 8GiB --min-free-space 10GiB \\
  --max-log-size 16MiB --output "$mutation_output"""
        self.assertIn(safe_example, development)
        self.assertNotRegex(
            development,
            r"uv run(?: --frozen)? python tools/focused_mutation\.py",
        )
        combined = "\n".join(
            (ROOT / path).read_text(encoding="utf-8")
            for path in ("README.md", "docs/development.md")
        )
        self.assertIsNone(
            re.search(
                r"cargo mutants --workspace --jobs ([2-9]|[1-9][0-9]+)",
                combined,
            )
        )
        self.assertNotRegex(combined, r"(?m)^cargo mutants --workspace(?:\s|$)")
