import datetime
from types import SimpleNamespace

import pytest

pytest.importorskip("hypothesis")
from hypothesis import strategies as st

from tools import django_search as search


def test_duration_witness_is_shrunk_and_reproducible() -> None:
    result = search.find_witness(
        "duration", lambda delta: delta.microseconds, lambda _delta: 0, 790, 1.0
    )
    assert result["status"] == "witness"
    assert result["args"] == [1]
    assert result["original"] == 1
    assert result["mutant"] == 0


def test_no_difference_does_not_prove_equivalence() -> None:
    def function(delta: datetime.timedelta) -> int:
        return delta.microseconds

    result = search.find_witness("duration", function, function, 776, 0.2)
    assert result["status"] == "no_witness_within_budget"


def test_exceptions_are_not_witnesses() -> None:
    def broken(delta: datetime.timedelta) -> int:
        raise ValueError(delta)

    result = search.find_witness("duration", lambda _delta: 0, broken, 777, 0.2)
    assert result["status"] == "unsupported_observation"
    assert result["args"] == []


def test_timesince_always_receives_explicit_now_and_depth() -> None:
    def probe(
        first: datetime.datetime, second: datetime.datetime, *, depth: int
    ) -> str:
        assert first.year == 2000
        assert isinstance(second, datetime.datetime)
        assert 1 <= depth <= 3
        return "valid"

    assert search.invoke("timesince", probe, [0, 60, 2]) == "valid"


def test_regression_uses_real_module_and_literal_expected_value() -> None:
    code = search.render_regression("duration", [1], 1)
    assert "django.utils.duration" in code
    assert "assertEqual" in code
    assert "timedelta" in code


def test_non_scalar_observation_is_rejected() -> None:
    result = search.find_witness(
        "duration", lambda _delta: [], lambda _delta: {}, 790, 0.2
    )
    assert result["status"] == "unsupported_observation"


def test_deadline_does_not_change_a_previously_observed_predicate(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    clock = [0.0]
    monkeypatch.setattr(search, "time", SimpleNamespace(monotonic=lambda: clock[0]))
    monkeypatch.setattr(search, "domain", lambda _target: st.just([1]))

    def mutant(_delta: datetime.timedelta) -> int:
        clock[0] = 2.0
        return 0

    result = search.find_witness(
        "duration", lambda delta: delta.microseconds, mutant, 790, 1.0
    )
    assert result["status"] == "witness"
    assert result["args"] == [1]
