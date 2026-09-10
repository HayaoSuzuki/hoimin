import os
import re
import subprocess
import sys
import unittest
import zipfile
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

from packaging.tags import Tag
from wheel_smoke import (
    COMMAND_TIMEOUT_SECONDS,
    WheelMetadata,
    assert_help_hides_python_option,
    assert_mutation_result,
    environment_hoimin,
    environment_python,
    is_compatible_wheel,
    isolated_environment,
    run,
    select_compatible_wheel,
    validate_wheel_metadata,
    wheel_metadata,
    wheel_path,
    write_fixture,
)

LINUX_RUNTIME_TAGS = frozenset(
    {
        Tag("cp314", "cp314", "manylinux_2_17_x86_64"),
        Tag("cp314", "abi3", "manylinux_2_17_x86_64"),
    }
)
WINDOWS_RUNTIME_TAGS = frozenset({Tag("cp314", "cp314", "win_amd64")})
MACOS_RUNTIME_TAGS = frozenset({Tag("cp314", "cp314", "macosx_11_0_arm64")})


def runtime_tags_for(system: str) -> frozenset[Tag]:
    return {
        "linux": LINUX_RUNTIME_TAGS,
        "win32": WINDOWS_RUNTIME_TAGS,
        "darwin": MACOS_RUNTIME_TAGS,
    }.get(system, frozenset())


class StandaloneContractTests(unittest.TestCase):
    def test_windows_quality_and_wheel_smoke_remain_in_manual_ci(self) -> None:
        repository_root = Path(__file__).resolve().parents[1]
        manual_ci = (
            repository_root / ".github/workflows/non-linux-ci.yml"
        ).read_text(encoding="utf-8")
        for job in ("quality", "wheel-smoke"):
            with self.subTest(job=job):
                section = re.search(
                    rf"(?ms)^  {re.escape(job)}:\n(?P<body>.*?)(?=^  \S|\Z)",
                    manual_ci,
                )
                self.assertIsNotNone(section)
                assert section is not None
                self.assertIn("windows-latest", section.group("body"))

    def test_documentation_requires_build_before_standalone_smoke(self) -> None:
        repository_root = Path(__file__).resolve().parents[1]
        development = (repository_root / "docs/development.md").read_text(encoding="utf-8")
        readme = (repository_root / "README.md").read_text(encoding="utf-8")
        development_build_command = "uvx maturin build --release"
        readme_build_command = "uv run maturin build --release"
        development_smoke = "uv run --frozen python tests/wheel_smoke.py"
        readme_smoke = "uv run python tests/wheel_smoke.py"

        self.assertIn(
            "must build a release wheel first",
            " ".join(development.split()),
        )
        self.assertLess(
            development.index(development_build_command),
            development.index(development_smoke),
        )
        self.assertLess(readme.index(readme_build_command), readme.index(readme_smoke))

    def test_standalone_script_rejects_an_empty_wheel_directory(self) -> None:
        repository_root = Path(__file__).resolve().parents[1]
        source = repository_root / "tests/wheel_smoke.py"
        with TemporaryDirectory() as temporary_directory:
            temporary_root = Path(temporary_directory)
            tests = temporary_root / "tests"
            tests.mkdir()
            script = tests / "wheel_smoke.py"
            script.write_bytes(source.read_bytes())
            environment = os.environ.copy()
            environment.pop("HOIMIN_WHEEL", None)

            completed = subprocess.run(
                [sys.executable, str(script)],
                cwd=temporary_root,
                env=environment,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )

            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("build a wheel first", completed.stderr)
            self.assertFalse((temporary_root / "target/wheels").exists())


class WheelSelectionTests(unittest.TestCase):
    def test_compatibility_cases(self) -> None:
        cases = (
            (
                "Linux accepts x86_64",
                "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl",
                "linux",
                "x86_64",
                True,
            ),
            (
                "Linux rejects Windows",
                "hoimin-0.1.0-cp314-cp314-win_amd64.whl",
                "linux",
                "x86_64",
                False,
            ),
            (
                "Linux rejects macOS x86_64",
                "hoimin-0.1.0-cp314-cp314-macosx_14_0_x86_64.whl",
                "linux",
                "x86_64",
                False,
            ),
            (
                "Linux rejects a wheel for another Python ABI",
                "hoimin-0.1.0-cp313-cp313-manylinux_2_17_x86_64.whl",
                "linux",
                "x86_64",
                False,
            ),
            (
                "Linux rejects musllinux on a glibc runtime",
                "hoimin-0.1.0-cp314-cp314-musllinux_1_2_x86_64.whl",
                "linux",
                "x86_64",
                False,
            ),
            (
                "Linux rejects a newer unsupported manylinux baseline",
                "hoimin-0.1.0-cp314-cp314-manylinux_2_99_x86_64.whl",
                "linux",
                "x86_64",
                False,
            ),
            (
                "Windows accepts amd64",
                "hoimin-0.1.0-cp314-cp314-win_amd64.whl",
                "win32",
                "amd64",
                True,
            ),
            (
                "Windows rejects Linux",
                "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl",
                "win32",
                "amd64",
                False,
            ),
            (
                "macOS arm64 accepts arm64",
                "hoimin-0.1.0-cp314-cp314-macosx_11_0_arm64.whl",
                "darwin",
                "arm64",
                True,
            ),
            (
                "macOS arm64 rejects universal2",
                "hoimin-0.1.0-cp314-cp314-macosx_11_0_universal2.whl",
                "darwin",
                "arm64",
                False,
            ),
            (
                "macOS rejects a newer unsupported deployment target",
                "hoimin-0.1.0-cp314-cp314-macosx_99_0_arm64.whl",
                "darwin",
                "arm64",
                False,
            ),
            (
                "macOS Intel rejects arm64",
                "hoimin-0.1.0-cp314-cp314-macosx_11_0_arm64.whl",
                "darwin",
                "x86_64",
                False,
            ),
            (
                "unknown system rejects all",
                "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl",
                "freebsd",
                "x86_64",
                False,
            ),
            (
                "malformed filename fails closed",
                "hoimin-any_x86_64.whl",
                "linux",
                "x86_64",
                False,
            ),
        )

        for name, filename, system, machine, expected in cases:
            with self.subTest(name=name):
                # Arrange
                wheel = Path(filename)

                # Act
                actual = is_compatible_wheel(
                    wheel,
                    system,
                    machine,
                    supported_tags=runtime_tags_for(system),
                )

                # Assert
                self.assertIs(actual, expected)

    def test_selects_the_expected_semantic_version(self) -> None:
        # Arrange
        wheels = [
            Path("hoimin-0.9.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
            Path("hoimin-0.10.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
            Path("hoimin-0.10.0-cp314-cp314-win_amd64.whl"),
        ]

        # Act
        actual = select_compatible_wheel(
            wheels,
            system="linux",
            machine="x86_64",
            expected_name="hoimin",
            expected_version="0.10.0",
            supported_tags=LINUX_RUNTIME_TAGS,
        )

        # Assert
        self.assertEqual(
            actual,
            Path("hoimin-0.10.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
        )

    def test_rejects_a_candidate_list_without_a_compatible_wheel(self) -> None:
        # Arrange
        wheels = [Path("hoimin-0.1.0-cp314-cp314-win_amd64.whl")]

        # Act
        error = self.assertRaisesRegex(AssertionError, "no current compatible wheel")

        # Assert
        with error:
            select_compatible_wheel(
                wheels,
                system="linux",
                machine="x86_64",
                expected_name="hoimin",
                expected_version="0.1.0",
                supported_tags=LINUX_RUNTIME_TAGS,
            )

    def test_rejects_stale_compatible_wheels(self) -> None:
        wheels = [
            Path("hoimin-0.9.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
        ]

        with self.assertRaisesRegex(AssertionError, "no current compatible wheel"):
            select_compatible_wheel(
                wheels,
                system="linux",
                machine="x86_64",
                expected_name="hoimin",
                expected_version="0.10.0",
                supported_tags=LINUX_RUNTIME_TAGS,
            )

    def test_rejects_ambiguous_current_compatible_wheels(self) -> None:
        wheels = [
            Path("hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl"),
            Path("hoimin-0.1.0-cp314-abi3-manylinux_2_17_x86_64.whl"),
        ]

        with self.assertRaisesRegex(AssertionError, "multiple current compatible wheels"):
            select_compatible_wheel(
                wheels,
                system="linux",
                machine="x86_64",
                expected_name="hoimin",
                expected_version="0.1.0",
                supported_tags=LINUX_RUNTIME_TAGS,
            )

    def test_uses_the_explicit_wheel_override(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            override = Path(temporary_directory) / "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl"
            override.touch()

            # Act
            actual = wheel_path(
                environment={"HOIMIN_WHEEL": str(override)},
                wheel_directory=Path(temporary_directory) / "wheels",
                system="linux",
                machine="x86_64",
                supported_tags=LINUX_RUNTIME_TAGS,
            )

            # Assert
            self.assertEqual(actual, override.resolve())

    def test_rejects_a_stale_explicit_wheel_override(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            override = Path(temporary_directory) / "hoimin-0.9.0-cp314-cp314-manylinux_2_17_x86_64.whl"
            override.touch()

            with self.assertRaisesRegex(AssertionError, "no current compatible wheel"):
                wheel_path(
                    environment={"HOIMIN_WHEEL": str(override)},
                    wheel_directory=Path(temporary_directory) / "wheels",
                    system="linux",
                    machine="x86_64",
                    supported_tags=LINUX_RUNTIME_TAGS,
                )

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

    def test_discovers_the_current_compatible_wheel(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            wheel_directory = Path(temporary_directory)
            stale = wheel_directory / "hoimin-0.0.9-cp314-cp314-manylinux_2_17_x86_64.whl"
            current = wheel_directory / "hoimin-0.1.0-cp314-cp314-manylinux_2_17_x86_64.whl"
            incompatible = wheel_directory / "hoimin-0.1.0-cp314-cp314-win_amd64.whl"
            stale.touch()
            current.touch()
            incompatible.touch()

            # Act
            actual = wheel_path(
                environment={},
                wheel_directory=wheel_directory,
                system="linux",
                machine="x86_64",
                supported_tags=LINUX_RUNTIME_TAGS,
            )

            # Assert
            self.assertEqual(actual, current)


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
                    project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
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


class SmokeFixtureTests(unittest.TestCase):
    def test_isolated_environment_removes_python_import_state(self) -> None:
        # Arrange
        source = {
            "KEEP": "value",
            "PYTHONHOME": "/python-home",
            "PYTHONPATH": "/checkout",
            "VIRTUAL_ENV": "/virtual-environment",
        }

        # Act
        actual = isolated_environment(source)

        # Assert
        self.assertEqual(actual, {"KEEP": "value", "PYTHONNOUSERSITE": "1"})

    def test_environment_python_paths(self) -> None:
        cases = (
            ("POSIX", False, Path("environment/bin/python")),
            ("Windows", True, Path("environment/Scripts/python.exe")),
        )

        for name, is_windows, expected in cases:
            with self.subTest(name=name):
                # Arrange
                root = Path("environment")

                # Act
                actual = environment_python(root, is_windows=is_windows)

                # Assert
                self.assertEqual(actual, expected)

    def test_environment_hoimin_paths(self) -> None:
        cases = (
            ("POSIX", False, Path("environment/bin/hoimin")),
            ("Windows", True, Path("environment/Scripts/hoimin.exe")),
        )

        for name, is_windows, expected in cases:
            with self.subTest(name=name):
                # Arrange
                root = Path("environment")

                # Act
                actual = environment_hoimin(root, is_windows=is_windows)

                # Assert
                self.assertEqual(actual, expected)

    def test_write_fixture_creates_the_expected_project(self) -> None:
        with TemporaryDirectory() as temporary_directory:
            # Arrange
            root = Path(temporary_directory) / "project"

            # Act
            target = write_fixture(root)

            # Assert
            self.assertEqual(target, root / "src" / "calc.py")
            self.assertEqual((root / "src" / "__init__.py").read_text(), "")
            self.assertEqual(
                target.read_text(),
                "def add(left, right):\n    return left + right\n",
            )
            self.assertEqual(
                (root / "tests" / "test_calc.py").read_text(),
                "from src.calc import add\n\n\ndef test_add():\n    assert add(2, 1) == 3\n",
            )


class CommandAndResultTests(unittest.TestCase):
    def test_run_returns_a_successful_completed_process(self) -> None:
        # Arrange
        argv = ["hoimin", "--version"]
        completed = subprocess.CompletedProcess(argv, 0, stdout="hoimin 0.1.0\n", stderr="")
        with patch("wheel_smoke.subprocess.run", return_value=completed) as mocked_run:
            # Act
            actual = run(argv, cwd=Path("work"), env={"PYTHONNOUSERSITE": "1"})

            # Assert
            self.assertIs(actual, completed)
            mocked_run.assert_called_once_with(
                argv,
                cwd=Path("work"),
                env={"PYTHONNOUSERSITE": "1"},
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                shell=False,
                timeout=COMMAND_TIMEOUT_SECONDS,
                check=False,
            )

    def test_run_reports_command_output_for_a_failure(self) -> None:
        # Arrange
        argv = ["hoimin", "run"]
        completed = subprocess.CompletedProcess(argv, 7, stdout="command output", stderr="command error")
        with patch("wheel_smoke.subprocess.run", return_value=completed):
            # Act
            error = self.assertRaisesRegex(
                AssertionError,
                r"(?s)command failed \(7\): .*stdout:\ncommand output\nstderr:\ncommand error",
            )

            # Assert
            with error:
                run(argv, cwd=Path("work"), env={})

    def test_help_output_rejects_the_python_option(self) -> None:
        cases = (
            (
                "stdout",
                subprocess.CompletedProcess(["hoimin"], 0, stdout="--python", stderr=""),
            ),
            (
                "stderr",
                subprocess.CompletedProcess(["hoimin"], 0, stdout="", stderr="--python"),
            ),
        )

        for channel, completed in cases:
            with self.subTest(channel=channel):
                # Arrange
                help_output = completed

                # Act
                error = self.assertRaisesRegex(AssertionError, "--python")

                # Assert
                with error:
                    assert_help_hides_python_option(help_output)

    def test_mutation_result_accepts_terminal_mutants(self) -> None:
        cases = (
            ("killed", '{"mutants": [{"status": "killed"}]}'),
            ("survived", '{"mutants": [{"status": "survived"}]}'),
        )

        for status, output in cases:
            with self.subTest(status=status):
                # Arrange
                result_output = output

                # Act
                actual = assert_mutation_result(result_output)

                # Assert
                self.assertIsNone(actual)

    def test_mutation_result_rejects_an_empty_mutant_list(self) -> None:
        # Arrange
        output = '{"mutants": []}'

        # Act
        error = self.assertRaises(AssertionError)

        # Assert
        with error:
            assert_mutation_result(output)

    def test_mutation_result_rejects_a_nonterminal_mutant_list(self) -> None:
        # Arrange
        output = '{"mutants": [{"status": "timeout"}]}'

        # Act
        error = self.assertRaises(AssertionError)

        # Assert
        with error:
            assert_mutation_result(output)


if __name__ == "__main__":
    unittest.main()
