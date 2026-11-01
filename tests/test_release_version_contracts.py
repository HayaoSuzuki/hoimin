from __future__ import annotations

import re
import sys

import deal
import pytest
from hypothesis import example, given
from hypothesis import strategies as st
from returns.result import Failure, Success

from tools import release

COMPONENTS = st.tuples(*(st.integers(0, 2**32 - 1) for _ in range(3)))
INVALID_VERSION_EXAMPLES = [
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
INVALID_VERSIONS = st.one_of(
    st.sampled_from(INVALID_VERSION_EXAMPLES),
    st.builds(
        lambda parts, suffix: ".".join(map(str, parts)) + suffix,
        COMPONENTS,
        st.text(
            alphabet="abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+_;\n",
            min_size=1,
            max_size=16,
        ),
    ),
)


@given(parts=COMPONENTS)
def test_stable_version_round_trip(parts: tuple[int, int, int]) -> None:
    version = ".".join(map(str, parts))

    parsed = release.parse_stable_version(version)

    assert parsed == Success(parts)


@given(parts=COMPONENTS)
def test_stable_version_validates(parts: tuple[int, int, int]) -> None:
    version = ".".join(map(str, parts))

    validated = release.check_release_version(version)

    assert validated == Success(None)


@given(parts=COMPONENTS)
def test_stable_version_adapter_returns_components(parts: tuple[int, int, int]) -> None:
    version = ".".join(map(str, parts))

    adapted = release.version_tuple(version)

    assert adapted == parts


@given(parts=COMPONENTS, counter=st.integers(0, 2**32 - 1))
def test_dev_versions_validate(parts: tuple[int, int, int], counter: int) -> None:
    version = ".".join(map(str, parts)) + f"-dev.{counter}"

    validated = release.check_release_version(version)

    assert validated == Success(None)


@given(parts=COMPONENTS, counter=st.integers(0, 2**32 - 1))
def test_dev_versions_do_not_become_stable(
    parts: tuple[int, int, int], counter: int
) -> None:
    version = ".".join(map(str, parts)) + f"-dev.{counter}"

    parsed = release.parse_stable_version(version)

    assert isinstance(parsed, Failure)


@given(version=INVALID_VERSIONS)
@example(version="")
@example(version="01.2.3")
@example(version="1.02.3")
@example(version="1.2.03")
@example(version="１.2.3")
@example(version="1.2")
@example(version="1.2.3\n")
@example(version="v1.2.3")
@example(version="1.2.3-dev.01")
@example(version="1.2.3;echo oops")
@example(version="-1.2.3")
@example(version="1.2.3+local")
def test_invalid_stable_versions_are_typed_failures(version: str) -> None:
    parsed = release.parse_stable_version(version)

    assert isinstance(parsed, Failure)
    assert isinstance(parsed.failure(), ValueError)
    assert str(parsed.failure()) == f"expected MAJOR.MINOR.PATCH, got {version!r}"


@given(version=INVALID_VERSIONS)
@example(version="")
@example(version="01.2.3")
@example(version="1.02.3")
@example(version="1.2.03")
@example(version="１.2.3")
@example(version="1.2")
@example(version="1.2.3\n")
@example(version="v1.2.3")
@example(version="1.2.3-dev.01")
@example(version="1.2.3;echo oops")
@example(version="-1.2.3")
@example(version="1.2.3+local")
def test_invalid_release_versions_are_typed_failures(version: str) -> None:
    checked = release.check_release_version(version)

    assert isinstance(checked, Failure)
    assert isinstance(checked.failure(), ValueError)
    assert str(checked.failure()) == f"invalid release version: {version!r}"


@given(version=INVALID_VERSIONS)
@example(version="")
@example(version="01.2.3")
@example(version="1.02.3")
@example(version="1.2.03")
@example(version="１.2.3")
@example(version="1.2")
@example(version="1.2.3\n")
@example(version="v1.2.3")
@example(version="1.2.3-dev.01")
@example(version="1.2.3;echo oops")
@example(version="-1.2.3")
@example(version="1.2.3+local")
def test_stable_adapter_preserves_validation_error(version: str) -> None:
    expected = f"expected MAJOR.MINOR.PATCH, got {version!r}"

    with pytest.raises(ValueError, match=re.escape(expected)) as error:
        release.version_tuple(version)

    assert str(error.value) == expected


@given(version=INVALID_VERSIONS)
@example(version="")
@example(version="01.2.3")
@example(version="1.02.3")
@example(version="1.2.03")
@example(version="１.2.3")
@example(version="1.2")
@example(version="1.2.3\n")
@example(version="v1.2.3")
@example(version="1.2.3-dev.01")
@example(version="1.2.3;echo oops")
@example(version="-1.2.3")
@example(version="1.2.3+local")
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

    assert isinstance(parsed, Failure)
    assert isinstance(parsed.failure(), ValueError)


@pytest.mark.skipif(
    sys.get_int_max_str_digits() == 0, reason="conversion limit disabled"
)
def test_large_components_remain_syntactically_valid() -> None:
    version = "9" * (sys.get_int_max_str_digits() + 1) + ".0.0"

    validated = release.check_release_version(version)

    assert validated == Success(None)


def test_contract_detects_incorrect_components(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(release, "_version_components", lambda *_: (1, 2, 4))

    with pytest.raises(deal.PostContractError):
        release.parse_stable_version("1.2.3")


@given(parts=st.lists(COMPONENTS, max_size=30))
@example(parts=[(2, 0, 0), (1, 10, 0), (1, 9, 0)])
def test_stable_version_ordering_is_numeric(parts: list[tuple[int, int, int]]) -> None:
    versions = [".".join(map(str, version)) for version in parts]
    expected = [".".join(map(str, version)) for version in sorted(parts)]

    ordered = sorted(versions, key=release.version_tuple)

    assert ordered == expected
