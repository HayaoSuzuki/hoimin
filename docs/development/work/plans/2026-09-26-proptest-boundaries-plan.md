# Proptest boundary implementation plan

1. Commit design and plan before test changes; use one isolated worktree and the existing shared Cargo lane.
2. Run affected baseline tests. Add encoding integration properties and private line/stream unit properties, with bounded constructive strategies and independent observations.
3. Demonstrate actual failure and shrinking under temporary production defects (encoding boundary, newline conversion, stream comparison). Restore all production sources and remove only artificial sensitivity seeds; rerun green with explicit seeds and a larger case count.
4. Document reproduction and failure persistence. Run full cargo test --workspace, exact CI Clippy commands, formatting and workflow tests. Record three implementation and test reviews with observed results; commit tests/docs and create a PR with gh stack.

## Plan self-reviews

1. Ordering: inspect existing coverage first, commit assumptions, measure baseline, add tests, then prove fault sensitivity before final verification. No intentional production behavior change is planned.
2. Resource use: keep one worktree and reuse target/batch-resume after cleaning only local crates. Limit generated sizes and shrinking; no new scheduled CI job or repeated network polling.
3. Completion: preserve default failure persistence, document deterministic replay and larger runs, inspect the final diff for temporary faults/seeds, and report actual checks. Prior merge/cleanup and graceful polling preferences continue to apply.

## CI follow-up plan and reviews

1. Preserve the CI log/seed, reproduce the inherited-descriptor mechanism with a controlled fork, and verify the release/reap control. Those probes are temporary and must be removed before normal tests.
2. Isolate only the failing ownership test via bounded re-execution of its exact libtest name. Review process ownership, child-only environment, failure diagnostics and unchanged assertions.
3. Verify the original shuffle seed, repeated isolated invocations and static checks; update evidence and source hashes, push the existing PR without rebasing, then monitor CI at intervals of at least five minutes. No unrelated fuzzing work is included.
