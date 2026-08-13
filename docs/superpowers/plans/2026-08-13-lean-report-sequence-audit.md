# Lean Report-Sequence Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the report-event protocol in Lean, replay Lean-generated expectations through `ReportSequence::observe`, and turn any strict mismatch into a named Rust regression and minimal production repair.

**Architecture:** Three imported Lean modules own the pure state machine, unbounded invariants, and fixed oracle cases. A non-imported executable owns bounded exploration, broken-variant sensitivity, statistics, and deterministic JSONL generation; a `hoimin-core` integration test translates that corpus into public `OutputEvent` values without reproducing expected semantics.

**Tech Stack:** Lean 4.32.2, Lake, Rust 2024 edition, Serde JSON, Cargo integration tests.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-report-sequence-audit` on branch `audit/lean-report-sequence`.
- Include specification, plan, formal files, corpus, Rust adapter, regression/fix if required, and report in this worktree.
- Preserve the untracked `.venv -> ../../.venv` setup link and never stage it.
- Use exactly `strict`, `model-only`, `internal-fixture`, and `infrastructure-error` as correspondence modes.
- Start implementation-facing rows as `model-only`; promote only complete same-premise public observations to `strict`.
- Keep exhaustive evaluation, shrinking, statistics, and corpus I/O out of imported Lean modules.
- Run one potentially expensive Lean command at a time with a 20-second limit, 768 MiB aggregate RSS limit, 250 ms sampling, and Lake `-Kjobs=1`.
- Put local `maxHeartbeats 100000` limits on non-trivial proofs; never use unlimited heartbeats.
- Treat parsing, setup, panic, timeout, and incomplete observation as `infrastructure-error`, never as a mismatch.
- Never hand-edit generated expectations or duplicate their semantics in Rust.
- Change production Rust only after a strict mismatch is minimized and a focused Rust regression fails for the same reason.
- Do not weaken Lean to match current Rust behavior.

---

## File Structure

- Create `formal/HoiminOracle/HoiminOracle/ReportSequenceModel.lean`: pure event, state, rejection, step, and broken transition semantics.
- Create `formal/HoiminOracle/HoiminOracle/ReportSequenceProofs.lean`: state invariant and general transition theorems.
- Create `formal/HoiminOracle/HoiminOracle/ReportSequenceCases.lean`: fixed cases, probes, and sensitivity witnesses.
- Create `formal/HoiminOracle/ReportSequenceAuditMain.lean`: bounded exploration, JSONL, freshness, sensitivity, cases, and statistics CLI.
- Create `formal/HoiminOracle/corpus/report-sequence.jsonl`: deterministic Lean-generated expectations.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: import the proof-oriented modules.
- Modify `formal/HoiminOracle/lakefile.toml`: register `generate_report_sequence`.
- Create `crates/hoimin-core/tests/lean_report_sequence_oracle.rs`: closed-schema public Rust replay adapter.
- Modify `crates/hoimin-core/tests/report_policy.rs` only if a strict mismatch needs a focused regression.
- Modify `crates/hoimin-core/src/report.rs` only if that regression proves a confirmed bug.
- Create `docs/superpowers/reports/2026-08-13-lean-report-sequence-audit.md`: final audit report.
- Create `docs/superpowers/reports/2026-08-13-lean-report-sequence-counterexamples.md` only if a mismatch or ambiguity remains.

### Task 1: Pure Lean protocol model and unbounded proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ReportSequenceModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/ReportSequenceProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-report-sequence-proof-consumer.lean`

**Interfaces:**
- Consumes: Lean `Std` only.
- Produces: `RunId`, `MutantId`, `Status`, `Termination`, `EventKind`, `Event`, `Rejection`, `State`, `Verdict`, `initial`, `step`, `run`, `Invariant`, and the named theorems below.

- [ ] **Step 1: Write the failing proof consumer**

Create `/tmp/hoimin-report-sequence-proof-consumer.lean`:

```lean
import HoiminOracle.ReportSequenceProofs
open HoiminOracle.ReportSequence

example (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome) :
    (step state event).state = state :=
  rejected_preserves_state state event rejected

example (state : State) (event : Event)
    (accepted : (step state event).rejection = none)
    (previous : state.last = some 1) : 1 < event.sequence :=
  accepted_sequence_advances state event accepted 1 previous

example (trace : List Event) : Invariant (run initial trace) :=
  run_preserves_invariant trace
```

- [ ] **Step 2: Run the consumer and verify RED**

```bash
cd formal/HoiminOracle
python3 tools/lean_resource_guard.py --timeout-seconds 20 \
  --rss-limit-mib 768 --sample-ms 250 --stats /tmp/report-sequence-proof-red.json \
  -- lake env lean /tmp/hoimin-report-sequence-proof-consumer.lean
```

Expected: FAIL because the module is absent. A guard failure is `infrastructure-error`; independently bound the Lean command before drawing a semantic conclusion.

- [ ] **Step 3: Implement the exact model types and validation order**

Create `ReportSequenceModel.lean` with these public shapes:

```lean
import Std
namespace HoiminOracle.ReportSequence

inductive RunId | first | second deriving Repr, DecidableEq, BEq
inductive MutantId | alpha | beta deriving Repr, DecidableEq, BEq
inductive Status
  | survived | killed | timeout | outOfMemory | processLimit | error | notRun
  deriving Repr, DecidableEq, BEq
inductive Termination
  | exitZero | exitNonzero | timeout | outOfMemory | processLimit | cancelled
  deriving Repr, DecidableEq, BEq
inductive EventKind
  | runStarted
  | mutantStarted (id : MutantId) (mutantSequence : Nat)
  | mutantFinished (id : MutantId) (mutantSequence : Nat)
      (status : Status) (termination : Option Termination)
  | diagnostic | runFinished
  deriving Repr, DecidableEq, BEq
structure Event where
  runId : RunId
  sequence : Nat
  kind : EventKind
  deriving Repr, DecidableEq, BEq
inductive Rejection
  | runNotStarted
  | runAlreadyStarted (runId : RunId)
  | runAlreadyFinished (runId : RunId)
  | runFinishedWithActiveMutants (count : Nat)
  | runIdMismatch (expected received : RunId)
  | notMonotonic (previous received : Nat)
  | mutantAlreadyStarted (id : MutantId) (mutantSequence : Nat)
  | duplicateMutantIdentity (id : MutantId) (mutantSequence : Nat)
  | mutantIdentitySequenceMismatch
      (id : MutantId) (expectedSequence receivedSequence : Nat)
  | mutantNotStarted (id : MutantId) (mutantSequence : Nat)
  | mutantStatusTerminationMismatch
      (id : MutantId) (mutantSequence : Nat)
      (status expectedStatus : Status) (termination : Termination)
  deriving Repr, DecidableEq, BEq
structure State where
  last : Option Nat
  runId : Option RunId
  active : List (MutantId × Nat)
  seen : List (MutantId × Nat)
  finished : Bool
  deriving Repr, DecidableEq, BEq
structure Verdict where
  state : State
  rejection : Option Rejection
  deriving Repr, DecidableEq, BEq
```

Define `classify`, `seenSequence?`, active membership/removal, `mutantError?`, `lifecycleError?`, `step`, and `run`. Match Rust precedence exactly: terminal/run lifecycle, mutant lifecycle/status, monotonicity, then mutation. Rejections return the original state.

- [ ] **Step 4: Define and prove the invariant**

Create `ReportSequenceProofs.lean` with:

```lean
def Invariant (state : State) : Prop :=
  state.active.Pairwise (fun left right => left ≠ right) ∧
  state.seen.Pairwise (fun left right => left.1 ≠ right.1) ∧
  (∀ entry, entry ∈ state.active → entry ∈ state.seen) ∧
  (state.finished = true → state.active = [])
```

Prove with local `maxHeartbeats 100000`: `rejected_preserves_state`, `accepted_sequence_advances`, `accepted_run_id_is_stable`, `seen_identity_is_stable`, `finish_removes_active`, `run_finish_requires_no_active`, `finished_is_terminal`, `step_preserves_invariant`, and `run_preserves_invariant`. Prefer one accepted-event preservation lemma per kind over a brittle monolithic tactic proof.

- [ ] **Step 5: Import modules and verify GREEN**

Run guarded builds for `HoiminOracle.ReportSequenceModel`, `HoiminOracle.ReportSequenceProofs`, and the proof consumer separately. Expected: PASS without `sorry`, `admit`, or audit-specific axioms.

- [ ] **Step 6: Commit model and proofs**

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/HoiminOracle/ReportSequenceModel.lean \
  formal/HoiminOracle/HoiminOracle/ReportSequenceProofs.lean
git commit -m "test(lean): prove report sequence invariants"
```

### Task 2: Fixed cases and broken-variant sensitivity

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ReportSequenceCases.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Test: `/tmp/hoimin-report-sequence-case-consumer.lean`

**Interfaces:**
- Consumes: Task 1 `Event`, `Verdict`, `step`, and `run`.
- Produces: `Expected`, `Case`, `cases`, four broken transitions, fixed witnesses, and `sensitivityPasses`.

- [ ] **Step 1: Write and run the failing case consumer**

```lean
import HoiminOracle.ReportSequenceCases
open HoiminOracle.ReportSequence
example : cases.length ≥ 17 := by decide
example : sensitivityPasses = true := by decide
example : (cases.map Case.id).eraseDups.length = cases.length := by decide
```

Expected: RED because the cases module is absent.

- [ ] **Step 2: Define cases once in Lean**

```lean
structure Expected where
  accepted : Bool
  errorCode : Option String
  errorFields : List (String × String)
  deriving Repr, DecidableEq, BEq
structure Case where
  id : String
  mode : String
  premise : String
  prefix : List Event
  target : Event
  expected : Expected
  probe : Option Event
  probeExpected : Option Expected
  deriving Repr, DecidableEq, BEq
```

Derive error code and every constructor field from the `Rejection` returned by
`step`; never type expected errors independently. Define at least:
`requires_run_start`, `accepts_diagnostic_after_start`, `rejects_cross_run`,
equal/reverse nonmonotonic cases, duplicate run start, active duplicate,
same/changed-sequence identity reuse, finish-before-start, finish identity
mismatch, all six present-termination classification mappings, representative
status/termination mismatches, absent-termination acceptance, active-mutant
final rejection, distinct concurrency, finish-then-final acceptance, and
after-final rejection. Add a valid probe to every rejection where it can expose
state pollution. Begin every mode as `model-only`.

- [ ] **Step 3: Implement applicable broken families**

Define `brokenMutateBeforeValidate`, `brokenAllowIdentityReuse`, `brokenAcceptEqualSequence`, and `brokenSequenceFirst`. Fixed witnesses must distinguish: invalid start followed by a valid probe; start/finish/reuse of `alpha`; equal report sequences; and a cross-run plus nonmonotonic event that must yield run-ID mismatch first.

- [ ] **Step 4: Verify cases and sensitivity GREEN**

Run the consumer under the resource guard. Expected: at least 17 unique IDs, all initial modes `model-only`, and all four sensitivity checks true.

- [ ] **Step 5: Commit cases**

```bash
git add formal/HoiminOracle/HoiminOracle.lean \
  formal/HoiminOracle/HoiminOracle/ReportSequenceCases.lean
git commit -m "test(lean): define report sequence oracle cases"
```

### Task 3: Bounded explorer and deterministic corpus

**Files:**
- Create: `formal/HoiminOracle/ReportSequenceAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/report-sequence.jsonl`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 2 cases and sensitivity.
- Produces: `generate_report_sequence` with `--output`, `--check`, `--cases`, `--sensitivity`, and `--stats`; schema-version-1 JSONL.

- [ ] **Step 1: Register the absent executable and verify RED**

```toml
[[lean_exe]]
name = "generate_report_sequence"
root = "ReportSequenceAuditMain"
```

Run `lake exe generate_report_sequence -- --sensitivity`; expect missing-root failure.

- [ ] **Step 2: Implement deterministic rendering and commands**

Use `Lean.Data.Json`, stable snake-case values, and atomic output. A row has this exact shape:

```json
{"schema":1,"id":"rejects_cross_run","mode":"model-only","premise":"started_first_run","prefix":[{"kind":"run_started","run_id":"first","sequence":1}],"target":{"kind":"diagnostic","run_id":"second","sequence":2},"expected":{"accepted":false,"error_code":"report.sequence.run_id_mismatch","error_fields":{"expected":"first","received":"second"}},"probe":{"kind":"diagnostic","run_id":"first","sequence":2},"probe_expected":{"accepted":true,"error_code":null,"error_fields":{}}}
```

`--check` compares bytes without rewriting; `--sensitivity` exits nonzero unless all four broken variants are detected.

- [ ] **Step 3: Implement shortest-first bounded exploration**

Use two run IDs, two mutant IDs, mutant sequences `0/1`, report sequences `0/1/2`, and representative start/finish/diagnostic/final events. Explore depth 0 upward in stable order, retain the first trace per exact semantic state, and check all outgoing transitions before deduplication. Start at the minimum depth containing every fixed witness; raise by only one after reviewing measurements.

`--stats` prints `depth`, `event_alphabet`, `generated_traces`, `retained_states`, `transitions`, `fixed_cases`, mode counts, and sensitivity booleans. The external guard records elapsed time and peak RSS.

- [ ] **Step 4: Generate and freshness-check the corpus**

Run one at a time under the guard: Lake build, `--sensitivity`, `--cases`, `--stats`, `--output corpus/report-sequence.jsonl`, then `--check corpus/report-sequence.jsonl`. Expected: all green, measured bound printed, all rows still `model-only`.

- [ ] **Step 5: Commit executable and corpus**

```bash
git add formal/HoiminOracle/lakefile.toml \
  formal/HoiminOracle/ReportSequenceAuditMain.lean \
  formal/HoiminOracle/corpus/report-sequence.jsonl
git commit -m "test(lean): generate report sequence oracle"
```

### Task 4: Strict public Rust correspondence adapter

**Files:**
- Create: `crates/hoimin-core/tests/lean_report_sequence_oracle.rs`
- Modify: `formal/HoiminOracle/HoiminOracle/ReportSequenceCases.lean`
- Regenerate: `formal/HoiminOracle/corpus/report-sequence.jsonl`

**Interfaces:**
- Consumes: Task 3 schema and public `ReportSequence::new` / `observe`.
- Produces: closed corpus validation, isolated replay, normalized typed errors, and strict correspondence.

- [ ] **Step 1: Write schema validation RED tests**

Define `OracleCase`, `OracleEvent`, and `Expected` with
`#[serde(deny_unknown_fields)]`; `Expected` contains `accepted`, `error_code`,
and a closed `error_fields` map whose allowed keys depend on the error code.
Add tests rejecting unknown schema/mode/enums, duplicate IDs, unpaired probes,
unknown or missing error fields, values outside the declared finite set, and an
empty corpus. Run `cargo test -p hoimin-core --test
lean_report_sequence_oracle corpus_ -- --nocapture`; expect compile RED until
parser helpers exist, then GREEN without replay.

- [ ] **Step 2: Implement translation and observation only**

```rust
fn observe(sequence: &mut ReportSequence, event: &OracleEvent)
    -> Result<Observed, String> {
    let output = to_output_event(event)?;
    Ok(match sequence.observe(&output) {
        Ok(()) => Observed::accepted(),
        Err(error) => Observed::rejected(normalize_error(&error)),
    })
}
```

Map `first/second` to fixed real strings and construct minimal public
`MutationCandidate`, `MutantStarted`, `MutantFinished`, `Diagnostic`, and
`RunSummary`. `normalize_error` extracts the Rust error variant and every
public constructor field, normalizing fixture strings back to Lean roles; it
must not calculate expected precedence. Compare the complete normalized error,
not only its variant.

- [ ] **Step 3: Replay each case in isolation**

Create a new `ReportSequence` per row, replay prefix, target, then probe on the same object. Wrap each row in `catch_unwind`. Prefix disagreement, panic, parsing/construction failure, or missing observation is `infrastructure-error`. Support `HOIMIN_REPORT_SEQUENCE_CASE=<id>`. Print `model-only` differences without calling them production mismatches.

- [ ] **Step 4: Review and promote exact correspondence**

Run all rows and each row separately. For every row, verify production can configure each premise and the adapter observes the complete claimed result. Promote reviewed Lean cases to `strict`, regenerate, and confirm the only intended corpus semantic change is mode. Then run the full adapter. Expected: all strict matches, or a minimal exact mismatch per failing ID.

- [ ] **Step 5: Commit adapter and promoted corpus**

```bash
git add crates/hoimin-core/tests/lean_report_sequence_oracle.rs \
  formal/HoiminOracle/HoiminOracle/ReportSequenceCases.lean \
  formal/HoiminOracle/corpus/report-sequence.jsonl
git commit -m "test: compare report sequence behavior with Lean"
```

### Task 5: Reconcile mismatches and apply only required repairs

**Files:**
- Create if needed: `docs/superpowers/reports/2026-08-13-lean-report-sequence-counterexamples.md`
- Modify if confirmed bug: `crates/hoimin-core/tests/report_policy.rs`
- Modify if confirmed bug: `crates/hoimin-core/src/report.rs`
- Modify if model defect: Task 1-3 Lean files and regenerated corpus.

**Interfaces:**
- Consumes: Task 4 per-case observations.
- Produces: zero unresolved strict mismatches or an explicit owner decision; every bug has a Lean witness, Rust regression, and minimal fix.

- [ ] **Step 1: Classify before editing production**

For each non-match record claim, boundary, finite domain, minimal trace, intermediate states, expected/actual, differing fields, mode, source locations, impact, limitation, owner question, and reproduction. Use only `confirmed bug`, `specification ambiguity`, `model defect`, or `infrastructure error`.

- [ ] **Step 2: For a confirmed bug, write the focused Rust regression first**

Add one named `report_policy.rs` test replaying the exact Lean trace and asserting target error plus probe. Run only that test and require RED for the recorded semantic difference.

- [ ] **Step 3: Apply the smallest production repair**

Modify only the relevant predicate, validation order, or deferred mutation in `ReportSequence::observe`/`mutant_error`. Preserve public types and unrelated error text. Rerun regression, single-case oracle, full oracle, and `report_policy`.

- [ ] **Step 4: Handle other classifications without speculative Rust edits**

For a model defect, repair premises/observation, rerun proofs and sensitivity, regenerate, and re-review. For ambiguity, keep `model-only` and record the owner question. For infrastructure error, repair setup/observation and rerun; draw no semantic conclusion.

- [ ] **Step 5: Commit reconciliation independently**

If Rust changed, commit its regression, minimal fix, and counterexample ledger as `fix: preserve report sequence protocol`. If no strict mismatch exists, make no empty production commit and omit the ledger.

### Task 6: Audit report and complete verification

**Files:**
- Create: `docs/superpowers/reports/2026-08-13-lean-report-sequence-audit.md`
- Modify only if final evidence exposes a defect: files from Tasks 1-5.

**Interfaces:**
- Consumes: all proof, search, corpus, replay, reconciliation, and resource evidence.
- Produces: reproducible report and clean audit branch.

- [ ] **Step 1: Run final Lean evidence separately**

Run guarded model/proof builds, external proof consumer, sensitivity, cases, stats, and corpus freshness with separate `/tmp/report-sequence-*.json` statistics. Preserve guard failures as `infrastructure-error` and independently bound semantic commands.

- [ ] **Step 2: Run focused Rust verification**

```bash
cargo test -p hoimin-core --test lean_report_sequence_oracle -- --nocapture
cargo test -p hoimin-core --test report_policy
cargo test -p hoimin-core --all-features --test report_policy
```

Run every former mismatch separately via `HOIMIN_REPORT_SEQUENCE_CASE`.

- [ ] **Step 3: Run repository quality gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
git diff --check
```

Expected: all exit zero. Investigate unrelated failures systematically; do not mask them.

- [ ] **Step 4: Write the final report**

Include claim/boundary, declared/implicit behavior, worksheet, exact alphabet/depth/state/transition counts, elapsed/RSS, theorem names/premises, sensitivity matrix, mode counts, strict replay results, classifications, decisions, repair or explicit no-repair result, limitations, abandoned bounds, and exact commands. Separate Lean-model facts from Rust observations.

- [ ] **Step 5: Self-review documentation and evidence**

```bash
rg -n "T[B]D|T[O]DO|FIXME|implement lat[e]r|fill in" \
  docs/superpowers/reports/2026-08-13-lean-report-sequence-audit.md \
  docs/superpowers/plans/2026-08-13-lean-report-sequence-audit.md
rg -n "sorry|admit|axiom" formal/HoiminOracle/HoiminOracle/ReportSequence*.lean \
  formal/HoiminOracle/ReportSequenceAuditMain.lean
git diff --check
git status --short
```

Expected: no placeholders, no proof escape hatches, no whitespace errors, and only `.venv` plus the uncommitted report.

- [ ] **Step 6: Commit report and record final evidence**

```bash
git add docs/superpowers/reports/2026-08-13-lean-report-sequence-audit.md
git commit -m "docs: report Lean report sequence audit"
git status -sb
git log --oneline --decorate -10
```

Expected: clean tracked state on `audit/lean-report-sequence`; `.venv` remains untracked and unstaged.
