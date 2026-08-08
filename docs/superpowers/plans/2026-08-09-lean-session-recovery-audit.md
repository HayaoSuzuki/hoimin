# Lean Session Persistence, Recovery, and Ownership Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reproducible Lean audit of session persistence, resume, result replacement, and live ownership, then compare Lean-generated expectations with `SessionHandler` under the same public-API premises without changing production behavior.

**Architecture:** Extend the pinned `formal/HoiminOracle` project with an independent `SessionAudit` namespace containing a finite durable-state/ownership model, explicit-premise proofs, broken transitions, deterministic strict cases, and an executable-only bounded explorer. A CLI integration test maps semantic roles to real `SessionHandler` objects, UUID-like run strings, fingerprints, mutants, and an isolated SQLite database; existing subprocess and contention fixtures supply internal-fixture evidence that cannot be represented as strict corpus replay.

**Tech Stack:** Lean 4 with `Std` and `Lean.Data.Json`, Lake, Rust 2024, SQLite through `rusqlite`, filesystem locks through the existing session ownership backend, `serde`, `serde_json`, Cargo integration tests.

## Global Constraints

- Production code under `crates/*/src/` must not change during this audit.
- The operational premise is a successfully opened current valid schema; schema migration, future schemas, and deliberately corrupt database rows are excluded.
- The finite model uses exactly two handler roles, two run roles, two fingerprint roles, two mutant roles, and all seven `MutationStatus` classes.
- Default exhaustive exploration has trace depth at most 8 and must be reported as bounded exploration, never as a proof of the Rust implementation.
- Imported model/proof modules must remain cheap; large `native_decide`, breadth-first exploration, shrinking, statistics, and corpus output belong only to the non-imported `SessionAuditMain.lean` executable.
- Correspondence modes are exactly `strict`, `model-only`, `internal-fixture`, and `infrastructure-error`; do not introduce or accept a `report` mode.
- The generated JSONL corpus contains only `strict` cases. Different-premise observations must not be labeled mismatches.
- Lean owns expected observations. The Rust adapter may translate roles and observe real return values/database rows, but must not recompute whether an event ought to succeed.
- A panic, setup error, SQLite observation failure, corpus parse failure, or subprocess launch failure is `infrastructure-error`, never a semantic mismatch.
- Each deliberately broken family—atomicity/transactionality, uniqueness/idempotency, and boundary/precedence—must yield a stable minimal witness before corpus generation succeeds.
- A real strict mismatch must remain reproducible and be classified as `confirmed bug`, `specification ambiguity`, `model defect`, or `infrastructure error`; production code is not repaired in this change.
- Existing `state-machine.jsonl`, `budget-cleanup.jsonl`, their generators, and their adapter semantics must remain unchanged.

## File structure

- Create `formal/HoiminOracle/HoiminOracle/SessionModel.lean`: finite roles, durable rows, ownership, public/internal events, observations, correct transition, and three broken transitions.
- Create `formal/HoiminOracle/HoiminOracle/SessionProofs.lean`: explicit-premise local and arbitrary-trace invariant theorems.
- Create `formal/HoiminOracle/HoiminOracle/SessionCases.lean`: named strict schedules and Lean-owned JSON encoding.
- Create `formal/HoiminOracle/SessionAuditMain.lean`: bounded BFS, shortest counterexamples, sensitivity gate, stats, corpus output, and corpus freshness checking.
- Modify `formal/HoiminOracle/HoiminOracle.lean`: expose only `SessionModel`, `SessionProofs`, and `SessionCases`; do not import `SessionAuditMain`.
- Modify `formal/HoiminOracle/lakefile.toml`: register `generate_session` without changing existing executables.
- Create `formal/HoiminOracle/corpus/session-recovery.jsonl`: deterministic strict expectations.
- Create `crates/hoimin-cli/tests/lean_session_oracle.rs`: strict public-API correspondence adapter and corpus validation.
- Create `docs/superpowers/reports/2026-08-09-lean-session-recovery-audit.md`: final audit report.
- Conditionally create `docs/superpowers/reports/2026-08-09-lean-session-recovery-counterexamples.md` only if a real strict mismatch remains.

---

### Task 1: Finite session model and broken transitions

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/SessionModel.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Produces roles `Handler`, `Run`, `Fingerprint`, `Mutant`, and `Payload` with two constructors each.
- Produces `Status`, `HandlerState`, `RunRow`, `ResultRow`, `Pending`, `State`, `Event`, `Rejection`, `Observation`, and `Verdict`.
- Produces `State.initial`, `safe`, `Invariant`, `step`, `run`, `publicEvents`, and stable rendering helpers consumed by Tasks 2 and 3.
- Produces broken transitions `brokenAtomicityStep`, `brokenUniquenessStep`, and `brokenBoundaryStep` plus fixed witness traces consumed by Task 3.

- [ ] **Step 1: Add the finite types and an intentionally failing invariant example**

Create `SessionModel.lean` with this public shape:

```lean
import Std

namespace HoiminOracle.SessionAudit

inductive Handler | h0 | h1 deriving Repr, DecidableEq
inductive Run | r0 | r1 deriving Repr, DecidableEq
inductive Fingerprint | f0 | f1 deriving Repr, DecidableEq
inductive Mutant | m0 | m1 deriving Repr, DecidableEq
inductive Payload | p0 | p1 deriving Repr, DecidableEq

inductive Status
  | killed | survived | timeout | outOfMemory
  | processLimit | error | notRun
  deriving Repr, DecidableEq

inductive HandlerState | closed | live deriving Repr, DecidableEq

structure RunRow where
  run : Run
  fingerprint : Fingerprint
  ordinal : Nat
  finished : Bool
  complete : Bool
  deriving Repr, DecidableEq

structure ResultRow where
  run : Run
  mutant : Mutant
  status : Status
  payload : Payload
  deriving Repr, DecidableEq

inductive Pending
  | loadCandidate (handler : Handler) (fingerprint : Fingerprint) (run : Run)
  | loadLocked (handler : Handler) (fingerprint : Fingerprint) (run : Run)
  | replacing (handler : Handler) (old : ResultRow) (replacement : ResultRow)
  deriving Repr, DecidableEq

structure State where
  handlers : List (Handler × HandlerState)
  runs : List RunRow
  results : List ResultRow
  owners : List (Run × Handler)
  nextOrdinal : Nat
  pending : Option Pending
  deriving Repr, DecidableEq
```

Use a list for `owners` so the uniqueness invariant is substantive rather than
guaranteed by an `Option` representation. Add helpers for lookup/update,
determinate status, resume eligibility, latest compatible run, durable
projection, and owner counts. End the first edit with an `example` that falsely
claims a state with both `(r0, h0)` and `(r0, h1)` is safe.

- [ ] **Step 2: Run Lean and observe RED**

Run:

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/SessionModel.lean
```

Expected: FAIL at the double-owner safety example.

- [ ] **Step 3: Define public and internal events with stable rejection codes**

Define:

```lean
inductive PersistValidity | valid | invalidDiagnostic
  deriving Repr, DecidableEq

inductive Event
  | open (handler : Handler)
  | begin (handler : Handler) (run : Run) (fingerprint : Fingerprint)
  | load (handler : Handler) (fingerprint : Fingerprint)
  | lookup (handler : Handler) (run : Run) (mutant : Mutant)
  | persist (handler : Handler) (run : Run) (mutant : Mutant)
      (status : Status) (payload : Payload) (validity : PersistValidity)
  | finish (handler : Handler) (run : Run) (complete : Bool)
  | drop (handler : Handler)
  | crash (handler : Handler)
  | loadReadCandidate (handler : Handler) (fingerprint : Fingerprint)
  | loadAcquire (handler : Handler)
  | loadRecheck (handler : Handler)
  | replacementDelete (handler : Handler) (run : Run) (mutant : Mutant)
      (status : Status) (payload : Payload)
  | replacementCommit (handler : Handler)
  | replacementRollback (handler : Handler)
  deriving Repr, DecidableEq

inductive Rejection
  | handlerClosed | duplicateRun | active | missingRun | completeRun
  | duplicateResult | invalidDiagnostic | noCandidate | internalState
  deriving Repr, DecidableEq
```

Map public rejections to the stable Rust codes actually compared by the
adapter: `session.begin`, `session.resume.active`, `session.lookup.state`,
`session.lookup.complete`, `session.persist.complete`,
`session.duplicate_result`, `session.persist.diagnostic`, and
`session.finish.state`. Internal-only rejections use model names and never enter
the strict corpus.

- [ ] **Step 4: Implement the correct transition**

Implement:

```lean
def safe (state : State) : Bool
def Invariant (state : State) : Prop := safe state = true
def durable (state : State) : List RunRow × List ResultRow
def SameExceptResult (before after : State) (replacement : ResultRow) : Prop
def observe (event : Event) (state : State)
    (rejection : Option Rejection) (selected : Option Run)
    (stored : Option ResultRow) : Observation
def step (state : State) (event : Event) : Verdict
def run : State → List Event → State
```

`safe` checks unique/live owners, no owner for complete runs, unique run roles,
unique `(run, mutant)` results, every result's run existence, and consistent
pending-operation references. Public `load` atomically performs read, acquire,
and eligibility recheck. The split `loadAcquire` event records its transient
lock only in `pending`; it does not add a logical owner until `loadRecheck`
confirms eligibility. `persist` rejects complete runs and determinate
replacement, replaces inconclusive rows only on a valid payload, and preserves
the complete durable projection on every rejection. Incomplete finish sets
`finished = true`, keeps `complete = false`, and releases ownership; repeating
it remains accepted. Complete finish releases ownership and makes every later
load/lookup/persist/finish reject or return no candidate as appropriate.

The correct `replacementDelete` event records tentative replacement work in
`pending` without modifying committed `results`. `replacementCommit` installs
the replacement atomically and `replacementRollback` restores the unchanged
durable projection. Only `brokenAtomicityStep` exposes the premature deletion
in committed state.

- [ ] **Step 5: Add the three broken families and minimal named witnesses**

Define:

```lean
def brokenAtomicityStep : State → Event → Verdict
def brokenUniquenessStep : State → Event → Verdict
def brokenBoundaryStep : State → Event → Verdict

def atomicityWitness : List Event
def uniquenessWitness : List Event
def boundaryWitness : List Event
```

The atomicity witness opens `h0`, begins `r0`, persists timeout payload `p0`,
then attempts invalid-diagnostic replacement with killed payload `p1`; the
broken transition loses `p0`. The uniqueness witness opens both handlers,
begins `r0` through `h0`, then loads it through `h1`; the broken transition
records both owners. The boundary witness performs a split load read through
`h1`, completes `r0` through `h0`, acquires the released lock, and demonstrates
that the broken no-recheck transition returns the completed candidate.

- [ ] **Step 6: Remove the false example, expose the model, and run GREEN**

Add to `HoiminOracle.lean`:

```lean
import HoiminOracle.SessionModel
```

Run:

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/SessionModel.lean
lake build
```

Expected: PASS with no `sorry` or custom axioms.

- [ ] **Step 7: Commit Task 1**

```bash
git add formal/HoiminOracle/HoiminOracle/SessionModel.lean
git add formal/HoiminOracle/HoiminOracle.lean
git commit -m "test: model session recovery and ownership in Lean"
```

---

### Task 2: Explicit-premise Lean proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/SessionProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Consumes Task 1's correct transition and invariant helpers.
- Produces `rejected_preserves_durable`, `completed_has_no_owner`, `determinate_is_immutable`, `successful_replacement_is_exact`, `failed_replacement_restores_old`, `drop_releases_exactly_handler`, `step_preserves_invariant`, and `run_preserves_invariant`.
- Proof modules must not import `SessionAuditMain` or execute the breadth-first explorer.

- [ ] **Step 1: Add the proof module with an intentionally false check**

Create `SessionProofs.lean`, import only `SessionModel`, and add this executable
false claim before writing the real theorems:

```lean
example : safe State.initial = false := by decide
```

- [ ] **Step 2: Run Lean and observe RED**

Run:

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/SessionProofs.lean
```

Expected: FAIL because the initial state satisfies `safe`.

- [ ] **Step 3: Remove the false check and prove rejection, completion, and ownership properties**

Replace the placeholder with proofs for:

```lean
theorem rejected_preserves_durable (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true)
    (publicEvent : event.isPublic = true) :
    durable (step state event).state = durable state
theorem completed_has_no_owner (state : State) (run : Run)
    (holds : Invariant state) (complete : runComplete state run = true) :
    ownerCount state run = 0
theorem drop_releases_exactly_handler (state : State) (handler : Handler) :
    ownerRuns (step state (.drop handler)).state handler = []
```

Use explicit premises instead of asserting that arbitrary malformed states are
reachable. Prove crash by reusing the same owner-release lemma because correct
drop and crash transitions share the release helper.

- [ ] **Step 4: Prove result replacement properties**

Add:

```lean
theorem determinate_is_immutable
    (state : State) (handler : Handler) (old replacement : ResultRow)
    (present : findResult state old.run old.mutant = some old)
    (determinate : old.status.isDeterminate = true) :
    (step state (.persist handler old.run old.mutant replacement.status
      replacement.payload .valid)).state.results = state.results

theorem successful_replacement_is_exact
    (state : State) (handler : Handler) (old replacement : ResultRow)
    (present : findResult state old.run old.mutant = some old)
    (inconclusive : old.status.isDeterminate = false)
    (accepted : (step state (.persist handler old.run old.mutant
      replacement.status replacement.payload .valid)).rejection = none) :
    SameExceptResult state
      (step state (.persist handler old.run old.mutant replacement.status
        replacement.payload .valid)).state replacement

theorem failed_replacement_restores_old
    (state : State) (handler : Handler) (old replacement : ResultRow)
    (present : findResult state old.run old.mutant = some old) :
    findResult
      (step state (.persist handler old.run old.mutant replacement.status
        replacement.payload .invalidDiagnostic)).state
      old.run old.mutant = some old
```

The successful theorem names the exact `(run, mutant)` row and proves all other
result rows and all run rows are unchanged. The failure theorem covers
`invalidDiagnostic` and proves the old status and payload both survive; proving
only the status is insufficient.

- [ ] **Step 5: Prove one-step and arbitrary-trace preservation**

Add:

```lean
theorem step_preserves_invariant (state : State) (event : Event)
    (holds : Invariant state) : Invariant (step state event).state

theorem run_preserves_invariant (state : State) (trace : List Event)
    (holds : Invariant state) : Invariant (run state trace)
```

Prove the trace theorem by induction over `trace` using the one-step theorem.
The theorem applies to Lean's model only; do not mention Rust reachability in
the theorem or comments.

- [ ] **Step 6: Expose proofs and run GREEN**

Add to `HoiminOracle.lean`:

```lean
import HoiminOracle.SessionProofs
```

Run:

```bash
cd formal/HoiminOracle
lake build
! rg -n "admit|axiom " HoiminOracle/SessionModel.lean HoiminOracle/SessionProofs.lean
```

Expected: PASS and no placeholder output.

- [ ] **Step 7: Commit Task 2**

```bash
git add formal/HoiminOracle/HoiminOracle/SessionProofs.lean
git add formal/HoiminOracle/HoiminOracle.lean
git commit -m "test: prove session lifecycle invariants in Lean"
```

---

### Task 3: Strict corpus, bounded explorer, and sensitivity gate

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/SessionCases.lean`
- Create: `formal/HoiminOracle/SessionAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/session-recovery.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes Task 1's model/broken transitions and Task 2's invariant definitions.
- Produces `SessionAudit.cases`, `renderCorpus`, and executable `generate_session --output PATH|--check PATH|--stats|--sensitivity`.
- Corpus case fields: `schema`, `id`, `mode`, `schedule`, and `expected`.
- Observation fields: `event`, `verdict`, `error_code`, `selected_run`, `stored_result`, `runs`, `results`, and `owners`.

- [ ] **Step 1: Add fixed case specifications with Lean-derived expectations**

Create `SessionCases.lean` with `NamedEvent`, `CaseSpec`, `OracleStep`, and
`OracleCase`. Every `expected` vector must be produced by folding Task 1's
`step`; do not hand-enter an expected verdict or database snapshot.

Use these stable strict case IDs and premises:

```text
newest_compatible_incomplete_run
live_owner_rejects_second_handler
incomplete_finish_releases_for_resume
handler_drop_releases_for_resume
completed_run_is_final
incomplete_finish_is_idempotent
determinate_killed_is_immutable
determinate_survived_is_immutable
timeout_replacement_commits_atomically
oom_replacement_commits_atomically
process_limit_replacement_commits_atomically
error_replacement_commits_atomically
not_run_replacement_commits_atomically
invalid_replacement_rolls_back
missing_run_persist_rolls_back
lookup_missing_and_completed_reject
different_runs_have_independent_owners
non_owner_incomplete_finish_releases_for_resume
```

All cases have `mode = "strict"`. Persistence events carry both status and
payload so `invalid_replacement_rolls_back` can assert that the entire old row,
candidate marker, and diagnostic marker survive.

- [ ] **Step 2: Observe missing-executable RED**

Run:

```bash
cd formal/HoiminOracle
lake exe generate_session -- --check corpus/session-recovery.jsonl
```

Expected: FAIL because `generate_session` is not registered.

- [ ] **Step 3: Implement executable-only BFS and minimal counterexamples**

Create `SessionAuditMain.lean` importing `SessionCases` and `SessionProofs`.
Define the stable event alphabet from all finite roles/statuses while excluding
syntactically impossible internal continuations. Explore breadth-first through
depth 8 and retain the first shortest trace for each equal semantic state only
after checking every outgoing transition.

The executable owns:

```lean
def auditDepth : Nat := 8
def reachableStateCount : Nat
def checkedTransitionCount : Nat
def firstCounterexample? (next : State → Event → Verdict) : Option (List Event)
def shrinkTrace (next : State → Event → Verdict) (trace : List Event) : List Event
def sensitivityPasses : Bool
```

`--stats` prints one machine-readable line:

```text
depth=8 alphabet=<n> states=<n> transitions=<n> corpus_cases=18
```

`--sensitivity` prints the family, violated invariant, and shortest trace for
atomicity, uniqueness, and boundary variants, and exits nonzero if any family
is not detected. Use `native_decide` only in this executable if the finite check
benefits from it.

- [ ] **Step 4: Add JSON rendering and the guarded generator**

Define in `SessionCases.lean`:

```lean
def eventName : Event → String
def oracleStepJson : OracleStep → Lean.Json
def oracleCaseJson : OracleCase → Lean.Json
def renderCorpus : String
```

`SessionAuditMain.lean` supports:

```text
generate_session --output PATH
generate_session --check PATH
generate_session --stats
generate_session --sensitivity
```

Output and check refuse to proceed unless the correct model has no bounded
counterexample and all three broken families are detected. Register:

```toml
[[lean_exe]]
name = "generate_session"
root = "SessionAuditMain"
```

Expose `SessionCases` from `HoiminOracle.lean`, but never import
`SessionAuditMain` there.

- [ ] **Step 5: Generate corpus and verify stable output**

Run:

```bash
cd formal/HoiminOracle
lake build
lake exe generate_session -- --sensitivity
lake exe generate_session -- --stats
lake exe generate_session -- --output corpus/session-recovery.jsonl
lake exe generate_session -- --check corpus/session-recovery.jsonl
git diff -- corpus/session-recovery.jsonl
```

Expected: all three broken families are detected and corpus JSON Lines appear
in the fixed case order.

- [ ] **Step 6: Confirm expensive evaluation placement**

Run:

```bash
rg -n "native_decide|reachableStateCount|checkedTransitionCount|firstCounterexample" formal/HoiminOracle
rg -n "SessionAuditMain" formal/HoiminOracle/HoiminOracle.lean formal/HoiminOracle/HoiminOracle
```

Expected: expensive exploration symbols occur only in
`SessionAuditMain.lean`; imported library modules do not reference the
executable.

- [ ] **Step 7: Commit Task 3**

```bash
git add formal/HoiminOracle/HoiminOracle/SessionCases.lean
git add formal/HoiminOracle/SessionAuditMain.lean
git add formal/HoiminOracle/corpus/session-recovery.jsonl
git add formal/HoiminOracle/HoiminOracle.lean
git add formal/HoiminOracle/lakefile.toml
git commit -m "test: generate Lean session recovery corpus"
```

---

### Task 4: Strict `SessionHandler` correspondence adapter

**Files:**
- Create: `crates/hoimin-cli/tests/lean_session_oracle.rs`

**Interfaces:**
- Consumes Task 3's generated JSONL and public `hoimin_cli::session::SessionHandler` methods.
- Produces tests `corpus_is_well_formed`, `corpus_rejects_unknown_schema_mode_and_event`, `corpus_rejects_duplicate_case_ids`, `case_panics_are_infrastructure_errors`, and `oracle_correspondence`.
- Supports single-case replay through `HOIMIN_SESSION_ORACLE_CASE=<case-id>`.

- [ ] **Step 1: Write the strict parser and deliberately failing adapter shell**

Add `#[serde(deny_unknown_fields)]` corpus structs matching every Task 3 field.
Validation accepts only schema `1`, mode `strict`, the exact event grammar, the
seven status names, and the two values for each semantic role. Temporarily have
`run_case` return:

```rust
CaseResult {
    class: CaseClass::InfrastructureError,
    detail: Some("adapter not implemented".to_owned()),
    ..
}
```

- [ ] **Step 2: Run the adapter test and observe RED**

Run:

```bash
cargo test -p hoimin-cli --test lean_session_oracle -- --nocapture
```

Expected: `oracle_correspondence` fails because every strict case is an
infrastructure error.

- [ ] **Step 3: Implement semantic-role translation through public APIs**

For each isolated case, create one `tempfile::TempDir`, one database path, and a
`BTreeMap<HandlerRole, SessionHandler>`. Map:

```rust
fn fingerprint(role: FingerprintRole) -> RunFingerprint {
    RunFingerprint::from_bytes(match role {
        FingerprintRole::F0 => [1; 32],
        FingerprintRole::F1 => [2; 32],
    })
}

fn run_id(role: RunRole) -> &'static str {
    match role { RunRole::R0 => "oracle-r0", RunRole::R1 => "oracle-r1" }
}

fn mutant_id(role: MutantRole) -> &'static str {
    match role { MutantRole::M0 => "oracle-m0", MutantRole::M1 => "oracle-m1" }
}
```

Translate events to `SessionHandler::open`, `begin`, `load`, `lookup`,
`persist`, `finish`, and `BTreeMap::remove` for drop. A crash event is rejected
by corpus validation because it is `internal-fixture`, not strict. Construct
`PersistResult` with payload-specific candidate replacement, output token, and
diagnostic code so snapshots distinguish `p0` from `p1`. For
`invalidDiagnostic`, change only `SessionDiagnostic.mutant_id` to a different
ID and invoke the same public `persist` method.

- [ ] **Step 4: Observe exact durable rows without predicting outcomes**

After every public event, open a read-only `rusqlite::Connection` and query:

```sql
SELECT run_id, hex(fingerprint), finished, complete FROM runs ORDER BY id;
SELECT r.run_id, r.mutant_id, r.status,
       c.replacement, r.output_token, d.code
FROM results r
JOIN candidates c USING(run_id, mutant_id)
LEFT JOIN diagnostics d
  ON d.run_id=r.run_id AND d.mutant_id=r.mutant_id
ORDER BY r.run_id, r.mutant_id, d.id;
```

Normalize real identifiers back to roles. Normalize success values and
`EffectFailed.failure.code()` into the observation. Track which handler
returned ownership-establishing success only to label the real handle; exercise
ownership itself with explicit competing `load` cases rather than treating
adapter bookkeeping as proof of locking.

- [ ] **Step 5: Classify complete case outcomes and catch harness failures**

Wrap each isolated case in
`catch_unwind(AssertUnwindSafe(|| driver.run(schedule)))`. Compare the
complete actual vector with Lean expectations only after the driver succeeds:

```rust
enum CaseClass { Match, Mismatch, InfrastructureError }
```

An API error is an ordinary observation. JSON parse errors, unknown role
mappings, SQL observation errors, missing handlers, and panics are
`InfrastructureError`. Do not turn an unexpected API error into infrastructure
failure; it must remain a semantic observation and therefore a mismatch when
Lean expected acceptance.

- [ ] **Step 6: Add parser guards, panic guard, and single-case replay**

Test unknown schema, any non-`strict` mode including `model-only`,
`internal-fixture`, `infrastructure-error`, and legacy `report`, unknown event,
unknown status, schedule/expected length mismatch, and duplicate IDs. Verify a
deliberate driver panic becomes `InfrastructureError`. Filter cases using
`HOIMIN_SESSION_ORACLE_CASE` and fail clearly if the named case does not exist.

- [ ] **Step 7: Run GREEN and focused single cases**

```bash
cargo test -p hoimin-cli --test lean_session_oracle -- --nocapture
HOIMIN_SESSION_ORACLE_CASE=invalid_replacement_rolls_back cargo test -p hoimin-cli --test lean_session_oracle oracle_correspondence -- --exact --nocapture
HOIMIN_SESSION_ORACLE_CASE=live_owner_rejects_second_handler cargo test -p hoimin-cli --test lean_session_oracle oracle_correspondence -- --exact --nocapture
```

Expected: all selected strict cases match with zero infrastructure errors.

- [ ] **Step 8: Commit Task 4**

```bash
git add crates/hoimin-cli/tests/lean_session_oracle.rs
git commit -m "test: compare sessions with Lean recovery oracle"
```

---

### Task 5: Internal-fixture evidence and audit report

**Files:**
- Create: `docs/superpowers/reports/2026-08-09-lean-session-recovery-audit.md`
- Conditionally create: `docs/superpowers/reports/2026-08-09-lean-session-recovery-counterexamples.md`

**Interfaces:**
- Consumes formal proofs, explorer output, sensitivity witnesses, strict corpus results, and existing `session_handler` fixtures.
- Produces a same-premise worksheet, evidence ledger, exact costs, and final classification without changing production code.

- [ ] **Step 1: Execute and time formal gates**

Run:

```bash
cd formal/HoiminOracle
/usr/bin/time -p lake build
/usr/bin/time -p lake exe generate_session -- --check corpus/session-recovery.jsonl
/usr/bin/time -p lake exe generate_session -- --stats
/usr/bin/time -p lake exe generate_session -- --sensitivity
```

Record elapsed times, depth, alphabet, reachable states, checked transitions,
corpus count, reduction rule, theorem names, and the shortest witness from each
broken family.

- [ ] **Step 2: Execute strict and internal-fixture evidence**

Run:

```bash
cargo test -p hoimin-cli --test lean_session_oracle -- --nocapture
cargo test -p hoimin-cli --test session_handler process_death_releases_run_ownership_for_immediate_resume -- --exact --nocapture
cargo test -p hoimin-cli --test session_handler session_operations_complete_under_contention_matrix -- --exact --nocapture
cargo test -p hoimin-cli --test session_handler failed_inconclusive_replacement_restores_the_previous_result -- --exact --nocapture
```

Classify the corpus as `strict`, process death and test-only pause behavior as
`internal-fixture`, split transaction/crash points as `model-only`, and any
harness/setup failure as `infrastructure-error`.

- [ ] **Step 3: Classify every divergence**

For each strict difference, rerun with
`HOIMIN_SESSION_ORACLE_CASE=<case-id>` and choose exactly one classification:
`confirmed bug`, `specification ambiguity`, `model defect`, or
`infrastructure error`. Do not weaken the model merely to match Rust. If the
model or adapter is defective, correct it, regenerate the corpus, rerun all
three sensitivity families, and rerun all strict cases.

- [ ] **Step 4: Write the report and conditional counterexample ledger**

The report uses:

```markdown
# Lean Session Persistence, Recovery, and Ownership Audit Report

## Result
## Claim and excluded scope
## Same-premise correspondence worksheet
## Formal state and event model
## What Lean proved
## Bounded exploration and cost
## Broken-variant sensitivity
## Strict implementation correspondence
## Internal-fixture evidence
## Minimal witnesses and classifications
## Model and adapter limitations
## Owner decisions
## Exact reproduction commands
```

If a real strict mismatch remains, create the counterexample ledger with:

```text
claim:
mode:
model boundary:
finite domain or theorem premises:
minimal event trace:
intermediate durable and ownership states:
expected observation:
actual observation:
classification:
impact:
owner question:
reproduction command:
```

- [ ] **Step 5: Check report language and commit documentation**

```bash
rg -n "proved production|unbounded production|mode.*report" docs/superpowers/reports/2026-08-09-lean-session-recovery-audit.md
git diff --check
git add docs/superpowers/reports/2026-08-09-lean-session-recovery-audit.md
git commit -m "docs: report Lean session recovery audit [skip ci]"
```

If a real ledger exists, add it to the same documentation commit.

---

### Task 6: Final audit verification

**Files:**
- Verify only; no `crates/*/src/` path may change.

**Interfaces:**
- Consumes every audit artifact.
- Produces final reproducibility, regression, scope, and clean-worktree evidence.

- [ ] **Step 1: Verify Lean and every corpus remains fresh**

```bash
cd formal/HoiminOracle
lake build
lake exe generate -- --check corpus/state-machine.jsonl
lake exe generate_budget -- --check corpus/budget-cleanup.jsonl
lake exe generate_session -- --check corpus/session-recovery.jsonl
lake exe generate_session -- --sensitivity
```

- [ ] **Step 2: Verify all oracle adapters and focused session tests**

```bash
cd ../..
cargo test -p hoimin-core --test lean_oracle
cargo test -p hoimin-core --test lean_budget_oracle
cargo test -p hoimin-cli --test lean_session_oracle
cargo test -p hoimin-cli --test session_handler
```

- [ ] **Step 3: Verify formatting, lint, full regressions, and diff hygiene**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check origin/main..HEAD
git diff --name-only origin/main..HEAD
git status -sb
```

Expected: all checks pass. Changed paths are limited to this design and plan,
the formal session model/proofs/cases/executable/corpus, the CLI audit adapter,
and the report or conditional ledger. No `crates/*/src/` file appears.

- [ ] **Step 4: Ensure the branch tip runs CI**

If the final commit contains `[skip ci]`, add:

```bash
git commit --allow-empty -m "ci: validate Lean session recovery audit"
```

- [ ] **Step 5: Commit any final non-documentation hygiene change**

If formatting changed the Rust adapter, commit only that verified diff:

```bash
git add crates/hoimin-cli/tests/lean_session_oracle.rs
git commit -m "test: finalize session oracle verification"
```

Do not create an empty commit if the branch tip already triggers CI.

- [ ] **Step 6: Report and stop at the audit boundary**

Report the Lean theorem scope, finite bounds and cost, broken-witness results,
strict match/mismatch counts, internal-fixture status, infrastructure errors,
report path, and any owner decisions. Do not repair production code within this
audit branch.
