"""Independent checks of the evidence and specification boundaries."""

import importlib.util
import copy
import itertools
import json
from pathlib import Path

from hypothesis import given, settings
from hypothesis import strategies as st
import pytest

import check_evidence


HERE = Path(__file__).resolve().parent
ANALYSIS = json.loads((HERE / "evidence/analysis.json").read_text(encoding="utf-8"))
SPEC = importlib.util.spec_from_file_location("research_subjects", HERE / "project/subjects.py")
SUBJECT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SUBJECT)


@pytest.mark.parametrize("name,killed,total", [
    ("pbt-narrow_weak", 0, 3), ("pbt-broad_weak", 2, 3),
    ("pbt-narrow_strong", 1, 3), ("pbt-broad_strong", 3, 3),
    ("pbt-heldout_weak", 2, 3), ("pbt-heldout_strong", 3, 3),
    ("stateful-single", 0, 3), ("stateful-history", 3, 3),
    ("resilience-happy", 3, 7), ("resilience-faults", 6, 7),
    ("retry-happy", 0, 14), ("retry-faults", 9, 14),
])
def test_report_has_complete_observed_population(name, killed, total):
    expected = (killed, total)

    rows = check_evidence.outcomes(name)

    assert (sum(r["status"] == "killed" for r in rows.values()), len(rows)) == expected


@pytest.mark.parametrize("weak,strong", [
    ("pbt-narrow_weak", "pbt-broad_weak"),
    ("pbt-narrow_strong", "pbt-broad_strong"),
    ("pbt-narrow_weak", "pbt-narrow_strong"),
    ("pbt-broad_weak", "pbt-broad_strong"),
    ("stateful-single", "stateful-history"),
    ("resilience-happy", "resilience-faults"),
    ("retry-happy", "retry-faults"),
])
def test_comparison_uses_identical_candidate_descriptors(weak, strong):
    before = check_evidence.outcomes(weak)

    after = check_evidence.outcomes(strong)

    assert {k: row["candidate"] for k, row in before.items()} == {
        k: row["candidate"] for k, row in after.items()
    }


@pytest.mark.parametrize("witness", ANALYSIS["witnesses"], ids=lambda w: w["candidate_id"][:18])
def test_replayed_failure_is_a_behavioral_test_failure(witness):
    output = witness["stdout"]

    lines = output.splitlines()

    assert witness["returncode"] == 1
    assert any(line.startswith("FAILED test_subject.py::test_") for line in lines)
    assert "ERROR collecting" not in output
    assert "ImportError" not in output
    assert "SyntaxError" not in output


def test_pbt_boundary_requires_joint_improvement():
    rows = [row for row in ANALYSIS["matrix"].values()
            if row["candidate"]["operator"] == "compare_order"]
    expected = ["survived", "survived", "survived", "killed"]

    actual = [rows[0]["arms"][name] for name in (
        "pbt-narrow_weak", "pbt-broad_weak", "pbt-narrow_strong", "pbt-broad_strong"
    )]

    assert len(rows) == 1
    assert actual == expected


def test_evidence_rejects_pytest_collection_error(monkeypatch):
    records = copy.deepcopy(check_evidence.events("pbt-broad_strong"))
    mutant = next(row for row in records if row["kind"] == "mutant_finished")
    mutant["outcome"]["executed"]["termination"] = {"Exit": 2}
    monkeypatch.setattr(check_evidence, "events", lambda _: records)

    with pytest.raises(AssertionError):
        check_evidence.outcomes("pbt-broad_strong")


@pytest.mark.parametrize("status", ["timeout", "out_of_memory", "error", "inconclusive", "not_run"])
def test_evidence_rejects_inconclusive_execution(monkeypatch, status):
    records = copy.deepcopy(check_evidence.events("pbt-broad_strong"))
    mutant = next(row for row in records if row["kind"] == "mutant_finished")
    mutant["status"] = status
    monkeypatch.setattr(check_evidence, "events", lambda _: records)

    with pytest.raises(AssertionError):
        check_evidence.outcomes("pbt-broad_strong")


def test_evidence_rejects_failed_baseline(monkeypatch):
    records = copy.deepcopy(check_evidence.events("pbt-broad_strong"))
    baseline = next(row for row in records if row["kind"] == "baseline_finished")
    baseline["termination"] = {"Exit": 1}
    monkeypatch.setattr(check_evidence, "events", lambda _: records)

    with pytest.raises(AssertionError):
        check_evidence.outcomes("pbt-broad_strong")


def test_evidence_rejects_incomplete_report(monkeypatch):
    records = copy.deepcopy(check_evidence.events("pbt-broad_strong"))
    records[-1]["complete"] = False
    monkeypatch.setattr(check_evidence, "events", lambda _: records)

    with pytest.raises(AssertionError):
        check_evidence.outcomes("pbt-broad_strong")


@pytest.mark.parametrize("values", list(itertools.chain.from_iterable(
    itertools.product(range(-3, 4), repeat=length) for length in range(4)
)))
def test_nonnegative_matches_finite_membership_specification(values):
    expected = {value for value in (0, 1, 2, 3) if value in values}

    actual = SUBJECT.nonnegative(values)

    assert actual == expected


@settings(max_examples=100, database=None, deadline=None, derandomize=True)
@given(old=st.integers(), increment=st.integers(min_value=1))
def test_cache_warm_update_observes_changed_value(old, increment):
    cache = SUBJECT.Cache()
    cache.put("key", old)
    cache.get("key")
    value = old + increment

    cache.put("key", value)
    actual = cache.get("key")

    assert actual == value


@settings(max_examples=100, database=None, deadline=None, derandomize=True)
@given(value=st.integers())
def test_cache_delete_invalidates_warm_value(value):
    cache = SUBJECT.Cache()
    cache.put("key", value)
    cache.get("key")

    cache.delete("key")

    with pytest.raises(KeyError):
        cache.get("key")


class FaultBackend:
    def __init__(self, old, failures):
        self.value = old
        self.failures = failures
        self.attempts = 0
        self.closed = False

    def write(self, value):
        self.attempts += 1
        self.value = object()
        if self.attempts <= self.failures:
            raise OSError("scheduled")
        self.value = value

    def close(self):
        self.closed = True


@settings(max_examples=100, database=None, deadline=None, derandomize=True)
@given(old=st.text(max_size=20), value=st.text(max_size=20), failures=st.integers(0, 2))
def test_recovery_preserves_requested_value(old, value, failures):
    backend = FaultBackend(old, failures)

    SUBJECT.publish(backend, value)

    assert (backend.value, backend.attempts, backend.closed) == (value, failures + 1, True)


@settings(max_examples=100, database=None, deadline=None, derandomize=True)
@given(old=st.text(max_size=20), value=st.text(max_size=20), failures=st.integers(3, 10))
def test_exhaustion_restores_original_value(old, value, failures):
    backend = FaultBackend(old, failures)

    with pytest.raises(OSError, match="scheduled"):
        SUBJECT.publish(backend, value)

    assert (backend.value, backend.attempts, backend.closed) == (old, 3, True)
