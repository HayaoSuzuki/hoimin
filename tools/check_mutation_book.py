"""Verify the downloaded workshop, coverage, and representative test gaps."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from zipfile import ZipFile

from tools.build_mutation_book_assets import ARCHIVE, EXAMPLE, FILES

EXPECTED_STATEMENTS = 15
EXPECTED_BRANCHES = 12


def pytest(root: Path, *targets: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(  # noqa: S603 -- Fixed argv runs this interpreter's pytest.
        [sys.executable, "-m", "pytest", "-q", "--tb=short", *targets],
        cwd=root,
        env={**os.environ, "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1"},
        capture_output=True,
        text=True,
        check=False,
    )


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="hoimin-book-check-") as temporary:
        root = Path(temporary) / "shipping-workshop"
        with ZipFile(ARCHIVE) as archive:
            expected = {f"shipping-workshop/{relative}" for relative in FILES}
            assert set(archive.namelist()) == expected, "Unexpected archive contents"
            for relative in FILES:
                assert (
                    archive.read(f"shipping-workshop/{relative}")
                    == (EXAMPLE / relative).read_bytes()
                ), f"Stale archive: {relative}"
            archive.extractall(
                temporary
            )  # Entries were checked against the exact list.

        environment = {**os.environ, "PYTEST_DISABLE_PLUGIN_AUTOLOAD": "1"}
        coverage = subprocess.run(
            [
                sys.executable,
                "-m",
                "coverage",
                "run",
                "--branch",
                "--source=shipping",
                "-m",
                "pytest",
                "-q",
            ],
            cwd=root,
            env=environment,
            capture_output=True,
            text=True,
            check=True,
        )
        print(coverage.stdout.strip())  # noqa: T201 -- CLI validation output.
        subprocess.run(
            [sys.executable, "-m", "coverage", "json", "-o", "coverage.json"],
            cwd=root,
            capture_output=True,
            check=True,
        )
        totals = json.loads((root / "coverage.json").read_text())["totals"]
        assert (
            totals["num_statements"] == totals["covered_lines"] == EXPECTED_STATEMENTS
        )
        assert totals["num_branches"] == totals["covered_branches"] == EXPECTED_BRANCHES
        assert totals["missing_lines"] == totals["missing_branches"] == 0

        improved = pytest(root, "tests", "solutions")
        assert improved.returncode == 0, improved.stdout + improved.stderr
        print(improved.stdout.strip())  # noqa: T201 -- CLI validation output.

        source = (root / "shipping.py").read_text(encoding="utf-8")
        mutations = (
            ("free_boundary", "subtotal >= 5000", "subtotal > 5000", True),
            ("weight_boundary", "weight > 2000", "weight >= 2000", True),
            ("member_discount", "fee - 200", "fee + 200", True),
            ("remote_charge", "fee += 400", "fee += 401", True),
            (
                "empty_order",
                "subtotal == 0 and weight == 0",
                "subtotal == 0 or weight == 0",
                True,
            ),
            ("equivalent_floor", "max(0, fee - 200)", "fee - 200", False),
            (
                "permitted_message",
                "subtotal and weight must be nonnegative",
                "negative input",
                False,
            ),
        )
        results = []
        for name, before, after, should_detect in mutations:
            assert source.count(before) == 1, (name, before)
            (root / "shipping.py").write_text(
                source.replace(before, after), encoding="utf-8"
            )
            # Avoid stale bytecode for equal-length edits within one timestamp tick.
            for bytecode in root.rglob("*.pyc"):
                bytecode.unlink()
            initial = pytest(root, "tests")
            assert initial.returncode == 0, (name, initial.stdout, initial.stderr)
            after_run = pytest(root, "tests", "solutions")
            if should_detect:
                assert after_run.returncode == 1, (
                    name,
                    after_run.stdout,
                    after_run.stderr,
                )
                assert "assert " in after_run.stdout, after_run.stdout
                assert "ERROR" not in after_run.stdout, after_run.stdout
            else:
                assert after_run.returncode == 0, (
                    name,
                    after_run.stdout,
                    after_run.stderr,
                )
            results.append(
                {
                    "mutation": name,
                    "initial": "passed",
                    "improved": "failed" if should_detect else "passed",
                }
            )

        print(  # noqa: T201 -- Machine-readable evidence for the maintainer.
            json.dumps({"coverage": totals, "experiments": results}, indent=2)
        )


if __name__ == "__main__":
    main()
