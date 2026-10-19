from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest
from hypothesis import given
from hypothesis import strategies as st

from tools import ci_selection


@given(
    path=st.sampled_from(
        [
            "tools/docs_site.py",
            "tests/test_docs_site.py",
            "mkdocs.yml",
            ".github/workflows/documentation.yml",
        ]
    )
)
def test_documentation_inputs_use_the_dedicated_build(path: str) -> None:
    flags = ci_selection.classify([path])

    assert flags == {
        "python": False,
        "rust": False,
        "formal": False,
        "creusot": False,
        "distribution": False,
    }


@pytest.mark.parametrize(
    ("path", "expected"),
    [
        ("README.md", {"distribution"}),
        ("CONTRIBUTING.md", set()),
        ("docs/usage.md", set()),
        ("docs/cli-reference.md", {"rust"}),
        ("docs/audits/note.md", set()),
        ("crates/hoimin-cli/src/main.rs", {"rust", "formal", "distribution"}),
        ("vendor/ruff_python_parser/src/lib.rs", {"rust", "formal", "distribution"}),
        ("tests/test_example.py", {"python", "rust", "formal"}),
        ("formal/HoiminOracle/Main.lean", {"formal", "rust"}),
        ("tools/release.py", {"python", "distribution"}),
        ("tests/wheel_smoke.py", set(ci_selection.CATEGORIES)),
        ("LICENSE", {"distribution"}),
        ("docs/json-schema/report.json", set(ci_selection.CATEGORIES)),
        ("docs/audits/repro.py", set(ci_selection.CATEGORIES)),
        ("tests/fixtures/case.json", set(ci_selection.CATEGORIES)),
        (".github/workflows/ci.yml", set(ci_selection.CATEGORIES)),
        ("Cargo.lock", set(ci_selection.CATEGORIES)),
        ("rust-toolchain.toml", set(ci_selection.CATEGORIES)),
        (
            "crates/hoimin-budget-math/src/projection.rs",
            {"rust", "formal", "creusot", "distribution"},
        ),
        ("formal/creusot/obligations.json", {"rust", "formal", "creusot"}),
        (
            "formal/creusot/proofs/x86_64-unknown-linux-gnu/proof.json",
            {"rust", "formal", "creusot"},
        ),
        ("tools/creusot_environment.py", set(ci_selection.CATEGORIES)),
        ("tools/verify_creusot.py", set(ci_selection.CATEGORIES)),
        ("why3find.json", set(ci_selection.CATEGORIES)),
        ("uv.lock", set(ci_selection.CATEGORIES)),
        ("unknown-file", set(ci_selection.CATEGORIES)),
    ],
)
def test_classification_keeps_executable_inputs_out_of_documentation(
    path: str, expected: set[str]
) -> None:
    flags = ci_selection.classify([path])

    assert {key for key, value in flags.items() if value} == expected


def test_mixed_changes_union_their_requirements() -> None:
    paths = ["LICENSE", "tests/test_example.py"]

    flags = ci_selection.classify(paths)

    assert {key for key, selected in flags.items() if selected} == {
        "python",
        "rust",
        "formal",
        "distribution",
    }


def test_empty_diff_requires_no_checks() -> None:
    flags = ci_selection.classify([])

    assert not any(flags.values())


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


def test_missing_git_revision_falls_back_to_all_checks(tmp_path: Path) -> None:
    flags = ci_selection.plan(
        tmp_path,
        "pull_request",
        {"pull_request": {"base": {"sha": "missing"}, "head": {"sha": "missing"}}},
    )

    assert all(flags.values())


@pytest.mark.parametrize(
    "event", ["workflow_dispatch", "pull_request_target", "unknown"]
)
def test_events_without_a_trustworthy_diff_select_everything(event: str) -> None:
    flags = ci_selection.plan(Path.cwd(), event, {})

    assert all(flags.values())


def needs_for(flags: dict[str, bool]) -> dict[str, dict[str, object]]:
    needs: dict[str, dict[str, object]] = {
        "changes": {
            "result": "success",
            "outputs": {key: str(value).lower() for key, value in flags.items()}
            | {"cgroup": "false"},
        }
    }
    needs.update(
        {
            job: {"result": "success" if required else "skipped"}
            for job, required in ci_selection.selected_jobs(flags, cgroup=False).items()
        }
    )
    return needs


def test_aggregate_accepts_only_intentional_skips() -> None:
    needs = needs_for(ci_selection.classify(["docs/usage.md"]))

    errors = ci_selection.aggregate(needs)

    assert errors == []


@pytest.mark.parametrize("result", ["failure", "cancelled", "skipped", "neutral", ""])
@pytest.mark.parametrize(
    "job", ["changes", "quality", "workflow-lint", "rust", "creusot"]
)
def test_required_job_never_becomes_success(result: str, job: str) -> None:
    needs = needs_for(dict.fromkeys(ci_selection.CATEGORIES, True))
    needs[job]["result"] = result

    errors = ci_selection.aggregate(needs)

    assert errors


@pytest.mark.parametrize("value", [None, "", "True", "garbage"])
def test_missing_or_invalid_planner_output_fails_closed(value: object) -> None:
    needs = needs_for(ci_selection.classify(["README.md"]))
    needs["changes"]["outputs"] = {"rust": value}

    errors = ci_selection.aggregate(needs)

    assert errors


def test_missing_required_job_is_rejected() -> None:
    needs = needs_for(dict.fromkeys(ci_selection.CATEGORIES, True))
    del needs["rust"]

    errors = ci_selection.aggregate(needs)

    assert errors


def test_unexpected_failure_is_rejected() -> None:
    needs = needs_for(ci_selection.classify(["README.md"]))
    needs["rust"]["result"] = "failure"

    errors = ci_selection.aggregate(needs)

    assert errors


def test_aggregate_cli_propagates_failure() -> None:
    needs = needs_for(dict.fromkeys(ci_selection.CATEGORIES, True))
    needs["rust"]["result"] = "cancelled"

    result = subprocess.run(
        [sys.executable, "-m", "tools.ci_selection", "aggregate"],
        input=json.dumps(needs),
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 1
    assert "rust" in result.stdout


@pytest.mark.parametrize(
    ("event", "expected"),
    [("pull_request", False), ("merge_group", True), ("push", True)],
    ids=["pr-excludes-base-only-changes", "merge-group", "push"],
)
def test_event_diff_selects_only_its_changes(
    tmp_path: Path,
    event: str,
    *,
    expected: bool,
) -> None:
    git(tmp_path, "init")
    git(tmp_path, "config", "user.name", "CI test")
    git(tmp_path, "config", "user.email", "ci@example.invalid")
    (tmp_path / "README.md").write_text("base\n", encoding="utf-8")
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-m", "base")
    common = git(tmp_path, "rev-parse", "HEAD")
    (tmp_path / "docs").mkdir()
    document = tmp_path / "docs" / "design note.md"
    document.write_text("feature\n", encoding="utf-8")
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-m", "documentation")
    head = git(tmp_path, "rev-parse", "HEAD")
    git(tmp_path, "switch", "-c", "base-only", common)
    (tmp_path / "Cargo.lock").write_text("base-only\n", encoding="utf-8")
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-m", "base-only change")
    base = git(tmp_path, "rev-parse", "HEAD")
    payloads: dict[str, dict[str, object]] = {
        "pull_request": {
            "pull_request": {"base": {"sha": base}, "head": {"sha": head}}
        },
        "merge_group": {"merge_group": {"base_sha": common, "head_sha": base}},
        "push": {"before": common, "after": base},
    }

    flags = ci_selection.plan(tmp_path, event, payloads[event])

    assert all(value == expected for value in flags.values())


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


@pytest.mark.parametrize("payload", ["{", "null", "[]"])
def test_aggregate_cli_rejects_malformed_json(payload: str) -> None:
    result = subprocess.run(
        [sys.executable, "-m", "tools.ci_selection", "aggregate"],
        input=payload,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
        timeout=60,
    )

    assert result.returncode == 1
