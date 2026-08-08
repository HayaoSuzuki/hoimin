# Lean Budget and Cleanup Formal Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reproducible Lean audit of workspace-copy budget reservations and atomic cleanup release, then compare Lean-generated expectations with Hoimin's public Rust accounting API without changing production behavior.

**Architecture:** Extend the pinned `formal/HoiminOracle` project with a separate budget namespace, proofs, bounded explorer, broken variants, and deterministic corpus generator. A dedicated Rust integration test consumes that corpus and translates events to `BudgetLedger`, `reserve_workspace_copy`, `release_workspace_copy`, and `WorkspaceCopyGrant`; the adapter reports semantic mismatches separately from infrastructure failures.

**Tech Stack:** Lean 4 with `Std` and `Lean.Data.Json`, Lake, Rust 2024, `serde`, `serde_json`, Cargo integration tests.

## Global Constraints

- Production code under `crates/*/src/` must not change during this audit.
- Bounded exploration covers budget kinds memory/copy/processes; limits and amounts `0`, `1`, `2`; cleanup shapes empty, singleton, two distinct IDs, duplicate IDs, released ID, and unknown ID; traces have length at most 6.
- Bounded search must be reported as a finite check, never as a proof.
- Lean theorems apply only to the model; implementation correspondence is established only by the Rust adapter observations.
- Every broken variant must be detected before corpus generation succeeds.
- Corpus expectations are generated only by Lean and must never be duplicated or recalculated in Rust.
- Every case runs independently and is classified as `match`, `mismatch`, or `infrastructure error`.
- A mismatch is preserved and reported; this audit must not repair production behavior.
- Existing `formal/HoiminOracle/corpus/state-machine.jsonl` and its adapter semantics must remain unchanged.

## File structure

- Create `formal/HoiminOracle/HoiminOracle/BudgetModel.lean`: accounting state, events, typed verdicts, correct and broken transitions, stable bounded explorer.
- Create `formal/HoiminOracle/HoiminOracle/BudgetProofs.lean`: preservation and reachability theorems.
- Create `formal/HoiminOracle/HoiminOracle/BudgetCases.lean`: strict cases and JSON rendering owned by Lean.
- Create `formal/HoiminOracle/BudgetMain.lean`: independent budget corpus output/check executable.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: expose the budget modules.
- Modify `formal/HoiminOracle/lakefile.toml`: register `generate_budget` without changing `generate`.
- Create `formal/HoiminOracle/corpus/budget-cleanup.jsonl`: generated expectations.
- Create `crates/hoimin-core/tests/lean_budget_oracle.rs`: public-API adapter.
- Create `docs/superpowers/reports/2026-08-09-lean-budget-cleanup-audit.md`: final report.
- Conditionally create `docs/superpowers/reports/2026-08-09-lean-budget-cleanup-counterexamples.md` only if a real mismatch remains.

---

### Task 1: Formal accounting model, bounded explorer, and theorems

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/BudgetModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/BudgetProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Produces: `BudgetAudit.Kind`, `Limits`, `Entry`, `State`, `Event`, `Rejection`, `Verdict`, `step`, `run`, `reachableUpTo`, `firstCounterexample?`, `auditDepth`, `alphabetSize`, `reachableStateCount`, `checkedTransitionCount`, and `brokenWitnessesDetected`.
- Produces theorems: `rejected_preserves_state`, `step_preserves_invariant`, `run_preserves_invariant`, `successful_release_is_exact`, and `reachable_is_safe`.
- Consumed by: Task 2 cases and renderer.

- [ ] **Step 1: Add model types and a deliberately false broken-transition example**

Create `BudgetModel.lean` with this public shape:

```lean
import Std

namespace HoiminOracle.BudgetAudit

inductive Kind | memory | copy | processes
  deriving Repr, DecidableEq

structure Limits where
  memory : Nat
  copy : Nat
  processes : Nat
  deriving Repr, DecidableEq

structure Entry where
  id : Nat
  kind : Kind
  amount : Nat
  deriving Repr, DecidableEq

structure State where
  limits : Limits
  active : List Entry
  released : List Nat
  nextId : Option Nat
  maxId : Nat
  deriving Repr, DecidableEq

inductive Event
  | reserve (kind : Kind) (amount : Nat)
  | release (ids : List Nat)
  deriving Repr, DecidableEq

inductive Rejection
  | limitReached | idsExhausted | duplicate | alreadyReleased | unknown
  deriving Repr, DecidableEq

structure Verdict where
  state : State
  rejection : Option Rejection
  allocated : Option Nat
  deriving Repr, DecidableEq
```

Define `safe : State -> Bool` as per-kind totals within limits, active/released disjointness, unique active IDs, and frontier validity. End the first edit with an example claiming that a partial-release broken transition preserves safety.

- [ ] **Step 2: Run Lean and observe RED**

Run:

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/BudgetModel.lean
```

Expected: FAIL at the deliberately false safety example.

- [ ] **Step 3: Implement correct reserve and atomic release**

Implement:

```lean
def total (state : State) (kind : Kind) : Nat
def available (state : State) (kind : Kind) : Nat
def validateRelease (state : State) (ids : List Nat) : Option Rejection
def reserve (state : State) (kind : Kind) (amount : Nat) : Verdict
def release (state : State) (ids : List Nat) : Verdict
def step (state : State) : Event -> Verdict
def run : State -> List Event -> State
```

`validateRelease` rejects duplicate input before released or unknown IDs. `release` validates before removing anything. `reserve` checks the per-kind limit before exhaustion, matching Rust precedence, and sets `nextId := none` after allocating `maxId`.

- [ ] **Step 4: Add stable bounded exploration and broken witnesses**

Use stable order: reserve memory/copy/processes, then cleanup shapes. Explore breadth-first while retaining the first shortest trace for each distinct semantic state. Check every outgoing event before deduplicating successor states; pruning a later trace that reaches an identical state is valid only for the state invariant and must be disclosed as the sole symmetry reduction. Fixed broken witnesses are never pruned. Define:

```lean
def semanticValues : List Nat := [0, 1, 2]
def eventAlphabet : List Event
structure Reachable where
  trace : List Event
  state : State
  deriving Repr, DecidableEq
def reachableUpTo (depth : Nat) : List Reachable
def firstCounterexample? (next : State -> Event -> Verdict)
    (initial : State) (depth : Nat) : Option (List Event)
def auditDepth : Nat := 6
def alphabetSize : Nat := eventAlphabet.length
def reachableStateCount : Nat := (reachableUpTo auditDepth).length
def checkedTransitionCount : Nat
```

Add:

```lean
def brokenPartialRelease : State -> Event -> Verdict
def brokenReuseReleasedId : State -> Event -> Verdict
def brokenCrossKindLimit : State -> Event -> Verdict

def brokenWitnessesDetected : Bool :=
  partialReleaseWitnessDetected && reuseWitnessDetected &&
    crossKindWitnessDetected

example : brokenWitnessesDetected = true := by decide
```

Each fixed witness must be minimal and include intermediate states through a named definition.

- [ ] **Step 5: Add explicit-premise theorems**

Create `BudgetProofs.lean` and prove:

```lean
theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true) :
    (step state event).state = state

theorem step_preserves_invariant (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event).state

theorem run_preserves_invariant (state : State) (trace : List Event)
    (holds : Invariant state) : Invariant (run state trace)

theorem successful_release_is_exact (state : State) (ids : List Nat)
    (accepted : (release state ids).rejection = none) :
    ExactRelease state ids (release state ids).state

theorem reachable_is_safe (trace : List Event) :
    safe (run auditInitial trace) = true
```

Keep `Invariant` and `ExactRelease` public. Put allocator frontier assumptions inside `Invariant`; do not infer production reachability from arbitrary states.

- [ ] **Step 6: Expose modules and run GREEN**

Add to `HoiminOracle.lean`:

```lean
import HoiminOracle.BudgetModel
import HoiminOracle.BudgetProofs
```

Run `cd formal/HoiminOracle && lake build`.

Expected: PASS, including all theorems and broken witnesses.

- [ ] **Step 7: Commit Task 1**

```bash
git add formal/HoiminOracle/HoiminOracle/BudgetModel.lean
git add formal/HoiminOracle/HoiminOracle/BudgetProofs.lean
git add formal/HoiminOracle/HoiminOracle.lean
git commit -m "test: model budget cleanup invariants in Lean"
```

---

### Task 2: Lean-owned deterministic corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/BudgetCases.lean`
- Create: `formal/HoiminOracle/BudgetMain.lean`
- Create: `formal/HoiminOracle/corpus/budget-cleanup.jsonl`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes Task 1's `step`, `Event`, `Verdict`, and `brokenWitnessesDetected`.
- Produces `BudgetAudit.renderCorpus : String` and `generate_budget --output PATH|--check PATH|--stats`.
- Corpus fields: `schema`, `id`, `mode`, `limits`, `max_id`, `schedule`, `expected`.
- Step fields: `event`, `verdict`, `error_code`, `allocated`, `active`, `released`, `totals`.

- [ ] **Step 1: Add fixed case specifications**

Cases cover independent kind limits, limit-before-exhaustion precedence, last-ID allocation and exhaustion, empty cleanup, singleton cleanup, reversed two-ID cleanup, duplicate cleanup, mixed active/unknown atomicity, mixed active/released atomicity, second cleanup rejection, zero-amount reservation, and cleanup after allocator exhaustion. Every expected step is derived by running Task 1's `step`.

- [ ] **Step 2: Observe missing-generator RED**

Run `cd formal/HoiminOracle && lake exe generate_budget -- --check corpus/budget-cleanup.jsonl`.

Expected: FAIL because `generate_budget` is not registered.

- [ ] **Step 3: Implement renderer and executable**

Define:

```lean
structure OracleStep
structure OracleCase
def cases : List OracleCase
def oracleStepJson : OracleStep -> Lean.Json
def oracleCaseJson : OracleCase -> Lean.Json
def renderCorpus : String
```

`BudgetMain.lean` follows existing `Main.lean` output/check behavior, imports `BudgetCases`, and refuses generation unless `brokenWitnessesDetected` is true. `--stats` must print machine-readable `depth=<n> alphabet=<n> states=<n> transitions=<n>` from Task 1 constants. Add:

```toml
[[lean_exe]]
name = "generate_budget"
root = "BudgetMain"
```

- [ ] **Step 4: Generate and freshness-check corpus**

```bash
cd formal/HoiminOracle
lake build
lake exe generate_budget -- --output corpus/budget-cleanup.jsonl
lake exe generate_budget -- --check corpus/budget-cleanup.jsonl
lake exe generate_budget -- --stats
git diff -- corpus/budget-cleanup.jsonl
```

Expected: PASS and deterministic JSON Lines in fixed case order.

- [ ] **Step 5: Commit Task 2**

```bash
git add formal/HoiminOracle/HoiminOracle/BudgetCases.lean
git add formal/HoiminOracle/BudgetMain.lean
git add formal/HoiminOracle/corpus/budget-cleanup.jsonl
git add formal/HoiminOracle/lakefile.toml
git commit -m "test: generate Lean budget cleanup corpus"
```

---

### Task 3: Public Rust API correspondence adapter

**Files:**
- Create: `crates/hoimin-core/tests/lean_budget_oracle.rs`

**Interfaces:**
- Consumes the generated corpus and public accounting APIs.
- Produces `corpus_is_well_formed`, `case_panics_are_infrastructure_errors`, `oracle_correspondence`, and filter `HOIMIN_BUDGET_ORACLE_CASE`.
- Normalizes typed verdict/error code, allocated ID, active entries, released IDs, and per-kind totals.

- [ ] **Step 1: Write a failing strict adapter shell**

Add corpus structs with `#[serde(deny_unknown_fields)]`, parse JSON Lines, and temporarily classify every selected case as `InfrastructureError("adapter not implemented")`.

- [ ] **Step 2: Observe RED**

Run `cargo test -p hoimin-core --test lean_budget_oracle -- --nocapture`.

Expected: FAIL because all strict cases are infrastructure errors.

- [ ] **Step 3: Implement the public-API driver**

For each isolated case:

1. create `BudgetLedger` from corpus limits;
2. translate reserve events to `BudgetLedger::reserve`;
3. retain model-role-to-real-`ReservationId` mappings only after successful allocation;
4. translate release roles into `CleanupFinished` and call `release_workspace_copy`;
5. derive stable codes from real `ReserveError` and `BudgetError` variants;
6. observe active state only through `reservation`, `reserved`, and IDs allocated during that case;
7. catch panics as infrastructure errors;
8. compare the complete observation vector with Lean expectations.

The adapter must not decide whether an event should succeed or calculate expected totals.

- [ ] **Step 4: Add parser and sensitivity guards**

Reject unknown schema, duplicate case IDs, unknown event/kind names, and schedule/expected length mismatch. Verify a deliberate panic becomes `InfrastructureError`, not `Mismatch`.

- [ ] **Step 5: Run GREEN and one case**

```bash
cargo test -p hoimin-core --test lean_budget_oracle -- --nocapture
HOIMIN_BUDGET_ORACLE_CASE=mixed_unknown_cleanup_is_atomic cargo test -p hoimin-core --test lean_budget_oracle oracle_correspondence -- --exact --nocapture
```

Expected: no infrastructure errors. Strict cases match or yield a stable mismatch listing exact fields.

- [ ] **Step 6: Commit Task 3**

```bash
git add crates/hoimin-core/tests/lean_budget_oracle.rs
git commit -m "test: compare budget cleanup with Lean oracle"
```

---

### Task 4: Reconcile and report

**Files:**
- Create: `docs/superpowers/reports/2026-08-09-lean-budget-cleanup-audit.md`
- Conditionally create: `docs/superpowers/reports/2026-08-09-lean-budget-cleanup-counterexamples.md`

**Interfaces:**
- Consumes formal output, explorer result, broken witnesses, corpus freshness, adapter output, and existing focused tests.
- Produces one self-contained report and, only for real mismatches, the required counterexample ledger.

- [ ] **Step 1: Execute formal gates**

```bash
cd formal/HoiminOracle
lake build
lake exe generate_budget -- --check corpus/budget-cleanup.jsonl
lake exe generate_budget -- --stats
```

Record depth `6`, printed alphabet size, reachable-state count, checked-transition count, the identical-state shortest-trace reduction, theorem names, and all three broken-witness results.

- [ ] **Step 2: Execute correspondence and focused tests**

```bash
cargo test -p hoimin-core --test lean_budget_oracle -- --nocapture
cargo test -p hoimin-core --test budget
cargo test -p hoimin-cli --test workspace_recovery
```

Rerun every mismatch using `HOIMIN_BUDGET_ORACLE_CASE=<case-id>`. Classify panics, timeouts, setup, parse, and command failures as infrastructure errors.

- [ ] **Step 3: Classify divergences**

Choose exactly one: `confirmed bug`, `specification ambiguity`, `model defect`, or `infrastructure error`. Do not weaken Lean to mirror Rust. If the model is defective, correct it, regenerate, rerun broken witnesses, and rerun every correspondence case.

- [ ] **Step 4: Write report and conditional ledger**

The report has:

```markdown
# Lean Budget and Cleanup Formal Audit Report

## Result
## Claim and model boundary
## Declared intent versus implicit behavior
## Finite exploration
## What Lean established
## Broken-variant sensitivity
## Implementation correspondence
## Minimal witnesses
## Classification and impact
## Model and adapter limitations
## Owner decisions
## Exact reproduction commands
```

A real mismatch ledger uses:

```text
claim:
model boundary:
finite domain or theorem premises:
minimal input or trace:
intermediate states:
classification:
implementation correspondence:
owner question:
reproduction command:
```

- [ ] **Step 5: Check claims and commit**

```bash
rg -n "proved production|unbounded production|TBD|TODO" docs/superpowers/reports/2026-08-09-lean-budget-cleanup-audit.md
git diff --check
git add docs/superpowers/reports/2026-08-09-lean-budget-cleanup-audit.md
git commit -m "docs: report Lean budget cleanup audit [skip ci]"
```

If a real ledger exists, add it before committing.

---

### Task 5: Final audit verification

**Files:**
- Verify only; no production source file may change.

**Interfaces:**
- Consumes all audit deliverables.
- Produces reproducibility and scope evidence.

- [ ] **Step 1: Verify Lean and both corpora**

```bash
cd formal/HoiminOracle
lake build
lake exe generate -- --check corpus/state-machine.jsonl
lake exe generate_budget -- --check corpus/budget-cleanup.jsonl
```

- [ ] **Step 2: Verify Rust adapters and focused suites**

```bash
cargo test -p hoimin-core --test lean_oracle
cargo test -p hoimin-core --test lean_budget_oracle
cargo test -p hoimin-core --test budget
cargo test -p hoimin-cli --test workspace_recovery
```

- [ ] **Step 3: Verify format, lint, and scope**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check origin/main..HEAD
git status -sb
git diff --name-only origin/main..HEAD
```

Expected: all checks pass. Changed paths are limited to design, plan, formal model/proofs/cases/executable/corpus, Rust audit adapter, and report/ledger. No `crates/*/src/` file appears.

- [ ] **Step 4: Ensure CI is not skipped by the branch tip**

If the final commit contains `[skip ci]`, add:

```bash
git commit --allow-empty -m "ci: validate Lean budget cleanup audit"
```

- [ ] **Step 5: Report and stop**

Report Lean conclusions, explicit bounds, broken-witness detection, Rust match/mismatch counts, infrastructure errors, report path, and owner decisions. Do not repair production code. Stop after this focused audit; session/scheduler auditing is a separate follow-up.
