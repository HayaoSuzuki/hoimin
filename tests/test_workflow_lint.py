from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from tools import workflow_lint

LINTER_ARGUMENTS = {
    "actionlint": ["-shellcheck", "shellcheck", "-pyflakes", ""],
    "zizmor": [
        "--offline",
        "--no-progress",
        "--persona",
        "auditor",
        "--format",
        "plain",
    ],
}


@pytest.fixture
def executable(request: pytest.FixtureRequest) -> str:
    assert isinstance(request.param, str)
    path = shutil.which(request.param)
    if path is None:
        pytest.skip("Install the workflow dependency group to exercise real linters")
    return path


FIXTURES = Path(__file__).parent / "fixtures" / "workflow-lint"


@pytest.mark.parametrize(
    ("executable", "fixture", "diagnostic"),
    [
        ("actionlint", "bad-expression.yml", "nonexistent"),
        ("actionlint", "bad-shell.yml", "SC2086"),
        ("zizmor", "dangerous-input.yml", "template-injection"),
        ("zizmor", "excessive-permissions.yml", "excessive-permissions"),
        ("zizmor", "anonymous-job.yml", "anonymous-definition"),
    ],
    indirect=["executable"],
)
def test_real_linters_reject_isolated_unsafe_workflows(
    executable: str, fixture: str, diagnostic: str
) -> None:
    args = LINTER_ARGUMENTS[Path(executable).stem]

    result = subprocess.run(  # noqa: S603
        [executable, *args, str(FIXTURES / fixture)],
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
        timeout=60,
    )

    assert result.returncode != 0
    assert diagnostic in result.stdout + result.stderr


@pytest.mark.parametrize("executable", ["shellcheck"], indirect=True)
def test_long_shell_script_is_checked_without_a_pipe_deadlock(
    tmp_path: Path, executable: str
) -> None:
    path = tmp_path / "long.yml"
    path.write_text(
        "name: Long script\non: push\njobs:\n  check:\n"
        "    runs-on: ubuntu-latest\n    steps:\n      - run: |\n"
        + "          echo safe\n" * 500
        + "          echo $unquoted\n",
        encoding="utf-8",
    )

    check_shells_result = workflow_lint.check_shells([path], executable)

    assert check_shells_result == 1


@pytest.mark.skipif(
    any(shutil.which(tool) is None for tool in ("actionlint", "shellcheck", "zizmor")),
    reason="Install the workflow dependency group",
)
def test_a_new_yaml_workflow_cannot_escape_the_runner(tmp_path: Path) -> None:
    directory = tmp_path / ".github/workflows"
    directory.mkdir(parents=True)
    shutil.copyfile(FIXTURES / "bad-expression.yml", directory / "new.yaml")
    environment = os.environ | {"PYTHONPATH": str(Path(__file__).resolve().parents[1])}

    result = subprocess.run(
        [sys.executable, "-m", "tools.workflow_lint"],
        cwd=tmp_path,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=60,
    )

    assert result.returncode == 1
    assert "nonexistent" in result.stdout + result.stderr


@pytest.mark.parametrize("executable", ["shellcheck"], indirect=True)
def test_shellcheck_does_not_suppress_actionlint_default_exclusions(
    tmp_path: Path,
    executable: str,
) -> None:
    path = tmp_path / "undefined.yml"
    path.write_text(
        "name: Undefined variable\non: push\njobs:\n  check:\n"
        '    runs-on: ubuntu-latest\n    steps:\n      - run: echo "$undefined"\n',
        encoding="utf-8",
    )

    # SC2154 is excluded by actionlint's built-in integration, but not our runner.
    check_shells_result = workflow_lint.check_shells([path], executable)

    assert check_shells_result == 1
