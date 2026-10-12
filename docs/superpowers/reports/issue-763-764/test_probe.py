# ruff: noqa: INP001 -- Standalone research checks, not an importable package.
from __future__ import annotations

import pytest
from hypothesis import given
from hypothesis import strategies as st
from probe import Edit, apply_edits, relations


@given(
    left=st.sets(st.integers(0, 4)),
    right=st.sets(st.integers(0, 4)),
    combined=st.sets(st.integers(0, 4)),
)
def test_relation_matches_casewise_definition(
    left: set[int], right: set[int], combined: set[int]
) -> None:
    result = relations(left, right, combined)

    assert result == {
        "ssh": bool(combined) and all(t in left and t in right for t in combined),
        "strict": bool(combined)
        and all(t in left and t in right for t in combined)
        and any(t not in combined for t in left & right),
    }


@given(left=st.sets(st.integers()), right=st.sets(st.integers()))
def test_surviving_pair_is_never_a_subsuming_success(
    left: set[int], right: set[int]
) -> None:
    result = relations(left, right, set())

    assert result == {"ssh": False, "strict": False}


@given(replacement=st.text(alphabet="αβabcd", min_size=1, max_size=8))
def test_unicode_edits_are_order_independent(replacement: str) -> None:
    source = "α + β"
    left = Edit(0, 2, "α", replacement, "example")
    right = Edit(5, 7, "β", "long_name", "example")

    forward = apply_edits(source, [left, right])
    backward = apply_edits(source, [right, left])

    assert forward == backward == f"{replacement} + long_name"


def test_overlapping_edits_are_rejected() -> None:
    source = "abc"
    edits = [Edit(0, 2, "ab", "x", "example"), Edit(1, 3, "bc", "y", "example")]

    with pytest.raises(ValueError, match="overlap"):
        apply_edits(source, edits)


def test_stale_source_is_rejected() -> None:
    edits = [Edit(0, 1, "a", "b", "example")]

    with pytest.raises(ValueError, match="source"):
        apply_edits("x", edits)
