from __future__ import annotations

import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
import venv
import zipfile
from dataclasses import dataclass
from email.parser import Parser
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class WheelMetadata:
    requires_python: str
    requires_dist: list[str] | None
    license_expression: str
    project_urls: list[str] | None


def run(argv: list[str], *, cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, shell=False, timeout=120, check=False)
    assert completed.returncode == 0, f"command failed ({completed.returncode}): {argv!r}\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
    return completed


def is_compatible_wheel(wheel: Path, system: str, machine: str) -> bool:
    if system == "win32":
        return "win_amd64" in wheel.name
    if system.startswith("linux"):
        return "x86_64" in wheel.name
    if system == "darwin":
        return machine == "arm64" and "macosx" in wheel.name and "arm64" in wheel.name
    return False


def wheel_path() -> Path:
    override = os.environ.get("HOIMIN_WHEEL")
    if override:
        wheel = Path(override).resolve()
        assert wheel.is_file(), f"HOIMIN_WHEEL does not exist: {wheel}"
        return wheel
    wheels = sorted((REPOSITORY_ROOT / "target" / "wheels").glob("hoimin-*.whl"))
    assert wheels, "build a wheel first with uv run maturin build --release"
    compatible = [
        wheel
        for wheel in wheels
        if is_compatible_wheel(wheel, sys.platform, platform.machine().lower())
    ]
    assert compatible, f"no wheel for {sys.platform}: {[wheel.name for wheel in wheels]}"
    return compatible[-1]


def wheel_metadata(wheel: Path) -> WheelMetadata:
    with zipfile.ZipFile(wheel) as archive:
        metadata_files = sorted(name for name in archive.namelist() if name.endswith(".dist-info/METADATA"))
        assert len(metadata_files) == 1
        parsed = Parser().parsestr(archive.read(metadata_files[0]).decode("utf-8"))
    return WheelMetadata(parsed["Requires-Python"], parsed.get_all("Requires-Dist"), parsed["License-Expression"], parsed.get_all("Project-URL"))


def environment_python(root: Path) -> Path:
    return root / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def environment_hoimin(root: Path) -> Path:
    return root / ("Scripts/hoimin.exe" if os.name == "nt" else "bin/hoimin")


def isolated_environment() -> dict[str, str]:
    environment = os.environ.copy()
    for name in ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV"):
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
    target.write_text("def add(left, right):\n    return left + right\n", encoding="utf-8")
    (tests / "test_calc.py").write_text("from src.calc import add\n\n\ndef test_add():\n    assert add(2, 1) == 3\n", encoding="utf-8")
    return target


def main() -> int:
    wheel = wheel_path()
    metadata = wheel_metadata(wheel)
    assert metadata.requires_python.replace(" ", "") == ">=3.14,<3.15"
    assert metadata.requires_dist is None
    assert metadata.license_expression == "MIT"
    assert metadata.project_urls == ["Repository, https://github.com/HayaoSuzuki/hoimin"]
    with tempfile.TemporaryDirectory(prefix="hoimin-wheel-smoke-") as temporary_directory:
        temporary_root = Path(temporary_directory)
        environment = isolated_environment()
        distribution_help = run(["uvx", "--python", "3.14", "--from", str(wheel), "hoimin", "--help"], cwd=temporary_root, env=environment)
        assert "--python" not in distribution_help.stdout
        assert "--python" not in distribution_help.stderr
        environment_root = temporary_root / "environment"
        venv.EnvBuilder(with_pip=True, clear=True).create(environment_root)
        python = environment_python(environment_root)
        run([str(python), "-m", "pip", "install", "--disable-pip-version-check", str(wheel), "pytest>=8.4,<9"], cwd=temporary_root, env=environment)
        executable = environment_hoimin(environment_root)
        version = run([str(executable), "--version"], cwd=temporary_root, env=environment)
        assert (version.stdout + version.stderr).strip().startswith("hoimin ")
        fixture = temporary_root / "project"
        target = write_fixture(fixture)
        original = hashlib.sha256(target.read_bytes()).digest()
        completed = run([str(executable), "run", "--root", str(fixture), "--source", "src", "--file", "src/calc.py", "--max-mutants", "16", "--max-candidates", "64", "--total-timeout", "60s", "--allow-best-effort-memory", "--format", "json", "--", str(python), "-m", "pytest", "-q"], cwd=fixture, env=environment)
        assert "--python" not in completed.stdout
        document = json.loads(completed.stdout)
        assert document["mutants"]
        assert any(mutant["status"] in {"killed", "survived"} for mutant in document["mutants"])
        assert hashlib.sha256(target.read_bytes()).digest() == original
        assert "PYTHONPATH" not in environment
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
