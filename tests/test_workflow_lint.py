from __future__ import annotations

import os
import runpy
import shutil
import subprocess
import sys
from contextlib import chdir
from pathlib import Path
from unittest.mock import create_autospec

import pytest
from pytest_mock import MockerFixture

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
def test_real_linters_reject_isolated_unsafe_workflows(  # pragma: no cover
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


@pytest.mark.parametrize(
    ("scope", "fallback", "expected"),
    [
        ({}, "bash", "bash"),
        ({"defaults": {"run": {"shell": "sh -e"}}}, "bash", "sh"),
    ],
)
def test_default_shell_uses_explicit_shell_or_fallback(
    scope: dict[str, object], fallback: str, expected: str
) -> None:
    result = workflow_lint.default_shell(scope, fallback)

    assert result == expected


def test_runner_rejects_empty_workflow_directory(tmp_path: Path) -> None:
    (tmp_path / ".github/workflows").mkdir(parents=True)
    with chdir(tmp_path), pytest.raises(RuntimeError, match="No workflows found"):
        workflow_lint.main()


@pytest.mark.parametrize("missing", ["actionlint", "shellcheck", "zizmor"])
def test_runner_reports_missing_dependency(
    tmp_path: Path, mocker: MockerFixture, missing: str
) -> None:
    original_which_spec = shutil.which
    directory = tmp_path / ".github/workflows"
    directory.mkdir(parents=True)
    (directory / "ci.yml").write_text("jobs: {}", encoding="utf-8")
    with chdir(tmp_path):
        mocker.patch.object(
            shutil,
            "which",
            create_autospec(
                original_which_spec,
                side_effect=lambda tool: None if tool == missing else tool,
            ),
        )
        with pytest.raises(RuntimeError, match=f"Missing {missing};"):
            workflow_lint.main()


def test_shell_checker_rejects_non_list_steps(tmp_path: Path) -> None:
    path = tmp_path / "invalid.yml"
    path.write_text("jobs:\n  test:\n    steps: {}\n", encoding="utf-8")
    with pytest.raises(TypeError):
        workflow_lint.check_shells([path], "unused-shellcheck")


def test_shell_checker_skips_non_shell_steps(tmp_path: Path) -> None:
    path = tmp_path / "windows.yml"
    path.write_text(
        "jobs:\n  test:\n    runs-on: windows-latest\n    steps:\n"
        "      - uses: actions/checkout@v4\n      - run: Write-Host hello\n",
        encoding="utf-8",
    )

    result = workflow_lint.check_shells([path], "unused-shellcheck")

    assert result == 0


def test_missing_linter_fixture_reports_skip(mocker: MockerFixture) -> None:
    request = mocker.Mock(spec=pytest.FixtureRequest)
    request.param = "missing-linter"
    mocker.patch.object(shutil, "which", return_value=None)
    with pytest.raises(pytest.skip.Exception, match="Install the workflow dependency"):
        executable._get_wrapped_function()(request)  # noqa: SLF001 -- Test the fixture's skip diagnostic.


@pytest.mark.parametrize(
    ("overrides", "expected_shell"),
    [
        (("", "", ""), "bash"),
        (("", "    runs-on: windows-latest\n", "        shell: bash -e {0}\n"), "bash"),
        (("defaults:\n  run:\n    shell: sh -e\n", "", ""), "sh"),
        (
            (
                "defaults:\n  run:\n    shell: sh\n",
                "    defaults:\n      run:\n        shell: bash\n",
                "",
            ),
            "bash",
        ),
        (
            (
                "",
                "    defaults:\n      run:\n        shell: bash\n",
                "        shell: sh -e {0}\n",
            ),
            "sh",
        ),
    ],
    ids=[
        "unix-default",
        "windows-override",
        "workflow",
        "job-override",
        "step-override",
    ],
)
def test_shell_checker_selects_shell_and_replaces_multiline_expressions(
    tmp_path: Path,
    mocker: MockerFixture,
    overrides: tuple[str, str, str],
    expected_shell: str,
) -> None:
    defaults, job, step = overrides
    path = tmp_path / "ci.yml"
    path.write_text(
        defaults
        + "jobs:\n  check:\n"
        + job
        + "    steps:\n      - run: |\n"
        + "          echo '${{\n            github.ref\n          }}'\n"
        + step,
        encoding="utf-8",
    )
    run = mocker.patch.object(
        subprocess,
        "run",
        autospec=True,
        return_value=subprocess.CompletedProcess([], 0),
    )

    result = workflow_lint.check_shells([path], "resolved-shellcheck")

    assert result == 0
    run.assert_called_once_with(
        ["resolved-shellcheck", "--norc", "--shell", expected_shell, "-x", "-"],
        input=b"echo 'expression'\n",
        check=False,
        timeout=30,
    )


@pytest.mark.parametrize(
    ("return_codes", "expected"),
    [
        ((0, 0, 0), 0),
        ((1, 0, 0), 1),
        ((0, 2, 0), 1),
        ((0, 0, 1), 1),
        ((1, 2, 1), 1),
    ],
    ids=["success", "actionlint-fails", "zizmor-fails", "shellcheck-fails", "all-fail"],
)
def test_runner_exit_status_aggregates_failures_without_skipping_checks(
    tmp_path: Path,
    mocker: MockerFixture,
    return_codes: tuple[int, int, int],
    expected: int,
) -> None:
    directory = tmp_path / ".github/workflows"
    directory.mkdir(parents=True)
    (directory / "ci.yml").write_text(
        "jobs:\n  check:\n    steps:\n      - run: echo safe\n", encoding="utf-8"
    )
    mocker.patch.object(shutil, "which", autospec=True, return_value="shellcheck")
    run = mocker.patch.object(
        subprocess,
        "run",
        autospec=True,
        side_effect=[
            subprocess.CompletedProcess([], return_codes[0]),
            subprocess.CompletedProcess([], return_codes[1]),
            subprocess.CompletedProcess([], return_codes[2]),
        ],
    )

    with chdir(tmp_path), pytest.raises(SystemExit) as raised:
        runpy.run_path(str(Path(workflow_lint.__file__).resolve()), run_name="__main__")

    assert raised.value.code == expected
    assert run.call_count == 3


@pytest.mark.parametrize(
    ("return_codes", "expected", "diagnostics"),
    [
        ((0, 0), 0, ""),
        ((1, 0), 1, "ShellCheck: {path}, job first, step 2\n"),
        ((0, 1), 1, "ShellCheck: {path}, job second, step 1\n"),
        (
            (1, 1),
            1,
            (
                "ShellCheck: {path}, job first, step 2\n"
                "ShellCheck: {path}, job second, step 1\n"
            ),
        ),
    ],
    ids=["success", "first-job-fails", "last-job-fails", "both-jobs-fail"],
)
def test_shell_checker_keeps_failure_evidence_across_jobs(  # noqa: PLR0913 -- One case includes exit status and diagnostics.
    tmp_path: Path,
    mocker: MockerFixture,
    capsys: pytest.CaptureFixture[str],
    *,
    return_codes: tuple[int, int],
    expected: int,
    diagnostics: str,
) -> None:
    path = tmp_path / "ci.yml"
    path.write_text(
        "jobs:\n  first:\n    steps:\n      - uses: actions/checkout@v4\n"
        "      - run: echo first\n  second:\n    steps:\n      - run: echo second\n",
        encoding="utf-8",
    )
    run = mocker.patch.object(
        subprocess,
        "run",
        autospec=True,
        side_effect=[
            subprocess.CompletedProcess([], return_codes[0]),
            subprocess.CompletedProcess([], return_codes[1]),
        ],
    )

    result = workflow_lint.check_shells([path], "resolved-shellcheck")

    assert result == expected
    assert capsys.readouterr().out == diagnostics.format(path=path)
    assert run.call_count == 2


def test_runner_checks_both_yaml_extensions_in_sorted_order(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    directory = tmp_path / ".github/workflows"
    directory.mkdir(parents=True)
    (directory / "z.yml").write_text("jobs: {}\n", encoding="utf-8")
    (directory / "a.yaml").write_text("jobs: {}\n", encoding="utf-8")
    (directory / "ignored.txt").write_text("not a workflow", encoding="utf-8")
    mocker.patch.object(shutil, "which", autospec=True, return_value="shellcheck")
    run = mocker.patch.object(
        subprocess,
        "run",
        autospec=True,
        return_value=subprocess.CompletedProcess([], 0),
    )

    with chdir(tmp_path):
        result = workflow_lint.main()

    assert result == 0
    assert run.call_args_list == [
        mocker.call(
            [
                "actionlint",
                "-shellcheck",
                "",
                "-pyflakes",
                "",
                str(Path(".github/workflows/a.yaml")),
                str(Path(".github/workflows/z.yml")),
            ],
            check=False,
            timeout=180,
        ),
        mocker.call(
            [
                "zizmor",
                "--offline",
                "--no-progress",
                "--persona",
                "auditor",
                "--format",
                "plain",
                str(Path(".github/workflows/a.yaml")),
                str(Path(".github/workflows/z.yml")),
            ],
            check=False,
            timeout=180,
        ),
    ]
