# Base-order research implementation plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a reproducible base-order mutation to the inheritance research PR.
**Architecture:** Extend the existing AST prototype and comparison runner; retain
historical receipts and save a separate base-order result file.
**Tech stack:** Python 3.14, standard library, pytest, existing hoimin CLI.
**Spec:** ../specs/2026-10-29-base-order-research-design.md

## Global constraints

Research only; no product IDs or defaults change. One candidate per example.
Seeded shuffle with first-two fallback; no uniformity claim. Historical receipts
remain unchanged. Preserve weak/strong/error distinctions and execution caps.

## Review focus

Identity shuffle must change order; cover with a known identity-producing seed.
Three-plus bases retain every base once; cover multiple counts and seeds.
Replaying a seed reproduces the edit; check descriptors, not elapsed times.
MRO failure must remain an error, not a useful lead; execute a risk fixture.
Explicit base calls can hide reorderings; include a surviving control.

## Task 1: Prototype, fixtures, and evidence

- [x] Write tests for two-base swap, seeded multi-base permutations, identity
  fallback, reproducibility, and the fixture verdicts in tests/test_inheritance_research.py.
- [x] Run the focused tests and observe the expected missing-feature failure.
- [x] Extend proposed(source, operator, seed=0), compare(..., seed=0), and CLI
  --seed/--base-order-only in docs/superpowers/reports/issue-1123/experiment.py.
- [x] Add positive cooperative callback, three-base callback, equivalent,
  explicit-call, and inconsistent-MRO examples with independent contracts.
- [x] Run focused tests, the bounded experiment, and relevant existing tests.
- [x] Update the report and knowledge source hashes; record actual environment,
  limitations, counts, and review. Review the complete diff and commit.

## Execution ledger

Execute inline: this is one small extension to an existing experiment. The user
has approved proceeding; no further implementation-method approval is needed.
Use an isolated app-managed worktree based on origin/research/issue-1123.

Ruling: fix the shared helper's two source-write sites to preserve LF on Windows;
otherwise CLI byte offsets disagree with fixture text. A failing regression test
demonstrated the issue. Historical result files remain unchanged.

Verification: focused 16 passed; full Python suite 1258 passed/50 skipped; both
new interpreter receipts agree (134 mutant checks each); default 15-example run
executes 290 mutant checks, and original-nine descriptors/verdicts match history.
Independent review found no actionable issues. No product or Rust changes.
