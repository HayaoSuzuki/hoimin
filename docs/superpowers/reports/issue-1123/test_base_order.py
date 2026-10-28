"""Executable contract checks for the fixed-corpus base-order probe."""

import ast
import random
import runpy
import tempfile
from pathlib import Path

import pytest

PROBE = runpy.run_path(str(Path(__file__).with_name("experiment.py")))


def test_check_preserves_source_bytes_on_windows(tmp_path):
    source = "value = 3\nassert value == 3\n"
    assert PROBE["CHECK"](tmp_path, source, "assert value == 3")["verdict"] == "passed"
    assert (tmp_path / "subject.py").read_bytes() == source.encode("ascii")


def bases(source):
    child = next(
        node
        for node in ast.parse(source).body
        if isinstance(node, ast.ClassDef) and node.name == "Child"
    )
    return [ast.unparse(base) for base in child.bases]


def mutate(source, seed=0):
    candidates = PROBE["proposed"](source, "base_order_shuffle", seed=seed)
    assert len(candidates) == 1
    return PROBE["APPLY"](source, candidates[0])


def test_two_bases_swap_without_changing_class_body():
    source = "class Child(A, B):\n    marker = 'unchanged'\n"
    assert mutate(source) == "class Child(B, A):\n    marker = 'unchanged'\n"


@pytest.mark.parametrize("count", range(3, 9))
def test_multi_base_shuffle_is_changed_replayable_and_preserves_bases(count):
    original = [f"Base{i}" for i in range(count)]
    source = f"class Child({', '.join(original)}):\n    pass\n"
    observed = set()
    for seed in range(32):
        result = mutate(source, seed)
        changed = bases(result)
        assert changed != original
        assert sorted(changed) == sorted(original)
        assert result == mutate(source, seed)
        observed.add(tuple(changed))
    assert len(observed) > 1


def test_identity_shuffle_falls_back_to_first_two_swap():
    # Seed 5 leaves three elements unchanged; exercise the fallback without mocks.
    shuffled = ["A", "B", "C"]
    random.Random(5).shuffle(shuffled)
    assert shuffled == ["A", "B", "C"]
    assert bases(mutate("class Child(A, B, C):\n    pass\n", 5)) == ["B", "A", "C"]


def test_candidate_generation_does_not_import_target():
    source = "raise RuntimeError('must not import')\nclass Child(A, B):\n    pass\n"
    assert bases(mutate(source)) == ["B", "A"]


@pytest.mark.parametrize(
    ("name", "weak", "strong"),
    [
        ("base_order_dispatch", "passed", "assertion_failed"),
        ("base_order_super_callback", "passed", "assertion_failed"),
        ("base_order_three_super_callback", "passed", "assertion_failed"),
        ("base_order_commuting_control", "passed", "passed"),
        ("base_order_explicit_call_control", "passed", "passed"),
        ("base_order_mro_risk", "error", "error"),
    ],
)
def test_fixture_verdicts_separate_assertion_gaps_controls_and_mro_errors(
    name, weak, strong
):
    example = next(e for e in PROBE["BASE_ORDER_EXAMPLES"] if e.name == name)
    mutant = mutate(example.source)
    with tempfile.TemporaryDirectory(prefix="base-order-test-") as directory:
        root = Path(directory)
        for check in (example.weak, example.strong):
            assert PROBE["CHECK"](root, example.source, check)["verdict"] == "passed"
        for check, expected in ((example.weak, weak), (example.strong, strong)):
            result = PROBE["CHECK"](root, mutant, check)
            assert result["verdict"] == expected
            if expected == "error":
                assert result["exception"] == "TypeError"
