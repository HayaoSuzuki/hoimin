"""Validate the single progress output contract with a full Schema validator."""

import copy
import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads(
    (ROOT / "docs/json-schema/progress-result.schema.json").read_text(encoding="utf-8")
)
VALIDATOR = Draft202012Validator(SCHEMA)
GOLDEN = json.loads(
    (ROOT / "crates/hoimin-cli/tests/golden/progress/schema-v3-current.json").read_text(
        encoding="utf-8"
    )
)
DETAILS = {
    "scope": "latest_pair",
    "previous_input": 0,
    "current_input": 1,
    "available": True,
    "eligibility": "matching",
    "limit": 0,
    "omitted": 1,
    "unidentified": 0,
    "transitions": [],
}


@pytest.mark.parametrize(
    "details",
    [
        None,
        DETAILS,
        {**DETAILS, "limit": 100, "omitted": 0},
        {**DETAILS, "available": False, "eligibility": None, "omitted": 0},
    ],
    ids=["not-requested", "limit-zero", "available", "unavailable"],
)
def test_progress_accepts_current_details_states(details: object) -> None:
    # Arrange
    document = copy.deepcopy(GOLDEN)
    document["details"] = details

    # Act
    errors = list(VALIDATOR.iter_errors(document))

    # Assert
    assert not errors


@pytest.mark.parametrize("version", [0, 1, 2, 4])
def test_progress_rejects_noncurrent_versions(version: int) -> None:
    document = {**GOLDEN, "schema_version": version}

    errors = list(VALIDATOR.iter_errors(document))

    assert any(list(error.path) == ["schema_version"] for error in errors)


def test_progress_requires_explicit_details() -> None:
    document = {key: value for key, value in GOLDEN.items() if key != "details"}

    errors = list(VALIDATOR.iter_errors(document))

    assert any(error.validator == "required" for error in errors)


@pytest.mark.parametrize(
    "details",
    [
        True,
        [],
        "unavailable",
        {},
        {**DETAILS, "unknown": True},
        {**DETAILS, "limit": -1},
    ],
)
def test_progress_rejects_malformed_details(details: object) -> None:
    document = {**GOLDEN, "details": details}

    errors = list(VALIDATOR.iter_errors(document))

    assert errors
