from __future__ import annotations

import json
import subprocess
import sys
from collections.abc import Mapping
from functools import partial
from pathlib import Path
from unittest.mock import create_autospec

import pytest
from pytest_mock import MockerFixture

from tools import ci_metrics


def test_metrics_separate_queue_execution_skips_and_attempts() -> None:
    run = {
        "id": 123,
        "run_attempt": 2,
        "event": "pull_request",
        "head_sha": "abc",
        "created_at": "2026-10-10T01:00:00Z",
        "updated_at": "2026-10-10T01:02:00Z",
    }
    jobs = [
        {
            "name": "Rust",
            "conclusion": "success",
            "created_at": "2026-10-10T01:00:20Z",
            "started_at": "2026-10-10T01:00:30Z",
            "completed_at": "2026-10-10T01:01:30Z",
        },
        {
            "name": "Lean",
            "conclusion": "skipped",
            "created_at": "2026-10-10T01:00:00Z",
            "started_at": "2026-10-10T01:00:00Z",
            "completed_at": "2026-10-10T01:00:00Z",
        },
    ]

    result = ci_metrics.summarize(run, jobs)

    assert result["run_attempt"] == 2
    assert result["elapsed_seconds"] == 120
    assert result["executed_jobs"] == 1
    assert result["job_seconds"] == 60
    assert result["queue_seconds"] == 10
    assert result["jobs"][0]["name"] == "Rust"
    assert result["jobs"][1]["name"] == "Lean"
    assert result["jobs"][0]["start_delay_seconds"] == 30


def test_incomplete_measurement_is_not_reported_as_zero() -> None:
    result = ci_metrics.job_metrics(
        {"name": "Waiting", "created_at": "2026-10-10T01:00:00Z"},
        "2026-10-10T01:00:00Z",
    )

    assert result["queue_seconds"] is None
    assert result["duration_seconds"] is None
    assert result["conclusion"] is None


@pytest.mark.parametrize(
    ("measurements", "expected"),
    [([1.0, None], None), ([None, 1.0], None), ([None], None), ([0.0], 0.0), ([], 0)],
    ids=["missing-last", "missing-first", "only-missing", "measured-zero", "empty"],
)
def test_total_preserves_missing_measurements(
    measurements: list[float | None], expected: float | None
) -> None:
    result = ci_metrics.total(measurements)

    assert result == expected


def test_api_pagination_uses_json_pages(mocker: MockerFixture) -> None:

    original_run_spec = ci_metrics.subprocess.run
    calls = []

    def run(args: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        calls.append((args, kwargs))
        return subprocess.CompletedProcess(args, 0, stdout='[{"jobs": []}]')

    mocker.patch.object(
        ci_metrics.subprocess,
        "run",
        create_autospec(original_run_spec, side_effect=run),
    )
    assert ci_metrics.api("/bin/gh", "repos/example/actions", pages=True) == [
        {"jobs": []}
    ]
    assert calls[0][0] == [
        "/bin/gh",
        "api",
        "repos/example/actions",
        "--paginate",
        "--slurp",
    ]
    assert calls[0][1]["check"] is True


def recorded_metrics_api(
    run: Mapping[str, object],
    calls: list[tuple[str, str, bool]],
    gh: str,
    endpoint: str,
    *,
    pages: bool = False,
) -> object:
    calls.append((gh, endpoint, pages))
    return [{"jobs": []}, {"jobs": []}] if pages else run


@pytest.fixture(params=["new", "existing"])
def metrics_output(tmp_path: Path, request: pytest.FixtureRequest) -> Path:
    output = tmp_path / "reports" / "nested" / "report.json"
    if request.param == "existing":
        output.parent.mkdir(parents=True)
        output.write_text('{"workflow": "stale"}', encoding="utf-8")
    return output


def test_main_writes_attempt_specific_metrics(
    metrics_output: Path, mocker: MockerFixture
) -> None:

    original_which_spec = ci_metrics.shutil.which
    original_api_spec = ci_metrics.api
    output = metrics_output
    mocker.patch.object(
        sys,
        "argv",
        ["metrics", "--run", "123", "--repo", "owner/repo", "--output", str(output)],
    )
    mocker.patch.object(
        ci_metrics.shutil,
        "which",
        create_autospec(original_which_spec, side_effect=lambda _: "/bin/gh"),
    )
    run = {
        "id": 123,
        "run_attempt": 2,
        "event": "push",
        "head_sha": "abc",
        "created_at": "2026-10-10T01:00:00Z",
        "name": "日本語 CI",
        "html_url": "https://example.test/run",
    }
    calls: list[tuple[str, str, bool]] = []
    mocker.patch.object(
        ci_metrics,
        "api",
        create_autospec(
            original_api_spec,
            side_effect=partial(recorded_metrics_api, run, calls),
        ),
    )

    ci_metrics.main()
    report = json.loads(output.read_text(encoding="utf-8"))

    assert report["repository"] == "owner/repo"
    assert report["workflow"] == "日本語 CI"
    assert report["url"] == run["html_url"]
    assert report["metrics"]["executed_jobs"] == 0
    assert report["measured_at"].endswith("+00:00")
    assert calls[-1] == (
        "/bin/gh",
        "repos/owner/repo/actions/runs/123/attempts/2/jobs?per_page=100",
        True,
    )


def test_main_requires_github_cli(tmp_path: Path, mocker: MockerFixture) -> None:

    original_which_spec = ci_metrics.shutil.which
    mocker.patch.object(
        sys, "argv", ["metrics", "--run", "1", "--output", str(tmp_path / "report")]
    )
    mocker.patch.object(
        ci_metrics.shutil,
        "which",
        create_autospec(original_which_spec, side_effect=lambda _: None),
    )
    with pytest.raises(FileNotFoundError, match="GitHub CLI is required"):
        ci_metrics.main()


@pytest.mark.parametrize(
    "pages", [{}, [{"jobs": {}}]], ids=["not-list", "jobs-not-list"]
)
def test_main_rejects_invalid_job_page_shapes(
    tmp_path: Path, mocker: MockerFixture, pages: object
) -> None:

    original_which_spec = ci_metrics.shutil.which
    original_api_spec = ci_metrics.api
    mocker.patch.object(
        sys, "argv", ["metrics", "--run", "1", "--output", str(tmp_path / "report")]
    )
    mocker.patch.object(
        ci_metrics.shutil,
        "which",
        create_autospec(original_which_spec, side_effect=lambda _: "/bin/gh"),
    )
    mocker.patch.object(
        ci_metrics,
        "api",
        create_autospec(
            original_api_spec,
            side_effect=lambda _gh, _endpoint, *, pages=False, payload=pages: (
                payload if pages else {"run_attempt": 1}
            ),
        ),
    )

    with pytest.raises(TypeError):
        ci_metrics.main()


@pytest.mark.parametrize(
    "module", ["tools.ci_metrics", "tools.docs_site", "tools.pypi_provenance"]
)
def test_tool_module_help_entrypoints(module: str) -> None:
    result = subprocess.run(  # noqa: S603 -- fixed module allowlist
        [sys.executable, "-m", module, "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0
    assert "usage:" in result.stdout


def test_api_without_pagination(mocker: MockerFixture) -> None:
    original_run_spec = ci_metrics.subprocess.run
    calls = []

    def run(args: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
        calls.append(args)
        return subprocess.CompletedProcess(args, 0, stdout='{"id": 1}')

    mocker.patch.object(
        ci_metrics.subprocess,
        "run",
        create_autospec(original_run_spec, side_effect=run),
    )
    assert ci_metrics.api("/bin/gh", "run") == {"id": 1}
    assert calls == [["/bin/gh", "api", "run"]]
