import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import create_autospec

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
    os.name != "posix" or shutil.which("ps") is None,
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
    assert_process_exited(child_pid)


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
    assert_process_exited(descendant_pid)


@pytest.mark.parametrize("command", [[], ["--"]])
def test_argument_parser_requires_command(
    mocker: MockerFixture, command: list[str]
) -> None:
    mocker.patch.object(
        sys,
        "argv",
        [
            "guard",
            "--timeout-seconds",
            "1",
            "--rss-limit-mib",
            "1",
            "--sample-ms",
            "1",
            "--stats",
            "stats.json",
            *command,
        ],
    )
    with pytest.raises(SystemExit, match="2"):
        guard.parse_arguments()


@pytest.mark.parametrize(
    "option", ["--timeout-seconds", "--rss-limit-mib", "--sample-ms"]
)
def test_argument_parser_rejects_nonpositive_limits(
    mocker: MockerFixture, option: str
) -> None:
    options = {"--timeout-seconds": "1", "--rss-limit-mib": "1", "--sample-ms": "1"}
    options[option] = "0"
    mocker.patch.object(
        sys,
        "argv",
        [
            "guard",
            *[value for pair in options.items() for value in pair],
            "--stats",
            "stats.json",
            "python",
        ],
    )
    with pytest.raises(SystemExit, match="2"):
        guard.parse_arguments()


def test_process_table_ignores_incomplete_rows(
    mocker: MockerFixture,
) -> None:
    original_run_spec = guard.subprocess.run
    mocker.patch.object(
        guard.subprocess,
        "run",
        create_autospec(
            original_run_spec,
            side_effect=lambda *_a, **_k: SimpleNamespace(
                returncode=0, stdout="1 0 1 10\nincomplete\n2 1 1 20\n"
            ),
        ),
    )

    result = guard.process_table()

    assert result == {1: (0, 1, 10), 2: (1, 1, 20)}


def test_process_table_rejects_ps_failure(mocker: MockerFixture) -> None:
    original_run_spec = guard.subprocess.run
    mocker.patch.object(
        guard.subprocess,
        "run",
        create_autospec(
            original_run_spec,
            side_effect=lambda *_a, **_k: SimpleNamespace(returncode=3),
        ),
    )

    with pytest.raises(RuntimeError, match="exit code 3"):
        guard.process_table()


def test_process_tree_handles_cycles_and_missing_root() -> None:
    assert (
        guard.process_tree_rss_kib(1, {1: (2, 1, 10), 2: (1, 1, 20), 3: (0, 3, 99)})
        == 30
    )
    assert guard.process_tree_rss_kib(99, {}) == 0


@pytest.mark.parametrize(
    ("error", "exists"),
    [(ProcessLookupError, False), (PermissionError, True), (None, True)],
)
def test_group_existence_distinguishes_missing_and_inaccessible(
    mocker: MockerFixture, error: type[Exception] | None, *, exists: bool
) -> None:
    original_killpg_spec = getattr(guard.os, "killpg", os.kill)

    def killpg(*_: object) -> None:
        if error:
            raise error

    mocker.patch.object(
        guard.os,
        "killpg",
        create_autospec(original_killpg_spec, side_effect=killpg),
        create=True,
    )
    assert guard.process_group_exists(1) is exists


@pytest.mark.parametrize("exits", [True, False])
def test_group_wait_reports_final_existence(
    mocker: MockerFixture, *, exits: bool
) -> None:
    original_monotonic_spec = guard.time.monotonic
    original_sleep_spec = guard.time.sleep
    original_process_group_exists_spec = guard.process_group_exists
    clock = iter([0, 0.5, 2])
    mocker.patch.object(
        guard.time,
        "monotonic",
        create_autospec(original_monotonic_spec, side_effect=lambda: next(clock)),
    )
    mocker.patch.object(
        guard.time,
        "sleep",
        create_autospec(original_sleep_spec, side_effect=lambda _: None),
    )
    mocker.patch.object(
        guard,
        "process_group_exists",
        create_autospec(
            original_process_group_exists_spec, side_effect=lambda _: not exits
        ),
    )
    process = mocker.Mock(spec=subprocess.Popen, pid=1, poll=lambda: None)
    assert guard.wait_for_process_group_exit(process, 1) is exits


@pytest.fixture(params=["unsupported", "spawn-failed"])
def stats_failure_runtime(
    request: pytest.FixtureRequest, mocker: MockerFixture
) -> None:
    original_popen_spec = guard.subprocess.Popen
    if request.param == "unsupported":
        mocker.patch.object(guard, "os", SimpleNamespace(name="unsupported"))
    else:
        mocker.patch.object(
            guard.subprocess,
            "Popen",
            create_autospec(
                original_popen_spec,
                side_effect=lambda *_a, **_k: (_ for _ in ()).throw(
                    OSError("spawn failed")
                ),
            ),
        )


@pytest.mark.usefixtures("stats_failure_runtime")
def test_stats_failure_reports_monitor_error(
    tmp_path: Path,
    mocker: MockerFixture,
    capsys: pytest.CaptureFixture[str],
) -> None:
    original_write_stats_spec = guard.write_stats

    def fail_write(*_: object) -> None:
        msg = "disk full"
        raise OSError(msg)

    mocker.patch.object(
        guard,
        "write_stats",
        create_autospec(original_write_stats_spec, side_effect=fail_write),
    )

    result = guard.run(guard.Arguments(stats=tmp_path / "stats"))

    assert result == 126
    assert "disk full" in capsys.readouterr().err


def test_monitor_failure_cleans_up_spawned_process(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    original_popen_spec = guard.subprocess.Popen
    original_process_table_spec = guard.process_table
    original_terminate_process_group_spec = guard.terminate_process_group
    process = SimpleNamespace(pid=1, poll=lambda: None)
    mocker.patch.object(
        guard.subprocess,
        "Popen",
        create_autospec(original_popen_spec, side_effect=lambda *_a, **_k: process),
    )
    mocker.patch.object(
        guard,
        "process_table",
        create_autospec(
            original_process_table_spec,
            side_effect=lambda: (_ for _ in ()).throw(RuntimeError("ps failed")),
        ),
    )
    cleaned = []
    mocker.patch.object(
        guard,
        "terminate_process_group",
        create_autospec(
            original_terminate_process_group_spec, side_effect=cleaned.append
        ),
    )
    assert (
        guard.run(guard.Arguments(timeout_seconds=10, stats=tmp_path / "stats")) == 126
    )
    assert cleaned == [process]
    assert json.loads((tmp_path / "stats").read_text())["reason"] == "monitor_error"


def test_argument_parser_accepts_direct_command(
    mocker: MockerFixture,
) -> None:
    mocker.patch.object(
        sys,
        "argv",
        [
            "guard",
            "--timeout-seconds",
            "1",
            "--rss-limit-mib",
            "1",
            "--sample-ms",
            "1",
            "--stats",
            "stats.json",
            "python",
        ],
    )
    assert guard.parse_arguments().command == ["python"]


@requires_posix_guard
@pytest.mark.parametrize("escalates", [True, False])
def test_termination_waits_and_escalates_only_when_needed(
    mocker: MockerFixture, *, escalates: bool
) -> None:
    original_killpg_spec = getattr(guard.os, "killpg")  # noqa: B009 -- Platform API may be unavailable.
    original_wait_for_process_group_exit_spec = guard.wait_for_process_group_exit
    signals = []
    mocker.patch.object(
        guard.os,
        "killpg",
        create_autospec(
            original_killpg_spec,
            side_effect=lambda pid, signum: signals.append((pid, signum)),
        ),
    )
    mocker.patch.object(
        guard,
        "wait_for_process_group_exit",
        create_autospec(
            original_wait_for_process_group_exit_spec,
            side_effect=lambda *_: not escalates,
        ),
    )
    waits = []
    process = mocker.Mock(
        spec=subprocess.Popen, pid=123, wait=lambda: waits.append(True)
    )
    guard.terminate_process_group(process)
    assert hasattr(signal, "SIGKILL")
    assert signals == (
        [(123, signal.SIGTERM), (123, signal.SIGKILL)]
        if escalates
        else [(123, signal.SIGTERM)]
    )
    assert waits == [True]


def assert_process_exited(pid: int) -> None:
    for _ in range(20):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.05)
    assert hasattr(signal, "SIGKILL")
    os.kill(pid, signal.SIGKILL)
    pytest.fail(f"guard left process {pid} alive")


def test_process_exit_assertion_retries_until_process_disappears(
    mocker: MockerFixture,
) -> None:
    probe = mocker.patch.object(os, "kill", side_effect=[None, ProcessLookupError])
    pause = mocker.patch.object(time, "sleep")

    assert_process_exited(123)

    assert probe.call_count == 2
    pause.assert_called_once_with(0.05)


def test_process_exit_assertion_cleans_up_a_leaked_process(
    mocker: MockerFixture,
) -> None:
    assert hasattr(signal, "SIGKILL")
    probe = mocker.patch.object(os, "kill")
    mocker.patch.object(time, "sleep")
    with pytest.raises(pytest.fail.Exception, match="guard left process 123 alive"):
        assert_process_exited(123)
    assert probe.call_count == 21
    probe.assert_called_with(123, signal.SIGKILL)
