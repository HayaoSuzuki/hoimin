import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest
import yaml

from tools import ci_fuzz

ROOT = Path(__file__).resolve().parents[1]
FAILURE_CODE = 23
MAX_SEED = 4294967295
JOB_MINUTES = 9


@pytest.mark.parametrize(
    ("remaining", "targets", "expected"),
    [(300, 5, 50), (120, 3, 30), (11, 1, 1), (59.9, 5, 1)],
)
def test_shares_remaining_budget_with_startup_margin(
    remaining: float, targets: int, expected: int
) -> None:
    assert ci_fuzz.fuzz_seconds(remaining, targets) == expected


@pytest.mark.parametrize("remaining", [-1, 0, 10, 54.9])
def test_insufficient_budget_cannot_be_success(remaining: float) -> None:
    with pytest.raises(ci_fuzz.FuzzError, match="budget"):
        ci_fuzz.fuzz_seconds(remaining, 5)


def test_command_failure_keeps_diagnostics(tmp_path: Path) -> None:
    campaign = ci_fuzz.Campaign(time.time() + 10, tmp_path, seed=123)
    with pytest.raises(ci_fuzz.FuzzError, match="broken"):
        campaign.run(
            "broken",
            [sys.executable, "-c", "print('failure detail'); raise SystemExit(23)"],
        )
    report = json.loads((tmp_path / "summary.json").read_text())
    assert report["stages"][0]["returncode"] == FAILURE_CODE
    assert "failure detail" in (tmp_path / "broken.log").read_text()
    assert report["status"] != "success"


@pytest.mark.skipif(os.name != "posix", reason="CI process groups require POSIX")
def test_timeout_terminates_child_processes(tmp_path: Path) -> None:
    sentinel = tmp_path / "child-survived"
    child_code = (
        "import time; from pathlib import Path; time.sleep(1); "
        f"Path({str(sentinel)!r}).touch()"
    )
    parent_code = (
        "import subprocess, sys, time; "
        f"subprocess.Popen([sys.executable, '-c', {child_code!r}]); "
        "print('started', flush=True); time.sleep(30)"
    )
    campaign = ci_fuzz.Campaign(time.time() + 10, tmp_path, seed=123)
    with pytest.raises(ci_fuzz.FuzzError, match="timeout"):
        campaign.run("timeout", [sys.executable, "-c", parent_code], timeout=0.5)
    time.sleep(1)
    assert not sentinel.exists()
    assert "started" in (tmp_path / "timeout.log").read_text()
    report = json.loads((tmp_path / "summary.json").read_text())
    assert report["stages"][0]["status"] == "timeout"


def test_expired_deadline_never_starts_command(tmp_path: Path) -> None:
    sentinel = tmp_path / "started"
    campaign = ci_fuzz.Campaign(time.time() - 1, tmp_path, seed=123)
    with pytest.raises(ci_fuzz.FuzzError, match="budget"):
        campaign.run(
            "expired",
            [sys.executable, "-c", f"open({str(sentinel)!r}, 'w').close()"],
        )
    assert not sentinel.exists()


def test_budget_uses_monotonic_time_after_initial_setup(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(ci_fuzz.time, "time", lambda: 1000)
    monkeypatch.setattr(ci_fuzz.time, "monotonic", lambda: 100)
    campaign = ci_fuzz.Campaign(1080, tmp_path, seed=1)
    # Setup already consumed 400 of 480 seconds. A wall-clock correction must
    # not restore that time or extend the remaining 80 seconds.
    monkeypatch.setattr(ci_fuzz.time, "time", lambda: 500)
    monkeypatch.setattr(ci_fuzz.time, "monotonic", lambda: 130)
    expected_remaining = 50
    assert campaign.remaining() == expected_remaining


@pytest.mark.skipif(os.name != "posix", reason="Fuzz CI runs on Linux")
@pytest.mark.parametrize("fail_target", ["", "source_index"])
def test_campaign_reports_all_targets_or_stops_at_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, fail_target: str
) -> None:
    # Cargo and uv are the expensive/external boundary. Execute lightweight
    # stand-ins while retaining the real orchestration and process management.
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    for tool in ("cargo", "uv"):
        executable = fake_bin / tool
        executable.write_text(
            f"#!{sys.executable}\n"
            "import sys\n"
            "print(sys.argv[1:])\n"
            f"if {fail_target!r} and {fail_target!r} in sys.argv:\n"
            "    sys.exit(23)\n"
        )
        executable.chmod(0o755)
    monkeypatch.setenv("PATH", str(fake_bin))
    (tmp_path / "fuzz").mkdir()
    (tmp_path / "fuzz/Cargo.toml").write_bytes((ROOT / "fuzz/Cargo.toml").read_bytes())
    monkeypatch.setattr(ci_fuzz, "ROOT", tmp_path)
    report_dir = tmp_path / "report"
    campaign = ci_fuzz.Campaign(time.time() + 120, report_dir, seed=123)
    if fail_target:
        with pytest.raises(ci_fuzz.FuzzError, match="source_index"):
            ci_fuzz.run_campaign(campaign)
        assert campaign.completed_targets == ["source_encoding"]
        assert not (report_dir / "candidate_validation.log").exists()
    else:
        ci_fuzz.run_campaign(campaign)
        assert campaign.completed_targets == [
            "source_encoding",
            "source_index",
            "candidate_validation",
            "analyzer_protocol",
            "python_analyzer",
        ]
        report = json.loads((report_dir / "summary.json").read_text())
        stages = {stage["name"]: stage for stage in report["stages"]}
        assert {"generate-grammar", "generate-libcst"} <= stages.keys()
        for name in campaign.completed_targets:
            argv = stages[name]["argv"]
            assert argv.index(f"fuzz/corpus/{name}") < argv.index(f"fuzz/seeds/{name}")
            duration = next(arg for arg in argv if arg.startswith("-max_total_time="))
            assert int(duration.split("=")[1]) > 0


def test_cli_failure_writes_report(tmp_path: Path) -> None:
    result = subprocess.run(  # noqa: S603 -- Repository CLI, fixed arguments.
        [
            sys.executable,
            str(ROOT / "tools/ci_fuzz.py"),
            "--deadline",
            "1",
            "--report",
            str(tmp_path),
            "--seed",
            "36158455427",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )
    assert result.returncode == 1
    report = json.loads((tmp_path / "summary.json").read_text())
    assert report["status"] == "failure"
    assert "budget" in report["error"]
    assert 1 <= report["seed"] <= MAX_SEED
    assert report["completed_targets"] == []


def test_workflow_runs_fuzz_in_parallel_with_bounded_total_time() -> None:
    workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
    job = workflow["jobs"]["fuzz"]
    assert job["needs"] == "quality"
    assert job["runs-on"] == "ubuntu-latest"
    assert job["timeout-minutes"] == JOB_MINUTES
    assert workflow["permissions"] == {"contents": "read"}
    assert "FUZZ_DEADLINE" in job["steps"][0]["run"]
    uploads = [
        step
        for step in job["steps"]
        if step.get("uses", "").startswith("actions/upload-artifact@")
    ]
    assert len(uploads) == 1
    assert uploads[0]["if"] == "always()"
    assert "fuzz/artifacts" in uploads[0]["with"]["path"]
