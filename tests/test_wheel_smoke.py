import os
import re
import subprocess
import sys
import zipfile
from pathlib import Path

import pytest
from packaging.tags import Tag
from pytest_mock import MockerFixture
from wheel_smoke import (
    COMMAND_TIMEOUT_SECONDS,
    WheelMetadata,
    assert_help_hides_python_option,
    assert_mutation_result,
    environment_hoimin,
    environment_python,
    is_compatible_wheel,
    isolated_environment,
    run,
    select_compatible_wheel,
    validate_wheel_metadata,
    wheel_metadata,
    wheel_path,
    write_fixture,
)

LINUX_RUNTIME_TAGS = frozenset(
    {
        Tag("cp314", "cp314", "manylinux_2_17_x86_64"),
        Tag("cp314", "abi3", "manylinux_2_17_x86_64"),
    }
)
WINDOWS_RUNTIME_TAGS = frozenset({Tag("cp314", "cp314", "win_amd64")})
MACOS_RUNTIME_TAGS = frozenset({Tag("cp314", "cp314", "macosx_11_0_arm64")})


def runtime_tags_for(system: str) -> frozenset[Tag]:
    tags = {
        "linux": LINUX_RUNTIME_TAGS,
        "win32": WINDOWS_RUNTIME_TAGS,
        "darwin": MACOS_RUNTIME_TAGS,
    }
    return tags.get(system, frozenset[Tag]())


@pytest.mark.parametrize(
    "job",
    [
        pytest.param("quality", id="quality"),
        pytest.param("wheel-smoke", id="wheel-smoke"),
    ],
)
def test_windows_quality_and_wheel_smoke_remain_in_manual_ci(job: str) -> None:
    repository_root = Path(__file__).resolve().parents[1]
    manual_ci = (repository_root / ".github/workflows/non-linux-ci.yml").read_text(
        encoding="utf-8"
    )
    section = re.search(
        rf"(?ms)^  {re.escape(job)}:\n(?P<body>.*?)(?=^  \S|\Z)", manual_ci
    )
    assert section is not None
    assert "windows-latest" in section.group("body")


def test_documentation_requires_build_before_standalone_smoke() -> None:
    repository_root = Path(__file__).resolve().parents[1]
    development = (repository_root / "docs/development.md").read_text(encoding="utf-8")
    readme = (repository_root / "README.md").read_text(encoding="utf-8")
    development_build_command = "uvx maturin build --release"
    readme_build_command = "uv run maturin build --release"
    development_smoke = "uv run --frozen python tests/wheel_smoke.py"
    readme_smoke = "uv run python tests/wheel_smoke.py"
    assert "must build a release wheel first" in " ".join(development.split())
    assert development.index(development_build_command) < development.index(
        development_smoke
    )
    assert readme.index(readme_build_command) < readme.index(readme_smoke)


def test_standalone_script_rejects_an_empty_wheel_directory(tmp_path: Path) -> None:
    repository_root = Path(__file__).resolve().parents[1]
    source = repository_root / "tests/wheel_smoke.py"
    temporary_root = tmp_path
    tests = temporary_root / "tests"
    tests.mkdir()
    script = tests / "wheel_smoke.py"
    script.write_bytes(source.read_bytes())
    environment = os.environ.copy()
    environment.pop("HOIMIN_WHEEL", None)
    # Execute an unchanged copy of the repository smoke helper.
    completed = subprocess.run(  # noqa: S603
        [sys.executable, str(script)],
        cwd=temporary_root,
        env=environment,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode != 0
    assert "build a wheel first" in completed.stderr
    assert not (temporary_root / "target/wheels").exists()


@pytest.mark.parametrize(
    ("filename", "system", "machine", "expected"),
    [
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl",
            "linux",
            "x86_64",
            True,
            id="Linux accepts x86_64",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-win_amd64.whl",
            "linux",
            "x86_64",
            False,
            id="Linux rejects Windows",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-macosx_14_0_x86_64.whl",
            "linux",
            "x86_64",
            False,
            id="Linux rejects macOS x86_64",
        ),
        pytest.param(
            "hoimin-0.1.0-cp313-cp313-manylinux_2_17_x86_64.whl",
            "linux",
            "x86_64",
            False,
            id="Linux rejects a wheel for another Python ABI",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-musllinux_1_2_x86_64.whl",
            "linux",
            "x86_64",
            False,
            id="Linux rejects musllinux on a glibc runtime",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-manylinux_2_99_x86_64.whl",
            "linux",
            "x86_64",
            False,
            id="Linux rejects a newer unsupported manylinux baseline",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-win_amd64.whl",
            "win32",
            "amd64",
            True,
            id="Windows accepts amd64",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl",
            "win32",
            "amd64",
            False,
            id="Windows rejects Linux",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-macosx_11_0_arm64.whl",
            "darwin",
            "arm64",
            True,
            id="macOS arm64 accepts arm64",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-macosx_11_0_universal2.whl",
            "darwin",
            "arm64",
            False,
            id="macOS arm64 rejects universal2",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-macosx_99_0_arm64.whl",
            "darwin",
            "arm64",
            False,
            id="macOS rejects a newer unsupported deployment target",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-macosx_11_0_arm64.whl",
            "darwin",
            "x86_64",
            False,
            id="macOS Intel rejects arm64",
        ),
        pytest.param(
            "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl",
            "freebsd",
            "x86_64",
            False,
            id="unknown system rejects all",
        ),
        pytest.param(
            "hoimin-any_x86_64.whl",
            "linux",
            "x86_64",
            False,
            id="malformed filename fails closed",
        ),
    ],
)
def test_compatibility_cases(
    filename: str, system: str, machine: str, *, expected: bool
) -> None:
    wheel = Path(filename)
    actual = is_compatible_wheel(
        wheel, system, machine, supported_tags=runtime_tags_for(system)
    )
    assert actual is expected


def test_selects_the_expected_semantic_version() -> None:
    wheels = [
        Path("hoimin-0.9.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
        Path("hoimin-0.10.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
        Path("hoimin-0.10.0-cp314-cp314-win_amd64.whl"),
    ]
    actual = select_compatible_wheel(
        wheels,
        system="linux",
        machine="x86_64",
        expected_name="hoimin",
        expected_version="0.10.0",
        supported_tags=LINUX_RUNTIME_TAGS,
    )
    assert actual == Path("hoimin-0.10.0-cp314-cp314-manylinux_2_17_x86_64.whl")


def test_rejects_a_candidate_list_without_a_compatible_wheel() -> None:
    wheels = [Path("hoimin-0.1.0-cp314-cp314-win_amd64.whl")]
    with pytest.raises(AssertionError, match="no current compatible wheel"):
        select_compatible_wheel(
            wheels,
            system="linux",
            machine="x86_64",
            expected_name="hoimin",
            expected_version="0.1.0",
            supported_tags=LINUX_RUNTIME_TAGS,
        )


def test_rejects_stale_compatible_wheels() -> None:
    wheels = [Path("hoimin-0.9.0-cp314-cp314-manylinux_2_17_x86_64.whl")]
    with pytest.raises(AssertionError, match="no current compatible wheel"):
        select_compatible_wheel(
            wheels,
            system="linux",
            machine="x86_64",
            expected_name="hoimin",
            expected_version="0.10.0",
            supported_tags=LINUX_RUNTIME_TAGS,
        )


def test_rejects_ambiguous_current_compatible_wheels() -> None:
    wheels = [
        Path("hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
        Path("hoimin-0.1.0-cp314-abi3-manylinux_2_17_x86_64.whl"),
    ]
    with pytest.raises(AssertionError, match="multiple current compatible wheels"):
        select_compatible_wheel(
            wheels,
            system="linux",
            machine="x86_64",
            expected_name="hoimin",
            expected_version="0.1.0",
            supported_tags=LINUX_RUNTIME_TAGS,
        )


def test_uses_the_explicit_wheel_override(tmp_path: Path) -> None:
    override = tmp_path / "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl"
    override.touch()
    actual = wheel_path(
        environment={"HOIMIN_WHEEL": str(override)},
        wheel_directory=tmp_path / "wheels",
        system="linux",
        machine="x86_64",
        supported_tags=LINUX_RUNTIME_TAGS,
    )
    assert actual == override.resolve()


def test_rejects_a_stale_explicit_wheel_override(tmp_path: Path) -> None:
    override = tmp_path / "hoimin-0.9.0-cp314-cp314-manylinux_2_17_x86_64.whl"
    override.touch()
    with pytest.raises(AssertionError, match="no current compatible wheel"):
        wheel_path(
            environment={"HOIMIN_WHEEL": str(override)},
            wheel_directory=tmp_path / "wheels",
            system="linux",
            machine="x86_64",
            supported_tags=LINUX_RUNTIME_TAGS,
        )


def test_rejects_a_missing_explicit_wheel_override(tmp_path: Path) -> None:
    missing = tmp_path / "missing.whl"
    with pytest.raises(AssertionError, match="HOIMIN_WHEEL does not exist"):
        wheel_path(
            environment={"HOIMIN_WHEEL": str(missing)},
            wheel_directory=tmp_path,
            system="linux",
            machine="x86_64",
        )


def test_rejects_an_empty_wheel_directory(tmp_path: Path) -> None:
    wheel_directory = tmp_path
    with pytest.raises(AssertionError, match="build a wheel first"):
        wheel_path(
            environment={},
            wheel_directory=wheel_directory,
            system="linux",
            machine="x86_64",
        )


def test_discovers_the_current_compatible_wheel(tmp_path: Path) -> None:
    wheel_directory = tmp_path
    stale = wheel_directory / "hoimin-0.0.9-cp314-cp314-manylinux_2_17_x86_64.whl"
    current = wheel_directory / "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl"
    incompatible = wheel_directory / "hoimin-0.1.0-cp314-cp314-win_amd64.whl"
    stale.touch()
    current.touch()
    incompatible.touch()
    actual = wheel_path(
        environment={},
        wheel_directory=wheel_directory,
        system="linux",
        machine="x86_64",
        supported_tags=LINUX_RUNTIME_TAGS,
    )
    assert actual == current


def test_reads_the_single_metadata_member(tmp_path: Path) -> None:
    wheel = tmp_path / "hoimin.whl"
    text = (
        "Requires-Python: >=3.14, <3.15\n"
        "License-Expression: MIT\n"
        "Project-URL: Repository, https://github.com/tokyogas-tech/hoimin\n"
    )
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.writestr("hoimin-0.1.0.dist-info/METADATA", text)
    actual = wheel_metadata(wheel)
    assert actual == WheelMetadata(
        requires_python=">=3.14, <3.15",
        requires_dist=None,
        license_expression="MIT",
        project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
    )


def test_rejects_an_archive_without_metadata(tmp_path: Path) -> None:
    wheel = tmp_path / "hoimin.whl"
    with zipfile.ZipFile(wheel, "w"):
        pass
    with pytest.raises(AssertionError, match="expected exactly one METADATA"):
        wheel_metadata(wheel)


def test_rejects_an_archive_with_multiple_metadata_members(tmp_path: Path) -> None:
    wheel = tmp_path / "hoimin.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.writestr("one.dist-info/METADATA", "License-Expression: MIT\n")
        archive.writestr("two.dist-info/METADATA", "License-Expression: MIT\n")
    with pytest.raises(AssertionError, match="expected exactly one METADATA"):
        wheel_metadata(wheel)


def test_accepts_the_expected_metadata() -> None:
    metadata = WheelMetadata(
        requires_python=">=3.14, <3.15",
        requires_dist=None,
        license_expression="MIT",
        project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
    )
    actual = validate_wheel_metadata(metadata)
    assert actual is None


@pytest.mark.parametrize(
    ("field", "metadata"),
    [
        pytest.param(
            "Requires-Python",
            WheelMetadata(
                requires_python=">=3.13,<3.15",
                requires_dist=None,
                license_expression="MIT",
                project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
            ),
            id="Requires-Python",
        ),
        pytest.param(
            "Requires-Dist",
            WheelMetadata(
                requires_python=">=3.14,<3.15",
                requires_dist=["pytest"],
                license_expression="MIT",
                project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
            ),
            id="Requires-Dist",
        ),
        pytest.param(
            "License-Expression",
            WheelMetadata(
                requires_python=">=3.14,<3.15",
                requires_dist=None,
                license_expression="Apache-2.0",
                project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
            ),
            id="License-Expression",
        ),
        pytest.param(
            "Project-URL",
            WheelMetadata(
                requires_python=">=3.14,<3.15",
                requires_dist=None,
                license_expression="MIT",
                project_urls=["Homepage, https://example.invalid/"],
            ),
            id="Project-URL",
        ),
    ],
)
def test_rejects_each_unexpected_metadata_field(
    field: str, metadata: WheelMetadata
) -> None:
    with pytest.raises(AssertionError, match=field):
        validate_wheel_metadata(metadata)


def test_isolated_environment_removes_python_import_state() -> None:
    source = {
        "KEEP": "value",
        "PYTHONHOME": "/python-home",
        "PYTHONPATH": "/checkout",
        "VIRTUAL_ENV": "/virtual-environment",
    }
    actual = isolated_environment(source)
    assert actual == {"KEEP": "value", "PYTHONNOUSERSITE": "1"}


@pytest.mark.parametrize(
    ("is_windows", "expected"),
    [
        pytest.param(False, Path("environment/bin/python"), id="POSIX"),
        pytest.param(True, Path("environment/Scripts/python.exe"), id="Windows"),
    ],
)
def test_environment_python_paths(*, is_windows: bool, expected: Path) -> None:
    root = Path("environment")
    actual = environment_python(root, is_windows=is_windows)
    assert actual == expected


@pytest.mark.parametrize(
    ("is_windows", "expected"),
    [
        pytest.param(False, Path("environment/bin/hoimin"), id="POSIX"),
        pytest.param(True, Path("environment/Scripts/hoimin.exe"), id="Windows"),
    ],
)
def test_environment_hoimin_paths(*, is_windows: bool, expected: Path) -> None:
    root = Path("environment")
    actual = environment_hoimin(root, is_windows=is_windows)
    assert actual == expected


def test_write_fixture_creates_the_expected_project(tmp_path: Path) -> None:
    root = tmp_path / "project"
    target = write_fixture(root)
    assert target == root / "src" / "calc.py"
    assert (root / "src" / "__init__.py").read_text() == ""
    assert target.read_text() == "def add(left, right):\n    return left + right\n"
    assert (
        (root / "tests" / "test_calc.py").read_text()
        == "from src.calc import add\n\n\ndef test_add():\n    assert add(2, 1) == 3\n"
    )


def test_run_returns_a_successful_completed_process(mocker: MockerFixture) -> None:
    argv = ["hoimin", "--version"]
    completed = subprocess.CompletedProcess(argv, 0, stdout="hoimin 0.1.0\n", stderr="")
    mocked_run = mocker.patch("wheel_smoke.subprocess.run", return_value=completed)
    actual = run(argv, cwd=Path("work"), env={"PYTHONNOUSERSITE": "1"})
    assert actual is completed
    mocked_run.assert_called_once_with(
        argv,
        cwd=Path("work"),
        env={"PYTHONNOUSERSITE": "1"},
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        shell=False,
        timeout=COMMAND_TIMEOUT_SECONDS,
        check=False,
    )


def test_run_reports_command_output_for_a_failure(mocker: MockerFixture) -> None:
    argv = ["hoimin", "run"]
    completed = subprocess.CompletedProcess(
        argv, 7, stdout="command output", stderr="command error"
    )
    mocker.patch("wheel_smoke.subprocess.run", return_value=completed)
    with pytest.raises(
        AssertionError,
        match=r"(?s)command failed \(7\): .*stdout:\ncommand output"
        r"\nstderr:\ncommand error",
    ):
        run(argv, cwd=Path("work"), env={})


@pytest.mark.parametrize(
    "completed",
    [
        pytest.param(
            subprocess.CompletedProcess(["hoimin"], 0, stdout="--python", stderr=""),
            id="stdout",
        ),
        pytest.param(
            subprocess.CompletedProcess(["hoimin"], 0, stdout="", stderr="--python"),
            id="stderr",
        ),
    ],
)
def test_help_output_rejects_the_python_option(
    completed: subprocess.CompletedProcess[str],
) -> None:
    with pytest.raises(AssertionError, match="--python"):
        assert_help_hides_python_option(completed)


@pytest.mark.parametrize(
    "output",
    [
        pytest.param('{"mutants": [{"status": "killed"}]}', id="killed"),
        pytest.param('{"mutants": [{"status": "survived"}]}', id="survived"),
    ],
)
def test_mutation_result_accepts_terminal_mutants(output: str) -> None:
    actual = assert_mutation_result(output)
    assert actual is None


def test_mutation_result_rejects_an_empty_mutant_list() -> None:
    output = '{"mutants": []}'
    with pytest.raises(AssertionError, match="mutation run produced no mutants"):
        assert_mutation_result(output)


def test_mutation_result_rejects_a_nonterminal_mutant_list() -> None:
    output = '{"mutants": [{"status": "timeout"}]}'
    with pytest.raises(
        AssertionError, match="mutation run had no killed or survived mutant"
    ):
        assert_mutation_result(output)
