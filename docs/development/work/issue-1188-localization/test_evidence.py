"""Reject altered claims against the retained public experiment."""

import gzip
import json
from pathlib import Path

import pytest
from check_evidence import verify

HERE = Path(__file__).resolve().parent


@pytest.mark.parametrize(
    "field",
    ["source", "selection", "completion", "rank", "eligible", "exhaustive", "harness"],
)
def test_evidence_check_rejects_a_tampered_public_claim(field):
    # Arrange
    record = json.loads(
        gzip.decompress((HERE / "evidence/empty-query.json.gz").read_bytes())
    )
    case = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))[0]
    if field == "source":
        record["source_sha256"] = "0" * 64
    elif field == "selection":
        record["selected_ids"].reverse()
    elif field == "completion":
        record["complete"] = not record["complete"]
    elif field == "eligible":
        record["eligible_candidates"] += 1
    elif field == "exhaustive":
        record["sample_exhaustive"] = not record["sample_exhaustive"]
    elif field == "harness":
        record["harness_sha256"] = {}
    else:
        record["analysis"]["fingerprint"]["rank"]["worst"] = 1
    # Act / Assert
    with pytest.raises(AssertionError):
        verify(record, case)
