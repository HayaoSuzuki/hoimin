from __future__ import annotations

import re
import sys

import deal
import pytest
from hypothesis import given
from hypothesis import strategies as st
from returns.result import Failure, Success

from tools import release

COMPONENTS = st.tuples(*(st.integers(0, 2**32 - 1) for _ in range(3)))
INVALID_VERSIONS = st.sampled_from(
    [
        "",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "１.2.3",
        "1.2",
        "1.2.3\n",
        "v1.2.3",
        "1.2.3-dev.01",
        "1.2.3;echo oops",
        "-1.2.3",
        "1.2.3+local",
    ]
)


@given(parts=COMPONENTS)
def test_stable_version_round_trip(parts: tuple[int, int, int]) -> None:
    version = ".".join(map(str, parts))

    parsed = release.parse_stable_version(version)
    validated = release.check_release_version(version)
    adapted = release.version_tuple(version)

    assert parsed == Success(parts)
    assert validated == Success(None)
    assert adapted == parts


@given(parts=COMPONENTS, counter=st.integers(0, 2**32 - 1))
def test_dev_versions_validate_without_becoming_stable(
    parts: tuple[int, int, int], counter: int
) -> None:
    version = ".".join(map(str, parts)) + f"-dev.{counter}"

    validated = release.check_release_version(version)
    parsed = release.parse_stable_version(version)

    assert validated == Success(None)
    assert isinstance(parsed, Failure)


@given(version=INVALID_VERSIONS)
def test_invalid_versions_are_typed_failures(version: str) -> None:
    parsed = release.parse_stable_version(version)
    checked = release.check_release_version(version)

    assert isinstance(parsed, Failure)
    assert isinstance(parsed.failure(), ValueError)
    assert str(parsed.failure()) == f"expected MAJOR.MINOR.PATCH, got {version!r}"
    assert isinstance(checked, Failure)
    assert isinstance(checked.failure(), ValueError)
    assert str(checked.failure()) == f"invalid release version: {version!r}"


@given(version=INVALID_VERSIONS)
def test_stable_adapter_preserves_validation_error(version: str) -> None:
    expected = f"expected MAJOR.MINOR.PATCH, got {version!r}"

    with pytest.raises(ValueError, match=re.escape(expected)) as error:
        release.version_tuple(version)

    assert str(error.value) == expected


@given(version=INVALID_VERSIONS)
def test_validation_adapter_preserves_validation_error(version: str) -> None:
    expected = f"invalid release version: {version!r}"

    with pytest.raises(ValueError, match=re.escape(expected)) as error:
        release.validate_version(version)

    assert str(error.value) == expected


@pytest.mark.skipif(
    sys.get_int_max_str_digits() == 0, reason="conversion limit disabled"
)
def test_large_components_are_normal_conversion_failures() -> None:
    version = "9" * (sys.get_int_max_str_digits() + 1) + ".0.0"

    parsed = release.parse_stable_version(version)
    validated = release.check_release_version(version)

    assert isinstance(parsed, Failure)
    assert isinstance(parsed.failure(), ValueError)
    assert validated == Success(None)


def test_contract_detects_incorrect_components(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(release, "_version_components", lambda *_: (1, 2, 4))

    with pytest.raises(deal.PostContractError):
        release.parse_stable_version("1.2.3")


def test_stable_version_ordering_is_numeric() -> None:
    versions = ["2.0.0", "1.10.0", "1.9.0"]

    ordered = sorted(versions, key=release.version_tuple)

    assert ordered == ["1.9.0", "1.10.0", "2.0.0"]
