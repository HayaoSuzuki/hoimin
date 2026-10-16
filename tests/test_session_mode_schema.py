"""Check the public normalized session contract with a full Schema validator."""

import json
from pathlib import Path

from hypothesis import given
from hypothesis import strategies as st
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads(
    (ROOT / "docs/json-schema/run-event.schema.json").read_text(encoding="utf-8")
)
VALIDATOR = Draft202012Validator(SCHEMA)
JSON_VALUES = st.recursive(
    st.none() | st.booleans() | st.integers() | st.text(max_size=32),
    lambda children: (
        st.lists(children, max_size=4)
        | st.dictionaries(st.text(max_size=8), children, max_size=4)
    ),
    max_leaves=12,
)


@given(value=JSON_VALUES)
def test_normalized_config_rejects_obsolete_resume_field(value: object) -> None:
    # Arrange
    source = ROOT / "crates/hoimin-cli/tests/golden/events/schema-v7-current.jsonl"
    event = json.loads(source.read_text(encoding="utf-8").splitlines()[0])
    event["normalized_config"]["resume"] = value

    # Act
    errors = list(VALIDATOR.iter_errors(event))

    # Assert
    assert errors


@given(value=JSON_VALUES)
def test_normalized_config_rejects_removed_outer_root(value: object) -> None:
    source = ROOT / "crates/hoimin-cli/tests/golden/events/schema-v7-current.jsonl"
    event = json.loads(source.read_text(encoding="utf-8").splitlines()[0])
    event["normalized_config"]["root"] = value

    errors = list(VALIDATOR.iter_errors(event))

    assert errors
