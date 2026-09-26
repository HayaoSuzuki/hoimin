import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from types import SimpleNamespace

import pytest
from pytest_mock import MockerFixture

from formal.HoiminOracle.tools import lean_resource_guard as guard

ROOT = Path(__file__).resolve().parents[1]
GUARD = ROOT / "formal" / "HoiminOracle" / "tools" / "lean_resource_guard.py"


def test_rejects_unsupported_process_groups_before_spawning(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    stats = tmp_path / "stats.json"
    marker = tmp_path / "spawned"
    arguments = guard.Arguments(
        timeout_seconds=1.0,
        rss_limit_mib=8,
        sample_ms=25,
        stats=stats,
        command=[sys.executable, "-c", f"open({str(marker)!r},'w').close()"],
    )

    # Keep the unsupported OS local to the guard; pathlib still needs the real OS.
    mocker.patch.object(guard, "os", SimpleNamespace(name="unsupported"))
    exit_code = guard.run(arguments)

    assert exit_code == 126
    assert json.loads(stats.read_text())["reason"] == "monitor_error"
    assert not marker.exists()


requires_posix_guard = pytest.mark.skipif(
    os.name != "posix" or not shutil.which("ps"),
    reason="Lean RSS guard requires POSIX process groups and ps",
)


def run_guard(
    tmp_path: Path, *command: str, timeout: float, rss_limit_mib: int
) -> tuple[subprocess.CompletedProcess[bytes], dict[str, object]]:
    stats = tmp_path / "stats.json"
    # Execute the repository guard with controlled Python test programs.
    completed = subprocess.run(  # noqa: S603
        [
            sys.executable,
            str(GUARD),
            "--timeout-seconds",
            str(timeout),
            "--rss-limit-mib",
            str(rss_limit_mib),
            "--sample-ms",
            "25",
            "--stats",
            str(stats),
            "--",
            *command,
        ],
        check=False,
        timeout=5,
    )
    document: object = json.loads(stats.read_text())
    assert isinstance(document, dict)
    assert all(isinstance(key, str) for key in document)
    return completed, {str(key): value for key, value in document.items()}


@requires_posix_guard
def test_terminates_process_group_at_rss_limit(tmp_path: Path) -> None:
    marker = tmp_path / "child.pid"
    allocating_child = (
        "import os,time; "
        f"open({str(marker)!r},'w').write(str(os.getpid())); "
        "time.sleep(.1); payload=bytearray(64*1024*1024); time.sleep(30)"
    )

    completed, stats = run_guard(
        tmp_path,
        sys.executable,
        "-c",
        allocating_child,
        timeout=3,
        rss_limit_mib=32,
    )

    assert completed.returncode == 125
    assert stats["reason"] == "rss_limit"
    child_pid = int(marker.read_text())
    for _ in range(20):
        try:
            os.kill(child_pid, 0)
        except ProcessLookupError:
            break
        time.sleep(0.05)
    else:
        pytest.fail(f"guard left child process {child_pid} alive")


@requires_posix_guard
def test_terminates_process_group_at_timeout(tmp_path: Path) -> None:
    completed, stats = run_guard(
        tmp_path,
        sys.executable,
        "-c",
        "import time; time.sleep(30)",
        timeout=0.2,
        rss_limit_mib=768,
    )

    assert completed.returncode == 124
    assert stats["reason"] == "timeout"


@requires_posix_guard
def test_propagates_normal_child_exit(tmp_path: Path) -> None:
    completed, stats = run_guard(
        tmp_path,
        sys.executable,
        "-c",
        "import sys; sys.exit(7)",
        timeout=3,
        rss_limit_mib=768,
    )

    assert completed.returncode == 7
    assert stats["reason"] == "child_exit"


@requires_posix_guard
def test_rejects_root_exit_that_leaves_a_descendant(tmp_path: Path) -> None:
    assert hasattr(signal, "SIGKILL"), "process groups require POSIX"
    marker = tmp_path / "descendant.pid"
    descendant = (
        "import os,signal,time; "
        "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
        f"open({str(marker)!r},'w').write(str(os.getpid())); time.sleep(30)"
    )
    spawning_root = (
        "import os,subprocess,sys,time\n"
        f"subprocess.Popen([sys.executable,'-c',{descendant!r}])\n"
        "deadline=time.monotonic()+2\n"
        f"marker={str(marker)!r}\n"
        "while not os.path.exists(marker) and time.monotonic()<deadline:\n"
        "    time.sleep(.01)\n"
    )

    completed, stats = run_guard(
        tmp_path,
        sys.executable,
        "-c",
        spawning_root,
        timeout=3,
        rss_limit_mib=768,
    )

    assert completed.returncode == 126
    assert stats["reason"] == "monitor_error"
    descendant_pid = int(marker.read_text())
    for _ in range(20):
        try:
            os.kill(descendant_pid, 0)
        except ProcessLookupError:
            break
        time.sleep(0.05)
    else:
        os.kill(descendant_pid, signal.SIGKILL)
        pytest.fail(f"guard left descendant process {descendant_pid} alive")
