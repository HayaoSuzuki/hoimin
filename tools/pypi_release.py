"""Validate published GitHub Release wheels before staging a PyPI upload."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import tomllib
import zipfile
from email.parser import Parser
from pathlib import Path

from tools.release import SEMVER

ROOT = Path(__file__).resolve().parents[1]
PLATFORM_PATTERNS = (
    r"win_amd64",
    r"manylinux_2_17_x86_64\.manylinux2014_x86_64",
    r"macosx_11_0_arm64",
)
LICENSE_FILES = ("LICENSE", "vendor/ruff_python_parser/LICENSE")


def validate_wheel(wheel: Path, version: str) -> None:
    project = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))[
        "project"
    ]
    prefix = f"hoimin-{version}.dist-info/"
    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        metadata_files = [
            name for name in names if name.endswith(".dist-info/METADATA")
        ]
        if metadata_files != [prefix + "METADATA"] or len(names) != len(set(names)):
            msg = f"ambiguous wheel metadata: {wheel.name}"
            raise ValueError(msg)
        metadata = Parser().parsestr(archive.read(metadata_files[0]).decode("utf-8"))
        expected = {
            "Name": "hoimin",
            "Version": version,
            "Requires-Python": project["requires-python"],
            "License-Expression": "Elastic-2.0",
        }
        for field, value in expected.items():
            expected_value = value
            actual = metadata.get_all(field, [])
            if field == "Requires-Python":
                expected_value = ",".join(sorted(value.replace(" ", "").split(",")))
                actual = [
                    ",".join(sorted(item.replace(" ", "").split(",")))
                    for item in actual
                ]
            if actual != [expected_value]:
                msg = f"unexpected {field} in {wheel.name}"
                raise ValueError(msg)
        if set(metadata.get_all("License-File", [])) != set(LICENSE_FILES):
            msg = f"missing license declarations in {wheel.name}"
            raise ValueError(msg)
        for name in LICENSE_FILES:
            # Windows checkouts can package CRLF; compare text without rewriting wheels.
            actual_license = archive.read(prefix + "licenses/" + name).replace(
                b"\r\n", b"\n"
            )
            expected_license = (ROOT / name).read_bytes().replace(b"\r\n", b"\n")
            if actual_license != expected_license:
                msg = f"unexpected license text in {wheel.name}: {name}"
                raise ValueError(msg)


def prepare_wheels(tag: str, release_json: Path, assets: Path, output: Path) -> None:
    if re.fullmatch("v" + SEMVER, tag) is None:
        msg = f"expected stable vMAJOR.MINOR.PATCH tag, got {tag!r}"
        raise ValueError(msg)
    state = json.loads(release_json.read_text())
    if (
        state.get("tagName") != tag
        or state.get("isDraft") is not False
        or state.get("isPrerelease") is not False
    ):
        msg = "expected a matching published, non-prerelease GitHub Release"
        raise ValueError(msg)
    version = tag[1:]
    wheels = sorted(assets.glob("*.whl"))
    if len(wheels) != len(PLATFORM_PATTERNS) or not all(
        sum(
            re.fullmatch(
                rf"hoimin-{re.escape(version)}-py3-none-{platform}\.whl", p.name
            )
            is not None
            for p in wheels
        )
        == 1
        for platform in PLATFORM_PATTERNS
    ):
        msg = "expected exactly one Windows, manylinux and macOS wheel for the tag"
        raise ValueError(msg)
    checksums: dict[str, str] = {}
    for line in (assets / "SHA256SUMS").read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.-]+)", line)
        if match is None or match[2] in checksums:
            msg = "invalid or duplicate SHA256SUMS entry"
            raise ValueError(msg)
        checksums[match[2]] = match[1]
    if {name for name in checksums if name.endswith(".whl")} != {
        p.name for p in wheels
    }:
        msg = "SHA256SUMS wheel entries do not match the downloaded wheels"
        raise ValueError(msg)
    for wheel in wheels:
        with wheel.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        if checksums[wheel.name] != digest:
            msg = f"checksum mismatch: {wheel.name}"
            raise ValueError(msg)
        validate_wheel(wheel, version)
    # Stage nothing until every wheel passes; reject a reused output directory.
    output.mkdir(parents=True, exist_ok=False)
    for wheel in wheels:
        shutil.copyfile(wheel, output / wheel.name)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--release-json", type=Path, required=True)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    prepare_wheels(args.tag, args.release_json, args.assets, args.output)


if __name__ == "__main__":
    main()
