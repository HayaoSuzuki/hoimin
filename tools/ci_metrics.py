"""Capture comparable Actions run/job times without treating missing data as zero."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from collections.abc import Mapping, Sequence
from datetime import UTC, datetime
from pathlib import Path
from typing import TypedDict

from tools.ci_selection import object_mapping


class JobMetrics(TypedDict):
    name: object
    conclusion: object
    queue_seconds: float | None
    duration_seconds: float | None
    start_delay_seconds: float | None


class RunMetrics(TypedDict):
    run_id: object
    run_attempt: object
    event: object
    head_sha: object
    elapsed_seconds: float | None
    executed_jobs: int
    job_seconds: float | None
    queue_seconds: float | None
    jobs: list[JobMetrics]


def seconds(start: object, end: object) -> float | None:
    if not isinstance(start, str) or not isinstance(end, str):
        return None
    return (datetime.fromisoformat(end) - datetime.fromisoformat(start)).total_seconds()


def job_metrics(job: Mapping[str, object], run_created: object) -> JobMetrics:
    return {
        "name": job.get("name"),
        "conclusion": job.get("conclusion"),
        "queue_seconds": seconds(job.get("created_at"), job.get("started_at")),
        "duration_seconds": seconds(job.get("started_at"), job.get("completed_at")),
        "start_delay_seconds": seconds(run_created, job.get("started_at")),
    }


def total(values: list[float | None]) -> float | None:
    if any(value is None for value in values):
        return None
    return sum(value for value in values if value is not None)


def summarize(
    run: Mapping[str, object], jobs: Sequence[Mapping[str, object]]
) -> RunMetrics:
    records = [job_metrics(job, run["created_at"]) for job in jobs]
    executed = [job for job in records if job["conclusion"] != "skipped"]
    return {
        "run_id": run["id"],
        "run_attempt": run["run_attempt"],
        "event": run["event"],
        "head_sha": run["head_sha"],
        "elapsed_seconds": seconds(
            run.get("run_started_at", run["created_at"]), run.get("updated_at")
        ),
        "executed_jobs": len(executed),
        "job_seconds": total([job["duration_seconds"] for job in executed]),
        "queue_seconds": total([job["queue_seconds"] for job in executed]),
        "jobs": records,
    }


def api(gh: str, endpoint: str, *, pages: bool = False) -> object:
    args = [gh, "api", endpoint]
    if pages:
        args.extend(["--paginate", "--slurp"])
    result = subprocess.run(  # noqa: S603 -- resolved executable and argv, no shell
        args, check=True, capture_output=True, text=True, encoding="utf-8", timeout=60
    )
    return json.loads(result.stdout)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="HayaoSuzuki/hoimin")
    parser.add_argument("--run", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    gh = shutil.which("gh")
    if gh is None:
        msg = "GitHub CLI is required"
        raise FileNotFoundError(msg)
    prefix = f"repos/{args.repo}/actions/runs/{args.run}"
    run = object_mapping(api(gh, prefix))
    pages = api(
        gh, f"{prefix}/attempts/{run['run_attempt']}/jobs?per_page=100", pages=True
    )
    if not isinstance(pages, list):
        raise TypeError
    jobs: list[dict[str, object]] = []
    for page in pages:
        items = object_mapping(page)["jobs"]
        if not isinstance(items, list):
            raise TypeError
        jobs.extend(object_mapping(item) for item in items)
    report = {
        "measured_at": datetime.now(UTC).isoformat(),
        "repository": args.repo,
        "workflow": run.get("name"),
        "url": run.get("html_url"),
        "metrics": summarize(run, jobs),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
