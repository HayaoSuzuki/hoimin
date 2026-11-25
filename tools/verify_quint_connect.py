"""Run live Quint Connect checks with the pinned evaluator and bounded generation."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONNECT = ROOT / "formal/quint/connect"
BUILD = ROOT / "formal/quint/_build/connect"


def guarded(
    arguments: list[str], *, name: str, seconds: int, env: dict[str, str] | None = None
) -> int:
    """Bound the entire descendant tree, including Cargo's test processes."""
    BUILD.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(  # noqa: S603 -- repository-owned guard and fixed tool argv
        [
            os.environ.get("HOIMIN_QUINT_PYTHON", sys.executable),
            str(ROOT / "formal/HoiminOracle/tools/lean_resource_guard.py"),
            "--timeout-seconds",
            str(seconds),
            "--rss-limit-mib",
            "2048",
            "--sample-ms",
            "50",
            "--stats",
            str(BUILD / f"{name}-{uuid.uuid4().hex}.stats.json"),
            "--",
            *arguments,
        ],
        env=env,
        check=False,
        timeout=seconds + 5,
    )
    return result.returncode


def quint_cli(arguments: list[str]) -> int:
    """Guard every simulation launched by the live library."""
    if not arguments or arguments[0] != "run":
        msg = "The Quint Connect bridge supports only quint run."
        raise ValueError(msg)
    return guarded(
        [
            str(ROOT / "formal/quint/node_modules/.bin/quint"),
            *arguments,
            "--backend=typescript",
            "--invariant=Safety",
        ],
        name="quint",
        seconds=20,
    )


def verify() -> int:
    """Keep live generator dependencies out of ordinary workspace tests."""
    cargo = shutil.which("cargo")
    if cargo is None:
        msg = "Cargo is required for Quint Connect verification"
        raise FileNotFoundError(msg)
    env = {
        **os.environ,
        "PATH": os.pathsep.join((str(CONNECT / "bin"), os.environ.get("PATH", ""))),
        "HOIMIN_QUINT_PYTHON": sys.executable,
        "CARGO_TARGET_DIR": str(ROOT / "target/quint-connect"),
        "CARGO_BUILD_JOBS": "2",
    }
    return guarded(
        [
            cargo,
            "test",
            "--locked",
            "--manifest-path",
            str(CONNECT / "Cargo.toml"),
            "--all-features",
            "--",
            "--test-threads=1",
        ],
        name="cargo",
        seconds=300,
        env=env,
    )


def main() -> int:
    return quint_cli(sys.argv[1:]) if len(sys.argv) > 1 else verify()


if __name__ == "__main__":
    raise SystemExit(main())
