# Wheel Selection Correctness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure wheel smoke tests execute the unique current hoimin wheel that is compatible with the requested host platform.

**Architecture:** Use `packaging.utils.parse_wheel_filename` as the PEP 427 parsing boundary and inspect its parsed distribution, version, and platform tags. Read the expected project name and version from `pyproject.toml`; reject stale, cross-platform, malformed, and ambiguous artifacts instead of choosing by filename ordering.

**Tech Stack:** Python 3.14, `packaging`, `tomllib`, `unittest`, uv.

## Global Constraints

- Preserve the explicit `HOIMIN_WHEEL` escape hatch, while validating that its artifact is current and host-compatible.
- Preserve Windows amd64, Linux x86_64, and macOS arm64 support; do not accept macOS universal2 until the published support policy changes.
- Do not infer compatibility from raw filename substrings.
- Fail closed when zero or multiple current compatible artifacts exist.
- Keep the standalone smoke command and README usage unchanged.

---

### Task 1: Specify parsed wheel compatibility

**Files:**
- Modify: `pyproject.toml`
- Modify: `uv.lock`
- Modify: `tests/test_wheel_smoke.py`

**Interfaces:**
- Consumes: `is_compatible_wheel(Path, system, machine)`.
- Produces: regression expectations for parsed platform tags and malformed filenames.

- [ ] Add `packaging>=26.0` as an explicit development dependency and refresh the lock file.
- [ ] Add valid PEP 427 wheel filenames for Linux, Windows, and macOS compatibility cases.
- [ ] Add a Linux regression case proving `macosx_*_x86_64` is rejected.
- [ ] Add a malformed filename case proving parsing fails closed.
- [ ] Run `uv run --frozen python -m unittest tests.test_wheel_smoke.WheelSelectionTests.test_compatibility_cases -v` and confirm the cross-platform/malformed cases fail for the expected substring-based behavior.

### Task 2: Require the current project version and unique artifact

**Files:**
- Modify: `tests/test_wheel_smoke.py`

**Interfaces:**
- Consumes: `select_compatible_wheel(wheels, system, machine, expected_name, expected_version)`.
- Produces: stale-version and ambiguity regression coverage used by `wheel_path`.

- [ ] Replace the lexicographic latest-wheel test with semantic `0.9.0`/`0.10.0` cases and an explicit expected version.
- [ ] Add a stale-only case that reports no current compatible wheel.
- [ ] Add two current compatible wheels and assert an ambiguity failure listing both filenames.
- [ ] Add an explicit override case with a stale or incompatible wheel and assert it is rejected.
- [ ] Run the focused `WheelSelectionTests` and confirm each new test fails for the intended old behavior.

### Task 3: Implement parsed, fail-closed selection

**Files:**
- Modify: `tests/wheel_smoke.py`

**Interfaces:**
- Produces: `project_identity() -> tuple[str, Version]`.
- Produces: `parsed_wheel(Path) -> tuple[NormalizedName, Version, BuildTag, frozenset[Tag]] | None`.
- Produces: `is_compatible_wheel(...) -> bool` based on parsed `Tag.platform`.
- Produces: `select_compatible_wheel(...) -> Path` requiring one exact current artifact.

- [ ] Parse the workspace project identity from `pyproject.toml` with `tomllib`.
- [ ] Parse filenames with `parse_wheel_filename`; treat invalid filenames as incompatible and include them in diagnostics.
- [ ] Match normalized distribution, exact parsed version, and supported host platform tags.
- [ ] Assert exactly one candidate remains; provide separate no-match and ambiguity messages with all candidate names.
- [ ] Validate `HOIMIN_WHEEL` through the same identity/platform selection path.
- [ ] Run `uv run --frozen python -m unittest tests/test_wheel_smoke.py -v` and confirm all wheel selection and smoke helper tests pass.

### Task 4: Verify and deliver

**Files:**
- Review: `README.md`
- Review: all branch changes

**Interfaces:**
- Consumes: the completed selection implementation and test evidence.
- Produces: a reviewed branch and PR closing Issue #80.

- [ ] Confirm README commands and supported platforms remain accurate; do not edit it if the user-facing contract is unchanged.
- [ ] Run `uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v`.
- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`.
- [ ] Run `git diff --check` and review the full branch diff against Issue #80.
- [ ] Commit, push `fix/issue-80-wheel-selection`, and create a PR with `Closes #80`.
