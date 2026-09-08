# Issue #433 implementation plan

## Task 1: Replace exhausted-group scans with an active queue

1. Run the existing selection tests as a baseline.
2. Add focused uneven-distribution selection tests covering full output and limited prefixes after groups exhaust; include empty and oversized requests.
3. Establish the performance regression using a temporary counter on the old implementation, then replace the repeated scan in `select_from_tier` with a queue of nonempty file groups. Preserve all ordering and limits.
4. Run focused selection and plan/verify tests, fmt, Clippy, and record the before/after queue-work evidence in the report. Commit implementation, tests, and design/plan/report together.
5. Controller runs independent release measurements, workspace tests, Rust 1.88 check, and independent whole-branch review; address findings, record results, then create a PR closing #433.

Use a separate worktree from origin/main, keep generated audit artifacts ignored, avoid unrelated changes and unnecessary comments. The user has authorized autonomous design, implementation, tests, and PR creation.
