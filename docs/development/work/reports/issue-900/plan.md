# Implementation plan

For maintainers reviewing issue #900. Implement the approved design in the isolated issue worktree.

1. Run the existing disk policy and guard oracle baseline.
2. Add failing tests for legacy wire rejection, typed round trips, and variant-derived evidence/code contracts. Use proptest for arbitrary messages and observations; use tables for the four finite variants.
3. Replace failure fields, centralize evidence/report conversion, and migrate callers without compatibility adapters.
4. Verify policy, lifecycle, shell report and cleanup behavior, both disk oracles, full Rust workspace, Python corpus, and documentation checks.
5. Record five implementation and verification reviews, commit, and open a PR without merging.

## Plan review passes

1. TDD sequencing: found caller migration could mask the required red evidence; add an old-wire rejection test before production changes.
2. Test scope: found enum mapping alone misses sticky evidence; retain existing lifecycle tests and run monitor/shell suites.
3. Property applicability: found arbitrary duration/message equality matters; use generated typed failure round trips and exact evidence checks rather than random finite variant selection alone.
4. Build isolation: found shared target directories could interfere with other work; use the dedicated issue-900 target and one Cargo job.
5. Verification coverage: found changing failure serialization affects saved events while reports are projections; verify wire rejection separately and keep report compatibility assertions.
