import os, shutil, subprocess, sys
from pathlib import Path
import pytest
from tools import ci_selection

def git(root: Path, *args: str) -> str:
    executable = shutil.which("git")
    assert executable is not None
    return subprocess.check_output(  # noqa: S603
        [executable, "-C", str(root), *args], text=True
    ).strip()


def test_diff_includes_deleted_and_both_renamed_paths(tmp_path: Path) -> None:
    git(tmp_path, "init")
    git(tmp_path, "config", "user.name", "CI test")
    git(tmp_path, "config", "user.email", "ci@example.invalid")
    (tmp_path / "executable.py").write_text("print(1)\n")
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-m", "base")
    base = git(tmp_path, "rev-parse", "HEAD")
    (tmp_path / "executable.py").rename(tmp_path / "README.md")
    git(tmp_path, "add", "-A")
    git(tmp_path, "commit", "-m", "rename")

    paths = ci_selection.changed_paths(tmp_path, base, "HEAD", merge_base=False)

    classification = ci_selection.classify(paths)

    assert set(paths) == {"executable.py", "README.md"}
    assert all(classification.values())


@pytest.mark.parametrize(
    ("event", "ref", "delegated", "expected"),
    [
        ("pull_request", "refs/pull/1/merge", "true", "false"),
        ("push", "refs/heads/main", "true", "true"),
        ("push", "refs/heads/feature", "true", "false"),
        ("push", "refs/heads/main", "false", "false"),
        ("workflow_dispatch", "refs/heads/main", "true", "false"),
    ],
)
def test_plan_cli_outputs_are_complete_and_cgroup_is_trusted_push_only(
    tmp_path: Path,
    event: str,
    ref: str,
    delegated: str,
    expected: str,
) -> None:
    payload = tmp_path / "event.json"
    payload.write_text("{}", encoding="utf-8")
    output = tmp_path / "output.txt"
    environment = os.environ | {
        "GITHUB_EVENT_NAME": event,
        "GITHUB_REF": ref,
        "GITHUB_EVENT_PATH": str(payload),
        "GITHUB_OUTPUT": str(output),
        "CI_CGROUP": delegated,
    }

    result = subprocess.run(
        [sys.executable, "-m", "tools.ci_selection", "plan"],
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=60,
    )

    values = dict(line.split("=", 1) for line in output.read_text().splitlines())

    assert result.returncode == 0
    assert set(values) == {*ci_selection.CATEGORIES, "cgroup"}
    assert all(values[key] == "true" for key in ci_selection.CATEGORIES)
    assert values["cgroup"] == expected
