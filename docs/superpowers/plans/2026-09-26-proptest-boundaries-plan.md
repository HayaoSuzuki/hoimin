# Proptest boundary implementation plan

1. Commit design and plan before test changes; use one isolated worktree and the existing shared Cargo lane.
2. Run affected baseline tests. Add encoding integration properties and private line/stream unit properties, with bounded constructive strategies and independent observations.
3. Demonstrate actual failure and shrinking under temporary production defects (encoding boundary, newline conversion, stream comparison). Restore all production sources and remove only artificial sensitivity seeds; rerun green with explicit seeds and a larger case count.
4. Document reproduction and failure persistence. Run full cargo test --workspace, exact CI Clippy commands, formatting and workflow tests. Record three implementation and test reviews with observed results; commit tests/docs and create a PR with gh stack.

## Plan self-reviews

1. Ordering: inspect existing coverage first, commit assumptions, measure baseline, add tests, then prove fault sensitivity before final verification. No intentional production behavior change is planned.
2. Resource use: keep one worktree and reuse target/batch-resume after cleaning only local crates. Limit generated sizes and shrinking; no new scheduled CI job or repeated network polling.
3. Completion: preserve default failure persistence, document deterministic replay and larger runs, inspect the final diff for temporary faults/seeds, and report actual checks. Prior merge/cleanup and graceful polling preferences continue to apply.
