"""Check mode-specific verification metadata against both current public schemas."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator

if TYPE_CHECKING:
    from jsonschema.protocols import Validator

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture(params=["run-event", "verify-preview"])
def validator(request: pytest.FixtureRequest) -> Validator:
    document = json.loads(
        (ROOT / f"docs/json-schema/{request.param}.schema.json").read_text(
            encoding="utf-8"
        )
    )
    schema = (
        document["$defs"]["verificationSelection"]
        if request.param == "run-event"
        else document["properties"]["verification_selection"]
    )
    return Draft202012Validator(schema)


def selection(mode: str, policy: str, scope: str) -> dict[str, object]:
    return {
        "mode": mode,
        "policy": policy,
        "scope": scope,
        "requested": 3,
        "selected": 2,
        "plan_truncated": False,
    }


@pytest.mark.parametrize(
    ("mode", "scope", "policy"),
    [
        ("candidate_ids", "explicit_candidates", "strict"),
        ("top", "retained_candidates", "explicit_candidates"),
        ("top", "retained_candidates", "splitmix64_fisher_yates_v1"),
    ],
)
def test_selection_rejects_policy_from_another_mode(
    validator: Validator, mode: str, scope: str, policy: str
) -> None:
    # Arrange.
    value = selection(mode, policy, scope)
    # Act.
    errors = list(validator.iter_errors(value))
    # Assert.
    assert errors


@pytest.mark.parametrize(
    ("mode", "policy", "scope"),
    [
        ("candidate_ids", "explicit_candidates", "retained_candidates"),
        ("top", "strict", "sampled_candidates"),
    ],
)
def test_selection_rejects_scope_from_another_mode(
    validator: Validator, mode: str, policy: str, scope: str
) -> None:
    # Arrange.
    value = selection(mode, policy, scope)
    # Act.
    errors = list(validator.iter_errors(value))
    # Assert.
    assert errors


@pytest.mark.parametrize(
    ("mode", "policy", "scope"),
    [
        ("candidate_ids", "explicit_candidates", "explicit_candidates"),
        ("top", "strict", "retained_candidates"),
        ("top", "file_round_robin_v1", "retained_candidates"),
        ("top", "line_round_robin_v1", "retained_candidates"),
    ],
)
def test_selection_accepts_current_non_sample_modes(
    validator: Validator, mode: str, policy: str, scope: str
) -> None:
    # Arrange.
    value = selection(mode, policy, scope)
    # Act.
    errors = list(validator.iter_errors(value))
    # Assert.
    assert not errors
