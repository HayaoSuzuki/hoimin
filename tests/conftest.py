from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture(scope="session", autouse=True)
def build_wheel_for_smoke_tests() -> None:
    if os.environ.get("HOIMIN_WHEEL"):
        return
    completed = subprocess.run(
        ["uv", "run", "maturin", "build", "--release", "--no-default-features"],  # noqa: S607
        cwd=REPOSITORY_ROOT,
        check=False,
    )
    assert completed.returncode == 0
