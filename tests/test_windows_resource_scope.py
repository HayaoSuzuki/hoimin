"""Exercise the Windows probe protocol with a controlled Job Object API."""

import ctypes
import importlib.util
import json
import runpy
import sys
from ctypes import wintypes
from pathlib import Path
from types import ModuleType, SimpleNamespace
from typing import cast

import pytest


@pytest.fixture
def probe(monkeypatch: pytest.MonkeyPatch) -> ModuleType:
    def query(
        _job: object,
        kind: int,
        result: ctypes.c_void_p,
        size: int,
        returned: ctypes.c_void_p,
    ) -> bool:
        ctypes.cast(returned, ctypes.POINTER(wintypes.DWORD)).contents.value = size
        if kind == 9:
            limits = ctypes.cast(
                result,
                ctypes.POINTER(cast("type[ctypes.Structure]", module.ExtendedLimits)),
            ).contents
            assert hasattr(limits, "BasicLimitInformation")
            assert hasattr(limits, "JobMemoryLimit")
            limits.BasicLimitInformation.LimitFlags = 42
            limits.JobMemoryLimit = 1024
        else:
            accounting = ctypes.cast(
                result,
                ctypes.POINTER(cast("type[ctypes.Structure]", module.Accounting)),
            ).contents
            assert hasattr(accounting, "ActiveProcesses")
            accounting.ActiveProcesses = 2
        return True

    monkeypatch.setattr(
        ctypes,
        "WinDLL",
        lambda *_a, **_k: SimpleNamespace(QueryInformationJobObject=query),
        raising=False,
    )
    monkeypatch.setattr(
        ctypes,
        "WinError",
        lambda code: OSError(code, "job query failed"),
        raising=False,
    )
    monkeypatch.setattr(ctypes, "get_last_error", lambda: 5, raising=False)
    path = (
        Path(__file__).resolve().parents[1]
        / "crates/hoimin-cli/tests/support/windows_resource_scope.py"
    )
    spec = importlib.util.spec_from_file_location("windows_probe", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_read_job_returns_typed_information(probe: ModuleType) -> None:
    assert probe.read_job(9, probe.ExtendedLimits).JobMemoryLimit == 1024
    assert probe.read_job(1, probe.Accounting).ActiveProcesses == 2


@pytest.mark.parametrize("failure", ["api", "size"])
def test_read_job_rejects_failed_or_truncated_information(
    probe: ModuleType, monkeypatch: pytest.MonkeyPatch, failure: str
) -> None:
    monkeypatch.setattr(probe, "query", lambda *_args: failure != "api")
    with pytest.raises(OSError if failure == "api" else AssertionError):
        probe.read_job(1, probe.Accounting)


def test_baseline_does_not_spawn_descendants(
    probe: ModuleType, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        probe, "import_module", lambda _: SimpleNamespace(first=11, second=22)
    )
    probe.main()


@pytest.mark.parametrize(
    ("mode", "first", "release"),
    [
        ("memory", 12, "immediate"),
        ("process", 11, "delayed"),
        ("process", 12, "child_exit"),
        ("process", 12, "timeout"),
    ],
)
def test_mutant_publishes_job_observation_and_waits_for_controller(  # noqa: PLR0913, PLR0917 -- Protocol scenarios vary independently.
    probe: ModuleType,
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
    mode: str,
    first: int,
    release: str,
) -> None:
    monkeypatch.setattr(
        probe, "import_module", lambda _: SimpleNamespace(first=first, second=23)
    )
    monkeypatch.setattr(sys, "argv", ["probe", mode, str(tmp_path)])
    child = SimpleNamespace(
        pid=123, poll=lambda: 0 if release == "child_exit" else None
    )
    monkeypatch.setattr(probe.subprocess, "Popen", lambda *_args: child)
    clock = iter([0, 31])
    monkeypatch.setattr(probe.time, "monotonic", lambda: next(clock))
    monkeypatch.setattr(probe.time, "sleep", lambda _: (tmp_path / "release").touch())
    if release == "immediate":
        (tmp_path / "release").touch()
    if release in {"child_exit", "timeout"}:
        with pytest.raises(AssertionError if release == "child_exit" else TimeoutError):
            probe.main()
    else:
        if release == "delayed":
            monkeypatch.setattr(probe.time, "monotonic", lambda: 0)
        probe.main()
    identity = "first" if first != 11 else "second"
    observation = json.loads((tmp_path / f"{identity}.json").read_text())
    assert observation["child_pid"] == 123
    assert observation["active_processes"] == 2
    assert observation["payload_bytes"] == (96 * 1024 * 1024 if mode == "memory" else 0)
    assert not (tmp_path / f"{identity}.tmp").exists()


def test_script_entry_runs_sequential_baseline(
    probe: ModuleType, monkeypatch: pytest.MonkeyPatch
) -> None:

    monkeypatch.setitem(sys.modules, "target", SimpleNamespace(first=11, second=22))
    assert probe.__file__ is not None
    runpy.run_path(probe.__file__, run_name="__main__")
