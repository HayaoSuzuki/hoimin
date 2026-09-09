# Issue #437 implementation plan

## Task 1 — Scoped, non-following current-file reads

1. Run target_handler baseline and record results.
2. Add regression tests before production changes. Demonstrate the old self-link failure and an excluded current-file read failure with real Git; test both scoped TargetHandler and standalone handle_git. Add a process-bounded FIFO plan regression whose cleanup runs even on failure.
3. Implement optional target scope in internal Git resolution, wire TargetHandler, and reuse WorkerRoot reads in blocking work. Use the existing changed-target path index for scope matching before reads, keep public APIs unchanged, and preserve true I/O errors and missing-file behavior. Avoid source comments unless a lint or unsafe justification requires them.
4. Add focused boundary coverage: empty scope, unborn HEAD indexed paths, ordinary untracked content/line counts, special files and linked parents. Check Windows logical equality without silently changing its semantics.
5. Run target_handler, target Git unit tests, Lean changed-target oracle, affected plan E2E, formatting and scoped Clippy. Record RED/GREEN evidence, tradeoffs, and exact commands. Commit implementation/tests/spec/plan/report together.
6. Controller performs full workspace/all-features tests, Rust 1.88 check, full Clippy, independent review, records results, pushes and creates PR closing #437. Keep the worktree.

## Plan self-review 1 — sequencing and failure sensitivity

Tests precede implementation. A hanging FIFO test in an in-process Tokio runtime could hang runtime shutdown even after timeout; use a subprocess with kill/wait cleanup for the hanging regression. Self-link and excluded-path failures provide bounded RED evidence before the FIFO fix.

## Plan self-review 2 — complete interface coverage

The scoped entry affects both unborn-indexed and untracked collection, while handle_git remains unscoped. Testing only plan would miss direct API behavior. Add both APIs and retain the existing Git parsing/property/oracle tests. No workspace mutation or unrelated root-checkout edits are required.

## Plan self-review 3 — validation and scope discipline

Reject timeout inflation and post-read filtering as fixes. Verify that filtering happens before I/O and no-follow regular-file enforcement happens at the read boundary. Run MSRV and Windows-compilation-compatible code paths via existing abstractions; no new dependencies. Final independent review checks the complete branch, including docs. Each shared interface is within Task 1, so no inter-task producer/consumer mismatch exists.
