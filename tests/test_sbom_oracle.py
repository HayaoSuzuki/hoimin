"""Replay the Lean-generated gate corpus against real checksum validation."""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from sbom_fixtures import COMMIT, attach_sboms
from test_release import release_assets

from tools import release

CORPUS = Path(__file__).parent / "fixtures/release-sbom-gate.csv"


CASES = [line.split(",") for line in CORPUS.read_text().splitlines()]


def gate_assets(case: list[str], directory: Path) -> Path:
    mask_text, valid, extra, _expected = case
    mask = int(mask_text)
    assets = release_assets(directory)
    paths = attach_sboms(assets)
    for index, path in enumerate(paths):
        if not mask & (1 << index):
            path.unlink()
        elif valid == "false":
            data = json.loads(path.read_text())
            data["metadata"]["component"]["version"] = "9.9.9"
            path.write_text(json.dumps(data))
    if extra == "true":
        (directory / "unexpected.txt").write_text("extra")
    sums = directory / "SHA256SUMS"
    sums.write_bytes(b"previous verified release")
    return sums


@pytest.mark.parametrize("case", [case for case in CASES if case[3] == "false"])
def test_lean_release_gate_rejects_invalid_assets(
    case: list[str], tmp_path: Path
) -> None:
    sums = gate_assets(case, tmp_path)

    with pytest.raises(ValueError, match=r".+"):
        release.checksums(tmp_path, "1.2.3", COMMIT)

    assert sums.read_bytes() == b"previous verified release"


@pytest.mark.parametrize("case", [case for case in CASES if case[3] == "true"])
def test_lean_release_gate_accepts_and_replays_valid_assets(
    case: list[str], tmp_path: Path
) -> None:
    sums = gate_assets(case, tmp_path)

    release.checksums(tmp_path, "1.2.3", COMMIT)

    assert len(sums.read_text().splitlines()) == 12
    before = sums.read_bytes()

    release.checksums(tmp_path, "1.2.3", COMMIT)

    assert sums.read_bytes() == before
