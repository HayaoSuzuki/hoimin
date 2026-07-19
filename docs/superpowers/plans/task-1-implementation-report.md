# Task 1 implementation report

Implemented the reproducible Python quality gate.

Changes:

- Added bounded dev dependencies for Hypothesis, pytest-cov, pytest-randomly, and Ruff; regenerated `uv.lock`.
- Configured pytest for `python/tests` and `tests`, randomized execution, subprocess-aware coverage, 100% minimum coverage, fixture-tree omission, and wheel-smoke collection.
- Added a session-autouse fixture that builds a release wheel unless `HOIMIN_WHEEL` is supplied.
- Added the CI `python-quality` job and documented local quality and mutation commands.
- Applied Ruff fixes and noqa directives to the existing Python modules so the configured Ruff commands pass.

Verification:

- `uv run ruff check python tests`: passed.
- `uv run ruff format --check python tests`: passed.
- `uv run pytest --collect-only -q`: collected 22 tests from both `python/tests` and `tests`, including `tests/wheel_smoke.py`.
- `uv run pytest`: all 22 test bodies passed, including the self-contained wheel smoke test. The gate then failed as intended for Task 1 because combined coverage was 94.50%, below the configured 100% requirement. Missing lines were analyzer validation/candidate paths (19), the `HOIMIN_WHEEL` branch in `tests/conftest.py` (1), and wheel-smoke helper branches (3).

Concern:

Resolved in the first review-fix commit: generated bare `noqa` directives were replaced with construct-local, rule-specific rationales or code-level fixes.

## Review fix report

Replaced the generated bare Ruff suppressions with rule-specific, construct-local explanations. Added a real module docstring and `python/__init__.py` so the analyzer is no longer an undocumented implicit-namespace script. Documented every test-only per-file exception in `pyproject.toml`, retaining only categories required by the existing pytest test and smoke-test conventions. No coverage threshold, source selection, or test collection behavior was weakened.

Verification after the review fix:

- `uv run ruff check python tests`: passed.
- `uv run ruff format --check python tests`: passed.
- Focused and full pytest remain behaviorally unchanged; the 100% coverage gate is still intentionally pending Task 2.

Second review fix: added method docstrings to emit_candidate and emit_diagnostic, replacing their inaccurate LibCST-hook D102 suppressions.
