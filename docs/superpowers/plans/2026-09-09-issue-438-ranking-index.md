# Issue #438 implementation plan

## Task 1 — Index resolved symbol selection

1. Run existing ranking and plan tests as baseline.
2. Add behavioral coverage for no-symbol targets with both Some/None candidate symbols, multiple files, matching/nonmatching names, repeated paths with disjoint/duplicate symbols, path equality boundaries, and reason/rank ordering. Verify old code still produces the required output; performance is the defect, not these outputs.
3. Record old performance/linear-work regression evidence before changing code. Implement a local borrowed path-to-symbol-set index, preserving all ranking rules and public APIs. Avoid unnecessary source comments.
4. Run focused ranking, plan-manifest/verify tests, fmt and scoped Clippy. Record measurement method and behavioral compatibility in a tracked report. Commit source/tests/spec/plan/report.
5. Controller compares before/after release ranking for no candidate symbol, candidate symbols without targets, and matching symbol targets; verifies full serialized output equivalence. Run full workspace/all-features tests, Rust1.88, full Clippy, and independent task/final review. Record results and create a PR closing #438.

## Plan self-review 1 — meaningful evidence

Behavioral tests passing before the change do not demonstrate performance regression. Preserve before-source artifact and run the same ranking workload before/after, including candidates with Some(symbol). Do not add a failing timing threshold to CI or tests that merely inspect implementation text.

## Plan self-review 2 — compatibility coverage

Tests must include repeated paths so map insertion cannot accidentally discard symbols. Include selection flags independent of resolved targets, and existing combined reason tests. Run plan and verify paths because both rank_candidates and validate_ranking_against use the new index.

## Plan self-review 3 — execution and review gates

One bounded implementation task owns ranking.rs and its tests; it leaves selection policy and manifests unchanged. Whole-workspace validation follows focused checks; final independent review inspects docs and performance claims as well as code. Keep benchmark setup and cloning outside timing, and label synthetic measurements honestly. Issue-specific worktree and tracked documents are required before execution.
