"""Hand-checked minimal Cargo graph, independent of the SBOM producer."""

import hashlib
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any

from tools import sbom

COMMIT = "a" * 40


def raw_bom() -> dict[str, Any]:
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "tools": [{"name": "cargo-cyclonedx", "version": "0.5.7"}],
            "component": {
                "type": "application",
                "name": "hoimin-cli",
                "version": "1.2.3",
                "bom-ref": "root",
                "purl": "pkg:cargo/hoimin-cli@1.2.3",
            },
            "properties": [
                {
                    "name": "cdx:rustc:sbom:target:triple",
                    "value": "x86_64-unknown-linux-gnu",
                }
            ],
        },
        "components": [
            {
                "type": "library",
                "name": name,
                "version": version,
                "bom-ref": name,
                "purl": f"pkg:cargo/{name}@{version}",
            }
            for name, version in [
                ("hoimin-core", "1.2.3"),
                ("littrs-ruff-python-parser", "0.6.2"),
                ("stacker", "0.1.25"),
            ]
        ],
        "dependencies": [
            {"ref": "root", "dependsOn": ["hoimin-core", "littrs-ruff-python-parser"]},
            {"ref": "hoimin-core", "dependsOn": []},
            {"ref": "littrs-ruff-python-parser", "dependsOn": ["stacker"]},
            {"ref": "stacker", "dependsOn": []},
        ],
    }


def source_tree(root: Path) -> None:
    vendor = root / "vendor/ruff_python_parser"
    vendor.mkdir(parents=True, exist_ok=True)
    (vendor / "README.hoimin.md").write_text("local changes\n")
    (root / "Cargo.lock").write_text("version = 4\n")


def capture_file(root: Path, target: str) -> Path:
    bom = raw_bom()
    bom["metadata"]["properties"][0]["value"] = target
    raw = root / "capture.json"
    raw.write_text(
        json.dumps(
            {
                "bom": bom,
                "lock_sha256": hashlib.sha256(
                    (root / "Cargo.lock").read_bytes()
                ).hexdigest(),
                "rustc": "rustc 1.99.0 (fixture)",
            }
        )
    )
    return raw


def attach_sboms(assets: list[Path]) -> list[Path]:
    paths = []
    with TemporaryDirectory() as temporary:
        root = Path(temporary)
        source_tree(root)
        for index, platform in enumerate(sbom.TARGETS):
            for offset, kind in ((0, "standalone"), (3, "wheel")):
                raw = capture_file(root, sbom.TARGETS[platform])
                paths.append(
                    sbom.finalize(
                        root,
                        raw,
                        assets[index + offset],
                        platform,
                        kind,
                        "1.2.3",
                        COMMIT,
                    )
                )
    return paths
