# Base-order operator implementation plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship default-on `base_order_shuffle` with individual opt-out.
**Architecture:** A small pure source-edit module called from class-statement
collection, registered in core configuration and CLI ranking. No runtime MRO
index or new dependencies.
**Tech stack:** Rust, Ruff AST, existing BLAKE3 and CPython integration tests.
**Spec:** ../specs/2026-10-29-base-order-operator-design.md

## Global constraints

One changed permutation per eligible class; deterministic source-derived shuffle;
preserve trivia and membership; no target import; unchanged historical receipts.
102 IDs, 83 defaults; `--exclude-operators base_order_shuffle` must suppress it.

## Review focus

- Unicode normalization duplicates must be excluded.
- Multiline/comment/grouped bases and CR/LF variants must compile after mutation.
- Class symbol/line selectors must select the edit, with stable candidate identity.
- Excluded classes must not build replacement payloads; cancellation/caps must work.
- Saved-plan verify must execute the exact saved edit and preserve original bytes.

## Task 1: Analyzer and registration

- [x] Add failing analyzer tests for lookup/default selection, swaps/permutations,
  preserved trivia, unsupported shapes, selectors, caps and cancellation.
- [x] Run targeted tests and confirm the missing operator failure.
- [x] Add the source-edit helper and integrate class collection; register the ID
  at the enum/default tails and in behavioral ranking. Update fixed registries.
- [x] Run analyzer/core tests and formatting/lint checks.

## Task 2: Public CLI, documentation and delivery

- [x] Add CLI tests for default-on/opt-out, actual CPython execution, deterministic
  plans, saved-plan verify weak survival/strong failure and source restoration.
- [x] Update catalog fixture, counts/help/usage and implementation evidence.
- [x] Run relevant and workspace checks within resource limits; review the diff
  independently, fix material findings, commit and update PR #1135.

## Execution ledger

The user explicitly requested implementation with opt-out; execute inline in
the existing isolated worktree and update the existing PR. No new approval loop.
Design choice: syntactic simple-name bases with stable source-derived shuffle;
runtime MRO validation would require a separate semantic-analysis feature and
does not belong in this source-edit operator.

Verification: initial five analyzer tests failed on the absent operator, then all
six passed. Five public CLI tests pass with CPython 3.14.9 and 3.15.1. The full
all-feature workspace run reports 3,878 passed / 32 ignored across its top-level
summaries and exits zero. Workspace/all-target/all-feature and vendor Clippy,
both formatting checks and generated-reference verification pass. Related Python
tests report 61 passed / 1 skipped. A read-only independent review found no
actionable issue. Clippy fixes affect only new-test diagnostics/string construction.
