"""Replay the Lean-generated gate corpus against real checksum validation."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from sbom_fixtures import COMMIT, attach_sboms
from test_release import release_assets

from tools import release

CORPUS = Path(__file__).parent / "fixtures/release-sbom-gate.csv"


@pytest.mark.parametrize("case", CORPUS.read_text().splitlines())
def test_lean_release_gate(case: str, tmp_path: Path) -> None:
    mask_text, valid, extra, expected = case.split(",")
    mask = int(mask_text)
    assets = release_assets(tmp_path)
    paths = attach_sboms(assets)
    for index, path in enumerate(paths):
        if not mask & (1 << index):
            path.unlink()
        elif valid == "false":
            data = json.loads(path.read_text())
            data["metadata"]["component"]["version"] = "9.9.9"
            path.write_text(json.dumps(data))
    if extra == "true":
        (tmp_path / "unexpected.txt").write_text("extra")
    sums = tmp_path / "SHA256SUMS"
    sums.write_bytes(b"previous verified release")
    try:
        release.checksums(tmp_path, "1.2.3", COMMIT)
    except ValueError:
        accepted = False
        assert sums.read_bytes() == b"previous verified release"
    else:
        accepted = True
        assert len(sums.read_text().splitlines()) == 12
        before = sums.read_bytes()
        release.checksums(tmp_path, "1.2.3", COMMIT)
        assert sums.read_bytes() == before
    assert accepted == (expected == "true")
