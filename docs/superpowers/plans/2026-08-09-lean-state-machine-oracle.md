# Lean State-Machine Oracle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Lean-generated executable oracle for Hoimin's effect-completion, stopping, cleanup, and final-report semantics; compare it with the public Rust state machine; and repair any confirmed mismatch with a retained regression.

**Architecture:** A dependency-free Lean 4 project models only the selected lifecycle slice, proves its durable invariants, and generates a deterministic JSONL schedule corpus. A Rust integration adapter reads that corpus, drives the real `RunState` through `transition`, and classifies each isolated case as match, mismatch, or infrastructure error without encoding expected values itself.

**Tech Stack:** Lean 4.32.2, Lake 5, Rust 1.88-compatible workspace code, Serde/serde_json, Cargo integration tests, Markdown investigation reports.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-complexity-audit-20260809` on branch `lean-complexity-audit-20260809`.
- Keep all design, plan, report, and counterexample documentation in this worktree.
- Lean proves properties of the model only; never claim that Lean proves the Rust implementation.
- Define expectations once in Lean; never duplicate expected observations in Rust fixtures or prompts.
- Run each corpus case in isolation and classify setup, parse, panic, timeout, and unexpected-exit failures as infrastructure errors.
- Begin new semantic cases in report mode; promote only reviewed, established cases to strict mode.
- Do not modify CI workflows in this change. Expose deterministic commands suitable for later CI adoption.
- Do not weaken the Lean model to match a Rust mismatch.
- Change production Rust only after a focused failing regression reproduces a confirmed mismatch.
- Preserve unrelated concurrent work; stage and commit only files named by the current task.

---

## File Structure

- Create `formal/HoiminOracle/lean-toolchain`: pin `leanprover/lean4:v4.32.2`.
- Create `formal/HoiminOracle/lakefile.toml`: define the library and corpus-generator executable.
- Create `formal/HoiminOracle/HoiminOracle.lean`: library import root.
- Create `formal/HoiminOracle/HoiminOracle/Model.lean`: lifecycle state, events, typed verdicts, and transition function.
- Create `formal/HoiminOracle/HoiminOracle/Proofs.lean`: invariant theorems and deliberately broken-model witnesses.
- Create `formal/HoiminOracle/HoiminOracle/Cases.lean`: cases defined once as Lean values and JSON encoding.
- Create `formal/HoiminOracle/Main.lean`: deterministic `generate` and `--check` command implementation.
- Generate `formal/HoiminOracle/corpus/state-machine.jsonl`: committed Lean-owned expectations.
- Create `crates/hoimin-core/tests/lean_oracle.rs`: corpus parser, real-state driver, normalization, classification, and single-case selection.
- Create `docs/superpowers/reports/2026-08-09-lean-state-machine-oracle.md`: self-contained result and reproduction report.
- Create `docs/superpowers/reports/2026-08-09-lean-state-machine-counterexamples.md`: mismatch and resolution ledger.
- Modify `docs/development.md`: local oracle commands and the formal-model/correspondence boundary.
- Modify production files only if a strict mismatch is confirmed; likely scope is `crates/hoimin-core/src/machine.rs` plus `crates/hoimin-core/tests/machine.rs`.

### Task 1: Establish the Lean model and executable examples

**Files:**
- Create: `formal/HoiminOracle/lean-toolchain`
- Create: `formal/HoiminOracle/lakefile.toml`
- Create: `formal/HoiminOracle/HoiminOracle.lean`
- Create: `formal/HoiminOracle/HoiminOracle/Model.lean`
- Create: `formal/HoiminOracle/HoiminOracle/Cases.lean`

**Interfaces:**
- Consumes: the durable claim and scope from `docs/superpowers/specs/2026-08-09-lean-state-machine-oracle-design.md`.
- Produces: `HoiminOracle.Model.step : State -> Event -> Verdict`, `HoiminOracle.Cases.cases : List OracleCase`, and stable semantic names shared by later generator and adapter tasks.

- [x] **Step 1: Pin the verified Lean toolchain and declare the project targets**

Write `formal/HoiminOracle/lean-toolchain` exactly as:

```text
leanprover/lean4:v4.32.2
```

Write `formal/HoiminOracle/lakefile.toml` with a `HoiminOracle` library and a `generate` executable rooted at `Main`. The project must use Lean's bundled `Lean.Data.Json`; do not add Mathlib or network-fetched dependencies.

- [x] **Step 2: Write executable examples before the model implementation**

In `HoiminOracle/Cases.lean`, state examples for these semantic schedules, importing the not-yet-created model definitions:

```lean
example : (step State.initial (.complete 99 .ordinary)).errorCode? =
    some "machine.effect.unknown" := by decide

example :
    let s := State.withPending 1 .ordinary
    (step s (.complete 1 .cleanup)).errorCode? =
      some "machine.effect.wrong_completion" := by decide

example :
    let s := State.withPending 1 .ordinary
    let accepted := (step s (.complete 1 .ordinary)).state
    (step accepted (.complete 1 .ordinary)).errorCode? =
      some "machine.effect.duplicate" := by decide

example :
    let s := State.withPending 1 .ordinary
    let stopped := (step s (.stop .cancelled)).state
    (step stopped (.complete 1 .ordinary)).errorCode? =
      some "machine.effect.retired" := by decide
```

- [x] **Step 3: Run the examples to observe Red**

Run:

```bash
cd formal/HoiminOracle
lake build
```

Expected: FAIL because `HoiminOracle.Model` and its declared types/functions do not yet exist.

- [x] **Step 4: Implement the minimal semantic model**

Define small, decidable types in `Model.lean`:

```lean
inductive Phase | running | cleaning | finalPending | finished
inductive EffectKind | ordinary | cleanup | finalOutput
inductive StopCause | deadline | cancelled
inductive Event | complete (id : Nat) (kind : EffectKind) | stop (cause : StopCause)
inductive Rejection | unknown | duplicate | retired | wrongCompletion

structure State where
  phase : Phase
  pending : List (Nat × EffectKind)
  completed : List Nat
  retired : List Nat
  stopCause : Option StopCause
  cleanupEmitted : Bool
  finalEmitted : Bool
  acceptedResults : Nat

structure Verdict where
  state : State
  emitted : List EffectKind
  rejection : Option Rejection
```

Implement `step` with transactional rejection, first-stop retention, pending-effect retirement,
cleanup-before-final ordering, and late-stop idempotence. Provide `errorCode?` with exact
`machine.effect.*` strings corresponding to `MachineError::code()`.

- [x] **Step 5: Build and run all Lean examples**

Run:

```bash
cd formal/HoiminOracle
lake build
```

Expected: PASS with all four examples checked by `decide`.

- [x] **Step 6: Commit the executable model slice**

```bash
git add formal/HoiminOracle/lean-toolchain formal/HoiminOracle/lakefile.toml formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle/Model.lean formal/HoiminOracle/HoiminOracle/Cases.lean
git commit -m "test: model Hoimin lifecycle semantics in Lean"
```

### Task 2: Prove invariants and validate broken witnesses

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/Proofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/HoiminOracle/Cases.lean`

**Interfaces:**
- Consumes: `State`, `Event`, `Verdict`, and `step` from Task 1.
- Produces: named theorems for review and `brokenWitnessesDetected : Bool`, which Task 3 requires before corpus generation.

- [x] **Step 1: State the invariant theorems before adding supporting lemmas**

Add theorem statements covering:

```lean
theorem rejected_preserves_state (s : State) (e : Event)
    (h : (step s e).rejection.isSome = true) : (step s e).state = s

theorem accepted_completion_not_pending (s : State) (id : Nat) (kind : EffectKind)
    (h : (step s (.complete id kind)).rejection = none) :
    id ∉ (step s (.complete id kind)).state.pending.map Prod.fst

theorem stop_is_first_writer_wins (s : State) (a b : StopCause)
    (h : s.stopCause = none) :
    (step (step s (.stop a)).state (.stop b)).state.stopCause = some a

theorem late_stop_preserves_final (s : State) (cause : StopCause)
    (h : s.phase = .finalPending ∨ s.phase = .finished) :
    (step s (.stop cause)).state = s

theorem no_ordinary_emission_after_stop (s : State) (e : Event)
    (h : s.stopCause.isSome = true) :
    .ordinary ∉ (step s e).emitted
```

- [x] **Step 2: Run Lean to observe proof obligations**

Run `cd formal/HoiminOracle && lake build`.

Expected: FAIL on unproved theorem bodies, demonstrating the properties are active obligations.

- [x] **Step 3: Prove the invariants from the transition definition**

Use case analysis and simplification over `step`, extracting repeated membership facts into small
private lemmas. Do not add model behavior solely to shorten proofs. Import `Proofs` from the library
root so `lake build` always checks every theorem.

- [x] **Step 4: Add deliberately broken-model witnesses**

In `Cases.lean`, define local broken variants that each remove one rule:

- `brokenAcceptDuplicate` accepts an already-completed effect;
- `brokenLateStop` overwrites the outcome after final output is pending;
- `brokenStopScheduling` emits ordinary work after a stop.

Define fixed witness schedules and assert that the correct expected observation differs from each
broken variant. Export:

```lean
def brokenWitnessesDetected : Bool :=
  duplicateWitnessDetected && lateStopWitnessDetected && stopSchedulingWitnessDetected

example : brokenWitnessesDetected = true := by decide
```

- [x] **Step 5: Rebuild and confirm proofs and witnesses pass**

Run `cd formal/HoiminOracle && lake build`.

Expected: PASS; all named theorems and the `brokenWitnessesDetected = true` example are checked.

- [x] **Step 6: Commit proofs and sensitivity witnesses**

```bash
git add formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle/Proofs.lean formal/HoiminOracle/HoiminOracle/Cases.lean
git commit -m "test: prove lifecycle oracle invariants"
```

### Task 3: Generate and freshness-check the versioned corpus

**Files:**
- Create: `formal/HoiminOracle/Main.lean`
- Create: `formal/HoiminOracle/corpus/state-machine.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle/Cases.lean`

**Interfaces:**
- Consumes: `cases : List OracleCase` and `brokenWitnessesDetected` from Tasks 1-2.
- Produces: `lake exe generate -- --output corpus/state-machine.jsonl` and `lake exe generate -- --check corpus/state-machine.jsonl`; the JSONL schema consumed by Task 4.

- [x] **Step 1: Define the corpus schema as Lean structures**

Define JSON-serializable values with stable string encodings:

```lean
structure OracleStep where
  event : String
  verdict : String
  errorCode : Option String
  phase : String
  emitted : List String
  pending : Nat
  acceptedResults : Nat

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : String
  scenario : String
  schedule : List String
  expected : List OracleStep
```

Use explicit `Lean.Json.mkObj` field order rather than relying on map iteration or an external JSON
formatter. Populate at least these report-mode cases:

- `unknown_completion_is_rejected`;
- `wrong_kind_is_transactional`;
- `duplicate_completion_is_rejected`;
- `retired_completion_after_cancel_is_rejected`;
- `deadline_stops_ordinary_scheduling`;
- `cancel_stops_ordinary_scheduling`;
- `cleanup_precedes_final_output`;
- `cleanup_is_emitted_once`;
- `final_output_is_emitted_once`;
- `deadline_after_final_pending_is_noop`;
- `cancel_after_final_pending_is_noop`;
- `deadline_after_finished_is_noop`;
- `cancel_after_finished_is_noop`.

- [x] **Step 2: Add the generator with an intentionally failing freshness check**

Implement `Main.lean` to reject generation unless `brokenWitnessesDetected` is true, render one
compact JSON object plus newline per case, and support:

```text
--output PATH   write generated bytes to PATH
--check PATH    compare generated bytes with PATH; exit nonzero on missing/stale content
```

Run `cd formal/HoiminOracle && lake exe generate -- --check corpus/state-machine.jsonl`.

Expected: FAIL because the committed corpus does not exist.

- [x] **Step 3: Generate the corpus only from Lean**

Run:

```bash
cd formal/HoiminOracle
lake exe generate -- --output corpus/state-machine.jsonl
lake exe generate -- --check corpus/state-machine.jsonl
```

The generator creates the parent directory when it is absent. Expected: both generation and
freshness check succeed; a second generation produces no diff.

- [x] **Step 4: Inspect semantic coverage and deterministic bytes**

Run:

```bash
cd formal/HoiminOracle
wc -l corpus/state-machine.jsonl
lake exe generate -- --output /tmp/hoimin-state-machine-oracle.jsonl
cmp corpus/state-machine.jsonl /tmp/hoimin-state-machine-oracle.jsonl
```

Expected: 13 lines and `cmp` exit 0. Do not hand-edit either file.

- [x] **Step 5: Commit generator and generated expectations**

```bash
git add formal/HoiminOracle/Main.lean formal/HoiminOracle/HoiminOracle/Cases.lean formal/HoiminOracle/corpus/state-machine.jsonl
git commit -m "test: generate Lean lifecycle oracle corpus"
```

### Task 4: Drive the public Rust state machine from the corpus

**Files:**
- Create: `crates/hoimin-core/tests/lean_oracle.rs`

**Interfaces:**
- Consumes: schema-1 JSONL cases from `formal/HoiminOracle/corpus/state-machine.jsonl`.
- Produces: `run_case(case: &OracleCase) -> CaseResult`, `CaseClass::{Match, Mismatch, InfrastructureError}`, and an optional `HOIMIN_ORACLE_CASE=cancel_after_final_pending_is_noop` single-case filter.

- [x] **Step 1: Add corpus parsing and schema-validation tests**

Create `lean_oracle.rs` with Serde input structures mirroring the Lean-owned JSON shape. Add tests
that read the committed corpus through `env!("CARGO_MANIFEST_DIR")`, reject a schema other than 1,
reject duplicate case IDs, reject an unknown mode/scenario/event/observation string, and assert no
expected list is empty.

Do not write Rust constants for expected phases, verdicts, emissions, pending counts, or errors.

- [x] **Step 2: Run parser tests to observe Red**

Run:

```bash
cargo test -p hoimin-core --test lean_oracle corpus_is_well_formed -- --exact
```

Expected: FAIL because the parser and scenario types are not yet implemented.

- [x] **Step 3: Implement stable observations and case classification**

Define adapter-only observation types:

```rust
#[derive(Debug, Eq, PartialEq)]
enum CaseClass {
    Match,
    Mismatch,
    InfrastructureError,
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    verdict: String,
    error_code: Option<String>,
    phase: String,
    emitted: Vec<String>,
    pending: usize,
    accepted_results: u64,
}
```

Normalize `RunPhase`, `RunEffect` variant names, `MachineError::code()`, `pending_count()`, and the
case driver's accepted-result counter. Parsing and driver-construction failures return
`InfrastructureError`; a successful execution with differing fields returns `Mismatch` with every
differing field rendered. Because `transition` consumes `RunState` and returns no state on error,
the rejected-event observation uses the stable pre-event snapshot; no rejected mutation can escape
through the public API.

- [x] **Step 4: Implement real public-state scenario drivers**

Use only `RunState::new`, public event/effect types, effect IDs emitted by `transition`, and public
state accessors. Implement these drivers:

- `pending_resolve`: start a fixture state and retain the emitted `ResolveTargets` ID;
- `retired_resolve`: start, stop, then retain the retired resolve ID;
- `cleaning_without_copy`: stop before preflight, acknowledge `RunStarted`, and reach cleanup;
- `final_pending_without_copy`: complete that cleanup and retain the emitted `RunFinished` ID;
- `finished_without_copy`: acknowledge that real final-output effect.

The fixture `RawRunConfig` must use one job, one process, one selected source target, finite nonzero
limits, JSONL output, no session, and a harmless native test command. Every state is reached by
calling `transition`; never expose or mutate private `RunState` fields.

- [x] **Step 5: Run the full adapter in report mode**

Run:

```bash
cargo test -p hoimin-core --test lean_oracle -- --nocapture
```

Expected: the test process completes and prints one `match`, `mismatch`, or `infrastructure error`
record per report-mode case. Report-mode semantic mismatches are printed and retained but do not
yet make the test fail. Infrastructure errors always fail the test.

- [x] **Step 6: Verify single-case reproduction**

Run:

```bash
HOIMIN_ORACLE_CASE=cancel_after_final_pending_is_noop \
  cargo test -p hoimin-core --test lean_oracle oracle_correspondence -- --exact --nocapture
```

Expected: exactly one case is executed and its complete expected/actual comparison is printed.

- [x] **Step 7: Commit the correspondence adapter**

```bash
git add crates/hoimin-core/tests/lean_oracle.rs
git commit -m "test: compare Rust machine with Lean oracle"
```

### Task 5: Audit mismatches and promote established semantics

**Files:**
- Modify: `formal/HoiminOracle/HoiminOracle/Cases.lean`
- Modify: `formal/HoiminOracle/corpus/state-machine.jsonl` (regenerate only)
- Modify: `crates/hoimin-core/tests/lean_oracle.rs`
- Create: `docs/superpowers/reports/2026-08-09-lean-state-machine-counterexamples.md`

**Interfaces:**
- Consumes: complete report-mode adapter output and single-case reproduction from Task 4.
- Produces: a reviewed counterexample ledger and a strict corpus containing every established matching case.

- [ ] **Step 1: Capture complete report-mode evidence**

Run:

```bash
cargo test -p hoimin-core --test lean_oracle oracle_correspondence -- --exact --nocapture
```

For every non-match, assign its exact ledger ID to a task-specific shell variable and run the
single-case reproduction:

```bash
ORACLE_CASE_ID=cancel_after_final_pending_is_noop
HOIMIN_ORACLE_CASE="$ORACLE_CASE_ID" \
  cargo test -p hoimin-core --test lean_oracle oracle_correspondence -- --exact --nocapture
```

Replace the example value with the exact non-match ID, then record the smallest schedule, Lean
expectation, Rust observation, differing fields, source locations, impact, and model limitation.

- [ ] **Step 2: Classify every non-match without changing either implementation**

Create the counterexample ledger with this exact header:

```markdown
| Case | Classification | Expected | Actual | Impact | Model limitation | Decision |
| --- | --- | --- | --- | --- | --- | --- |
```

Use only `confirmed bug`, `specification ambiguity`, `model defect`, or `infrastructure error`.
Correct a model defect only when the abstraction contradicts the approved design or real public
contract; never change Lean merely because Rust differs.

- [ ] **Step 3: Promote reviewed matches to strict mode in Lean**

Change the `mode` field from `report` to `strict` in the Lean case definitions for every case whose
claim is established by the approved design and existing public tests. Regenerate, never edit, the
corpus:

```bash
cd formal/HoiminOracle
lake exe generate -- --output corpus/state-machine.jsonl
lake exe generate -- --check corpus/state-machine.jsonl
```

- [ ] **Step 4: Make strict mismatches blocking**

Update `oracle_correspondence` so `CaseClass::Mismatch` fails only for corpus cases whose Lean-owned
mode is `strict`; report cases continue to print a nonblocking mismatch. Run the adapter and expect
all established matching cases to pass while confirmed bugs fail by case ID.

- [ ] **Step 5: Commit the reviewed baseline and ledger**

```bash
git add formal/HoiminOracle/HoiminOracle/Cases.lean formal/HoiminOracle/corpus/state-machine.jsonl crates/hoimin-core/tests/lean_oracle.rs docs/superpowers/reports/2026-08-09-lean-state-machine-counterexamples.md
git commit -m "test: establish strict Lean correspondence cases"
```

### Task 6: Repair each confirmed production mismatch with TDD

**Files:**
- Modify when evidence requires: `crates/hoimin-core/tests/machine.rs`
- Modify when evidence requires: `crates/hoimin-core/src/machine.rs`
- Modify: `docs/superpowers/reports/2026-08-09-lean-state-machine-counterexamples.md`

**Interfaces:**
- Consumes: only ledger rows classified `confirmed bug` with a strict single-case reproduction.
- Produces: a focused Rust regression per confirmed bug, the smallest production repair, and a strict passing retained corpus case.

- [ ] **Step 1: Invoke systematic debugging for the first confirmed mismatch**

Trace the exact event schedule through `transition`, `accept_completion`, `retire_pending`,
`cleanup_effects`, `post_cleanup_effects`, and `final_report_effects` as applicable. Identify the
first Rust transition whose stable observation differs from Lean. Record that source line and state
precondition in the ledger. If there are no `confirmed bug` rows, skip Steps 2-6 and state explicitly
in the ledger that production code was unchanged.

- [ ] **Step 2: Add one focused failing Rust regression**

Derive a valid Rust test name by replacing hyphens in the exact ledger case ID with underscores and
prefixing `lean_oracle_regression_`; for example,
`lean_oracle_regression_cleanup_precedes_final_output`. Add it to
`crates/hoimin-core/tests/machine.rs`, recreate the same schedule through public transitions, assert
the stable invariant represented by the corpus case, and add
`// pins: lean oracle cleanup_precedes_final_output` immediately before the non-obvious
former-defect assertion, substituting the exact ledger ID when the failing case differs.

- [ ] **Step 3: Run the regression and strict single-case adapter to observe Red**

Run:

```bash
ORACLE_CASE_ID=cleanup_precedes_final_output
ORACLE_TEST_NAME=lean_oracle_regression_cleanup_precedes_final_output
cargo test -p hoimin-core --test machine "$ORACLE_TEST_NAME" -- --exact
HOIMIN_ORACLE_CASE="$ORACLE_CASE_ID" \
  cargo test -p hoimin-core --test lean_oracle oracle_correspondence -- --exact --nocapture
```

Replace both example assignments with the exact confirmed ledger row and its mechanically derived
test name before running the commands.

Expected: both fail for the same semantic difference, not for setup or parsing.

- [ ] **Step 4: Implement the smallest state-machine correction**

Change only the transition guard, retirement ordering, cleanup/final scheduling guard, or typed
validation responsible for the first divergence. Preserve effect IDs, public error codes, unrelated
phase behavior, and the original first stop cause. Do not refactor the 2,000-line transition table
unless the confirmed defect cannot be corrected locally.

- [ ] **Step 5: Run Green and nearby state-machine coverage**

Run the two commands from Step 3, followed by:

```bash
cargo test -p hoimin-core --test machine
cargo test -p hoimin-core --test lean_oracle
```

Expected: PASS; the retained strict corpus case now matches and all existing machine schedules pass.

- [ ] **Step 6: Record resolution and commit one bug at a time**

Update the ledger row with the root cause, production source location, regression name, and resolved
status. Commit only that repair:

```bash
git add crates/hoimin-core/src/machine.rs crates/hoimin-core/tests/machine.rs docs/superpowers/reports/2026-08-09-lean-state-machine-counterexamples.md
git commit -m "fix: restore Lean oracle state-machine invariant"
```

Repeat Tasks 6.1-6.6 independently for each remaining confirmed bug. Do not combine unrelated
mismatches in one production commit.

### Task 7: Document commands, results, and future CI boundary

**Files:**
- Modify: `docs/development.md`
- Create: `docs/superpowers/reports/2026-08-09-lean-state-machine-oracle.md`

**Interfaces:**
- Consumes: proof names, final corpus counts/modes, adapter classifications, ledger decisions, and any production fixes.
- Produces: a self-contained handoff and exact local commands suitable for future CI wiring.

- [ ] **Step 1: Add the local oracle workflow to development documentation**

Document these commands in order:

```bash
cd formal/HoiminOracle && lake build
cd formal/HoiminOracle && lake exe generate -- --check corpus/state-machine.jsonl
cargo test -p hoimin-core --test lean_oracle
HOIMIN_ORACLE_CASE=cancel_after_final_pending_is_noop cargo test -p hoimin-core --test lean_oracle oracle_correspondence -- --exact --nocapture
```

State that Lean proves the model, the adapter checks correspondence, report cases are exploratory,
strict cases are blocking, and CI integration is future work.

- [ ] **Step 2: Write the self-contained investigation report**

Include these sections:

- Scope and durable claim.
- What Lean established inside the model, naming each theorem.
- What the adapter observed in the real implementation.
- Match, mismatch, and infrastructure-error counts.
- Confirmed bugs and repairs, or an explicit statement that no production mismatch was confirmed.
- Counterexample ledger link.
- Model limitations and unresolved ownership decisions.
- Exact reproduction and full-verification commands.
- Future CI boundary without editing `.github/workflows`.

- [ ] **Step 3: Check documentation contracts and links**

Run:

```bash
rg -n "Lean|oracle|corpus|strict|report mode|CI" docs/development.md docs/superpowers/reports/2026-08-09-lean-state-machine-oracle.md
git diff --check
```

Expected: every required boundary and command appears; no whitespace errors or broken relative paths.

- [ ] **Step 4: Commit the handoff documentation**

```bash
git add docs/development.md docs/superpowers/reports/2026-08-09-lean-state-machine-oracle.md
git commit -m "docs: report Lean state-machine oracle results"
```

### Task 8: Run the final verification and review gate

**Files:**
- Modify only if verification exposes a scoped defect: files already named in Tasks 1-7.

**Interfaces:**
- Consumes: the completed model, corpus, adapter, ledger, optional fixes, and documentation.
- Produces: fresh evidence that the branch is ready for handoff.

- [ ] **Step 1: Verify Lean, witnesses, and corpus freshness**

Run:

```bash
cd formal/HoiminOracle
lake build
lake exe generate -- --check corpus/state-machine.jsonl
```

Expected: PASS with no generated diff.

- [ ] **Step 2: Verify strict and single-case correspondence**

Run:

```bash
cargo test -p hoimin-core --test lean_oracle
```

Then reproduce every ledger mismatch individually with `HOIMIN_ORACLE_CASE`. Expected: all strict
cases pass; unresolved report-mode cases remain documented and nonblocking; no infrastructure errors.

- [ ] **Step 3: Run Rust formatting and lint gates**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: PASS with zero warnings.

- [ ] **Step 4: Run the complete Rust test baseline**

Ensure the worktree-local untracked `.venv` symlink still points to the repository's controlled
environment, then run:

```bash
cargo test --workspace
```

Expected: PASS with the same intentional ignored benchmarks/fixtures as baseline and no failures.

- [ ] **Step 5: Audit the branch diff and worktree isolation**

Run:

```bash
git diff --check origin/main...HEAD
git status -sb
git log --oneline --decorate origin/main..HEAD
```

Expected: only scoped formal assets, Rust adapter/tests or confirmed repair, and worktree-contained
documentation are committed. The local `.venv` symlink remains untracked and is not staged.

- [ ] **Step 6: Request code review and resolve findings**

Use `superpowers:requesting-code-review` against the complete `origin/main...HEAD` diff. Address
only verified findings, rerun the relevant focused gate after each change, and then rerun Steps 1-5
before claiming completion.
