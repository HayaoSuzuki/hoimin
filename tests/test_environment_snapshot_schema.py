"""Validate the public environment metadata contract with a full JSON Schema engine."""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
EVENT_SCHEMA = json.loads(
    (ROOT / "docs/json-schema/run-event.schema.json").read_text(encoding="utf-8")
)
SNAPSHOT_VALIDATOR = Draft202012Validator(EVENT_SCHEMA["$defs"]["environmentSnapshot"])


@pytest.mark.parametrize(
    "snapshot",
    [
        {"state": "untracked"},
        {"state": "captured", "names": ["FLAG"], "digest": "a" * 64},
    ],
)
def test_valid_environment_snapshots(snapshot: dict[str, object]) -> None:
    SNAPSHOT_VALIDATOR.validate(snapshot)


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
    assert list(SNAPSHOT_VALIDATOR.iter_errors(snapshot))
