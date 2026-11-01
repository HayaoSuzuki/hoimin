"""Check exact integer boundaries of the typed mutation elapsed wire schema."""

import json
from pathlib import Path
from typing import Any

import pytest
from hypothesis import example, given
from hypothesis import strategies as st
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads(
    (ROOT / "docs/user/reference/json-schema/run-event.schema.json").read_text(
        encoding="utf-8"
    )
)
VALIDATOR = Draft202012Validator(SCHEMA["$defs"]["mutationDuration"])
U64_MAX = 2**64 - 1


def load_event(kind: str) -> dict[str, Any]:
    source = ROOT / "crates/hoimin-cli/tests/golden/events/schema-v10-current.jsonl"
    return next(
        event
        for line in source.read_text().splitlines()
        if (event := json.loads(line))["kind"] == kind
    )


@given(seconds=st.integers(0, U64_MAX), nanos=st.integers(0, 999_999_999))
@example(seconds=0, nanos=0)
@example(seconds=0, nanos=999_999_999)
@example(seconds=U64_MAX, nanos=0)
@example(seconds=U64_MAX, nanos=999_999_999)
def test_elapsed_accepts_canonical_boundaries(seconds: int, nanos: int) -> None:
    elapsed = {"secs": seconds, "nanos": nanos}

    errors = list(VALIDATOR.iter_errors(elapsed))

    assert errors == []


@given(seconds=st.one_of(st.integers(-(2**80), -1), st.integers(U64_MAX + 1, 2**80)))
@example(seconds=-1)
@example(seconds=U64_MAX + 1)
def test_elapsed_rejects_seconds_outside_u64(seconds: int) -> None:
    elapsed = {"secs": seconds, "nanos": 0}

    errors = list(VALIDATOR.iter_errors(elapsed))

    assert errors


@pytest.mark.parametrize("elapsed", [[0, 0], None, "0"])
def test_elapsed_requires_object(elapsed: object) -> None:
    candidate = elapsed

    errors = list(VALIDATOR.iter_errors(candidate))

    assert errors


@given(nanos=st.one_of(st.integers(-(2**64), -1), st.integers(1_000_000_000, 2**64)))
@example(nanos=-1)
@example(nanos=1_000_000_000)
def test_elapsed_rejects_noncanonical_nanoseconds(nanos: int) -> None:
    elapsed = {"secs": 0, "nanos": nanos}

    errors = list(VALIDATOR.iter_errors(elapsed))

    assert errors


def test_elapsed_rejects_unknown_fields() -> None:
    elapsed = {"secs": 0, "nanos": 0, "unknown": True}

    errors = list(VALIDATOR.iter_errors(elapsed))

    assert errors


@pytest.mark.parametrize("status", [None, "unknown", 1])
def test_mutant_status_rejects_noncanonical_values(status: object) -> None:
    event = load_event("mutant_finished")
    event["status"] = status

    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))

    assert errors


def test_mutant_status_is_required_even_with_complete_outcome() -> None:
    event = load_event("mutant_finished")
    event.pop("status", None)

    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))

    assert errors


@pytest.mark.parametrize("status", ["killed", "survived"])
def test_mutant_status_schema_checks_shape_without_semantic_agreement(
    status: str,
) -> None:
    # Arrange: the fixture's Exit(7)/complete outcome remains killed in both cases.
    event = load_event("mutant_finished")
    event["status"] = status

    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))
    # Assert: strict Rust readers, rather than generic Schema, enforce agreement.

    assert errors == []


@pytest.mark.parametrize("exit_code", [0, 2, 3, 4, 130])
def test_run_summary_accepts_execution_exit_codes(exit_code: int) -> None:
    event = load_event("run_finished")
    event["exit_code"] = exit_code

    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))

    assert not errors


def test_run_summary_rejects_removed_survivor_exit_code() -> None:
    event = load_event("run_finished")
    event["exit_code"] = 1

    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))

    assert errors


@pytest.mark.parametrize("metrics", [None, "metrics.json"])
def test_execution_config_accepts_metrics_only(metrics: str | None) -> None:
    event = load_event("run_started")
    event["normalized_config"]["output"] = {"metrics": metrics}

    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))

    assert not errors


def test_execution_config_rejects_removed_format_selection() -> None:
    event = load_event("run_started")
    event["normalized_config"]["output"] = {"format": "jsonl", "metrics": None}

    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))

    assert errors
