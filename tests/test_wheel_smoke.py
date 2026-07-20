from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from wheel_smoke import (
    is_compatible_wheel,
    select_compatible_wheel,
    wheel_path,
)


class WheelSelectionTests(unittest.TestCase):
    def test_compatibility_cases(self) -> None:
        cases = (
            ("Linux accepts x86_64", "hoimin-manylinux_x86_64.whl", "linux", "x86_64", True),
            ("Linux rejects Windows", "hoimin-win_amd64.whl", "linux", "x86_64", False),
            ("Windows accepts amd64", "hoimin-win_amd64.whl", "win32", "amd64", True),
            ("Windows rejects Linux", "hoimin-manylinux_x86_64.whl", "win32", "amd64", False),
            ("macOS arm64 accepts arm64", "hoimin-macosx_11_0_arm64.whl", "darwin", "arm64", True),
            ("macOS arm64 rejects universal2", "hoimin-macosx_11_0_universal2.whl", "darwin", "arm64", False),
            ("macOS Intel rejects arm64", "hoimin-macosx_11_0_arm64.whl", "darwin", "x86_64", False),
            ("unknown system rejects all", "hoimin-any_x86_64.whl", "freebsd", "x86_64", False),
        )

        for name, filename, system, machine, expected in cases:
            with self.subTest(name=name):
                # Arrange
                wheel = Path(filename)

                # Act
                actual = is_compatible_wheel(wheel, system, machine)

                # Assert
                self.assertIs(actual, expected)

    def test_selects_the_latest_compatible_wheel(self) -> None:
        # Arrange
        wheels = [
            Path("hoimin-0.2.0-manylinux_x86_64.whl"),
            Path("hoimin-0.1.0-manylinux_x86_64.whl"),
            Path("hoimin-0.3.0-win_amd64.whl"),
        ]

        # Act
        actual = select_compatible_wheel(wheels, system="linux", machine="x86_64")

        # Assert
        self.assertEqual(actual, Path("hoimin-0.2.0-manylinux_x86_64.whl"))

    def test_rejects_a_candidate_list_without_a_compatible_wheel(self) -> None:
        # Arrange
        wheels = [Path("hoimin-0.1.0-win_amd64.whl")]

        # Act
        error = self.assertRaisesRegex(AssertionError, "no wheel for linux")

        # Assert
        with error:
            select_compatible_wheel(wheels, system="linux", machine="x86_64")

    def test_uses_the_explicit_wheel_override(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            override = Path(temporary_directory) / "override.whl"
            override.touch()

            # Act
            actual = wheel_path(
                environment={"HOIMIN_WHEEL": str(override)},
                wheel_directory=Path(temporary_directory) / "wheels",
                system="linux",
                machine="x86_64",
            )

            # Assert
            self.assertEqual(actual, override.resolve())

    def test_rejects_a_missing_explicit_wheel_override(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            missing = Path(temporary_directory) / "missing.whl"

            # Act
            error = self.assertRaisesRegex(AssertionError, "HOIMIN_WHEEL does not exist")

            # Assert
            with error:
                wheel_path(
                    environment={"HOIMIN_WHEEL": str(missing)},
                    wheel_directory=Path(temporary_directory),
                    system="linux",
                    machine="x86_64",
                )

    def test_rejects_an_empty_wheel_directory(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            wheel_directory = Path(temporary_directory)

            # Act
            error = self.assertRaisesRegex(AssertionError, "build a wheel first")

            # Assert
            with error:
                wheel_path(
                    environment={},
                    wheel_directory=wheel_directory,
                    system="linux",
                    machine="x86_64",
                )

    def test_discovers_the_latest_compatible_wheel(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            wheel_directory = Path(temporary_directory)
            older = wheel_directory / "hoimin-0.1.0-manylinux_x86_64.whl"
            latest = wheel_directory / "hoimin-0.2.0-manylinux_x86_64.whl"
            incompatible = wheel_directory / "hoimin-0.3.0-win_amd64.whl"
            older.touch()
            latest.touch()
            incompatible.touch()

            # Act
            actual = wheel_path(
                environment={},
                wheel_directory=wheel_directory,
                system="linux",
                machine="x86_64",
            )

            # Assert
            self.assertEqual(actual, latest)


if __name__ == "__main__":
    unittest.main()
