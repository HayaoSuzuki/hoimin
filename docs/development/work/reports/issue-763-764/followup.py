# ruff: noqa: INP001 -- Standalone, unsupported research confirmation.
"""Confirm the post-hoc string-literal comparator against isolated existing tests."""

from __future__ import annotations

import gzip
import hashlib
import json
import shutil
import subprocess
import sys
import time

from probe import HERE, ROOT, Edit, apply_edits

CONFIRMATION_SECONDS = 240


def main() -> None:
    started = time.perf_counter()
    evidence = json.loads(gzip.decompress((HERE / "evidence.json.gz").read_bytes()))
    target = evidence["targets"][1]
    source = target["source"]
    raw = source.encode()
    needle = b'"CONTRIBUTING.md"'
    start = raw.index(needle)
    string_edit = Edit(
        start, start + len(needle), needle.decode(), '""', "string_literal_empty"
    )
    edits = [Edit(**data) for data in target["edits"]]
    variants = {
        "baseline": source,
        "left": apply_edits(source, [edits[4]]),
        "right": apply_edits(source, [edits[5]]),
        "pair": apply_edits(source, [edits[4], edits[5]]),
        "string": apply_edits(source, [string_edit]),
    }
    git = shutil.which("git")
    assert git is not None
    original_tests = subprocess.check_output(  # noqa: S603 -- Resolved git, fixed read-only revision/path.
        [git, "show", "e7c0f98:tests/test_ci_selection.py"], timeout=10
    )
    updated_tests = (ROOT / "tests/test_ci_selection.py").read_bytes()
    full_source = (ROOT / "tools/ci_selection.py").read_text(encoding="utf-8")
    assert full_source.count(source) == 1
    work = ROOT / ".superpowers/issue-763-764-validation"
    logs = HERE / "followup-logs"
    logs.mkdir(exist_ok=True)
    rows = []
    configurations = [
        (
            "original",
            original_tests,
            name,
            0 if name in {"baseline", "pair", "string"} else 1,
        )
        for name in variants
    ]
    configurations.extend(
        ("with_contributing_case", updated_tests, name, 0 if name == "baseline" else 1)
        for name in ("baseline", "pair", "string")
    )
    for tests_name, tests, name, expected_exit in configurations:
        assert time.perf_counter() - started < CONFIRMATION_SECONDS, (
            "confirmation budget exhausted"
        )
        directory = work / f"{tests_name}-{name}"
        (directory / "tools").mkdir(parents=True, exist_ok=True)
        (directory / "tools/ci_selection.py").write_text(
            full_source.replace(source, variants[name]), encoding="utf-8"
        )
        (directory / "test_ci_selection.py").write_bytes(tests)
        (directory / "pytest.ini").write_text(
            "[pytest]\npythonpath = .\n", encoding="utf-8"
        )
        run_started = time.perf_counter()
        result = subprocess.run(
            [sys.executable, "-m", "pytest", "-q", "test_ci_selection.py"],
            cwd=directory,
            capture_output=True,
            timeout=45,
            check=False,
        )
        log = f"{tests_name}-{name}.log"
        (logs / log).write_bytes(result.stdout + result.stderr)
        assert result.returncode == expected_exit, f"unexpected result: {log}"
        rows.append(
            {
                "tests": tests_name,
                "variant": name,
                "exit_code": result.returncode,
                "elapsed_seconds": time.perf_counter() - run_started,
                "log": log,
            }
        )
    result = {
        "classification": "post-hoc comparator, not a preregistered search arm",
        "string_edit": {
            "start": start,
            "original": needle.decode(),
            "replacement": '""',
        },
        "original_tests_sha256": hashlib.sha256(original_tests).hexdigest(),
        "updated_tests_sha256": hashlib.sha256(updated_tests).hexdigest(),
        "rows": rows,
        "elapsed_seconds": time.perf_counter() - started,
    }
    (HERE / "followup.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
