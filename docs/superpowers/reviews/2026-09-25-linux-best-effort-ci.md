# PR #595 / #596: scope the Linux best-effort CI build

## Failure evidence

Both heads passed Quality, MSRV, Rust, Lean, shuffle, contracts, wheel and boundary checks. Only linux-best-effort failed before executing its test. Run 36080754771 linked the unrelated lean_exception_match_binding_oracle and hoimin binary; run 36080774133 linked lean_multiple_handler_join_oracle. Both report ld terminated with signal 7 (Bus error). The logs do not establish a disk-full or OOM diagnosis.

## Design

The intended test lives in the process_handler integration target. Select it with `--test process_handler` and the complete test name followed by `-- --exact`. This removes unrelated test-binary links from this focused job. Apply the existing CI build configuration: CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=2. Leave the complete workspace test jobs and the actual policy assertions intact.

Design review 1: traced the failure to linking rather than a failed policy assertion; verified the named test's actual integration target.
Design review 2: compared successful workspace jobs' resource settings and confirmed the focused job currently has none; reuse their configuration without changing toolchains.
Design review 3: checked coverage ownership; full workspace tests remain in Rust/contracts/shuffle jobs, while this job must execute exactly the intended policy test. No production behavior or product contract changes.

## Implementation plan

1. Commit this scoped design, plan and review record in each existing PR worktree.
2. Add the four existing CI environment settings to linux-best-effort and select only process_handler with an exact test name.
3. Parse the workflow, run existing workflow-contract tests, and execute the exact Cargo invocation in each branch; require one passing test rather than accepting an empty test filter.
4. Review the implementation and validation three times, commit and push both branches, and check the resulting GitHub CI until completion.

Plan review 1: the narrow command is tied to the verified Rust test location, and the resource settings are job-local.
Plan review 2: native execution of the new invocation proves selection is nonempty; existing workflow tests protect job reachability, toolchain and action contracts.
Plan review 3: both PR heads need the same fix; keep separate commits, avoid rebase/force push, and use fresh Linux CI as the final check. The existing OKF product/audit claims do not change; this follow-up records CI execution scope only.
