"""Run all fuzz targets within a deadline that includes CI setup time."""

import argparse
import json
import os
import signal
import subprocess
import time
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = "nightly-2026-07-27"
STARTUP_MARGIN = 10


class FuzzError(Exception):
    """A failed stage or an exhausted budget; neither is successful fuzzing."""


def fuzz_seconds(remaining: float, targets: int) -> int:
    seconds = int(remaining / targets) - STARTUP_MARGIN
    if seconds < 1:
        msg = f"insufficient fuzz budget: {remaining:.1f}s for {targets} targets"
        raise FuzzError(msg)
    return seconds


class Campaign:
    def __init__(self, deadline: float, report: Path, *, seed: int) -> None:
        self.deadline = time.monotonic() + deadline - time.time()
        self.report = report
        self.seed = seed % 4294967295 or 1
        self.stages: list[dict[str, object]] = []
        self.completed_targets: list[str] = []
        report.mkdir(parents=True, exist_ok=True)
        self.save("running")

    def remaining(self) -> float:
        return self.deadline - time.monotonic()

    def save(self, status: str, error: str | None = None) -> None:
        (self.report / "summary.json").write_text(
            json.dumps(
                {
                    "status": status,
                    "error": error,
                    "seed": self.seed,
                    "remaining_seconds": self.remaining(),
                    "completed_targets": self.completed_targets,
                    "stages": self.stages,
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )

    def run(
        self,
        name: str,
        argv: list[str],
        *,
        timeout: float | None = None,
        reserve: float = 0,
    ) -> None:
        available = self.remaining() - reserve
        limit = available if timeout is None else min(timeout, available)
        if limit <= 0:
            msg = f"budget exhausted before {name}"
            raise FuzzError(msg)
        started = time.monotonic()
        stage: dict[str, object] = {
            "name": name,
            "argv": argv,
            "timeout_seconds": limit,
            "status": "error",
        }
        self.stages.append(stage)
        print(f"{name}: limit {limit:.1f}s", flush=True)  # noqa: T201 -- CI progress.
        try:
            # Fixed commands, no shell. The group includes compiler/fuzzer children.
            with (
                (self.report / f"{name}.log").open("wb") as log,
                subprocess.Popen(  # noqa: S603
                    argv,
                    cwd=ROOT,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                ) as process,
            ):
                try:
                    stage["returncode"] = process.wait(timeout=limit)
                except subprocess.TimeoutExpired as error:
                    assert hasattr(os, "killpg"), "fuzz process groups require POSIX"
                    assert hasattr(signal, "SIGKILL"), (
                        "fuzz process groups require POSIX"
                    )
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                    stage["status"] = "timeout"
                    msg = f"{name}: timeout after {limit:.1f}s"
                    raise FuzzError(msg) from error
                if process.returncode:
                    msg = f"{name}: exit {process.returncode}; see {name}.log"
                    raise FuzzError(msg)
                stage["status"] = "success"
        finally:
            stage["elapsed_seconds"] = time.monotonic() - started
            self.save("running")


def run_campaign(campaign: Campaign) -> None:
    manifest = tomllib.loads((ROOT / "fuzz/Cargo.toml").read_text(encoding="utf-8"))
    targets = [target["name"] for target in manifest["bin"]]
    reserve = len(targets) * (STARTUP_MARGIN + 1)
    cargo = ["cargo", f"+{TOOLCHAIN}", "fuzz"]
    python = ["uv", "run", "--frozen", "--no-sync", "python"]
    campaign.run(
        "dependencies",
        ["uv", "sync", "--frozen", "--group", "fuzz", "--no-install-project"],
        reserve=reserve,
    )
    campaign.run("build", [*cargo, "build", "--codegen-units", "16"], reserve=reserve)
    campaign.run(
        "generator-tests",
        [*python, "-m", "pytest", "-q", "tests/test_hypothesmith_corpus.py"],
        reserve=reserve,
    )
    for strategy in ("grammar", "libcst"):
        campaign.run(
            f"generate-{strategy}",
            [
                *python,
                "tools/hypothesmith_corpus.py",
                "--strategy",
                strategy,
                "--examples",
                "50",
                "--seed",
                str(campaign.seed),
            ],
            reserve=reserve,
        )
    for index, target in enumerate(targets):
        seconds = fuzz_seconds(campaign.remaining(), len(targets) - index)
        corpus = f"fuzz/corpus/{target}"
        (ROOT / corpus).mkdir(parents=True, exist_ok=True)
        campaign.run(
            target,
            [
                *cargo,
                "run",
                "--codegen-units",
                "16",
                target,
                corpus,
                f"fuzz/seeds/{target}",
                "--",
                f"-max_total_time={seconds}",
                f"-seed={campaign.seed}",
                "-max_len=4096",
                "-timeout=5",
                "-rss_limit_mb=1024",
            ],
            timeout=seconds + STARTUP_MARGIN,
        )
        campaign.completed_targets.append(target)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--deadline", type=int, required=True, help="Unix time in seconds"
    )
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--seed", type=int, required=True)
    args = parser.parse_args()
    campaign = Campaign(args.deadline, args.report, seed=args.seed)
    try:
        run_campaign(campaign)
    except (FuzzError, OSError) as error:
        campaign.save("failure", str(error))
        print(str(error), flush=True)  # noqa: T201 -- CI failure diagnostic.
        return 1
    campaign.save("success")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
