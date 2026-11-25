"""Infrastructure failure must never count as detection of a model defect."""

import json
from pathlib import Path

import pytest

from tools.verify_quint import canonical_trace, validate_result


def test_canonical_trace_preserves_model_state(tmp_path: Path) -> None:
    # Arrange.
    path = tmp_path / "trace.json"
    path.write_text(
        json.dumps(
            {
                "#meta": {"timestamp": 99, "source": "/host/spec.qnt", "status": "ok"},
                "vars": ["amount"],
                "states": [{"amount": {"#bigint": "3"}}],
            }
        )
    )
    # Act.
    result = json.loads(canonical_trace(path))
    # Assert.
    assert result == {
        "#meta": {"format": "ITF", "status": "ok"},
        "vars": ["amount"],
        "states": [{"amount": {"#bigint": "3"}}],
    }


@pytest.mark.parametrize("code", [0, 2, 124, 125, 126, -9])
def test_negative_control_rejects_non_violation_exit(code: int) -> None:
    # Arrange.
    output = "[violation] found a counterexample"
    # Act / Assert.
    with pytest.raises(RuntimeError, match="counterexample"):
        validate_result(code, output, counterexample=True)


@pytest.mark.parametrize("output", ["", "[failure] TLC error", "java not found"])
def test_negative_control_rejects_infrastructure_failure(output: str) -> None:
    # Arrange.
    code = 1
    # Act / Assert.
    with pytest.raises(RuntimeError, match="counterexample"):
        validate_result(code, output, counterexample=True)


def test_negative_control_accepts_actual_violation() -> None:
    # Arrange.
    output = "[violation] found a counterexample"
    # Act.
    result = validate_result(1, output, counterexample=True)
    # Assert.
    assert result is None


@pytest.mark.parametrize("code", [1, 124, 125, 126])
def test_positive_model_rejects_failed_check(code: int) -> None:
    # Arrange.
    output = "[ok] No violation found"
    # Act / Assert.
    with pytest.raises(RuntimeError, match="verification"):
        validate_result(code, output, counterexample=False)
