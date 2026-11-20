"""Run all fuzz targets within a deadline that includes CI setup time."""

import argparse
import json
import os
import random
import shutil
import signal
import subprocess
import time
import tomllib
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = "nightly-2026-10-08"
STARTUP_MARGIN = 10
MAX_CACHED_INPUTS = 512


@contextmanager
def active_corpus(cache: Path, *, seed: int) -> Generator[Path]:
    """Replay a bounded cache sample and retain discoveries, including on failure.

    libFuzzer executes its entire initial corpus before checking max_total_time.
    Keep the growing cache out of that startup path; fixed and freshly generated
    inputs are supplied separately and are never sampled.
    """
    cache.mkdir(parents=True, exist_ok=True)
    entries = sorted(p for p in cache.iterdir() if p.is_file() and not p.is_symlink())
    selected = random.Random(seed).sample(  # noqa: S311 -- Reproducible fuzz selection.
        entries, min(len(entries), MAX_CACHED_INPUTS)
    )
    with TemporaryDirectory(prefix="hoimin-fuzz-corpus-") as directory:
        active = Path(directory)
        for source in selected:
            shutil.copyfile(source, active / source.name)
        try:
            yield active
        finally:
            for source in active.iterdir():
                if source.is_file() and not source.is_symlink():
                    shutil.copyfile(source, cache / source.name)


class FuzzError(Exception):
    """A failed stage or an exhausted budget; neither is successful fuzzing."""


def positive_seconds(value: str) -> int:
    seconds = int(value)
    if seconds < 1:
        msg = "seconds must be positive"
        raise argparse.ArgumentTypeError(msg)
    return seconds


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
        env: dict[str, str] | None = None,
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
        if env is not None:
            # Record explicit selectors only, never the inherited environment.
            stage["environment"] = env
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
                    env=None if env is None else {**os.environ, **env},
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


def run_campaign(campaign: Campaign, *, seconds_per_target: int | None = None) -> None:
    manifest = tomllib.loads((ROOT / "fuzz/Cargo.toml").read_text(encoding="utf-8"))
    targets = [target["name"] for target in manifest["bin"]]
    reserve = len(targets) * (STARTUP_MARGIN + 1)
    cargo = ["cargo", f"+{TOOLCHAIN}", "fuzz"]
    campaign.run("build", [*cargo, "build", "--codegen-units", "16"], reserve=reserve)
    generated = campaign.report / "generated-python"
    generated.mkdir(parents=True, exist_ok=True)
    # Interpreter-specific generation feeds one interpreter-independent Rust run.
    # Temporary environments never replace the development venv or enter artifacts.
    with TemporaryDirectory(prefix="hoimin-fuzz-python-") as environments:
        for version in ("3.14", "3.15"):
            suffix = version.replace(".", "")
            env = {"UV_PROJECT_ENVIRONMENT": str(Path(environments) / version)}
            python = [
                "uv",
                "run",
                "--frozen",
                "--no-sync",
                "--python",
                version,
                "python",
            ]
            campaign.run(
                f"dependencies-{suffix}",
                [
                    "uv",
                    "sync",
                    "--frozen",
                    "--group",
                    "fuzz",
                    "--no-install-project",
                    "--python",
                    version,
                ],
                reserve=reserve,
                env=env,
            )
            campaign.run(
                f"generator-tests-{suffix}",
                [*python, "-m", "pytest", "-q", "tests/test_hypothesmith_corpus.py"],
                reserve=reserve,
                env=env,
            )
            strategies = (
                ("grammar", "libcst", "python315")
                if version == "3.15"
                else ("grammar", "libcst")
            )
            for strategy in strategies:
                campaign.run(
                    f"generate-{strategy}-{suffix}",
                    [
                        *python,
                        "tools/hypothesmith_corpus.py",
                        "--output",
                        str(generated),
                        "--strategy",
                        strategy,
                        "--examples",
                        "50",
                        "--seed",
                        str(campaign.seed),
                    ],
                    reserve=reserve,
                    env=env,
                )
    for index, target in enumerate(targets):
        seconds = (
            seconds_per_target
            if seconds_per_target is not None
            else fuzz_seconds(campaign.remaining(), len(targets) - index)
        )
        cache = ROOT / "fuzz/corpus" / target
        with active_corpus(cache, seed=campaign.seed) as corpus:
            inputs = [str(corpus), f"fuzz/seeds/{target}"]
            if target == "python_analyzer":
                inputs.append(str(generated))
            campaign.run(
                target,
                [
                    *cargo,
                    "run",
                    "--codegen-units",
                    "16",
                    target,
                    *inputs,
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
    parser.add_argument("--seconds-per-target", type=positive_seconds)
    args = parser.parse_args()
    campaign = Campaign(args.deadline, args.report, seed=args.seed)
    try:
        run_campaign(campaign, seconds_per_target=args.seconds_per_target)
    except (FuzzError, OSError) as error:
        campaign.save("failure", str(error))
        print(str(error), flush=True)  # noqa: T201 -- CI failure diagnostic.
        return 1
    campaign.save("success")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
