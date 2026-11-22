"""One invocation per input/history/schedule; requirements are in ../design.md."""

import json
from pathlib import Path
from unittest.mock import patch
import urllib.error

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

import subjects
import retry_subject


CORPUS = json.loads(Path(__file__).with_name("corpus.json").read_text(encoding="utf-8"))


def expected_nonnegative(values):
    """Independent finite-domain membership oracle, not the subject's predicate."""
    return {member for member in (0, 1, 2, 3) if member in values}


@pytest.mark.parametrize("values", CORPUS["narrow"])
def test_pbt_narrow_weak(values):
    forbidden = {-3, -2, -1}

    actual = subjects.nonnegative(values)

    assert actual.isdisjoint(forbidden)


@pytest.mark.parametrize("values", CORPUS["broad"])
def test_pbt_broad_weak(values):
    forbidden = {-3, -2, -1}

    actual = subjects.nonnegative(values)

    assert actual.isdisjoint(forbidden)


@pytest.mark.parametrize("values", CORPUS["narrow"])
def test_pbt_narrow_strong(values):
    expected = expected_nonnegative(values)

    actual = subjects.nonnegative(values)

    assert actual == expected


@pytest.mark.parametrize("values", CORPUS["broad"])
def test_pbt_broad_strong(values):
    expected = expected_nonnegative(values)

    actual = subjects.nonnegative(values)

    assert actual == expected


@pytest.mark.parametrize("values", CORPUS["heldout"])
def test_pbt_heldout_weak(values):
    forbidden = {-3, -2, -1}

    actual = subjects.nonnegative(values)

    assert actual.isdisjoint(forbidden)


@pytest.mark.parametrize("values", CORPUS["heldout"])
def test_pbt_heldout_strong(values):
    expected = expected_nonnegative(values)

    actual = subjects.nonnegative(values)

    assert actual == expected


@settings(max_examples=80, database=None, deadline=None, derandomize=True)
@given(st.lists(st.integers(-3, 3), max_size=8))
def test_pbt_generated_membership(values):
    expected = expected_nonnegative(values)

    actual = subjects.nonnegative(values)

    assert actual == expected


def observe_history(cache, operations):
    """Observe one full history, including missing reads, without shared state."""
    observations = []
    for action, key, value in operations:
        if action == "put":
            cache.put(key, value)
        elif action == "delete":
            cache.delete(key)
        else:
            try:
                observations.append((key, cache.get(key)))
            except KeyError:
                observations.append((key, "missing"))
    return observations


def model_history(operations):
    """The specification has storage only, with no cache or invalidation rule."""
    storage = {}
    observations = []
    for action, key, value in operations:
        if action == "put":
            storage[key] = value
        elif action == "delete":
            storage.pop(key, None)
        else:
            observations.append((key, storage.get(key, "missing")))
    return observations


@pytest.mark.parametrize("operations", CORPUS["single"])
def test_stateful_single(operations):
    cache = subjects.Cache()
    expected = model_history(operations)

    actual = observe_history(cache, operations)

    assert actual == expected


@pytest.mark.parametrize("operations", CORPUS["history"])
def test_stateful_history(operations):
    cache = subjects.Cache()
    expected = model_history(operations)

    actual = observe_history(cache, operations)

    assert actual == expected


@settings(max_examples=80, database=None, deadline=None, derandomize=True)
@given(st.lists(st.tuples(st.sampled_from(["put", "get", "delete"]),
                         st.integers(0, 2), st.integers(-2, 2)), max_size=12))
def test_stateful_generated_history(operations):
    cache = subjects.Cache()
    expected = model_history(operations)

    actual = observe_history(cache, operations)

    assert actual == expected


class Backend:
    """Local deterministic failure schedule; each write may change state then fail."""

    def __init__(self, failures):
        self.value = "old"
        self.failures = failures
        self.attempts = 0
        self.closed = False

    def write(self, value):
        self.attempts += 1
        self.value = "partial"
        if self.attempts <= self.failures:
            raise OSError("injected write failure")
        self.value = value

    def close(self):
        self.closed = True


def test_resilience_happy():
    backend = Backend(0)

    subjects.publish(backend, "new")

    assert (backend.value, backend.closed, backend.attempts) == ("new", True, 1)


@pytest.mark.parametrize("failures", [0, 1, 2])
def test_resilience_recovery(failures):
    backend = Backend(failures)

    subjects.publish(backend, "new")

    assert (backend.value, backend.closed, backend.attempts) == (
        "new", True, failures + 1
    )


def test_resilience_exhaustion():
    backend = Backend(3)

    with pytest.raises(OSError, match="injected write failure"):
        subjects.publish(backend, "new")

    assert (backend.value, backend.closed, backend.attempts) == ("old", True, 3)


def test_retry_happy():
    with patch.object(retry_subject, "verify_index", return_value=True) as verify:

        actual = retry_subject.verify_with_propagation(
            "pypi", "v0.15.0", "a" * 40, Path("dist"), allow_missing=False
        )

    assert actual is True
    assert verify.call_count == 1


@pytest.mark.parametrize("failures", [0, 1, 2, 4])
def test_retry_recovery(failures):
    error = urllib.error.HTTPError("https://invalid.local/", 404, "injected", {}, None)
    responses = [error] * failures + [True]
    expected_delays = [10, 20, 40, 60][:failures]
    with (
        patch.object(retry_subject, "verify_index", side_effect=responses) as verify,
        patch.object(retry_subject.time, "monotonic", return_value=0),
        patch.object(retry_subject.time, "sleep") as sleep,
    ):

        actual = retry_subject.verify_with_propagation(
            "pypi", "v0.15.0", "a" * 40, Path("dist"), allow_missing=False
        )

    assert actual is True
    assert verify.call_count == failures + 1
    assert [call.args[0] for call in sleep.call_args_list] == expected_delays


@pytest.mark.parametrize("status", [403, 429, 500])
def test_retry_rejects_other_errors(status):
    error = urllib.error.HTTPError("https://invalid.local/", status, "injected", {}, None)
    with (
        patch.object(retry_subject, "verify_index", side_effect=error) as verify,
        patch.object(retry_subject.time, "sleep") as sleep,
    ):

        with pytest.raises(urllib.error.HTTPError) as raised:
            retry_subject.verify_with_propagation(
                "pypi", "v0.15.0", "a" * 40, Path("dist"), allow_missing=False
            )

    assert raised.value is error
    assert verify.call_count == 1
    sleep.assert_not_called()
