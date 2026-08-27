# Disk-safe mutation execution implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox
> (`- [ ]`) syntax for tracking. Use `superpowers:test-driven-development` for each
> behavior change, `lean-test-oracle` for Tasks 1–2 and 8,
> `superpowers:systematic-debugging` for an unexpected failure, and
> `superpowers:verification-before-completion` before a delivery claim.

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

**Tech stack:** Rust 1.88+ (`tokio`, `tempfile`, `fs2` 0.4.3, `cap-std` 4.0.2,
`cap-fs-ext` 4.0.2, `serde`), Python 3.14 standard library, Lean 4/Lake,
cargo-mutants 27.1.0.

**Approved design:**
`docs/superpowers/specs/2026-08-27-disk-safe-mutation-execution-design.md`

## Review record

This plan received three fresh passes on 2026-08-27: architecture/security,
implementation/TDD correspondence, and formal/race/delivery. Material findings were
folded into the steps; open-ended instructions are not permitted. Task 10 repeats three
same-SHA implementation reviews because document approval is not implementation
evidence.

## Global Constraints

- Work only in `/Users/hayao/RustroverProjects/hoimin/.worktrees/disk-safe-mutation`
  on branch `feat/disk-safe-mutation`.
- Before each task, require a clean tracked worktree and record the exact HEAD. Preserve
  ignored evidence under `.superpowers/sdd/2026-08-27-disk-safe-mutation-execution/`.
- Use `apply_patch` for source and documentation edits. Do not delete repository
  `target/`, Cargo caches, user output, or any path not created by the current task.
- Use small fake byte counts in unit tests. Never create GiB-scale fixtures.
- Do not run full-workspace cargo-mutants. The only mutation gate is the focused,
  single-worker command in Task 10, guarded by a fresh scratch root and disk preflight.
- Start every real mutation or compatibility build only when more than 10 GiB is free.
  Stop immediately on ENOSPC, rising unowned scratch, or monitor/tool failure.
- A disk stop is an infrastructure failure. It must not become `Killed`, increment the
  mutation score, or overwrite a prior primary failure.
- After every code-review or CI-driven edit, invalidate evidence for the old SHA and
  repeat the affected gates and independent reviews on one clean final SHA.
- Commit only the files named by the task. Keep generated Lean corpus files tracked;
  keep runtime reports and temporary mutation output ignored.

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
- Produces: `HoiminOracle.DiskGuard.step`, `run`, `Invariant`, the seven named theorems,
  an eight-event-family depth-five bounded explorer, and schema-1 JSONL records with
  exact correspondence mode plus `policy` or `runtime` layer. Tasks 2, 6, and 8 consume
  every applicable record and may not reinterpret its premises.

- [ ] **Step 1: Write the finite model and fixed cases**

Model thresholds symbolically, not as GiB values:

```lean
namespace HoiminOracle.DiskGuard

inductive StopReason where
  | sizeExceeded | reserveReached | measurementFailed | processFailed
  deriving BEq, DecidableEq, Repr

abbrev RootId := Fin 2

inductive ComponentState where | pending | active | terminal | failed
  deriving BEq, DecidableEq, Repr

structure State where
  stop : Option StopReason := none
  active : Nat := 0
  dispatched : Nat := 0
  ownedRoots : Finset RootId := {}
  deliveryRoots : Finset RootId := {}
  cleanupRequested : Finset RootId := {}
  cleanupClean : Finset RootId := {}
  cleanupFailed : Finset RootId := {}
  cleanupRetained : Finset RootId := {}
  process : ComponentState := .pending
  outputDrain : ComponentState := .pending
  monitorJoin : ComponentState := .pending
  report : ComponentState := .pending
  finished : Bool := false
  deriving BEq, Repr

inductive Event where
  | dispatch
  | observe (owned maxOwned free minFree : Nat)
  | meterFailed
  | processFailed
  | processExited
  | requestCleanup (root : RootId)
  | cleanupSucceeded (root : RootId)
  | cleanupFailed (root : RootId)
  | cleanupRetained (root : RootId)
  | outputDrained
  | monitorJoined
  | monitorJoinFailed
  | reportSucceeded
  | reportFailed
  | finish
  deriving BEq, Repr
```

Use `owned >= maxOwned` and `free <= minFree` as inclusive terminal boundaries.
When both hold in one successful observation, choose `reserveReached` and retain
`sizeExceeded` as secondary evidence; a failed measurement carries no numeric reading.
`step` must preserve the first stop reason, reject dispatch once `stop.isSome`, permit
exactly one logical cleanup request per owned root, and reject `finish` until active
work is zero, the process/output/monitor/report components are terminal, and every owned
root has exactly one terminal cleanup outcome. It rejects a cleanup request for a root
in `deliveryRoots` until `report` is terminal.

Define strict fixed cases for below/at/above thresholds, meter failure, zero/one/two
active processes, two competing stop reasons, cleanup success/failure/retention, and
report success/failure. Normalize them to dispatch, observe, process terminal, output
terminal, monitor terminal, cleanup, report terminal, and finish. Explore family
skeletons through depth five (`1 + 8 + 8² + 8³ + 8⁴ + 8⁵ = 37,449`) before rejection
and root/payload symmetry expansion.
Every JSONL record must contain schema `1`, case ID, correspondence mode, layer, event
sequence, and the complete expected terminal observation. Record transition/state
counts, elapsed time, and peak memory in the generator summary.

- [ ] **Step 2: Add proof obligations and deliberate broken variants**

Prove seven named obligations: `stopped_never_dispatches`,
`first_reason_is_sticky`, `cleanup_requested_once_per_root`,
`cleanup_failure_is_not_clean`, `finished_implies_cleanup_terminal`, and
`finished_implies_components_terminal`, plus `delivery_cleanup_after_report`. Each theorem
quantifies over a starting state and an accepted event trace, and states its property on
the trace fold. This shared trace premise prevents the proofs from drifting away from
the corpus generator.

Add broken functions for strictness (`>` instead of `>=`), reserve direction,
stop-reason overwrite, post-stop dispatch, duplicate cleanup, clean-on-cleanup-error,
and early finish. `--sensitivity` must return zero only if every broken family is
distinguished by at least one fixed witness.

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
../../.venv/bin/python ../../tools/lean_resource_guard.py --timeout-seconds 30 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/disk-guard-build.json -- lake build HoiminOracle.DiskGuardProofs generate_disk_guard
../../.venv/bin/python ../../tools/lean_resource_guard.py --timeout-seconds 20 --rss-limit-mib 768 --sample-ms 250 --stats /tmp/disk-guard-sensitivity.json -- lake exe generate_disk_guard -- --sensitivity
lake exe generate_disk_guard -- --output corpus/disk-guard-lifecycle.jsonl
lake exe generate_disk_guard -- --check corpus/disk-guard-lifecycle.jsonl
lake env lean HoiminOracle/DiskGuardBrokenConsumer.lean
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
}

pub enum DiskSecondary {
    Observation { reason: DiskStopReason, value: DiskObservation },
    Error { code: String, message: String },
}

pub struct DiskRootId(u8);

pub enum DiskCleanupOutcome {
    Clean,
    Failed(String),
    Retained,
}

impl DiskRootId {
    pub fn new(value: u8) -> Self;
}

impl DiskLifecycle {
    pub fn new(owned_roots: impl IntoIterator<Item = DiskRootId>) -> Self;
    pub fn observe(&mut self, policy: DiskPolicy, value: DiskObservation) -> DiskDecision;
    pub fn fail_measurement(&mut self, message: String) -> DiskDecision;
    pub fn request_dispatch(&mut self) -> bool;
    pub fn process_exited(&mut self);
    pub fn request_cleanup(&mut self, root: DiskRootId) -> bool;
    pub fn complete_cleanup(&mut self, root: DiskRootId, outcome: DiskCleanupOutcome);
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
pub const WORKSPACE_CLEANUP_FAILED: &str = "workspace.cleanup.failed";
```

Keep the decision and transition seams uniquely named `evaluate_disk_policy` and
`apply_disk_lifecycle_event`; public `DiskLifecycle` methods delegate to them. These are
production helpers, not test-only duplicates, and Task 10 targets them narrowly.

Do not add a disk variant to `ProcessTermination`; a disk stop is never a test
termination. Use checked counters and make duplicate logical cleanup requests return
`false` without incrementing state.

- [ ] **Step 3: Complete policy and correspondence tests**

Cover both boundaries, precedence, secondary errors, zero/one/two active work items,
duplicate cleanup, cleanup absence failure, and finish gating. The Lean adapter must
execute every `policy` record through public methods, reject unknown layers/modes,
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
- Modify: `crates/hoimin-cli/tests/progress.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Modify: `crates/hoimin-cli/tests/report_heap.rs`
- Modify: `crates/hoimin-cli/tests/session_handler.rs`
- Create: `crates/hoimin-cli/tests/golden/reports/schema-v3-original.json`

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
    pub enforcement: Vec<String>,
    pub stop: Option<DiskStopReport>,
    pub cleanup: Vec<DiskCleanupReport>,
    pub reclaimed_bytes: Option<u64>,
    pub stale_roots_reclaimed: u64,
}

pub struct DiskFilesystemReport {
    pub key: String,
    pub start_available_bytes: Option<u64>,
    pub minimum_available_bytes: Option<u64>,
    pub end_available_bytes: Option<u64>,
}

pub enum DiskCleanupStatus {
    Clean,
    Failed,
    Retained,
    CleanupAfterDelivery,
}
```

`RunSummary.disk` is present for new schema reports and must distinguish the primary run
and cleanup outcomes. Internally retain a third `ReportDeliveryOutcome`; a delivery
failure is observable through the shell result, stderr, and incomplete session because
the failed output channel cannot reliably describe its own failure. Add serde and
failure-path tests before implementation and require RED. Add an exact machine sequence
RED requiring `Cleanup -> EmitOutput(RunFinished) -> OutputEmitted -> FinishSession`.
Both report failure and post-report session-finalization failure must leave the session
incomplete and return a typed error.

Execution-root records use `Clean`, `Failed`, or explicit `Retained`. The delivery-root
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

Update golden reports mechanically only after the semantic tests pass. Old incomplete
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
git add crates/hoimin-core/src/config.rs crates/hoimin-core/src/resume.rs \
  crates/hoimin-core/src/report.rs crates/hoimin-core/src/machine.rs \
  crates/hoimin-core/tests/lean_oracle.rs \
  crates/hoimin-core/tests/lean_report_sequence_oracle.rs \
  crates/hoimin-core/tests/plan_config.rs \
  crates/hoimin-core/tests/resume_policy.rs \
  crates/hoimin-core/tests/report_policy.rs crates/hoimin-core/tests/machine.rs \
  crates/hoimin-cli/src/plan.rs crates/hoimin-cli/src/report/human.rs \
  crates/hoimin-cli/src/session/mod.rs \
  crates/hoimin-cli/tests/progress.rs \
  crates/hoimin-cli/tests/report_handler.rs \
  crates/hoimin-cli/tests/report_heap.rs crates/hoimin-cli/tests/session_handler.rs \
  crates/hoimin-cli/tests/golden/reports/schema-v3-original.json
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
- Create: `crates/hoimin-cli/src/workspace/disk.rs`
- Modify: `crates/hoimin-cli/src/workspace/mod.rs`
- Modify: `crates/hoimin-cli/src/workspace/copy.rs`
- Modify: `crates/hoimin-cli/src/shell.rs`
- Modify: `crates/hoimin-cli/src/report/mod.rs`
- Create: `crates/hoimin-cli/tests/disk_workspace.rs`
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
- depth 129, entry 1,000,001, marker 64 KiB plus one byte, and direct child 100,001
  each fail closed without unbounded allocation;
- free bytes equal to the reserve stop at preflight;
- manifest logical bytes at the maximum fail before snapshot creation;
- active locked lease is skipped;
- owner cleanup and janitor contend on the coordinator and exactly one claims rename;
- abandoned valid unlocked lease is reclaimed;
- retained, malformed, wrong-owner, symlink, nested, and foreign entries are preserved;
- malformed/path-like run IDs and a managed leaf not owned by the current user fail;
- staging entries are reclaimed only after 24 hours and only when empty except for a
  valid marker;
- removal callback success without path absence is `workspace.cleanup.failed`.

Require these tests to fail because `workspace::owned` and `workspace::disk` do not yet
exist:

```bash
cargo test -p hoimin-cli --test disk_workspace --no-run
```

- [ ] **Step 2: Add the cross-platform primitive dependency**

Add direct `fs2 = "0.4.3"` and `cap-std = "4.0.2"` to `hoimin-cli`; retain
`cap-fs-ext` at lockfile version 4.0.2. Use:

```rust
use fs2::FileExt;
fs2::available_space(path)
fs2::FileExt::try_lock_exclusive(&lease)
cap_fs_ext::DirExt::open_dir_nofollow(&parent, child)
```

The lock is advisory and valid only because both creator and janitor obey the protocol.
Do not use PID liveness as a lock substitute. Keep `libc` and `windows-sys` for stable
filesystem/file identities and the Unix hard backstop; do not add a second lock crate.

- [ ] **Step 3: Implement managed-root creation and the lease API**

Use these fixed names and marker schemas:

```rust
const MANAGED_DIR: &str = "hoimin-workspaces-v1";
const STAGING_PREFIX: &str = ".staging-";
const ACTIVE_PREFIX: &str = "run-";
const DELETING_PREFIX: &str = ".deleting-";
const LEASE_FILE: &str = ".hoimin-lease.json";
const RETAIN_FILE: &str = ".hoimin-retain.json";
const COORDINATOR_FILE: &str = ".hoimin-coordinator";
const LEASE_SCHEMA: u32 = 1;
const MAX_MARKER_BYTES: u64 = 64 * 1024;
const MAX_MANAGED_CHILDREN: usize = 100_000;

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

`ManagedRunRoot::create(parent, run_id)` must canonicalize/create the managed parent,
reject a symlinked managed leaf and a run ID other than the canonical UUID text it
generated, verify current-user ownership, set user-only permissions, create a staging
directory and marker with `create_new`, lock
the marker, acquire the managed-root coordinator, rename to `run-{run_id}`, and retain
the open locked file. On Windows, use `OpenOptionsExt` to grant read/write/delete sharing
and request the access needed for parent-directory rename/removal, so the locked lease
can remain open through absence verification. Expose only:

```rust
pub(crate) fn path(&self) -> &Utf8Path;
pub(crate) fn tempdir(&self, prefix: &str) -> Result<TempDir, WorkspaceError>;
pub(crate) fn retain(&self) -> Result<(), WorkspaceError>;
pub(crate) fn cleanup(&self) -> CleanupRecord;
pub(crate) fn abandon_for_janitor(&self, reason: String) -> CleanupRecord;
pub(crate) fn reclaim_abandoned(parent: &Path, now: SystemTime) -> ReclaimReport;
```

Cleanup closes child handles first, acquires the coordinator, revalidates its active
child and marker through anchored handles, and renames only its own direct child to
`.deleting-{run_id}` while the lease remains locked. It then releases the coordinator
before anchored removal, retains the per-run lease through absence verification, and
closes it only afterward. `Drop` is best effort only when cleanup was not deferred;
`abandon_for_janitor` permanently disables recursive Drop cleanup for that instance.

Run the creation, UUID/name, active-lease, and owner/janitor coordinator race tests before
continuing. The only accepted failures at this point are deletion and platform-security
cases implemented by the next two steps.

- [ ] **Step 4: Implement and test the platform security boundary**

On Unix, set and verify mode `0700` on the managed directory. On Windows, add the needed
`windows-sys` security authorization features and install a protected DACL granting full
access only to the current user SID, `SYSTEM`, and `Administrators`; read it back before
creating a staging child. A failure to establish this boundary fails closed. Tests use a
platform adapter rather than shelling out to `icacls`.

On Unix, implement anchored removal through directory-relative no-follow operations. On
Windows, use handles opened with `DELETE`, directory-list, and attribute rights; reject
reparse points and use handle-relative rename/disposition operations. Never close the
validated handle and recursively reopen its accumulated path. Run symlink/junction
swap, protected-DACL, read-only-child repair, and absence-verification tests.

- [ ] **Step 5: Implement the bounded stale-root janitor**

The janitor opens the canonical managed root as `cap_std::fs::Dir`, enumerates at most
`MAX_MANAGED_CHILDREN` direct entries, and opens candidates with
`DirExt::open_dir_nofollow`; it never performs check-then-path-open traversal. It caps
marker reads before deserialization, validates schema/owner/name/run ID and retention,
then acquires the nonblocking lease and coordinator. Under both locks it compares the
already-open child and marker identities again before renaming. Valid `.deleting-`
entries left by failed cleanup are eligible for the same locked retry; unknown entries
are preserved. Never consume a cleanup path from JSON output, environment, or a child
command.

Keep the validated claim transition in one uniquely named production helper,
`claim_managed_child`; both owner cleanup and janitor call it with their allowed prefix
and expected identity.

After claiming or recognizing a `.deleting-` entry, release the coordinator but keep
the candidate lease locked through removal and absence verification. If a staging entry
has a marker, validate and lock it before claim; never treat age alone as liveness.

Run active, abandoned, retained, malformed, oversized-marker, excessive-child, and
24-hour staging tests with two coordinators contending. Require exactly one reclaim
claim and no deletion of any rejected fixture.

- [ ] **Step 6: Implement the meter**

Expose injectable traits and production implementations:

```rust
pub(crate) trait AvailableSpace: Send + Sync {
    fn available(&self, path: &Path) -> io::Result<u64>;
    fn filesystem_key(&self, path: &Path) -> io::Result<FilesystemKey>;
}

pub(crate) struct DiskMeter<S> { /* roots, S */ }

pub(crate) struct MeterReading {
    pub owned_bytes: u64,
    pub available_by_filesystem: BTreeMap<FilesystemKey, u64>,
    pub conservative_entries: bool,
    pub elapsed: Duration,
}
```

Walk iteratively from `cap_std::fs::Dir` handles. Enumerate a handle, obtain no-follow
metadata, and open every child directory with `open_dir_nofollow` before adding its
handle to the work queue. Never reopen a descendant by accumulated path. Use checked
addition and fail at depth 128 or 1,000,000 entries. On Unix identify files with
`MetadataExt::{dev, ino}` and filesystems with `dev`; on Windows group by a normalized
volume root returned by `GetVolumePathNameW` and use conservative entry counting. Query
`fs2::available_space` once per filesystem key.

Name the production walker `measure_owned_tree` and the platform deletion entry point
`remove_claimed_tree`. Both consume already-open capabilities or handles; neither
accepts an arbitrary absolute descendant path. Task 10 mutation-tests the portable
walker; native tests and security review cover platform deletion adapters.

Run aggregation, hard-link, symlink-swap, vanished-entry, depth/entry-cap, overflow, and
injected free-space boundary tests before runtime wiring.

- [ ] **Step 7: Move all public runtime scratch under two leased roots**

Add an `Arc<ManagedRunRoot>` with interior lifecycle state to
`WorkspaceHandler`/`WorkspacePlan`. Replace ambient
`tempdir()` calls in production snapshot and worker materialization with
`managed.tempdir("snapshot-")` and `managed.tempdir("worker-")`. Before snapshot
creation, compute `manifest.logical_bytes() * (1 + requested_workers)` with checked
arithmetic and reject `>= max_workspace_size`; use an injected seam to prove
`create_disk_snapshot` was not called. Do not move test-only
tempdirs or repository output. `handle_cleanup` and `close` must attempt every worker and
pending-worker cleanup even after one fails, drop the plan and snapshot, then explicitly
clean the run root when component-drain safety permits it. Aggregate ordered secondary
errors and return cleanup failure if absence verification fails; do not short-circuit
and strand later roots silently.

In `prepare_shell_setup_sync`, run the janitor, then create an execution root and a
delivery root with distinct owner kinds. Put snapshot, workers, process output, analyzer
candidate spool, and other pre-report scratch below the execution root. Construct
`PreparedReport` with a child of the delivery root. Register both with the meter. The
workspace cleanup effect removes only the execution root; `ReportHandler` retains the
delivery-root handle for Task 6 finalization. Add a test proving execution cleanup does
not remove the still-required JSON spool.

- [ ] **Step 8: Verify Task 5 behavior**

Run:

```bash
cargo test -p hoimin-cli --test disk_workspace -- --nocapture
cargo test -p hoimin-cli --test workspace_recovery -- --nocapture
cargo test -p hoimin-cli --test workspace_handler -- --nocapture
cargo test -p hoimin-cli workspace:: -- --nocapture
cargo fmt --all -- --check
cargo clippy -p hoimin-cli --all-targets --all-features -- -D warnings
git diff --check
```

- [ ] **Step 9: Commit**

```bash
git add Cargo.lock crates/hoimin-cli/Cargo.toml \
  crates/hoimin-cli/src/workspace/owned.rs \
  crates/hoimin-cli/src/workspace/disk.rs \
  crates/hoimin-cli/src/workspace/mod.rs \
  crates/hoimin-cli/src/workspace/copy.rs \
  crates/hoimin-cli/src/shell.rs \
  crates/hoimin-cli/src/report/mod.rs \
  crates/hoimin-cli/tests/disk_workspace.rs \
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
- Modify: `crates/hoimin-cli/src/resource/portable.rs`
- Modify: `crates/hoimin-cli/src/resource/mod.rs`
- Modify: `crates/hoimin-cli/src/resource/windows.rs`
- Modify: `crates/hoimin-cli/src/report/mod.rs`
- Modify: `crates/hoimin-cli/src/report/json.rs`
- Modify: `crates/hoimin-cli/tests/process_handler.rs`
- Modify: `crates/hoimin-cli/tests/report_handler.rs`
- Create: `crates/hoimin-cli/tests/disk_shutdown.rs`
- Create: `crates/hoimin-cli/tests/lean_disk_shutdown_oracle.rs`

**Interfaces:**

- Consumes: Tasks 1–5 corpus, policy, persisted limits, CLI config, leased root, and
  meter. `DiskStopRequested.source_effect` is `None` for monitor observations and the
  exact pending process effect for `SIGXFSZ`.
- Produces: `DiskMonitor::{start,sample_now,stop_and_join}`, global disk-stop machine
  transitions, post-drain/pre-classification sampling, `RLIMIT_FSIZE` attribution, and
  `ReportHandler` acknowledgement only after delivery-root cleanup, plus same-premise
  execution of every `runtime` record through the controlled shell adapter. Task 10
  treats this runtime as the only Rust mutation safety boundary.

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
    pub source_effect: Option<EffectId>,
    pub failure: DiskFailure,
}

pub enum RunEvent {
    // existing variants
    DiskStopRequested(DiskStopRequested),
}
```

Replace the process handler's bare close result at this integration boundary with:

```rust
pub struct ProcessDrainReport {
    pub all_reaped: bool,
    pub output_drains_joined: bool,
    pub secondary_errors: Vec<String>,
}
```

Only `all_reaped && output_drains_joined` permits execution-root removal. A false field
marks cleanup deferred and leaves the lease for the janitor; secondary errors after both
true do not prevent an attempted workspace cleanup.

Handle it through the same global-stop transition family as cancellation/deadline, but
preserve its typed code and observation in disk evidence. A monitor supplies
`source_effect=None`; the per-file backstop supplies the exact pending process effect ID
so the machine closes that completion before global shutdown. Synthetic unfinished
mutant events use `MutationStatus::NotRun`. A later `EffectFailed` becomes secondary and
cannot replace the disk primary reason.

- [ ] **Step 3: Implement synchronous and periodic monitor sampling**

After the machine accepts `StartRequested` but before the shell pops its first effect,
`DiskMonitor::start` performs one synchronous preflight sample. A threshold failure is
therefore an ordered run event while no analyzer, baseline, or mutant process
has launched. A failed or threshold sample is queued as `DiskStopRequested` immediately.
Otherwise one dedicated standard thread samples one scan at a time and waits until
`scan_started + 250ms`; filesystem traversal never blocks a Tokio runtime worker.

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
    source_effect: None,
    failure,
}));
```

Run the pre-dispatch and post-drain race fixtures. Require exact event ordering and
assert the normal cancellation/deadline path still uses the same shutdown loop.

- [ ] **Step 5: Wire monitor join, deferred cleanup, and final evidence**

Do not create a second shutdown loop. Before dispatching the existing `Cleanup` effect,
call the idempotent `disk_monitor.stop_and_join(shutdown_budget)`. If join completes,
continue to workspace cleanup. If it exceeds the shutdown budget, record secondary
`disk.measurement.failed`, call `managed_root.abandon_for_janitor`, mark cleanup failed,
and leave the monitor thread holding its root and lease handles until it exits rather
than racing recursive removal against a live scan. After process drain, take one final
pre-clean reading; after successful monitor join and cleanup, query each affected
filesystem parent once for end-free and reclaimed-byte evidence. Before dispatching the
final `EmitOutput(RunFinished(_))`, merge these observations into its disk field without
replacing machine-owned stop/cleanup state. Final evidence remains on the existing
ordered report path. Run blocked-join, cleanup-failure, end-free-query-failure, and
successful reclaimed-byte fixtures before adding the Unix backstop.

Do not short-circuit cleanup on a process resource-close error. Use
`ProcessDrainReport`: if process reap, output drains, and monitor join are confirmed,
attempt every execution cleanup and retain close errors as secondary; otherwise call
`abandon_for_janitor` and make no recursive removal attempt. Apply the same rule to the
shutdown-budget and detached-cleanup paths.

For `RunFinished`, make `ReportHandler` write and flush output, drop its JSON spool,
clean and absence-verify the delivery root, and only then return `OutputEmitted`. On
write failure it still attempts exact delivery-root cleanup and returns the report error
with cleanup failure secondary; on cleanup failure it returns typed
`workspace.cleanup.failed`. In both cases no `OutputEmitted` occurs and a session stays
incomplete. The emitted summary labels this root `cleanup_after_delivery` and does not
predeclare success. Add JSON, JSONL, and human-handler fixtures for success, write
failure, cleanup failure, and neighboring-sentinel preservation.

- [ ] **Step 6: Add Unix per-file `RLIMIT_FSIZE` without misclassification**

Add to `ProcessLimits`:

```rust
pub max_file_size_bytes: u64,
```

Populate it from `RunLimits.max_workspace_size`. In both macOS and other Unix
`configure_command` closures, apply soft and hard `RLIMIT_FSIZE` to this value after a
checked `rlim_t` conversion. Use a narrowly documented Clippy allow only where an ABI
identity conversion triggers `useless_conversion`.

Keep `ProcessTermination` unchanged. Change the internal exit decoder to return:

```rust
enum ExitClassification {
    Test(ProcessTermination),
    FileSizeLimit,
}
```

Map Unix `SIGXFSZ` to `FileSizeLimit`. After process-tree cleanup and output drain,
surface `workspace.size.exceeded` as a run-wide infrastructure stop; never pass
`128 + SIGXFSZ` to mutation status classification. Non-Unix retains normal exit
classification. Report enforcement as `portable_only` or
`hard_per_file:RLIMIT_FSIZE`; do not claim aggregate hard quota.

- [ ] **Step 7: Verify ordering and platform behavior**

```bash
cargo test -p hoimin-cli --test disk_shutdown -- --nocapture
cargo test -p hoimin-cli --test process_handler -- --nocapture
cargo test -p hoimin-cli --test lean_disk_shutdown_oracle -- --nocapture
cargo test -p hoimin-core --test machine -- --nocapture
cargo test -p hoimin-cli shell::tests:: -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

The SIGXFSZ test writes only KiB, confirms the file-size signal path, and asserts the
mutant count does not increase. On Windows, compile and run the portable sampler/lifecycle
tests; there is no RLIMIT claim.

- [ ] **Step 8: Commit**

```bash
git add crates/hoimin-core/src/event.rs crates/hoimin-core/src/machine.rs \
  crates/hoimin-core/src/model.rs crates/hoimin-core/tests/machine.rs \
  crates/hoimin-cli/src/workspace/disk.rs crates/hoimin-cli/src/shell.rs \
  crates/hoimin-cli/src/process/mod.rs crates/hoimin-cli/src/resource/mod.rs \
  crates/hoimin-cli/src/resource/portable.rs \
  crates/hoimin-cli/src/resource/windows.rs \
  crates/hoimin-cli/src/report/mod.rs crates/hoimin-cli/src/report/json.rs \
  crates/hoimin-cli/tests/process_handler.rs \
  crates/hoimin-cli/tests/report_handler.rs \
  crates/hoimin-cli/tests/disk_shutdown.rs \
  crates/hoimin-cli/tests/lean_disk_shutdown_oracle.rs
git commit -m "feat: stop and clean runs at disk safety limits"
```

### Task 7: Give the focused mutation workflow owned bounded scratch

**Files:**

- Create: `tools/focused_mutation_support/disk.py`
- Create: `tools/focused_mutation_support/lease.py`
- Modify: `tools/focused_mutation_support/discovery.py`
- Modify: `tools/focused_mutation_support/model.py`
- Modify: `tools/focused_mutation_support/mutation.py`
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
  binary spools, schema-2 `RunRecord`, and exact `--max-disk`, `--min-free-space`,
  `--jobs`, `--max-log-size`, `--scratch-root`, and `--keep-scratch` options. Task 8
  audits these public policy calls.

- [ ] **Step 1: Add parser, workflow, and bounded-drain RED tests**

Assert exact defaults:

```python
max_disk_bytes = 8 * 1024**3
min_free_bytes = 10 * 1024**3
jobs = 1
max_log_bytes = 16 * 1024**2
sample_interval_seconds = 0.250
```

Reject nonpositive bytes/jobs and reject a scratch root that is a file or cannot be
canonicalized/query free-space. A symlinked parent such as macOS `/tmp` is accepted only
after canonicalization; leased children must still be real directories. With fake
subprocesses/meters, cover `TMPDIR`/`TMP`/`TEMP` propagation, explicit `--jobs`,
size/reserve/meter stops, termination/reap before delete,
candidate outcome extraction before delete, cleanup on success/failure/timeout/interrupt,
and `--keep-scratch` with monitoring still active.

Require the output path to be absent or an empty real directory at startup. A non-empty
directory, symlink, or file must fail before scratch creation and child launch. Assert
the accepted output directory is included in pre-dispatch, periodic, post-drain, and
final pre-clean samples, but cleanup removes only leased scratch. Reject an output path
inside either implementation's canonical managed root.

For bounded logs, have a fake child write more than the allowance to stdout and stderr
concurrently. Assert it exits without pipe blockage, combined retained bytes never exceed
the limit, observed bytes remain exact, and deterministic head/tail slices are retained.

Run and require behavioral failures, not import/syntax failures:

```bash
uv run --frozen python -m unittest \
  tests.test_focused_mutation_disk \
  tests.test_focused_mutation_runner \
  tests.test_focused_mutation_reporting
```

- [ ] **Step 2: Implement the bounded Python policy and anchored meter**

Use language-native dataclasses mirroring the Lean observation/result names. On Unix,
open the root with `O_DIRECTORY | O_NOFOLLOW`, enumerate with `os.listdir(fd)`, obtain
`os.stat(name, dir_fd=fd, follow_symlinks=False)`, and open descendants with
`os.open(name, flags, dir_fd=fd)`; never reopen an accumulated descendant path. On
Windows, use a `ctypes` adapter that opens directory handles without delete sharing,
rejects reparse points, enumerates through the handle, and verifies final handle paths.
If either adapter cannot preserve anchored no-follow traversal, fail closed. Apply the
same depth-128 and 1,000,000-entry caps as Rust. Free space uses
`shutil.disk_usage(canonical_root).free` once per filesystem identity.
Stable file identity is `(st_dev, st_ino)` when both are nonzero; otherwise count each
entry and mark conservative.

Run policy boundary, filesystem grouping, hard-link fallback, symlink/reparse swap,
depth/entry cap, and measurement-failure tests before adding deletion.

- [ ] **Step 3: Implement the coordinator, lease, and anchored cleanup**

Implement the same managed direct-child protocol under
`tempfile.gettempdir()/hoimin-focused-v1`. Use `fcntl.flock(... LOCK_EX|LOCK_NB)` on Unix
and `msvcrt.locking` on Windows through a small `LeaseLock` adapter. The Windows adapter
uses `CreateFileW` with read/write/delete sharing, converts the handle with
`msvcrt.open_osfhandle`, seeks to byte zero, and locks exactly one existing marker byte
with `LK_NBLCK`; unlock uses the same byte and `LK_UNLCK`. The marker/retention
schema, direct-child validation, prefixes, 24-hour staging rule, and absence verification
must match the approved design; owner kind is `focused_python`. Marker reads are capped
at 64 KiB and managed-root enumeration at 100,000 direct children. Creation, cleanup
claim, and janitor claim acquire one coordinator lock. Rename active to deleting while
the lease and coordinator remain held, release only the coordinator, and keep the lease
locked through anchored removal and absence verification. Never use path-based
`shutil.rmtree` as a fallback.

Apply mode `0700` on Unix. On Windows, use `ctypes` with the same protected-DACL policy
as Rust: obtain the current token user SID, build a DACL for that SID plus `SYSTEM` and
`Administrators`, apply it, and read it back. A DACL API failure is a measurement/setup
failure; it never falls back to a broadly writable directory.

Run creation, active/abandoned/retained/staging, malformed-marker, contention, DACL/mode,
and cleanup-absence tests. Each destructive test must assert the exact allowed root and
the preservation of a neighboring sentinel.

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

The wait loop samples every 250ms. On disk stop it records the first code, terminates and
reaps the process tree using the existing runner lifecycle, joins drain threads, and only
then returns/raises. Cleanup failures append to `cleanup_errors` and do not replace the
primary state.

Take a synchronous sample immediately before launch and another after process reap plus
drain join but before result classification. The disk decision wins if it is ready with
process completion. A meter join timeout marks cleanup deferred, leaves the lease/root
for the next janitor, and disables destructor cleanup rather than deleting under a live
scan. A process-reap or output-drain join timeout has the same deferred-cleanup rule.

- [ ] **Step 5: Own cargo-mutants output one candidate at a time**

Extend `Options` and parser with `--max-disk`, `--min-free-space`, `--jobs`,
`--max-log-size`, `--scratch-root`, and `--keep-scratch`. Use the existing byte/duration
parsing style and add byte parsing in one named helper.

Create one leased run root before cargo-mutants discovery. Set `TMPDIR`, `TMP`, `TEMP`,
an absolute `CARGO_TARGET_DIR` below the leased root, and `CARGO_INCREMENTAL=0` for
inventory, baseline, and mutation commands; leave the shared Cargo home untouched. When
the user supplies any
`--file` or `--symbol`, discovery must not append unrelated recent files merely to reach
ten candidates. Add an exact-discovery regression test. Change:

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

Extend `RunState` with `DISK_LIMIT` and bump focused `SCHEMA_VERSION` from 1 to 2. Store
disk policy, observations, enforcement, stale cleanup, retained root, and cleanup results
plus the effective `jobs` value in `RunRecord`. `RunStore` output remains user-owned and is counted/monitored but never
deleted. `--keep-scratch` creates the separate retention marker and prints the exact
quoted deletion path.

Return zero only for `COMPLETED`. Preserve 130 for interruption; return 3 for
`BUDGET_EXHAUSTED` and 2 for other non-completed states, including `DISK_LIMIT` and
report failure. Add exact exit-code tests so a partial inventory cannot satisfy a
fail-closed shell gate.

Run exact discovery, command construction, sequential candidate compaction, and
candidate-directory absence tests before outer finalization.

- [ ] **Step 6: Implement catchable finalization and guarded report writes**

Restructure the outer workflow finalization so the order is fixed: close dispatch,
terminate/reap an active command, join both output drains, extract the current compact
outcome, take the final pre-clean sample, stop/join the meter, clean or retain scratch,
query end free space, update cleanup evidence, then checkpoint `run.json` and render
Markdown. A report write error changes only the report outcome and does not erase disk
or cleanup evidence already held in memory.

For each checkpoint or final Markdown write, stream into a uniquely named
wrapper-created atomic temporary through a writer that splits encoder output into at
most 64 KiB chunks. Before each chunk, take a fresh output-root reading and check the
projected temporary plus still-present destination against `max_disk` and
`min_free_space`. Refuse at an inclusive boundary, close and unlink only that exact
temporary, and keep the prior destination intact. Tests inject equal/below/above values
and require report-delivery failure through stderr/status without unbounded buffering.

- [ ] **Step 7: Run focused Python tests and commit**

```bash
uv run --frozen python -m unittest discover -s tests -p 'test_focused_mutation*.py'
uv run --frozen python -m unittest tests.test_skills
uv run --frozen mypy tools/focused_mutation.py tools/focused_mutation_support
git diff --check
git add tools/focused_mutation.py tools/focused_mutation_support \
  tests/test_focused_mutation_disk.py tests/test_focused_mutation_discovery.py \
  tests/test_focused_mutation_runner.py \
  tests/test_focused_mutation_reporting.py tests/test_focused_mutation_budget.py
git commit -m "feat: bound and clean focused mutation scratch"
```

### Task 8: Add Python correspondence to the Lean oracle

**Files:**

- Create: `tests/test_focused_mutation_disk_oracle.py`
- Modify: `tools/focused_mutation_support/disk.py`
- Modify: `tests/test_focused_mutation_docs.py`

**Interfaces:**

- Consumes: Task 1 schema-1 corpus and Task 7 public Python policy/lifecycle calls.
- Produces: a strict parser, per-mode match/mismatch/infrastructure results, and broken
  threshold, precedence, dispatch, and cleanup witnesses. It executes every `policy`
  record through public policy calls and every `runtime` record through the controlled
  runner. No adapter reads private fields to force correspondence.

- [ ] **Step 1: Write the real-adapter corpus test**

Load `formal/HoiminOracle/corpus/disk-guard-lifecycle.jsonl` with strict key validation.
Map each event to the public Python disk-policy/lifecycle entry points; do not duplicate
the expected transition function in the test. Classify every case as match, mismatch, or
infrastructure error, require zero reviewed mismatches, and retain deliberately broken
threshold/stickiness/cleanup variants that each fail at least one corpus case.

- [ ] **Step 2: Run and commit**

```bash
uv run --frozen python -m unittest tests.test_focused_mutation_disk_oracle -v
uv run --frozen python -m unittest tests.test_focused_mutation_docs -v
git diff --check
git add tests/test_focused_mutation_disk_oracle.py \
  tools/focused_mutation_support/disk.py tests/test_focused_mutation_docs.py
git commit -m "test: audit Python disk guard correspondence"
```

### Task 9: Document safe mutation operation and compatibility

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`
- Modify: `crates/hoimin-cli/tests/cli_config.rs`
- Modify: `tests/test_focused_mutation_docs.py`

**Interfaces:**

- Consumes: the final CLI and wrapper options from Tasks 4 and 7.
- Produces: user guidance and executable documentation assertions for defaults,
  enforcement limits, cleanup, retention, and safe one-worker invocation.

- [ ] **Step 1: Add documentation contract RED tests**

Require README/development docs to contain the exact public defaults, generated-workspace
versus copy-size distinction, mandatory reserve, 250ms reaction-window disclaimer,
single-worker focused default, cleanup/retention/stale-recovery behavior, capability
labels, and an exact safe wrapper example. Require no routine example matching:

```text
cargo mutants --workspace --jobs [2-9]
```

- [ ] **Step 2: Update docs**

Document these commands:

```bash
hoimin run --max-workspace-size 8GiB --min-free-space 10GiB -- python -m pytest
mutation_output="$(mktemp -d /tmp/hoimin-focused.XXXXXX)"
uv run --frozen python tools/focused_mutation.py \
  --budget 30m --jobs 1 --max-disk 8GiB --min-free-space 10GiB \
  --max-log-size 16MiB --output "$mutation_output"
```

Explain that raising a limit is explicit risk acceptance; monitoring cannot prevent one
child from consuming the reserve inside one sample interval; `RLIMIT_FSIZE` is per-file;
and only a verified named quota backend is aggregate hard enforcement. Include how to
remove a retained exact scratch path without suggesting wildcard deletion. State that
the output directory must start absent or empty, is monitored but preserved, and should
be a fresh `mktemp -d` path as in the example.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p hoimin-cli --test cli_config -- --nocapture
uv run --frozen python -m unittest tests.test_focused_mutation_docs -v
rg -n "cargo mutants --workspace --jobs [2-9]" README.md docs/development.md
git diff --check
git add README.md docs/development.md crates/hoimin-cli/tests/cli_config.rs \
  tests/test_focused_mutation_docs.py
git commit -m "docs: explain disk-safe mutation execution"
```

The `rg` command must exit 1 with no matches; treat any match as RED.

### Task 10: Final verification, restrained mutation, review, and PR

**Files:**

- Modify only if an exact equivalent mutant is proved:
  `.cargo/mutants.toml` with a fully anchored `exclude_re` and adjacent TOML reason
- Create ignored evidence under:
  `.superpowers/sdd/2026-08-27-disk-safe-mutation-execution/`

**Interfaces:**

- Consumes: every tracked deliverable and its focused test entry point.
- Produces: final-SHA native/compatibility/formal/mutation evidence, three independent
  same-SHA reviews, a pushed branch, and a PR whose remote head equals the reviewed SHA.

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
check_disk() { uv run --frozen python -c 'import shutil, sys; sys.exit(0 if shutil.disk_usage(".").free > 10 * 1024**3 else 1)'; }
check_disk
cargo fmt --all -- --check
check_disk
cargo clippy --workspace --all-targets --all-features -- -D warnings
check_disk
cargo test --workspace
check_disk
cargo test -p hoimin-cli --test run_e2e
check_disk
uv run --frozen python -m unittest discover -s tests
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
if rg -n '(^| )(tokio|rusqlite|tempfile|windows-sys|libc|hoimin-cli)( |$)' "$core_tree"; then exit 1; fi
check_disk
uvx maturin build --release
check_disk
uv run --frozen python tests/wheel_smoke.py
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

- [ ] **Step 4: Run only the guarded focused mutation slice**

Preflight free space and ensure no concurrent cargo-mutants process. Run through the new
wrapper, not raw full-workspace cargo-mutants:

```bash
set -e
candidate_sha="$(cat .superpowers/sdd/2026-08-27-disk-safe-mutation-execution/candidate-sha.txt)"
test "$(git rev-parse HEAD)" = "$candidate_sha"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
if ps -axo pid,etime,command | rg '[c]argo mutants'; then exit 1; fi
mutation_output="$(mktemp -d "$PWD/.superpowers/sdd/2026-08-27-disk-safe-mutation-execution/focused.XXXXXX")"
uv run --frozen python tools/focused_mutation.py \
  --budget 30m \
  --jobs 1 \
  --max-disk 8GiB \
  --min-free-space 10GiB \
  --max-log-size 16MiB \
  --symbol evaluate_disk_policy \
  --symbol apply_disk_lifecycle_event \
  --symbol measure_owned_tree \
  --symbol claim_managed_child \
  --output "$mutation_output"
uv run --frozen python -c 'import json, pathlib, sys; p=pathlib.Path(sys.argv[1]); d=json.loads((p / "run.json").read_text()); assert d["schema_version"] == 2; assert d["state"] == "completed"; assert d["jobs"] == 1; assert 0 < len(d["candidates"]) <= 80; assert all(c["state"] not in {"pending", "not_run", "timeout", "error", "survived"} for c in d["candidates"])' "$mutation_output"
test "$(git rev-parse HEAD)" = "$candidate_sha"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
```

The four function names above are required unique, platform-portable implementation
seams from Tasks 2 and 5; add an exact discovery test and stop if any resolves to zero
or multiple functions.
Acceptance requires no more than 80 inventoried mutants, exact cargo-mutants 27.1.0;
one mutation worker; no disk/budget/timeout/
tool/error outcome; every applicable focused mutant caught; scratch cleanup verified;
retained evidence bounded; disk remained above reserve. A platform-inapplicable mutant
must be mapped by exact name and cfg. Do not run the full 3,000+ mutant inventory.

If a non-equivalent outside-scope survivor appears, prove whether it is pre-existing with
an exact single-worker same-host `origin/main` narrow comparison and stop for scope
direction. If an exact equivalent mutant is independently reviewed, add a fully anchored
exclusion plus reason to `.cargo/mutants.toml`, commit it, and repeat a fresh focused run.

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

- [ ] **Step 6: Push, create PR, and watch GitHub Actions gracefully**

Push only after all same-SHA gates/reviews pass:

```bash
git push -u origin feat/disk-safe-mutation
gh pr create --base main --head feat/disk-safe-mutation \
  --title "Prevent mutation runs from exhausting disk" \
  --body-file /tmp/hoimin-disk-safe-pr-body.md
```

The PR body links the approved design, states defaults/limits, enumerates native evidence,
explains that no full-workspace mutation was run, and records exact cleanup outcomes.
Verify remote head SHA once, then watch actual checks at 30-second intervals. Do not poll
GitHub Actions every 10 seconds:

```bash
set -e
candidate_sha="$(cat .superpowers/sdd/2026-08-27-disk-safe-mutation-execution/candidate-sha.txt)"
remote_sha="$(gh pr view --json headRefOid --jq .headRefOid)"
test "$remote_sha" = "$candidate_sha"
gh pr checks --watch --interval 30
```

Require the complete PR job set from `.github/workflows/ci.yml`: Quality on Ubuntu,
Windows, and macOS; Rust MSRV; Rust on Ubuntu, Windows, and macOS; randomized Rust;
contracts; Core dependency purity on Ubuntu and Windows; Wheel smoke on Ubuntu,
Windows, and macOS; and linux-best-effort. `linux-cgroup-v2-hard` is push-to-main-only
and must be recorded as absent by workflow design, not passed or failed on the PR. If CI
changes code, repeat local gates, reviews, push, remote SHA guard, and checks. Do not
merge without separate user or maintainer authorization.

## Completion checklist

- [ ] Defaults are 8 GiB owned bytes, 10 GiB reserve, 250ms, jobs 1, logs 16 MiB.
- [ ] Plan/resume/report schemas reject old incomplete artifacts explicitly.
- [ ] Disk stops are infrastructure failures and never improve mutation score.
- [ ] Processes are terminated/reaped and output drained before cleanup.
- [ ] Monitor stops/joins before one logical cleanup request per owned root.
- [ ] Cleanup verifies absence; runtime retains primary, execution-cleanup, and delivery
  outcomes separately. A delivered report contains run/execution cleanup plus an honest
  post-delivery cleanup marker; failed delivery finalization is surfaced through typed
  error, stderr, and incomplete session state.
- [ ] Live/retained/malformed/symlink/foreign roots are never deleted.
- [ ] Abandoned valid leases are reclaimed safely on the next start.
- [ ] Focused candidate trees are compacted then deleted after each result.
- [ ] Combined stdout/stderr retained bytes obey the configured command cap.
- [ ] Lean proofs, sensitivity, freshness, Rust adapter, and Python adapter pass.
- [ ] macOS/Linux native evidence is honest; Windows CI lifecycle evidence passes.
- [ ] Only guarded, one-worker focused mutation evidence is used.
- [ ] Final SHA is clean, independently reviewed, pushed, and equal to the PR head.
- [ ] GitHub checks are watched at 30-second intervals and the exact PR job set passes;
  push-to-main-only jobs are identified as not applicable.
