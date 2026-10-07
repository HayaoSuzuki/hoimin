# Issue 710 implementation plan

1. Add a public CLI test showing line-diverse is rejected before implementation; preserve this RED evidence.
2. Extend the CLI/report policy enums and current schemas, parameterize the borrowing iterator's grouping, and route both preview and execution through the existing resolver.
3. Add exact dense-line, cross-file, tier, multi-line, exhausted-group and paging tests. Add a Lean-generated public CLI oracle and explicit broken-rule controls.
4. Compare strict/file/line policies with one equal candidate budget using authored checks; record row reach, execution time and new survivor locations without overstating real defect detection.
5. Update README/OKF, perform five implementation and test self-reviews plus independent review, run appropriate/full Rust tests, fmt/CI clippy, Lean/corpus freshness, commit, create stacked PR and cargo clean.

## Plan self-reviews

1. Contract coverage: every Issue acceptance edge maps to unit, public CLI, oracle or trial evidence.
2. Red/green: start through the existing public CLI, so unknown-policy rejection is the expected pre-change failure rather than a missing enum compile error.
3. Shared path: one resolver feeds dry-run and execution; tests compare both and versioned report metadata.
4. Evidence boundaries: actual Lean-generated expectations, no hand-edited corpus; no broad defect detection claim from authored fixtures.
5. Work isolation: #709 target was cleaned before this worktree; Lean work is separate from Rust files and no concurrent cargo builds are delegated.
