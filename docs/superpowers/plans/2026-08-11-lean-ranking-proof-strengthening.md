# Lean Candidate Ranking Proof Strengthening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove that every ID returned by diverse selection comes from the saved ranked candidate list.

**Architecture:** Add compositional membership lemmas around the existing pure list functions, culminating in one public diverse-selection theorem. Keep finite executable checks responsible for uniqueness, length, and tier ordering.

**Tech Stack:** Lean 4.24.0, Lake, Rust verification adapter unchanged

## Global Constraints

- Run at most one potentially expensive Lean process at a time.
- Apply a 20-second external deadline and `maxHeartbeats 100000`; never use unlimited heartbeats.
- Do not raise bounds after timeout or abnormal memory behavior.
- Do not modify production ranking semantics or corpus schema.

---

### Task 1: Add the diverse-selection origin theorem

**Files:**
- Modify: `formal/HoiminOracle/HoiminOracle/CandidateRankingProofs.lean`
- Test: `/tmp/hoimin-ranking-proof-consumer.lean` (ephemeral)

**Interfaces:**
- Consumes: `firstForPath?`, `selectRound`, `eraseSelected`, `roundRobinWithFuel`, `roundRobinTier`, `diverseOrderWithFuel`, `diverseOrder`, and `diverseSelect` from `CandidateRankingModel.lean`
- Produces: `diverseSelect_member_of_saved (ranked) (limit) (id) (selected) : ∃ candidate ∈ ranked, candidate.candidate.id = id`

- [ ] **Step 1: Write the failing proof consumer**

  Import `HoiminOracle.CandidateRankingProofs` and state an example whose body is
  `exact diverseSelect_member_of_saved ranked limit id selected`.

- [ ] **Step 2: Run the consumer to verify RED**

  Run `lake env lean /tmp/hoimin-ranking-proof-consumer.lean` under the external
  deadline. Expect failure because `diverseSelect_member_of_saved` is unknown.

- [ ] **Step 3: Add minimal compositional membership lemmas**

  Prove each helper emits or retains only elements of its candidate input, then
  compose those lemmas into `diverseSelect_member_of_saved`. Keep every theorem
  under the local heartbeat cap.

- [ ] **Step 4: Run the consumer and library build to verify GREEN**

  Run the consumer and `lake -Kjobs=1 build` under separate 20-second deadlines.
  Expect both to pass without warnings or resource symptoms.

- [ ] **Step 5: Commit the proof and design artifacts**

  Commit the design, plan, and proof changes together as the independently useful
  guarantee strengthening.

### Task 2: Refresh evidence and report

**Files:**
- Modify: `docs/superpowers/reports/2026-08-11-lean-candidate-ranking-audit.md`

**Interfaces:**
- Consumes: `diverseSelect_member_of_saved`
- Produces: an accurate audit report separating unbounded and finite evidence

- [ ] **Step 1: Run candidate audit regression checks**

  Run corpus `--check`, `--stats`, and `--sensitivity` one at a time under the
  20-second deadline.

- [ ] **Step 2: Update the report**

  Add the new theorem, narrow the finite-only limitation, and record the focused
  verification evidence and unchanged resource envelope.

- [ ] **Step 3: Run final proof and formatting checks**

  Re-run the Lean build and consumer, then run `git diff --check`.

- [ ] **Step 4: Commit the report**

  Commit the refreshed evidence separately so the proof change remains easy to
  review.
