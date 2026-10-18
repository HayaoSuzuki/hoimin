# Final-review documentation fix — issues #992–#998

Corrected `docs/releases.md` to name the four distinct files that release CI versions in its build checkout: root `Cargo.toml`, `pyproject.toml`, root `Cargo.lock` and `uv.lock`. Kept the separate instruction to update the `hoimin-core` and `hoimin-python` entries in `fuzz/Cargo.lock` when raising the repository minimum.

## Self-review passes

1. **Finding correspondence:** Re-read the Minor finding in `.superpowers/sdd/2026-10-19-issues-992-998/final-review.md`; it identifies only the inaccurate “those five files” sentence and asks for the four files to be stated explicitly.
2. **Implementation path count:** Inspected `tools/release.py:set_version`; it has edits for root `Cargo.toml`, `pyproject.toml`, root `Cargo.lock` (three package entries) and `uv.lock`, giving four distinct paths.
3. **Workflow invocation:** Inspected `.github/workflows/release.yml` around “Set release version in manifests and lockfiles”; the workflow invokes `tools/release.py set-version` in the build checkout.
4. **Fuzz lock preservation:** Re-read the preceding release-guide lines and confirmed the separate `fuzz/Cargo.lock` minimum-version maintenance instruction remains intact.
5. **Scope review:** Reviewed the resulting diff; it changes the single inaccurate sentence, adds no runtime/tooling/version/manifest changes, and does not modify the deferred core/CLI metadata warnings.
6. **Commit scope review:** Confirmed the intended commit contains only `docs/releases.md` and this maintenance record; the detailed report is kept in the SDD report path as requested.

## Validation

- Source comparison: `tools/release.py:set_version` and the release workflow call site confirm the four named release-build files.
- `git diff --check`: passed before commit.
- No tests were added or run; this is a prose-only correction and the task explicitly excludes new tests.
