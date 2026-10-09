"""Generative and seeded mutation fuzz tests for the release boundary."""

from __future__ import annotations

import copy
import json
import random
from pathlib import Path
from tempfile import TemporaryDirectory

import pytest
from sbom_fixtures import COMMIT, attach_sboms
from test_release import release_assets

from tools import release, sbom

hypothesis = pytest.importorskip("hypothesis")
from hypothesis import given, settings  # noqa: E402
from hypothesis import strategies as st  # noqa: E402


@given(st.permutations(range(3)), st.permutations(range(4)))
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


def test_seeded_structural_fuzz(tmp_path: Path) -> None:
    assets = release_assets(tmp_path)
    path = attach_sboms(assets)[0]
    original = json.loads(path.read_text())
    paths = locations(original)
    rng = random.Random(741)  # noqa: S311 -- Reproducible fuzzing, not cryptography.
    rejected = 0
    accepted = 0
    replacements = [None, False, 0, "", "wrong", [], {}, [None], {"name": "x"}]
    for _ in range(1000):
        data = copy.deepcopy(original)
        location = rng.choice(paths)
        replacement = rng.choice(replacements)
        if not location:
            data = replacement
        else:
            parent = data
            for key in location[:-1]:
                parent = parent[key]
            if rng.randrange(2):
                del parent[location[-1]]
            else:
                parent[location[-1]] = replacement
        path.write_text(json.dumps(data))
        try:
            sbom.validate(
                path, assets[0], "windows-x86_64", "standalone", "1.2.3", COMMIT
            )
        except ValueError:
            rejected += 1
        else:
            accepted += 1
    assert rejected > 0
    assert accepted > 0  # Removing optional fields can be valid.
