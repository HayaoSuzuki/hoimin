"""Check changed-selection bounds and saved-format contracts in the public schema."""

import copy
import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
EVENT_SCHEMA = json.loads(
    (ROOT / "docs/json-schema/run-event.schema.json").read_text(encoding="utf-8")
)
VALIDATOR = Draft202012Validator(EVENT_SCHEMA)
EVENT = json.loads(
    (ROOT / "crates/hoimin-cli/tests/golden/events/schema-v8-current.jsonl")
    .read_text(encoding="utf-8")
    .splitlines()[0]
)


@pytest.mark.parametrize(
    "changed",
    [None, {"context": 0}, {"context": 1073741823, "diff_base": "main"}],
)
def test_current_event_accepts_typed_changed_selection(changed: object) -> None:
    # Arrange.
    event = copy.deepcopy(EVENT)
    event["schema_version"] = 8
    selection = event["normalized_config"]["selection"]
    selection["changed"] = changed

    # Act.
    errors = list(VALIDATOR.iter_errors(event))

    # Assert.
    assert not errors


@pytest.mark.parametrize(
    "changed",
    [
        True,
        False,
        {},
        {"context": -1},
        {"context": 1073741824},
        {"context": 0, "extra": 1},
    ],
)
def test_current_event_rejects_invalid_changed_selection(changed: object) -> None:
    # Arrange.
    event = copy.deepcopy(EVENT)
    event["normalized_config"]["selection"]["changed"] = changed

    # Act.
    errors = list(VALIDATOR.iter_errors(event))

    # Assert.
    assert errors


@pytest.mark.parametrize("field", ["changed_context", "diff_base"])
def test_current_event_rejects_obsolete_selection_siblings(field: str) -> None:
    # Arrange.
    event = copy.deepcopy(EVENT)
    event["normalized_config"]["selection"][field] = None

    # Act.
    errors = list(VALIDATOR.iter_errors(event))

    # Assert.
    assert errors
