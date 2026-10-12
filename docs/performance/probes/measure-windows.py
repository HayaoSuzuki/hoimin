"""Run a fixed argv with a deadline; record Windows process CPU and peak RSS.

Usage: python measure-windows.py OUTPUT.json TIMEOUT_SECONDS EXPECTED_EXIT EXE [ARG ...]
This is an offline measurement probe, not part of the product or CI.
"""

import ctypes
import json
import subprocess
import sys
import time
from ctypes import wintypes
from pathlib import Path


class Memory(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("faults", wintypes.DWORD),
        *[(name, ctypes.c_size_t) for name in (
            "peak_rss", "rss", "peak_paged", "paged", "peak_nonpaged",
            "nonpaged", "pagefile", "peak_pagefile",
        )],
    ]


def main():
    output, timeout, expected_exit, *command = sys.argv[1:]
    memory_info = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
    memory_info.argtypes = [wintypes.HANDLE, ctypes.POINTER(Memory), wintypes.DWORD]
    memory_info.restype = wintypes.BOOL
    process_times = ctypes.WinDLL("kernel32", use_last_error=True).GetProcessTimes
    process_times.argtypes = [wintypes.HANDLE, *[ctypes.POINTER(wintypes.FILETIME)] * 4]
    process_times.restype = wintypes.BOOL
    start = time.perf_counter()
    with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
        try:
            stdout, stderr = process.communicate(timeout=float(timeout))
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate()
            raise
        wall = time.perf_counter() - start
        handle = wintypes.HANDLE(int(process._handle))
        memory = Memory(cb=ctypes.sizeof(Memory))
        if not memory_info(handle, ctypes.byref(memory), memory.cb):
            raise ctypes.WinError(ctypes.get_last_error())
        times = [wintypes.FILETIME() for _ in range(4)]
        if not process_times(handle, *[ctypes.byref(value) for value in times]):
            raise ctypes.WinError(ctypes.get_last_error())
        cpu = sum((value.dwHighDateTime << 32) + value.dwLowDateTime for value in times[2:]) / 10_000_000
        result = dict(argv=command, wall_seconds=wall, cpu_seconds=cpu,
                      peak_rss_bytes=memory.peak_rss, returncode=process.returncode,
                      stdout=stdout.decode("utf-8"), stderr=stderr.decode("utf-8"))
        Path(output).write_text(json.dumps(result, indent=2), encoding="utf-8")
        print(json.dumps(result))
        if process.returncode != int(expected_exit):
            raise SystemExit(f"unexpected exit status: {process.returncode}")


if __name__ == "__main__":
    main()
