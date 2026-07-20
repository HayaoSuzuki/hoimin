from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
import zipfile

from wheel_smoke import (
    WheelMetadata,
    is_compatible_wheel,
    select_compatible_wheel,
    validate_wheel_metadata,
    wheel_metadata,
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


class WheelMetadataTests(unittest.TestCase):
    def test_reads_the_single_metadata_member(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            wheel = Path(temporary_directory) / "hoimin.whl"
            text = (
                "Requires-Python: >=3.14, <3.15\n"
                "License-Expression: MIT\n"
                "Project-URL: Repository, https://github.com/tokyogas-tech/hoimin\n"
            )
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr("hoimin-0.1.0.dist-info/METADATA", text)

            # Act
            actual = wheel_metadata(wheel)

            # Assert
            self.assertEqual(
                actual,
                WheelMetadata(
                    requires_python=">=3.14, <3.15",
                    requires_dist=None,
                    license_expression="MIT",
                    project_urls=[
                        "Repository, https://github.com/tokyogas-tech/hoimin"
                    ],
                ),
            )

    def test_rejects_an_archive_without_metadata(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            wheel = Path(temporary_directory) / "hoimin.whl"
            with zipfile.ZipFile(wheel, "w"):
                pass

            # Act
            error = self.assertRaisesRegex(AssertionError, "expected exactly one METADATA")

            # Assert
            with error:
                wheel_metadata(wheel)

    def test_rejects_an_archive_with_multiple_metadata_members(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            wheel = Path(temporary_directory) / "hoimin.whl"
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr("one.dist-info/METADATA", "License-Expression: MIT\n")
                archive.writestr("two.dist-info/METADATA", "License-Expression: MIT\n")

            # Act
            error = self.assertRaisesRegex(AssertionError, "expected exactly one METADATA")

            # Assert
            with error:
                wheel_metadata(wheel)

    def test_accepts_the_expected_metadata(self) -> None:
        # Arrange
        metadata = WheelMetadata(
            requires_python=">=3.14, <3.15",
            requires_dist=None,
            license_expression="MIT",
            project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
        )

        # Act
        actual = validate_wheel_metadata(metadata)

        # Assert
        self.assertIsNone(actual)

    def test_rejects_each_unexpected_metadata_field(self) -> None:
        cases = (
            (
                "Requires-Python",
                WheelMetadata(
                    requires_python=">=3.13,<3.15",
                    requires_dist=None,
                    license_expression="MIT",
                    project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
                ),
            ),
            (
                "Requires-Dist",
                WheelMetadata(
                    requires_python=">=3.14,<3.15",
                    requires_dist=["pytest"],
                    license_expression="MIT",
                    project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
                ),
            ),
            (
                "License-Expression",
                WheelMetadata(
                    requires_python=">=3.14,<3.15",
                    requires_dist=None,
                    license_expression="Apache-2.0",
                    project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
                ),
            ),
            (
                "Project-URL",
                WheelMetadata(
                    requires_python=">=3.14,<3.15",
                    requires_dist=None,
                    license_expression="MIT",
                    project_urls=["Homepage, https://example.invalid/"],
                ),
            ),
        )

        for field, metadata in cases:
            with self.subTest(field=field):
                # Arrange
                invalid_metadata = metadata

                # Act
                error = self.assertRaisesRegex(AssertionError, field)

                # Assert
                with error:
                    validate_wheel_metadata(invalid_metadata)


if __name__ == "__main__":
    unittest.main()
