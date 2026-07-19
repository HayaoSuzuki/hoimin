from __future__ import annotations

import hashlib
from email.parser import Parser
import json
import os
from pathlib import Path
import subprocess
import sys
import venv
import zipfile


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


def run(
    argv: list[str], *, cwd: Path, env: dict[str, str]
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
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
        "from src.calc import add\n\n"
        "def test_add():\n"
        "    assert add(2, 1) == 3\n",
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


def result_signature(document: dict[str, object]) -> set[tuple[str, str]]:
    mutants = document["mutants"]
    assert isinstance(mutants, list)
    return {
        (str(mutant["candidate"]["id"]), str(mutant["status"]))
        for mutant in mutants
    }


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
    assert all(
        mutant["resource_mode"] in {"hard", "best_effort"}
        for mutant in document["mutants"]
    )
    return document


def test_installed_wheel_is_checkout_independent(tmp_path: Path) -> None:
    wheel = wheel_path()
    metadata = wheel_metadata(wheel)
    assert metadata["Requires-Python"].replace(" ", "") == ">=3.12,<3.15"
    assert {value.replace(" ", "") for value in metadata.get_all("Requires-Dist")} >= {
        "libcst>=1.8.6,<2"
    }
    assert metadata["License-Expression"] == "MIT"
    assert metadata.get_all("Project-URL") == [
        "Repository, https://github.com/HayaoSuzuki/hoimin"
    ]

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
