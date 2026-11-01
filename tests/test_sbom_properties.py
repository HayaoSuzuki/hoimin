"""Generative and seeded mutation fuzz tests for the release boundary."""

from __future__ import annotations

import copy
import json
import random
from collections.abc import Iterator
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any

import pytest
from sbom_fixtures import COMMIT, attach_sboms
from test_release import release_assets

from tools import release, sbom

hypothesis = pytest.importorskip("hypothesis")
from hypothesis import given, settings  # noqa: E402
from hypothesis import strategies as st  # noqa: E402


@given(st.permutations(range(5)), st.permutations(range(6)))
@settings(max_examples=100, deadline=None)
def test_graph_order_does_not_change_validation(
    components: list[int],
    dependencies: list[int],
) -> None:
    with TemporaryDirectory() as temporary:
        directory = Path(temporary)
        assets = release_assets(directory)
        paths = attach_sboms(assets)
        data = json.loads(paths[0].read_text())
        data["components"] = [data["components"][i] for i in components]
        data["dependencies"] = [data["dependencies"][i] for i in dependencies]
        paths[0].write_text(json.dumps(data))

        release.checksums(directory, "1.2.3", COMMIT)

        assert len((directory / "SHA256SUMS").read_text().splitlines()) == 12


@given(st.binary(max_size=2048))
@settings(max_examples=250, deadline=None)
def test_arbitrary_bytes_fail_closed(contents: bytes) -> None:
    with TemporaryDirectory() as temporary:
        directory = Path(temporary)
        asset = directory / "asset"
        asset.write_bytes(b"archive")
        path = directory / "sbom.json"
        path.write_bytes(contents)

        with pytest.raises(ValueError, match=r".+"):
            sbom.validate(path, asset, "linux-x86_64", "standalone", "1.2.3", COMMIT)


def locations(
    value: object, path: tuple[str | int, ...] = ()
) -> list[tuple[str | int, ...]]:
    result = [path]
    if isinstance(value, dict):
        for key, child in value.items():
            assert isinstance(key, str)
            result.extend(locations(child, (*path, key)))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            result.extend(locations(child, (*path, index)))
    return result


def structural_mutations(original: dict[str, Any]) -> Iterator[object]:
    paths = locations(original)
    rng = random.Random(741)  # noqa: S311 -- Reproducible fuzzing, not cryptography.
    replacements = [None, False, 0, "", "wrong", [], {}, [None], {"name": "x"}]
    for _ in range(1000):
        data = copy.deepcopy(original)
        location = rng.choice(paths)
        replacement = rng.choice(replacements)
        if not location:
            data = replacement
        else:
            parent: Any = data
            for key in location[:-1]:
                parent = parent[key]
            if rng.randrange(2):
                del parent[location[-1]]
            else:
                parent[location[-1]] = replacement
        yield data


def accepts_sbom(path: Path, asset: Path) -> bool:
    try:
        sbom.validate(path, asset, "windows-x86_64", "standalone", "1.2.3", COMMIT)
    except ValueError:
        return False
    return True


# Accepted optional-field mutations in the original seed-741 regression corpus.
ACCEPTED_SEEDED_MUTATIONS = frozenset(
    {184, 188, 331, 334, 376, 468, 482, 519, 574, 786, 813}
)


@pytest.fixture(scope="module")
def seeded_mutations(tmp_path_factory: pytest.TempPathFactory) -> list[object]:
    directory = tmp_path_factory.mktemp("sbom-structural-corpus")
    assets = release_assets(directory)
    path = attach_sboms(assets)[0]
    original = json.loads(path.read_text())
    return list(structural_mutations(original))


@pytest.mark.parametrize(
    "mutation_index", range(1000), ids=lambda index: f"seed-741-mutation-{index}"
)
def test_seeded_structural_mutation(
    tmp_path: Path, seeded_mutations: list[object], mutation_index: int
) -> None:
    assets = release_assets(tmp_path)
    path = attach_sboms(assets)[0]
    path.write_text(json.dumps(seeded_mutations[mutation_index]))
    expected = mutation_index in ACCEPTED_SEEDED_MUTATIONS

    accepted = accepts_sbom(path, assets[0])

    assert accepted == expected
