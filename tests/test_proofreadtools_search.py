from pathlib import Path

import pytest

from tools import proofreadtools_search as search

ORIGINAL = (
    "def _length_ratio_upper_bound(x: str, y: str) -> float:\n"
    "    return 2 * min(len(x), len(y)) / (len(x) + len(y))\n"
)


def test_witness_is_valid_and_reproduces_difference() -> None:
    mutant = ORIGINAL.replace("min(len(x), len(y))", "len(x)")
    result = search.find_witness(ORIGINAL, mutant, 790, 2.0)
    assert result["status"] == "witness"
    x, y = result["args"]
    assert 1 <= len(x) <= 32
    assert 1 <= len(y) <= 32
    assert result["original"] != result["mutant"]
    code = search.render_regression((x, y), result["original"])
    assert "from proofreadtools.checks._variant_candidates import" in code
    assert "assert" in code


def test_no_difference_is_not_an_equivalence_proof() -> None:
    assert (
        search.find_witness(ORIGINAL, ORIGINAL, 790, 0.2)["status"]
        == "no_witness_within_budget"
    )


@pytest.mark.parametrize("expression", ["1 / 0", "float('nan')", "'different type'"])
def test_invalid_observation_is_not_a_witness(expression: str) -> None:
    mutant = (
        "def _length_ratio_upper_bound(x: str, y: str) -> float:\n    return "
        + expression
        + "\n"
    )
    assert search.find_witness(ORIGINAL, mutant, 790, 0.2)["status"] != "witness"


def test_output_does_not_overwrite_existing_file(tmp_path: Path) -> None:
    output = tmp_path / "evidence.json"
    output.write_text("sentinel", encoding="utf-8")
    with pytest.raises(FileExistsError):
        search.write_evidence(output, {"status": "witness"})
    assert output.read_text(encoding="utf-8") == "sentinel"
