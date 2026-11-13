"""Verify each maintenance command exposes its parser as a Python module."""

import subprocess
import sys
from pathlib import Path

import pytest


@pytest.mark.parametrize(
    "module",
    [
        "boundary_contracts",
        "capture_sbom",
        "ci_metrics",
        "ci_selection",
        "docs_site",
        "knowledge_references",
        "performance_shapes",
        "pypi_provenance",
        "pypi_release",
        "release",
        "sbom",
    ],
)
def test_tool_module_help_exits_without_executing_work(module: str) -> None:
    result = subprocess.run(  # noqa: S603 -- Fixed local maintenance module names.
        [sys.executable, "-m", f"tools.{module}", "--help"],
        cwd=Path(__file__).resolve().parents[1],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
        timeout=30,
    )
    assert result.returncode == 0, result.stderr
    assert "usage:" in result.stdout.lower()
    assert not result.stderr
