# Worker-owned state implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

Goal: eliminate independently synchronized Worker phase/candidate/result fields.
Architecture: one data-carrying enum inside core machine, stacked on #895.

- [x] Confirm #895 base, clean own inactive target, use jobs1/testthreads2/debug0/incremental0.
- [x] Record baseline core state-machine and focused lifecycle tests; inspect existing adversarial proptests for ownership coverage.
- [x] Inspect coverage and reuse the existing adversarial properties for this private representation refactor; record compiler-driven migration without inventing behavioral RED evidence.
- [x] Replace WorkerPhase/WorkerState fields with enum variants, including Ready and unit Resetting.
- [x] Migrate scheduling, persistence, output acknowledgement and stop recovery using complete value replacement.
- [x] Remove obsolete consistency checks; preserve bounded-worker/effect checks.
- [x] Update ownership knowledge, spec/plan/review records and source catalogs, refreshing source hashes after final edits.
- [x] Run core machine/property tests, relevant contracts, workspace tests, all-feature Clippy, fmt, docs and Python checks as affected. Record five implementation and verification reviews.
- [ ] Commit the branch, publish using gh-only Git Data API, create/attach PR targeting #895, and watch CI with spaced checks.

## Five plan self-reviews

1. Every completion criterion maps to ownership representation, transitions, recovery or verification tasks.
2. Genuine dependency is #895; stack the PR instead of mixing outcome and worker redesign in one branch.
3. Stop accounting and output acknowledgement are checked independently, with existing generated traces exercising combinations.
4. No compatibility aliases or duplicate legacy enum/fields survive; compiler errors enumerate callers during migration.
5. Resource and verification work are bounded: isolated target, no concurrent clean, checks reflect source changes and CI handles platform-specific coverage.

Final verified dependency is #895 commit276d84284a69d0cb4c99e44c47e5889c586e3e7f. Publication/CI is an operational follow-up after this local verification record; its checkbox is not prematurely marked complete.
