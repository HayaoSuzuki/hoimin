# Issue 453 scoped discovery implementation plan

**Goal:** Avoid traversing and retaining unrelated subtrees for exact file/line selections.

**Architecture:** Build a normalized exact-file scope, retain its ancestor directories in the existing root walk, and share the same scope between normal and include-restoration walks.

### Task 1: Establish deterministic regressions

1. Add test-only visit and collected-record counters around the production walk.
2. Create one requested file and 1,000 unrelated nested files; require one collected record and a bounded number of root-level visits.
3. Add a multiple-line-path fixture that pins retained output.

### Task 2: Add the scope filter

1. Normalize and deduplicate exact files and their ancestors using core platform path equality.
2. Combine scope pruning with built-in exclusions on both normal and restored walks.
3. Fall back to full discovery for source, symbol, empty, or internally invalid exact selections.

### Task 3: Verify selector and diagnostic behavior

1. Run existing target-handler coverage for combinations, ignore/include/exclude, missing/non-Python, symlink, portable paths, and case rules.
2. Add exact include restoration coverage.
3. Explicitly pin that broad discovery diagnoses unrelated malformed paths while exact discovery ignores them.

### Task 4: Measure and document

1. Record release five-sample medians for 2k/4k/8k unrelated files, asserting identical discovered output.
2. Update the selection OKF contract and design/audit indexes with exact source hashes.
3. Record three concrete reviews for OKF, design, plan, implementation, tests, and PR.

### Task 5: Verify and publish

Run formatting, clippy, CLI/contracts, workspace, Python, release-wheel smoke, OKF, and diff gates with two Cargo jobs; commit only issue #453 files and open a PR to `main`.
