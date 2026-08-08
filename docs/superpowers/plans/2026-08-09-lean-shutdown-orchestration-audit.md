# Lean Shutdown Orchestration Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reproducible Lean audit of Hoimin's asynchronous shutdown orchestration, compare same-premise scenarios with the real CLI, and produce repair-ready evidence for every mismatch without changing production code.

**Architecture:** Add an independent `ShutdownAudit` model, proof module, case generator, and bounded explorer to the pinned Lean project. Lean owns detailed event traces and expected terminal observations; a Rust integration adapter executes the corresponding public CLI scenarios, while an evidence worksheet keeps existing internal fixtures and model-only boundaries separate. Any surviving divergence is preserved as a named case and documented down to the responsible functions, branches, ownership, first RED repair test, and competing repair designs.

**Tech Stack:** Lean 4 and Lake with `Std`/`Lean.Data.Json`, Rust 2024, Tokio, `serde`/`serde_json`, `rusqlite`, `tempfile`, Cargo integration tests, real Unix SIGINT or Windows console control events.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/lean-shutdown-orchestration-20260809` on branch `audit/lean-shutdown-orchestration-20260809`.
- Do not modify any file under `crates/*/src/`, including test-only additions inside production modules.
- Do not repair production behavior in this audit branch.
- Lean-generated expectations are authoritative; Rust must observe implementation behavior and must not calculate expected semantics.
- Every case declares exactly one mode: `strict`, `internal-fixture`, or `model-only`; harness failures are classified separately as `infrastructure-error`.
- A same-premise mismatch must not be removed or weakened to make CI green. Pin the exact reviewed mismatch set instead.
- The `.venv` worktree symlink is local setup and must never be committed.
- Pure documentation commits include `[skip ci]`; the final branch tip must trigger CI.
- Large exploration and JSON generation live only in an executable module and are not imported by proof/library modules.
- Report theorem-backed claims, bounded checks, implementation observations, and platform limitations separately.

---

### Task 1: Existing-evidence and correspondence worksheet

**Files:**
- Create: `docs/superpowers/reports/2026-08-09-shutdown-orchestration-coverage-worksheet.md`

**Interfaces:**
- Consumes: the approved design and existing tests/functions in `interrupt.rs`, `shell.rs`, `process/mod.rs`, `machine.rs`, and `run_e2e.rs`.
- Produces: a row identifier, claim, exact premise mode, existing evidence, missing observation, and planned corpus scenario for every audit property. Later tasks use the stable row IDs `SHUT-01` through `SHUT-12`.

- [ ] **Step 1: Inventory exact existing tests and implementation boundaries**

Run:

```bash
rg -n '^\s*(async )?fn (.*interrupt|.*sigint|.*shutdown|.*drain|.*cleanup|.*deadline|.*timeout|.*detach|.*session.*finish|.*report)' \
  crates/hoimin-cli/src/interrupt.rs \
  crates/hoimin-cli/src/shell.rs \
  crates/hoimin-cli/src/process/mod.rs \
  crates/hoimin-cli/tests/run_e2e.rs
rg -n 'RunEvent::(CancellationRequested|DeadlineReached|CleanupFinished|SessionFinished|RunFinished)' \
  crates/hoimin-core/src/machine.rs crates/hoimin-cli/src/shell.rs
```

Record exact function/test names. At minimum map:

- `second_signal_forces_130_without_waiting_for_first_consumer`;
- `shutdown_budget_first_activation_cannot_be_extended`;
- `shutdown_budget_total_timeout_deadline_is_anchored_to_run_deadline`;
- `cancellation_and_timeout_precede_a_simultaneous_exit`;
- `process_error_precedes_output_cleanup_error`;
- `cleanup_failures_preserve_primary_error_and_append_details_in_order`;
- `shutdown_budget_preempts_an_owned_blocking_close`;
- `expired_final_close_detaches_cleanup_without_extending_the_wait`;
- `shutdown_drain_expiry_reports_process_and_blocking_task_counts`;
- `expired_shutdown_skips_metrics_with_an_incomplete_warning`;
- `first_sigint_finishes_a_parseable_incomplete_session` or its Windows counterpart;
- `second_sigint_forces_130_while_session_finish_is_blocked` or its Windows counterpart.

- [ ] **Step 2: Write the worksheet with a fixed row schema**

Use this table header and fill all twelve rows with concrete evidence or
`missing — Task N`:

```markdown
| ID | Claim | Mode | Existing evidence | Missing observation | Planned case |
| --- | --- | --- | --- | --- | --- |
| SHUT-01 | First cause and first deadline are retained | internal-fixture | `shutdown_budget_first_activation_cannot_be_extended` | cause/exit agreement through the real CLI | `first_interrupt_precedes_deadline` |
```

The twelve rows cover first-cause retention, forced interrupt boundedness,
post-stop spawn suppression, terminate/reap at-most-once, process/output race,
cleanup/report/session singleton dispatch, primary-error precedence,
non-regression, detachment ownership transfer, successful terminal obligations,
and parseable incomplete persistence.

- [ ] **Step 3: Validate that every design claim has exactly one row**

Run:

```bash
for id in 01 02 03 04 05 06 07 08 09 10 11 12; do
  test "$(rg -c "SHUT-$id" docs/superpowers/reports/2026-08-09-shutdown-orchestration-coverage-worksheet.md)" = 1
done
rg -n 'missing — Task [2-7]|strict|internal-fixture|model-only' \
  docs/superpowers/reports/2026-08-09-shutdown-orchestration-coverage-worksheet.md
git diff --check
```

Expected: every ID occurs once, every row has an explicit mode, and every gap
names the task that owns it.

- [ ] **Step 4: Commit the worksheet**

```bash
git add docs/superpowers/reports/2026-08-09-shutdown-orchestration-coverage-worksheet.md
git commit -m "docs: map shutdown audit evidence [skip ci]"
```

---

### Task 2: Lean shutdown state and correct/broken transitions

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ShutdownModel.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Consumes: no executable module; imports only `Std`.
- Produces: namespace `HoiminOracle.ShutdownAudit` with `State`, `Event`, `Verdict`, `Observation`, `safe`, `step`, `run`, seven broken transitions, and fixed witness traces.

- [ ] **Step 1: Add a deliberately false ownership-safety example**

Create the model module with finite roles and this initial RED check:

```lean
import Std

namespace HoiminOracle.ShutdownAudit

inductive StopCause
  | interrupt | forcedInterrupt | deadline | processFailure | infrastructureFailure
  deriving Repr, DecidableEq, BEq

inductive ComponentState
  | absent | pending | complete | failed | detached
  deriving Repr, DecidableEq, BEq

inductive ProcessState
  | notStarted | running | terminationRequested | exited | reaped
  deriving Repr, DecidableEq, BEq

inductive Event
  | boot | startProcess | firstInterrupt | secondInterrupt | deadlineReached
  | processExited | processFailed | infrastructureFailed
  | requestTermination | reapProcess
  | startOutputDrain | outputDrained | outputFailed
  | startBlocking | blockingCompleted | detachBlocking
  | startCleanup | cleanupCompleted | cleanupFailed
  | startSessionFinish | sessionFinished | sessionFailed
  | startReport | reportWritten | reportFailed
  | startMetrics | metricsWritten | metricsFailed | metricsSkipped
  | returnSuccess | returnFailure
  deriving Repr, DecidableEq, BEq

structure State where
  cause : Option StopCause := none
  deadlineOrdinal : Option Nat := none
  process : ProcessState := .notStarted
  output : ComponentState := .absent
  blocking : ComponentState := .absent
  workspace : ComponentState := .absent
  session : ComponentState := .absent
  report : ComponentState := .absent
  metrics : ComponentState := .absent
  blockingOwnershipTransferred : Bool := false
  primaryError : Option StopCause := none
  appendedErrors : List StopCause := []
  processStarts : Nat := 0
  terminationRequests : Nat := 0
  reaps : Nat := 0
  cleanupDispatches : Nat := 0
  sessionDispatches : Nat := 0
  reportDispatches : Nat := 0
  metricsDispatches : Nat := 0
  sessionCompleteFlag : Bool := false
  reportCompleteFlag : Bool := false
  exitCode : Option Nat := none
  returned : Bool := false
  deriving Repr, DecidableEq, BEq

def State.initial : State := {}

def atMostOne (value : Nat) : Bool := value == 0 || value == 1

def safe (state : State) : Bool :=
  atMostOne state.processStarts && atMostOne state.terminationRequests &&
  atMostOne state.reaps && atMostOne state.cleanupDispatches &&
  atMostOne state.sessionDispatches && atMostOne state.reportDispatches &&
  atMostOne state.metricsDispatches &&
  (!(state.blocking == .detached) || state.blockingOwnershipTransferred) &&
  (!(state.process == .reaped) || state.reaps == 1) &&
  (!(state.exitCode == some 130) || state.cause == some .interrupt ||
    state.cause == some .forcedInterrupt)

example : safe State.initial = false := by decide
```

- [ ] **Step 2: Run Lean and observe RED**

Run:

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/ShutdownModel.lean
```

Expected: FAIL because every counter is zero and the initial state is safe.

- [ ] **Step 3: Define verdicts, observations, helpers, and the correct transition**

Remove the false example and add these exact interfaces:

```lean
inductive Rejection
  | invalidPhase | duplicate | missingOwnership | alreadyReturned
  deriving Repr, DecidableEq, BEq

structure Verdict where
  state : State
  rejection : Option Rejection := none
  deriving Repr, DecidableEq, BEq

structure Observation where
  event : Event
  rejection : Option Rejection
  cause : Option StopCause
  process : ProcessState
  output blocking workspace session report metrics : ComponentState
  primaryError : Option StopCause
  appendedErrors : List StopCause
  dispatches : Nat × Nat × Nat × Nat
  sessionCompleteFlag : Bool
  reportCompleteFlag : Bool
  exitCode : Option Nat
  returned : Bool
  deriving Repr, DecidableEq, BEq

def observe (event : Event) (verdict : Verdict) : Observation
def step (state : State) (event : Event) : Verdict
def run : State → List Event → State
```

Implement `step` as a total match over every event. The transition rules are:

- `firstInterrupt` installs `.interrupt`, primary error `.interrupt`, and
  deadline ordinal `0` only when no shutdown exists;
- `deadlineReached` installs `.deadline` and ordinal `0` only when no shutdown
  exists; otherwise it retains the existing cause/deadline;
- `secondInterrupt` always retains durable component states, selects
  `.forcedInterrupt`, exit 130, clears every modeled wait obligation by
  detaching an active blocking component with ownership transfer, and marks the
  state returned;
- `boot` is accepted only from `State.initial` and establishes the owned
  workspace/session obligations as `.pending`; for these generic component
  states, `.pending` means an unsettled lifecycle obligation, not necessarily a
  currently executing future;
- each `start*` increments its counter only from the component's valid prior
  state and rejects duplicates without mutation;
- `processFailed` moves a started process to `.exited` and records
  `.processFailure`; `infrastructureFailed` records
  `.infrastructureFailure` without fabricating a component completion;
- failure events retain the first `primaryError`; later failures append their
  class in event order;
- `detachBlocking` is accepted only after setting
  `blockingOwnershipTransferred = true` in the same transition;
- `returnSuccess` requires process reaped when started, workspace/session/report
  complete when present, `sessionCompleteFlag = true`,
  `reportCompleteFlag = true`, and no primary error, then selects exit 0;
- `returnFailure` requires a selected primary error and never regresses any
  completed component. It selects 130 for interrupt/forced interrupt, 4 for
  deadline, and 3 for process/infrastructure failure;
- accepted session/report completion sets its complete flag exactly when no
  stop cause exists; interrupted or timed-out completion settles the component
  while retaining `false` so the adapter can distinguish parseable incomplete
  output from a complete run.

Extend `safe` with component non-regression bookkeeping, returned-state
terminality, success obligations, first-error consistency, no report dispatch
before a started process is reaped and cleanup/session obligations are settled,
and no pending wait when exit code 130 is selected. Keep every `.all` or
implication explicitly parenthesized so Boolean lambda bodies cannot swallow
following clauses.

- [ ] **Step 4: Add seven broken families and fixed witnesses**

Define:

```lean
def brokenCauseStep : State → Event → Verdict
def brokenDuplicateStep : State → Event → Verdict
def brokenOrderingStep : State → Event → Verdict
def brokenPrecedenceStep : State → Event → Verdict
def brokenForcedWaitStep : State → Event → Verdict
def brokenRegressionStep : State → Event → Verdict
def brokenDetachStep : State → Event → Verdict

def causeWitness duplicateWitness orderingWitness precedenceWitness : List Event
def forcedWaitWitness regressionWitness detachWitness : List Event
```

Use these fixed traces:

```lean
def causeWitness := [.boot, .startProcess, .firstInterrupt, .deadlineReached]
def duplicateWitness := [.boot, .firstInterrupt, .startCleanup, .startCleanup]
def orderingWitness := [.boot, .startProcess, .firstInterrupt, .startReport, .reportWritten]
def precedenceWitness := [.boot, .startProcess, .processFailed, .startCleanup, .cleanupFailed]
def forcedWaitWitness := [.boot, .startBlocking, .firstInterrupt, .secondInterrupt]
def regressionWitness := [.boot, .startSessionFinish, .sessionFinished, .firstInterrupt]
def detachWitness := [.boot, .startBlocking, .firstInterrupt, .detachBlocking]
```

Each broken step changes only its named fault family. The correct trace must
satisfy its local predicate; the broken trace must fail it.

- [ ] **Step 5: Import the model and run GREEN**

Add to `HoiminOracle.lean`:

```lean
import HoiminOracle.ShutdownModel
```

Run:

```bash
lake env lean HoiminOracle/ShutdownModel.lean
lake build
rg -n 'sorry|admit|axiom' HoiminOracle/ShutdownModel.lean
```

Expected: Lean commands pass and `rg` prints nothing.

- [ ] **Step 6: Commit the model**

```bash
cd ../..
git add formal/HoiminOracle/HoiminOracle/ShutdownModel.lean formal/HoiminOracle/HoiminOracle.lean
git commit -m "test: model shutdown orchestration in Lean"
```

---

### Task 3: Explicit-premise Lean proofs

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ShutdownProofs.lean`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`

**Interfaces:**
- Consumes: Task 2's `State`, `Event`, `step`, component enums, and helper predicates.
- Produces: named one-step and trace-lifted theorems; imports no case or explorer module.

- [ ] **Step 1: Add the proof module with a false cause-retention theorem**

```lean
import HoiminOracle.ShutdownModel

namespace HoiminOracle.ShutdownAudit

example (state : State) (cause : StopCause)
    (present : state.cause = some cause) :
    (step state .deadlineReached).state.cause = some .deadline := by
  simp_all [step]
```

- [ ] **Step 2: Run Lean and observe RED**

Run:

```bash
cd formal/HoiminOracle
lake env lean HoiminOracle/ShutdownProofs.lean
```

Expected: FAIL for a state whose existing cause is not `.deadline`.

- [ ] **Step 3: Replace the false theorem with exact proof obligations**

Prove these signatures:

```lean
theorem rejected_preserves_state (state : State) (event : Event)
    (rejected : (step state event).rejection.isSome = true) :
    (step state event).state = state

theorem first_cause_is_retained (state : State) (event : Event) (cause : StopCause)
    (present : state.cause = some cause)
    (notForced : event ≠ .secondInterrupt) :
    (step state event).state.cause = some cause

theorem first_deadline_is_not_extended (state : State) (event : Event) (ordinal : Nat)
    (present : state.deadlineOrdinal = some ordinal) :
    (step state event).state.deadlineOrdinal = some ordinal

theorem forced_interrupt_returns_130 (state : State) :
    let after := (step state .secondInterrupt).state
    after.exitCode = some 130 ∧ after.returned = true ∧ after.blocking ≠ .pending

theorem primary_error_is_retained (state : State) (event : Event) (cause : StopCause)
    (present : state.primaryError = some cause) :
    (step state event).state.primaryError = some cause

theorem singleton_dispatches_are_bounded (state : State) (event : Event)
    (holds : safe state = true) :
    let after := (step state event).state
    after.cleanupDispatches ≤ 1 ∧ after.sessionDispatches ≤ 1 ∧
      after.reportDispatches ≤ 1 ∧ after.metricsDispatches ≤ 1

theorem reaped_never_runs_again (state : State) (event : Event)
    (reaped : state.process = .reaped) :
    (step state event).state.process ≠ .running

theorem detached_has_transferred_ownership (state : State)
    (holds : safe state = true) (detached : state.blocking = .detached) :
    state.blockingOwnershipTransferred = true

def StructuralInvariant (state : State) : Prop :=
  state.processStarts ≤ 1 ∧ state.terminationRequests ≤ 1 ∧ state.reaps ≤ 1 ∧
  state.cleanupDispatches ≤ 1 ∧ state.sessionDispatches ≤ 1 ∧
  state.reportDispatches ≤ 1 ∧ state.metricsDispatches ≤ 1

theorem step_preserves_structural_invariant (state : State) (event : Event)
    (holds : StructuralInvariant state) :
    StructuralInvariant (step state event).state

theorem run_preserves_structural_invariant (state : State) (trace : List Event)
    (holds : StructuralInvariant state) :
    StructuralInvariant (run state trace)
```

Use exhaustive event cases plus explicit splits on component phases. Do not
replace the structural invariant with `safe = true` unless Lean proves it
without expensive evaluation. The report must state the narrower theorem scope.

- [ ] **Step 4: Import proofs and run GREEN**

Add:

```lean
import HoiminOracle.ShutdownProofs
```

Run:

```bash
lake env lean HoiminOracle/ShutdownProofs.lean
lake build
rg -n 'sorry|admit|axiom' HoiminOracle/ShutdownProofs.lean
```

Expected: PASS and no forbidden proof escape.

- [ ] **Step 5: Commit proofs**

```bash
cd ../..
git add formal/HoiminOracle/HoiminOracle/ShutdownProofs.lean formal/HoiminOracle/HoiminOracle.lean
git commit -m "test: prove shutdown orchestration invariants in Lean"
```

---

### Task 4: Bounded explorer, sensitivity gate, and Lean-owned corpus

**Files:**
- Create: `formal/HoiminOracle/HoiminOracle/ShutdownCases.lean`
- Create: `formal/HoiminOracle/ShutdownAuditMain.lean`
- Create: `formal/HoiminOracle/corpus/shutdown-orchestration.jsonl`
- Modify: `formal/HoiminOracle/HoiminOracle.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`

**Interfaces:**
- Consumes: Task 2 model/witnesses and Task 3 proofs.
- Produces: `shutdownCases`, executable `generate_shutdown`, depth/statistics output, deterministic JSONL, and a mandatory seven-family sensitivity gate.

- [ ] **Step 1: Add case structs and an intentionally missing executable test**

Define in `ShutdownCases.lean`:

```lean
inductive Mode | strict | internalFixture | modelOnly
  deriving Repr, DecidableEq, BEq

inductive Scenario
  | normalCompletion | firstInterruptRunning | totalTimeoutReapsDescendant
  | secondInterruptBlockedFinish | totalTimeoutBlockedFinish
  | processExitVsCancellation | cleanupFailureAfterProcessFailure
  | outputEofVsExpiry | cleanupCompletionVsExpiry
  | sessionFinishVsLateCancellation | reportWriteVsLateError
  | blockingCompletionVsDetach
  deriving Repr, DecidableEq, BEq

structure TerminalObservation where
  cause : Option StopCause
  exitCode : Option Nat
  process : ProcessState
  output blocking workspace session report metrics : ComponentState
  primaryError : Option StopCause
  appendedErrors : List StopCause
  dispatches : Nat × Nat × Nat × Nat
  sessionCompleteFlag : Bool
  reportCompleteFlag : Bool
  returned : Bool
  deriving Repr, DecidableEq, BEq

structure OracleCase where
  schema : Nat := 1
  id : String
  mode : Mode
  scenario : Scenario
  schedule : List Event
  expected : TerminalObservation
```

Then run before registering the executable:

```bash
cd formal/HoiminOracle
lake exe generate_shutdown -- --stats
```

Expected: FAIL with unknown executable.

- [ ] **Step 2: Define twelve deterministic cases**

Create exactly these IDs and derive every `expected` value by folding `step`
over `schedule`:

```text
normal_completion                         strict
first_interrupt_running                   strict
total_timeout_reaps_descendant             strict
second_interrupt_blocked_finish            internal-fixture
total_timeout_blocked_finish               internal-fixture
process_exit_vs_cancellation               internal-fixture
cleanup_failure_after_process_failure      internal-fixture
output_eof_vs_expiry                       internal-fixture
cleanup_completion_vs_expiry               internal-fixture
session_finish_vs_late_cancellation         model-only
report_write_vs_late_error                  model-only
blocking_completion_vs_detach               model-only
```

Each schedule starts at `.boot`, contains only the milestone order named by its
scenario, and ends in `.returnSuccess`, `.returnFailure`, or `.secondInterrupt`.
`normal_completion` must settle process, output, cleanup, session, report, and
metrics before returning success. `first_interrupt_running` and
`total_timeout_reaps_descendant` must return failure with a reaped process and
an incomplete session. Internal/model-only cases exercise the exact races named
by their IDs.

Use these exact schedules (line breaks are presentation only):

```text
normal_completion =
  boot, startProcess, processExited, startOutputDrain, outputDrained,
  reapProcess, startCleanup, cleanupCompleted, startSessionFinish,
  sessionFinished, startReport, reportWritten, startMetrics, metricsWritten,
  returnSuccess

first_interrupt_running =
  boot, startProcess, firstInterrupt, requestTermination, processExited,
  startOutputDrain, outputDrained, reapProcess, startCleanup, cleanupCompleted,
  startSessionFinish, sessionFinished, startReport, reportWritten,
  startMetrics, metricsSkipped, returnFailure

total_timeout_reaps_descendant =
  boot, startProcess, deadlineReached, requestTermination, processExited,
  startOutputDrain, outputDrained, reapProcess, startCleanup, cleanupCompleted,
  startSessionFinish, sessionFinished, startReport, reportWritten,
  startMetrics, metricsSkipped, returnFailure

second_interrupt_blocked_finish =
  boot, startProcess, firstInterrupt, requestTermination, processExited,
  startOutputDrain, outputDrained, reapProcess, startCleanup, cleanupCompleted,
  startSessionFinish, startBlocking, secondInterrupt

total_timeout_blocked_finish =
  boot, startProcess, deadlineReached, requestTermination, processExited,
  startOutputDrain, outputDrained, reapProcess, startCleanup, cleanupCompleted,
  startSessionFinish, startBlocking, detachBlocking, returnFailure

process_exit_vs_cancellation =
  boot, startProcess, processExited, firstInterrupt, startOutputDrain,
  outputDrained, reapProcess, startCleanup, cleanupCompleted,
  startSessionFinish, sessionFinished, startReport, reportWritten, returnFailure

cleanup_failure_after_process_failure =
  boot, startProcess, processFailed, startOutputDrain, outputFailed, reapProcess,
  startCleanup, cleanupFailed, startSessionFinish, sessionFailed,
  startReport, reportFailed, returnFailure

output_eof_vs_expiry =
  boot, startProcess, firstInterrupt, requestTermination, processExited,
  startOutputDrain, outputDrained, reapProcess, startCleanup, cleanupCompleted,
  returnFailure

cleanup_completion_vs_expiry =
  boot, startProcess, firstInterrupt, requestTermination, processExited,
  startOutputDrain, outputDrained, reapProcess, startCleanup, cleanupCompleted,
  deadlineReached, startSessionFinish, sessionFinished, returnFailure

session_finish_vs_late_cancellation =
  boot, startCleanup, cleanupCompleted, startSessionFinish, sessionFinished,
  firstInterrupt, startReport, reportWritten, returnFailure

report_write_vs_late_error =
  boot, startCleanup, cleanupCompleted, startSessionFinish, sessionFinished,
  startReport, reportWritten, infrastructureFailed, returnFailure

blocking_completion_vs_detach =
  boot, startBlocking, firstInterrupt, blockingCompleted, detachBlocking,
  returnFailure
```

Serialize JSON fields as stable snake case. Store both the detailed schedule
and terminal observation; Rust may compare only fields observable under the
case's mode and must list omitted fields explicitly.

- [ ] **Step 3: Implement BFS, shrinking, sensitivity, and CLI options**

In `ShutdownAuditMain.lean`, define:

```lean
def eventAlphabet : List Event
def auditDepth : Nat := 9
def reachableUpTo (depth : Nat) : List Reachable
def firstCounterexample? (next : State → Event → Verdict)
    (depth : Nat) : Option (List Event)
def shrinkTrace (next : State → Event → Verdict) (trace : List Event) : List Event
def reachableStateCount : Nat
def checkedTransitionCount : Nat
def sensitivityResults : List (String × Bool × List Event)
```

The BFS must generate all successors from a frontier before deduplication.
Begin with exact-state deduplication. Add no partial-order reduction unless
depth 9 exceeds 30 seconds; if needed, compare reduced and unreduced outputs at
depth 6 and encode the equality check in `ensureAudit`.

Support:

```text
--output PATH
--check PATH
--stats
--sensitivity
```

`ensureAudit` runs before every operation and fails unless the correct model is
safe to depth 9 and all seven broken witnesses are detected. `--stats` prints
depth, alphabet size, states, transitions, corpus count, and reduction mode.
`--sensitivity` prints each family and its shrunk trace.

- [ ] **Step 4: Register executable and observe GREEN**

Add to `lakefile.toml`:

```toml
[[lean_exe]]
name = "generate_shutdown"
root = "ShutdownAuditMain"
```

Import only `ShutdownCases` from `HoiminOracle.lean`; do not import
`ShutdownAuditMain`.

Run:

```bash
lake build generate_shutdown
lake exe generate_shutdown -- --stats
lake exe generate_shutdown -- --sensitivity
```

Expected: correct model safe, seven `detected=true` lines, and statistics below
the 30-second threshold.

- [ ] **Step 5: Generate and verify the corpus**

Run:

```bash
lake exe generate_shutdown -- --output corpus/shutdown-orchestration.jsonl
lake exe generate_shutdown -- --check corpus/shutdown-orchestration.jsonl
test "$(wc -l < corpus/shutdown-orchestration.jsonl)" = 12
git diff --check
```

Expected: deterministic 12-line JSONL and a successful freshness check.

- [ ] **Step 6: Commit explorer and corpus**

```bash
cd ../..
git add formal/HoiminOracle/HoiminOracle.lean
git add formal/HoiminOracle/HoiminOracle/ShutdownCases.lean
git add formal/HoiminOracle/ShutdownAuditMain.lean
git add formal/HoiminOracle/corpus/shutdown-orchestration.jsonl
git add formal/HoiminOracle/lakefile.toml
git commit -m "test: generate Lean shutdown orchestration corpus"
```

---

### Task 5: Real-CLI strict correspondence adapter

**Files:**
- Create: `crates/hoimin-cli/tests/lean_shutdown_oracle.rs`

**Interfaces:**
- Consumes: Task 4 JSONL and `CARGO_BIN_EXE_hoimin`; uses no private production API.
- Produces: strict scenario replay, infrastructure classification, named-case filtering through `HOIMIN_SHUTDOWN_ORACLE_CASE`, and an exact reviewed mismatch set.

- [ ] **Step 1: Write strict corpus structs/parser and a failing adapter shell**

Define all corpus structs with `#[serde(deny_unknown_fields)]`, using strings for
Lean enum names at the parser boundary. Validation requires schema 1, exact
modes/scenarios/events, twelve unique IDs, a nonempty schedule, and one expected
terminal observation.

Define:

```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalObservation {
    cause: Option<String>,
    exit_code: Option<u64>,
    process: String,
    output: String,
    blocking: String,
    workspace: String,
    session: String,
    report: String,
    metrics: String,
    primary_error: Option<String>,
    appended_errors: Vec<String>,
    dispatches: [u64; 4],
    session_complete_flag: bool,
    report_complete_flag: bool,
    returned: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCase {
    schema: u64,
    id: String,
    mode: String,
    scenario: String,
    schedule: Vec<String>,
    expected: TerminalObservation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaseClass { Match, Mismatch, InfrastructureError }

#[derive(Clone, Debug, Eq, PartialEq)]
struct CliObservation {
    exit_code: Option<i32>,
    report: String,
    session: String,
    descendant_running: bool,
    bounded_exit: bool,
}

struct CaseResult {
    id: String,
    class: CaseClass,
    expected: CliObservation,
    actual: Option<CliObservation>,
    detail: Option<String>,
}
```

Initially return `InfrastructureError("adapter not implemented")` from
`run_strict_case`.

- [ ] **Step 2: Run RED**

Run:

```bash
cargo test -p hoimin-cli --test lean_shutdown_oracle -- --nocapture
```

Expected: parser tests pass and `oracle_correspondence` fails with three named
strict infrastructure errors.

- [ ] **Step 3: Implement isolated public-binary fixtures**

Implement these exact helpers in the integration test:

```rust
fn corpus_text() -> &'static str;
fn parse_corpus(input: &str) -> Result<Vec<OracleCase>, String>;
fn write_parallel_project(root: &Path) -> Result<(), String>;
fn python_executable() -> PathBuf;
fn spawn_scenario(case: &OracleCase, paths: &FixturePaths)
    -> Result<tokio::process::Child, String>;
async fn wait_for_readiness(paths: &FixturePaths, child: &mut Child)
    -> Result<FixtureProcesses, String>;
async fn send_first_interrupt(child: &mut Child) -> Result<(), String>;
async fn reap_fixture_processes(processes: &FixtureProcesses) -> Result<(), String>;
fn observe_report(bytes: &[u8]) -> Result<String, String>;
fn observe_session(path: &Path) -> Result<String, String>;
async fn execute_strict_case(case: &OracleCase) -> Result<CliObservation, String>;
```

Use the same controlled Python workload shape as `run_e2e.rs`: the original
test exits normally, while a mutant writes an active PID marker, spawns a
SIGINT-ignoring descendant, and sleeps. Always drain stdout/stderr concurrently
to avoid pipe backpressure. Always reap the Hoimin child and fixture descendants
on success, mismatch, timeout, and panic paths.

Map the three strict scenarios as follows:

- `normal_completion`: run JSON output without a signal; expect exit 0,
  parseable complete report/session, no live descendant, bounded exit;
- `first_interrupt_running`: wait for live incomplete session and descendant,
  send one real signal, expect exit 130, parseable incomplete report/session,
  descendant stopped, bounded exit;
- `total_timeout_reaps_descendant`: use a short public `--total-timeout`, expect
  incomplete exit policy, parseable incomplete report/session, descendant
  stopped, bounded exit.

Convert only Lean fields observable by these scenarios into
`CliObservation`. Put the exact projection function in Rust and test that it
rejects a strict case whose Lean terminal state cannot determine all five CLI
fields.

- [ ] **Step 4: Add parser, panic, teardown, and mismatch-set tests**

Add exact tests:

```rust
#[test] fn corpus_is_well_formed();
#[test] fn corpus_rejects_unknown_schema_mode_scenario_event_and_state();
#[test] fn corpus_rejects_duplicate_case_ids();
#[test] fn strict_projection_rejects_unobservable_expectations();
#[tokio::test] async fn case_panics_are_infrastructure_errors_and_teardown_runs();
#[tokio::test] async fn oracle_correspondence();
```

Wrap each case in `catch_unwind(AssertUnwindSafe(...))` and make teardown a
separate guard whose failure is infrastructure error. Read
`HOIMIN_SHUTDOWN_ORACLE_CASE`; fail if absent from the corpus or not strict.

Start with `const REVIEWED_MISMATCHES: &[&str] = &[]`. If a real difference is
found, rerun the single case, verify the harness teardown, classify it under
Task 7, and add only the reviewed ID. The test must fail if the actual mismatch
set differs from the constant or any infrastructure error occurs.

- [ ] **Step 5: Run GREEN and every strict case individually**

Run:

```bash
cargo test -p hoimin-cli --test lean_shutdown_oracle -- --nocapture
HOIMIN_SHUTDOWN_ORACLE_CASE=normal_completion cargo test -p hoimin-cli --test lean_shutdown_oracle oracle_correspondence -- --exact --nocapture
HOIMIN_SHUTDOWN_ORACLE_CASE=first_interrupt_running cargo test -p hoimin-cli --test lean_shutdown_oracle oracle_correspondence -- --exact --nocapture
HOIMIN_SHUTDOWN_ORACLE_CASE=total_timeout_reaps_descendant cargo test -p hoimin-cli --test lean_shutdown_oracle oracle_correspondence -- --exact --nocapture
cargo clippy -p hoimin-cli --test lean_shutdown_oracle -- -D warnings
```

Expected: zero infrastructure errors and only the exact reviewed mismatch set.

- [ ] **Step 6: Commit the adapter**

```bash
git add crates/hoimin-cli/tests/lean_shutdown_oracle.rs
git commit -m "test: compare shutdown with Lean oracle"
```

---

### Task 6: Internal-fixture and race evidence

**Files:**
- Modify: `docs/superpowers/reports/2026-08-09-shutdown-orchestration-coverage-worksheet.md`
- Conditionally modify: `crates/hoimin-cli/tests/lean_shutdown_oracle.rs` only when a missing observation is achievable without production-source changes.

**Interfaces:**
- Consumes: Task 1 rows, Task 4 internal/model cases, Task 5 harness, and existing unit/E2E fixtures.
- Produces: evidence or an explicit same-premise limitation for SHUT-01 through SHUT-12. No row remains merely `missing`.

- [ ] **Step 1: Run the exact existing internal and E2E evidence**

Run:

```bash
cargo test -p hoimin-cli interrupt::tests::second_signal_forces_130_without_waiting_for_first_consumer -- --exact --nocapture
cargo test -p hoimin-cli shell::tests::shutdown_budget_first_activation_cannot_be_extended -- --exact --nocapture
cargo test -p hoimin-cli shell::tests::shutdown_budget_preempts_an_owned_blocking_close -- --exact --nocapture
cargo test -p hoimin-cli shell::tests::expired_final_close_detaches_cleanup_without_extending_the_wait -- --exact --nocapture
cargo test -p hoimin-cli shell::tests::shutdown_drain_expiry_reports_process_and_blocking_task_counts -- --exact --nocapture
cargo test -p hoimin-cli process::tests::cancellation_and_timeout_precede_a_simultaneous_exit -- --exact --nocapture
cargo test -p hoimin-cli process::tests::cleanup_failures_preserve_primary_error_and_append_details_in_order -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e first_sigint_finishes_a_parseable_incomplete_session -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e second_sigint_forces_130_while_session_finish_is_blocked -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e total_timeout_exits_after_grace_when_session_finish_is_locked -- --exact --nocapture
```

On Windows use the exact `ctrl_c_event` counterparts. A platform-filtered zero
test count is recorded as not applicable, not a pass.

- [ ] **Step 2: Compare each internal/model schedule with actual branch order**

For each non-strict case, trace the corresponding branch through:

```text
establish_event_shutdown_budget
shutdown_expiry_error
drain_processes
accept_drained_completion
finish_failed_event_drain
close_context_resources / detach_context_resources
finalize_metrics_with_shutdown
finish_with_interrupt_monitor
```

Record file/line anchors, owned values before and after the await, and whether
the production path can realize the same ordering. Mark each worksheet row
`covered`, `strict mismatch`, `internal-fixture mismatch`, or `model-only — no
same-premise seam`, with the exact reproduction command.

- [ ] **Step 3: Add only integration-level missing evidence**

If a missing observation can be exposed using the real binary, public CLI
arguments, OS signals, SQLite locks, filesystem permissions, or controlled test
processes, add a named scenario to the Task 5 adapter and a regenerated Lean
case. Do not add hooks to `shell.rs`, `process/mod.rs`, or any other production
module. Repeat Task 4 sensitivity/freshness and all Task 5 tests after each
case.

- [ ] **Step 4: Close every worksheet row and commit**

Run:

```bash
rg -n 'missing —|T[B]D|T[O]DO' docs/superpowers/reports/2026-08-09-shutdown-orchestration-coverage-worksheet.md
git diff --check
```

Expected: `rg` prints nothing. If only documentation changed:

```bash
git add docs/superpowers/reports/2026-08-09-shutdown-orchestration-coverage-worksheet.md
git commit -m "docs: complete shutdown evidence worksheet [skip ci]"
```

If the corpus/adapter also changed, commit code and generated corpus first with
`test: extend shutdown race correspondence`, then commit the worksheet with
`[skip ci]`.

---

### Task 7: Audit report and repair-ready counterexample ledger

**Files:**
- Create: `docs/superpowers/reports/2026-08-09-lean-shutdown-orchestration-audit.md`
- Conditionally create: `docs/superpowers/reports/2026-08-09-lean-shutdown-orchestration-counterexamples.md`

**Interfaces:**
- Consumes: theorem list, explorer statistics, sensitivity witnesses, strict adapter classifications, and completed coverage worksheet.
- Produces: final scope/cost/evidence report and one complete ledger section per unresolved mismatch.

- [ ] **Step 1: Collect fresh timed formal evidence**

Run from `formal/HoiminOracle`:

```bash
/usr/bin/time -p lake build
/usr/bin/time -p lake exe generate_shutdown -- --check corpus/shutdown-orchestration.jsonl
/usr/bin/time -p lake exe generate_shutdown -- --stats
/usr/bin/time -p lake exe generate_shutdown -- --sensitivity
rg -n '^theorem ' HoiminOracle/ShutdownProofs.lean
```

Record exact times, depth, alphabet, states, transitions, reduction mode,
corpus count, theorem names, and seven shrunk broken witnesses.

- [ ] **Step 2: Rerun and classify every strict/internal difference**

Run the full adapter and each mismatching ID separately. For each difference,
choose exactly one classification: `confirmed bug`, `specification ambiguity`,
`model defect`, or `infrastructure error`. Correct and regenerate only model or
adapter defects; never adjust a valid expectation to match Rust.

- [ ] **Step 3: Write the report with exact claim boundaries**

Use these sections:

```markdown
# Lean Shutdown Orchestration Audit Report

## Result
## Audited contract and excluded scope
## Existing-evidence worksheet summary
## Formal state and event model
## What Lean proved
## What remained bounded
## Exploration size, reductions, and cost
## Broken-family sensitivity
## Strict real-CLI correspondence
## Internal-fixture evidence
## Model-only boundaries
## Mismatches and classifications
## Model, adapter, timing, and platform limitations
## Repair handoff
## Exact reproduction commands
```

State explicitly that Lean proves properties of the model, not compiled Rust,
and that timing correspondence comes from controlled CLI observations.

- [ ] **Step 4: Write a complete ledger entry for every unresolved mismatch**

Each entry must contain all fields below, with no blank values:

```text
claim:
mode:
model boundary:
minimal event trace:
state before each event:
state after each event:
Lean expected observation:
Rust actual observation:
classification:
production reachability:
root-cause function and branch:
owned value or effect identity:
user impact:
recommended repair:
alternative repair:
compatibility risk:
deadlock/cancellation/data-loss risk:
first RED repair test:
focused verification:
full verification:
owner decision:
```

For the first RED test, include compilable Rust test code using an existing
public or internal seam and identify the separate repair worktree file path.
Do not implement the test or repair in this audit branch.

- [ ] **Step 5: Self-review and commit documentation**

Run:

```bash
rg -n 'T[B]D|T[O]DO|proved production|unbounded production|fix later|unknown' \
  docs/superpowers/reports/2026-08-09-lean-shutdown-orchestration-audit.md \
  docs/superpowers/reports/2026-08-09-lean-shutdown-orchestration-counterexamples.md
git diff --check
```

Omit the counterexample path from `rg` if no ledger is needed. Resolve every
hit or explicitly rephrase a genuine limitation. Then commit:

```bash
git add docs/superpowers/reports/2026-08-09-lean-shutdown-orchestration-audit.md
git add docs/superpowers/reports/2026-08-09-lean-shutdown-orchestration-counterexamples.md
git commit -m "docs: report Lean shutdown orchestration audit [skip ci]"
```

Omit the second `git add` when no counterexample file exists.

---

### Task 8: Final reproducibility, regression, and scope verification

**Files:**
- Verify only; modify only audit artifacts when a verification defect is found.

**Interfaces:**
- Consumes: every preceding artifact.
- Produces: fresh completion evidence, clean changed-path list, and a CI-running branch tip.

- [ ] **Step 1: Verify every Lean corpus and sensitivity gate**

Run:

```bash
cd formal/HoiminOracle
lake build
lake exe generate -- --check corpus/state-machine.jsonl
lake exe generate_budget -- --check corpus/budget-cleanup.jsonl
lake exe generate_shutdown -- --check corpus/shutdown-orchestration.jsonl
lake exe generate_shutdown -- --stats
lake exe generate_shutdown -- --sensitivity
```

Expected: all builds/freshness checks pass, correct model has no bounded
counterexample, and every broken family is detected.

- [ ] **Step 2: Verify oracle adapters and focused shutdown behavior**

Run:

```bash
cd ../..
cargo test -p hoimin-core --test lean_oracle
cargo test -p hoimin-core --test lean_budget_oracle
cargo test -p hoimin-cli --test lean_shutdown_oracle -- --nocapture
cargo test -p hoimin-cli --test run_e2e first_sigint_finishes_a_parseable_incomplete_session -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e second_sigint_forces_130_while_session_finish_is_blocked -- --exact --nocapture
cargo test -p hoimin-cli --test run_e2e total_timeout_exits_after_grace_when_session_finish_is_locked -- --exact --nocapture
```

Use Windows counterpart names when applicable and record platform exclusions.

- [ ] **Step 3: Verify workspace-wide quality and tests**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check origin/main..HEAD
git diff --name-only origin/main..HEAD
git status -sb
```

Expected: all commands pass; changed paths are limited to the design, plan,
coverage worksheet, formal shutdown modules/executable/corpus, CLI integration
adapter, audit report, and optional counterexample ledger. No path matching
`crates/*/src/*` appears. `.venv` is the only permitted local untracked setup
entry and must be unlinked before final status.

- [ ] **Step 4: Ensure the branch tip runs CI**

If the current tip contains `[skip ci]`, create:

```bash
git commit --allow-empty -m "ci: validate Lean shutdown orchestration audit"
```

Do not create an empty commit if the tip already triggers CI.

- [ ] **Step 5: Report and stop at the audit boundary**

Report theorem scope, finite depth/state/transition cost, sensitivity results,
strict match/mismatch counts, internal fixture results, infrastructure errors,
changed-path verification, report/ledger paths, and repair owner decisions.
Do not implement any production repair in this worktree.
