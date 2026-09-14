# Issue 490 boundary contract implementation plan

Spec: ../specs/2026-09-14-issue-490-boundary-contracts-design.md.
Base: enhancement/issue-467 (#538), fast-forwarded before changes.

- [x] Extend existing Lean progress-input adapter with three encodings,
  independent Lean expectations and explicit projection-applicability reports.
- [x] Add literal selector/overflow fixture and preparation-stage precedence
  tests; register existing real session, backend, metrics and fingerprint tests.
- [x] Add registry runner and unit tests for exact-test execution, skipped
  native cases, mismatch/infrastructure labels and report-only preservation.
- [x] Execute Lean model/sensitivity/freshness when the parent releases the
  shared Lean window; reuse checked-in corpus without changing expectations.
- [x] Add CI ordering, full worksheet, report artifacts and minimal witness;
  update OKF concept and source indexes with final hashes.
- [x] Run focused checks, perform three concrete self-review passes per stage,
  commit/push and create a PR based on enhancement/issue-467.

Plan reviews: preserve existing v2/v3 adapters as the baseline; do not generate
expected outcomes from production helpers; ensure each unavailable path has a
reason and each claimed execution has a captured exact-test result. Bounds use
existing fixed fixtures, no enlarged Lean state space or duplicated native backend.

Publication is the remaining operation after the verified worktree review;
the PR URL and commit are supplied in the handoff. Each preceding stage has
three concrete review passes in the issue-490 report.
