"""Release SBOM behavioral contracts."""

import hashlib
import json
from pathlib import Path

import pytest
from sbom_fixtures import COMMIT, capture_file, raw_bom, source_tree
from test_release import release_assets

from tools import capture_sbom, release, sbom


def test_release_without_sboms_is_rejected(tmp_path: Path) -> None:
    release_assets(tmp_path)
    with pytest.raises(ValueError, match="missing"):
        release.checksums(tmp_path, "1.2.3")
    assert not (tmp_path / "SHA256SUMS").exists()


def test_sbom_preserves_transitive_graph_and_binds_artifact(tmp_path: Path) -> None:

    source_tree(tmp_path)
    asset = tmp_path / "hoimin-v1.2.3-linux-x86_64.tar.gz"
    asset.write_bytes(b"archive")
    raw = capture_file(tmp_path, "x86_64-unknown-linux-gnu")
    path = sbom.finalize(
        tmp_path, raw, asset, "linux-x86_64", "standalone", "1.2.3", COMMIT
    )
    data = json.loads(path.read_text())
    assert path.name == "hoimin-v1.2.3-linux-x86_64-standalone.cdx.json"
    props = {p["name"]: p["value"] for p in data["metadata"]["properties"]}
    assert props["hoimin:artifact:sha256"] == hashlib.sha256(b"archive").hexdigest()
    assert props["hoimin:commit"] == COMMIT
    assert {c["name"] for c in data["components"]} == {
        "hoimin-core",
        "littrs-ruff-python-parser",
        "stacker",
    }
    parser = next(
        c for c in data["components"] if c["name"] == "littrs-ruff-python-parser"
    )
    assert parser["pedigree"]["notes"]
    assert COMMIT in parser["purl"]
    sbom.validate(path, asset, "linux-x86_64", "standalone", "1.2.3", COMMIT)


@pytest.mark.parametrize(
    "damage",
    [
        "version",
        "commit",
        "target",
        "artifact",
        "lock",
        "schema",
        "empty",
        "duplicate",
        "edge",
        "node",
        "orphan",
        "provenance",
        "parser",
        "generator",
        "nested-duplicate",
        "readme",
        "readme-url",
        "tools-object",
    ],
)
def test_corrupt_sbom_is_rejected(tmp_path: Path, damage: str) -> None:  # noqa: C901, PLR0912 -- Independent corruption cases.

    source_tree(tmp_path)
    asset = tmp_path / "archive.tar.gz"
    asset.write_bytes(b"archive")
    path = sbom.finalize(
        tmp_path,
        capture_file(tmp_path, "x86_64-unknown-linux-gnu"),
        asset,
        "linux-x86_64",
        "standalone",
        "1.2.3",
        COMMIT,
    )
    data = json.loads(path.read_text())
    if damage == "version":
        data["metadata"]["component"]["version"] = "9.9.9"
    elif damage in {"commit", "target", "artifact", "lock"}:
        key = {
            "commit": "hoimin:commit",
            "target": "cdx:rustc:sbom:target:triple",
            "artifact": "hoimin:artifact:sha256",
            "lock": "hoimin:lock:sha256",
        }[damage]
        next(p for p in data["metadata"]["properties"] if p["name"] == key)["value"] = (
            "wrong"
        )
    elif damage == "schema":
        data["bomFormat"] = "other"
    elif damage == "empty":
        data["components"] = []
    elif damage == "duplicate":
        data["components"].append(data["components"][0])
    elif damage == "edge":
        data["dependencies"][0]["dependsOn"].append("missing")
    elif damage == "node":
        data["dependencies"].pop()
    elif damage == "orphan":
        data["dependencies"][2]["dependsOn"] = []
    elif damage == "provenance":
        data["components"][1]["properties"] = []
    elif damage == "parser":
        data["components"][1]["name"] = "other"
    elif damage == "nested-duplicate":
        data["metadata"]["component"]["components"] = [data["components"][0]]
    elif damage == "readme":
        parser = data["components"][1]
        parser["properties"] = [
            p
            for p in parser["properties"]
            if p["name"] != "hoimin:vendor:readme-sha256"
        ]
    elif damage == "readme-url":
        data["components"][1]["externalReferences"] = [
            {"type": "documentation", "url": "https://example.com/wrong-commit"}
        ]
    elif damage == "tools-object":
        data["metadata"]["tools"] = {"components": []}
    else:
        data["metadata"]["tools"][0]["version"] = "0.0.0"
    path.write_text(json.dumps(data))
    with pytest.raises(ValueError, match=r".+"):
        sbom.validate(path, asset, "linux-x86_64", "standalone", "1.2.3", COMMIT)


@pytest.mark.parametrize("change_lock", [False, True])
def test_capture_matches_real_generator_version_and_preserves_lock(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    *,
    change_lock: bool,
) -> None:

    source_tree(tmp_path)
    before = (tmp_path / "Cargo.lock").read_bytes()

    def command(root: Path, *args: str) -> str:
        if args == ("cargo", "cyclonedx", "--version"):
            return "cargo-cyclonedx-cyclonedx 0.5.7"
        if args[:2] == ("cargo", "metadata"):
            assert "--locked" in args
            assert "--no-default-features" in args
        elif args[:2] == ("cargo", "cyclonedx"):
            assert "--no-default-features" in args
            path = root / "crates/hoimin-cli/hoimin-release-sbom.json"
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps(raw_bom()))
            if change_lock:
                (root / "Cargo.lock").write_bytes(b"changed")
        return "rustc fixture"

    monkeypatch.setattr(capture_sbom, "run", command)
    output = tmp_path / "result.json"
    if change_lock:
        with pytest.raises(ValueError, match=r"changed Cargo\.lock"):
            capture_sbom.capture(tmp_path, "x86_64-unknown-linux-gnu", output)
        assert not output.exists()
    else:
        capture_sbom.capture(tmp_path, "x86_64-unknown-linux-gnu", output)
        assert (
            json.loads(output.read_text())["bom"]["components"][2]["name"] == "stacker"
        )
    assert (tmp_path / "Cargo.lock").read_bytes() == before
