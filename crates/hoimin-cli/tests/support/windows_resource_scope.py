"""Finite Windows Job Object probe run by the real CLI acceptance tests."""

import ctypes
import json
import os
import subprocess
import sys
import time
from ctypes import wintypes
from importlib import import_module
from pathlib import Path


class BasicLimits(ctypes.Structure):
    LimitFlags: int
    ActiveProcessLimit: int
    _fields_ = [
        ("PerProcessUserTimeLimit", ctypes.c_int64),
        ("PerJobUserTimeLimit", ctypes.c_int64),
        ("LimitFlags", wintypes.DWORD),
        ("MinimumWorkingSetSize", ctypes.c_size_t),
        ("MaximumWorkingSetSize", ctypes.c_size_t),
        ("ActiveProcessLimit", wintypes.DWORD),
        ("Affinity", ctypes.c_size_t),
        ("PriorityClass", wintypes.DWORD),
        ("SchedulingClass", wintypes.DWORD),
    ]


class IoCounters(ctypes.Structure):
    _fields_ = [
        (name, ctypes.c_uint64)
        for name in (
            "ReadOperationCount",
            "WriteOperationCount",
            "OtherOperationCount",
            "ReadTransferCount",
            "WriteTransferCount",
            "OtherTransferCount",
        )
    ]


class ExtendedLimits(ctypes.Structure):
    BasicLimitInformation: BasicLimits
    JobMemoryLimit: int
    PeakJobMemoryUsed: int
    _fields_ = [
        ("BasicLimitInformation", BasicLimits),
        ("IoInfo", IoCounters),
        ("ProcessMemoryLimit", ctypes.c_size_t),
        ("JobMemoryLimit", ctypes.c_size_t),
        ("PeakProcessMemoryUsed", ctypes.c_size_t),
        ("PeakJobMemoryUsed", ctypes.c_size_t),
    ]


class Accounting(ctypes.Structure):
    ActiveProcesses: int
    _fields_ = [
        (name, ctypes.c_int64)
        for name in (
            "TotalUserTime",
            "TotalKernelTime",
            "ThisPeriodTotalUserTime",
            "ThisPeriodTotalKernelTime",
        )
    ] + [
        (name, wintypes.DWORD)
        for name in (
            "TotalPageFaultCount",
            "TotalProcesses",
            "ActiveProcesses",
            "TotalTerminatedProcesses",
        )
    ]


assert hasattr(ctypes, "WinDLL"), "Job Object probe requires Windows"
kernel = ctypes.WinDLL("kernel32", use_last_error=True)
query = kernel.QueryInformationJobObject
query.argtypes = [
    wintypes.HANDLE,
    ctypes.c_int,
    ctypes.c_void_p,
    wintypes.DWORD,
    ctypes.POINTER(wintypes.DWORD),
]
query.restype = wintypes.BOOL


def read_job[T: ctypes.Structure](kind: int, structure: type[T]) -> T:
    assert hasattr(ctypes, "WinError"), "Job Object probe requires Windows"
    assert hasattr(ctypes, "get_last_error"), "Job Object probe requires Windows"
    result = structure()
    returned = wintypes.DWORD()
    # NULL selects the calling process's immediate job, including nested jobs.
    if not query(
        None, kind, ctypes.byref(result), ctypes.sizeof(result), ctypes.byref(returned)
    ):
        raise ctypes.WinError(ctypes.get_last_error())
    if returned.value != ctypes.sizeof(result):
        raise AssertionError((kind, returned.value, ctypes.sizeof(result)))
    return result


def publish(path: Path, value: dict[str, int]) -> None:
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value), encoding="utf-8")
    temporary.replace(path)


def main() -> None:
    # The Rust acceptance test creates target.py in the worker at runtime.
    target = import_module("target")
    assert hasattr(target, "first")
    assert hasattr(target, "second")
    first = target.first
    second = target.second
    assert isinstance(first, int)
    assert isinstance(second, int)
    if (first, second) == (11, 22):
        return  # The sequential baseline must not wait for the two-mutant barrier.
    identity = "first" if first != 11 else "second"  # noqa: PLR2004 - Fixture baseline.
    mode, directory = sys.argv[1:]
    shared = Path(directory)
    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    # The backend must clean up this descendant after its root exits.
    payload = bytearray(96 * 1024 * 1024) if mode == "memory" else bytearray()
    for offset in range(0, len(payload), 4096):
        payload[offset] = 1
    limits = read_job(9, ExtendedLimits)
    accounting = read_job(1, Accounting)
    publish(
        shared / (identity + ".json"),
        {
            "pid": os.getpid(),
            "child_pid": child.pid,
            "flags": limits.BasicLimitInformation.LimitFlags,
            "memory_limit": limits.JobMemoryLimit,
            "process_limit": limits.BasicLimitInformation.ActiveProcessLimit,
            "active_processes": accounting.ActiveProcesses,
            "peak_job_memory": limits.PeakJobMemoryUsed,
            "payload_bytes": len(payload),
        },
    )
    deadline = time.monotonic() + 30
    while not (shared / "release").exists():
        if child.poll() is not None:
            msg = "descendant exited before simultaneous observation"
            raise AssertionError(msg)
        if time.monotonic() >= deadline:
            msg = "controller did not observe both roots"
            raise TimeoutError(msg)
        time.sleep(0.01)
    # Keep the payload strongly referenced through the barrier.
    assert len(payload) == (96 * 1024 * 1024 if mode == "memory" else 0)


if __name__ == "__main__":
    main()
