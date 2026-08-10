# Lean candidate ranking and top-selection audit implementation plan

**Goal:** Establish Lean-checked ranking/selection invariants and compare a
Lean-owned corpus with Hoimin's public plan/verify behavior.

**Architecture:** Keep cheap model definitions and proofs in imported Lean
modules, finite evaluation and serialization in a dedicated executable, and
implementation correspondence in a standalone Rust integration test. Do not
change production behavior without a confirmed failing correspondence case.

## Global constraints

- Use test-driven development for every executable or production-facing change.
- One Lean process at a time, 20-second external deadlines, local heartbeat
  limits at or below 100,000, and finite manifest size at most four.
- Lean owns expected JSONL values; Rust only validates and observes.
- Preserve `.venv -> ../../.venv` as untracked worktree setup.
- Commit documentation, formal evidence, correspondence, and report in reviewable units.

### Task 1: Formal model and proofs

**Files:**
- Create `formal/HoiminOracle/HoiminOracle/CandidateRankingModel.lean`
- Create `formal/HoiminOracle/HoiminOracle/CandidateRankingProofs.lean`
- Modify `formal/HoiminOracle/HoiminOracle.lean`

- [ ] Add missing imports first and confirm the bounded Lean build fails.
- [ ] Define finite reason/operator/path/candidate/ranked-candidate types.
- [ ] Implement reason construction, score sum, deterministic ranking,
      validation, strict selection, and diverse tier/path round-robin.
- [ ] Prove reason scores and sums, strict prefix and length, selected
      membership, and no-duplicate preservation under explicit premises.
- [ ] Add three deliberately broken variants and minimal witnesses.
- [ ] Run `perl -e 'alarm shift; exec @ARGV' 20 lake build` after each small unit.

### Task 2: Cases, finite audit, and generated corpus

**Files:**
- Create `formal/HoiminOracle/HoiminOracle/CandidateRankingCases.lean`
- Create `formal/HoiminOracle/CandidateRankingAuditMain.lean`
- Create `formal/HoiminOracle/corpus/candidate-ranking.jsonl`
- Modify `formal/HoiminOracle/lakefile.toml`

- [ ] Add case imports/executable target first and observe the missing-module red.
- [ ] Encode stable-order, selector-score, strict-prefix, diverse-tier, and
      truncated-boundary cases with explicit correspondence modes.
- [ ] Add finite enumeration statistics and fail-closed sensitivity checks.
- [ ] Generate the corpus, check it is deterministic, and record size/cost.

### Task 3: Public CLI correspondence adapter

**Files:**
- Create `crates/hoimin-cli/tests/lean_candidate_ranking_oracle.rs`

- [ ] Add parser/schema tests that initially fail because the corpus adapter is absent.
- [ ] Build isolated Python fixtures and map real candidates to semantic roles
      by path, line, column, operator, and symbol.
- [ ] Invoke public `plan` and `verify --top` strict/diverse with bounded process cleanup.
- [ ] Compare saved reasons/scores/ranks and selected role order to Lean values.
- [ ] Keep model-only cases parsed and reported but outside strict correspondence.

### Task 4: Existing tests and mismatch handling

- [ ] Run focused ranking and selection library tests and `--test plan`.
- [ ] If correspondence fails, classify model, harness, specification, or
      implementation cause before changing code.
- [ ] For a confirmed implementation defect, preserve the failing adapter case,
      add the smallest focused Rust regression test, then implement one repair.

### Task 5: Report and verification

**Files:**
- Create `docs/superpowers/reports/2026-08-11-lean-candidate-ranking-audit.md`

- [ ] Record theorem statements, finite domain, statistics, sensitivity
      witnesses, correspondence matrix, timings, limits, and exclusions.
- [ ] Run corpus freshness, bounded audit, focused tests, `cargo fmt --check`,
      Clippy with warnings denied, full workspace tests, and diff hygiene.
- [ ] Self-review every changed line and confirm only `.venv` remains untracked setup.

### Task 6: Delivery

- [ ] Commit the verified implementation and report.
- [ ] Push `audit/lean-candidate-ranking` and create a PR with evidence.
- [ ] Monitor every required check, diagnose and repair failures test-first,
      and merge only after the branch is green and mergeable.
- [ ] Confirm the PR is merged and report the merge commit and retained local setup.
