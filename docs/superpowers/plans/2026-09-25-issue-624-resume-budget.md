# Issue 624 implementation plan

Use this worktree as the second related resume stack level above #622; after the parent merges, rebase onto main before publication. Commit design and plan before production code.

1. Add a public first(1)/same(1)/increase(2) regression for killed and survived, checking run ID, executed count, reused termination and current normalized limit. Observe RED on increased budget. Add decreased/completed/incompatible controls and a fresh-run comparison.
2. Add the nonzero budget to session effects, remove only max-mutants from fingerprint encoding, bump schema, and update existing effect fixtures and fingerprint compatibility tests.
3. Migrate database to version 4 with nullable legacy budget and constrained fixed-width positive u64 for new rows. Store on begin; select latest eligible incomplete run and atomically recheck/reserve budget after ownership on load. Exercise unsigned boundaries, repeated load, conflict, completed/decreased/older-compatible cases and migration preservation.
4. Integrate a bounded Lean budget-resume model/generator/corpus and Rust implementation adapter, retaining existing session/candidate accounting oracles. Acquire the global Lean slot and retain 20s/2GiB/10k-heartbeat limits.
5. Review design/plan/implementation/tests at least three times each and get independent review. Run focused and full workspace suites, exact CI Clippy/fmt, workflow registry and bounded Lean freshness/sensitivity. Record evidence, publish with gh-stack, merge only after CI, remove merged worktree.

## Plan review passes

1. Public RED uses existing CLI flags and actual persistent incomplete runs; it fails on unwanted new run/reexecution rather than a newly missing API. Cover both reusable verdicts and ensure baseline still executes.
2. Guard decreases and completed sessions at database selection/recheck, not only in a helper. Existing ownership and result monotonicity tests must stay green. Positive budget and unsigned-boundary tests validate persistence independently of report output.
3. Separate digest equality from run eligibility in tests and Lean. Include old-schema behavior and other-limit changes, then compare public increased resume with fresh execution. No Python production changes are planned, so Rust coverage and Lean correspondence are appropriate rather than Python mutation testing.
