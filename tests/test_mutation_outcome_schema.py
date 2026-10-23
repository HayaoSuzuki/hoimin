"""Check exact integer boundaries of the typed mutation elapsed wire schema."""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads(
    (ROOT / "docs/json-schema/run-event.schema.json").read_text(encoding="utf-8")
)
VALIDATOR = Draft202012Validator(SCHEMA["$defs"]["mutationDuration"])
U64_MAX = 2**64 - 1


@pytest.mark.parametrize("seconds", [0, U64_MAX])
@pytest.mark.parametrize("nanos", [0, 999_999_999])
def test_elapsed_accepts_canonical_boundaries(seconds: int, nanos: int) -> None:
    # Arrange
    elapsed = {"secs": seconds, "nanos": nanos}
    # Act
    errors = list(VALIDATOR.iter_errors(elapsed))
    # Assert
    assert errors == []


@pytest.mark.parametrize("seconds", [-1, U64_MAX + 1])
def test_elapsed_rejects_seconds_outside_u64(seconds: int) -> None:
    # Arrange
    elapsed = {"secs": seconds, "nanos": 0}
    # Act
    errors = list(VALIDATOR.iter_errors(elapsed))
    # Assert
    assert errors


@pytest.mark.parametrize("elapsed", [[0, 0], None, "0"])
def test_elapsed_requires_object(elapsed: object) -> None:
    # Arrange
    candidate = elapsed
    # Act
    errors = list(VALIDATOR.iter_errors(candidate))
    # Assert
    assert errors


@pytest.mark.parametrize("nanos", [-1, 1_000_000_000])
def test_elapsed_rejects_noncanonical_nanoseconds(nanos: int) -> None:
    # Arrange
    elapsed = {"secs": 0, "nanos": nanos}
    # Act
    errors = list(VALIDATOR.iter_errors(elapsed))
    # Assert
    assert errors


def test_elapsed_rejects_unknown_fields() -> None:
    # Arrange
    elapsed = {"secs": 0, "nanos": 0, "unknown": True}
    # Act
    errors = list(VALIDATOR.iter_errors(elapsed))
    # Assert
    assert errors


@pytest.mark.parametrize("status", [None, "unknown", 1])
def test_mutant_status_rejects_noncanonical_values(status: object) -> None:
    # Arrange
    report = json.loads(
        (
            ROOT / "crates/hoimin-cli/tests/golden/reports/schema-v10-original.json"
        ).read_text(encoding="utf-8")
    )
    event = report["mutants"][0]
    event["status"] = status
    # Act
    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))
    # Assert
    assert errors


def test_mutant_status_is_required_even_with_complete_outcome() -> None:
    # Arrange
    report = json.loads(
        (
            ROOT / "crates/hoimin-cli/tests/golden/reports/schema-v10-original.json"
        ).read_text(encoding="utf-8")
    )
    event = report["mutants"][0]
    event.pop("status", None)
    # Act
    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))
    # Assert
    assert errors


@pytest.mark.parametrize("status", ["killed", "survived"])
def test_mutant_status_schema_checks_shape_without_semantic_agreement(
    status: str,
) -> None:
    # Arrange: the fixture's Exit(7)/complete outcome remains killed in both cases.
    report = json.loads(
        (
            ROOT / "crates/hoimin-cli/tests/golden/reports/schema-v10-original.json"
        ).read_text(encoding="utf-8")
    )
    event = report["mutants"][0]
    event["status"] = status
    # Act
    errors = list(Draft202012Validator(SCHEMA).iter_errors(event))
    # Assert: strict Rust readers, rather than generic Schema, enforce agreement.
    assert errors == []


@pytest.mark.parametrize("exit_code", [0, 2, 3, 4, 130])
def test_run_summary_accepts_execution_exit_codes(exit_code: int) -> None:
    report = json.loads(
        (
            ROOT / "crates/hoimin-cli/tests/golden/reports/schema-v10-current.json"
        ).read_text(encoding="utf-8")
    )
    event = report["summary"]
    event["exit_code"] = exit_code
    assert not list(Draft202012Validator(SCHEMA).iter_errors(event))


def test_run_summary_rejects_removed_survivor_exit_code() -> None:
    report = json.loads(
        (
            ROOT / "crates/hoimin-cli/tests/golden/reports/schema-v10-current.json"
        ).read_text(encoding="utf-8")
    )
    event = report["summary"]
    event["exit_code"] = 1
    assert list(Draft202012Validator(SCHEMA).iter_errors(event))
