from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import zipfile
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = (
    "win_amd64",
    "manylinux_2_17_x86_64.manylinux2014_x86_64",
    "macosx_11_0_arm64",
)


def write_wheel(path: Path, *, license_expression: str = "Elastic-2.0") -> None:
    prefix = "hoimin-1.2.3.dist-info/"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(
            prefix + "METADATA",
            "Metadata-Version: 2.4\nName: hoimin\nVersion: 1.2.3\n"
            "Requires-Python: >=3.14,<3.15\n"
            f"License-Expression: {license_expression}\n"
            "License-File: LICENSE\n"
            "License-File: vendor/ruff_python_parser/LICENSE\n\nDescription\n",
        )
        for name in ("LICENSE", "vendor/ruff_python_parser/LICENSE"):
            archive.writestr(prefix + "licenses/" + name, (ROOT / name).read_bytes())


def write_checksums(assets: Path) -> None:
    (assets / "SHA256SUMS").write_text(
        "".join(
            f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
            for path in sorted(assets.glob("*.whl"))
        )
        + "0" * 64
        + "  hoimin-v1.2.3-linux-x86_64.tar.gz\n",
    )


@pytest.fixture
def release_assets(tmp_path: Path) -> Path:
    assets = tmp_path / "assets"
    assets.mkdir()
    for platform in PLATFORMS:
        write_wheel(assets / f"hoimin-1.2.3-py3-none-{platform}.whl")
    write_checksums(assets)
    (tmp_path / "release.json").write_text(
        json.dumps({"tagName": "v1.2.3", "isDraft": False, "isPrerelease": False})
    )
    return assets


def prepare(assets: Path, tag: str = "v1.2.3") -> subprocess.CompletedProcess[str]:
    return subprocess.run(  # noqa: S603
        [
            sys.executable,
            "-m",
            "tools.pypi_release",
            "--tag",
            tag,
            "--release-json",
            str(assets.parent / "release.json"),
            "--assets",
            str(assets),
            "--output",
            str(assets.parent / "dist"),
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
    )


def test_stages_only_verified_wheels_without_changing_bytes(
    release_assets: Path,
) -> None:
    result = prepare(release_assets)
    assert result.returncode == 0, result.stderr
    staged = list((release_assets.parent / "dist").iterdir())
    assert {path.name for path in staged} == {
        f"hoimin-1.2.3-py3-none-{platform}.whl" for platform in PLATFORMS
    }
    for path in staged:
        assert path.read_bytes() == (release_assets / path.name).read_bytes()


@pytest.mark.parametrize("requires_python", [">=3.14, <3.15", "<3.15,>=3.14"])
def test_accepts_equivalent_python_specifier_formatting(
    release_assets: Path, requires_python: str
) -> None:
    wheel = release_assets / "hoimin-1.2.3-py3-none-win_amd64.whl"
    with zipfile.ZipFile(wheel) as archive:
        files = {name: archive.read(name) for name in archive.namelist()}
    metadata = "hoimin-1.2.3.dist-info/METADATA"
    files[metadata] = files[metadata].replace(b">=3.14,<3.15", requires_python.encode())
    with zipfile.ZipFile(wheel, "w") as archive:
        for name, content in files.items():
            archive.writestr(name, content)
    write_checksums(release_assets)
    result = prepare(release_assets)
    assert result.returncode == 0, result.stderr


@pytest.mark.parametrize("line_ending", [b"\n", b"\r\n"], ids=["LF", "CRLF"])
def test_accepts_license_line_endings_without_changing_wheel_bytes(
    release_assets: Path, line_ending: bytes
) -> None:
    wheel = release_assets / "hoimin-1.2.3-py3-none-win_amd64.whl"
    with zipfile.ZipFile(wheel) as archive:
        files = {name: archive.read(name) for name in archive.namelist()}
    for name in ("LICENSE", "vendor/ruff_python_parser/LICENSE"):
        member = "hoimin-1.2.3.dist-info/licenses/" + name
        files[member] = (
            files[member].replace(b"\r\n", b"\n").replace(b"\n", line_ending)
        )
    with zipfile.ZipFile(wheel, "w") as archive:
        for name, content in files.items():
            archive.writestr(name, content)
    write_checksums(release_assets)
    original = wheel.read_bytes()
    result = prepare(release_assets)
    assert result.returncode == 0, result.stderr
    assert (release_assets.parent / "dist" / wheel.name).read_bytes() == original


@pytest.mark.parametrize(
    "license_name", ["LICENSE", "vendor/ruff_python_parser/LICENSE"]
)
@pytest.mark.parametrize("damage", ["text", "whitespace"])
def test_rejects_license_changes_with_windows_line_endings(
    release_assets: Path, license_name: str, damage: str
) -> None:
    wheel = release_assets / "hoimin-1.2.3-py3-none-win_amd64.whl"
    with zipfile.ZipFile(wheel) as archive:
        files = {name: archive.read(name) for name in archive.namelist()}
    member = "hoimin-1.2.3.dist-info/licenses/" + license_name
    content = files[member].replace(b"\r\n", b"\n").replace(b"\n", b"\r\n")
    files[member] = (
        content.replace(b"License", b"Modified", 1)
        if damage == "text"
        else content.replace(b" ", b"  ", 1)
    )
    with zipfile.ZipFile(wheel, "w") as archive:
        for name, content in files.items():
            archive.writestr(name, content)
    write_checksums(release_assets)
    result = prepare(release_assets)
    assert result.returncode != 0
    assert "unexpected license text" in result.stderr
    assert not (release_assets.parent / "dist").exists()


@pytest.mark.parametrize(
    "damage",
    [
        "missing-platform",
        "extra-wheel",
        "corrupt-wheel",
        "missing-checksum",
        "duplicate-checksum",
    ],
)
def test_rejects_bad_assets_before_staging(release_assets: Path, damage: str) -> None:
    wheel = release_assets / "hoimin-1.2.3-py3-none-win_amd64.whl"
    if damage == "missing-platform":
        wheel.unlink()
    elif damage == "extra-wheel":
        write_wheel(release_assets / "hoimin-1.2.3-py3-none-any.whl")
        write_checksums(release_assets)
    elif damage == "corrupt-wheel":
        wheel.write_bytes(b"corrupted")
    elif damage == "missing-checksum":
        (release_assets / "SHA256SUMS").write_text("")
    else:
        sums = release_assets / "SHA256SUMS"
        sums.write_text(sums.read_text() * 2)
    result = prepare(release_assets)
    assert result.returncode != 0
    assert not (release_assets.parent / "dist").exists()


@pytest.mark.parametrize(
    "damage",
    [
        "old-license",
        "wrong-version",
        "missing-license",
        "wrong-license-text",
        "duplicate-metadata",
        "wrong-project",
        "wrong-python",
    ],
)
def test_rejects_invalid_wheel_contents(release_assets: Path, damage: str) -> None:
    wheel = release_assets / "hoimin-1.2.3-py3-none-win_amd64.whl"
    with zipfile.ZipFile(wheel) as archive:
        files = {name: archive.read(name) for name in archive.namelist()}
    metadata = "hoimin-1.2.3.dist-info/METADATA"
    license_file = "hoimin-1.2.3.dist-info/licenses/LICENSE"
    replacements = {
        "old-license": (b"Elastic-2.0", b"MIT"),
        "wrong-version": (b"Version: 1.2.3", b"Version: 1.2.4"),
        "wrong-project": (b"Name: hoimin", b"Name: other"),
        "wrong-python": (b">=3.14,<3.15", b">=3.8"),
    }
    if damage in replacements:
        files[metadata] = files[metadata].replace(*replacements[damage])
    elif damage == "missing-license":
        del files[license_file]
    elif damage == "wrong-license-text":
        files[license_file] = b"different terms"
    else:
        files["other.dist-info/METADATA"] = files[metadata]
    with zipfile.ZipFile(wheel, "w") as archive:
        for name, contents in files.items():
            archive.writestr(name, contents)
    write_checksums(release_assets)
    result = prepare(release_assets)
    assert result.returncode != 0
    assert not (release_assets.parent / "dist").exists()


@pytest.mark.parametrize("tag", ["main", "v1.2.3-dev.1", "v01.2.3", "v1.2.3;echo oops"])
def test_rejects_nonstable_tags(release_assets: Path, tag: str) -> None:
    assert prepare(release_assets, tag).returncode != 0
    assert not (release_assets.parent / "dist").exists()


@pytest.mark.parametrize(
    ("field", "value"),
    [("isDraft", True), ("isPrerelease", True), ("tagName", "v9.9.9")],
)
def test_requires_matching_published_release(
    release_assets: Path, field: str, *, value: str | bool
) -> None:
    state = {"tagName": "v1.2.3", "isDraft": False, "isPrerelease": False}
    state[field] = value
    (release_assets.parent / "release.json").write_text(json.dumps(state))
    assert prepare(release_assets).returncode != 0
    assert not (release_assets.parent / "dist").exists()


def test_pypi_workflow_keeps_oidc_in_protected_upload_job() -> None:
    workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
    assert workflow["permissions"] == {"contents": "read"}
    prepare_job = workflow["jobs"]["pypi-prepare"]
    assert "github.ref == 'refs/heads/main'" in prepare_job["if"]
    assert "github.repository == 'HayaoSuzuki/hoimin'" in prepare_job["if"]
    assert "id-token" not in prepare_job.get("permissions", {})
    publish = workflow["jobs"]["pypi-publish"]
    assert publish["needs"] == ["pypi-prepare", "pypi-inspect"]
    assert publish["environment"]["name"] == "pypi"
    assert publish["permissions"] == {"id-token": "write"}
    assert len(publish["steps"]) == 2
    assert publish["steps"][0]["uses"].startswith("actions/download-artifact@")
    upload = publish["steps"][1]
    assert upload["uses"].startswith("pypa/gh-action-pypi-publish@")
    assert upload["with"]["packages-dir"] == "dist/"
    assert "password" not in upload["with"]
    assert upload["with"].get("skip-existing", False) is False


def test_sbom_checksums_do_not_change_pypi_payload(release_assets: Path) -> None:
    sbom = release_assets / "hoimin-v1.2.3-linux-x86_64-wheel.cdx.json"
    sbom.write_text('{"bomFormat":"CycloneDX"}')
    sums = release_assets / "SHA256SUMS"
    sums.write_text(
        sums.read_text()
        + hashlib.sha256(sbom.read_bytes()).hexdigest()
        + "  "
        + sbom.name
        + "\n"
    )
    result = prepare(release_assets)
    assert result.returncode == 0, result.stderr
    output = release_assets.parent / "dist"
    assert len(list(output.iterdir())) == 3
    for path in output.iterdir():
        assert path.suffix == ".whl"
        assert path.read_bytes() == (release_assets / path.name).read_bytes()
