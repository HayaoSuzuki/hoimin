### Finding Verdicts

- **Proof preflight ignored the project-local `why3find.json`** — ADDRESSED. `tools/creusot_environment.py:94-102` requires a pinned installed `project_config`; `tools/creusot_environment.py:160-167` hashes `PROJECT/why3find.json` against it; `tools/creusot_environment.py:200-216` makes `--project-root` mandatory for `check` and reserves `check-installation` for image self-check. `formal/creusot/smoke.sh:13` now checks its actual crate root before proof. Controlled tests at `tests/test_creusot_environment.py:120-156` cover changed `provers`, `time`, `tactics` and a missing selected file; `tests/test_creusot_environment.py:103-119` covers the installation-only split. The fix report records behavioral RED (four project-drift cases returned zero with the old checker), GREEN (38/38), and actual ARM64 overlay checks rejecting changed or missing project configuration while the six ordinary-crate generation/proof pairs passed.

### New Breakage in the Fix Diff

- None. `formal/creusot/Dockerfile:56` and `formal/creusot/configure.sh:28` use the installation-only command before a project exists; `formal/creusot/README.md:42-60` directs proof runners to the project-aware command. Both manifests add `project_config` referring to their existing hashed installed JSON. Reported focused tests (38 passed), Ruff, ty, shell/embedded Python syntax, and local overlay smoke cover the amended path; passing suites were not rerun by this reviewer.

### Out-of-Scope Observations

- None. Native x86_64 final-recipe cold/warm setup-plus-production-proof acceptance remains the assigned Task 4 gate; the reported ARM64 overlay does not assert it.

### Verdict

**Fix round:** All findings addressed, no new Critical/Important breakage.
