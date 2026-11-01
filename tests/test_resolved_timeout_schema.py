"""Validate the resolved individual process-cap field in current reports."""

import copy
import json
from pathlib import Path

import pytest
from hypothesis import example, given
from hypothesis import strategies as st
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT / "docs/json-schema/run-event.schema.json").read_text())
VALIDATOR = Draft202012Validator(SCHEMA)
SUMMARY = json.loads(
    (ROOT / "crates/hoimin-cli/tests/golden/events/schema-v10-current.jsonl")
    .read_text()
    .splitlines()[-1]
)


@given(
    value=st.one_of(
        st.none(),
        st.fixed_dictionaries(
            {"secs": st.integers(0, 2**64 - 1), "nanos": st.integers(0, 999_999_999)}
        ),
    )
)
@example(value=None)
@example(value={"secs": 5, "nanos": 0})
@example(value={"secs": 1, "nanos": 234567891})
def test_resolved_timeout_accepts_null_or_precise_duration(value: object) -> None:
    event = copy.deepcopy(SUMMARY)
    event["resolved_mutant_timeout"] = value

    errors = list(VALIDATOR.iter_errors(event))

    assert not errors


@pytest.mark.parametrize(
    "value",
    [
        5,
        "5s",
        {"secs": 1},
        {"secs": 1, "nanos": 1000000000},
        {"secs": -1, "nanos": 0},
        {"secs": 5, "nanos": 0, "unknown": 1},
    ],
)
def test_resolved_timeout_rejects_invalid_wire_shapes(value: object) -> None:
    event = copy.deepcopy(SUMMARY)
    event["resolved_mutant_timeout"] = value

    errors = list(VALIDATOR.iter_errors(event))

    assert errors


def test_resolved_timeout_requires_explicit_null_when_unresolved() -> None:
    event = copy.deepcopy(SUMMARY)
    del event["resolved_mutant_timeout"]

    errors = list(VALIDATOR.iter_errors(event))

    assert errors
