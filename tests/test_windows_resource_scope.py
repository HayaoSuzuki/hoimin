"""Exercise the Windows probe protocol with a controlled Job Object API."""

import ctypes
import importlib.util
import json
import runpy
import sys
from contextlib import AbstractContextManager, nullcontext
from ctypes import wintypes
from pathlib import Path
from types import ModuleType, SimpleNamespace
from typing import cast
from unittest.mock import create_autospec

import pytest
from pytest_mock import MockerFixture


@pytest.fixture
def probe(mocker: MockerFixture) -> ModuleType:
    original_windll_spec = getattr(ctypes, "WinDLL", ctypes.CDLL)
    original_winerror_spec = getattr(ctypes, "WinError", OSError)
    original_get_last_error_spec = getattr(ctypes, "get_last_error", ctypes.get_errno)

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

    mocker.patch.object(
        ctypes,
        "WinDLL",
        create_autospec(
            original_windll_spec,
            side_effect=lambda *_a, **_k: SimpleNamespace(
                QueryInformationJobObject=query
            ),
        ),
        create=True,
    )
    mocker.patch.object(
        ctypes,
        "WinError",
        create_autospec(
            original_winerror_spec,
            side_effect=lambda code: OSError(code, "job query failed"),
        ),
        create=True,
    )
    mocker.patch.object(
        ctypes,
        "get_last_error",
        create_autospec(original_get_last_error_spec, side_effect=lambda: 5),
        create=True,
    )
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
    probe: ModuleType, mocker: MockerFixture, failure: str
) -> None:
    original_query_spec = probe.query
    mocker.patch.object(
        probe,
        "query",
        create_autospec(
            original_query_spec, side_effect=lambda *_args: failure != "api"
        ),
    )
    with pytest.raises(OSError if failure == "api" else AssertionError):
        probe.read_job(1, probe.Accounting)


def test_baseline_does_not_spawn_descendants(
    probe: ModuleType, mocker: MockerFixture
) -> None:
    original_import_module_spec = probe.import_module
    mocker.patch.object(
        probe,
        "import_module",
        create_autospec(
            original_import_module_spec,
            side_effect=lambda _: SimpleNamespace(first=11, second=22),
        ),
    )
    probe.main()


@pytest.fixture
def mutant_protocol(
    request: pytest.FixtureRequest,
    probe: ModuleType,
    mocker: MockerFixture,
    tmp_path: Path,
) -> tuple[ModuleType, Path]:
    mode, first, release = cast("tuple[str, int, str]", request.param)
    original_import_module_spec = probe.import_module
    original_popen_spec = probe.subprocess.Popen
    original_monotonic_spec = probe.time.monotonic
    original_sleep_spec = probe.time.sleep
    mocker.patch.object(
        probe,
        "import_module",
        create_autospec(
            original_import_module_spec,
            side_effect=lambda _: SimpleNamespace(first=first, second=23),
        ),
    )
    mocker.patch.object(sys, "argv", ["probe", mode, str(tmp_path)])
    child = SimpleNamespace(
        pid=123, poll=lambda: 0 if release == "child_exit" else None
    )
    mocker.patch.object(
        probe.subprocess,
        "Popen",
        create_autospec(original_popen_spec, side_effect=lambda *_args: child),
    )
    clock = iter([0, 31])
    mocker.patch.object(
        probe.time,
        "monotonic",
        create_autospec(original_monotonic_spec, side_effect=lambda: next(clock)),
    )
    mocker.patch.object(
        probe.time,
        "sleep",
        create_autospec(
            original_sleep_spec, side_effect=lambda _: (tmp_path / "release").touch()
        ),
    )
    if release == "immediate":
        (tmp_path / "release").touch()
    elif release == "delayed":
        mocker.patch.object(
            probe.time,
            "monotonic",
            create_autospec(original_monotonic_spec, side_effect=lambda: 0),
        )
    return probe, tmp_path


@pytest.mark.parametrize(
    ("mutant_protocol", "expectation", "identity", "payload_bytes"),
    [
        (("memory", 12, "immediate"), nullcontext(), "first", 96 * 1024 * 1024),
        (("process", 11, "delayed"), nullcontext(), "second", 0),
        (("process", 12, "child_exit"), pytest.raises(AssertionError), "first", 0),
        (("process", 12, "timeout"), pytest.raises(TimeoutError), "first", 0),
    ],
    indirect=["mutant_protocol"],
)
def test_mutant_publishes_job_observation_and_waits_for_controller(
    mutant_protocol: tuple[ModuleType, Path],
    expectation: AbstractContextManager[object],
    identity: str,
    payload_bytes: int,
) -> None:
    probe, tmp_path = mutant_protocol

    with expectation:
        probe.main()

    observation = json.loads((tmp_path / f"{identity}.json").read_text())
    assert observation["child_pid"] == 123
    assert observation["active_processes"] == 2
    assert observation["payload_bytes"] == payload_bytes
    assert not (tmp_path / f"{identity}.tmp").exists()


def test_script_entry_runs_sequential_baseline(
    probe: ModuleType, mocker: MockerFixture
) -> None:

    mocker.patch.dict(sys.modules, {"target": SimpleNamespace(first=11, second=22)})
    assert probe.__file__ is not None
    runpy.run_path(probe.__file__, run_name="__main__")
