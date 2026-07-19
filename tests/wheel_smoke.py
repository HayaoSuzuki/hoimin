from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import venv
import zipfile
from email.parser import Parser
from pathlib import Path
from typing import TYPE_CHECKING, cast
from unittest.mock import Mock

import pytest
from conftest import build_wheel_for_smoke_tests

if TYPE_CHECKING:
    from collections.abc import Callable

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


def run(argv: list[str], *, cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(  # noqa: S603, UP022 -- Smoke helper captures stdout and stderr separately for failure diagnostics.
        argv,
        cwd=cwd,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        shell=False,
        timeout=120,
        check=False,
    )
    assert completed.returncode == 0, (
        f"command failed ({completed.returncode}): {argv!r}\n"
        f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
    )
    return completed


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
        if (sys.platform == "win32" and "win_amd64" in wheel.name)
        or (sys.platform.startswith("linux") and "x86_64" in wheel.name)
    ]
    assert compatible, f"no wheel for {sys.platform}: {[wheel.name for wheel in wheels]}"
    return compatible[-1]


def wheel_metadata(wheel: Path):
    with zipfile.ZipFile(wheel) as archive:
        metadata_files = sorted(
            name for name in archive.namelist() if name.endswith(".dist-info/METADATA")
        )
        assert len(metadata_files) == 1
        return Parser().parsestr(archive.read(metadata_files[0]).decode("utf-8"))


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
    target.write_text(
        "def add(left, right):\n    return left + right\n",
        encoding="utf-8",
    )
    (tests / "__init__.py").write_text("", encoding="utf-8")
    (tests / "test_pytest.py").write_text(
        "from src.calc import add\n\ndef test_add():\n    assert add(2, 1) == 3\n",
        encoding="utf-8",
    )
    (tests / "test_unittest.py").write_text(
        "import unittest\n"
        "from src.calc import add\n\n"
        "class AddTests(unittest.TestCase):\n"
        "    def test_add(self):\n"
        "        self.assertEqual(add(2, 1), 3)\n",
        encoding="utf-8",
    )
    return target


def mutant_signature(mutant: object) -> tuple[str, str]:
    assert isinstance(mutant, dict)
    mutant_record = cast("dict[str, object]", mutant)
    candidate = mutant_record["candidate"]
    assert isinstance(candidate, dict)
    candidate_record = cast("dict[str, object]", candidate)
    return str(candidate_record["id"]), str(mutant_record["status"])


def result_signature(document: dict[str, object]) -> set[tuple[str, str]]:
    mutants = document["mutants"]
    assert isinstance(mutants, list)
    return {mutant_signature(mutant) for mutant in mutants}


def run_mutations(
    executable: Path,
    python: Path,
    fixture: Path,
    test_argv: list[str],
    environment: dict[str, str],
) -> dict[str, object]:
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
            "--python",
            str(python),
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
            *test_argv,
        ],
        cwd=fixture,
        env=environment,
    )
    document = json.loads(completed.stdout)
    assert document["run"]["resource_control"]["mode"] in {"hard", "best_effort"}
    assert document["baseline"]["resource_mode"] in {"hard", "best_effort"}
    assert document["mutants"]
    assert all(mutant["resource_mode"] in {"hard", "best_effort"} for mutant in document["mutants"])
    return document


def test_installed_wheel_is_checkout_independent(tmp_path: Path) -> None:
    wheel = wheel_path()
    metadata = wheel_metadata(wheel)
    assert metadata["Requires-Python"].replace(" ", "") == ">=3.14,<3.15"
    assert {value.replace(" ", "") for value in metadata.get_all("Requires-Dist")} >= {
        "libcst>=1.8.6,<2"
    }
    assert metadata["License-Expression"] == "MIT"
    assert metadata.get_all("Project-URL") == ["Repository, https://github.com/HayaoSuzuki/hoimin"]

    environment_root = tmp_path / "empty-environment"
    venv.EnvBuilder(with_pip=True, clear=True).create(environment_root)
    python = environment_python(environment_root)
    environment = isolated_environment()
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
        cwd=tmp_path,
        env=environment,
    )

    executable = environment_hoimin(environment_root)
    version = run([str(executable), "--version"], cwd=tmp_path, env=environment)
    assert (version.stdout + version.stderr).strip().startswith("hoimin ")

    fixture = tmp_path / "outside-checkout" / "project"
    target = write_fixture(fixture)
    original = hashlib.sha256(target.read_bytes()).digest()
    assert not fixture.is_relative_to(REPOSITORY_ROOT)

    pytest_result = run_mutations(
        executable,
        python,
        fixture,
        [str(python), "-m", "pytest", "-q", "tests/test_pytest.py"],
        environment,
    )
    unittest_result = run_mutations(
        executable,
        python,
        fixture,
        [str(python), "-m", "unittest", "tests.test_unittest"],
        environment,
    )

    assert result_signature(pytest_result) == result_signature(unittest_result)
    assert hashlib.sha256(target.read_bytes()).digest() == original
    assert "PYTHONPATH" not in environment


def test_wheel_path_uses_existing_override(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    wheel = tmp_path / "override.whl"
    wheel.touch()
    monkeypatch.setenv("HOIMIN_WHEEL", str(wheel))

    assert wheel_path() == wheel.resolve()


def test_wheel_path_rejects_missing_override(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setenv("HOIMIN_WHEEL", str(tmp_path / "missing.whl"))

    with pytest.raises(AssertionError, match="HOIMIN_WHEEL does not exist"):
        wheel_path()


def test_wheel_path_requires_a_compatible_platform_wheel(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.delenv("HOIMIN_WHEEL", raising=False)
    monkeypatch.setattr("wheel_smoke.REPOSITORY_ROOT", tmp_path)
    wheel_directory = tmp_path / "target" / "wheels"
    wheel_directory.mkdir(parents=True)

    with pytest.raises(AssertionError, match="build a wheel first"):
        wheel_path()

    (wheel_directory / "hoimin-0.1.0-py3-none-any.whl").touch()
    monkeypatch.setattr("wheel_smoke.sys.platform", "linux")
    with pytest.raises(AssertionError, match="no wheel for linux"):
        wheel_path()

    compatible_wheel = wheel_directory / "hoimin-0.1.0-cp314-cp314-win_amd64.whl"
    compatible_wheel.touch()
    monkeypatch.setattr("wheel_smoke.sys.platform", "win32")
    assert wheel_path() == compatible_wheel


def test_wheel_metadata_parses_and_validates_metadata_files(tmp_path: Path) -> None:
    wheel = tmp_path / "hoimin.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.writestr("hoimin-0.1.0.dist-info/METADATA", "Name: hoimin\nVersion: 0.1.0\n")

    metadata = wheel_metadata(wheel)

    assert metadata["Name"] == "hoimin"
    assert metadata["Version"] == "0.1.0"

    with zipfile.ZipFile(wheel, "w") as archive:
        archive.writestr("first.dist-info/METADATA", "Name: first\n")
        archive.writestr("second.dist-info/METADATA", "Name: second\n")
    with pytest.raises(AssertionError):
        wheel_metadata(wheel)


@pytest.mark.parametrize(
    ("platform_name", "python_path", "hoimin_path"),
    [("nt", "Scripts/python.exe", "Scripts/hoimin.exe"), ("posix", "bin/python", "bin/hoimin")],
)
def test_environment_paths_match_platform(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    platform_name: str,
    python_path: str,
    hoimin_path: str,
) -> None:
    monkeypatch.setattr("wheel_smoke.os.name", platform_name)

    assert environment_python(tmp_path) == tmp_path / python_path
    assert environment_hoimin(tmp_path) == tmp_path / hoimin_path


def test_isolated_environment_removes_inherited_python_settings(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("PYTHONPATH", "unexpected")
    monkeypatch.setenv("PYTHONHOME", "unexpected")
    monkeypatch.setenv("VIRTUAL_ENV", "unexpected")

    environment = isolated_environment()

    assert "PYTHONPATH" not in environment
    assert "PYTHONHOME" not in environment
    assert "VIRTUAL_ENV" not in environment
    assert environment["PYTHONNOUSERSITE"] == "1"


def test_wheel_override_skips_session_build(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setenv("HOIMIN_WHEEL", str(tmp_path / "override.whl"))
    sentinel = Mock()
    monkeypatch.setattr("conftest.subprocess.run", sentinel)

    # Pytest fixture wrapping retains the original callable on __wrapped__.
    wrapped_fixture = cast(
        "Callable[[], None]",
        build_wheel_for_smoke_tests.__wrapped__,  # ty: ignore[unresolved-attribute]
    )
    assert wrapped_fixture() is None
    sentinel.assert_not_called()


def test_wheel_path_selects_linux_x86_64_wheel(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.delenv("HOIMIN_WHEEL", raising=False)
    monkeypatch.setattr("wheel_smoke.REPOSITORY_ROOT", tmp_path)
    monkeypatch.setattr("wheel_smoke.os.name", "posix")
    monkeypatch.setattr("wheel_smoke.sys.platform", "linux")
    wheel_directory = tmp_path / "target" / "wheels"
    wheel_directory.mkdir(parents=True)
    compatible_wheel = wheel_directory / "hoimin-0.1.0-cp314-cp314-manylinux_x86_64.whl"
    compatible_wheel.touch()

    assert wheel_path() == compatible_wheel
