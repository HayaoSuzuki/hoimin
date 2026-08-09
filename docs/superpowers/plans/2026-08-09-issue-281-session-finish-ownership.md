# Issue #281 Session Finish Ownership Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject session finish requests from handlers that do not own the run, without changing durable state.

**Architecture:** Put a process-local ownership guard at the start of `SessionHandler::finish`, before its SQLite transaction. Encode the same transition in the Lean recovery model, regenerate the strict corpus, and remove the temporary known-mismatch allowance from the Rust correspondence adapter.

**Tech Stack:** Rust 2024, `rusqlite`, Rust integration tests, Lean 4, Lake, JSONL oracle corpus.

## Global Constraints

- Preserve complete and incomplete finish behavior while the caller owns the run.
- Treat successful finish as releasing both the lock and that handler's authority to finish again.
- Reject non-owner finish with `session.finish.owner` before any database write.
- Do not broaden ownership enforcement to lookup or persistence in this issue.
- Keep all 18 session-recovery oracle cases strict; no reviewed mismatch remains.

---

## File map

- Modify `crates/hoimin-cli/tests/session_handler.rs`: public regression for rejected non-owner finish and unchanged state.
- Modify `crates/hoimin-cli/src/session/mod.rs`: pre-transaction ownership guard and error documentation.
- Modify `formal/HoiminOracle/HoiminOracle/SessionModel.lean`: `notOwner` rejection and guarded finish transition.
- Modify `formal/HoiminOracle/HoiminOracle/SessionProofs.lean`: prove the guarded finish still satisfies ownership obligations.
- Modify `formal/HoiminOracle/HoiminOracle/SessionCases.lean`: retain the counterexample schedule under a contract-accurate case ID.
- Modify `formal/HoiminOracle/corpus/session-recovery.jsonl`: regenerate expected observations from Lean.
- Modify `crates/hoimin-cli/tests/lean_session_oracle.rs`: require every strict case to match.
- Modify `docs/superpowers/reports/2026-08-09-lean-session-recovery-audit.md`: append the verified resolution without rewriting the historical finding.
- Modify `docs/superpowers/reports/2026-08-09-lean-session-recovery-counterexamples.md`: mark the counterexample resolved by Issue #281.

### Task 1: Public regression and minimal Rust repair

**Interfaces:**

- Consumes: `SessionHandler::{open,begin,finish,load}` and `EffectFailure::code()`.
- Produces: non-owner `finish(FinishSession) -> Err(EffectFailed)` with code `session.finish.owner` and no durable mutation.

- [ ] **Step 1: Write the failing integration test**

Add `non_owner_finish_is_rejected_without_changing_run_state` to
`crates/hoimin-cli/tests/session_handler.rs`. Create owner and contender
handlers, begin an incomplete run with the owner, call incomplete finish from
the contender, and assert `session.finish.owner`. Verify the contender still
gets `session.resume.active`; drop the owner; then verify the contender resumes
the same run. The production change that makes this test fail is removal of the
ownership guard.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p hoimin-cli --test session_handler non_owner_finish_is_rejected_without_changing_run_state -- --exact
```

Expected: FAIL because the current non-owner finish returns `Ok`.

- [ ] **Step 3: Implement the minimal guard**

At the start of `SessionHandler::finish`, before
`transaction_with_behavior`, return:

```rust
if !self.ownerships.contains_key(&request.run_id) {
    return Err(state_failure(
        id,
        "session.finish.owner",
        "handler does not own run",
    ));
}
```

Keep the transaction and post-commit `ownerships.remove` ordering unchanged.
Update the method's `# Errors` text to include missing ownership.

- [ ] **Step 4: Verify GREEN and local regression coverage**

Run:

```bash
cargo test -p hoimin-cli --test session_handler non_owner_finish_is_rejected_without_changing_run_state -- --exact
cargo test -p hoimin-cli --test session_handler
```

Expected: both commands PASS.

If the full integration target exposes the old incomplete-finish idempotency
expectation, rename it to
`successful_finish_releases_the_handlers_finish_authority` and assert that
subsequent incomplete or complete finish calls return
`session.finish.owner`. A reusable post-release authorization is unsafe because
another handler may acquire the run between calls.

### Task 2: Formalize the repaired ownership contract

**Interfaces:**

- Consumes: `owns state run handler`, `finishRun`, and generated `OracleCase` observations.
- Produces: rejection `session.finish.owner` for non-owner finish and unchanged state in Lean.

- [ ] **Step 1: Add the Lean expectation before changing the transition**

Add a theorem to `SessionProofs.lean` asserting that a live non-owner finish is
rejected and leaves state unchanged. Run:

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/SessionProofs.lean
```

Expected: FAIL because `finishRun` currently accepts the operation.

- [ ] **Step 2: Implement the minimal model change**

Add `Rejection.notOwner`, map it to `session.finish.owner`, and insert an
`else if !owns state run handler then reject state .notOwner` branch after the
live-handler check in `finishRun`. Adjust existing finish proofs for the new
branch.

- [ ] **Step 3: Rename and regenerate the strict case**

Rename `incomplete_finish_is_idempotent` to
`released_handler_cannot_finish_again`. Rename
`non_owner_incomplete_finish_releases_for_resume` to
`non_owner_incomplete_finish_is_rejected`, preserving both schedules. Generate
and check the corpus:

```bash
cd formal/HoiminOracle
lake exe generate_session -- --output corpus/session-recovery.jsonl
lake exe generate_session -- --check corpus/session-recovery.jsonl
```

- [ ] **Step 4: Remove the mismatch allowance and verify correspondence**

Replace the single-ID mismatch logic in `lean_session_oracle.rs` with an
assertion that every selected case has `CaseClass::Match`. Run:

```bash
HOIMIN_SESSION_ORACLE_CASE=non_owner_incomplete_finish_is_rejected cargo test -p hoimin-cli --test lean_session_oracle oracle_correspondence -- --exact --nocapture
cargo test -p hoimin-cli --test lean_session_oracle -- --nocapture
```

Expected: all selected and complete oracle tests PASS with zero mismatches.

### Task 3: Resolution record and full verification

**Interfaces:**

- Consumes: verified Rust and Lean outputs from Tasks 1 and 2.
- Produces: a traceable resolution record and merge-ready branch.

- [ ] **Step 1: Append the resolution evidence**

Add a dated resolution section to both session recovery reports. Record Issue
#281, the ownership guard, the renamed strict case, 18 matches, zero
mismatches, and the exact verification commands. Preserve the original audit
result as historical evidence.

- [ ] **Step 2: Run formal verification**

```bash
cd formal/HoiminOracle
lake build
lake exe generate_session -- --check corpus/session-recovery.jsonl
lake exe generate_session -- --stats
lake exe generate_session -- --sensitivity
```

Expected: all commands exit 0; statistics report 18 strict cases.

- [ ] **Step 3: Run Rust verification**

```bash
cargo fmt --check
cargo test --workspace
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 4: Commit, review, and integrate**

Commit implementation and resolution documents, push the issue branch, create
a PR containing `Fixes #281`, wait for required checks, and squash-merge it.
Then fast-forward local `main` and remove the merged worktree and local branch.
