"""Failure checks for the isolated corpus experiment, not product tests."""
# ruff: noqa: INP001 -- Standalone research checks beside their script.

import pytest
from experiment import compare_features, read_features
from hypothesis import given
from hypothesis import strategies as st

COMPLETE = "1\n0\ninput\nSTARTED 0 12\nFT 0 1\nCOV 0 7\n"


@given(prefix=st.integers(min_value=0, max_value=len(COMPLETE) - 1))
def test_incomplete_merge_cannot_establish_feature_preservation(
    tmp_path_factory: pytest.TempPathFactory, prefix: int
) -> None:
    tmp_path = tmp_path_factory.mktemp("control")
    control = tmp_path / "partial.control"
    control.write_text(COMPLETE[:prefix], encoding="utf-8")

    with pytest.raises(ValueError, match="incomplete"):
        read_features(control)


def test_equal_counts_do_not_hide_a_missing_feature() -> None:
    original = ({1, 2}, {7})
    compact = ({1, 3}, {7})

    comparison = compare_features(original, compact)

    assert comparison["preserved"] is False
    assert comparison["missing_features"] == [2]
    assert comparison["added_features"] == [3]
