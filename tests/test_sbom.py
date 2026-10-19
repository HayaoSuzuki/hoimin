"""Release SBOM behavioral contracts."""

import hashlib
import json
import subprocess
import sys
from collections.abc import Callable
from operator import setitem
from pathlib import Path
from typing import Any

import pytest
from sbom_fixtures import COMMIT, capture_file, raw_bom, source_tree
from test_release import release_assets

from tools import capture_sbom, release, sbom


@pytest.fixture
def utf8_child(monkeypatch: pytest.MonkeyPatch) -> str:
    # Model Windows' default codec while executing a real child process.
    monkeypatch.setattr(subprocess, "_text_encoding", lambda: "cp1252")
    return (
        "import sys; "
        "sys.stdout.buffer.write('裏側\\n'.encode('utf-8')); "
        "sys.stderr.buffer.write('診断\\n'.encode('utf-8')); "
    )


def test_capture_decodes_successful_tool_output_as_utf8(
    tmp_path: Path, utf8_child: str
) -> None:
    result = capture_sbom.run(
        tmp_path, sys.executable, "-c", utf8_child + "sys.exit(0)"
    )

    assert result == "裏側"


def test_capture_decodes_failed_tool_output_as_utf8(
    tmp_path: Path, utf8_child: str
) -> None:
    with pytest.raises(subprocess.CalledProcessError) as error:
        capture_sbom.run(tmp_path, sys.executable, "-c", utf8_child + "sys.exit(7)")

    assert error.value.returncode == 7
    assert error.value.stdout == "裏側\n"
    assert error.value.stderr == "診断\n"


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
        "hoimin-budget-math",
        "hoimin-core",
        "hoimin-python",
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
    "corrupt",
    [
        pytest.param(
            lambda data: setitem(data["metadata"]["component"], "version", "9.9.9"),
            id="version",
        ),
        pytest.param(
            lambda data: setitem(
                next(
                    p
                    for p in data["metadata"]["properties"]
                    if p["name"] == "hoimin:commit"
                ),
                "value",
                "wrong",
            ),
            id="commit",
        ),
        pytest.param(
            lambda data: setitem(
                next(
                    p
                    for p in data["metadata"]["properties"]
                    if p["name"] == "cdx:rustc:sbom:target:triple"
                ),
                "value",
                "wrong",
            ),
            id="target",
        ),
        pytest.param(
            lambda data: setitem(
                next(
                    p
                    for p in data["metadata"]["properties"]
                    if p["name"] == "hoimin:artifact:sha256"
                ),
                "value",
                "wrong",
            ),
            id="artifact",
        ),
        pytest.param(
            lambda data: setitem(
                next(
                    p
                    for p in data["metadata"]["properties"]
                    if p["name"] == "hoimin:lock:sha256"
                ),
                "value",
                "wrong",
            ),
            id="lock",
        ),
        pytest.param(lambda data: setitem(data, "bomFormat", "other"), id="schema"),
        pytest.param(lambda data: setitem(data, "components", []), id="empty"),
        pytest.param(
            lambda data: data["components"].append(data["components"][0]),
            id="duplicate",
        ),
        pytest.param(
            lambda data: data["dependencies"][0]["dependsOn"].append("missing"),
            id="edge",
        ),
        pytest.param(lambda data: data["dependencies"].pop(), id="node"),
        pytest.param(
            lambda data: setitem(data["dependencies"][2], "dependsOn", []), id="orphan"
        ),
        pytest.param(
            lambda data: setitem(data["components"][1], "properties", []),
            id="provenance",
        ),
        pytest.param(
            lambda data: setitem(data["components"][1], "name", "other"), id="parser"
        ),
        pytest.param(
            lambda data: setitem(data["metadata"]["tools"][0], "version", "0.0.0"),
            id="generator",
        ),
        pytest.param(
            lambda data: setitem(
                data["metadata"]["component"], "components", [data["components"][0]]
            ),
            id="nested-duplicate",
        ),
        pytest.param(
            lambda data: setitem(
                data["components"][1],
                "properties",
                [
                    p
                    for p in data["components"][1]["properties"]
                    if p["name"] != "hoimin:vendor:readme-sha256"
                ],
            ),
            id="readme",
        ),
        pytest.param(
            lambda data: setitem(
                data["components"][1],
                "externalReferences",
                [{"type": "documentation", "url": "https://example.com/wrong-commit"}],
            ),
            id="readme-url",
        ),
        pytest.param(
            lambda data: setitem(data["metadata"], "tools", {"components": []}),
            id="tools-object",
        ),
    ],
)
def test_corrupt_sbom_is_rejected(
    tmp_path: Path, corrupt: Callable[[dict[str, Any]], object]
) -> None:
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
    corrupt(data)
    path.write_text(json.dumps(data))

    with pytest.raises(ValueError, match=r".+"):
        sbom.validate(path, asset, "linux-x86_64", "standalone", "1.2.3", COMMIT)


@pytest.fixture
def cargo_capture(monkeypatch: pytest.MonkeyPatch) -> Callable[[Path, bytes], None]:
    def install(root: Path, lock_contents: bytes) -> None:
        def metadata(*args: str) -> str:
            assert "--locked" in args
            assert "--no-default-features" in args
            return "rustc fixture"

        def cyclonedx(*args: str) -> str:
            assert "--no-default-features" in args
            path = root / "crates/hoimin-cli/hoimin-release-sbom.json"
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps(raw_bom()))
            for member in ("hoimin-budget-math", "hoimin-core", "hoimin-python"):
                sidecar = root / "crates" / member / "hoimin-release-sbom.json"
                sidecar.parent.mkdir(parents=True)
                sidecar.write_text("generated sidecar")
            (root / "Cargo.lock").write_bytes(lock_contents)
            return "rustc fixture"

        def command(_root: Path, *args: str) -> str:
            handlers = {
                ("cargo", "cyclonedx", "--version"): lambda *_args: (
                    "cargo-cyclonedx-cyclonedx 0.5.7"
                ),
                ("cargo", "metadata"): metadata,
                ("cargo", "cyclonedx"): cyclonedx,
            }
            handler = handlers.get(
                args, handlers.get(args[:2], lambda *_args: "rustc fixture")
            )
            return handler(*args)

        monkeypatch.setattr(capture_sbom, "run", command)

    return install


def test_capture_matches_real_generator_version_and_preserves_lock(
    tmp_path: Path, cargo_capture: Callable[[Path, bytes], None]
) -> None:
    source_tree(tmp_path)
    before = (tmp_path / "Cargo.lock").read_bytes()
    cargo_capture(tmp_path, before)
    output = tmp_path / "result.json"

    capture_sbom.capture(tmp_path, "x86_64-unknown-linux-gnu", output)

    assert json.loads(output.read_text())["bom"]["components"][2]["name"] == "stacker"
    assert (tmp_path / "Cargo.lock").read_bytes() == before


def test_capture_rejects_and_restores_changed_lock(
    tmp_path: Path, cargo_capture: Callable[[Path, bytes], None]
) -> None:
    source_tree(tmp_path)
    before = (tmp_path / "Cargo.lock").read_bytes()
    cargo_capture(tmp_path, b"changed")
    output = tmp_path / "result.json"

    with pytest.raises(ValueError, match=r"changed Cargo\.lock"):
        capture_sbom.capture(tmp_path, "x86_64-unknown-linux-gnu", output)

    assert not output.exists()
    assert (tmp_path / "Cargo.lock").read_bytes() == before


@pytest.mark.parametrize("changed_lock", [False, True])
def test_capture_removes_workspace_sidecars(
    tmp_path: Path,
    cargo_capture: Callable[[Path, bytes], None],
    *,
    changed_lock: bool,
) -> None:
    source_tree(tmp_path)
    before = (tmp_path / "Cargo.lock").read_bytes()
    cargo_capture(tmp_path, b"changed" if changed_lock else before)
    output = tmp_path / "result.json"

    if changed_lock:
        with pytest.raises(ValueError, match=r"changed Cargo\.lock"):
            capture_sbom.capture(tmp_path, "x86_64-unknown-linux-gnu", output)
    else:
        capture_sbom.capture(tmp_path, "x86_64-unknown-linux-gnu", output)

    assert not list(tmp_path.glob("crates/*/hoimin-release-sbom.json"))


@pytest.mark.parametrize("corruption", ["missing", "duplicate", "version"])
def test_python_repository_component_requires_one_matching_version(
    tmp_path: Path, corruption: str
) -> None:
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
    component = next(c for c in data["components"] if c["name"] == "hoimin-python")
    if corruption == "missing":
        data["components"].remove(component)
        data["dependencies"] = [
            node for node in data["dependencies"] if node["ref"] != "hoimin-python"
        ]
        data["dependencies"][0]["dependsOn"].remove("hoimin-python")
        data["dependencies"][0]["dependsOn"].append("littrs-ruff-python-parser")
    elif corruption == "duplicate":
        data["components"].append({**component, "bom-ref": "python-duplicate"})
        data["dependencies"].append({"ref": "python-duplicate", "dependsOn": []})
        data["dependencies"][0]["dependsOn"].append("python-duplicate")
    else:
        component["version"] = "9.9.9"
    path.write_text(json.dumps(data))

    with pytest.raises(ValueError, match="wrong hoimin-python version"):
        sbom.validate(path, asset, "linux-x86_64", "standalone", "1.2.3", COMMIT)


@pytest.mark.parametrize("corruption", ["missing", "version"])
def test_budget_math_identity_is_required(tmp_path: Path, corruption: str) -> None:
    source_tree(tmp_path)
    asset = tmp_path / "hoimin-v1.2.3-linux-x86_64.tar.gz"
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
    component = next(c for c in data["components"] if c["name"] == "hoimin-budget-math")
    if corruption == "missing":
        data["components"].remove(component)
        data["dependencies"] = [
            node for node in data["dependencies"] if node["ref"] != "hoimin-budget-math"
        ]
        for node in data["dependencies"]:
            if "hoimin-budget-math" in node["dependsOn"]:
                node["dependsOn"].remove("hoimin-budget-math")
    else:
        component["version"] = "9.9.9"
    path.write_text(json.dumps(data))

    with pytest.raises(ValueError, match="wrong hoimin-budget-math version"):
        sbom.validate(path, asset, "linux-x86_64", "standalone", "1.2.3", COMMIT)
