"""Check the bounded janitor detail omission contract in the public schema."""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT / "docs/json-schema/run-event.schema.json").read_text())
OMISSIONS = SCHEMA["$defs"]["janitorSummary"]["properties"]["detail_omissions"]
VALIDATOR = Draft202012Validator(OMISSIONS)
COUNTERS = {"normal_details": 2, "issue_details": 1, "truncated_details": 3}


def test_janitor_accepts_capacity_and_truncation_counts() -> None:
    assert not list(VALIDATOR.iter_errors(COUNTERS))


def test_janitor_rejects_retired_presentation_suppression_count() -> None:
    omissions = {**COUNTERS, "suppressed_normal_details": 0}
    errors = list(VALIDATOR.iter_errors(omissions))
    assert any(error.validator == "additionalProperties" for error in errors)


@pytest.mark.parametrize("counter", COUNTERS)
def test_janitor_requires_each_capacity_and_truncation_count(counter: str) -> None:
    omissions = {key: value for key, value in COUNTERS.items() if key != counter}
    assert list(VALIDATOR.iter_errors(omissions))


@pytest.mark.parametrize("counter", COUNTERS)
@pytest.mark.parametrize("value", [-1, None, True, 0.5])
def test_janitor_rejects_invalid_omission_counts(counter: str, value: object) -> None:
    omissions = {**COUNTERS, counter: value}
    assert list(VALIDATOR.iter_errors(omissions))
