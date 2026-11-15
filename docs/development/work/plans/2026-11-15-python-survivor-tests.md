# Python mutation survivor tests implementation plan

> **For agentic workers:** Use superpowers:subagent-driven-development for independent test domains. Keep runtime verification serial.

**Goal:** Protect meaningful Python behavior identified by the survivor audit and submit the tests in a pull request.

**Architecture:** Extend existing pytest fixtures and test modules at repository-owned boundaries. Keep production code unchanged; use independent expected values, bounded witnesses and precise external-command doubles.

**Tech stack:** Python 3.14, pytest, pytest-mock, Hypothesis, hoimin 0.14.63 through `uv tool run`.

**Spec:** The repository's test-structure requirements in `docs/development/guides/contributing.md` and the supported tool behaviors listed below, inspected at base `7f9a42ce7d4064bbab56005f59ada29e4573dd02`.

## Global constraints

- One independent case per test invocation, AAA, parameterize repeated finite cases and use bounded properties where appropriate.
- No scenario-selecting `if` or independent-case `for` in test bodies.
- Do not add tests solely to kill equivalent mutations or verify standard-library/third-party mechanics.
- Do not mutate or build Rust. Mutation execution uses one worker, workspace cap 8GiB and free-space reserve 10GiB, with dedicated temporary directories removed on every exit.
- Preserve source selectors, profile, limits and native test argv within each saved plan. Source/config changes require a new plan.
- Static audit classifications are hypotheses; record reclassification if a meaningful supported input cannot be demonstrated.

## Review focus

- Command doubles must reject invalid repository-generated argv instead of manufacturing success.
- Missing first metrics, single invalid CI outputs and selective push diffs need independent expected outcomes.
- SBOM producers and validators must not share the only oracle for target/environment identity.
- Build/smoke orchestration must actually run its downstream validation.
- Cyclic graph checks must terminate with bounded work even under the selected mutant.

## Task 1: Distribution and provenance

**Files:** `tests/test_sbom.py`, `tests/test_pypi_provenance.py`, `tests/test_pypi_release.py`, `tests/test_archive_smoke.py`, `tests/test_wheel_smoke.py`.

- [x] Cover the 19 audited behavioral gaps with precise command contracts, stale-sidecar rejection, nested output creation, independent SBOM identity expectations, complete runtime observations and smoke delegation.
- [x] Run relevant pytest modules and Ruff/ty; main owns execution.
- [x] Reverify selected matching candidate IDs against updated tests; commit after review.

## Task 2: Execution and resource support

**Files:** `tests/test_ci_fuzz.py`, `tests/test_creusot_environment.py`, `tests/test_performance_shapes.py`, `tests/test_verify_creusot.py`, `tests/test_lean_resource_guard.py`.

- [x] Cover 22 audited behavioral gaps: valid bounded command construction, required arguments, corpus directories, malformed manifests, fixture metadata, empty ancillary obligations, complete resource observations and bounded graph/deadline behavior.
- [x] Keep test helpers deterministic and resource bounded; run relevant pytest and Ruff/ty centrally.
- [x] Reverify matching candidate IDs, review and commit.

## Task 3: CI, documentation and corpus contracts

**Files:** `tests/test_boundary_contracts.py`, `tests/test_ci_metrics.py`, `tests/test_ci_selection.py`, `tests/test_docs_site.py`, `tests/test_hypothesmith_corpus.py`, `tests/test_knowledge_references.py`, `tests/test_workflow_lint.py`.

- [x] Cover 24 audited behavioral gaps and strengthen the cyclic footnote test with bounded expansion.
- [x] Verify independent malformed input cases, schema/count outputs, selective event changes, emitted documentation links, and corpus template/profile behavior.
- [x] Run matching pytest modules, reverify original IDs, review and commit.

## Task 4: Integration and PR

- [x] Run the full Python suite, formatting/lint/type checks and AST inspection of changed test bodies.
- [x] Perform a fresh independent review of all test changes and resolve findings.
- [x] Document the distinction between selected consumer tests, full-suite witnesses and static judgments; do not synthesize a whole-project mutation score.
- [x] Confirm mutation workspace cleanup and prepare the reviewed change and PR description using the repository template.

## Rulings

- Work in the clean existing checkout on a dedicated branch to reuse the existing environment and avoid duplicate caches. No worktree or Rust build is necessary for these test-only changes.
- Prior mutation observations at the unchanged base provide the before evidence; new tests must pass original code and kill the same candidate IDs. Never change production behavior just to improve the score.
- Follow the repository's implementation-plan location under `docs/development/work/plans/`; no new OKF runtime contract is introduced by tests alone.

## Verification result

On macOS with Python 3.14.10, the full Python suite passed 2,714 tests and skipped 35. Ruff formatting/lint and ty passed. The selected 65 behavioral survivors and one bounded cyclic-input candidate were killed with hoimin 0.14.63 across 19 source-file batches; nine candidates in two batches were repeated after case refinements. This is selected-candidate evidence, not a whole-project mutation score. Runtime sources remained unchanged. No Rust mutation test or local Rust build was run.
