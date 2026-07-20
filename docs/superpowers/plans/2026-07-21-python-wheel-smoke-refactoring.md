# Python Wheel Smoke Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Python wheel smoke harness and its unit tests clear, small, deterministic, and behaviorally compatible with the existing release-level smoke test.

**Architecture:** Keep `tests/wheel_smoke.py` as one standard-library module, but make each side effect or protocol check a narrowly named function. Cover those functions with AAA-style `unittest` cases, using `subTest` only for a homogeneous input table. Retain the real-wheel script as the packaging and orchestration integration test.

**Tech Stack:** Python 3.14 standard library (`unittest`, `unittest.mock`, `zipfile`, `venv`, `subprocess`), uv, Maturin, GitHub Actions.

## Global Constraints

- Do not add Python test dependencies or convert the project to pytest.
- Keep `tests/wheel_smoke.py` as one module; do not introduce classes or new Python modules.
- Preserve the public CLI, wheel metadata values, release workflow, wheel-selection semantics, and smoke-test command arguments.
- `HOIMIN_WHEEL` must override discovery when it names an existing file.
- Linux and Windows x86-64 selection and Apple Silicon macOS arm64 selection must keep their current semantics.
- Tests use explicit `# Arrange`, `# Act`, and `# Assert` sections. Each test or `subTest` represents exactly one scenario.
- Keep `uv run --frozen python tests/wheel_smoke.py` as the actual-wheel integration test.
- Leave the pre-existing untracked `.idea/` directory untouched.

---

## File Structure

- Modify: `tests/wheel_smoke.py` — small, typed helper functions and the thin real-wheel smoke orchestration.
- Modify: `tests/test_wheel_smoke.py` — AAA-style `unittest` coverage for wheel lookup, archive metadata, environment isolation, fixture creation, subprocess handling, and JSON result checks.
- Modify: `docs/development.md` — document the fast unit command before the real-wheel smoke command.
- Modify: `.github/workflows/ci.yml` — run the standard-library Python unit suite in the existing `wheel-smoke` job before building a wheel; retain the current build and real-smoke steps.

### Task 1: Make wheel discovery a pure, unit-tested boundary

**Files:**
- Modify: `tests/wheel_smoke.py:3-16,33-57,97`
- Modify: `tests/test_wheel_smoke.py:1-29`

**Interfaces:**
- Consumes: a list of `Path` wheel candidates and a `(system, machine)` host pair.
- Produces: `select_compatible_wheel(wheels: list[Path], *, system: str, machine: str) -> Path` and `wheel_path(*, environment: Mapping[str, str], wheel_directory: Path, system: str, machine: str) -> Path`.
- Invariant: `wheel_path` uses `HOIMIN_WHEEL` before discovery; discovered candidates are sorted and the final compatible candidate is selected.

- [ ] **Step 1: Replace the existing mixed compatibility tests with focused AAA tests**

Replace `tests/test_wheel_smoke.py` with the following initial test module. Every row in `test_compatibility_cases` is one `subTest` scenario; the remaining tests cover one lookup outcome each.

```python
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
```

- [ ] **Step 2: Run the unit suite to verify the new lookup test fails**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v
```

Expected: FAIL during import because `select_compatible_wheel` does not yet exist.

- [ ] **Step 3: Inject discovery inputs and implement selection**

In `tests/wheel_smoke.py`, import `Mapping` and replace the existing `wheel_path` implementation with the following code. Keep `is_compatible_wheel` unchanged.

```python
from collections.abc import Mapping


def select_compatible_wheel(
    wheels: list[Path], *, system: str, machine: str
) -> Path:
    compatible = sorted(
        wheel
        for wheel in wheels
        if is_compatible_wheel(wheel, system, machine)
    )
    assert compatible, f"no wheel for {system}: {[wheel.name for wheel in wheels]}"
    return compatible[-1]


def wheel_path(
    *,
    environment: Mapping[str, str],
    wheel_directory: Path,
    system: str,
    machine: str,
) -> Path:
    override = environment.get("HOIMIN_WHEEL")
    if override:
        wheel = Path(override).resolve()
        assert wheel.is_file(), f"HOIMIN_WHEEL does not exist: {wheel}"
        return wheel

    wheels = sorted(wheel_directory.glob("hoimin-*.whl"))
    assert wheels, "build a wheel first with uv run maturin build --release"
    return select_compatible_wheel(wheels, system=system, machine=machine)
```

Change the first statement of `main()` to supply the live dependencies explicitly:

```python
    wheel = wheel_path(
        environment=os.environ,
        wheel_directory=REPOSITORY_ROOT / "target" / "wheels",
        system=sys.platform,
        machine=platform.machine().lower(),
    )
```

- [ ] **Step 4: Run the focused wheel-selection suite**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v
```

Expected: PASS; all wheel compatibility, override, empty-directory, and compatible-candidate cases pass.

- [ ] **Step 5: Commit the wheel-discovery boundary**

```console
git add tests/wheel_smoke.py tests/test_wheel_smoke.py
git commit -m "test: clarify wheel smoke selection"
```

### Task 2: Extract wheel metadata validation and archive tests

**Files:**
- Modify: `tests/wheel_smoke.py:19-30,60-65,98-102`
- Modify: `tests/test_wheel_smoke.py`

**Interfaces:**
- Consumes: a wheel archive `Path` or a `WheelMetadata` record.
- Produces: `wheel_metadata(wheel: Path) -> WheelMetadata` and `validate_wheel_metadata(metadata: WheelMetadata) -> None`.
- Invariant: exactly one `*.dist-info/METADATA` member is required; the Python requirement ignores spaces, while all other metadata fields match the existing values exactly.

- [ ] **Step 1: Add archive and metadata-contract tests**

Add `import zipfile` and replace the `wheel_smoke` import with the following block. Then append `WheelMetadataTests` below `WheelSelectionTests`.

```python
from wheel_smoke import (
    WheelMetadata,
    is_compatible_wheel,
    select_compatible_wheel,
    validate_wheel_metadata,
    wheel_metadata,
    wheel_path,
)


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
```

- [ ] **Step 2: Run the unit suite to verify metadata validation is absent**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v
```

Expected: FAIL during import because `validate_wheel_metadata` does not yet exist.

- [ ] **Step 3: Add named expected metadata and validation**

In `tests/wheel_smoke.py`, immediately after `WheelMetadata`, add the expected record and replace `wheel_metadata` with the following two functions:

```python
EXPECTED_WHEEL_METADATA = WheelMetadata(
    requires_python=">=3.14,<3.15",
    requires_dist=None,
    license_expression="MIT",
    project_urls=["Repository, https://github.com/tokyogas-tech/hoimin"],
)


def wheel_metadata(wheel: Path) -> WheelMetadata:
    with zipfile.ZipFile(wheel) as archive:
        metadata_files = sorted(
            name
            for name in archive.namelist()
            if name.endswith(".dist-info/METADATA")
        )
        assert len(metadata_files) == 1, (
            f"expected exactly one METADATA member in {wheel}: {metadata_files}"
        )
        parsed = Parser().parsestr(archive.read(metadata_files[0]).decode("utf-8"))

    return WheelMetadata(
        requires_python=parsed["Requires-Python"],
        requires_dist=parsed.get_all("Requires-Dist"),
        license_expression=parsed["License-Expression"],
        project_urls=parsed.get_all("Project-URL"),
    )


def validate_wheel_metadata(metadata: WheelMetadata) -> None:
    assert metadata.requires_python.replace(" ", "") == (
        EXPECTED_WHEEL_METADATA.requires_python
    ), f"unexpected Requires-Python: {metadata.requires_python!r}"
    assert metadata.requires_dist == EXPECTED_WHEEL_METADATA.requires_dist, (
        f"unexpected Requires-Dist: {metadata.requires_dist!r}"
    )
    assert metadata.license_expression == EXPECTED_WHEEL_METADATA.license_expression, (
        f"unexpected License-Expression: {metadata.license_expression!r}"
    )
    assert metadata.project_urls == EXPECTED_WHEEL_METADATA.project_urls, (
        f"unexpected Project-URL: {metadata.project_urls!r}"
    )
```

Replace the four inline metadata assertions at the start of `main()` with:

```python
    validate_wheel_metadata(wheel_metadata(wheel))
```

- [ ] **Step 4: Run the focused metadata suite**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v
```

Expected: PASS; valid archives and metadata pass, while missing, multiple, and mismatching records fail only inside their asserted test cases.

- [ ] **Step 5: Commit the metadata boundary**

```console
git add tests/wheel_smoke.py tests/test_wheel_smoke.py
git commit -m "test: cover wheel smoke metadata"
```

### Task 3: Isolate the environment and fixture helpers

**Files:**
- Modify: `tests/wheel_smoke.py:68-93,105,110-117`
- Modify: `tests/test_wheel_smoke.py`

**Interfaces:**
- Consumes: a parent environment mapping, a fixture root, and an explicit Windows flag.
- Produces: `isolated_environment(source: Mapping[str, str]) -> dict[str, str]`, `environment_python(root: Path, *, is_windows: bool) -> Path`, and `environment_hoimin(root: Path, *, is_windows: bool) -> Path`.
- Invariant: remove `PYTHONPATH`, `PYTHONHOME`, and `VIRTUAL_ENV`; preserve unrelated variables; set `PYTHONNOUSERSITE` to `"1"`; fixture source and test text remain unchanged.

- [ ] **Step 1: Add deterministic environment, executable-path, and fixture tests**

Extend the `wheel_smoke` import with `environment_hoimin`, `environment_python`, `isolated_environment`, and `write_fixture`, then append this class.

```python
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
```

- [ ] **Step 2: Run the unit suite to verify the injected environment interface fails**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v
```

Expected: FAIL because `isolated_environment` accepts no source mapping and the executable helpers accept no `is_windows` keyword argument.

- [ ] **Step 3: Implement explicit environment and platform inputs**

In `tests/wheel_smoke.py`, add the constants and replace the helper functions with this code:

```python
ISOLATED_ENVIRONMENT_REMOVALS = ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV")
FIXTURE_SOURCE = "def add(left, right):\n    return left + right\n"
FIXTURE_TEST = "from src.calc import add\n\n\ndef test_add():\n    assert add(2, 1) == 3\n"


def environment_python(root: Path, *, is_windows: bool) -> Path:
    return root / ("Scripts/python.exe" if is_windows else "bin/python")


def environment_hoimin(root: Path, *, is_windows: bool) -> Path:
    return root / ("Scripts/hoimin.exe" if is_windows else "bin/hoimin")


def isolated_environment(source: Mapping[str, str]) -> dict[str, str]:
    environment = dict(source)
    for name in ISOLATED_ENVIRONMENT_REMOVALS:
        environment.pop(name, None)
    environment["PYTHONNOUSERSITE"] = "1"
    return environment


def write_fixture(root: Path) -> Path:
    source = root / "src"
    tests = root / "tests"
    source.mkdir(parents=True)
    tests.mkdir()
    (source / "__init__.py").write_text("", encoding="utf-8")
    target = source / "calc.py"
    target.write_text(FIXTURE_SOURCE, encoding="utf-8")
    (tests / "test_calc.py").write_text(FIXTURE_TEST, encoding="utf-8")
    return target
```

In `main()`, make these exact substitutions:

```python
        environment = isolated_environment(os.environ)
        is_windows = os.name == "nt"
```

```python
        python = environment_python(environment_root, is_windows=is_windows)
```

```python
        executable = environment_hoimin(environment_root, is_windows=is_windows)
```

- [ ] **Step 4: Run the focused fixture-helper suite**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v
```

Expected: PASS; the environment is independent of the host process, both executable layouts are selected from supplied input, and the fixture text is exact.

- [ ] **Step 5: Commit the deterministic fixture helpers**

```console
git add tests/wheel_smoke.py tests/test_wheel_smoke.py
git commit -m "refactor: isolate wheel smoke helpers"
```

### Task 4: Validate subprocess and smoke output with focused helpers

**Files:**
- Modify: `tests/wheel_smoke.py:27-30,96-126`
- Modify: `tests/test_wheel_smoke.py`

**Interfaces:**
- Consumes: command argv, `cwd`, an isolated environment, `CompletedProcess[str]`, and JSON text emitted by `hoimin`.
- Produces: `run(argv: list[str], *, cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]`, `assert_help_hides_python_option(completed: subprocess.CompletedProcess[str]) -> None`, and `assert_mutation_result(output: str) -> None`.
- Invariant: `run` captures output, never invokes a shell, applies the 120-second timeout, and raises a diagnostic containing argv, return code, stdout, and stderr for nonzero commands.

- [ ] **Step 1: Add mocked subprocess and result-contract tests**

Add `import subprocess` and `from unittest.mock import patch`. Extend the `wheel_smoke` import with `COMMAND_TIMEOUT_SECONDS`, `assert_help_hides_python_option`, `assert_mutation_result`, and `run`, then append this class.

```python
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
            ("stdout", subprocess.CompletedProcess(["hoimin"], 0, stdout="--python", stderr="")),
            ("stderr", subprocess.CompletedProcess(["hoimin"], 0, stdout="", stderr="--python")),
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
```

- [ ] **Step 2: Run the unit suite to verify the output validators are absent**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_wheel_smoke.py' -v
```

Expected: FAIL during import because `COMMAND_TIMEOUT_SECONDS`, `assert_help_hides_python_option`, and `assert_mutation_result` do not yet exist.

- [ ] **Step 3: Implement output validators and make `main()` a linear orchestration**

Add the timeout constant near `REPOSITORY_ROOT`, expand `run`, and add these validators:

```python
COMMAND_TIMEOUT_SECONDS = 120


def run(
    argv: list[str], *, cwd: Path, env: dict[str, str]
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        argv,
        cwd=cwd,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        shell=False,
        timeout=COMMAND_TIMEOUT_SECONDS,
        check=False,
    )
    assert completed.returncode == 0, (
        f"command failed ({completed.returncode}): {argv!r}\n"
        f"stdout:\n{completed.stdout}\n"
        f"stderr:\n{completed.stderr}"
    )
    return completed


def assert_help_hides_python_option(
    completed: subprocess.CompletedProcess[str],
) -> None:
    assert "--python" not in completed.stdout, (
        f"installed help unexpectedly exposes --python in stdout: {completed.stdout!r}"
    )
    assert "--python" not in completed.stderr, (
        f"installed help unexpectedly exposes --python in stderr: {completed.stderr!r}"
    )


def assert_mutation_result(output: str) -> None:
    document = json.loads(output)
    mutants = document["mutants"]
    assert mutants, "mutation run produced no mutants"
    assert any(mutant["status"] in {"killed", "survived"} for mutant in mutants), (
        f"mutation run had no killed or survived mutant: {mutants!r}"
    )
```

Replace `main()` with the following complete orchestration, keeping its return value and command arguments unchanged:

```python
def main() -> int:
    wheel = wheel_path(
        environment=os.environ,
        wheel_directory=REPOSITORY_ROOT / "target" / "wheels",
        system=sys.platform,
        machine=platform.machine().lower(),
    )
    validate_wheel_metadata(wheel_metadata(wheel))

    with tempfile.TemporaryDirectory(prefix="hoimin-wheel-smoke-") as temporary_directory:
        temporary_root = Path(temporary_directory)
        environment = isolated_environment(os.environ)
        is_windows = os.name == "nt"
        distribution_help = run(
            ["uvx", "--python", "3.14", "--from", str(wheel), "hoimin", "--help"],
            cwd=temporary_root,
            env=environment,
        )
        assert_help_hides_python_option(distribution_help)

        environment_root = temporary_root / "environment"
        venv.EnvBuilder(with_pip=True, clear=True).create(environment_root)
        python = environment_python(environment_root, is_windows=is_windows)
        run(
            [str(python), "-m", "pip", "install", "--disable-pip-version-check", str(wheel), "pytest>=8.4,<9"],
            cwd=temporary_root,
            env=environment,
        )
        executable = environment_hoimin(environment_root, is_windows=is_windows)
        version = run([str(executable), "--version"], cwd=temporary_root, env=environment)
        assert (version.stdout + version.stderr).strip().startswith("hoimin ")

        fixture = temporary_root / "project"
        target = write_fixture(fixture)
        original = hashlib.sha256(target.read_bytes()).digest()
        completed = run(
            [str(executable), "run", "--root", str(fixture), "--source", "src", "--file", "src/calc.py", "--max-mutants", "16", "--max-candidates", "64", "--total-timeout", "60s", "--allow-best-effort-memory", "--format", "json", "--", str(python), "-m", "pytest", "-q"],
            cwd=fixture,
            env=environment,
        )
        assert "--python" not in completed.stdout
        assert_mutation_result(completed.stdout)
        assert hashlib.sha256(target.read_bytes()).digest() == original
        assert "PYTHONPATH" not in environment

    return 0
```

- [ ] **Step 4: Run the full Python unit suite**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: PASS; mocked subprocesses verify the exact process contract, while output validators accept only the expected help and mutation-result states.

- [ ] **Step 5: Commit the command and result contracts**

```console
git add tests/wheel_smoke.py tests/test_wheel_smoke.py
git commit -m "refactor: clarify wheel smoke orchestration"
```

### Task 5: Run the unit suite in development and CI, then verify the real wheel

**Files:**
- Modify: `docs/development.md:3-13`
- Modify: `.github/workflows/ci.yml:99-111`

**Interfaces:**
- Consumes: the standard-library unit suite, the existing Maturin build, and the existing real-wheel smoke script.
- Produces: a documented and CI-enforced two-level Python verification sequence.
- Invariant: the release workflow remains unchanged; `wheel-smoke` still builds a release wheel and runs `tests/wheel_smoke.py` after the unit suite.

- [ ] **Step 1: Add the fast unit command before the real-wheel smoke command**

In `docs/development.md`, replace the current quality-gate command block with:

```console
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uv run maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

In the `wheel-smoke` job of `.github/workflows/ci.yml`, insert the unit step immediately before the existing Maturin build step:

```yaml
      - run: uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
      - run: uvx maturin build --release
      - run: uv run --frozen python tests/wheel_smoke.py
```

- [ ] **Step 2: Run the fast unit suite exactly as CI will**

Run:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

Expected: PASS; all focused unit and `subTest` scenarios are collected without third-party test dependencies.

- [ ] **Step 3: Build a real release wheel and run the integration smoke test**

Run:

```console
uv run maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

Expected: PASS; `uvx --from` and a clean virtual environment run the installed `hoimin` executable, return a valid mutation result, and leave the fixture source unchanged.

- [ ] **Step 4: Inspect the final diff and commit the verification wiring**

Run:

```console
git diff --check
git status --short
```

Expected: no whitespace errors; only the planned Python, test, documentation, and CI files are staged or modified, plus the pre-existing untracked `.idea/` directory.

Commit only the planned files:

```console
git add tests/wheel_smoke.py tests/test_wheel_smoke.py docs/development.md .github/workflows/ci.yml
git commit -m "test: run Python wheel smoke units in CI"
```

## Plan Self-Review

- Spec coverage: Tasks 1-4 implement the agreed helper boundaries, AAA and `subTest` conventions, error diagnostics, and unchanged smoke flow. Task 5 keeps the real-wheel check and makes the unit suite part of normal development and CI. No Rust, CLI, release-workflow, or dependency change is included.
- Placeholder scan: every implementation step includes exact file paths, signatures, code, a command, and expected result. There are no deferred implementation markers.
- Type consistency: `wheel_path` receives `Mapping[str, str]` and returns `Path`; `isolated_environment` returns the `dict[str, str]` accepted by `run`; `environment_python` and `environment_hoimin` use the same explicit `is_windows` flag in both tests and `main()`; all validator names in Task 4 match their `main()` call sites.
