from __future__ import annotations

import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
import tomllib
import venv
import zipfile
from collections.abc import Mapping
from dataclasses import dataclass
from email.parser import Parser
from pathlib import Path

from packaging.tags import sys_tags
from packaging.utils import (
    InvalidWheelFilename,
    canonicalize_name,
    parse_wheel_filename,
)
from packaging.version import Version

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
COMMAND_TIMEOUT_SECONDS = 120


@dataclass(frozen=True)
class WheelMetadata:
    requires_python: str
    requires_dist: list[str] | None
    license_expression: str
    project_urls: list[str] | None


EXPECTED_WHEEL_METADATA = WheelMetadata(
    requires_python=">=3.14,<3.15",
    requires_dist=None,
    license_expression="MIT",
    project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
)

ISOLATED_ENVIRONMENT_REMOVALS = ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV")
FIXTURE_SOURCE = "def add(left, right):\n    return left + right\n"
FIXTURE_TEST = "from src.calc import add\n\n\ndef test_add():\n    assert add(2, 1) == 3\n"


def run(argv: list[str], *, cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        argv,
        cwd=cwd,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        shell=False,
        timeout=COMMAND_TIMEOUT_SECONDS,
        check=False,
    )
    assert completed.returncode == 0, (
        f"command failed ({completed.returncode}): {argv!r}\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
    )
    return completed


def assert_help_hides_python_option(
    completed: subprocess.CompletedProcess[str],
) -> None:
    assert "--python" not in completed.stdout, (
        f"installed help unexpectedly exposes --python in stdout: {completed.stdout!r}"
    )
    assert "--python" not in completed.stderr, (
        f"installed help unexpectedly exposes --python in stderr: {completed.stderr!r}"
    )


def assert_mutation_result(output: str) -> None:
    document = json.loads(output)
    mutants = document["mutants"]
    assert mutants, "mutation run produced no mutants"
    assert any(mutant["status"] in {"killed", "survived"} for mutant in mutants), (
        f"mutation run had no killed or survived mutant: {mutants!r}"
    )


def is_compatible_wheel(wheel: Path, system: str, machine: str) -> bool:
    try:
        _, _, _, tags = parse_wheel_filename(wheel.name)
    except InvalidWheelFilename:
        return False
    runtime_tags = {(tag.interpreter, tag.abi) for tag in sys_tags()}
    platforms = {tag.platform for tag in tags if (tag.interpreter, tag.abi) in runtime_tags}
    normalized_machine = machine.lower()
    if system == "win32":
        return normalized_machine in {"amd64", "x86_64"} and "win_amd64" in platforms
    if system.startswith("linux"):
        return normalized_machine in {"amd64", "x86_64"} and any(
            platform_tag.endswith("_x86_64") and platform_tag.startswith(("linux_", "manylinux", "musllinux"))
            for platform_tag in platforms
        )
    if system == "darwin":
        return normalized_machine == "arm64" and any(
            platform_tag.startswith("macosx_") and platform_tag.endswith("_arm64") for platform_tag in platforms
        )
    return False


def project_identity() -> tuple[str, Version]:
    document = tomllib.loads((REPOSITORY_ROOT / "pyproject.toml").read_text())
    project = document["project"]
    return canonicalize_name(project["name"]), Version(project["version"])


def select_compatible_wheel(
    wheels: list[Path],
    *,
    system: str,
    machine: str,
    expected_name: str,
    expected_version: str | Version,
) -> Path:
    normalized_name = canonicalize_name(expected_name)
    version = Version(expected_version) if isinstance(expected_version, str) else expected_version
    compatible = []
    for wheel in wheels:
        try:
            name, candidate_version, _, _ = parse_wheel_filename(wheel.name)
        except InvalidWheelFilename:
            continue
        if name == normalized_name and candidate_version == version and is_compatible_wheel(wheel, system, machine):
            compatible.append(wheel)
    names = [wheel.name for wheel in wheels]
    assert compatible, f"no current compatible wheel for {normalized_name} {version} on {system}/{machine}: {names}"
    assert len(compatible) == 1, (
        f"multiple current compatible wheels for {normalized_name} {version} on "
        f"{system}/{machine}: {[wheel.name for wheel in compatible]}"
    )
    return compatible[0]


def wheel_path(
    *,
    environment: Mapping[str, str],
    wheel_directory: Path,
    system: str,
    machine: str,
) -> Path:
    override = environment.get("HOIMIN_WHEEL")
    if override:
        wheel = Path(override).resolve()
        assert wheel.is_file(), f"HOIMIN_WHEEL does not exist: {wheel}"
        expected_name, expected_version = project_identity()
        return select_compatible_wheel(
            [wheel],
            system=system,
            machine=machine,
            expected_name=expected_name,
            expected_version=expected_version,
        )
    wheels = sorted(wheel_directory.glob("hoimin-*.whl"))
    assert wheels, "build a wheel first with uv run maturin build --release"
    expected_name, expected_version = project_identity()
    return select_compatible_wheel(
        wheels,
        system=system,
        machine=machine,
        expected_name=expected_name,
        expected_version=expected_version,
    )


def wheel_metadata(wheel: Path) -> WheelMetadata:
    with zipfile.ZipFile(wheel) as archive:
        metadata_files = sorted(name for name in archive.namelist() if name.endswith(".dist-info/METADATA"))
        assert len(metadata_files) == 1, f"expected exactly one METADATA member in {wheel}: {metadata_files}"
        parsed = Parser().parsestr(archive.read(metadata_files[0]).decode("utf-8"))

    return WheelMetadata(
        requires_python=parsed["Requires-Python"],
        requires_dist=parsed.get_all("Requires-Dist"),
        license_expression=parsed["License-Expression"],
        project_urls=parsed.get_all("Project-URL"),
    )


def validate_wheel_metadata(metadata: WheelMetadata) -> None:
    assert metadata.requires_python.replace(" ", "") == (EXPECTED_WHEEL_METADATA.requires_python), (
        f"unexpected Requires-Python: {metadata.requires_python!r}"
    )
    assert metadata.requires_dist == EXPECTED_WHEEL_METADATA.requires_dist, (
        f"unexpected Requires-Dist: {metadata.requires_dist!r}"
    )
    assert metadata.license_expression == EXPECTED_WHEEL_METADATA.license_expression, (
        f"unexpected License-Expression: {metadata.license_expression!r}"
    )
    assert metadata.project_urls == EXPECTED_WHEEL_METADATA.project_urls, (
        f"unexpected Project-URL: {metadata.project_urls!r}"
    )


def environment_python(root: Path, *, is_windows: bool) -> Path:
    return root / ("Scripts/python.exe" if is_windows else "bin/python")


def environment_hoimin(root: Path, *, is_windows: bool) -> Path:
    return root / ("Scripts/hoimin.exe" if is_windows else "bin/hoimin")


def isolated_environment(source: Mapping[str, str]) -> dict[str, str]:
    environment = dict(source)
    for name in ISOLATED_ENVIRONMENT_REMOVALS:
        environment.pop(name, None)
    environment["PYTHONNOUSERSITE"] = "1"
    return environment


def write_fixture(root: Path) -> Path:
    source = root / "src"
    tests = root / "tests"
    source.mkdir(parents=True)
    tests.mkdir()
    (source / "__init__.py").write_text("", encoding="utf-8")
    target = source / "calc.py"
    target.write_text(FIXTURE_SOURCE, encoding="utf-8")
    (tests / "test_calc.py").write_text(FIXTURE_TEST, encoding="utf-8")
    return target


def main() -> int:
    wheel = wheel_path(
        environment=os.environ,
        wheel_directory=REPOSITORY_ROOT / "target" / "wheels",
        system=sys.platform,
        machine=platform.machine().lower(),
    )
    validate_wheel_metadata(wheel_metadata(wheel))

    with tempfile.TemporaryDirectory(prefix="hoimin-wheel-smoke-") as temporary_directory:
        temporary_root = Path(temporary_directory)
        environment = isolated_environment(os.environ)
        is_windows = os.name == "nt"

        distribution_help = run(
            ["uvx", "--python", "3.14", "--from", str(wheel), "hoimin", "--help"],
            cwd=temporary_root,
            env=environment,
        )
        assert_help_hides_python_option(distribution_help)

        environment_root = temporary_root / "environment"
        venv.EnvBuilder(with_pip=True, clear=True).create(environment_root)
        python = environment_python(environment_root, is_windows=is_windows)
        run(
            [
                str(python),
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                str(wheel),
                "pytest>=8.4,<9",
            ],
            cwd=temporary_root,
            env=environment,
        )
        executable = environment_hoimin(environment_root, is_windows=is_windows)
        version = run([str(executable), "--version"], cwd=temporary_root, env=environment)
        assert (version.stdout + version.stderr).strip().startswith("hoimin ")

        fixture = temporary_root / "project"
        target = write_fixture(fixture)
        original = hashlib.sha256(target.read_bytes()).digest()
        completed = run(
            [
                str(executable),
                "run",
                "--root",
                str(fixture),
                "--source",
                "src",
                "--file",
                "src/calc.py",
                "--max-mutants",
                "16",
                "--max-candidates",
                "64",
                "--total-timeout",
                "60s",
                "--allow-best-effort-memory",
                "--format",
                "json",
                "--",
                str(python),
                "-m",
                "pytest",
                "-q",
            ],
            cwd=fixture,
            env=environment,
        )
        assert "--python" not in completed.stdout
        assert_mutation_result(completed.stdout)
        assert hashlib.sha256(target.read_bytes()).digest() == original
        assert "PYTHONPATH" not in environment

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
