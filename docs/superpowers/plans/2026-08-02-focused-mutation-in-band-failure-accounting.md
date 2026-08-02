# Focused Mutation In-Band Failure Accounting Plan

1. Extend the focused-mutation workflow fixture so tests can supply cargo-mutants version output.
2. Add failing regression tests for the three in-band terminal paths and assert candidate `not_run` reasons in memory and in `run.json`.
3. Add a centralized pre-mutation failure helper and route all three paths through it.
4. Run focused tests, the full Python suite, compile checks, mutation testing for changed Python behavior, and diff validation.
5. Request independent review, create the PR, wait for all CI jobs, and merge only after success.
