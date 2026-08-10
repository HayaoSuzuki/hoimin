# Lean Schema Migration Concurrency Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Audit concurrent SQLite schema configuration with unbounded Lean invariants, a small bounded schedule audit, broken-model witnesses, and public Rust correspondence.

**Architecture:** Model the migration transaction as a two-actor state machine with one committed database and one staged transaction. Keep proofs in imported modules, bounded exploration and JSON generation in a standalone executable, and implementation correspondence in a public Rust integration test.

**Tech Stack:** Lean 4.32.2, Lake, Rust 1.88+, rusqlite, serde_json

## Global Constraints

- Run at most one potentially expensive Lean process at a time.
- Use `maxHeartbeats 100000` and a 20-second external deadline; never use unlimited heartbeats.
- Bound the explorer to two actors and depth 9 with a checked state ceiling; do not raise bounds after resource symptoms.
- Keep exact forced stale reads `model-only`; do not expose private production seams.
- Preserve unrelated user files and the root checkout.

---

### Task 1: Define and prove the migration state machine

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/SchemaMigrationModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/SchemaMigrationProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-schema-proof-consumer.lean` (ephemeral)

**Interfaces:**
- Produces: `Version`, `Actor`, `Phase`, `Event`, `State`, `initial`, `step`, `run`, and `Invariant`
- Produces: `step_preserves`, `run_preserves`, `successful_actor_has_current_schema`, `run_preserves_legacy_data`, and `failure_rolls_back_committed_state`

- [ ] Write a consumer importing the absent proof module and invoking the promised public theorems.
- [ ] Run it under the external deadline and confirm RED is an unknown module or identifier.
- [ ] Implement the smallest total transition model matching the correspondence worksheet.
- [ ] Prove the invariant and focused semantic theorems with local heartbeat caps.
- [ ] Run the consumer and focused proof build under separate deadlines and confirm GREEN.
- [ ] Commit the model, proofs, imports, design, and plan.

### Task 2: Add bounded cases, sensitivity, and deterministic corpus generation

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/SchemaMigrationCases.lean`
- Create: `formal/HoiminOracle/SchemaMigrationAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/schema-migration-concurrency.jsonl`
- Modify: `formal/HoiminOracle/lakefile.toml`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Produces: fixed correct traces, `reachableStates`, three sensitivity witnesses, and typed `OracleCase` records
- Produces executable: `generate_schema_migration`

- [ ] Add fixed expected traces for fresh, v1, stale-read, rollback, and future-version scenarios.
- [ ] Add deduplicated two-actor exploration through depth 9 and enforce the fixed state ceiling.
- [ ] Add one fixed witness for each broken family and require all three before corpus output.
- [ ] Run `--stats` and `--sensitivity` under separate deadlines; confirm fixed depth/state counts and all witnesses.
- [ ] Generate the corpus, then require `--check` to report it fresh.
- [ ] Commit the executable evidence and corpus.

### Task 3: Add public SQLite correspondence

**Files:**
- Create: `crates/hoimin-cli/tests/lean_schema_migration_oracle.rs`

**Interfaces:**
- Consumes: `formal/HoiminOracle/corpus/schema-migration-concurrency.jsonl`
- Consumes public API: `SessionHandler::open`
- Produces: strict parser validation and public observations for concurrent fresh/v1 opens, rollback, and future rejection

- [ ] Add a failing parser/adapter test before the corpus-backed implementation is complete.
- [ ] Parse with `deny_unknown_fields`, exact modes, scenarios, version names, unique IDs, and complete expectations.
- [ ] Synchronize public concurrent opens with a start barrier and a bounded coordinator; do not assert private read timing.
- [ ] Reuse the owned schema-v1 golden fixture and build isolated malformed/future SQLite fixtures.
- [ ] Run the focused integration test and schema unit tests, fixing only model/adapter correspondence mismatches.
- [ ] Commit the adapter.

### Task 4: Report, verify, and integrate

**Files:**
- Create: `docs/superpowers/reports/2026-08-11-lean-schema-migration-concurrency-audit.md`

**Interfaces:**
- Produces: scoped conclusions, correspondence ledger, counterexample records, limitations, commands, and next priority

- [ ] Run the Lean build, corpus freshness, stats, sensitivity, focused Rust tests, formatting, Clippy, and full workspace tests.
- [ ] Write the report without claiming Lean proves Rust or model-only schedules are implementation mismatches.
- [ ] Run `git diff --check`, inspect the complete branch diff, and commit the report.
- [ ] Push, create a PR, wait for all applicable CI checks, squash merge, and verify `origin/main` contains the merge.
