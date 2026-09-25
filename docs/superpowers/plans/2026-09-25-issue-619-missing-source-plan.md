# Issue 619 implementation plan

1. Commit this plan and reviewed design before production edits.
2. Add public CLI regression tests for missing, mixed, changed-clean, plan/run, and saved-plan re-resolution; prove baseline marker absent on run/verify errors. Observe failures against old implementation.
3. Add the shared source validation and README contract. Keep regular-file source and legitimate empty selections successful.
4. Test lexical relative/root-equal/absolute normalization and dangling symlink behavior with filesystem-backed TargetHandler cases; exercise permission/error classification where portable.
5. Review implementation and tests in three distinct passes: call-path/order, compatibility and filesystem boundaries, test discrimination and failure messages. Record outcomes and independent review.
6. Run related target/plan/public tests, exact formatting and both CI Clippy commands, then full workspace tests. Integrate current main before publication if necessary. Publish with gh stack, monitor CI at least five minutes apart, merge and remove the clean worktree after all checks succeed.

## Plan self-reviews

1. Requirements mapping: all Issue acceptance conditions have explicit tests; candidate count cannot substitute for the missing-path assertion. Add a mixed valid/missing case even though valid candidates exist.
2. Test independence: subprocess output/exit status and an external baseline marker demonstrate public behavior; existing-empty controls detect accidentally rejecting all zero-target selections. Saved-plan generation precedes deleting its source.
3. Execution review: shared Cargo lane avoids per-worktree build duplication; maintain meaningful errors and no schema changes. Lean is intentionally omitted because no state-machine decision rule changes. Design and plan commit must precede production edits; CI remains asynchronous.
