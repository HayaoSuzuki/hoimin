# ruff: noqa: INP001
# Standalone audit script/test, not an importable package.
"""Focused validation of the one-off measurement fixture/output contract."""

import importlib.util
import json
from pathlib import Path

import pytest

SPEC = importlib.util.spec_from_file_location(
    "issue1005_measure", Path(__file__).with_name("measure.py")
)
assert SPEC is not None
assert SPEC.loader is not None
measure = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(measure)


@pytest.mark.parametrize(
    ("n", "expected"), [(16, 17695), (32, 70127), (64, 279439), (128, 1115855)]
)
def test_fixture_pair_preserves_file_and_byte_counts(
    tmp_path: Path, n: int, expected: int
) -> None:
    # Arrange.
    measure.fixtures(tmp_path)
    rows = json.loads((tmp_path / "fixtures.json").read_text())
    pair = [row for row in rows if row["n"] == n]
    # Act.
    measure.verify_fixtures(tmp_path)
    # Assert: literal issue counts, with only import-source names differing.
    assert [row["source_bytes"] for row in pair] == [expected, expected]
    assert [len(row["files"]) for row in pair] == [3 * n + 1, 3 * n + 1]
    assert [row["graph_edges"] for row in pair] == [2 * n * n, 0]
    assert (
        (tmp_path / "fixtures" / f"control-{n}" / "a000.py")
        .read_text()
        .startswith("from x000 import")
    )
    assert (
        (tmp_path / "fixtures" / f"control-{n}" / "m000.py")
        .read_text()
        .startswith("from y000 import")
    )


@pytest.mark.parametrize(
    ("field", "value"),
    [("candidates", []), ("diagnostics", ["unexpected"]), ("truncated", True)],
)
def test_output_validator_rejects_semantic_mismatch(field: str, value: object) -> None:
    # Arrange.
    document: dict[str, object] = {
        "candidates": [{"id": "one"}],
        "diagnostics": [],
        "truncated": False,
    }
    document[field] = value
    # Act / Assert.
    with pytest.raises(AssertionError):
        measure.validate(document)


def test_fixture_manifest_detects_source_change(tmp_path: Path) -> None:
    # Arrange.
    measure.fixtures(tmp_path)
    (tmp_path / "fixtures/dag-16/target.py").write_text("changed = 1\n")
    # Act / Assert.
    with pytest.raises(AssertionError):
        measure.verify_fixtures(tmp_path)
