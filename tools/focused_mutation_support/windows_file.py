from __future__ import annotations

import ctypes
from ctypes import wintypes
import os
from pathlib import Path
from typing import Callable, Final, cast


DELETE: Final = 0x00010000
GENERIC_READ: Final = 0x80000000
FILE_SHARE_READ: Final = 0x00000001
FILE_SHARE_WRITE: Final = 0x00000002
FILE_SHARE_DELETE: Final = 0x00000004
OPEN_EXISTING: Final = 3
FILE_ATTRIBUTE_NORMAL: Final = 0x00000080


class WindowsHandle:
    def __init__(self, value: int, close_handle: Callable[[int], int]) -> None:
        self._value: int | None = value
        self._close_handle = close_handle

    def close(self) -> None:
        if self._value is None:
            return
        if not self._close_handle(self._value):
            code = ctypes.get_last_error()  # type: ignore[attr-defined]
            raise ctypes.WinError(code)  # type: ignore[attr-defined]
        self._value = None

    def __enter__(self) -> WindowsHandle:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


def _open(path: Path, access: int, share: int) -> WindowsHandle:
    if os.name != "nt":
        raise RuntimeError("Windows file probing is only available on Windows")
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)  # type: ignore[attr-defined]
    create_file = kernel32.CreateFileW
    create_file.argtypes = (
        wintypes.LPCWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.LPVOID,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.HANDLE,
    )
    create_file.restype = wintypes.HANDLE
    close_handle = kernel32.CloseHandle
    close_handle.argtypes = (wintypes.HANDLE,)
    close_handle.restype = wintypes.BOOL
    value = create_file(
        str(path),
        access,
        share,
        None,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        None,
    )
    invalid = wintypes.HANDLE(-1).value
    if value == invalid:
        code = ctypes.get_last_error()  # type: ignore[attr-defined]
        raise OSError(
            code,
            ctypes.FormatError(code),  # type: ignore[attr-defined]
            str(path),
            code,
        )
    return WindowsHandle(
        cast(int, value),
        cast(Callable[[int], int], close_handle),
    )


def probe_delete_access(path: Path) -> None:
    with _open(
        path,
        DELETE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    ):
        pass


def open_without_delete_sharing_for_tests(path: Path) -> WindowsHandle:
    return _open(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE)
