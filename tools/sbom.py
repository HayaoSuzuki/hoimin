"""Bind Cargo CycloneDX graphs to release assets and validate them offline."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from functools import lru_cache
from pathlib import Path
from typing import Any
from urllib.parse import quote

from jsonschema import Draft7Validator, ValidationError
from jsonschema.protocols import Validator
from referencing import Registry, Resource

if bool(__package__):
    from .release_policy import RELEASE_PLATFORMS
else:
    from release_policy import RELEASE_PLATFORMS

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {platform.name: platform.rust_target for platform in RELEASE_PLATFORMS}
KINDS = ("standalone", "wheel")
SCOPE = "Cargo normal and build dependencies; excludes dev/fuzz and OS libraries"
REPOSITORY = "https://github.com/HayaoSuzuki/hoimin"
PARSER = "ruff_python_parser"
RUFF_VERSION = "0.0.19"
VENDORED_RUFF = frozenset(
    (
        PARSER,
        "ruff_python_ast",
        "ruff_python_trivia",
        "ruff_source_file",
        "ruff_text_size",
        "ruff_unicode_names2",
        "ruff_unicode_names2_generator",
        "ruff_cache",
        "ruff_macros",
    )
)
UPSTREAM = "30516b42ab972ffa2569e5efcfee81135da466d3"


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def filename(version: str, platform: str, kind: str) -> str:
    return f"hoimin-v{version}-{platform}-{kind}.cdx.json"


def environment(platform: str, kind: str) -> str:
    if platform == "linux-x86_64":
        return (
            "ubuntu-22.04"
            if kind == "standalone"
            else "ghcr.io/pyo3/maturin:v1.15.0 (manylinux2014)"
        )
    return "windows-latest" if platform == "windows-x86_64" else "macos-14"


@lru_cache(maxsize=1)
def schema_validator() -> Validator:
    directory = ROOT / "tools/schemas/cyclonedx-1.5"
    schemas = [
        json.loads(path.read_text(encoding="utf-8"))
        for path in sorted(directory.glob("*.schema.json"))
    ]
    registry = Registry().with_resources(
        (schema["$id"], Resource.from_contents(schema)) for schema in schemas
    )
    schema = next(s for s in schemas if s["$id"].endswith("bom-1.5.schema.json"))
    return Draft7Validator(schema, registry=registry)


def properties(items: list[dict[str, str]]) -> dict[str, str]:
    result = {p["name"]: p["value"] for p in items}
    if len(result) != len(items):
        msg = "duplicate SBOM property"
        raise ValueError(msg)
    return result


def require(condition: bool, message: str) -> None:  # noqa: FBT001 -- Predicate assertion.
    if not condition:
        raise ValueError(message)


def validate_graph(data: dict[str, Any], version: str, commit: str) -> None:
    root = data["metadata"]["component"]
    require(
        root["name"] == "hoimin-cli" and root["version"] == version,
        "wrong SBOM root/version",
    )
    components = data["components"]
    require(bool(components), "empty dependency graph")
    all_components = [root, *components]
    refs = [c["bom-ref"] for c in all_components]
    nested_refs = []
    pending_components = list(all_components)
    while pending_components:
        component = pending_components.pop()
        nested_refs.append(component["bom-ref"])
        pending_components.extend(component.get("components", []))
    require(len(set(nested_refs)) == len(nested_refs), "duplicate component reference")
    for component in all_components:
        require(
            bool(component.get("version"))
            and component.get("purl", "").startswith("pkg:cargo/"),
            "missing component identity",
        )
    graph = {d["ref"]: d["dependsOn"] for d in data["dependencies"]}
    require(len(graph) == len(data["dependencies"]), "duplicate dependency reference")
    require(set(graph) == set(refs), "missing dependency node")
    for edges in graph.values():
        require(
            len(edges) == len(set(edges)) and set(edges) <= set(refs),
            "duplicate or dangling dependency edge",
        )
    reached = set()
    pending = [root["bom-ref"]]
    while pending:
        ref = pending.pop()
        if ref not in reached:
            reached.add(ref)
            pending.extend(graph[ref])
    require(reached == set(refs), "unreachable component")
    for member in ("hoimin-budget-math", "hoimin-core", "hoimin-python"):
        matches = [c for c in components if c["name"] == member]
        require(
            len(matches) == 1 and matches[0]["version"] == version,
            f"wrong {member} version",
        )
    require(
        sum(c["name"] == PARSER for c in components) == 1,
        "missing or duplicate vendored parser",
    )
    for component in components:
        if component["name"] in VENDORED_RUFF:
            validate_vendor(component, commit)


def validate_vendor(component: dict[str, Any], commit: str) -> None:
    props = properties(component.get("properties", []))
    name = component["name"]
    require(
        props.get("hoimin:upstream:commit") == UPSTREAM
        and props.get("hoimin:vendor:commit") == commit
        and props.get("hoimin:upstream:package") == f"pkg:cargo/{name}@{RUFF_VERSION}",
        "missing Ruff provenance",
    )
    require(
        re.fullmatch(r"[0-9a-f]{64}", props.get("hoimin:vendor:readme-sha256", ""))
        is not None,
        "missing Ruff README digest",
    )
    readme = f"{REPOSITORY}/blob/{commit}/vendor/ruff_python_parser/README.hoimin.md"
    require(
        {"type": "documentation", "url": readme}
        in component.get("externalReferences", []),
        "missing pinned Ruff README reference",
    )
    source = quote(f"{REPOSITORY}@{commit}", safe="")
    require(
        component["version"] == RUFF_VERSION
        and component["purl"]
        == f"pkg:cargo/{name}@{RUFF_VERSION}?vcs_url={source}#vendor/{name}"
        and bool(component.get("pedigree", {}).get("notes")),
        "unidentified Ruff source changes",
    )


def validate(  # noqa: PLR0913, PLR0917 -- Explicit release identity.
    path: Path, asset: Path, platform: str, kind: str, version: str, commit: str
) -> None:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        schema_validator().validate(data)
        require(data["specVersion"] == "1.5", "wrong CycloneDX version")
        props = properties(data["metadata"]["properties"])
        expected = {
            "hoimin:commit": commit,
            "hoimin:platform": platform,
            "hoimin:kind": kind,
            "hoimin:features": "--no-default-features",
            "hoimin:scope": SCOPE,
            "hoimin:build-environment": environment(platform, kind),
            "cdx:rustc:sbom:target:triple": TARGETS[platform],
            "hoimin:artifact:name": asset.name,
            "hoimin:artifact:sha256": digest(asset),
        }
        require(re.fullmatch(r"[0-9a-f]{40}", commit) is not None, "invalid commit")
        require(
            all(props.get(k) == v for k, v in expected.items()),
            "SBOM metadata/artifact mismatch",
        )
        require(
            re.fullmatch(r"[0-9a-f]{64}", props["hoimin:lock:sha256"]) is not None,
            "invalid lock digest",
        )
        require(bool(props["hoimin:rustc"]), "missing rustc identity")
        require(isinstance(data["metadata"]["tools"], list), "wrong generator format")
        require(
            any(
                t.get("name") == "cargo-cyclonedx" and t.get("version") == "0.5.7"
                for t in data["metadata"]["tools"]
            ),
            "wrong generator",
        )
        validate_graph(data, version, commit)
    except (
        KeyError,
        TypeError,
        ValidationError,
        UnicodeError,
        RecursionError,
    ) as error:
        msg = f"invalid SBOM {path.name}: {error}"
        raise ValueError(msg) from error


def finalize(  # noqa: PLR0913, PLR0917 -- Explicit release identity.
    root: Path,
    raw: Path,
    asset: Path,
    platform: str,
    kind: str,
    version: str,
    commit: str,
) -> Path:
    capture = json.loads(raw.read_text(encoding="utf-8"))
    require(
        capture["lock_sha256"] == digest(root / "Cargo.lock"),
        "Cargo.lock differs from captured graph",
    )
    data = capture["bom"]
    props = properties(data["metadata"]["properties"])
    require(
        props.get("cdx:rustc:sbom:target:triple") == TARGETS[platform],
        "wrong captured target",
    )
    props.update(
        {
            "hoimin:commit": commit,
            "hoimin:platform": platform,
            "hoimin:kind": kind,
            "hoimin:features": "--no-default-features",
            "hoimin:scope": SCOPE,
            "hoimin:build-environment": environment(platform, kind),
            "hoimin:lock:sha256": capture["lock_sha256"],
            "hoimin:rustc": capture["rustc"],
            "hoimin:artifact:name": asset.name,
            "hoimin:artifact:sha256": digest(asset),
        }
    )
    data["metadata"]["properties"] = [
        {"name": k, "value": v} for k, v in sorted(props.items())
    ]
    for component in data["components"]:
        name = component["name"]
        if name in VENDORED_RUFF:
            source = quote(f"{REPOSITORY}@{commit}", safe="")
            component["purl"] = (
                f"pkg:cargo/{name}@{component['version']}?vcs_url={source}#vendor/{name}"
            )
            component["pedigree"] = {
                "notes": (
                    "Ruff 0.17.2 source snapshot with standalone manifests; "
                    "parser retains hoimin stack growth, interpolation recovery "
                    "and iterative cleanup changes. "
                    "See pinned README.hoimin.md."
                )
            }
            component["properties"] = [
                {
                    "name": "hoimin:upstream:package",
                    "value": f"pkg:cargo/{name}@{RUFF_VERSION}",
                },
                {"name": "hoimin:upstream:commit", "value": UPSTREAM},
                {"name": "hoimin:vendor:commit", "value": commit},
                {
                    "name": "hoimin:vendor:readme-sha256",
                    "value": digest(
                        root / "vendor/ruff_python_parser/README.hoimin.md"
                    ),
                },
            ]
            component.setdefault("externalReferences", []).append(
                {
                    "type": "documentation",
                    "url": (
                        f"{REPOSITORY}/blob/{commit}/"
                        "vendor/ruff_python_parser/README.hoimin.md"
                    ),
                }
            )
    output = asset.parent / filename(version, platform, kind)
    temporary = output.with_suffix(".tmp")
    try:
        temporary.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
        validate(temporary, asset, platform, kind, version, commit)
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--raw", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--platform", choices=TARGETS, required=True)
    parser.add_argument("--kind", choices=KINDS, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    if args.kind == "wheel":
        assets = list(args.directory.glob("*.whl"))
        require(len(assets) == 1, "expected one wheel in platform artifact directory")
        asset = assets[0]
    else:
        suffix = "zip" if args.platform.startswith("windows") else "tar.gz"
        asset = args.directory / f"hoimin-v{args.version}-{args.platform}.{suffix}"
    finalize(
        args.root, args.raw, asset, args.platform, args.kind, args.version, args.commit
    )


if __name__ == "__main__":
    main()
