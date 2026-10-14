"""Check identifier safety in the public review reference schema."""

import json
from pathlib import Path

from hypothesis import example, given
from hypothesis import strategies as st
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads(
    (ROOT / "docs/json-schema/review-result.schema.json").read_text(encoding="utf-8")
)
REPORT = SCHEMA["$defs"]["event"]["properties"]["draft"]["properties"]["report"]
VALIDATOR = Draft202012Validator({"$defs": SCHEMA["$defs"], **REPORT})
CONTROLS = tuple(chr(code) for code in (*range(32), *range(127, 160)))


@example(control="\n", at_end=True)
@given(control=st.sampled_from(CONTROLS), at_end=st.booleans())
def test_report_identifier_rejects_control_characters(
    *, control: str, at_end: bool
) -> None:
    # Arrange
    run_id = f"run-1{control}" if at_end else f"run{control}1"
    report = {
        "artifact": {"path": "report.json", "content_hash": "a" * 64},
        "run_id": run_id,
    }

    # Act
    errors = list(VALIDATOR.iter_errors(report))

    # Assert
    assert errors
