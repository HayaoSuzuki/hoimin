"""Failure boundaries for the mandatory production Creusot proof gate."""

from __future__ import annotations

import ast
import contextlib
import json
import os
import runpy
import shutil
import signal
import subprocess
import sys
import threading
import time
from contextlib import AbstractContextManager, chdir
from pathlib import Path
from types import SimpleNamespace
from typing import Any, BinaryIO
from unittest.mock import create_autospec

import pytest
from pytest_mock import MockerFixture

from tools import verify_creusot

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "tests/fixtures/creusot" / "x86_64-unknown-linux-gnu/first"


def wait_for_process_exit(state_file: Path, *, attempts: int = 500) -> str | None:
    """Allow asynchronous SIGKILL delivery before checking the final state."""
    state = None
    for _ in range(attempts):
        try:
            state = state_file.read_text().split(") ", 1)[1][0]
        except FileNotFoundError:
            return None
        if state == "Z":
            return state
        time.sleep(0.01)
    return state


def test_process_exit_wait_accepts_absent_process(tmp_path: Path) -> None:
    assert wait_for_process_exit(tmp_path / "missing-stat") is None


def test_process_exit_wait_accepts_zombie(tmp_path: Path) -> None:
    state_file = tmp_path / "stat"
    state_file.write_text("123 (child) Z remaining fields")
    assert wait_for_process_exit(state_file) == "Z"


def test_process_exit_wait_polls_until_delivery(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    state_file = tmp_path / "stat"
    state_file.write_text("123 (child) D remaining fields")
    sleep = create_autospec(
        time.sleep,
        side_effect=lambda _: state_file.write_text("123 (child) Z remaining fields"),
    )
    mocker.patch.object(time, "sleep", sleep)

    assert wait_for_process_exit(state_file, attempts=2) == "Z"
    sleep.assert_called_once_with(0.01)


def test_process_exit_wait_reports_state_after_bounded_attempts(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    state_file = tmp_path / "stat"
    state_file.write_text("123 (child) D remaining fields")
    sleep = create_autospec(time.sleep, return_value=None)
    mocker.patch.object(time, "sleep", sleep)

    assert wait_for_process_exit(state_file, attempts=2) == "D"
    assert sleep.call_args_list == [((0.01,),), ((0.01,),)]


class InterruptingWaitLock:
    """Inject SIGTERM at acquisition and expose a stranded lock without hanging."""

    def __init__(self, lock: threading.Lock) -> None:
        self.lock = lock
        self.fired = False

    def acquire(self, blocking: bool = True, timeout: float = -1) -> bool:  # noqa: FBT001, FBT002 -- Match Lock.acquire.
        if (
            self.fired and self.lock.locked()
        ):  # pragma: no cover -- Fails only when the wait-lock regression recurs.
            message = "SIGTERM escaped while the subprocess wait lock was held"
            raise AssertionError(message)
        acquired = self.lock.acquire(blocking, timeout)
        if acquired and not self.fired:
            self.fired = True
            os.kill(os.getpid(), signal.SIGTERM)
        return acquired

    def release(self) -> None:
        self.lock.release()

    def __enter__(self) -> None:
        self.acquire()

    def __exit__(self, *_args: object) -> None:
        self.release()


def test_runner_source_parses_for_pinned_container_python() -> None:
    source = (ROOT / "tools/verify_creusot.py").read_text(encoding="utf-8")

    ast.parse(source, filename="tools/verify_creusot.py", feature_version=(3, 12))


@pytest.fixture
def proof_project(tmp_path: Path) -> tuple[Path, dict[str, Any]]:
    """Use the real generated proof fixture and current production source."""
    manifest: dict[str, Any] = json.loads(
        (ROOT / "formal/creusot/obligations.json").read_text()
    )
    for source in {item["source"] for item in manifest["functions"]} | {
        item["source"] for item in manifest["logical_definitions"]
    }:
        destination = tmp_path / source
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / source, destination)
    shutil.copytree(EVIDENCE / "verif", tmp_path / "verif")
    return tmp_path, manifest


@pytest.mark.parametrize(
    ("function_id", "old", "new"),
    [
        (
            "projection",
            "ensures(result.jobs@ == jobs@)",
            "ensures(result.jobs@ >= jobs@)",
        ),
        ("reconstruction", "requires(nanos@ <= duration_max_nanos())", ""),
        ("projection", "pub fn project_top_budget(", "pub fn changed_top_budget("),
    ],
)
def test_source_scope_rejects_missing_or_weakened_contract(
    proof_project: tuple[Path, dict[str, Any]],
    function_id: str,
    old: str,
    new: str,
) -> None:
    root, manifest = proof_project
    function = next(item for item in manifest["functions"] if item["id"] == function_id)
    source = root / function["source"]
    original = source.read_text()

    assert old in original
    source.write_text(original.replace(old, new, 1))

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize(
    "addition",
    [
        "#[cfg_attr(creusot, requires(selected@ < 100))]\n",
        "#[cfg_attr(creusot, ensures(result.waves@ >= 0))]\n",
        "#[cfg_attr(all(creusot), requires(selected@ < 100))]\n",
        "#[cfg_attr(creusot, creusot_std::requires(selected@ < 100))]\n",
        (
            "#[cfg_attr(creusot, requires(selected@ < 100), "
            "ensures(result.waves@ >= 0))]\n"
        ),
        "#[cfg_attr(creusot, cfg_attr(creusot, requires(selected@ < 100)))]\n",
    ],
)
def test_source_scope_rejects_extra_contract(
    proof_project: tuple[Path, dict[str, Any]], addition: str
) -> None:
    root, manifest = proof_project
    source = root / "crates/hoimin-budget-math/src/projection.rs"
    source.write_text(
        source.read_text().replace(
            "pub fn project_top_budget(", addition + "pub fn project_top_budget(", 1
        )
    )

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize(
    "comment", ["// an ordinary comment\n", "/* block comment */\n"]
)
def test_source_scope_rejects_attribute_above_comment(
    proof_project: tuple[Path, dict[str, Any]], comment: str
) -> None:
    root, manifest = proof_project
    source = root / "crates/hoimin-budget-math/src/projection.rs"
    original = source.read_text()
    source.write_text(
        original.replace(
            "#[must_use]",
            "#[cfg_attr(all(creusot), requires(selected@ < 100))]\n"
            + comment
            + "#[must_use]",
            1,
        )
    )

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


def test_source_scope_rejects_inline_attribute_before_function(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    source = root / "crates/hoimin-budget-math/src/projection.rs"
    source.write_text(
        source.read_text().replace(
            "pub fn project_top_budget(",
            "#[cfg_attr(all(creusot), requires(selected@ < 100))] "
            "pub fn project_top_budget(",
            1,
        )
    )

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


def test_source_scope_accepts_current_production_contracts(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project

    verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize("damage", ["function", "contract", "logical", "target"])
def test_manifest_cannot_drop_required_proof_scope(
    proof_project: tuple[Path, dict[str, Any]], damage: str
) -> None:
    root, manifest = proof_project
    damage_manifest_scope(root, manifest, damage)

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


def test_manifest_and_source_cannot_weaken_contract_together(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    projection = next(
        item for item in manifest["functions"] if item["id"] == "projection"
    )
    contract = next(
        item for item in projection["contracts"] if item["id"] == "jobs_preserved"
    )
    source = root / projection["source"]
    old = contract["source"]
    new = old.replace("==", ">=")
    source.write_text(source.read_text().replace(old, new, 1))
    contract["source"] = new

    with pytest.raises(verify_creusot.ProofError, match="approved scope"):
        verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize("damage", ["missing", "empty", "old", "marker"])
def test_generated_scope_rejects_incomplete_or_stale_coma(
    proof_project: tuple[Path, dict[str, Any]], damage: str
) -> None:
    root, manifest = proof_project
    path = root / "verif/hoimin_budget_math_rlib/projection/project_top_budget.coma"
    damage_generated_scope(path, damage)

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_generated_scope(root, manifest, generated_after_ns=2)


def test_generated_scope_accepts_current_named_files(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project

    verify_creusot.check_generated_scope(root, manifest, generated_after_ns=0)


def test_saved_strategies_attach_only_to_current_generated_files(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    target = "x86_64-unknown-linux-gnu"
    strategy_root = Path(manifest["strategy_roots"][target])
    shutil.copytree(ROOT / strategy_root, root / strategy_root)

    count = verify_creusot.install_strategies(root, manifest, target)

    assert count == 25
    assert len(list((root / "verif").rglob("proof.json"))) == 25


@pytest.mark.parametrize(
    "damage", ["missing_goal", "unproved", "missing_target", "empty"]
)
def test_proof_status_rejects_incomplete_targets(
    proof_project: tuple[Path, dict[str, Any]], damage: str
) -> None:
    root, manifest = proof_project
    log = (EVIDENCE / "proof.log").read_text(encoding="utf-8")
    log = damaged_proof_log(log, damage)

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_proof_status(root, manifest, log)


def test_successful_alternative_may_recover_solver_timeout(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    log = "Z3 attempt timed out; CVC5 alternative succeeded\n" + (
        EVIDENCE / "proof.log"
    ).read_text(encoding="utf-8")

    result = verify_creusot.check_proof_status(root, manifest, log)

    assert result["files"] == 25
    assert result["goals"] == 59


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX process groups required")
def test_command_enforces_overall_deadline(tmp_path: Path) -> None:
    deadline = time.monotonic() + 0.05

    with pytest.raises(verify_creusot.ProofError, match="deadline"):
        verify_creusot.run_command(
            ["python3", "-c", "import time; time.sleep(5)"],
            cwd=tmp_path,
            log_path=tmp_path / "sleep.log",
            deadline=deadline,
        )


@pytest.mark.parametrize("failure", ["exit", "missing"])
def test_command_rejects_failed_or_missing_tool(tmp_path: Path, failure: str) -> None:
    old_handler = signal.getsignal(signal.SIGTERM)
    command = (
        ["python3", "-c", "raise SystemExit(3)"]
        if failure == "exit"
        else [str(tmp_path / "missing-tool")]
    )

    with pytest.raises((verify_creusot.ProofError, FileNotFoundError)):
        verify_creusot.run_command(
            command,
            cwd=tmp_path,
            log_path=tmp_path / "failed.log",
            deadline=time.monotonic() + 10,
        )

    assert signal.getsignal(signal.SIGTERM) == old_handler


def test_mismatched_environment_preflight_stops_before_generation(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    checker = tmp_path / "checker.py"
    checker.write_text("raise SystemExit('tool hash mismatch')\n")
    manifest = tmp_path / "toolchain.json"
    manifest.write_text("{}")
    mocker.patch.object(verify_creusot, "ENVIRONMENT", checker)
    mocker.patch.object(verify_creusot, "TOOLCHAIN", manifest)

    with pytest.raises(verify_creusot.ProofError, match="command failed"):
        verify_creusot.proof_preflight(
            tmp_path, tmp_path, time.monotonic() + 10, "mismatch"
        )

    assert "tool hash mismatch" in (tmp_path / "mismatch-preflight.log").read_text()


@pytest.mark.skipif(
    sys.platform != "linux", reason="Linux session ownership is required"
)
def test_deadline_kills_child_in_separate_process_group(tmp_path: Path) -> None:
    pid_file = tmp_path / "child.pid"
    script = (
        "import os, pathlib, time\n"
        "pid = os.fork()\n"
        "if pid == 0:\n"
        "    os.setpgid(0, 0)\n"
        f"    pathlib.Path({str(pid_file)!r}).write_text(str(os.getpid()))\n"
        "    time.sleep(20)\n"
        "else:\n"
        f"    path = pathlib.Path({str(pid_file)!r})\n"
        "    while not path.exists(): time.sleep(0.001)\n"
        "    time.sleep(20)\n"
    )

    with pytest.raises(verify_creusot.ProofError, match="deadline"):
        verify_creusot.run_command(
            [sys.executable, "-c", script],
            cwd=tmp_path,
            log_path=tmp_path / "separate-group.log",
            deadline=time.monotonic() + 0.5,
        )

    child = int(pid_file.read_text())
    state_file = Path(f"/proc/{child}/stat")

    state = wait_for_process_exit(state_file)
    assert state in (None, "Z"), f"child {child} remained in process state {state}"


@pytest.mark.skipif(
    sys.platform != "linux", reason="Linux session ownership is required"
)
def test_sigterm_kills_child_in_separate_process_group(tmp_path: Path) -> None:
    def cleanup_child(pid: int | None) -> None:
        if (
            pid is not None
        ):  # pragma: no branch -- None occurs only if child startup assertions fail.
            with contextlib.suppress(ProcessLookupError):
                getattr(os, "killpg")(pid, getattr(signal, "SIGKILL"))  # noqa: B009

    pid_file = tmp_path / "child.pid"
    child = (
        "import os, pathlib, sys, time\n"
        "os.setpgid(0, 0)\n"
        "pathlib.Path(sys.argv[1]).write_text(str(os.getpid()))\n"
        "time.sleep(20)\n"
    )
    wrapper = (
        "import pathlib, sys, time\n"
        "from tools.verify_creusot import ProofError, run_command\n"
        f"child = {child!r}\n"
        "root = pathlib.Path(sys.argv[2])\n"
        "try:\n"
        "    run_command([sys.executable, '-c', child, sys.argv[1]], "
        "cwd=root, log_path=root / 'term.log', deadline=time.monotonic() + 20)\n"
        "except ProofError:\n"
        "    raise SystemExit(17)\n"
    )
    process = subprocess.Popen(  # noqa: S603 -- fixed test Python code, no shell
        [sys.executable, "-c", wrapper, str(pid_file), str(tmp_path)], cwd=ROOT
    )
    child_pid = None
    try:
        for _ in range(
            500
        ):  # pragma: no branch -- Exhaustion fails the child-start assertion.
            if pid_file.exists():
                child_pid = int(pid_file.read_text())
                break
            assert process.poll() is None, "runner exited before starting child"
            time.sleep(0.01)
        assert child_pid is not None, "child did not start"

        process.send_signal(signal.SIGTERM)
        exit_code = process.wait(timeout=10)

        assert exit_code == 17
        state_file = Path(f"/proc/{child_pid}/stat")
        assert not state_file.exists() or (
            state_file.read_text().split(") ", 1)[1][0] == "Z"
        )
    finally:
        if (
            process.poll() is None
        ):  # pragma: no cover -- Emergency cleanup after a failing test.
            process.kill()
            process.wait(timeout=10)
        cleanup_child(child_pid)


@pytest.mark.skipif(
    sys.platform != "linux", reason="Linux proof-child signal delivery required"
)
def test_command_child_receives_sigterm(tmp_path: Path) -> None:
    def cleanup_child(pid: int | None) -> None:
        if (
            pid is not None
        ):  # pragma: no branch -- None occurs only if child startup assertions fail.
            with contextlib.suppress(ProcessLookupError):
                getattr(os, "killpg")(pid, getattr(signal, "SIGKILL"))  # noqa: B009

    pid_file = tmp_path / "child.pid"
    received = tmp_path / "received"
    child = (
        "import os, pathlib, signal, sys, time\n"
        "os.setpgid(0, 0)\n"
        "def on_term(_signum, _frame):\n"
        "    pathlib.Path(sys.argv[2]).write_text('received')\n"
        "    raise SystemExit(0)\n"
        "signal.signal(signal.SIGTERM, on_term)\n"
        "pathlib.Path(sys.argv[1]).write_text(str(os.getpid()))\n"
        "time.sleep(20)\n"
    )
    wrapper = (
        "import pathlib, sys, time\n"
        "from tools.verify_creusot import ProofError, run_command\n"
        f"child = {child!r}\n"
        "root = pathlib.Path(sys.argv[3])\n"
        "try:\n"
        "    run_command([sys.executable, '-c', child, sys.argv[1], sys.argv[2]], "
        "cwd=root, log_path=root / 'child-signal.log', "
        "deadline=time.monotonic() + 3)\n"
        "except ProofError:\n"
        "    raise SystemExit(17)\n"
    )
    process = subprocess.Popen(  # noqa: S603 -- fixed test Python code, no shell
        [sys.executable, "-c", wrapper, str(pid_file), str(received), str(tmp_path)],
        cwd=ROOT,
    )
    child_pid = None
    try:
        for _ in range(
            500
        ):  # pragma: no branch -- Exhaustion fails the child-start assertion.
            if pid_file.exists():
                child_pid = int(pid_file.read_text())
                break
            assert process.poll() is None, "runner exited before starting child"
            time.sleep(0.01)
        assert child_pid is not None, "child did not start"

        os.kill(child_pid, signal.SIGTERM)
        exit_code = process.wait(timeout=10)

        assert exit_code == 0
        assert received.read_text() == "received"
    finally:
        if (
            process.poll() is None
        ):  # pragma: no cover -- Emergency cleanup after a failing test.
            process.kill()
            process.wait(timeout=10)
        cleanup_child(child_pid)


@pytest.mark.skipif(sys.platform != "linux", reason="POSIX subprocess wait lock")
def test_sigterm_during_wait_lock_acquisition_does_not_deadlock(
    tmp_path: Path,
    mocker: MockerFixture,
) -> None:
    original_popen_spec = verify_creusot.subprocess.Popen
    original_popen = subprocess.Popen
    spawned: list[tuple[subprocess.Popen[bytes], threading.Lock]] = []

    def start(  # noqa: PLR0913 -- Forward the real Popen call with typed arguments.
        command: list[str],
        *,
        cwd: Path,
        stdout: BinaryIO,
        stderr: int,
        start_new_session: bool,
        env: dict[str, str],
    ) -> subprocess.Popen[bytes]:
        process = original_popen(
            command,
            cwd=cwd,
            stdout=stdout,
            stderr=stderr,
            start_new_session=start_new_session,
            env=env,
        )
        # The CPython wait implementation takes this lock before its try/finally.
        lock = getattr(process, "_waitpid_lock")  # noqa: B009 -- POSIX implementation detail
        assert isinstance(lock, type(threading.Lock()))
        spawned.append((process, lock))
        mocker.patch.object(process, "_waitpid_lock", InterruptingWaitLock(lock))
        # Python 3.15 waits with pidfd/kqueue before taking the legacy lock.
        # Exercise the lock path this regression test specifically protects.
        mocker.patch.object(
            process,
            "_wait_pidfd",
            create_autospec(lambda _: False, side_effect=lambda _: False),
            create=True,
        )
        mocker.patch.object(
            process,
            "_wait_kqueue",
            create_autospec(lambda _: False, side_effect=lambda _: False),
            create=True,
        )
        return process

    mocker.patch.object(
        verify_creusot.subprocess,
        "Popen",
        create_autospec(original_popen_spec, side_effect=start),
    )
    try:
        with pytest.raises(verify_creusot.ProofError, match="SIGTERM"):
            verify_creusot.run_command(
                [sys.executable, "-c", "import time; time.sleep(20)"],
                cwd=tmp_path,
                log_path=tmp_path / "wait-lock.log",
                deadline=time.monotonic() + 5,
            )
        assert spawned
        assert all(process.returncode is not None for process, _ in spawned)
    finally:
        for process, lock in spawned:
            if lock.locked():  # pragma: no cover -- Only for a stranded test lock.
                lock.release()
            mocker.patch.object(process, "_waitpid_lock", lock)
            if (
                process.poll() is None
            ):  # pragma: no cover -- Emergency cleanup after a failing test.
                verify_creusot.terminate_session(process.pid)
                process.wait(timeout=5)


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX SIGTERM is required")
@pytest.mark.parametrize(
    ("when", "expected_cleaned"),
    [("during_launch", [12345]), ("after_reap", [])],
    ids=["during-launch", "after-reap"],
)
def test_sigterm_cleanup_respects_command_lifecycle(
    tmp_path: Path,
    mocker: MockerFixture,
    when: str,
    expected_cleaned: list[int],
) -> None:
    original_popen_spec = verify_creusot.subprocess.Popen
    original_terminate_session_spec = verify_creusot.terminate_session
    cleaned: list[int] = []

    class Command:
        pid = 12345
        returncode: int | None = None

        def wait(self, timeout: float | None = None) -> int:
            del timeout
            if when == "after_reap":
                self.returncode = 0
                os.kill(os.getpid(), signal.SIGTERM)
            return 0

    def start(*_args: object, **_kwargs: object) -> Command:
        if when == "during_launch":
            os.kill(os.getpid(), signal.SIGTERM)
        return Command()

    mocker.patch.object(
        verify_creusot.subprocess,
        "Popen",
        create_autospec(original_popen_spec, side_effect=start),
    )
    mocker.patch.object(
        verify_creusot,
        "terminate_session",
        create_autospec(original_terminate_session_spec, side_effect=cleaned.append),
    )

    with pytest.raises(verify_creusot.ProofError, match="SIGTERM"):
        verify_creusot.run_command(
            [sys.executable, "-c", "pass"],
            cwd=tmp_path,
            log_path=tmp_path / "lifecycle.log",
            deadline=time.monotonic() + 10,
        )

    assert cleaned == expected_cleaned


@pytest.mark.skipif(sys.platform == "win32", reason="Linux session ownership required")
def test_session_cleanup_stops_command_group_before_solver_groups(
    mocker: MockerFixture,
) -> None:
    original_getsid_spec = getattr(verify_creusot.os, "getsid")  # noqa: B009 -- Platform API may be unavailable.
    original_owned_session_groups_spec = verify_creusot.owned_session_groups
    original_killpg_spec = getattr(verify_creusot.os, "killpg")  # noqa: B009 -- Platform API may be unavailable.
    calls: list[int] = []
    mocker.patch.object(verify_creusot.sys, "platform", "linux")
    mocker.patch.object(
        verify_creusot.os,
        "getsid",
        create_autospec(original_getsid_spec, side_effect=lambda _: 999),
    )
    mocker.patch.object(
        verify_creusot,
        "owned_session_groups",
        create_autospec(
            original_owned_session_groups_spec, side_effect=lambda _: {100, 101}
        ),
    )
    mocker.patch.object(
        verify_creusot.os,
        "killpg",
        create_autospec(
            original_killpg_spec, side_effect=lambda group, _: calls.append(group)
        ),
    )

    verify_creusot.terminate_session(100)

    assert calls[0] == 100
    assert 101 in calls


@pytest.mark.parametrize(
    ("unresolved_target", "expectation", "expected_results"),
    [
        pytest.param(
            None,
            contextlib.nullcontext(),
            (list(verify_creusot.TARGETS), [{"files": 25, "goals": 59}] * 3),
            id="all-targets-proved",
        ),
        pytest.param(
            "x86_64-pc-windows-msvc",
            pytest.raises(verify_creusot.ProofError, match="unproved goal"),
            ([], []),
            id="windows-goal-unproved",
        ),
    ],
)
@pytest.mark.parametrize("existing_generated", [False, True])
def test_runner_requires_three_fresh_target_results(  # noqa: PLR0913 -- Proof outcome and initial-state matrix.
    proof_project: tuple[Path, dict[str, Any]],
    mocker: MockerFixture,
    unresolved_target: str | None,
    expectation: AbstractContextManager[object],
    expected_results: tuple[list[str], list[dict[str, int]]],
    *,
    existing_generated: bool,
) -> None:
    original_install_strategies_spec = verify_creusot.install_strategies
    original_run_command_spec = verify_creusot.run_command
    root, manifest = proof_project
    for name in (
        "Cargo.toml",
        "Cargo.lock",
        "why3find.json",
        "crates/hoimin-budget-math/Cargo.toml",
        "formal/creusot/obligations.json",
    ):
        destination = root / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, destination)
    environment = root / "environment.py"
    environment.write_text("# preflight fixture\n")
    toolchain = root / "toolchain.json"
    toolchain.write_text("{}")
    mocker.patch.object(verify_creusot, "ENVIRONMENT", environment)
    mocker.patch.object(verify_creusot, "TOOLCHAIN", toolchain)
    mocker.patch.object(
        verify_creusot,
        "install_strategies",
        create_autospec(original_install_strategies_spec, side_effect=lambda *_: 25),
    )
    calls: list[list[str]] = []

    def fixture_command(
        command: list[str], *, cwd: Path, log_path: Path, deadline: float
    ) -> dict[str, Any]:
        del deadline
        calls.append(command)
        target = log_path.parent.name
        if "--only=coma" in command:
            shutil.copytree(EVIDENCE / "verif", cwd / "verif", dirs_exist_ok=True)
            for path in (cwd / "verif").rglob("*.coma"):
                # Assign nanoseconds explicitly: filesystem "now" can lag the
                # clock used to mark generation in a container.
                modified_ns = time.time_ns()
                os.utime(path, ns=(modified_ns, modified_ns))
        elif "--only=prove" in command:
            text = (EVIDENCE.parents[1] / target / "first" / "proof.log").read_text(
                encoding="utf-8"
            )
            if target == unresolved_target:
                text = text.replace(
                    "  Goal vc_project_top_budget: ✔ (1)",
                    "  Goal vc_project_top_budget: ✘ (0)",
                    1,
                )
            log_path.write_text(text, encoding="utf-8")
        else:
            log_path.write_text("fixture subprocess completed\n")
        return {"command": command, "elapsed_seconds": 0.01, "log": str(log_path)}

    mocker.patch.object(
        verify_creusot,
        "run_command",
        create_autospec(original_run_command_spec, side_effect=fixture_command),
    )
    if not existing_generated:
        shutil.rmtree(root / verify_creusot.GENERATED)
    output = root / "report"
    output.mkdir()

    expected_targets, expected_proofs = expected_results
    results: list[dict[str, Any]] = []

    with expectation:
        results = verify_creusot.verify(root, output, manifest, time.monotonic() + 10)

    proves = [call for call in calls if "--only=prove" in call]
    generations = [call for call in calls if "--only=coma" in call]

    assert [result["target"] for result in results] == expected_targets
    assert [result["proved"] for result in results] == expected_proofs
    assert len(generations) == 3
    assert all(
        "--ignore-rust-version" in call[call.index("--") + 1 :] for call in generations
    )
    assert len(proves) == 3
    assert all(
        "--no-cache" in call and "-j" in call and call[call.index("-j") + 1] == "2"
        for call in proves
    )
    assert sum("check" in call for call in calls) == 6


def damage_manifest_scope(root: Path, manifest: dict[str, Any], damage: str) -> None:
    if damage == "function":
        manifest["functions"] = [
            item for item in manifest["functions"] if item["id"] != "projection"
        ]
    elif damage == "contract":
        projection = next(
            item for item in manifest["functions"] if item["id"] == "projection"
        )
        projection["contracts"] = [
            item
            for item in projection["contracts"]
            if item["id"] != "mathematical_ceiling"
        ]
        source = root / projection["source"]
        source.write_text(
            source.read_text().replace(
                "#[cfg_attr(creusot, ensures(result.waves@ == "
                "(selected@ + jobs@ - 1) / jobs@))]",
                "",
            )
        )
    elif damage == "logical":
        manifest["logical_definitions"] = []
    else:
        manifest["targets"] = manifest["targets"][:2]


def damage_generated_scope(path: Path, damage: str) -> None:
    if damage == "missing":
        path.unlink()
    elif damage == "empty":
        path.write_text("")
    elif damage == "old":
        os.utime(path, ns=(1, 1))
    else:
        path.write_text(
            path.read_text().replace(
                "project_top_budget ensures #5", "changed ensures #5"
            )
        )


def damaged_proof_log(log: str, damage: str) -> str:
    if damage == "missing_goal":
        log = log.replace("  Goal vc_project_top_budget: ✔ (1)\n", "", 1)
    elif damage == "unproved":
        log = log.replace(
            "  Goal vc_project_top_budget: ✔ (1)",
            "  Goal vc_project_top_budget: ✘ (0)",
            1,
        )
    elif damage == "missing_target":
        log = log.replace("Proved (25 files) ✔", "Proved (24 files) ✔")
    else:
        log = ""
    return log


def test_nested_trailing_block_comment_is_skipped() -> None:
    source = "/* outer /* inner */ outer */"
    assert verify_creusot.trailing_comment_start(source, len(source)) == 0


@pytest.mark.parametrize("success", [True, False])
def test_main_publishes_success_and_failure_reports(
    tmp_path: Path, mocker: MockerFixture, *, success: bool
) -> None:
    original_verify_spec = verify_creusot.verify
    output = tmp_path / "report"
    mocker.patch.object(sys, "argv", ["verify", "--output", str(output)])
    with chdir(ROOT):

        def verify(*_args: object) -> list[dict[str, str]]:
            if not success:
                message = "rejected proof"
                raise verify_creusot.ProofError(message)
            (output / "nested").mkdir()
            (output / "nested" / "evidence").write_text("proof")
            return [{"target": "verified"}]

        mocker.patch.object(
            verify_creusot,
            "verify",
            create_autospec(original_verify_spec, side_effect=verify),
        )
        assert verify_creusot.main() == int(not success)
        summary = json.loads((output / "summary.json").read_text())
        assert summary["status"] == ("passed" if success else "failed")
        assert summary["output_disk_bytes"] == (5 if success else 0)
        if not success:
            assert summary["error"] == "rejected proof"


def test_linux_session_inventory_ignores_unrelated_and_vanished_processes(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    original_iterdir_spec = Path.iterdir
    proc = tmp_path / "proc"
    proc.mkdir()
    for name, content in {
        "self": "ignored",
        "1": "1 (name with spaces) S 0 11 77",
        "2": "2 (zombie) Z 0 12 77",
        "3": "3 (other) S 0 13 88",
        "4": "malformed",
        "5": "5 (bad) S 0 x 77",
    }.items():
        directory = proc / name
        directory.mkdir()
        (directory / "stat").write_text(content)
    (proc / "6").mkdir()
    original = Path.iterdir
    mocker.patch.object(
        Path,
        "iterdir",
        create_autospec(
            original_iterdir_spec,
            side_effect=lambda path: original(proc if path == Path("/proc") else path),
        ),
    )
    assert verify_creusot.owned_session_groups(77) == {11}


def test_proof_report_rejects_goal_before_theory(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    with pytest.raises(verify_creusot.ProofError, match="goal without theory"):
        verify_creusot.check_proof_status(root, manifest, "  Goal orphan: ✔ (1)")


def test_source_scope_rejects_missing_structural_model(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    model = manifest["structural_models"][0]
    source = root / model["source"]
    source.write_text(
        source.read_text()
        .replace(f"enum {model['type']}", f"enum Changed{model['type']}")
        .replace(f"struct {model['type']}", f"struct Changed{model['type']}")
    )
    with pytest.raises(
        verify_creusot.ProofError, match="structural model type missing"
    ):
        verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize(
    "measurement", ["Maximum resident set size (kbytes): 123", "missing"]
)
def test_command_requires_linux_memory_measurement(
    tmp_path: Path, mocker: MockerFixture, measurement: str
) -> None:

    original_is_file_spec = Path.is_file
    original_popen_spec = subprocess.Popen
    mocker.patch.object(sys, "platform", "linux")
    mocker.patch.object(
        Path,
        "is_file",
        create_autospec(original_is_file_spec, side_effect=lambda _: True),
    )

    def spawn(*_args: object, **_kwargs: object) -> SimpleNamespace:
        (tmp_path / "command.time").write_text(measurement)
        return SimpleNamespace(returncode=0, wait=lambda **_kw: 0)

    mocker.patch.object(
        subprocess, "Popen", create_autospec(original_popen_spec, side_effect=spawn)
    )
    if measurement == "missing":
        with pytest.raises(
            verify_creusot.ProofError, match="missing per-command memory"
        ):
            verify_creusot.run_command(
                ["proof"],
                cwd=tmp_path,
                log_path=tmp_path / "command.log",
                deadline=time.monotonic() + 10,
            )
    else:
        result = verify_creusot.run_command(
            ["proof"],
            cwd=tmp_path,
            log_path=tmp_path / "command.log",
            deadline=time.monotonic() + 10,
        )
        assert result["peak_process_or_child_rss_kib"] == 123


def test_session_cleanup_stops_scanning_after_groups_exit(
    mocker: MockerFixture,
) -> None:
    original_getsid_spec = getattr(os, "getsid", lambda _: 999)
    original_kill_group_spec = verify_creusot.kill_group
    original_owned_session_groups_spec = verify_creusot.owned_session_groups
    calls = []
    mocker.patch.object(sys, "platform", "linux")
    mocker.patch.object(
        os,
        "getsid",
        create_autospec(original_getsid_spec, side_effect=lambda _: 999),
        create=True,
    )
    mocker.patch.object(
        verify_creusot,
        "kill_group",
        create_autospec(original_kill_group_spec, side_effect=calls.append),
    )
    mocker.patch.object(
        verify_creusot,
        "owned_session_groups",
        create_autospec(
            original_owned_session_groups_spec, side_effect=lambda _: set()
        ),
    )
    verify_creusot.terminate_session(100)
    assert calls == [100]


def test_cli_entry_publishes_failure_when_manifest_is_missing(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    with chdir(tmp_path):
        mocker.patch.object(
            sys, "argv", ["verify", "--output", str(tmp_path / "report")]
        )
        with pytest.raises(SystemExit, match="1"):
            runpy.run_path(str(ROOT / "tools/verify_creusot.py"), run_name="__main__")
        assert (
            json.loads((tmp_path / "report/summary.json").read_text())["status"]
            == "failed"
        )


def test_attached_attributes_handles_nested_arrays_and_rejects_missing_opening() -> (
    None
):
    assert verify_creusot.attached_attributes(
        "#[custom([1, 2])]\nfn proof() {}", "proof", None
    ) == ["#[custom([1, 2])]"]
    with pytest.raises(verify_creusot.ProofError, match="malformed attached attribute"):
        verify_creusot.attached_attributes("]\nfn proof() {}", "proof", None)


def test_repeated_sigterm_during_cleanup_preserves_deadline_error(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    original_popen_spec = subprocess.Popen
    original_wait_for_command_spec = verify_creusot.wait_for_command
    original_terminate_session_spec = verify_creusot.terminate_session
    events: list[str] = []
    process = SimpleNamespace(
        pid=123, returncode=None, wait=lambda: events.append("reaped")
    )

    def deadline(*_args: object, **_kwargs: object) -> None:
        command = "proof"
        raise subprocess.TimeoutExpired(command, 1)

    def cleanup(_pid: int) -> None:
        os.kill(os.getpid(), signal.SIGTERM)
        events.append("cleaned")

    mocker.patch.object(
        subprocess,
        "Popen",
        create_autospec(
            original_popen_spec, side_effect=lambda *_args, **_kwargs: process
        ),
    )
    mocker.patch.object(
        verify_creusot,
        "wait_for_command",
        create_autospec(original_wait_for_command_spec, side_effect=deadline),
    )
    mocker.patch.object(
        verify_creusot,
        "terminate_session",
        create_autospec(original_terminate_session_spec, side_effect=cleanup),
    )
    with pytest.raises(verify_creusot.ProofError, match="overall proof deadline"):
        verify_creusot.run_command(
            ["proof"],
            cwd=tmp_path,
            log_path=tmp_path / "proof.log",
            deadline=time.monotonic() + 1,
        )
    assert events == ["cleaned", "reaped"]


def test_successful_command_without_optional_memory_tool(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    original_is_file_spec = Path.is_file
    mocker.patch.object(
        Path,
        "is_file",
        create_autospec(original_is_file_spec, side_effect=lambda _: False),
    )
    result = verify_creusot.run_command(
        [sys.executable, "-c", "pass"],
        cwd=tmp_path,
        log_path=tmp_path / "proof.log",
        deadline=time.monotonic() + 5,
    )
    assert result["peak_process_or_child_rss_kib"] is None
