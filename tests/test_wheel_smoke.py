from pathlib import Path
import unittest

from wheel_smoke import is_compatible_wheel


class CompatibleWheelTests(unittest.TestCase):
    def test_accepts_only_arm64_macos_wheels_on_apple_silicon(self) -> None:
        arm64 = Path("hoimin-0.1.0-py3-none-macosx_11_0_arm64.whl")
        x86_64 = Path("hoimin-0.1.0-py3-none-macosx_10_12_x86_64.whl")
        universal2 = Path("hoimin-0.1.0-py3-none-macosx_10_12_universal2.whl")

        self.assertTrue(is_compatible_wheel(arm64, "darwin", "arm64"))
        self.assertFalse(is_compatible_wheel(x86_64, "darwin", "arm64"))
        self.assertFalse(is_compatible_wheel(universal2, "darwin", "arm64"))
        self.assertFalse(is_compatible_wheel(arm64, "darwin", "x86_64"))

    def test_preserves_linux_and_windows_wheel_selection(self) -> None:
        linux = Path("hoimin-0.1.0-py3-none-manylinux_2_17_x86_64.whl")
        windows = Path("hoimin-0.1.0-py3-none-win_amd64.whl")

        self.assertTrue(is_compatible_wheel(linux, "linux", "x86_64"))
        self.assertFalse(is_compatible_wheel(windows, "linux", "x86_64"))
        self.assertTrue(is_compatible_wheel(windows, "win32", "amd64"))
        self.assertFalse(is_compatible_wheel(linux, "win32", "amd64"))


if __name__ == "__main__":
    unittest.main()
