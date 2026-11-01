# Python and Rust test structure implementation plan

> **For agentic workers:** Implement the independently assigned Python and Rust domains, then review the combined diff and run the quality gates.

**Goal:** Apply AAA, one case per invocation, parameterization and property-based testing throughout repository-owned Python and Rust tests.

**Architecture:** Preserve production behavior and existing expectations. Use pytest/Hypothesis for Python and rstest/proptest for Rust. Keep lifecycle sequences and aggregate invariants together; split independent input scenarios.

**Spec:** The user's five requirements and `docs/development/guides/contributing.md#test-structure`.

## Constraints and review focus

- Exclude vendor, embedded source fixtures and generated formal corpora.
- Preserve finite categories, boundary values, regression inputs and property shrinking.
- Preserve platform eligibility, resource bounds and subprocess cleanup.
- Separate independent cases without multiplying expensive harness execution for assertions about one run.
- Keep new framework dependencies test-only.

## Tasks

- [x] Update the development guide and its knowledge playbook; add the workspace rstest dependency and refresh the lockfile.
- [x] Audit and refactor Python tests; parent owns `test_ci_workflow.py`, Python worker owns other modules.
- [x] Audit and refactor tests in hoimin-budget-math, hoimin-core and hoimin-python.
- [x] Audit and refactor hoimin-cli tests.
- [x] Review assertion/case preservation and run Python tests, Rust workspace tests, formatting, strict lint and type checks. Record environmental limitations separately.

## Execution notes

The user confirmed that both existing tests and development rules are in scope.
Work proceeds in the supplied checkout; unrelated untracked IDE directories are untouched.
This is a test-structure refactor: baseline and post-change execution establish
preservation; no production mutation campaign is needed.

Baseline for `tests/test_ci_workflow.py`: 179 passed.

## Verification and preservation review

The Python workspace suite passed 2,423 tests with 35 skips on macOS. After
restoring the explicit 59.9-second fuzz-budget regression, its module passed all
19 tests. Ruff formatting/lint and strict ty checks passed.

The final Rust workspace suite with all features passed 10,403 tests with 45
ignored entries across 196 test/doc-test groups. The command exited successfully.
The final Python rerun passed 2,423 tests with 35 skips; its generated corpus
emitted one SyntaxWarning. Native execution was on macOS; Windows/Linux-only
paths received static review rather than native execution.

The final 24-file integration batch passed 652 tests with two ignored entries;
the latest scope-file suite passed 46 tests, and the six contract/operator files
passed 494 tests. Allocation tests with newly
independent workloads passed 43 cases with eight harness threads. The synchronous
source-encoding meter now uses the existing measured-thread allocator, preserving
its original peak bounds while excluding unrelated libtest allocations.

Independent review caught and corrected process-global heap counter races,
Windows side-effect assertions separated from their operations, and explicit
supervisor cleanup separated from its prepared backend. A real integration
failure exposed a database-alias case that inherited its database from an earlier
case; the fixture now initializes its own valid database. Both alias cases pass.

Preserve ordered fresh/resume, retry, cancellation and mutation protocols as one
scenario. Retain comparisons across allocation samples when growth is the
property under test, and keep generated corpus runners exhaustive. These loops
are distinct from independently authored input tables.

Final gates passed: Ruff formatting and lint, strict ty, workspace rustfmt,
workspace all-target/all-feature Clippy with `-D warnings`, and `git diff --check`.
The knowledge playbook's YAML, citations, local links and development-guide hash
were checked; the knowledge inventory reports no missing Markdown/source targets
or unreachable pages. Production implementation behavior and release versions
were unchanged.
