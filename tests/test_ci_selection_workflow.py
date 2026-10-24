from __future__ import annotations

import json
import tomllib
from pathlib import Path

import yaml
from packaging.requirements import Requirement

from tools.ci_selection import CATEGORIES, selected_jobs

ROOT = Path(__file__).resolve().parents[1]


def test_rust_job_checks_generated_cli_reference() -> None:
    for filename in ("ci.yml", "non-linux-ci.yml"):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows" / filename).read_text(encoding="utf-8")
        )
        steps = workflow["jobs"]["rust"]["steps"]

        assert any(
            step.get("run")
            == (
                "cargo run --locked -p hoimin-cli "
                "--example generate_cli_reference -- --check"
            )
            for step in steps
        ), filename


def test_non_linux_performance_gate_uses_bash_for_runner_temp() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/non-linux-ci.yml").read_text(encoding="utf-8")
    )
    steps = workflow["jobs"]["rust"]["steps"]
    gate = next(step for step in steps if step.get("name") == "Performance shape gates")

    assert gate["shell"] == "bash"
    assert '"$RUNNER_TEMP/performance-gates"' in gate["run"]


def test_planner_and_aggregate_explicitly_install_python_314() -> None:
    for filename, names in (
        ("ci.yml", ("changes", "result")),
        ("release.yml", ("changes",)),
    ):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows" / filename).read_text(encoding="utf-8")
        )
        for name in names:
            steps = workflow["jobs"][name]["steps"]
            setup = next(
                i
                for i, step in enumerate(steps)
                if step.get("uses", "").startswith("actions/setup-python@")
            )

            assert steps[setup]["with"]["python-version"] == "3.14"
            assert all(
                "python tools/ci_selection.py" not in step.get("run", "")
                for step in steps[:setup]
            )


def test_aggregate_waits_for_every_planned_job_even_when_upstream_fails() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    jobs = workflow["jobs"]
    gate = jobs["result"]

    assert gate["if"] == "always()"

    selected_jobs_result = selected_jobs(dict.fromkeys(CATEGORIES, True), cgroup=True)

    assert set(gate["needs"]) == {"changes"} | set(selected_jobs_result)
    assert gate["name"] == "CI result"
    assert "toJSON(needs)" in str(gate["steps"])


def test_creusot_job_has_bounded_native_setup_and_failure_diagnostics() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    job = workflow["jobs"]["creusot"]
    steps = job["steps"]

    assert job["runs-on"] == "ubuntu-latest"
    assert job["timeout-minutes"] == 30
    assert "changes" in job["needs"]
    assert "needs.changes.outputs.creusot == 'true'" in job["if"]
    assert any("docker build" in step.get("run", "") for step in steps)
    assert any("tools/verify_creusot.py" in step.get("run", "") for step in steps)
    assert any(
        step.get("if") == "failure()"
        and "actions/upload-artifact@" in step.get("uses", "")
        for step in steps
    )
    assert "creusot" in workflow["jobs"]["result"]["needs"]


def test_creusot_image_cache_keys_include_every_effective_recipe_input() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    steps = workflow["jobs"]["creusot"]["steps"]
    cache_keys = [
        step["with"]["key"]
        for step in steps
        if step.get("uses", "").startswith(
            ("actions/cache/restore@", "actions/cache/save@")
        )
    ]

    assert len(cache_keys) == 2
    assert cache_keys[0] == cache_keys[1]
    for input_path in (
        "formal/creusot/Dockerfile",
        "formal/creusot/toolchain.json",
        "formal/creusot/install*.sh",
        "formal/creusot/configure.sh",
        "formal/creusot/why3find.json",
        "formal/creusot/why3.conf",
        "formal/creusot/creusot_why3.conf",
        "tools/creusot_environment.py",
    ):
        assert f"'{input_path}'" in cache_keys[0]


def test_heavy_checks_and_rust_quality_are_selected_from_change_outputs() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    jobs = workflow["jobs"]
    for name in set(selected_jobs(dict.fromkeys(CATEGORIES, True), cgroup=True)) - {
        "quality",
        "workflow-lint",
    }:
        assert "changes" in jobs[name]["needs"]
        assert "needs.changes.outputs." in jobs[name]["if"]
    rust_steps = [
        step for step in jobs["quality"]["steps"] if "cargo " in step.get("run", "")
    ]

    assert len(rust_steps) == 4
    assert all(
        step["if"] == "needs.changes.outputs.rust == 'true'" for step in rust_steps
    )


def test_preview_cancellation_and_publication_are_separate() -> None:
    ci = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"))
    release = yaml.safe_load(
        (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    )
    for workflow in (ci, release):
        concurrency = workflow["concurrency"]

        assert "github.event.pull_request.number" in concurrency["group"]
        assert (
            concurrency["cancel-in-progress"]
            == "${{ github.event_name == 'pull_request' }}"
        )

    assert "github.run_id" in ci["concurrency"]["group"]
    assert (
        "github.event.pull_request.merge_commit_sha" in release["concurrency"]["group"]
    )
    assert release["jobs"]["changes"].get("permissions", {"contents": "read"}) == {
        "contents": "read"
    }
    assert release["jobs"]["prepare"].get("permissions", {"contents": "read"}) == {
        "contents": "read"
    }
    assert "needs.changes.outputs.distribution" in release["jobs"]["prepare"]["if"]


def test_fork_checks_use_read_permissions_without_secrets_or_oidc() -> None:
    ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    workflow = yaml.safe_load(ci)

    assert workflow["permissions"] == {"contents": "read"}
    assert "secrets." not in ci
    assert "id-token:" not in ci
    assert all("permissions" not in job for job in workflow["jobs"].values())
    release = yaml.safe_load(
        (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    )
    write_jobs = {
        name
        for name, job in release["jobs"].items()
        if job.get("permissions", {}).get("contents") == "write"
    }

    assert write_jobs == {"reserve", "publish"}
    assert release["jobs"]["reserve"]["if"] == (
        "github.event_name == 'pull_request_target' && "
        "github.event.pull_request.merged == true"
    )


def test_workflow_tools_are_managed_by_renovate() -> None:
    manifest = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    requirements = manifest["dependency-groups"]["workflow"]

    assert len(requirements) == 3
    names = {Requirement(requirement).name for requirement in requirements}
    renovate = json.loads((ROOT / "renovate.json").read_text(encoding="utf-8"))

    assert any(
        set(rule.get("matchPackageNames", [])) == names
        and rule["matchManagers"] == ["pep621"]
        for rule in renovate["packageRules"]
    )


def test_sbom_tool_path_is_scoped_to_capture_steps() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    )
    for name in ("windows-wheel", "linux-wheel", "macos-wheel"):
        steps = workflow["jobs"][name]["steps"]
        install = next(
            step
            for step in steps
            if step.get("name") == "Install pinned SBOM generator"
        )

        assert "GITHUB_PATH" not in install["run"]
        capture = next(
            step
            for step in steps
            if step.get("name") == "Generate and validate release SBOMs"
        )
        script = capture["run"]

        assert script.index("export PATH=") < script.index(
            "python tools/capture_sbom.py"
        )
    windows_steps = workflow["jobs"]["windows-wheel"]["steps"]
    windows_capture = next(
        step
        for step in windows_steps
        if step.get("name") == "Generate and validate release SBOMs"
    )
    assert 'cygpath -u "$RUNNER_TEMP"' in windows_capture["run"]


def test_rust_ci_tests_both_python_versions_without_artifact_collisions() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    )
    rust = workflow["jobs"]["rust"]
    assert rust["strategy"]["matrix"]["python"] == ["3.14", "3.15"]
    setup = next(
        step
        for step in rust["steps"]
        if step.get("uses", "").startswith("actions/setup-python@")
    )
    assert setup["with"]["python-version"] == "${{ matrix.python }}"
    upload = next(
        step
        for step in rust["steps"]
        if step.get("uses", "").startswith("actions/upload-artifact@")
    )
    assert "${{ matrix.python }}" in upload["with"]["name"]


def test_same_wheel_is_smoked_on_315_in_every_supported_platform_job() -> None:
    command = (
        "uv run --no-project --python 3.15 --with packaging==26.0 "
        "python tests/wheel_smoke.py"
    )
    for filename, names in (
        ("ci.yml", ("wheel-smoke",)),
        ("non-linux-ci.yml", ("wheel-smoke",)),
        ("release.yml", ("windows-wheel", "linux-wheel", "macos-wheel")),
    ):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows" / filename).read_text(encoding="utf-8")
        )
        for name in names:
            steps = workflow["jobs"][name]["steps"]
            old = next(
                i
                for i, step in enumerate(steps)
                if step.get("run")
                == "uv run --frozen --no-sync python tests/wheel_smoke.py"
            )
            new = next(i for i, step in enumerate(steps) if step.get("run") == command)
            assert new == old + 1, (filename, name)
            assert not steps[new].get("continue-on-error", False)
            if filename == "release.yml":
                assert steps[new]["env"] == {
                    "HOIMIN_SMOKE_REPORT": "wheel-smoke-315.json"
                }
                upload = next(
                    step
                    for step in steps[new + 1 :]
                    if step.get("uses", "").startswith("actions/upload-artifact@")
                )
                assert upload["with"]["path"] == "wheel-smoke*.json"


def test_renovate_preserves_both_supported_python_minor_lines() -> None:
    config = json.loads((ROOT / "renovate.json").read_text(encoding="utf-8"))
    rules = [
        rule
        for rule in config["packageRules"]
        if "actions/python-versions" in rule.get("matchPackageNames", [])
    ]
    assert any(rule.get("allowedVersions") == ">=3.14 <3.16" for rule in rules)
    assert any(
        set(rule.get("matchUpdateTypes", [])) == {"major", "minor"}
        and rule.get("enabled") is False
        for rule in rules
    )
    assert all(
        not ("allowedVersions" in rule and "matchUpdateTypes" in rule) for rule in rules
    )
