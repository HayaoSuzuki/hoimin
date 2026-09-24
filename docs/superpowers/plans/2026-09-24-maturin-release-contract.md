# Maturin release contract implementation plan

**Goal:** Repair PR #583 on current main without recurring version-fixture failures.
**Architecture:** The release workflow owns the exact maturin version. Its contract
validates a canonical `vMAJOR.MINOR.PATCH` pin and identical versions across both
platform builds, then compares every other field exactly.
**Tech stack:** GitHub Actions YAML, Python unittest and PyYAML.
**Spec:** The bounded design below extends the existing artifact-only release
contract in `tests/test_ci_workflow.py`.

## Design and constraints

Base: `e043613`. Rebase alone leaves #583 failing because the release contract
still duplicates `v1.14.1`. Update both workflow pins to `v1.15.0`; normalize only
`with.maturin-version` on validated `PyO3/maturin-action` steps when asserting the
release contract. Require a nonempty set containing exactly one version. Reject
floating, missing, non-string, prerelease and malformed versions. Keep all other
inputs, action identities, SHA pins, commands, permissions and job shape exact.
Do not alter pyproject's compatible dependency range or the lockfile.

The alternative of simply replacing the expected version fixes only this update;
removing all input comparisons weakens unrelated guards. Neither is selected.
This branch only addresses maturin; #586/#587 receive Renovate-managed rebases.
Close #579/#580/#581 because #585 already put their exact pins on main. Close #583
only once its replacement PR exists. Do not merge any PR automatically.

## Review focus

- A valid future version must pass without editing fixtures.
- A missing/floating/non-string/noncanonical version must fail.
- Different Windows/Linux versions must fail.
- An extra or renamed input and altered build arguments must still fail.
- Wrong actions and publishing paths must remain rejected.

## Task 1: Release pin validation and update

Files: `.github/workflows/release.yml`, `tests/test_ci_workflow.py`,
`docs/development.md`, existing Rust reproducibility spec and its OKF source entry.
Interface: retain `assert_artifact_only_release(test, workflow)`; its version
normalization is release-specific, leaving `workflow_contract` unchanged.

- [x] Baseline: `python -m unittest discover -s tests -p test_ci_workflow.py`.
- [x] Change both workflow versions, add acceptance/invalid/mismatch/input tests,
  and run the workflow suite. Expect release comparisons against the old pin to
  fail while invalid values and mismatched build inputs remain rejected.
- [x] In the release assertion, inspect matching maturin steps, validate with
  `v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)`, collect versions,
  normalize that one value to a fixture marker, and require one shared version.
- [x] Run all Python tests; verify workflow diff changes only two versions.
  Build a wheel using maturin 1.15.0 and run the existing smoke script locally.
  Record the platform; GitHub-hosted Windows/Linux release jobs are not local tests.
- [x] Update documentation/source metadata, perform three implementation/test
  self-reviews and one independent review, commit and open a replacement PR.

## Design self-reviews

1. Root cause: #583 CI log fails on `v1.15.0 != v1.14.1`; current main retains the
   old fixture, unlike the Action SHA repair already merged in #585.
2. Boundary: preserve complete release shape; normalize one known Action input
   only after syntax validation. Do not normalize arbitrary version-looking text.
3. Consistency: verify both platform builds use the same full version; unchanged
   expected jobs/steps reject added or removed builds and retain publication guards.

## Plan self-reviews

1. Reproduction: test main baseline, then the actual proposed workflow version
   before implementing the assertion change; this separates existing failures.
2. Negative coverage: include missing keys, non-string values, trailing newlines,
   leading zeroes, inconsistent pins and an unrelated extra input.
3. Evidence: full Python coverage plus an actual pinned-maturin build/smoke test;
   no production Python changes, so hoimin mutation targeting is inapplicable.


## Implementation self-reviews

1. Inspected the workflow diff: exactly two maturin version substitutions; no
   changes to build arguments, actions, permissions, platform targets or locks.
2. Inspected normalization: restricted to the release assertion and known maturin
   steps. Type/format validation precedes set insertion and normalization; missing
   inputs fail, and existing structural equality rejects added/removed steps.
3. Verified shared version enforcement and preservation of all other input keys.
   Canonical future versions pass without updating expected fixture constants.

## Test self-reviews

1. Baseline: all 36 workflow tests passed on latest main. After updating the
   workflow and adding three tests, 39 tests produced six expected failing
   assertions against the old version. An initial test fixture job-name typo was
   corrected before that clean reproduction; there were no errors in the final
   RED run. After repair, all 103 Python tests passed (Python 3.14.7).
2. Inspected rejection cases for malformed/missing/non-string values, differing
   platform versions, changed args and extra inputs. Existing identity, SHA and
   disguised publication tests remain passing. A fresh reviewer ran 39 workflow
   tests plus additional boundary and hostile-input probes, with no findings.
3. Built a real macOS ARM64 wheel with `uvx --from maturin==1.15.0 maturin build
   --release --locked --compatibility pypi --no-default-features` (exit 0), then ran
   `python tests/wheel_smoke.py` against it (exit 0). This is local build/install/
   command evidence; hosted Windows/Linux release execution remains untested.

## Documentation and completion evidence

Updated the development guide and existing reproducibility design amendment;
refreshed only its source entry in the OKF catalog. Parsed 25 concept headers and
four reserved indexes, checked catalog source IDs/footnotes/local paths and the
amended source hash. `git diff --check` passed. Independent code review had no
findings; parent verification covers documentation and local wheel compatibility.

The design and plan were committed before implementation in `649a807`.

## Renovate PR triage

- #579/#580/#581: closed after verifying their exact 37 pins already exist on main
  via merged #585 (`5105a23`). No branch deletion or version suppression config
  was added.
- #586: requested Renovate rebase via its checkbox; new head `c4f7d40` includes
  current main `e043613`. Its only diff is one cache SHA/version comment. All
  100 Python tests passed, and parsed workflow behavior is identical to main.
- #587: same rebase procedure; new head `3d1a3ac` includes `e043613`. All 23
  checkout references across five workflows are updated. All 100 Python tests
  passed; parsed workflow behavior is identical to main. Both PRs remain open
  for review with refreshed GitHub CI.
- #583: replaced by this independent branch because main still needed the
  maturin-specific contract correction; original bot branch remains untouched.
