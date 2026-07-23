# Coverage Matrix

| Area | Module | Static scan | Invariant trace | Dynamic evidence | Platform | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| analysis-output | `crates/hoimin-cli/src/analyzer/mod.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/protocol.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/rust.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/rust_tests.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/analyzer/store.rs` | complete | pending | pending | pending | pending | |
| delivery | `crates/hoimin-cli/src/cli.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/fingerprint_inputs.rs` | complete | pending | pending | pending | pending | |
| delivery | `crates/hoimin-cli/src/lib.rs` | complete | pending | pending | pending | pending | |
| delivery | `crates/hoimin-cli/src/main.rs` | complete | pending | pending | pending | pending | |
| orchestration | `crates/hoimin-cli/src/metrics.rs` | complete | complete | complete | portable | complete | Stage and worker lifecycle traced; maps are bounded by fixed stage names and configured workers, and `finish` rejects outstanding process state. |
| persistence | `crates/hoimin-cli/src/plan.rs` | complete | pending | pending | pending | pending | |
| orchestration | `crates/hoimin-cli/src/process/mod.rs` | complete | complete | complete | portable | complete | Prepare/spawn/attach/select/terminate/wait/classify/output traced. Successful cancellation and timeout reap descendants; RUST-003 records the terminate-error branch that skips explicit root reap. |
| orchestration | `crates/hoimin-cli/src/process/output.rs` | complete | complete | complete | portable | complete | Two 8 KiB readers feed an eight-chunk bounded channel; retained bytes are capped and the collector drains to EOF even after spool failure. |
| analysis-output | `crates/hoimin-cli/src/progress/compare.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/progress/input.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/progress/mod.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/progress/render.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/human.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/json.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/jsonl.rs` | complete | pending | pending | pending | pending | |
| analysis-output | `crates/hoimin-cli/src/report/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/resource/linux.rs` | complete | pending | limited | Linux cgroup | limited | Delegated Linux cgroup execution was not run on the macOS audit host. |
| isolation | `crates/hoimin-cli/src/resource/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/resource/portable.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/resource/windows.rs` | complete | pending | limited | Windows | limited | Windows execution was not run on the macOS audit host. |
| persistence | `crates/hoimin-cli/src/session/mod.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/session/schema.rs` | complete | pending | pending | pending | pending | |
| orchestration | `crates/hoimin-cli/src/shell.rs` | complete | complete | complete | portable | complete | All effects, process completion, cancellation, drain, close/error precedence, metrics, and four proposed extraction boundaries traced; process/run E2E suites pass. |
| persistence | `crates/hoimin-cli/src/target/fs.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/target/git.rs` | complete | pending | pending | pending | pending | |
| persistence | `crates/hoimin-cli/src/target/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/copy.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/manifest.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/mod.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/mutation.rs` | complete | pending | pending | pending | pending | |
| isolation | `crates/hoimin-cli/src/workspace/reset.rs` | complete | pending | pending | pending | pending | |
| core | `crates/hoimin-core/src/budget.rs` | complete | complete | complete | portable | complete | Reservation/grant/release traced; contracts and budget policy tests pass. RUST-002 records the unchecked reservation-ID boundary. |
| core | `crates/hoimin-core/src/candidate.rs` | complete | complete | complete | portable | complete | Hash/path/span/location validation traced; candidate and machine policy tests pass. |
| core | `crates/hoimin-core/src/config.rs` | complete | complete | complete | portable | complete | Raw limits, duration arithmetic, selector/operator normalization, and resume preconditions traced. |
| core | `crates/hoimin-core/src/contracts.rs` | complete | complete | complete | portable | complete | Contract feature evidence exercises stable invariant IDs. |
| core | `crates/hoimin-core/src/effect.rs` | complete | complete | complete | portable | complete | Every effect is mapped to its completion kind, worker, and registered ID. |
| core | `crates/hoimin-core/src/event.rs` | complete | complete | complete | portable | complete | Completion and typed failure payloads traced through machine acceptance. |
| core | `crates/hoimin-core/src/lib.rs` | complete | complete | complete | portable | complete | Public core surface reviewed with all module policy suites. |
| core | `crates/hoimin-core/src/machine.rs` | complete | complete | complete | portable | complete | Terminal candidate, failure, cleanup-once, report, and bounded-ledger paths traced against 33 machine tests. |
| core | `crates/hoimin-core/src/model.rs` | complete | complete | complete | portable | complete | Numeric protocol fields traced through validation, persistence, reporting, and fingerprint encoding. |
| core | `crates/hoimin-core/src/report.rs` | complete | complete | complete | portable | complete | Score denominator, exit precedence, event identity/order, and report summary traced. |
| core | `crates/hoimin-core/src/resume.rs` | complete | complete | complete | portable | complete | Compatibility fields, canonical set ordering, native argv units, and reuse policy traced. |
| core | `crates/hoimin-core/src/target.rs` | complete | complete | complete | portable | complete | Root/path checks and file/line/symbol/changed union/intersection normalization traced. |
| core | `crates/hoimin-core/src/telemetry.rs` | complete | complete | complete | portable | complete | Metric duration and worker-accounting invariants covered by focused core contracts evidence. |

## Static-scan triage notes

The six raw logs in `.audit/rust-codebase/scans/` contain 4,332 broad matches. Matches
under `crates/*/tests/`, the entirety of `analyzer/rust_tests.rs`, and matches inside
`#[cfg(test)]` modules were classified as test-only before reviewing production code.
The broad index pattern intentionally also matched attributes, slice types, and array
literals; those syntactic false positives were rejected rather than treated as risk by
count.

- `analyzer/{mod,protocol,rust,store}.rs`: parser offsets and slices originate from
  validated Ruff ranges or checked spool records; store `expect`s follow immediate
  initialization. Saturating/default conversions are explicit bounds policy.
  `analyzer/rust_tests.rs` is test-only.
- `cli.rs`, `lib.rs`, and `main.rs`: range defaults and nonzero defaults are
  construction invariants. Best-effort writes occur only while reporting an already
  selected CLI error; a failed Tokio runtime build cannot enter application error
  handling.
- `fingerprint_inputs.rs`, `plan.rs`, and `target/{fs,git,mod}.rs`: symlinks and
  root-relative paths are rejected, candidate lookups are preceded by validation, and
  fallbacks preserve lexical/user-facing values. No unchecked persistence operation
  remains.
- `metrics.rs`: temporary-file write, flush, sync, and persist failures propagate.
  Time conversion deliberately saturates; `Drop` restores only test process state.
- `process/{mod,output}.rs`: spawn, attach, wait, kill, join, and spool failures are
  surfaced or combined on ordinary paths. RUST-003 records the cancellation/timeout
  exception where a supervisor termination error skips the explicit root wait. The
  ignored second `start_kill` is a best-effort fallback after a bounded wait.
- `progress/{compare,input,mod,render}.rs`: indexing is through validated collections
  or map entry APIs. The `unreachable!` arm follows the local two-input comparison
  state invariant.
- `report/{human,json,jsonl,mod}.rs`: serialization and write errors are converted to
  `EffectFailed`; temporary JSON storage is owned and checked.
- `resource/mod.rs` and `resource/portable.rs`: unsafe blocks are narrow FFI calls with
  ownership/lifetime comments; termination errors propagate from the explicit path.
  The Windows attach race remains as `RUST-001`.
- `resource/linux.rs`: cgroup paths are canonicalized and subtree walks reject
  symlinks. Kill/reap/cleanup errors are retained in retryable pending-cleanup values;
  ignored operations occur only in `Drop` fallback paths. Linux-only execution remains
  a platform evidence gap, not an additional static lead.
- `resource/windows.rs`: Win32 handles use an owning wrapper and checked API results;
  integer conversions are bounded by API constants/layout sizes. The platform was not
  executable on this host; the pre-assignment race is recorded as `RUST-001`.
- `session/{mod,schema}.rs`: state transitions use transactions and commit errors
  propagate; migrations advance `user_version` inside the same transaction. Schema
  `unwrap`s are confined to tests.
- `shell.rs`: task joins, cancellation, deadline, cleanup, and close failures are
  drained or combined. Completion sends are ignored only after receiver shutdown, and
  metrics diagnostics are explicitly best-effort so they do not replace the run result.
- `workspace/{copy,manifest,mod,mutation,reset}.rs`: canonical roots, component-wise
  path validation, symlink rejection, snapshots, and post-reset comparison bound file
  access. Cleanup failures remain retryable and prevent accounting release; defaults
  represent unavailable advisory metadata or absent environment values.
- `hoimin-core` modules: matches are declarative attributes/slice types or checked,
  saturating conversions. `machine.rs` collection indexing follows registered-effect
  and worker-state checks; `resume.rs` casts encode platform-bounded lengths into the
  stable fingerprint format; `target.rs` fallbacks implement normalization policy.
  No unsafe block or cleanup side effect exists in core production code.

## Core state and policy audit

The Task 4 call/effect map is recorded in
`.audit/rust-codebase/core/invariants.md`. The trace covers configuration and
target normalization, candidate validation, effect/completion registration,
terminal candidate drain, cleanup-once behavior, report construction and exit
precedence, workspace budget grant/release, fingerprint compatibility, and
stored-result replacement through the CLI session caller.

Task 3 retained no core lead: its sole retained item is the CLI isolation lead
`RUST-001`. Task 4 added `RUST-002` for the reservation-ID exhaustion boundary;
the current machine reserves one copy grant per run, but the public budget
ledger can reuse `ReservationId(u64::MAX)` and replace an active entry.

## CLI orchestration and process-lifecycle audit

Task 5 traced each `RunEffect` from acquisition through its completion event.
Target resolution retains the resolved target vector for fingerprinting; preflight,
worker creation, mutation/reset, verification, and cleanup acquire workspace state
released by `Cleanup` and the unconditional outer `workspace.close`. Candidate replay
retains at most one active candidate per configured worker and removes it on EOF or
successful reset. Session effects lazily acquire one handler, whose transactional
operations are completed by `FinishSession` or connection drop. Output emission owns
no external resource, and a report write error becomes `EffectFailed`, is accepted
before any newly produced effects are queued, cancels processes, and drains the
process set. Process effects acquire a worker environment, dispatch/start gates,
supervisor attachment, pipe tasks, a spool, and one completion slot; the normal,
cancellation, timeout, and attach-failure paths release these obligations, subject
to RUST-003 on supervisor-termination failure.

The run loop preserves cleanup visibility in two layers. The state machine turns
effect failures into cleanup/report/session effects, while the outer close always
attempts both workspace and process backend close. `combine_close_results` keeps a
run infrastructure error first and appends workspace then process-close failures.
Metrics observation errors and atomic-write errors remain warning diagnostics and
do not replace the run result. Accepted phase transitions finish the departed
metrics stage, including direct early cleanup; process drain closes worker timing
state before `MetricsCollector::finish`.

Boundedness was checked collection by collection:

- `JoinSet` and in-flight completions are bounded by `jobs` and `jobs + 1`;
  `joinset_and_completion_queue_stay_bounded_across_many_mutants` observes those
  bounds, and every stop/failure path joins the set then drains the channel.
- Each process has two 8 KiB read buffers and an eight-element pipe channel.
  Spool retention is bounded by `max_output_bytes`; observed output is streamed and
  counted with saturation rather than retained in memory.
- `active_candidates` and metrics worker state are bounded by configured workers.
  Metrics stages are the fixed run-phase set. Resolved targets and candidate/report
  spools scale with selected project input and are lifecycle-owned by the temporary
  run directory rather than accumulated across runs.
- The effect deque and core pending/completion collections are drained by accepted
  transitions and bounded by the core worker/effect protocol; queued process metrics
  are cancelled when effects are discarded. `metrics_warnings` is run-lifetime
  retained and has no explicit numeric cap, but additions require a metrics invariant
  or write failure rather than ordinary candidate throughput, so it is not retained
  as a production lead.

Focused evidence is
`.audit/rust-codebase/orchestration-tests.log`: 50 tests passed (17
`process_handler`, 33 `run_e2e`) on the macOS portable backend. Delegated Linux
cgroup and Windows Job Object behavior remain platform-limited evidence. RUST-001
continues to own the Windows pre-attach isolation race; Task 5 created no duplicate
lead. RUST-003 instead covers post-attach terminate-error cleanup and reap.
