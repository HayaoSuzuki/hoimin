# Seeded sampling implementation plan

> Use superpowers:executing-plans inline; preserve the current branch as requested.

**Goal:** complete issue 695 with reproducible, auditable verification samples.
**Architecture:** isolated index sampler, CLI/resolver integration, optional report
metadata, existing ordered scheduler. **Tech stack:** Rust, proptest, libFuzzer, Lean.
**Spec:** [design.md](design.md).

## Global constraints

Explicit positive sample count and u64 seed; no new runtime dependencies; reject
truncated/empty populations and insufficient execution budgets; preserve validation
and existing selection modes. Commit documentation on the current branch.

## Review focus

Conflicting flags; extreme seeds/counts; modified and truncated manifests; failures
after selection but before completion; candidate-set changes in progress.

## Task 1: selection and CLI

- [x] Add public CLI tests first and observe rejection of the missing sample flag.
- [x] Add `plan/sampling.rs`: sample_indices(population, count, seed) -> Vec<usize>.
      Pin vectors, membership, cardinality, uniqueness, deterministic prefixes and
      rejection boundaries in unit/property tests.
- [x] Wire Sample into cli.rs, plan.rs, preview.rs, core/report.rs and human output.
      Reuse ordered candidate dispatch; preserve limits; update current schemas.
- [x] Run sampling integration tests and existing plan/report/progress tests.

## Task 2: independent verification

- [x] Build a bounded Lean model/corpus before Rust oracle assertions. Prove model
      bounds and shuffle permutation properties; retain broken-variant witnesses.
- [x] Compare generated index orders and rejection decisions with real CLI output;
      check corpus freshness in CI. Add a bounded libFuzzer target using real sampler.
- [x] Exercise timeout, baseline failure, tampering, budget, dry-run side effects,
      JSON/JSONL/human reports and progress candidate changes.
- [x] Run fixed-seed bias inspection and multiple authored Python project trials
      against full-population executions, recording time, score error and unknowns.

## Task 3: review, documentation and commit

- [x] Document usage, reproducibility, limits and empirical scope in README/report.
- [x] Perform five implementation and five test self-review passes; fix findings.
- [x] Run format, clippy, workspace tests, Lean/corpus, fuzz and schema checks.
- [x] Commit only task files; record commands, evidence and any remaining limits.

## Five plan reviews

1. Coverage: every acceptance criterion has a CLI/resolver/report or progress test.
2. Interfaces: sampler returns indices; resolver maps once to owned IDs; metadata
   carries the same ordered IDs supplied to the scheduler.
3. Independent evidence: generated Lean corpus supplies expectations, and fuzz
   and properties check invariants without copying the sampler into assertions.
4. Resources: bounded fuzz domains and time limits; guarded Lean commands; do not
   run expensive formal exploration concurrently. No new worktree per user request.
5. Deliverables: assessment, design, plan, reviews, reproducible evidence and code
   all belong to this branch. User already requested progression through all stages.
