"""Validate the public environment metadata contract with a full JSON Schema engine."""

import json
from pathlib import Path

import pytest
from hypothesis import example, given
from hypothesis import strategies as st
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
EVENT_SCHEMA = json.loads(
    (ROOT / "docs/json-schema/run-event.schema.json").read_text(encoding="utf-8")
)
SNAPSHOT_VALIDATOR = Draft202012Validator(EVENT_SCHEMA["$defs"]["environmentSnapshot"])


@given(
    snapshot=st.one_of(
        st.just({"state": "untracked"}),
        st.fixed_dictionaries(
            {
                "state": st.just("captured"),
                "names": st.lists(
                    st.from_regex(r"[A-Za-z_][A-Za-z0-9_]{0,20}", fullmatch=True),
                    min_size=1,
                    max_size=5,
                    unique=True,
                ),
                "digest": st.text(
                    alphabet="0123456789abcdef", min_size=64, max_size=64
                ),
            }
        ),
    )
)
@example(snapshot={"state": "untracked"})
@example(snapshot={"state": "captured", "names": ["FLAG"], "digest": "a" * 64})
def test_valid_environment_snapshots(snapshot: dict[str, object]) -> None:

    errors = list(SNAPSHOT_VALIDATOR.iter_errors(snapshot))

    assert not errors


@pytest.mark.parametrize(
    "snapshot",
    [
        {},
        {"state": "pending"},
        {"state": "untracked", "names": []},
        {"state": "untracked", "digest": "a" * 64},
        {"state": "captured", "names": ["FLAG"]},
        {"state": "captured", "digest": "a" * 64},
        {"state": "captured", "names": [], "digest": "a" * 64},
        {"state": "captured", "names": ["FLAG", "FLAG"], "digest": "a" * 64},
        {"state": "captured", "names": ["NAME=VALUE"], "digest": "a" * 64},
        {"state": "captured", "names": ["FLAG\n"], "digest": "a" * 64},
        {"state": "captured", "names": ["FLAG"], "digest": "a" * 63},
        {"state": "captured", "names": ["FLAG"], "digest": "a" * 65},
        {"state": "captured", "names": ["FLAG"], "digest": "A" * 64},
        {"state": "captured", "names": ["FLAG"], "digest": "g" * 64},
        {"state": "captured", "names": ["FLAG"], "digest": "a" * 64 + "\n"},
        {
            "state": "captured",
            "names": ["FLAG"],
            "digest": "a" * 64,
            "value": "private",
        },
    ],
)
def test_invalid_environment_snapshots(snapshot: dict[str, object]) -> None:

    errors = list(SNAPSHOT_VALIDATOR.iter_errors(snapshot))

    assert errors
