# Lean Bounded Candidate Discovery Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove bounded candidate discovery semantics in Lean, replay Lean-owned cases against Rust internals and the public plan command, and repair any confirmed same-premise mismatch.

**Architecture:** Three imported Lean modules own pure reference semantics, universal theorems, and closed cases. A non-imported executable checks broken variants and owns deterministic JSONL. Rust unit and integration adapters map model roles to `CandidatePrefix`, real producers, ordered targets, spool records, and public plan observations without expanding the public API.

**Tech Stack:** Lean 4.32.2, Lake, Rust 2024, Serde JSON, Cargo tests.

## Global constraints

- Work only in `.worktrees/lean-bounded-candidate-discovery` on branch `audit/lean-bounded-candidate-discovery`.
- Keep Lean I/O and finite enumeration out of the imported library root.
- Limit nontrivial Lean proofs to 100,000 heartbeats and run Lean commands serially.
- Treat failed resource observation as infrastructure failure, never a proof result.
- Add a Rust production correction only after a focused regression fails under matching Lean premises.
- Keep generated expectations Lean-owned; Rust validates and observes but does not duplicate the oracle.

## Task 1: Pure model and universal proofs

**Files:**
- Create `formal/HoiminOracle/HoiminOracle/BoundedCandidateDiscoveryModel.lean`
- Create `formal/HoiminOracle/HoiminOracle/BoundedCandidateDiscoveryProofs.lean`
- Modify `formal/HoiminOracle/HoiminOracle.lean`

- [ ] Write `/tmp/hoimin-bounded-discovery-proof-consumer.lean` importing the absent proof module and checking prefix, bound, truncation, contiguous sequence, and terminal-target theorem names.
- [ ] Run `lake env lean /tmp/hoimin-bounded-discovery-proof-consumer.lean` and record the expected missing-module failure.
- [ ] Implement `Candidate`, `Discovery`, stable eligibility/order/dedup reference functions, `bounded`, `sequences`, producer windows, `TargetStep`, and terminal ordered-target accumulation.
- [ ] Prove `bounded_candidates_eq_reference_take`, `bounded_length_le_limit`, `bounded_truncated_iff`, `bounded_zero`, `sequences_contiguous`, `target_count_le_limit`, and `truncation_stops_later_targets` with local heartbeat limits.
- [ ] Import the modules and rerun the proof module and consumer to green.
- [ ] Commit the model and proofs.

## Task 2: Closed cases, sensitivity, executable, and corpus

**Files:**
- Create `formal/HoiminOracle/HoiminOracle/BoundedCandidateDiscoveryCases.lean`
- Create `formal/HoiminOracle/BoundedCandidateDiscoveryAuditMain.lean`
- Create `formal/HoiminOracle/corpus/bounded-candidate-discovery.jsonl`
- Modify `formal/HoiminOracle/HoiminOracle.lean`
- Modify `formal/HoiminOracle/lakefile.toml`

- [ ] Register `generate_bounded_candidate_discovery`, run it, and record the expected missing-root failure.
- [ ] Define the closed strict/internal/model-only cases from the design correspondence table.
- [ ] Define broken variants and witnesses for producer lookahead, ordering, eligibility timing, duplicate accounting, merge retention, overflow reporting, global target capacity, terminal truncation, and incomplete public projection.
- [ ] Implement `--output`, `--check`, `--cases`, `--sensitivity`, and `--stats`; refuse corpus output unless all cases and sensitivity checks pass.
- [ ] Build serially, run sensitivity and cases, generate the corpus, and verify `--check` succeeds.
- [ ] Commit cases, executable, registration, and generated corpus.

## Task 3: Rust closed-schema adapter and internal correspondence

**Files:**
- Create `crates/hoimin-cli/tests/lean_bounded_candidate_discovery_oracle.rs`
- Modify `crates/hoimin-cli/src/analyzer/rust_tests.rs`
- Modify `crates/hoimin-cli/src/analyzer/mod.rs` tests only if internal target/spool access is required

- [ ] Add the integration parser first; observe failure because the expected corpus or adapter case dispatch is absent.
- [ ] Enforce schema version, exact case IDs/modes, enum closure, unique IDs, internally consistent expected prefixes, and rejection of unknown/crossed fields.
- [ ] Add analyzer unit fixtures that map corpus roles to out-of-order duplicate `CandidatePrefix` pushes and to all three real producer paths; compare exact order, overflow, and retention peaks.
- [ ] Add ordered-target and truncated non-final spool correspondence, including globally contiguous sequences and absence of later-target analysis.
- [ ] Run focused unit and integration tests to green and commit.

## Task 4: Strict public plan correspondence and bug policy

**Files:**
- Modify `crates/hoimin-cli/tests/lean_bounded_candidate_discovery_oracle.rs`
- Modify production Rust only if a same-premise mismatch is observed

- [ ] Replay the strict corpus case through the real `hoimin plan` binary and compare candidate prefix, `truncated: true`, candidate-limit diagnostic, and exit code 4.
- [ ] Classify parse, process, fixture, timeout, and unsupported-width failures as infrastructure errors.
- [ ] If correspondence fails, add the smallest direct Rust regression and observe red before changing production; then apply the minimal repair and rerun focused tests.
- [ ] If correspondence matches, record explicitly that no production repair is justified.
- [ ] Commit the public adapter and any test-first repair.

## Task 5: Audit report and verification

**Files:**
- Create `docs/superpowers/reports/2026-08-14-lean-bounded-candidate-discovery-audit.md`

- [ ] Run the Lean build, proof consumer, `--cases`, `--sensitivity`, and corpus freshness check serially.
- [ ] Run focused analyzer/oracle/plan tests, `cargo fmt --check`, workspace tests, and clippy for all targets/features.
- [ ] Record exact commands, results, correspondence modes, sensitivity witnesses, resource status, and whether Rust changed in the report.
- [ ] Run `git diff --check`, inspect the final diff, and commit the report.

## Task 6: Review, PR, CI, and merge

- [ ] Request an independent read-only review against issue #309 and this plan; resolve every Critical or Important finding.
- [ ] Repeat fresh verification after review changes.
- [ ] Push the branch and create a PR whose body includes `Closes #309`, design summary, proof claims, correspondence evidence, and test commands.
- [ ] Monitor required CI checks; diagnose and correct failures in the same worktree.
- [ ] Merge the green PR, verify issue closure and merged commit, then remove the clean worktree.
