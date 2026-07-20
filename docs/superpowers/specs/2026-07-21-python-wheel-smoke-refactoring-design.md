# Python Wheel Smoke Refactoring Design

## Goal

Refactor the repository's Python wheel smoke harness so that its behavior is
easy to understand, its unit tests express one scenario at a time, and the
release-level smoke check keeps its current externally observable behavior.

The active Python production surface is `tests/wheel_smoke.py`.  Its purpose is
to select a host-compatible wheel, verify distribution metadata, install the
wheel in an isolated virtual environment, and run a small mutation-testing
project through the installed `hoimin` executable.

## Scope

In scope:

- Refactor `tests/wheel_smoke.py` without changing the wheel, CLI, or CI
  contract.
- Expand `tests/test_wheel_smoke.py` into a clear unit suite for every helper
  and its meaningful error paths.
- Retain `uv run --frozen python tests/wheel_smoke.py` as the end-to-end smoke
  check that uses a real release wheel.

Out of scope:

- Rust code, wheel metadata values, CLI options, release workflow changes, and
  new third-party Python test dependencies.
- The historical Python analyzer mentioned in older design documents; it was
  removed from this repository and is not an active code path.

## Architecture

Keep the harness as one small standard-library module.  Do not introduce
classes or split it into multiple modules.  Instead, give each existing
responsibility a small, explicit function with a narrow contract:

- Determine whether one wheel filename is compatible with a `(system,
  machine)` pair, then select the latest compatible candidate.
- Resolve the `HOIMIN_WHEEL` override or discover candidate wheels from the
  repository's wheel directory.
- Read the one `*.dist-info/METADATA` file from a wheel and validate the
  project metadata contract.
- Build a sanitized child-process environment from a supplied environment
  mapping, removing Python-importing state and enabling `PYTHONNOUSERSITE`.
- Derive virtual-environment executable paths and create the minimal fixture
  project.
- Run a subprocess with captured output and report a failing command with its
  exit status, stdout, and stderr.

`main()` remains a thin, readable orchestration flow:

1. Locate the wheel and validate its metadata.
2. Create a temporary directory and sanitized environment.
3. Verify the installed-wheel `uvx` help output has no development-only
   `--python` option.
4. Create an isolated virtual environment, install the wheel and pytest, and
   verify the installed executable's version.
5. Create the fixture, run `hoimin`, validate its JSON result, and verify that
   the original source file was not changed.

Fixed protocol values, limits, and environment-variable names should have
descriptive constants when doing so makes the flow easier to scan.  The
existing command arguments and assertions remain semantically unchanged.

## Test Design

Use Python's built-in `unittest`; no pytest, parameterization library, or
additional project dependency is introduced.

Every test follows Arrange, Act, Assert structure with those three comments.
Each test or `subTest` covers exactly one input scenario and one expected
outcome.  `subTest` is used only for a homogeneous table of independent input
cases, such as host/platform wheel compatibility.  It does not combine
unrelated success and failure behaviors into one test body.

The unit suite covers:

- compatible and incompatible wheel filename cases for Linux, Windows, macOS,
  and unknown systems;
- candidate selection, absent compatible candidates, and explicit
  `HOIMIN_WHEEL` override behavior;
- correct metadata extraction and malformed, missing, or multiple metadata
  members;
- expected and unexpected package metadata;
- removal of `PYTHONPATH`, `PYTHONHOME`, and `VIRTUAL_ENV`, while preserving
  other input variables and setting `PYTHONNOUSERSITE`;
- POSIX and Windows virtual-environment executable paths;
- fixture file contents and the returned target path; and
- subprocess invocation arguments and failure diagnostics, with
  `subprocess.run` mocked.

The existing real-wheel command remains an integration test rather than being
mocked into a large `main()` unit test.  It protects the orchestration and
packaging boundary; the helper tests make its failures local and diagnosable.

## Error Handling and Compatibility

The harness is a test program, so assertion failures remain appropriate.  Each
failure must identify the relevant input: the wheel path or candidate list for
selection errors, the metadata member for archive errors, and command argv
plus stdout and stderr for subprocess failures.

The refactoring must preserve these observable contracts:

- `HOIMIN_WHEEL`, when set to an existing file, takes precedence over discovery.
- Wheel selection still uses the latest sorted host-compatible candidate.
- Linux, Windows, and Apple Silicon macOS wheel compatibility semantics stay
  unchanged.
- The child environment is isolated from checkout-local Python imports.
- The final mutation run emits JSON containing mutants, includes at least one
  killed or survived status, and leaves the fixture's source file unchanged.

## Implementation and Verification

Work test-first.  For each helper, first add a focused failing unit test; then
extract or simplify only the production code needed to make that test pass.
Refactor the thin orchestration last, after its helper contracts are covered.

Run the focused unit suite while iterating, then verify the complete change
with:

```console
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
uv run maturin build --release
uv run --frozen python tests/wheel_smoke.py
```

The second and third commands prove that the packaged executable, rather than
the repository checkout, still passes the smoke scenario.
