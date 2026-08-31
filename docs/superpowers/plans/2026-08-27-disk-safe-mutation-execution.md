# Disk-safe mutation execution implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prevent `hoimin run` and the repository focused Rust mutation workflow from
exhausting disk by enforcing owned-byte and free-space limits, bounding retained logs,
and cleaning only leased Hoimin-owned scratch on every catchable exit.

**Architecture:** A small pure disk-policy state machine is shared semantically through
a Lean-generated corpus. The Rust CLI adds a run-wide monitor around existing worker
workspaces and feeds a typed infrastructure stop into the existing shell shutdown path.
The Python wrapper owns cargo-mutants scratch, monitors it with the same policy, drains
bounded logs, extracts compact evidence, and deletes each bulky candidate tree before
continuing. Both implementations use locked leases and a conservative direct-child
janitor; neither accepts arbitrary cleanup paths from child output or reports.

**Tech Stack:** Rust 1.88+ (`tokio`, `tempfile`, `fs2` 0.4.3, `crc32fast` 1.4,
`cap-std` 4.0.2,
`cap-fs-ext` 4.0.2, `serde`), Python 3.14 standard library, Lean 4/Lake,
cargo-mutants 27.1.0.

**Spec:**
`docs/superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md`

## Review record

This plan received ten passes on 2026-08-27 and 2026-08-28: architecture/security,
implementation/TDD correspondence, formal/race/delivery, guard resource consumption,
cross-platform concurrency, formal executability, orphan-process recovery,
cleanup-resource bounds, capture/schema consistency, and platform/formal executability.
Material findings are folded into the steps. Task 10
repeats three same-SHA implementation reviews because document approval is not
implementation evidence.

Six additional zero-based passes on 2026-08-28 covered launcher/classification,
formal lifecycle, implementation correspondence, crash/disk-full recovery, bounded
capture, and executable shell gates. They found unguarded environment creation,
unobservable handled `SIGXFSZ` failures, unsafe cleanup after failed components,
incorrect finish acceptance, Rust/Python root-topology drift, crash-orphaned report
temporaries, unbounded tool-controlled reads, and fail-open no-match checks. The tasks
below contain those corrections.

A Windows-native implementation review on 2026-08-31 corrected two assumptions that
Unix testing and compile-only Windows coverage had not established. First, regular files
do not retain directory ACE inheritance flags, so exact protected-DACL verification must
use object-kind-specific descriptors. Second, delete sharing does not allow a directory
rename while a descendant marker is open. The resulting design uses a coordinator-held,
identity-checked close/rename/reopen/relock handoff. It also reads a locked lease through
the locking handle because Windows byte-range locks are mandatory for competing handles.
An A/B run retained native handle-relative rename and removed speculative parent rights
and NUL padding: the Win32 rename wrapper returned `ERROR_INVALID_PARAMETER`, while
`NtSetInformationFile(FileRenameInformation)` succeeded with the live parent handle.
The subsequent cleanup regression established a separate acquisition-order rule:
opening a new `DELETE`-capable directory handle during cleanup fails while a live
handle to that directory denies delete sharing. A shared descendant can permit the late
open but still prevents the directory rename until it closes. The live owner must
acquire rename capability before creating markers and retain the same handle through
publication and claim. A janitor, which cannot inherit a crashed process's handle, must
acquire its candidate `DELETE` handle before opening any descendant marker.
Managed-child quiescence likewise may be published only after every sibling descendant
handle has closed.

## Global Constraints

- Work only in an isolated worktree on branch `feat/disk-safe-mutation`, tracking
  `origin/feat/disk-safe-mutation`. `origin/main` is the PR base/comparison ref, not this
  branch's upstream; do not change the upstream to `origin/main`.
- Before each task, require a clean tracked worktree and record the exact HEAD. Preserve
  ignored evidence under `.superpowers/sdd/2026-08-27-disk-safe-mutation-execution/`.
- Use `apply_patch` for source and documentation edits. Do not delete repository
  `target/`, Cargo caches, user output, or any path not created by the current task.
- Use `superpowers:test-driven-development` for every behavior change,
  `lean-test-oracle` for Tasks 1–2 and 8, `superpowers:systematic-debugging` for an
  unexpected failure, and `superpowers:verification-before-completion` before a
  delivery claim.
- Use small fake byte counts in unit tests. Inject iterators/counters for the 250,001-entry
  and 100,001-child boundaries; never materialize those trees or create GiB-scale fixtures.
- Keep only one full command spool in memory. Never copy a per-command retained log into
  every candidate record; candidate diagnostics are separately and globally bounded.
- Do not run Rust mutation testing. On 2026-08-30 the user explicitly disabled both
  full-workspace and focused cargo-mutants execution after repeated disk-capacity
  incidents. Task 10 records the mutation gate as deliberately omitted; ordinary,
  formal, compatibility, and native lifecycle gates remain required.
- Provision the locked Python environment as an explicit setup gate for tests and wheel
  smoke verification. Do not invoke the focused mutation wrapper in this delivery.
- Start every compatibility build only when more than 10 GiB is free. Stop immediately
  on ENOSPC, rising unowned scratch, or monitor/tool failure.
- A disk stop is an infrastructure failure. It must not become `Killed`, increment the
  mutation score, or overwrite a prior primary failure.
- After every code-review or CI-driven edit, invalidate evidence for the old SHA and
  repeat the affected gates and independent reviews on one clean final SHA.
- Commit only the files named by the task. Keep generated Lean corpus files tracked;
  keep runtime reports and temporary mutation output ignored.

## Execution partition

This program plan contains seven implementation batches. Execute and review them in
order: Task 1; Tasks 2–4; Task 5; Task 6; Task 7; Tasks 8–9; Task 10.
Do not assign two partitions to one implementation turn. Task 5 and Task 7 each require
their own clean-start brief and final commit review because they own different languages
and deletion implementations.

Within a partition, each bullet in a RED list is one TDD increment: add one behavioral
assertion, run its narrow target and confirm the intended behavioral failure, add the
smallest production change, rerun that target, then continue. A numbered Step is a
review checkpoint, not permission for a single bulk edit. Record the command and failure
cause for each increment in ignored evidence; compile, import, fixture-setup, and ENOSPC
failures do not count as RED.

---

### Task 1: Define and prove the disk decision/lifecycle oracle

**Files:**

- Create: `formal/HoiminOracle/HoiminOracle/DiskGuardModel.lean`
- Create: `formal/HoiminOracle/HoiminOracle/DiskGuardCases.lean`
- Create: `formal/HoiminOracle/HoiminOracle/DiskGuardProofs.lean`
- Create: `formal/HoiminOracle/DiskGuardAuditMain.lean`
- Modify: `formal/HoiminOracle/lakefile.toml`
- Create: `formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl`
- Create: `formal/HoiminOracle/DiskGuardBrokenConsumer.lean`

**Interfaces:**

- Consumes: the approved threshold, stop-precedence, process-drain, monitor-join, and
  root-specific cleanup contract from the design.
- Produces: `HoiminOracle.DiskGuard.step`, `run`, `Invariant`, the fourteen named theorems,
  an eight-event-family depth-five bounded explorer, and schema-1 JSONL records with
  exact correspondence mode, `policy` or `runtime` layer, and nonempty
  `implementation_targets`. Tasks 2, 6, and 8 consume every record naming their
  implementation and may not reinterpret its premises.

- [ ] **Step 1: Write the finite model and fixed cases**

Model thresholds symbolically, not as GiB values:

```lean
import Std

namespace HoiminOracle.DiskGuard

inductive StopReason where
  | sizeExceeded | reserveReached | measurementFailed | processFailed
  deriving BEq, DecidableEq, Repr

abbrev RootId := Fin 2

inductive ComponentState where | pending | active | succeeded | failed
  deriving BEq, DecidableEq, Repr

structure State where
  stop : Option StopReason := none
  secondaryStops : List StopReason := []
  active : Nat := 0
  dispatched : Nat := 0
  ownedRoots : List RootId := []
  deliveryRoots : List RootId := []
  cleanupRequested : List RootId := []
  cleanupClean : List RootId := []
  cleanupFailed : List RootId := []
  cleanupDeferred : List RootId := []
  cleanupRetained : List RootId := []
  processDrain : ComponentState := .pending
  outputDrain : ComponentState := .pending
  monitorJoin : ComponentState := .pending
  report : ComponentState := .pending
  finished : Bool := false
  deriving BEq, Repr

inductive Event where
  | dispatch
  | observe (owned maxOwned free minFree : Nat)
  | meterFailed
  | processDrainFailed
  | processDrainSucceeded
  | requestCleanup (root : RootId)
  | cleanupSucceeded (root : RootId)
  | cleanupFailed (root : RootId)
  | cleanupDeferred (root : RootId)
  | cleanupRetained (root : RootId)
  | outputDrained
  | monitorJoined
  | monitorJoinFailed
  | reportSucceeded
  | reportFailed
  | finish
  deriving BEq, Repr
```

The pinned Lean project imports `Std` and has no Mathlib dependency, so do not use
`Finset`. Add insertion helpers that check membership and prove `List.Nodup` for all seven
root collections plus `secondaryStops` along accepted traces. The fixed domain
contains two root IDs, so list lookup remains bounded and the JSON order is deterministic.

Use `owned >= maxOwned` and `free <= minFree` as inclusive terminal boundaries.
When both hold in one successful observation, choose `reserveReached` and retain
`sizeExceeded` as secondary evidence; a failed measurement carries no numeric reading.
Insert a secondary reason once and preserve observation order. `step` must preserve the
first stop reason, reject dispatch once `stop.isSome`, permit
exactly one logical cleanup request per owned root, and reject `finish` until active
work is zero, the process-drain/output-drain/monitor/report components are settled (`succeeded` or
`failed`), and every owned
root has exactly one terminal cleanup outcome (`clean`, `failed`, or `retained`); a
`deferred` outcome remains visible but blocks `finish`. It rejects a cleanup request for
a root until process-drain, output-drain, and monitor components are settled. It accepts
`cleanupSucceeded` or `cleanupFailed` only when all three components succeeded; if any
failed, only `cleanupDeferred` or `cleanupRetained` is valid. It also rejects a cleanup
request for a root in `deliveryRoots` until `report` is settled. A failed report write
therefore still permits the required delivery-root cleanup after the three deletion
safety components succeeded.

The finite Lean state records each secondary stop-reason class once. Rust and Python
retain ordered structured evidence and deduplicate only an exactly equal observation or
an exactly equal `(code, message)` error before projecting to that finite reason list.
In addition, accept `finish` only when `report = succeeded` and every root in
`deliveryRoots` is in `cleanupClean`. Report failure or failed delivery cleanup remains a
settled error trace whose final `finish` event is rejected. Execution-root failed or
retained cleanup may finish when report and delivery cleanup succeed.
`processDrainSucceeded` and `processDrainFailed` are one global shutdown boundary, not
one child exit; each sets `active = 0` and the corresponding `processDrain` state. The
model therefore does not confuse one of two child completions with proof that every
owned process was reaped.
Construct `DiskLifecycle` only after a process/drain/monitor owner starts. Setup rollback
before any owner starts is an explicit `ManagedRunRoot` fixture outside this model; do
not manufacture pending-component cleanup events to represent it. Add that distinction
to the correspondence worksheet as `internal-fixture`.

Define strict fixed cases for below/at/above thresholds, meter failure, zero/one/two
active processes, two competing stop reasons, cleanup
success/failure/deferral/retention, and report success/failure. Normalize them to
dispatch, observe, process-drain terminal, output
terminal, monitor terminal, cleanup, report terminal, and finish. Explore family
skeletons through depth five (`1 + 8 + 8² + 8³ + 8⁴ + 8⁵ = 37,449`). Do not expand
that set across a larger root/payload alphabet. Use canonical representatives, but do
not generalize the bounded result to arbitrary noncanonical traces. Validate a finite
set of root-renaming and payload-class samples, and add one fixed witness for every
noncanonical root and payload class used by the correspondence corpus.
Add fixed traces outside the depth-five explorer for complete ordered success,
report-failure then delivery cleanup and rejected finish, delivery-cleanup failure and
rejected finish, monitor-failure then deferred cleanup and rejected
finish, rejected cleanup before component settlement, rejected destructive cleanup after
a failed component, and explicit retention. Bound each fixed trace at 16 events. These
long traces are mandatory corpus records; the depth-five count is only the normalized
local refutation pass and cannot stand in for lifecycle correspondence.
Every JSONL record must contain schema `1`, case ID, correspondence mode, layer, a
nonempty duplicate-free `implementation_targets` list containing only `rust` and/or
`python`, event sequence, and the complete expected terminal observation. Policy cases
target both; complete one-root focused-runtime cases target Python; complete two-root
delivery cases target Rust because Python's output root is user-owned and not a leased
cleanup root. A smaller runtime case targets both only with an explicit same-premise
worksheet row for each adapter. Record transition/state
counts, elapsed time, and peak memory in the generator summary.
Report `bounded_skeleton_count=37449`, `fixed_case_count`, and total corpus records as
separate values; do not assert that the corpus line count equals 37,449 after adding the
fixed traces.

For every mismatch and every deliberate broken witness, emit a deterministic audit
record containing: claim, model boundary, finite domain or theorem premises, minimal
trace, intermediate states, classification, implementation correspondence, owner
question, and exact reproduction command. Resource-walker, OS-handle, signal, and
renderer-memory claims remain outside this model and must not be promoted from their
runtime `internal-fixture` tests to Lean proof claims.

- [ ] **Step 2: Add proof obligations and deliberate broken variants**

Prove fourteen named obligations: `stopped_never_dispatches`,
`settled_process_drain_rejects_dispatch`, `active_work_rejects_cleanup`,
`first_reason_is_sticky`, `simultaneous_threshold_preserves_secondary`,
`cleanup_requested_once_per_root`,
`cleanup_failure_is_not_clean`, `cleanup_request_implies_components_settled`,
`destructive_cleanup_implies_components_succeeded`,
`finished_implies_cleanup_terminal`, and
`finished_implies_components_settled`, `delivery_cleanup_after_report_settled`,
`finished_implies_report_succeeded`, and `finished_implies_delivery_clean`. The two
single-transition guard theorems state their exact rejected event premise; the other
twelve quantify over a starting state and an accepted event trace and state their
property on the trace fold. This shared trace premise prevents those proofs from
drifting away from the corpus generator.
`finished_implies_cleanup_terminal` must prove that no owned root remains deferred.
Add finite root-renaming and payload-class sample checks for the bounded explorer; these
checks are regression evidence, not universal commutation lemmas. The generator must
fail if a representative lacks a corresponding fixed witness. Root-renaming samples
map `ownedRoots`, `deliveryRoots`, all five cleanup collections, and root-bearing events
together; they may not swap an event root without the state's delivery role. Payload
samples pair observations with identical `owned >= maxOwned` and `free <= minFree`
truth values and require the same policy transition, including simultaneous-stop
secondary evidence. Cover both root IDs and every below/at/above threshold class used
by the tracked corpus. The depth-five result remains explicitly canonical-only.

Add broken functions for strictness (`>` instead of `>=`), reserve direction,
lost simultaneous secondary evidence, stop-reason overwrite, post-stop dispatch,
post-process-drain dispatch,
duplicate cleanup, clean-on-cleanup-error, and early finish. `--sensitivity` must return zero only if every broken family is
distinguished by at least one fixed witness.
Add two lifecycle broken variants: one accepts cleanup before component settlement, and
one accepts clean/failed destructive cleanup after process, drain, or monitor failure.
Add broken finish variants that accept report failure or failed delivery cleanup. The
long fixed witnesses, rather than incidental depth-five enumeration, must kill all four.

- [ ] **Step 3: Add the generator and initially stale corpus RED**

Register:

```toml
[[lean_exe]]
name = "generate_disk_guard"
root = "DiskGuardAuditMain"
```

Before generating the tracked corpus, run the freshness check and require nonzero:

```bash
cd formal/HoiminOracle
lake exe generate_disk_guard -- --check corpus/disk-guard-lifecycle.jsonl
```

The only accepted RED is missing/stale corpus. A Lean compile error is not accepted.

- [ ] **Step 4: Generate, prove, and verify under the Lean resource guard**

Run sequentially:

```bash
cd formal/HoiminOracle
../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/disk-guard-build.json -- lake build HoiminOracle.DiskGuardProofs generate_disk_guard
../../.venv/bin/python tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/disk-guard-sensitivity.json -- lake exe generate_disk_guard -- --sensitivity
lake exe generate_disk_guard -- --output corpus/disk-guard-lifecycle.jsonl
lake exe generate_disk_guard -- --check corpus/disk-guard-lifecycle.jsonl
lake env lean DiskGuardBrokenConsumer.lean
```

`DiskGuardBrokenConsumer.lean` must verify that every broken family has a witness; it
must not import an undefined symbol merely to manufacture a RED.

- [ ] **Step 5: Review and commit**

Self-review theorem premises against the approved design three times: threshold
boundaries; lifecycle ordering; sensitivity completeness. Then run:

```bash
git diff --check
git status --short
```

Commit:

```bash
git add formal/HoiminOracle/HoiminOracle/DiskGuardModel.lean \
  formal/HoiminOracle/HoiminOracle/DiskGuardCases.lean \
  formal/HoiminOracle/HoiminOracle/DiskGuardProofs.lean \
  formal/HoiminOracle/DiskGuardAuditMain.lean \
  formal/HoiminOracle/DiskGuardBrokenConsumer.lean \
  formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl \
  formal/HoiminOracle/lakefile.toml
git commit -m "formal: define disk guard lifecycle oracle"
```

### Task 2: Add the pure Rust disk policy and Lean adapter

**Files:**

- Create: `crates/hoimin-core/src/disk.rs`
- Modify: `crates/hoimin-core/src/lib.rs`
- Create: `crates/hoimin-core/tests/disk_policy.rs`
- Create: `crates/hoimin-core/tests/lean_disk_guard_oracle.rs`

**Interfaces:**

- Consumes: schema-1 `disk-guard-lifecycle.jsonl` from Task 1.
- Produces: dependency-pure `DiskPolicy`, `DiskObservation`, `DiskDecision`,
  `DiskFailure`, `DiskSecondary`, `DiskRootId`, `DiskLifecycle`,
  `DiskCleanupOutcome`, and `DiskLifecycleSnapshot` exports from `hoimin_core`. Task 3
  persists the policy; Task 6 wires lifecycle events to the shell.

- [ ] **Step 1: Write the adapter RED against the intended public API**

Parse the corpus with `#[serde(deny_unknown_fields)]` and invoke only these public
entry points:

```rust
pub const DISK_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

pub struct DiskPolicy {
    pub max_owned_bytes: NonZeroU64,
    pub min_free_bytes: NonZeroU64,
}

pub struct DiskObservation {
    pub owned_bytes: u64,
    pub available_bytes: u64,
    pub measured_in: Duration,
}

pub enum DiskStopReason {
    WorkspaceSizeExceeded,
    FilesystemReserveReached,
    MeasurementFailed,
    ProcessFailed,
}

pub enum DiskSecondary {
    Observation { reason: DiskStopReason, value: DiskObservation },
    Error { code: String, message: String },
}

pub enum DiskRootId {
    Execution,
    Delivery,
}

pub enum DiskCleanupOutcome {
    Clean,
    Failed(String),
    Deferred(String),
    Retained,
}

pub enum DiskLifecycleError {
    DuplicateRoot(DiskRootId),
}

pub enum DiskLifecycleEvent {
    DispatchRequested,
    Observation { policy: DiskPolicy, value: DiskObservation },
    MeasurementFailed { message: String },
    ProcessDrainSucceeded,
    ProcessDrainFailed,
    OutputDrainSucceeded,
    OutputDrainFailed,
    MonitorJoinSucceeded,
    MonitorJoinFailed,
    ReportSucceeded,
    ReportFailed,
    CleanupRequested { root: DiskRootId },
    CleanupCompleted { root: DiskRootId, outcome: DiskCleanupOutcome },
    FinishRequested,
}

impl DiskLifecycle {
    pub fn new(
        owned_roots: impl IntoIterator<Item = DiskRootId>,
    ) -> Result<Self, DiskLifecycleError>;
    pub fn apply(&mut self, event: DiskLifecycleEvent) -> bool;
    pub fn may_finish(&self) -> bool;
    pub fn snapshot(&self) -> DiskLifecycleSnapshot;
}
```

Run and require a compile-failure RED caused only by missing `hoimin_core::disk`:

```bash
cargo test -p hoimin-core --test lean_disk_guard_oracle --no-run
```

- [ ] **Step 2: Implement the pure policy**

Implement inclusive thresholds, reserve-before-size precedence for one observation, and
first-reason stickiness. `DiskDecision` is
`Continue` or `Stop(DiskFailure)`; `DiskFailure` stores a stable code, the first
observation when present, and ordered structured `DiskSecondary` values. The
`available_bytes` policy input is the minimum across the reading's filesystem map; the
report retains the full map. Expose exact codes:

```rust
pub const WORKSPACE_SIZE_EXCEEDED: &str = "workspace.size.exceeded";
pub const FILESYSTEM_RESERVE_REACHED: &str = "filesystem.reserve.reached";
pub const DISK_MEASUREMENT_FAILED: &str = "disk.measurement.failed";
pub const PROCESS_LIFECYCLE_FAILED: &str = "process.failed";
pub const WORKSPACE_CLEANUP_FAILED: &str = "workspace.cleanup.failed";
pub const WORKSPACE_CLEANUP_DEFERRED: &str = "workspace.cleanup.deferred";
```

Keep the decision and transition seams uniquely named `evaluate_disk_policy` and
`apply_disk_lifecycle_event`; public `DiskLifecycle` methods delegate to them. These are
production helpers, not test-only duplicates, and ordinary/formal tests target them
directly without mutation execution.
`apply_disk_lifecycle_event` consumes the public `DiskLifecycleEvent` above, so both
corpus adapters exercise process, drain, monitor, report, cleanup, and finish transitions
through one complete interface. `DiskRootId` is a two-variant enum matching Lean
`Fin 2`; no public constructor may create a third root ID.
Allow zero, one, or both roots as the finite model does, but reject a duplicate root in
the constructor rather than silently changing the caller's cleanup obligations. The
Python constructor has the same result and error code.

Do not add a disk variant to `ProcessTermination`; a disk stop is never a test
termination. Use checked counters and make duplicate logical cleanup requests return
`false` without incrementing state.

- [ ] **Step 3: Complete policy and correspondence tests**

Cover both boundaries, precedence, secondary errors, zero/one/two active work items,
duplicate cleanup, cleanup absence failure, deferred cleanup, rejection before component
settlement, rejection of destructive cleanup after any failed safety component, the
complete policy-layer fixed cases, duplicate-root constructor rejection, and finish gating.
Finish gating includes report-failure and delivery-cleanup-failure rejection.
The Lean adapter must execute every `policy` record whose targets include `rust` through
public methods, reject unknown layers/modes/targets and duplicate or empty target lists,
classify each applicable case as match/mismatch/infrastructure error, and require:

- every strict case matches;
- every applicable policy case was executed exactly once;
- no reviewed mismatch exists;
- no infrastructure error exists;
- locally broken Rust policy functions fail at least one case each.

Run:

```bash
cargo test -p hoimin-core --test disk_policy -- --nocapture
cargo test -p hoimin-core --test lean_disk_guard_oracle -- --nocapture
cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 4: Commit**

```bash
git add crates/hoimin-core/src/disk.rs crates/hoimin-core/src/lib.rs \
  crates/hoimin-core/tests/disk_policy.rs \
  crates/hoimin-core/tests/lean_disk_guard_oracle.rs
git commit -m "feat: define portable disk guard policy"
```

### Task 3: Persist and validate the new limits

**Files:**

- Modify: `crates/hoimin-core/Cargo.toml`
- Modify: `crates/hoimin-core/src/config.rs`
- Modify: `crates/hoimin-core/src/resume.rs`
- Modify: `crates/hoimin-core/src/report.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/tests/lean_oracle.rs`
- Modify: `crates/hoimin-core/tests/lean_report_sequence_oracle.rs`
- Modify: `crates/hoimin-core/tests/plan_config.rs`
- Modify: `crates/hoimin-core/tests/resume_policy.rs`
- Modify: `crates/hoimin-core/tests/report_policy.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-cli/src/plan.rs`
- Modify: `crates/hoimin-cli/src/report/human.rs`
- Modify: `crates/hoimin-cli/src/session/mod.rs`
- Modify: `crates/hoimin-cli/src/progress/input.rs`
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Modify: `crates/hoimin-cli/tests/report_heap.rs`
- Modify: `crates/hoimin-cli/tests/session_handler.rs`
- Create: `crates/hoimin-cli/tests/golden/reports/schema-v3-original.json`
- Create: `crates/hoimin-cli/tests/golden/reports/schema-v3-current.json`
- Create: `crates/hoimin-cli/tests/golden/events/schema-v3-original.jsonl`
- Create: `crates/hoimin-cli/tests/golden/events/schema-v3-current.jsonl`
- Modify: `docs/json-schema/run-event.schema.json`
- Modify: `docs/json-schema/run-result.schema.json`

**Interfaces:**

- Consumes: Task 2 disk-policy values and the existing byte parser, plan manifest,
  fingerprint encoder, session dispatcher, report state, and machine cleanup flags.
- Produces: normalized `RunLimits::{max_workspace_size,min_free_space}`, plan schema 3,
  fingerprint schema 5, report schema 3, typed old-session incompatibility, and the
  machine-owned portion of `DiskRunSummary`. Tasks 4–6 must use these fields directly.

- [ ] **Step 1: Add failing defaults/serialization/fingerprint tests**

Assert exact defaults and normalized values:

```rust
assert_eq!(RawRunLimits::default().max_workspace_size, 8 * 1024 * 1024 * 1024);
assert_eq!(RawRunLimits::default().min_free_space, 10 * 1024 * 1024 * 1024);
```

Require zero to fail with `--max-workspace-size` or `--min-free-space`. Require two
otherwise-identical fingerprints to differ when either field differs. Require plan
schema 2 and fingerprint schema 4 fixtures to fail explicitly with “regenerate the
plan” or “start a new session”; do not deserialize defaults into persisted artifacts.

Add a SQLite fixture containing a newest incomplete run whose fingerprint row has
schema 4. With an explicit resume request and no schema-5 digest match, require typed
`session.resume.incompatible` rather than `resume: None`. Add controls for a matching
schema-5 run, a current-schema nonmatch, and a request without `--resume`.

Require the report summary shape:

```rust
pub struct DiskRunSummary {
    pub configured_max_owned_bytes: u64,
    pub configured_min_free_bytes: u64,
    pub peak_owned_bytes: u64,
    pub minimum_available_bytes: Option<u64>,
    pub filesystems: Vec<DiskFilesystemReport>,
    pub sample_count: u64,
    pub maximum_measurement_ms: u64,
    pub enforcement: Vec<DiskEnforcementReport>,
    pub stop: Option<DiskStopReport>,
    pub cleanup: Vec<DiskCleanupReport>,
    pub removed_logical_bytes: Option<u64>,
    pub stale_roots_reclaimed: u64,
}

pub enum DiskEnforcementReport {
    PortableGuard,
    CapacityOnly { root_kind: String, filesystem_key: String },
    VerifiedAggregate { backend: String, probe: DiskCapabilityProbe },
}

pub struct DiskCapabilityProbe {
    pub capability: String,
    pub verified: bool,
    pub observation: String,
}

pub struct DiskFilesystemReport {
    pub key: String,
    pub start_available_bytes: Option<u64>,
    pub minimum_available_bytes: Option<u64>,
    pub end_available_bytes: Option<u64>,
    pub available_bytes_change: Option<i128>,
}

pub enum DiskCleanupStatus {
    Clean,
    Failed,
    Deferred,
    Retained,
    CleanupAfterDelivery,
}

pub struct DiskCleanupReport {
    pub root_id: String,
    pub owner: String,
    pub status: DiskCleanupStatus,
    pub examined_entries: u64,
    pub removed_entries: u64,
    pub details: Vec<String>,
    pub omitted_detail_count: u64,
    pub remaining_root: Option<String>,
}
```

Reject `VerifiedAggregate` unless `verified=true`, the backend/capability names match a
registered probe, and observation evidence of at most 4 KiB is present. No free-form
string may
encode `hard_per_file`, `RLIMIT_FSIZE`, or an unprobed aggregate claim. Apply the same
tagged shape to focused schema 2 and both v3 JSON Schemas.

`RunSummary.disk` is present for new schema reports and must distinguish the primary run
and cleanup outcomes. Internally retain a third `ReportDeliveryOutcome`; a delivery
failure is observable through the shell result, stderr, and incomplete session because
the failed output channel cannot reliably describe its own failure. Add serde and
failure-path tests before implementation and require RED. Add an exact machine sequence
RED requiring `Cleanup -> EmitOutput(RunFinished) -> OutputEmitted -> FinishSession`.
Both report failure and post-report session-finalization failure must leave the session
incomplete and return a typed error.

Historical schema-v2 reports remain usable by `hoimin progress`, but only through a
separate `LegacyV2ReportDocument` parser in `progress::input` that extracts the progress
fields and converts directly to `InputReport`. Do not deserialize v2 with current
`OutputEvent`, and do not add serde defaults to `RunLimits`, plan manifests, session
fingerprints, or current `RunSummary`; those persisted execution artifacts remain
strict. Tests require both v2 golden reports to stay usable as progress inputs while all
newly emitted events and reports carry schema 3 and a required disk summary.

Execution-root records use `Clean`, `Failed`, `Deferred`, or explicit `Retained`.
`Deferred` is incomplete and uses `workspace.cleanup.deferred`; it means a lifecycle or
resource budget ended safely while a lease-backed root remained, not that a removal
operation reported failure. The delivery-root
record serialized in `RunFinished` uses `CleanupAfterDelivery`; it never predicts
success. Its actual cleanup result belongs to output acknowledgement/session
finalization and the typed return path.

- [ ] **Step 2: Add config fields and bump persisted schemas**

Add `max_workspace_size` and `min_free_space` to `RawRunLimits` and normalized
`RunLimits` as `NonZeroU64`. Extend `limit_flag`, conversion, validation, every explicit
fixture, and `resume::encode_limits`. Bump:

```rust
pub const FINGERPRINT_SCHEMA_VERSION: u8 = 5;
pub const PLAN_SCHEMA_VERSION: u32 = 3;
pub const REPORT_SCHEMA_VERSION: u32 = 3;
```

Update both `schema-v3-original` and `schema-v3-current` report and event fixtures
mechanically only after the semantic tests pass. Keep every schema-v2 fixture for
the isolated progress compatibility tests; do not silently retarget those tests or parse
their events as current `OutputEvent`. Point current typed regeneration tests at the v3
fixtures, and validate both v3 report fixtures plus both v3 event fixtures. Update
`docs/json-schema/run-event.schema.json` and `run-result.schema.json` to the required v3
disk shape. The published current schemas need not validate the retained v2 compatibility
documents. Old incomplete
session fingerprints remain incompatible; no migration inserts limits. Extend
`SessionHandler::load` only for explicit resume: after the exact-digest query misses,
join the newest incomplete `runs` row to `fingerprints.schema_version`; reject an older
schema with the typed code above. Do not select an old run merely because it is newest.

- [ ] **Step 3: Add orthogonal disk report state**

Add `DiskRunSummary`, `DiskStopReport`, and `DiskCleanupReport` to `report.rs` and
`disk: DiskRunSummary` to `RunSummary`. Keep candidate status accounting unchanged.
Store the authoritative disk stop and cleanup state in `RunState`; machine cleanup
completion updates cleanup outcome, while report delivery failure stays in the existing
report flags. Initialize runtime-only meter fields empty. Ensure cleanup failure cannot
set `complete=true`, and never serialize report delivery as successful before the output
effect completes. Reorder session completion after acknowledged `RunFinished` output;
`RunSummary.complete` covers primary and execution-cleanup outcomes only. A failed
report, delivery cleanup, or subsequent `FinishSession` effect keeps the persisted run
incomplete.

Run:

```bash
cargo test -p hoimin-core --test plan_config -- --nocapture
cargo test -p hoimin-core --test resume_policy -- --nocapture
cargo test -p hoimin-core --test report_policy -- --nocapture
cargo test -p hoimin-core --test machine -- --nocapture
cargo test -p hoimin-core --test lean_report_sequence_oracle -- --nocapture
cargo test -p hoimin-cli --test progress -- --nocapture
cargo test -p hoimin-cli --test report_handler -- --nocapture
cargo test -p hoimin-cli --test report_heap -- --nocapture
cargo test -p hoimin-cli --test session_handler -- --nocapture
cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 4: Commit**

Verify no disk monitor, process-limit, or workspace-deletion integration entered this
commit; the only lifecycle change is the tested report/session ordering above. Then:

```bash
git add crates/hoimin-core/Cargo.toml \
  crates/hoimin-core/src/config.rs crates/hoimin-core/src/resume.rs \
  crates/hoimin-core/src/report.rs crates/hoimin-core/src/machine.rs \
  crates/hoimin-core/tests/lean_oracle.rs \
  crates/hoimin-core/tests/lean_report_sequence_oracle.rs \
  crates/hoimin-core/tests/plan_config.rs \
  crates/hoimin-core/tests/resume_policy.rs \
  crates/hoimin-core/tests/report_policy.rs crates/hoimin-core/tests/machine.rs \
  crates/hoimin-cli/src/plan.rs crates/hoimin-cli/src/report/human.rs \
  crates/hoimin-cli/src/session/mod.rs crates/hoimin-cli/src/progress/input.rs \
  crates/hoimin-cli/tests/progress.rs \
  crates/hoimin-cli/tests/report_handler.rs \
  crates/hoimin-cli/tests/report_heap.rs crates/hoimin-cli/tests/session_handler.rs \
  crates/hoimin-cli/tests/golden/reports/schema-v3-original.json \
  crates/hoimin-cli/tests/golden/reports/schema-v3-current.json \
  crates/hoimin-cli/tests/golden/events/schema-v3-original.jsonl \
  crates/hoimin-cli/tests/golden/events/schema-v3-current.jsonl \
  docs/json-schema/run-event.schema.json docs/json-schema/run-result.schema.json
git commit -m "feat: persist disk safety limits and evidence"
```

### Task 4: Expose safe public CLI defaults

**Files:**

- Modify: `crates/hoimin-cli/src/cli.rs`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `crates/hoimin-cli/tests/plan.rs`

**Interfaces:**

- Consumes: Task 3 normalized disk-limit fields and the existing byte parser.
- Produces: `run` and `plan` flags `--max-workspace-size` and `--min-free-space` with
  mandatory 8 GiB/10 GiB defaults. Task 6 receives the values only through `RunConfig`.

- [ ] **Step 1: Add CLI RED tests**

Parse `run` and `plan` without disk flags and assert exact 8 GiB/10 GiB normalized
limits. Parse explicit byte values. Reject zero, malformed, and overflow values with the
exact flag name. Require `--help` to state that workspace size includes generated files
and that the free-space threshold is mandatory.

- [ ] **Step 2: Implement CLI plumbing**

Add to `RawMutationArgs`:

```rust
/// Run-wide logical size of Hoimin-owned workspaces, including generated files.
#[arg(long, default_value = "8GiB", value_name = "BYTES")]
max_workspace_size: String,

/// Minimum available bytes preserved on every owned-workspace filesystem.
#[arg(long, default_value = "10GiB", value_name = "BYTES")]
min_free_space: String,
```

Parse with the existing byte parser and populate `RawRunLimits`. Update safety help for
`run`, `plan`, and verification. Do not add a disable flag.

- [ ] **Step 3: Verify and commit**

```bash
set -e
cargo test -p hoimin-cli --test cli_config -- --nocapture
cargo test -p hoimin-cli --test plan -- --nocapture
cargo test -p hoimin-cli cli::tests:: -- --nocapture
cargo fmt --all -- --check
git diff --check
git add crates/hoimin-cli/src/cli.rs crates/hoimin-cli/tests/cli_config.rs \
  crates/hoimin-cli/tests/plan.rs
git commit -m "feat: expose disk safety limits in the CLI"
```

### Task 5: Build the leased owned-root, janitor, and portable meter

**Files:**

- Modify: `crates/hoimin-cli/Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/hoimin-cli/src/workspace/owned.rs`
- Create: `crates/hoimin-cli/src/workspace/owned/windows.rs`
- Create: `crates/hoimin-cli/src/workspace/disk.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/copy.rs`
- Modify: `crates/hoimin-cli/src/workspace/root.rs`
- Create: `crates/hoimin-cli/src/workspace/root/windows.rs`
- Modify: `crates/hoimin-cli/src/analyzer/mod.rs`
- Modify: `crates/hoimin-cli/src/process/output.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/src/report/mod.rs`
- Create: `crates/hoimin-cli/tests/disk_workspace.rs`
- Modify: `crates/hoimin-cli/tests/workspace_handler.rs`
- Modify: `crates/hoimin-cli/tests/workspace_recovery.rs`

**Interfaces:**

- Consumes: Task 2 `DiskPolicy`, Task 3 limits, `cap_std::fs::Dir`,
  `cap_fs_ext::DirExt::open_dir_nofollow`, and `fs2` space/lock primitives.
- Produces: `ManagedRunRoot`, `ManagedRootCoordinator`, `DiskMeter`, `MeterReading`,
  `CleanupRecord`, and `ReclaimReport`. Shell setup creates separate execution and
  delivery roots; Task 6 owns their ordered lifecycle. No caller receives an
  arbitrary-path deletion method.

- [ ] **Step 1: Write owned-root and meter RED tests**

Use tiny temporary trees and injected `AvailableSpace` values. Cover:

- two files aggregate across snapshot and worker children;
- symlinks are not followed;
- Unix `(dev, ino)` hard links count once; platforms without identity record
  `conservative_entries` and may double count;
- vanished entries are ignored, permission/overflow errors fail closed;
- a directory swapped to a symlink between enumeration and open is never followed;
- depth 129, entry 250,001, a five-second injected scan deadline, marker 64 KiB plus
  one byte, and direct child 100,001 each fail closed without unbounded allocation;
- entry/direct-child cap tests use injected streaming iterators and counters; they never
  create 250,001 or 100,001 filesystem objects;
- a wide tree never holds more than 129 directory handles and never collects a full
  directory listing; the hard-link identity set never exceeds 250,000 entries;
- Linux janitor enumeration reopens a capability `O_PATH` directory as a readable,
  independently owned `openat2` handle without crossing links or mounts;
- selection checks its absolute five-second deadline after opening the iterator and
  after every `next()`, including EOF, and never advances the coordinator cursor after
  the deadline;
- free bytes equal to the reserve stop at preflight;
- manifest logical bytes at the maximum fail before snapshot creation;
- active locked lease is skipped;
- owner cleanup and janitor contend on the coordinator and exactly one claims rename;
- a valid unlocked lease whose last heartbeat is at least 24 hours old is reclaimed;
- an unlocked lease with a heartbeat younger than 24 hours is preserved; a missing,
  malformed, or future heartbeat is also preserved;
- a valid cleanup-ready marker written after injected process reap, both drain joins,
  and monitor join permits immediate
  reclaim, while a forged/early marker is rejected;
- retained, malformed, wrong-owner, symlink, nested, and foreign entries are preserved;
- malformed/path-like run IDs and a managed leaf not owned by the current user fail;
- staging entries are reclaimed only after 24 hours and only when empty except for a
  valid marker;
- removal callback success without path absence is `workspace.cleanup.failed`.
- one cleanup slice stops after 50,000 examined entries or five cooperative seconds,
  persists safe progress, and resumes from the remaining tree;
- cleanup removes a real depth-129 fixture even though measurement rejects it, while an
  injected depth 4,097 or cursor-name total above 64 KiB fails closed using at most three
  cleanup directory handles;
- owner cleanup stops after 60 seconds with `workspace.cleanup.deferred`; startup
  reclamation stops after 30 seconds, considers at most 256 candidates fairly, and
  retains at most 256 diagnostic details;
- startup janitor preserved-root and bounded error details are merged into the public
  execution cleanup evidence instead of retaining only the reclaimed-root count;
- a persisted lexicographic janitor cursor advances past a selected deferred root and
  wraps, so more than 256 eligible roots are reached across invocations without an
  unbounded listing;
- fixed coordinator golden bytes match the 1,025-byte layout; truncated writes, bad CRC,
  nonzero padding, invalid cursor grammar, equal-generation disagreement, and generation
  overflow fail closed while the prior valid slot remains readable;
- a slice that makes no progress because of permission, identity, or traversal failure
  is `workspace.cleanup.failed`, not deferred;
- dropping a live root or child performs no recursive removal; the next janitor can
  reclaim it only with cleanup-ready evidence or a heartbeat at least 24 hours old;
- constructor failure after execution-root or delivery-root publication rolls back only
  roots that have no process, drain, or monitor owner.

Require these tests to fail because `workspace::owned` and `workspace::disk` do not yet
exist:

```bash
cargo test -p hoimin-cli --test disk_workspace --no-run
```

- [ ] **Step 2: Add the cross-platform primitive dependency**

Add direct `fs2 = "0.4.3"`, `crc32fast = "1.4"`, and `cap-std = "4.0.2"` to `hoimin-cli`; retain
`cap-fs-ext` at lockfile version 4.0.2. Use:

```rust
use fs2::FileExt;
fs2::FileExt::try_lock_exclusive(&lease)
cap_fs_ext::DirExt::open_dir_nofollow(&parent, child)
```

Use `crc32fast` for the coordinator slot's IEEE CRC-32; Python uses the compatible
standard-library `zlib.crc32`. The lock is advisory and valid only because both creator
and janitor obey the protocol.
Do not use PID liveness as a lock substitute. Keep `libc` and `windows-sys` for stable
filesystem/file identities and handle-anchored capacity queries; do not add a second lock
crate or use path-based `fs2::available_space` for meter observations.

- [ ] **Step 3: Implement managed-root creation and the lease API**

Use these fixed names and marker schemas:

```rust
const MANAGED_DIR: &str = "hoimin-workspaces-v1";
const STAGING_PREFIX: &str = ".staging-";
const ACTIVE_PREFIX: &str = "run-";
const DELETING_PREFIX: &str = ".deleting-";
const LEASE_FILE: &str = ".hoimin-lease.json";
const RETAIN_FILE: &str = ".hoimin-retain.json";
const HEARTBEAT_FILE: &str = ".hoimin-heartbeat.json";
const CLEANUP_READY_FILE: &str = ".hoimin-cleanup-ready.json";
const COORDINATOR_FILE: &str = ".hoimin-coordinator";
const LEASE_SCHEMA: u32 = 1;
const MAX_MARKER_BYTES: u64 = 64 * 1024;
const MAX_MANAGED_CHILDREN: usize = 100_000;
const MAX_TREE_ENTRIES: usize = 250_000;
const MAX_TREE_DEPTH: usize = 128;
const MAX_OPEN_DIRECTORIES: usize = MAX_TREE_DEPTH + 1;
const MAX_SCAN_DURATION: Duration = Duration::from_secs(5);
const MAX_CLEANUP_SLICE_ENTRIES: usize = 50_000;
const MAX_CLEANUP_SLICE_DURATION: Duration = Duration::from_secs(5);
const MAX_CLEANUP_DEPTH: usize = 4_096;
const MAX_CLEANUP_CURSOR_BYTES: usize = 64 * 1024;
const MAX_CLEANUP_OPEN_DIRECTORIES: usize = 3;
const OWNER_CLEANUP_BUDGET: Duration = Duration::from_secs(60);
const JANITOR_CLEANUP_BUDGET: Duration = Duration::from_secs(30);
const JANITOR_SELECTION_BUDGET: Duration = Duration::from_secs(5);
const MAX_RECLAIM_CANDIDATES: usize = 256;
const MAX_DIAGNOSTIC_DETAILS: usize = 256;
const MAX_DIAGNOSTIC_DETAIL_BYTES: usize = 4 * 1024;
const COORDINATOR_BYTES: u64 = 1_025;
const COORDINATOR_SLOT_BYTES: usize = 512;
const COORDINATOR_CURSOR_BYTES: usize = 480;

pub(crate) struct CleanupRecord {
    pub(crate) status: DiskCleanupStatus,
    pub(crate) examined_entries: u64,
    pub(crate) removed_entries: u64,
    pub(crate) details: Vec<String>,
    pub(crate) omitted_detail_count: u64,
    pub(crate) remaining_root: Option<Utf8PathBuf>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OwnerKind {
    PublicExecution,
    PublicDelivery,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseMarker {
    schema: u32,
    run_id: String,
    created_unix_seconds: u64,
    owner: OwnerKind,
}
```

`ManagedRootCoordinator::open(parent)` must canonicalize/create the managed parent,
reject a symlinked managed leaf, verify current-user ownership, and set user-only
permissions. `ManagedRunRoot::create(&coordinator, owner)` generates a canonical UUID
internally, then creates a staging
directory and marker with `create_new`, lock
the marker, acquire the managed-root coordinator, rename to `run-{run_id}`, and retain
the open locked file. Unix keeps the lease and heartbeat open across publication. Windows
must not attempt that sequence: an open descendant prevents directory rename even when
the handle grants delete sharing. Immediately after staging creation and before any
marker open, acquire a no-follow `DELETE`-capable handle and duplicate it for ordinary
inspection. The duplicate carries the same OS access rights but is never used as rename
authority. Retain the original handle in `ManagedRunRoot`. While holding the
coordinator, record the exact lease and heartbeat identities and contents, close both
handles, rename through the retained source handle relative to the live coordinator
directory handle, reopen both markers relative to the live run-directory inspection
handle, verify current-user ownership plus exact identity and contents, and relock the
lease before releasing the coordinator. Never reopen owner rename authority during
publication or cleanup. If marker reopen fails, first rename the anchored root back to
staging through the retained handle; if that rollback cannot complete, write
cleanup-ready evidence bound to the original lease identity so the root is immediately
reclaimable after unwinding.
Expose only:

Managed-root bootstrap must tolerate two creators without replacing an existing object:
atomically create the directory if absent, reopen it as a capability, verify identity
and permissions, then create or open the regular coordinator file through that
capability. Preallocate and flush a fixed coordinator layout containing the lock byte
plus two CRC-protected `(generation, cursor)` slots before any byte-range lock adapter
uses the file. Reject a symlink, reparse point, non-regular coordinator, wrong fixed
length, ownership mismatch, or
permission mismatch. Test two fresh processes racing the first bootstrap.
The byte layout is normative: byte 0 is the lock; slots begin at 1 and 513. A slot is
`HMCUR001` (8 bytes), schema `u32` LE, generation `u64` LE, cursor length `u16` LE,
480 zero-padded ASCII cursor bytes, six zero reserved bytes, and CRC-32 LE over its first
508 bytes. Validate all padding and managed-name grammar. Choose the highest valid
generation; equal generations with unequal contents fail closed, equal contents choose
slot zero. Write the next checked generation to the inactive slot using positioned I/O
and flush it. Treat generation overflow as cursor-persistence failure while still
allowing already selected candidates to be cleaned.

```rust
pub(crate) fn path(&self) -> &Utf8Path;
pub(crate) fn create_child(&self, prefix: &str) -> Result<ManagedChild, WorkspaceError>;
pub(crate) fn retain(&self) -> Result<(), WorkspaceError>;
pub(crate) fn mark_cleanup_ready(&self) -> Result<(), WorkspaceError>;
pub(crate) fn cleanup(&self, budget: Duration) -> CleanupRecord;
pub(crate) fn abandon_for_janitor(&self, reason: String) -> CleanupRecord;
pub(crate) fn reclaim_abandoned(
    coordinator: &ManagedRootCoordinator,
    now: SystemTime,
) -> ReclaimReport;
```

`ManagedChild` is a path capability, not `tempfile::TempDir`; its `Drop` closes handles
without deleting content. `ManagedRunRoot::Drop` also releases handles and performs no
recursive deletion. Catchable control flow must call `cleanup` explicitly. Panic or
task-cancellation tests must leave a locked or abandoned root for the next janitor.
Before `ManagedChild::Drop` decrements `live_children`, it explicitly closes its
directory and parent handles, then releases its shared lease. Any containing type with
another descendant handle must close that handle first: `WorkerWorkspace::Drop`, for
example, closes `WorkerRoot` before dropping its managed-child token. A deterministic
Windows gate test must pause immediately after quiescence publication and prove cleanup
can rename at that point.

Cleanup closes child handles first, acquires the coordinator, and revalidates its active
child and marker through anchored handles. Unix renames only its own direct child to
`.deleting-{run_id}` while the lease remains locked. Windows captures exact lease and
heartbeat evidence, closes the heartbeat and final lease handles, renames the directory
through the handle retained since staging creation, reopens the lease from the live
candidate-directory inspection handle, verifies owner, identity, and complete marker
contents, and relocks it before releasing the coordinator. On a failed rename it must
reopen and relock the original lease and restore the exact heartbeat before returning.
It must not attempt to acquire a replacement owner `DELETE` handle at this point. It
then performs anchored removal, retains the per-run lease through absence
verification, and closes it only afterward. `abandon_for_janitor` records the deferred reason and releases
only the caller's lease guard without invoking recursive cleanup; a monitor or other
live component's cloned guard remains locked. `cleanup` repeats resumable anchored
slices until absence, a hard error/no-progress result, or its total budget. Each slice
performs a streaming post-order walk, removes files and empty directories as it reaches
them, and checks its entry/time limits before and after each filesystem operation. It
stores a bounded component/identity cursor, reopens that cursor one component at a time
from the anchored root with no-follow identity checks, and holds only root/current-parent/
current-child handles. Do not reuse the meter's depth-128 stack: cleanup accepts depth up
to 4,096 and 64 KiB of cursor-name bytes so it can remove the depth violation that
caused a meter stop.
Reaching a slice or total budget after progress returns `Deferred`, preserves the
`.deleting-` root, and never reports `removed_logical_bytes`. An individual blocking
filesystem syscall is not portably preemptible and is an explicit design limitation.

Create and flush `HEARTBEAT_FILE` before publishing the active root. Keep its identity
open and refresh only its modification time with `futimens` or `SetFileTime` at least
once per minute; do not replace the path. A heartbeat update failure becomes
`disk.measurement.failed` and closes dispatch. `mark_cleanup_ready` creates and flushes
`CLEANUP_READY_FILE` with schema, run ID, and
lease identity while the lease remains locked; Task 6 may call it only after
`ProcessDrainReport` proves process reap and output drain and the disk monitor reports a
successful join. The monitor owns a cloned/shared lease guard until its thread exits; a
join timeout cannot unlock the lease from another owner. A janitor may claim an
unlocked active or deleting root only when this marker
validates or `now >= last_heartbeat + 24h`; a future clock value, underflow, missing
heartbeat, or malformed heartbeat preserves the root. Staging entries that never became
active use their creation timestamp. Neither marker can authorize a different run ID or
owner kind.
These markers provide crash-recovery evidence, not cryptographic authenticity against a
deliberately malicious process running with the same user credentials. “Forged marker”
tests cover wrong schema, identity, owner, ordering, or filename; they must not claim a
portable same-user sandbox boundary.
On Windows, byte-range locks are mandatory for competing handles. Once janitor acquires
the lease, it reads and validates the marker through that locked handle rather than
opening a second reader. The coordinator is held across the bounded close/rename/reopen/
relock interval, so no compliant creator, owner cleanup, or janitor can observe the lease
temporarily unlocked.

Run the creation, UUID/name, active-lease, and owner/janitor coordinator race tests before
continuing. The only accepted failures at this point are deletion and platform-security
cases implemented by the next two steps.

- [ ] **Step 4: Implement and test the platform security boundary**

On Unix, set and verify mode `0700` on the managed directory. On Windows, add the needed
`windows-sys` security authorization features and install a protected DACL granting full
access only to the current user SID, `SYSTEM`, and `Administrators`; read it back before
creating a staging child. Directory ACEs carry object/container inheritance; regular-file
ACEs do not, because Windows strips those inheritance flags from a file and exact DACL
self-verification must compare the descriptor appropriate to the object kind. A failure
to establish this boundary fails closed. Tests use a platform adapter rather than
shelling out to `icacls`.

On Unix, implement anchored removal through directory-relative no-follow operations. On
Windows, use handles opened with `DELETE`, directory-list, and attribute rights; reject
reparse points and use handle-relative rename/disposition operations. Never close the
validated handle and recursively reopen its accumulated path. Run symlink/junction
swap, protected-DACL, read-only-child repair, and absence-verification tests.

Do not call `cap_std::fs::Dir::remove_dir_all` on Windows. The pinned
`cap-primitives 4.0.2` Windows implementation closes its validated directory handle and
calls path-based `std::fs::remove_dir_all`; its source labels that transition racy. Use a
small owned `windows-sys 0.60` adapter around `CreateFileW`,
`GetFinalPathNameByHandleW`, `GetFileInformationByHandleEx`, and
`SetFileInformationByHandle`. Capability-relative rename uses
`NtSetInformationFile(FileRenameInformation)` with a live parent handle because the
Win32 wrapper rejects this relative-root form with `ERROR_INVALID_PARAMETER` on the
supported Windows host. Every constant is covered by a Windows compile test.
Keep Unix and Windows deletion behind one trait so a platform stub cannot fall back to
ambient recursive removal.

Keep measurement and deletion handle policies separate. Windows read-only scan handles
must include `FILE_SHARE_DELETE`; the open handle and identity checks anchor the object
while permitting a candidate directory to be removed. Destructive janitor claim
handles request `DELETE` access and run only after root monitor join. The live owner
acquires its rename handle before any staging descendant exists and retains it for both
namespace transitions. The janitor checks the protected named mutation barrier,
acquires the coordinator, and then acquires its `DELETE`-capable candidate before
opening or locking any descendant marker; that same handle is used for claim. The
current owner
may remove a compacted candidate below the still-monitored run root; the walker treats
its vanished entries as concurrent cleanup. Add a Windows regression in which a scan
observes a candidate while the owner removes it, then assert no traversal escape and no sharing
violation.

- [ ] **Step 5: Implement the bounded stale-root janitor**

The janitor opens the canonical managed root as `cap_std::fs::Dir`, streams at most
`MAX_MANAGED_CHILDREN` direct entries without collecting the listing, considers at most
`MAX_RECLAIM_CANDIDATES` eligible roots, and opens candidates with
`DirExt::open_dir_nofollow`; on Windows this is the one `DELETE`-capable candidate
opened under the coordinator before any marker handle. It never performs a second
late-open for rename authority or check-then-path-open traversal. It caps
marker reads before deserialization, validates schema/owner/name/run ID and retention,
then acquires the nonblocking lease and coordinator. Under both locks it compares the
already-open child and marker identities again before renaming. Valid `.deleting-`
entries left by failed cleanup are eligible for the same locked retry; unknown entries
are preserved. Never consume a cleanup path from JSON output, environment, or a child
command.

For active and deleting entries, require either a valid cleanup-ready marker or a
checked age of at least 24 hours after the last valid heartbeat. Do not treat lock availability
as proof that descendant processes exited.

Keep the validated claim transition in one uniquely named production helper,
`claim_managed_child`; both owner cleanup and janitor call it with their allowed prefix
and expected identity. The owner wrapper additionally transfers its unique lease and
heartbeat handles into and out of the Windows handoff; a live `ManagedChild` retains a
shared lease guard and prevents that transfer.

After claiming or recognizing a `.deleting-` entry, release the coordinator but keep
the candidate lease locked through removal and absence verification. On Windows this is
the reopened, identity-equal lease acquired before coordinator release. If a staging entry
has a marker, validate and lock it before claim; never treat age alone as liveness.

Run active, abandoned, retained, malformed, oversized-marker, excessive-child, and
24-hour staging tests with two coordinators contending. Require exactly one reclaim
claim and no deletion of any rejected fixture. Give each candidate one cleanup slice
before a second pass, stop the whole janitor after `JANITOR_CLEANUP_BUDGET`, and retain
only `MAX_DIAGNOSTIC_DETAILS` exact detail records of at most
`MAX_DIAGNOSTIC_DETAIL_BYTES` each plus aggregate omitted/error/truncated counts.
While holding the coordinator, stream the direct-child set within
`JANITOR_SELECTION_BUDGET` and retain only the lexicographically next
`MAX_RECLAIM_CANDIDATES` names after the highest-generation valid coordinator cursor.
Write the advanced cursor into the inactive fixed slot and flush after selection even if
a root later defers; wrap after the last name. A slot write/fsync failure is reported but
does not prevent cleanup of already selected candidates, and the previous CRC-valid slot
remains authoritative.
Test that a huge first root cannot starve a later tiny root and that 257 roots require two
invocations with the final root selected on the second. A selection deadline/cap failure
does not update the cursor or delete anything.

- [ ] **Step 6: Implement the meter**

Expose injectable traits and production implementations:

```rust
pub(crate) trait AvailableSpace: Send + Sync {
    fn available(&self, root: &RootCapability) -> io::Result<u64>;
    fn filesystem_key(&self, root: &RootCapability) -> io::Result<FilesystemKey>;
}

pub(crate) struct RootCapability {
    pub(crate) dir: cap_std::fs::Dir,
    pub(crate) display_path: Utf8PathBuf,
}

pub(crate) struct DiskMeter<S> { /* roots, S */ }

pub(crate) struct MeterReading {
    pub owned_bytes: u64,
    pub available_by_filesystem: BTreeMap<FilesystemKey, u64>,
    pub conservative_entries: bool,
    pub elapsed: Duration,
}
```

Walk with a streaming depth-first stack of `cap_std::fs::Dir` handles. Enumerate one
entry at a time, obtain no-follow metadata, and open the child with
`open_dir_nofollow` before descent. Finish and close a child before moving to its next
sibling; never enqueue every directory in a wide tree and never reopen a descendant by
accumulated path. Use checked addition and fail at depth 128, 250,000 entries, 129 open
directory handles, or five seconds. Check the injected monotonic deadline at least every
256 entries. On Unix identify files with `MetadataExt::{dev, ino}`, group filesystems by
`dev`, and call `fstatvfs` on the open root descriptor. On Windows group by volume serial
from the open handle, derive a normalized volume root from its verified
`GetFinalPathNameByHandleW` result, and call `GetDiskFreeSpaceExW` only after the identity
check. Use conservative entry counting on Windows. Query each filesystem key once per
sample; never reopen the original root path for a capacity reading.

Name the production walker `measure_owned_tree` and the platform deletion entry point
`remove_claimed_tree`. Both consume already-open capabilities or handles; neither
accepts an arbitrary absolute descendant path. Ordinary tests cover the portable
walker; native tests and security review cover platform deletion adapters.

Run aggregation, hard-link, symlink-swap, vanished-entry, depth/entry-cap, overflow, and
injected free-space boundary tests before runtime wiring.

- [ ] **Step 7: Move all public runtime scratch under two leased roots**

Add an `Arc<ManagedRunRoot>` with interior lifecycle state to
`WorkspaceHandler`/`WorkspacePlan`. Replace ambient
`tempdir()` calls in production snapshot and worker materialization with
`managed.create_child("snapshot-")` and `managed.create_child("worker-")`. Before snapshot
creation, compute `manifest.logical_bytes() * (1 + requested_workers)` with checked
arithmetic and reject `>= max_workspace_size`; use an injected seam to prove
`create_disk_snapshot` was not called. Do not move test-only
tempdirs or repository output. `handle_cleanup` and `close` must attempt every worker and
pending-worker cleanup even after one fails, drop the plan and snapshot, then explicitly
clean the run root when component-drain safety permits it. Aggregate ordered secondary
errors and return `Failed` if absence verification fails for a hard reason or `Deferred`
if the 60-second owner budget expires after safe progress; do not short-circuit and
strand later roots silently.

In `prepare_shell_setup_sync`, run the janitor, then create an execution root and a
delivery root with distinct owner kinds. Put snapshot, workers, process output, analyzer
candidate spool, and other pre-report scratch below the execution root. Construct
`PreparedReport` with a child of the delivery root. Register both with the meter. The
workspace cleanup effect removes only the execution root; `ReportHandler` retains the
delivery-root handle for Task 6 finalization. Add a test proving execution cleanup does
not remove the still-required JSON spool.

Register an explicit setup rollback guard immediately after the first lease is
published. Constructor failures from workspace, resource backend, process, analyzer,
report, or monitor setup must unwind through this guard. It may remove roots only while
its state proves that no process, drain thread, or monitor started; otherwise it calls
`abandon_for_janitor`. Add one injected failure test at each constructor boundary and
assert exact root absence or intentional abandonment.

- [ ] **Step 8: Verify Task 5 behavior**

Run:

```bash
cargo test -p hoimin-cli --test disk_workspace -- --nocapture
cargo test -p hoimin-cli --test workspace_recovery -- --nocapture
cargo test -p hoimin-cli --test workspace_handler -- --nocapture
cargo test -p hoimin-cli workspace:: -- --nocapture
cargo fmt --all -- --check
cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
rustup target add x86_64-pc-windows-msvc
cargo check -p hoimin-cli --all-targets --all-features --target x86_64-pc-windows-msvc
git diff --check
```

- [ ] **Step 9: Commit**

```bash
git add Cargo.lock crates/hoimin-cli/Cargo.toml \
  crates/hoimin-cli/src/workspace/owned.rs \
  crates/hoimin-cli/src/workspace/owned/windows.rs \
  crates/hoimin-cli/src/workspace/disk.rs \
  crates/hoimin-cli/src/workspace/mod.rs \
  crates/hoimin-cli/src/workspace/copy.rs \
  crates/hoimin-cli/src/workspace/root.rs \
  crates/hoimin-cli/src/workspace/root/windows.rs \
  crates/hoimin-cli/src/analyzer/mod.rs \
  crates/hoimin-cli/src/process/output.rs \
  crates/hoimin-cli/src/shell.rs \
  crates/hoimin-cli/src/report/mod.rs \
  crates/hoimin-cli/tests/disk_workspace.rs \
  crates/hoimin-cli/tests/workspace_handler.rs \
  crates/hoimin-cli/tests/workspace_recovery.rs
git commit -m "feat: lease and measure mutation workspaces"
```

### Task 6: Integrate disk stops with process shutdown and evidence

**Files:**

- Modify: `crates/hoimin-core/src/event.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/src/model.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-cli/src/workspace/disk.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/src/process/mod.rs`
- Modify: `crates/hoimin-cli/src/resource/mod.rs`
- Modify: `crates/hoimin-cli/src/resource/portable.rs`
- Modify: `crates/hoimin-cli/src/report/mod.rs`
- Modify: `crates/hoimin-cli/src/report/json.rs`
- Modify: `crates/hoimin-cli/tests/process_handler.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Modify: `docs/superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md`
- Create: `crates/hoimin-cli/tests/disk_shutdown.rs`
- Create: `crates/hoimin-cli/tests/lean_disk_shutdown_oracle.rs`

**Interfaces:**

- Consumes: Tasks 1–5 corpus, policy, persisted limits, CLI config, leased root, and
  meter.
- Produces: `DiskMonitor::{start,sample_now,stop_and_join}`, global disk-stop machine
  transitions, post-drain/pre-classification sampling, and
  `ReportHandler` acknowledgement only after delivery-root cleanup, plus same-premise
  execution of every `runtime` record targeting `rust` through the controlled shell
  adapter. Task 10 verifies this runtime without executing Rust mutation testing.

- [ ] **Step 1: Add lifecycle RED tests**

Use a fake meter and a process fixture that creates one child plus one descendant. Assert
the exact observed order:

```text
disk_stop
dispatch_gate_closed
root_termination_requested
root_reaped
output_drained
monitor_stopped
monitor_joined
workspace_cleanup_requested_once
workspace_absent
run_finished_flushed
delivery_cleanup_requested_once
delivery_workspace_absent
output_acknowledged
session_finished
```

Use a session-backed fixture for the full sequence; the no-session fixture ends after
`output_acknowledged`.

Test size, reserve, and meter-failure stops, plus a normal run. Race a disk stop with a
process failure and assert first-reason stickiness and secondary error retention. Assert
an interrupted mutant becomes `NotRun`, not `Killed`. Inject cleanup absence failure and
assert the primary disk code survives while `complete=false` and cleanup is failed.
Add deterministic barriers for two boundary races: a threshold becomes true immediately
before dispatch, and process completion becomes ready with the post-drain sample. Both
must select the disk stop and must not launch or credit the candidate. Block monitor
join past the shutdown budget and assert the root is deferred, remains present, and no
recursive remover or fallback `Drop` runs while the scan holds its directory handle.

Run the focused RED:

```bash
cargo test -p hoimin-cli --test disk_shutdown -- --nocapture
```

- [ ] **Step 2: Add a global disk-stop event**

Add this event rather than fabricating an `EffectId`:

```rust
pub struct DiskStopRequested {
    pub failure: DiskFailure,
}

pub enum RunEvent {
    // existing variants
    DiskStopRequested(DiskStopRequested),
}
```

Keep the existing `ProcessHandler::close() -> Result<(), ResourceError>` API for its
current resource-backend callers. Add a shell-shutdown method with the richer result:

```rust
pub struct ProcessDrainReport {
    pub all_reaped: bool,
    pub output_drains_joined: bool,
    pub secondary_errors: Vec<String>,
}

impl ProcessHandler {
    pub async fn drain_for_shutdown(&self, budget: Duration) -> ProcessDrainReport;
}
```

Only `all_reaped && output_drains_joined` permits execution-root removal. A false field
marks cleanup deferred and leaves the lease for the janitor; secondary errors after both
true do not prevent an attempted workspace cleanup. That conjunction is necessary but
not sufficient for marker publication: a successful monitor join is also required before
`managed_root.mark_cleanup_ready()`; an error writing that marker is secondary for the
current explicit cleanup but prevents any later janitor from using the immediate-reclaim
path.

On portable Unix, reaping either a naturally exited root or a root terminated while still
owned does not itself prove that the original process group is empty. Move the PGID out
of the signalable live-root slot before any fallible post-reap operation, retain it in a
non-signalable verification slot, and probe with signal zero until absence is proven or
the bounded quiescence grace expires. If a member remains, preserve the command result
but leave the sticky process-reap proof false so execution and delivery cleanup are
deferred. If the group is absent, normal childless or successfully terminated completion
is quiescent. Do not turn the probe into a post-reap `killpg` call. Cover natural exit,
live-root termination, absent-group, live-group, and probe-error branches.

Handle it through the same global-stop transition family as cancellation/deadline, but
preserve its typed code and observation in disk evidence. Synthetic unfinished mutant
events use `MutationStatus::NotRun`. A later `EffectFailed` becomes secondary and cannot
replace the disk primary reason.

- [ ] **Step 3: Implement synchronous and periodic monitor sampling**

After the machine accepts `StartRequested` but before the shell pops its first effect,
`DiskMonitor::start` performs one synchronous preflight sample. A threshold failure is
therefore an ordered run event while no analyzer, baseline, or mutant process
has launched. A failed or threshold sample is queued as `DiskStopRequested` immediately.
Otherwise one dedicated standard thread samples one scan at a time and waits 250ms
after the scan completes; filesystem traversal never blocks a Tokio runtime worker.
Each walk enforces the five-second cooperative deadline from Task 5. A slow scan cannot
turn the monitor into a continuous filesystem-I/O loop.
The monitor refreshes each live root heartbeat at startup and, before beginning the next
scan, whenever 60 seconds have elapsed since the prior successful refresh. Except for an
individual filesystem call that does not return, no live interval exceeds one minute. A
failed refresh publishes `disk.measurement.failed`; tests use an
injected wall clock and writer to cover the exact 60-second boundary.

Immediately before every analyzer, baseline, or mutant process dispatch, call
`sample_now` and feed its decision through the priority event path. Immediately after a
process exits and both output streams drain, call `sample_now` again before decoding or
submitting its result. If both process completion and a disk stop are ready, the disk
stop wins and the candidate remains `NotRun`.

Run meter-thread tests proving one in-flight scan, scan-start spacing, initial and
boundary samples, first-stop stickiness, and measurement-failure delivery.

- [ ] **Step 4: Wire disk-stop priority into the existing shell loop**

The monitor publishes the first stop through `watch::Sender<Option<DiskFailure>>` and
updates stats behind a mutex. In `run_loop_prepared`, add the receiver to the same
priority check as cancellation/deadline. On receipt:

```rust
cancellation.cancel();
stop_signalled = true;
priority_event = Some(RunEvent::DiskStopRequested(DiskStopRequested {
    failure,
}));
```

Run the pre-dispatch and post-drain race fixtures. Require exact event ordering and
assert the normal cancellation/deadline path still uses the same shutdown loop.

- [ ] **Step 5: Wire monitor join, deferred cleanup, and final evidence**

Do not create a second shutdown loop. Before dispatching the existing `Cleanup` effect,
call the idempotent `disk_monitor.stop_and_join(shutdown_budget)`. If join completes,
continue to workspace cleanup. If it exceeds the shutdown budget, record secondary
`disk.measurement.failed`, call `managed_root.abandon_for_janitor`, mark cleanup
`Deferred`, emit no cleanup-ready marker, and leave the monitor thread holding its
shared root and lease guard until it exits rather
than racing recursive removal against a live scan. After process drain, take one final
pre-clean reading; after successful monitor join and cleanup, query each affected
filesystem parent once for end-free evidence. Record the final pre-clean logical size as
`removed_logical_bytes` only after absence verification and record a signed
`available_bytes_change`; do not label free-space delta as physically reclaimed bytes.
Before dispatching the
final `EmitOutput(RunFinished(_))`, merge these observations into its disk field without
replacing machine-owned stop/cleanup state. Final evidence remains on the existing
ordered report path. Run blocked-join, cleanup-failure, end-free-query-failure,
concurrent negative free-space change, and successful removed-logical-byte fixtures
before the final Task 6 gate.

Do not short-circuit cleanup on a process resource-close error. Use
`ProcessDrainReport`: if process reap, output drains, and monitor join are confirmed,
attempt every execution cleanup and retain close errors as secondary; otherwise call
`abandon_for_janitor` and make no recursive removal attempt. Apply the same rule to the
shutdown-budget and detached-cleanup paths.

On portable Unix, retry the non-signalling original-PGID absence check for at most 250
milliseconds. This leaves explicit room inside the existing fixed two-second shutdown
grace for the one-second output-drain bound and shell completion delivery. Expiry is not
quiescence: record process reap as unproven and retain the workspace for the janitor.

For `RunFinished`, make `ReportHandler` write and flush output, drop its JSON spool,
clean and absence-verify the delivery root, and only then return `OutputEmitted`. On
write failure it still attempts exact delivery-root cleanup and returns the report error
with cleanup failure secondary; on cleanup failure it returns typed
`workspace.cleanup.failed`; on a cleanup budget result it returns
`workspace.cleanup.deferred`. In all cases no `OutputEmitted` occurs and a session stays
incomplete. The emitted summary labels this root `cleanup_after_delivery` and does not
predeclare success. Add JSON, JSONL, and human-handler fixtures for success, write
failure, cleanup failure, and neighboring-sentinel preservation.

- [ ] **Step 6: Verify ordering and platform behavior**

```bash
cargo test -p hoimin-cli --test disk_shutdown -- --nocapture
cargo test -p hoimin-cli --test process_handler -- --nocapture
cargo test -p hoimin-cli --test lean_disk_shutdown_oracle -- --nocapture
cargo test -p hoimin-core --test machine -- --nocapture
cargo test -p hoimin-cli shell::tests:: -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
rustup target add x86_64-pc-windows-msvc
cargo check -p hoimin-cli --all-targets --all-features --target x86_64-pc-windows-msvc
git diff --check
```

On Windows, compile and run the portable sampler/lifecycle tests. Add a regression that
simulates a child catching a per-file limit error and exiting like a test failure; the
runtime must have no installed per-file limit path that could turn it into mutation
credit.
The Rust oracle test compares expected and executed Rust case-ID sets for equality,
including every applicable long trace and the Rust-only delivery-root traces, and rejects
malformed target lists. Report-failure and delivery-cleanup-failure traces must perform
their allowed cleanup, reject `finish`, and leave output/session acknowledgement absent.

- [ ] **Step 7: Commit**

```bash
git add crates/hoimin-core/src/event.rs crates/hoimin-core/src/machine.rs \
  crates/hoimin-core/src/model.rs crates/hoimin-core/tests/machine.rs \
  crates/hoimin-cli/src/workspace/disk.rs crates/hoimin-cli/src/shell.rs \
  crates/hoimin-cli/src/process/mod.rs crates/hoimin-cli/src/resource/mod.rs \
  crates/hoimin-cli/src/resource/portable.rs \
  crates/hoimin-cli/src/report/mod.rs crates/hoimin-cli/src/report/json.rs \
  crates/hoimin-cli/tests/process_handler.rs \
  crates/hoimin-cli/tests/report_handler.rs \
  crates/hoimin-cli/tests/run_e2e.rs \
  docs/superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md \
  crates/hoimin-cli/tests/disk_shutdown.rs \
  crates/hoimin-cli/tests/lean_disk_shutdown_oracle.rs
git commit -m "feat: stop and clean runs at disk safety limits"
```

### Task 7: Give the focused mutation workflow owned bounded scratch

**Files:**

- Modify: `tools/__init__.py`
- Create: `tools/focused_mutation_support/disk.py`
- Create: `tools/focused_mutation_support/lease.py`
- Create: `tools/focused_mutation_support/windows_file.py`
- Modify: `tools/focused_mutation_support/__init__.py`
- Modify: `tools/focused_mutation_support/discovery.py`
- Modify: `tools/focused_mutation_support/model.py`
- Modify: `tools/focused_mutation_support/mutation.py`
- Modify: `tools/focused_mutation_support/reporting.py`
- Modify: `tools/focused_mutation_support/runner.py`
- Modify: `tools/focused_mutation_support/store.py`
- Modify: `tools/focused_mutation.py`
- Create: `tests/test_focused_mutation_disk.py`
- Modify: `tests/test_focused_mutation_discovery.py`
- Modify: `tests/test_focused_mutation_runner.py`
- Modify: `tests/test_focused_mutation_reporting.py`
- Modify: `tests/test_focused_mutation_budget.py`

**Interfaces:**

- Consumes: Task 1 corpus vocabulary and the existing focused mutation budget, runner,
  store, discovery, cargo-mutants 27.1.0 inventory, and process-tree termination code.
- Produces: Python `DiskPolicy`, `DiskLifecycle`, `DiskGuard`, `ManagedScratch`, bounded
  binary spools, anchored `OwnedOutput`, schema-2 `RunRecord`, and exact `--max-disk`,
  `--min-free-space`,
  `--jobs`, `--max-log-size`, `--scratch-root`, and `--keep-scratch` options. Task 8
  audits these public policy calls.

- [ ] **Step 1: Add parser, workflow, and bounded-drain RED tests**

Assert exact defaults:

```python
max_disk_bytes = 8 * 1024**3
min_free_bytes = 10 * 1024**3
jobs = 1
max_log_bytes = 16 * 1024**2
max_command_log_bytes = 64 * 1024**2
max_tool_json_bytes = 8 * 1024**2
max_selector_count = 1_000
max_selector_bytes = 16 * 1024
max_candidate_diagnostic_bytes = 16 * 1024
max_run_diagnostic_bytes = 16 * 1024**2
max_report_bytes = 32 * 1024**2
max_reported_path_bytes = 16 * 1024
sample_interval_seconds = 0.250
```

Reject nonpositive bytes/jobs, reject `--jobs 5` or greater, reject
`--max-log-size` above 64 MiB, and reject a scratch root
that is a file or cannot be
canonicalized/query free-space. A symlinked parent such as macOS `/tmp` is accepted only
after canonicalization; leased children must still be real directories. With fake
subprocesses/meters, cover `TMPDIR`/`TMP`/`TEMP` propagation, explicit `--jobs`,
size/reserve/meter stops, termination/reap before delete,
candidate outcome extraction before delete, cleanup on success/failure/timeout/interrupt,
and `--keep-scratch` with monitoring still active.
Reject any path that can enter JSON, Markdown, or stderr when its strict UTF-8 encoding
or its destination-specific escaped UTF-8 representation exceeds 16 KiB or contains
surrogate/non-Unicode data, before creating a marker or child.
This keeps the exact recovery path representable in JSON, Markdown, and the bounded
stderr fallback. Test 16 KiB and 16 KiB plus one with injected path encoders rather than
creating an operating-system maximum-length tree.
With an injected Cargo home on a distinct fake filesystem, require reserve preflight and
periodic stops while asserting that its files are never traversed, charged to owned
bytes, or deleted. Deduplicate the capacity reading when Cargo home shares a filesystem
with scratch/output.

Require the output path to be absent or an empty real directory at startup. A non-empty
directory, symlink, or file must fail before scratch creation and child launch. Assert
the accepted output directory is included in pre-dispatch, periodic, post-drain, and
final pre-clean samples, but cleanup removes only leased scratch. Reject an output path
inside either implementation's canonical managed root.
The sole preflight exception is bounded recovery of a validated abandoned
`.hoimin-output-owner`: acquire its lock nonblocking, match its run ID and anchored output
identity, and inspect only the two deterministic atomic temporary names derived from
that run ID and fixed `run.json`/Markdown kinds. Unlink an existing candidate only when
it is a regular non-symlink entry reached through the anchored directory. Preserve
completed reports and every foreign entry, then reject the still non-empty output. If
only the marker remains, unlink that wrapper-created marker and allow normal empty-root
initialization. Test simulated death before write, during each report write, after JSON
replace, and with a neighboring sentinel or symlink at a derived name.
Run this orphan cleanup before the fresh-run reserve check, then resample capacity. Add a
fixture whose initial free space is at the reserve and rises after exact temporary
removal; it may proceed only after the second reading is above the reserve.

Race two initializers against the same absent and same empty output path. Exactly one
must create and lock `.hoimin-output-owner`; the loser must fail before scratch creation
or child launch. Hold an anchored output directory descriptor or handle through final
report flush. Replace the path with a symlink in a fixture and prove writes stay bound
to the opened directory or fail without touching the replacement target.

On Unix, implement relative create/replace/fsync operations through the held directory
descriptor. On Windows, open the output directory without `FILE_SHARE_DELETE` and retain
that handle until final flush, preventing root rename/deletion while child paths are
used. Reject reparse points and verify final path, volume, and file identity before and
after each atomic replacement. Do not reuse the meter handle policy here: read-only
meter handles must share deletion, while output-owner handles intentionally must not.

Before creating `.hoimin-output-owner`, query the absent output's anchored parent or the
existing empty output handle and require free space above the reserve. Cap the marker at
64 KiB and retain it as provenance after success. Assert a boundary failure creates no
marker, scratch, or child process and that the first checkpoint uses the guarded writer.

Feed inventory stdout of 8 MiB plus one byte, an inventory that exceeds the stdout half
of a user-lowered combined log cap, 10,001 discovered entries, and 1,001
selected candidates. Each case must fail before baseline. Feed a large candidate record
through Markdown and JSON writers and assert encoder writes never exceed 64 KiB and
each encoded report fails before byte 32 MiB plus one. Assert
`render_markdown(record) -> str` is no longer the production API.

For bounded logs, have a fake child write more than the allowance to stdout and stderr
concurrently. Assert it exits without pipe blockage, combined retained bytes never exceed
the limit, observed bytes remain exact, and deterministic head/tail slices are retained.
Run 1,000 synthetic candidate classifications and assert only the active command owns
the full spool, each persisted diagnostic is at most 16 KiB, the shared persisted
diagnostic bodies total at most 16 MiB, and later metadata is retained after the shared
body allowance is exhausted.
Also feed `--prior-inventory` and one-candidate `outcomes.json` files of 8 MiB plus one,
1,001 selectors, and one 16 KiB plus one selector. Reject each before JSON decoding or
baseline. Give a fake compiler log a multi-GiB reported length backed by a seekable sparse
fixture and assert the extractor reads only its bounded prefix/tail windows and records
the observed length; never materialize that size.

Run and require behavioral failures, not import/syntax failures:

```bash
python3 -c 'import shutil, sys; sys.exit(0 if shutil.disk_usage(".").free > 10 * 1024**3 else 1)'
test -x .venv/bin/python
.venv/bin/python -m unittest \
  tests.test_focused_mutation_disk \
  tests.test_focused_mutation_runner \
  tests.test_focused_mutation_reporting
```

- [ ] **Step 2: Implement the bounded Python policy and anchored meter**

Use language-native dataclasses mirroring the Lean observation/result names. Define
the two-value `DiskRootId`, every `DiskLifecycleEvent` listed in Task 2, and public
`evaluate_disk_policy`/`apply_disk_lifecycle_event` entry points. Apply the same rules:
cleanup requests wait for settled process/drain/monitor components, clean or failed
destructive outcomes require all three to have succeeded, failed components permit only
deferred or retained, and delivery cleanup also waits for report settlement. Do not
expose an integer constructor that can create a root absent from Lean `Fin 2`.
Define
`MAX_TREE_ENTRIES = 250_000`, `MAX_TREE_DEPTH = 128`,
`MAX_OPEN_DIRECTORIES = 129`, `MAX_SCAN_SECONDS = 5.0`,
`MAX_CLEANUP_SLICE_ENTRIES = 50_000`, `MAX_CLEANUP_SLICE_SECONDS = 5.0`,
`MAX_CLEANUP_DEPTH = 4_096`, `MAX_CLEANUP_CURSOR_BYTES = 64 * 1024`,
`MAX_CLEANUP_OPEN_DIRECTORIES = 3`,
`OWNER_CLEANUP_SECONDS = 60.0`, `JANITOR_CLEANUP_SECONDS = 30.0`,
`JANITOR_SELECTION_SECONDS = 5.0`,
`MAX_RECLAIM_CANDIDATES = 256`, `MAX_DIAGNOSTIC_DETAILS = 256`, and
`MAX_DIAGNOSTIC_DETAIL_BYTES = 4 * 1024`. On Unix,
open the root with `O_DIRECTORY | O_NOFOLLOW`, enumerate lazily with `os.scandir(fd)`
without converting it to a list, obtain
`os.stat(entry.name, dir_fd=fd, follow_symlinks=False)`, and open descendants with
`os.open(name, flags, dir_fd=fd)`; never reopen an accumulated descendant path. On
Unix, use the `scandir` context manager or explicitly close every iterator before its
directory descriptor; add an injected early-return test that observes both closures. On
Windows, use a `ctypes` adapter that opens read-only directory handles with
`FILE_SHARE_DELETE`, rejects reparse points, enumerates through the handle, and verifies
final handle paths and identities.
If either adapter cannot preserve anchored no-follow traversal, fail closed. Use a
streaming depth-first stack, close a child before visiting its next sibling, and enforce
the same depth, entry, descriptor, and monotonic five-second caps as Rust. Check the
deadline at least every 256 entries. Unix free-space measurement uses `os.fstatvfs` on
the held root descriptor. Windows derives a volume root from the verified handle and
confirms volume identity before `GetDiskFreeSpaceExW`. Query once per filesystem
identity; do not call `shutil.disk_usage` on a root path that can be renamed.
Stable file identity is `(st_dev, st_ino)` when both are nonzero; otherwise count each
entry and mark conservative.

Run policy boundary, filesystem grouping, hard-link fallback, symlink/reparse swap,
depth/entry/descriptor/time caps, scan-versus-candidate-removal, and
measurement-failure tests before adding deletion.

- [ ] **Step 3: Implement the coordinator, lease, and anchored cleanup**

Implement the same managed direct-child protocol under
`tempfile.gettempdir()/hoimin-focused-v1`. Use `fcntl.flock(... LOCK_EX|LOCK_NB)` on Unix
and `msvcrt.locking` on Windows through a small `LeaseLock` adapter. The Windows adapter
uses `CreateFileW` with read/write/delete sharing, converts the handle with
`msvcrt.open_osfhandle`, seeks to byte zero, and locks exactly one existing marker byte
with `LK_NBLCK`; unlock uses the same byte and `LK_UNLCK`. The marker/retention
schema, direct-child validation, prefixes, 24-hour staging rule, and absence verification
must match the approved design; owner kind is `focused_python`. Marker reads are capped
at 64 KiB and managed-root enumeration streams at most 100,000 direct children without
collecting them. It considers at most 256 reclaim candidates and retains at most 256
diagnostic details plus aggregate omitted/error counts. Creation, cleanup
claim, and janitor claim acquire one coordinator lock. Rename active to deleting while
the lease and coordinator remain held, release only the coordinator, and keep the lease
locked through anchored removal and absence verification. Never use path-based
`shutil.rmtree` as a fallback.

Implement resumable post-order cleanup slices. Each slice stops at 50,000 examined
entries or five cooperative seconds, and the owner stops at 60 seconds. A slice that
makes safe progress but exhausts a slice/total budget returns `DEFERRED` and preserves
the `.deleting-` root; a no-progress permission/identity/traversal error returns
`FAILED`. The janitor spends at most 30 seconds, gives each selected candidate one slice
before a second pass, and proves a huge first tree cannot starve a later small tree.
Check deadlines around each operation and document that Python cannot preempt one kernel
filesystem call that does not return.
Use the bounded root-relative component/identity cursor from the Rust contract rather
than the meter's depth-128 handle stack. Hold at most root/current-parent/current-child,
permit cleanup depth 4,096 and cursor names totaling 64 KiB, and add a real depth-129
cleanup regression plus injected depth/cursor overflow tests.
Persist the same bounded lexicographic janitor cursor in the inactive fixed coordinator
slot. Select the next 256 names by a five-second streaming scan under the coordinator,
advance after selection even when cleanup defers, and wrap at the end. An incomplete
selection changes neither cursor nor filesystem. A cursor-slot write failure retains the
prior valid slot but still permits already selected cleanup. Test 257-root
cross-invocation fairness and injected torn-slot recovery without retaining the full
directory listing or allocating a new marker during janitor startup.

Add `.hoimin-heartbeat.json` and `.hoimin-cleanup-ready.json` with schema, run ID, owner
kind, and lease identity. Retain the heartbeat handle and refresh its modification time
at startup and whenever 60 seconds have elapsed since the prior success; do not replace
the path or allow a live interval longer than one minute. An
update failure stops dispatch. The workflow creates and flushes the
cleanup-ready marker only after process reap, both drain joins, and monitor join. The
monitor holds a shared lease guard until its thread exits. Janitor
claim requires a valid cleanup-ready marker or a last valid heartbeat at least 24 hours
old. A fresh, missing, malformed, or future heartbeat preserves an unlocked root. Test a
surviving orphan fixture, immediate cleanup-ready reclaim, 24-hour heartbeat reclaim, a
missing/future timestamp, and a forged marker.
The forged cases cover schema/run/owner/lease/order mismatches. Do not claim marker
authenticity against a malicious same-credential child; the user-only directory is an
account boundary, not a sandbox.

Bootstrap the managed directory and coordinator under the same create-or-open identity
checks as Rust. The coordinator has the same preallocated lock byte and two fixed
CRC/generation cursor slots, byte offsets, little-endian fields, ASCII grammar, padding,
tie/overflow rules, and 1,025-byte total length. Compute the IEEE checksum with
`zlib.crc32(payload) & 0xffff_ffff` and test cross-language golden slot bytes. Serialize
and flush each lease or output-ownership marker before locking its first existing byte on Windows. A
zero-length file, replacement, or non-regular file fails setup.

Apply mode `0700` on Unix. On Windows, use `ctypes` with the same protected-DACL policy
as Rust: obtain the current token user SID, build a DACL for that SID plus `SYSTEM` and
`Administrators`, apply it, and read it back. A DACL API failure is a measurement/setup
failure; it never falls back to a broadly writable directory.

Run creation, active/abandoned/retained/staging, malformed-marker, contention, DACL/mode,
and cleanup-absence tests. Each destructive test must assert the exact allowed root and
the preservation of a neighboring sentinel.

`ManagedScratch.__del__`, context-manager error handling, and child guard finalizers may
close handles but may not recursively remove a tree. Register explicit cleanup with an
outer `ExitStack` immediately after lease publication. If unwinding cannot prove that
the active process, drain threads, and meter are terminal, abandon the root for the next
janitor. Add exception and interpreter-finalizer seam tests.

- [ ] **Step 4: Add monitored command execution and bounded spools**

Extend `CommandRunner.run` with explicit `environment`, `disk_guard`, and one combined
`max_log_bytes`. Replace direct output-file handles with two drain threads and one shared
locked allowance. Divide the combined allowance deterministically: stdout receives
`ceil(limit/2)` and stderr `floor(limit/2)`; within each stream, half is prefix and the
remainder is tail. Discarded bytes are still read. Add to `CommandRecord`:

```python
stdout_observed_bytes: int = 0
stdout_retained_bytes: int = 0
stdout_truncated: bool = False
stderr_observed_bytes: int = 0
stderr_retained_bytes: int = 0
stderr_truncated: bool = False
disk_stop_code: str | None = None
```

Implement prefix and tail storage with fixed-capacity `bytearray` buffers and a circular
tail index. Drain reads are at most 64 KiB. Do not repeatedly concatenate immutable
`bytes`; a `tracemalloc` fixture must keep live spool storage within the configured
allowance plus two read chunks (allocator overhead reported separately).

Reject a command allowance above `MAX_COMMAND_LOG_BYTES = 64 * 1024**2`. Keep the full
bounded prefix/tail buffers only on the active `CommandRecord`. Classification copies at
most `MAX_CANDIDATE_DIAGNOSTIC_BYTES = 16 * 1024` into the candidate record, charges one
shared `MAX_RUN_DIAGNOSTIC_BYTES = 16 * 1024**2` allowance, records truncation/observed
counts, and releases the full spool before the next command. The run record must not
retain a 16 MiB body for every candidate.

The periodic monitor waits 250ms after each completed scan. The command wait loop reads
the published result without starting overlapping scans. On disk stop it records the
first code, terminates and
reaps the process tree using the existing runner lifecycle, joins drain threads, and only
then returns/raises. Cleanup failures append to `cleanup_errors` and do not replace the
primary state.

Take a synchronous sample immediately before launch and another after process reap plus
drain join but before result classification. The disk decision wins if it is ready with
process completion. A meter join timeout marks cleanup deferred, leaves the lease/root
for the next janitor, and disables destructor cleanup rather than deleting under a live
scan. A process-reap or output-drain join timeout has the same deferred-cleanup rule.

Install scoped `SIGINT` and `SIGTERM` handlers on the main thread. Each handler requests
the same cancellation path and restores the prior handler during finalization. Signal
tests send the real catchable signal to a subprocess-hosted wrapper and assert reap,
drain, meter join, and explicit cleanup order.

- [ ] **Step 5: Own cargo-mutants output one candidate at a time**

Extend `Options` and parser with `--max-disk`, `--min-free-space`, `--jobs`,
`--max-log-size`, `--scratch-root`, and `--keep-scratch`. Use the existing byte/duration
parsing style and add byte parsing in one named helper.
Accept jobs only in `1..=4`; this hard ceiling remains in force even when the user raises
the disk limits.

Create one leased run root before cargo-mutants discovery. Set `TMPDIR`, `TMP`, `TEMP`,
an absolute `CARGO_TARGET_DIR` below the leased root, and `CARGO_INCREMENTAL=0` for
inventory, baseline, and mutation commands; leave the shared Cargo home untouched.
Resolve effective Cargo home from `CARGO_HOME` or the platform user default, open it or
its nearest existing parent as a capacity-only handle, and register its filesystem for
every reserve sample. Reopen/identity-check the exact directory if Cargo creates it.
Never walk it for owned bytes or pass it to cleanup. Record the enforcement label
`capacity_only:cargo_home` and deduplicate by filesystem identity. When
the user supplies any
`--file` or `--symbol`, discovery must not append unrelated recent files merely to reach
ten candidates. Add an exact-discovery regression test. Change:

Route cargo-mutants inventory through `CommandRunner`, not
`subprocess.run(capture_output=True)`. Set `MAX_INVENTORY_BYTES = 8 * 1024**2` and limit
complete inventory stdout to `min(MAX_INVENTORY_BYTES, stdout_allowance)`, where
`stdout_allowance = ceil(max_log_bytes / 2)`. Reject
more than 10,000 parsed inventory entries, and reject more than 1,000 selected
candidates. Truncation is an infrastructure error; a truncated JSON prefix must never be
accepted as a complete inventory.
Accept at most 1,000 combined `--file`/`--symbol` selectors and at most 16 KiB of strict
UTF-8 per selector before discovery.

Route cargo-mutants version checks and Git/repository metadata probes through the same
draining runner with a fixed 64 KiB combined allowance. Remove production
`capture_output=True` calls; a noisy or truncated probe is a typed setup failure, not an
occasion to buffer arbitrary child output.

```python
def build_mutation_command(
    repository: Path,
    output_directory: Path,
    candidate: Candidate,
    iterate: bool,
    jobs: int,
) -> list[str]:
```

and always include `--jobs`, `str(jobs)`. Each candidate output path is under leased
scratch. After `classify_mutation_output`, copy only the exact outcome name, bounded log
metadata, and minimal diagnostic tail into `RunRecord`, then delete and absence-verify
the candidate directory before the next loop iteration.
Before decoding, read `--prior-inventory` and candidate `outcomes.json` through one
`read_bounded_regular_json(path, 8 * 1024**2)` helper that rejects symlinks/non-regular
files and reads no more than cap plus one. Extract compiler/debug evidence with bounded
seek/read prefix and tail operations; production code may not call `read_text()` or
`read_bytes()` on a tool-controlled file. Open diagnostic files no-follow and require a
regular file before seeking.
If candidate cleanup returns `FAILED` or `DEFERRED`, close dispatch and finalize the run
incomplete; do not start another candidate while prior bulky output remains.

Extend `RunState` with `DISK_LIMIT` and bump focused `SCHEMA_VERSION` from 1 to 2. Store
disk policy, observations, enforcement, stale cleanup, retained root, and cleanup results
plus `output_recovery.removed_temporary_count`,
`output_recovery.remaining_temporary_names`, and the
effective `jobs` value in `RunRecord`. `RunStore` receives `OwnedOutput` rather
than reopening `Path` for each checkpoint. It writes relative to the held capability,
keeps the ownership marker locked through final flush, and never deletes the user-owned
root. `--keep-scratch` creates the separate retention marker and prints the exact
quoted deletion path.
Schema 2 includes `scratch.path`, `scratch.run_id`, and a cleanup record with exact
`status`, examined/removed counts, omitted details, and remaining-root identity. A clean
record must have `remaining_root=null`; a retained/deferred/failed record must name the
same validated leased root. The sole fail-closed exception is an identity-integrity or
namespace-I/O failure that prevents validating any current path for the still-open root
capability: it reports `deferred` with `remaining_root=null`, records the bounded lookup
diagnostic, stops further dispatch, and carries the complete last pre-clean owned-byte
floor and identity provenance into the post-clean observation. It never reports clean or
removed bytes. These report paths are evidence only and are never accepted by cleanup
APIs.
Treat a successful parent-relative, identity-checked `rmdir` as authoritative absence
when the syscall itself crosses the cleanup deadline: return `clean` and start no
follow-up filesystem operation. If budget remains, perform the anchored absence check
and return `failed` if the name survives, including under an injected no-op adapter.

Return zero only for `COMPLETED`. Preserve 130 for interruption; return 3 for
`BUDGET_EXHAUSTED` and 2 for other non-completed states, including `DISK_LIMIT` and
report failure. Add exact exit-code tests so a partial inventory cannot satisfy a
fail-closed shell gate.

Run exact discovery, command construction, sequential candidate compaction, and
candidate-directory absence tests before outer finalization.

- [ ] **Step 6: Implement catchable finalization and guarded report writes**

Restructure the outer workflow finalization so the order is fixed: close dispatch,
terminate/reap an active command, join both output drains, extract the current compact
outcome, take the final pre-clean sample, stop/join the meter, mark cleanup-ready when
reap, both drains, and monitor join are confirmed, clean or retain scratch,
query end free space, update cleanup evidence, then checkpoint `run.json` and render
Markdown. A report write error changes only the report outcome and does not erase disk
or cleanup evidence already held in memory.
Scratch cleanup uses the 60-second owner budget. If it returns `DEFERRED`, preserve the
exact lease-backed root, set the run incomplete, return the typed nonzero cleanup code,
and still attempt bounded final evidence delivery; do not loop until the filesystem is
empty.

For each checkpoint or final Markdown write, stream into the deterministic
`.hoimin-output-{run_id}-{kind}.tmp` regular file with `create_new`, through a writer that
splits encoder output into at most 64 KiB chunks and a hard
`MAX_REPORT_BYTES = 32 * 1024**2` total. Permit checkpoints
only at quiescent boundaries after process reap and both drains. Reuse the already
required synchronous boundary sample to issue a registry-generation token and freeze
scratch registration/child dispatch until replace completes. Before opening the
temporary, require the boundary sample itself to be below both limits.

During streaming, count only exact bytes newly written to the temporary and require
`base_owned_bytes + temporary_bytes + len(next_chunk) < max_disk` with checked
arithmetic. Make a handle-based free-space query and require
`available_bytes > min_free_bytes + len(next_chunk)` with checked arithmetic before each
chunk; do not rescan the
entire owned tree per 64 KiB. The initial sample already includes the still-present old
destination. This lets a small report succeed under a user `max_disk` below 32 MiB while
the fixed report cap remains 32 MiB. After
flush, verify the generation token, output identity, and free space once more before
replace. Refuse at an inclusive boundary, close and unlink only that exact temporary,
and keep the prior destination intact. Tests keep scratch nonempty while writing, inject
equal/below/above values, mutate the registry to invalidate a token, and catch both
double-counting, output-only undercounting, and a chunk that would cross the reserve. A
call-count fixture proves one checkpoint
uses the existing boundary scan rather than hundreds of tree scans. Require
report-delivery failure through stderr/status without unbounded buffering.
Only one writer may be active, so at most one such temporary exists. Catchable failure
closes and unlinks the exact current file. The next invocation runs the bounded marker
recovery described in Step 1 before rejecting a non-empty output; it never learns a
cleanup name from JSON, Markdown, child output, or an arbitrary path argument.
When final evidence cannot be written and scratch remains, emit one escaped stderr line
of at most 20 KiB with the typed report code and validated remaining root. Test a reserve
boundary with deferred scratch and assert the line contains no child-output bytes and
requires no filesystem write.
For the final post-clean report, cleanup invalidates the pre-clean token; issue a new
token from the required post-clean absence/end-free observation before writing. A
retained or deferred root remains registered in that sample.

Replace `render_markdown(record) -> str` in production with
`write_markdown(record, writer) -> None`, yielding headers and one candidate row at a
time. Do not call the recursive `RunRecord.to_dict()` on the complete record. Supply a
small top-level mapping that references the existing candidate/command lists and a
`JSONEncoder.default` implementation that converts one dataclass at a time, then use
`iterencode`. The guarded writer splits any encoder chunk again at 64 KiB, so neither
renderer duplicates the complete run record or constructs the complete output string.
Keep a compatibility collector only in tests that need a string assertion.

- [ ] **Step 7: Run focused Python tests and commit**

```bash
.venv/bin/python -m unittest discover -s tests -p 'test_focused_mutation*.py'
.venv/bin/python -m unittest tests.test_skills
.venv/bin/mypy tools/focused_mutation.py tools/focused_mutation_support
git diff --check
git add tools/__init__.py tools/focused_mutation.py tools/focused_mutation_support \
  tests/test_focused_mutation_disk.py tests/test_focused_mutation_discovery.py \
  tests/test_focused_mutation_runner.py \
  tests/test_focused_mutation_reporting.py tests/test_focused_mutation_budget.py
git commit -m "feat: bound and clean focused mutation scratch"
```

### Task 8: Add Python correspondence to the Lean oracle

**Files:**

- Create: `tests/test_focused_mutation_disk_oracle.py`
- Modify: `tools/focused_mutation.py`
- Modify: `tools/focused_mutation_support/disk.py`
- Modify: `tests/test_focused_mutation_docs.py`

**Interfaces:**

- Consumes: Task 1 schema-1 corpus and Task 7 public Python policy/lifecycle calls.
- Produces: a strict parser, per-mode match/mismatch/infrastructure results, and broken
  threshold, precedence, dispatch, and cleanup witnesses. It executes every `policy`
  record targeting `python` through public policy calls and every `runtime` record
  targeting `python` through the controlled runner. Rust-only delivery-root records are
  reported as non-applicable by target, not matched or skipped. No adapter reads private
  fields to force correspondence.

- [ ] **Step 1: Write the real-adapter corpus test**

Load `formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl` with strict key validation.
Map each event to the public Python disk-policy/lifecycle entry points; do not duplicate
the expected transition function in the test. Classify every case as match, mismatch, or
infrastructure error, require zero reviewed mismatches, and retain deliberately broken
threshold/stickiness/cleanup variants that each fail at least one corpus case.
Require execution of every Python-targeted long fixed lifecycle trace, including
unsafe-cleanup rejections; a depth-five explorer statistic cannot satisfy those case IDs.
Compare the expected and executed Python case-ID sets for equality. Reject an unknown,
duplicate, or empty target list, and never count a Rust-only record in Python match
statistics.

- [ ] **Step 2: Run and commit**

```bash
.venv/bin/python -m unittest tests.test_focused_mutation_disk_oracle -v
.venv/bin/python -m unittest tests.test_focused_mutation_docs -v
git diff --check
git add tests/test_focused_mutation_disk_oracle.py \
  tools/focused_mutation.py tools/focused_mutation_support/disk.py \
  tests/test_focused_mutation_docs.py \
  docs/superpowers/plans/2026-08-27-disk-safe-mutation-execution.md
git commit -m "test: audit Python disk guard correspondence"
```

### Task 9: Document safe mutation operation and compatibility

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`
- Modify: `docs/superpowers/plans/2026-08-27-disk-safe-mutation-execution.md`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `tests/test_focused_mutation_docs.py`

**Interfaces:**

- Consumes: the final CLI and wrapper options from Tasks 4 and 7.
- Produces: user guidance and executable documentation assertions for defaults,
  enforcement limits, cleanup, retention, and safe one-worker invocation.

- [ ] **Step 1: Add documentation contract RED tests**

Require README/development docs to contain the exact public defaults, generated-workspace
versus copy-size distinction, mandatory reserve, the fact that 250 ms is a post-scan
delay rather than a reaction guarantee, the ordinary 5.25-second cooperative detection
ceiling and blocking-syscall limitation,
single-worker focused default, cleanup/retention/stale-recovery behavior, capability
labels, jobs maximum four, scan/inventory/tool-JSON/selector/candidate caps,
cleanup-ready/heartbeat grace,
cleanup slice/owner/janitor budgets, 64 MiB command-log hard maximum, 16 KiB/16 MiB
diagnostic caps, 32 MiB report cap, 16 KiB encoded-path cap, Cargo-home capacity-only monitoring, and an exact safe
wrapper example. Require no routine example matching:

```text
cargo mutants --workspace --jobs ([2-9]|[1-9][0-9]+)
```

- [ ] **Step 2: Update docs**

Document these commands:

```bash
test -x .venv/bin/python
hoimin_python="$(pwd -P)/.venv/bin/python"
hoimin run --file tools/focused_mutation_support/disk.py --allow-best-effort-memory --max-workspace-size 8GiB --min-free-space 10GiB -- "$hoimin_python" -m unittest tests.test_focused_mutation_disk
python3 -c 'import shutil, sys; sys.exit(0 if shutil.disk_usage(".").free > 10 * 1024**3 else 1)'
mutation_output="$(mktemp -d /tmp/hoimin-focused.XXXXXX)"
test -x .venv/bin/python
.venv/bin/python tools/focused_mutation.py \
  --budget 30m --jobs 1 --max-disk 8GiB --min-free-space 10GiB \
  --max-log-size 16MiB --output "$mutation_output"
```

Explain that raising a consumption limit or lowering the reserve is explicit risk
acceptance; monitoring cannot prevent one child from consuming the reserve between
samples; the periodic cadence is scan duration
plus 250 ms, normally at most about 5.25 seconds under the cooperative scan deadline;
no per-file signal limit is installed for scored mutation because handled write-limit
failures cannot be attributed without risking false mutation credit;
focused cargo-mutants jobs default to one and cannot exceed four; and only a verified
named quota backend is aggregate hard enforcement. Include how to
remove a retained exact scratch path without suggesting wildcard deletion. State that
the output directory must start absent or empty, is monitored but preserved, and should
be a fresh `mktemp -d` path as in the example.
State that environment provisioning, including `uv sync --frozen`, occurs outside the
wrapper's monitoring boundary and is not part of the safe mutation command. Require the
operator to provision it earlier with separate capacity controls, then invoke the guarded
mutation command through the already existing `.venv/bin/python`; do not present
`uv run ... focused_mutation.py` as a safe example.
State that the first `run.json` checkpoint follows setup. An abrupt termination before
that checkpoint can leave only `.hoimin-output-owner`, with no recoverable mutation
result; direct operators to use `run.json` for recovery only when it exists.
Explain that deferred cleanup is incomplete and retried by the bounded janitor, that a
single blocking filesystem syscall cannot be preempted portably, and that shared Cargo
home is reserve-monitored but never traversed or deleted.
Remove routine raw `cargo mutants --workspace` examples. Describe a complete inventory
only behind a verified named quota backend or an isolated hard-capacity volume, one
worker, a separate 10 GiB host reserve, and exact-volume cleanup.
State that this repository does not provide a supported complete-inventory command and
that complete inventory remains unavailable until an operator provisions and verifies
that infrastructure; do not invite operators to improvise a raw workspace command.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p hoimin-cli --test cli_config -- --nocapture
.venv/bin/python -m unittest tests.test_focused_mutation_docs -v
if rg -n 'cargo mutants --workspace --jobs ([2-9]|[1-9][0-9]+)' README.md docs/development.md; then
  exit 1
else
  test "$?" -eq 1
fi
if rg -n '^cargo mutants --workspace(?:[[:space:]]|$)' README.md docs/development.md; then
  exit 1
else
  test "$?" -eq 1
fi
git diff --check
git add README.md docs/development.md crates/hoimin-cli/tests/cli_config.rs \
  tests/test_focused_mutation_docs.py \
  docs/superpowers/plans/2026-08-27-disk-safe-mutation-execution.md
git commit -m "docs: explain disk-safe mutation execution"
```

Exit 1 is the only accepted no-match result. A tool/I/O error such as exit 2 fails the
gate.

### Task 10: Final verification, review, and delivery handoff

**Files:**

- Modify: `crates/hoimin-core/Cargo.toml`
- Modify: `crates/hoimin-core/src/disk.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/src/report.rs`
- Modify: `crates/hoimin-core/tests/disk_policy.rs`
- Modify: `crates/hoimin-core/tests/lean_disk_guard_oracle.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`
- Modify: `crates/hoimin-core/tests/report_policy.rs`
- Modify: `crates/hoimin-cli/src/process/mod.rs`
- Modify: `crates/hoimin-cli/src/resource/mod.rs`
- Modify: `crates/hoimin-cli/src/resource/portable.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/src/workspace/disk.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/owned.rs`
- Modify: `crates/hoimin-cli/tests/lean_disk_shutdown_oracle.rs`
- Modify: `crates/hoimin-cli/tests/process_handler.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Modify: `crates/hoimin-cli/tests/run_e2e.rs`
- Modify: `docs/json-schema/run-event.schema.json`
- Modify: `docs/superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md`
- Modify: `docs/superpowers/plans/2026-08-27-disk-safe-mutation-execution.md`
- Modify: `formal/HoiminOracle/DiskGuardAuditMain.lean`
- Modify: `formal/HoiminOracle/DiskGuardBrokenConsumer.lean`
- Modify: `formal/HoiminOracle/HoiminOracle/DiskGuardCases.lean`
- Modify: `formal/HoiminOracle/HoiminOracle/DiskGuardModel.lean`
- Modify: `formal/HoiminOracle/HoiminOracle/DiskGuardProofs.lean`
- Modify: `formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl`
- Modify: `tests/test_focused_mutation_disk.py`
- Modify: `tests/test_focused_mutation_disk_oracle.py`
- Modify: `tests/test_focused_mutation_reporting.py`
- Modify: `tools/focused_mutation.py`
- Modify: `tools/focused_mutation_support/disk.py`
- Create ignored evidence under:
  `.superpowers/sdd/2026-08-27-disk-safe-mutation-execution/`

**Interfaces:**

- Consumes: every tracked deliverable and its focused test entry point.
- Produces: final-SHA native/compatibility/formal evidence, an explicit record that Rust
  mutation was prohibited and omitted, three independent same-SHA reviews, and a
  delivery handoff. Push/PR work occurs only after the user's explicit choice.

Task 10 review fixes may touch any file above because they close cross-task lifecycle,
schema, platform, formal-correspondence, or reporting findings. Stage only the explicit
files that changed, commit each fix wave before freezing `candidate-sha.txt`, and never
use `git add -A` for this handoff. For the final current fix wave the staging command is:

```bash
git add -- \
  crates/hoimin-core/src/machine.rs \
  crates/hoimin-core/tests/lean_disk_guard_oracle.rs \
  crates/hoimin-core/tests/machine.rs \
  crates/hoimin-cli/src/workspace/disk.rs \
  tools/focused_mutation_support/disk.py \
  tests/test_focused_mutation_disk.py \
  docs/superpowers/plans/2026-08-27-disk-safe-mutation-execution.md
git commit -m "fix: preserve exact secondary evidence"
```

- [ ] **Step 1: Rebase on current main before final evidence**

```bash
set -e
git fetch origin
git rebase origin/main
test -z "$(git status --porcelain=v1 --untracked-files=all)"
git diff --check origin/main...HEAD
mkdir -p .superpowers/sdd/2026-08-27-disk-safe-mutation-execution
git rev-parse HEAD > .superpowers/sdd/2026-08-27-disk-safe-mutation-execution/candidate-sha.txt
```

Resolve only expected documentation/config fixture overlaps while preserving both sides'
semantics. Stop and escalate on unexpected production conflicts. Every later artifact
must name the post-rebase candidate SHA.

- [ ] **Step 2: Run all ordinary gates sequentially**

Run this as one fail-closed shell block. `check_disk` must pass before the first command
and after every Cargo, Lean, Python-test, contract, and wheel command:

```bash
set -e
candidate_sha="$(cat .superpowers/sdd/2026-08-27-disk-safe-mutation-execution/candidate-sha.txt)"
test "$(git rev-parse HEAD)" = "$candidate_sha"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
check_disk() { python3 -c 'import os, pathlib, shutil, sys, tempfile; roots=[pathlib.Path("."), pathlib.Path(tempfile.gettempdir()), pathlib.Path(os.environ.get("CARGO_HOME", pathlib.Path.home()/".cargo")), pathlib.Path(os.environ.get("UV_CACHE_DIR", pathlib.Path.home()/".cache/uv"))]; existing=[]; [(lambda p: existing.append(p))(next(x for x in [p,*p.parents] if x.exists())) for p in roots]; seen=set(); free=[]
for p in existing:
 d=p.stat().st_dev
 if d not in seen: seen.add(d); free.append(shutil.disk_usage(p).free)
sys.exit(0 if free and min(free) > 10 * 1024**3 else 1)'; }
check_disk
test -x .venv/bin/python
cargo fmt --all -- --check
check_disk
cargo clippy --workspace --all-targets --all-features -- -D warnings
check_disk
cargo test --workspace
check_disk
cargo test -p hoimin-cli --test run_e2e
check_disk
.venv/bin/python -m unittest discover -s tests
check_disk
(cd formal/HoiminOracle && lake build)
check_disk
(cd formal/HoiminOracle && lake exe generate_disk_guard -- --check corpus/disk-guard-lifecycle.jsonl)
check_disk
(cd formal/HoiminOracle && lake exe generate_disk_guard -- --sensitivity)
check_disk
cargo +1.88 check --workspace --all-targets --all-features --locked
check_disk
cargo +nightly-2026-07-27 test --workspace -- -Z unstable-options --shuffle
check_disk
cargo test -p hoimin-core --features contracts
check_disk
cargo test -p hoimin-cli --features contracts
check_disk
core_tree="$(mktemp /tmp/hoimin-core-tree.XXXXXX)"
trap 'unlink "$core_tree" 2>/dev/null || true' EXIT
cargo tree -p hoimin-core --edges normal --prefix none > "$core_tree"
if rg -n '(^| )(tokio|rusqlite|tempfile|windows-sys|libc|hoimin-cli)( |$)' "$core_tree"; then
  exit 1
else
  test "$?" -eq 1
fi
check_disk
uvx maturin build --release
check_disk
.venv/bin/python tests/wheel_smoke.py
check_disk
git diff --check origin/main...HEAD
test "$(git rev-parse HEAD)" = "$candidate_sha"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

Remove only the exact wheel created by this run after smoke verification. Retain normal
build caches unless disk safety requires an explicit, separately approved cleanup.

- [ ] **Step 3: Run native macOS/Linux/Windows evidence**

On macOS and native Linux, run focused disk preflight/threshold/cleanup tests and record
actual filesystem/free-space capability labels. Linux container evidence must use
`--init`, a non-root UID/GID, a Linux-native temp volume, exact candidate SHA, and
read-only source. Windows CI must run locked-lease and process-tree lifecycle tests.

No platform skip may be reported as execution. A missing platform produces an explicit
evidence gap, not a fabricated pass.

Every platform report must name `candidate_sha`; after all native evidence, re-run the
exact SHA and clean-status assertions from Step 2.

- [x] **Step 4: Omit Rust mutation execution by explicit user directive**

On 2026-08-30 the user prohibited further Rust mutation testing because prior runs
caused unsafe disk growth. Do not invoke raw cargo-mutants, the focused wrapper, an
inventory-only cargo-mutants command, or an origin/main mutation comparison. This is an
intentional safety decision, not a passed or fabricated mutation result. Record the
omission and reason beside the candidate SHA. Acceptance for this delivery therefore
uses the complete ordinary Rust/Python suite, Lean build/freshness/sensitivity,
compatibility/contracts/wheel gates, native macOS/Linux evidence, and three independent
same-SHA reviews.

- [ ] **Step 5: Three same-SHA independent reviews**

Request independent reviews of:

1. disk policy, threshold, and report semantics;
2. process drain, monitor join, cleanup, leases, and deletion security;
3. Python bounded logs/scratch lifecycle, docs, and delivery evidence.

Each reviewer must inspect the exact candidate SHA and clean status. Any code change
invalidates all three approvals and returns to Steps 2–5. Perform three additional local
self-critical passes: spec coverage, error/race paths, and security/destructive scope.
Record each verdict beside `candidate-sha.txt`. A review is not current unless its
recorded SHA equals that file and the reviewed worktree was clean.

- [ ] **Step 6: Present the finishing options; push or create a PR only if selected**

After all same-SHA gates/reviews pass, present the finishing-branch menu. Run the
following only if the user explicitly selects the push/PR option:

```bash
git push -u origin feat/disk-safe-mutation
gh pr create --base main --head feat/disk-safe-mutation \
  --title "Prevent mutation runs from exhausting disk" \
  --body-file /tmp/hoimin-disk-safe-pr-body.md
```

The PR body links the approved design, states defaults/limits, enumerates native evidence,
explains that no Rust mutation testing of any scope was run by explicit user directive,
and records exact cleanup outcomes.
Verify remote head SHA once, then watch actual checks at 60-second intervals. Do not poll
GitHub Actions every 10 seconds:

```bash
set -e
candidate_sha="$(cat .superpowers/sdd/2026-08-27-disk-safe-mutation-execution/candidate-sha.txt)"
remote_sha="$(gh pr view --json headRefOid --jq .headRefOid)"
test "$remote_sha" = "$candidate_sha"
gh pr checks --watch --interval 60
```

Require the complete PR job set from `.github/workflows/ci.yml`: Quality on Ubuntu,
Windows, and macOS; Rust MSRV; Rust on Ubuntu, Windows, and macOS; randomized Rust;
contracts; Core dependency purity on Ubuntu and Windows; Wheel smoke on Ubuntu,
Windows, and macOS; and linux-best-effort. `linux-cgroup-v2-hard` is push-to-main-only
and must be recorded as absent by workflow design, not passed or failed on the PR. If CI
changes code, repeat local gates, reviews, push, remote SHA guard, and checks. Do not
merge without separate user or maintainer authorization.

## Completion checklist

- [ ] Defaults are 8 GiB owned bytes, 10 GiB reserve, 250ms, jobs 1 with a hard
  maximum of 4, and logs 16 MiB.
- [ ] Plan/resume schemas reject old incomplete artifacts explicitly; new reports emit
  schema 3, while schema-2 progress input uses only the isolated legacy reader.
- [ ] Disk stops are infrastructure failures and never improve mutation score.
- [ ] Processes are terminated/reaped and output drained before cleanup.
- [ ] Monitor stops/joins before one logical cleanup request per owned root.
- [ ] Cleanup is sliced/budgeted and verifies absence; `Deferred` remains incomplete and
  retryable. Runtime retains primary, execution-cleanup, and delivery
  outcomes separately. A delivered report contains run/execution cleanup plus an honest
  post-delivery cleanup marker; failed delivery finalization is surfaced through typed
  error, stderr, and incomplete session state.
- [ ] Live/retained/malformed/symlink/foreign roots are never deleted.
- [ ] Cleanup-ready or 24-hour-stale valid leases are reclaimed on a later start;
  fresh/missing/malformed/future heartbeats are preserved.
- [ ] Focused candidate trees are compacted then deleted after each result.
- [ ] Combined stdout/stderr retained bytes obey the configured command cap.
- [ ] One active command may hold the bounded full spool; persisted candidate diagnostic
  bodies obey the 16 KiB per-candidate and 16 MiB run-wide caps.
- [ ] Complete inventory stdout obeys both the fixed 8 MiB cap and its share of the
  configured command cap; short metadata probes are also drained and bounded.
- [ ] Prior-inventory/outcome JSON, selector count/bytes, reported paths, diagnostic
  seeks, and atomic output temporaries obey their hard caps.
- [ ] The safe mutation command starts with an existing `.venv/bin/python`; environment
  provisioning cannot occur between final preflight and wrapper startup.
- [ ] Report failure or non-clean delivery cleanup cannot set lifecycle `finished` or
  acknowledge output/session completion.
- [ ] Lean proofs, sensitivity, freshness, Rust adapter, and Python adapter pass.
- [ ] macOS/Linux native evidence is honest; Windows CI lifecycle evidence passes.
- [ ] Rust mutation execution is omitted and recorded by explicit user directive; no
  cargo-mutants process or new mutation artifact exists for the candidate SHA.
- [ ] Final SHA is clean and independently reviewed. If the user selects the PR option,
  it is pushed and equals the PR head; otherwise remote delivery is explicitly not
  applicable for this handoff.
- [ ] If the user selects the PR option, GitHub checks are watched at 60-second intervals
  and the exact PR job set passes; otherwise this external-state gate is marked not
  applicable. Push-to-main-only jobs are identified separately.
