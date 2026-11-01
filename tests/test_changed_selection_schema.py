"""Check changed-selection bounds and saved-format contracts in the public schema."""

import copy
import json
from pathlib import Path

import pytest
from hypothesis import example, given
from hypothesis import strategies as st
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
EVENT_SCHEMA = json.loads(
    (ROOT / "docs/user/reference/json-schema/run-event.schema.json").read_text(
        encoding="utf-8"
    )
)
VALIDATOR = Draft202012Validator(EVENT_SCHEMA)
EVENT = json.loads(
    (ROOT / "crates/hoimin-cli/tests/golden/events/schema-v10-current.jsonl")
    .read_text(encoding="utf-8")
    .splitlines()[0]
)


@given(
    changed=st.one_of(
        st.none(),
        st.fixed_dictionaries(
            {"context": st.integers(0, 1073741823)},
            optional={"diff_base": st.one_of(st.none(), st.text(max_size=32))},
        ),
    )
)
@example(changed=None)
@example(changed={"context": 0})
@example(changed={"context": 1073741823, "diff_base": "main"})
def test_current_event_accepts_typed_changed_selection(changed: object) -> None:
    event = copy.deepcopy(EVENT)
    event["schema_version"] = 10
    selection = event["normalized_config"]["selection"]
    selection["changed"] = changed

    errors = list(VALIDATOR.iter_errors(event))

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
    event = copy.deepcopy(EVENT)
    event["normalized_config"]["selection"]["changed"] = changed

    errors = list(VALIDATOR.iter_errors(event))

    assert errors


@pytest.mark.parametrize("field", ["changed_context", "diff_base"])
def test_current_event_rejects_obsolete_selection_siblings(field: str) -> None:
    event = copy.deepcopy(EVENT)
    event["normalized_config"]["selection"][field] = None

    errors = list(VALIDATOR.iter_errors(event))

    assert errors
