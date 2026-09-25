# Issue 607 implementation plan

Use the existing isolated `feat/issue-607-stale-plan-details` worktree, based on main `efe35318`. The user authorized design, implementation, tests, commits and publication; no additional design approval is pending.

1. Commit this plan and the reviewed design before changing code or tests.
2. Add comparison regressions for both record kinds: all change kinds, unchanged/reordered records, stable mixed ordering, empty sides, escaped paths, and exactly ten versus more than ten differences. Add public CLI regressions for source/fingerprint times modified/added/removed, normal/dry-run equality, status 2, empty stdout and an absent test marker. Verify earlier exact fingerprint resolution and source-target errors remain distinguishable and source mismatch retains priority.
3. Run the new tests against the old implementation and record expected failures. Clean only local core/CLI package artifacts in the assigned cache before switching worktrees.
4. Bind the existing record maps once, retain equality's early return, derive classified differences from those maps and format bounded details for both existing error variants. Document the ordering, cap and quoting in README. No new file reads or hash calculations.
5. Run focused GREEN tests; conduct three separate implementation and test review passes. Run workspace tests, CI workspace/all-target/all-feature clippy, vendored parser clippy and both format checks. Record actual evidence and limitations.
6. Obtain independent review through the coordinating agent, address any findings, commit the final implementation and evidence, then publish through `gh stack`. The coordinator owns CI polling, merging and cleanup.

## Plan self-review

1. Mapped all six requested stale cases to public subprocess tests and reserved pure record tests for ordering, cap, escaping and equality. Tests assert literal outcomes, not formatter-generated expectations.
2. Checked fixtures for earlier failure traps: glob removal keeps another selected source/fingerprint file so comparison is reached. Separate exact-file removal deliberately exercises the existing resolution error instead. Marker lives outside the project to distinguish test execution from setup.
3. Checked verification cost and scope: reuse the lane's cache with one build job, clean only core/CLI artifacts, and use bounded subprocess deadlines. Rust tests and documentation changes need no Python mutation run or Lean slot. Publication waits for completed checks and independent review.
