from __future__ import annotations

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
    assert result["jobs"][0]["start_delay_seconds"] == 30


def test_incomplete_measurement_is_not_reported_as_zero() -> None:
    result = ci_metrics.job_metrics(
        {"name": "Waiting", "conclusion": None, "created_at": "2026-10-10T01:00:00Z"},
        "2026-10-10T01:00:00Z",
    )
    assert result["queue_seconds"] is None
    assert result["duration_seconds"] is None
