from __future__ import annotations

import pytest

from tools import nextest_evaluation as probe


def test_descendants_exclude_unrelated_processes_and_include_nested_children() -> None:
    rows = [(10, 1, 100), (11, 10, 200), (12, 11, 300), (13, 1, 400)]
    assert probe.tree_usage(rows, 10) == (3, 600)


def test_inventory_rejects_different_binary_with_same_test_names() -> None:
    with pytest.raises(AssertionError, match="inventory"):
        probe.check_inventory(
            {("pkg", "lib", "one", "a", False)}, {("pkg", "test", "one", "a", False)}
        )


def test_inventory_rejects_changed_ignore_status() -> None:
    with pytest.raises(AssertionError, match="inventory"):
        probe.check_inventory(
            {("pkg", "lib", "one", "a", False)}, {("pkg", "lib", "one", "a", True)}
        )


def test_inventory_rejects_missing_test() -> None:
    with pytest.raises(AssertionError, match="inventory"):
        probe.check_inventory({("pkg", "lib", "one", "a", False)}, set())


def test_inventory_accepts_identical_rows() -> None:
    rows = {("pkg", "lib", "one", "a", False)}
    probe.check_inventory(rows, rows)
