from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from tools import workflow_lint

FIXTURES = Path(__file__).parent / "fixtures" / "workflow-lint"


@pytest.mark.parametrize(
    ("tool", "fixture", "diagnostic"),
    [
        ("actionlint", "bad-expression.yml", "nonexistent"),
        ("actionlint", "bad-shell.yml", "SC2086"),
        ("zizmor", "dangerous-input.yml", "template-injection"),
        ("zizmor", "excessive-permissions.yml", "excessive-permissions"),
        ("zizmor", "anonymous-job.yml", "anonymous-definition"),
    ],
)
def test_real_linters_reject_isolated_unsafe_workflows(
    tool: str, fixture: str, diagnostic: str
) -> None:
    executable = shutil.which(tool)
    if executable is None:
        pytest.skip("Install the workflow dependency group to exercise real linters")
    args = (
        ["-shellcheck", "shellcheck", "-pyflakes", ""]
        if tool == "actionlint"
        else ["--offline", "--no-progress", "--persona", "auditor", "--format", "plain"]
    )
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


def test_long_shell_script_is_checked_without_a_pipe_deadlock(tmp_path: Path) -> None:
    executable = shutil.which("shellcheck")
    if executable is None:
        pytest.skip("Install the workflow dependency group")
    path = tmp_path / "long.yml"
    path.write_text(
        "name: Long script\non: push\njobs:\n  check:\n"
        "    runs-on: ubuntu-latest\n    steps:\n      - run: |\n"
        + "          echo safe\n" * 500
        + "          echo $unquoted\n",
        encoding="utf-8",
    )
    assert workflow_lint.check_shells([path], executable) == 1


def test_a_new_yaml_workflow_cannot_escape_the_runner(tmp_path: Path) -> None:
    if any(
        shutil.which(tool) is None for tool in ("actionlint", "shellcheck", "zizmor")
    ):
        pytest.skip("Install the workflow dependency group")
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


def test_shellcheck_does_not_suppress_actionlint_default_exclusions(
    tmp_path: Path,
) -> None:
    executable = shutil.which("shellcheck")
    if executable is None:
        pytest.skip("Install the workflow dependency group")
    path = tmp_path / "undefined.yml"
    path.write_text(
        "name: Undefined variable\non: push\njobs:\n  check:\n"
        '    runs-on: ubuntu-latest\n    steps:\n      - run: echo "$undefined"\n',
        encoding="utf-8",
    )
    # SC2154 is excluded by actionlint's built-in integration, but not our runner.
    assert workflow_lint.check_shells([path], executable) == 1
