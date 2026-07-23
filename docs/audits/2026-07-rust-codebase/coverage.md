# Coverage Matrix

| Area | Module | Static scan | Invariant trace | Dynamic evidence | Platform | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| analysis-output | `crates/hoimin-cli/src/analyzer/mod.rs` | complete | complete | complete | portable | complete | Source read/UTF-8 failure, per-target remaining limits, stable global sequence assignment, candidate validation, cancellation, and final spool handoff traced; focused analyzer suites pass. |
| analysis-output | `crates/hoimin-cli/src/analyzer/protocol.rs` | complete | complete | complete | portable | complete | Bounded JSONL lines/output, exact record fields and kinds, effect IDs, source ordering, diagnostic/limit state, summary counts, and terminal framing traced and tested. |
| analysis-output | `crates/hoimin-cli/src/analyzer/rust.rs` | complete | complete | complete | portable | complete | Ruff byte ranges, Unicode code-point columns, nested scopes, annotation resolution, selection/operator/profile ordering, deduplication, sorting, syntax diagnostics, and post-filter candidate limits traced; property and focused tests pass. |
| analysis-output | `crates/hoimin-cli/src/analyzer/rust_tests.rs` | complete | complete | complete | test-only | complete | Test-only module supplies syntax, selection, annotation, Unicode, scope, limit, ordering, and arbitrary-input dynamic evidence for `rust.rs`. |
| analysis-output | `crates/hoimin-cli/src/analyzer/store.rs` | complete | complete | complete | portable | complete | JSONL record-size/count/sequence bounds, flush/sync/preserve, boundary-aligned offsets, bounded reverse sequence lookup, corruption/truncation detection, and EOF record-count validation traced and tested. |
| delivery | `crates/hoimin-cli/src/cli.rs` | complete | complete | complete | portable | complete | Clap subcommands, selector/resource/output defaults, native argv boundary, plan/verify restrictions, progress arity/patience, numeric conversion, and README examples traced; CLI configuration tests pass. |
| persistence | `crates/hoimin-cli/src/fingerprint_inputs.rs` | complete | complete | complete | portable | complete | Exact and glob inputs, ignored-file policy, sorting/deduplication, path normalization, read failures, regular-file checks, and non-followed symlinks traced; focused input tests pass. |
| delivery | `crates/hoimin-cli/src/lib.rs` | complete | complete | complete | portable | complete | Parse/config/prepare/run dispatch, stdout/stderr separation, plan JSON, report failures, and exit-code mapping traced; CLI and delivery suites pass. |
| delivery | `crates/hoimin-cli/src/main.rs` | complete | complete | complete | portable/Linux launcher | complete | Linux launcher interception, Tokio runtime construction, async CLI dispatch, and process exit propagation traced; cross-platform CI and wheel smoke exercise the binary entry point. |
| orchestration | `crates/hoimin-cli/src/metrics.rs` | complete | complete | complete | portable | complete | Stage and worker lifecycle traced; maps are bounded by fixed stage names and configured workers, and `finish` rejects outstanding process state. |
| persistence | `crates/hoimin-cli/src/plan.rs` | complete | complete | complete | portable | complete | Manifest header/record/candidate validation, source and fingerprint revalidation, requested-ID normalization, rediscovery, baseline handoff, and selected replay traced. RUST-005 records normalized-config invariants bypassed by direct deserialization. |
| orchestration | `crates/hoimin-cli/src/process/mod.rs` | complete | complete | complete | portable | complete | Prepare/spawn/attach/select/terminate/wait/classify/output traced. Successful cancellation and timeout reap descendants; RUST-003 records the terminate-error branch that skips explicit root reap. |
| orchestration | `crates/hoimin-cli/src/process/output.rs` | complete | complete | complete | portable | complete | Two 8 KiB readers feed an eight-chunk bounded channel; retained bytes are capped and the collector drains to EOF even after spool failure. |
| analysis-output | `crates/hoimin-cli/src/progress/compare.rs` | complete | complete | complete | portable | complete | Semantic-key indexing, ambiguity, conclusive transitions, scores, added/removed candidates, stall accumulation, and saturation were traced; existing tests cover the current added/removed behavior. The exact candidate-ID set eligibility contract and its missing enforcement were traced, while the absent eligibility-boundary test remains part of the RUST-007 gap. RUST-008 records the intentional stall-retention design's ambiguity with the public “consecutive stalls” contract. |
| analysis-output | `crates/hoimin-cli/src/progress/input.rs` | complete | complete | complete | portable | complete | Deny-unknown JSON documents, nested schema versions, exact event positions, run identity, strict sequence order, baseline success, and summary completeness traced and tested. |
| analysis-output | `crates/hoimin-cli/src/progress/mod.rs` | complete | complete | complete | portable | complete | Ordered read/compare/render dispatch and exit-2 error handoff traced; progress CLI integration tests pass. |
| analysis-output | `crates/hoimin-cli/src/progress/render.rs` | complete | complete | complete | portable | complete | Versioned JSON decision fields, human output, unusable/ambiguity diagnostics, serialization/write failures, and schema validation traced and tested. |
| analysis-output | `crates/hoimin-cli/src/report/human.rs` | complete | complete | complete | portable | complete | All six lifecycle event variants, normalized configuration details, mutation statuses, diagnostics, and flush failures traced; report handler tests pass. |
| analysis-output | `crates/hoimin-cli/src/report/json.rs` | complete | complete | complete | portable | complete | Run/baseline retention, disk-backed mutant streaming, ignored non-document events, exact document framing/version, terminal summary, flush, and partial-write poisoning traced; heap and failure-injection tests pass. |
| analysis-output | `crates/hoimin-cli/src/report/jsonl.rs` | complete | complete | complete | portable | complete | One serialized event per line with immediate flush and typed serialization/I/O propagation traced and tested. |
| analysis-output | `crates/hoimin-cli/src/report/mod.rs` | complete | complete | complete | portable | complete | Format routing, diagnostic stderr separation, JSON spool ownership, exact event forwarding, and report failure mapping traced; report suites pass. |
| isolation | `crates/hoimin-cli/src/resource/linux.rs` | complete | complete | limited | Linux cgroup | limited | Probe, stopped-launcher attach, run/root accounting, classification, recursive termination, and retryable cleanup traced statically. Delegated cgroup v2 execution was not run on the macOS audit host. |
| isolation | `crates/hoimin-cli/src/resource/mod.rs` | complete | complete | complete | portable | complete | Shared prepare/attach/terminate/classify/close dispatch traced; platform-hard variants retain their platform-limited evidence status. |
| isolation | `crates/hoimin-cli/src/resource/portable.rs` | complete | complete | complete | macOS portable | complete | Best-effort mode, pre-exec process group/CPU limit, descendant termination, and no-op backend close traced; 14 portable process tests passed. Windows portable attach remains owned by RUST-001. |
| isolation | `crates/hoimin-cli/src/resource/windows.rs` | complete | complete | limited | Windows Job Object | limited | Suspended hard-backend attach, run/root Job Objects, notification classification, termination, and retryable close traced statically. Windows execution was not run on the macOS audit host. |
| persistence | `crates/hoimin-cli/src/session/mod.rs` | complete | complete | complete | portable | complete | Begin/persist/replace/finish transactions, rollback, finality, run-scoped row shape, resume selection, and caller dispatch traced. The two-statement lookup/finalize interleaving was reviewed and rejected as RUST-006 because lookup can linearize before completion and finish does not mutate stored rows. |
| persistence | `crates/hoimin-cli/src/session/schema.rs` | complete | complete | complete | SQLite/WAL | complete | Five-second busy bound, foreign keys, WAL setup, atomic versioned migrations, failed-upgrade rollback, data preservation, and idempotent reopen traced and tested. |
| orchestration | `crates/hoimin-cli/src/shell.rs` | complete | complete | complete | portable | complete | All effects, process completion, cancellation, drain, close/error precedence, metrics, and four proposed extraction boundaries traced; process/run E2E suites pass. |
| persistence | `crates/hoimin-cli/src/target/fs.rs` | complete | complete | complete | portable | complete | Default ignore behavior, explicit include restoration, exclude precedence, regular Python files, non-UTF-8 rejection, non-followed symlinks, and root-relative discovery traced. |
| persistence | `crates/hoimin-cli/src/target/git.rs` | complete | complete | complete | Git | complete | HEAD/merge-base diffs, staged/unstaged/untracked and unborn repositories, deletion/binary exclusion, UTF-8 path handling, rename detection, hostile revisions/config, and pinned diff format traced and tested. |
| persistence | `crates/hoimin-cli/src/target/mod.rs` | complete | complete | complete | portable/Git | complete | Explicit normalization and changed-line intersection preserve core target invariants; empty explicit selections and effect-ID/error mapping traced. |
| isolation | `crates/hoimin-cli/src/workspace/copy.rs` | complete | complete | complete | portable | complete | Preflight identity, aggregate allowance binding, partial-copy charge rollback, original recheck, and worker-slot rollback traced. |
| isolation | `crates/hoimin-cli/src/workspace/manifest.rs` | complete | complete | complete | portable | complete | Canonical-root manifest discovery, normalized relative entries, content hashes, exclusions, and non-followed symlink diagnostics traced. |
| isolation | `crates/hoimin-cli/src/workspace/mod.rs` | complete | complete | complete | portable | complete | Handler lifecycle, reservation identity, retryable cleanup, read-only tree removal, and drop accounting traced. RUST-004 records an actor-conditional workspace-integrity race after path validation. |
| isolation | `crates/hoimin-cli/src/workspace/mutation.rs` | complete | complete | complete | portable | complete | Original integrity, manifest/hash/span/original-byte checks, writable conversion, and mutation postcondition traced; path operation remains subject to RUST-004. |
| isolation | `crates/hoimin-cli/src/workspace/reset.rs` | complete | complete | complete | portable | complete | Original recheck, unexpected-entry removal, byte/permission restoration, post-reset snapshot comparison, and poisoned-worker discard/retry traced. |
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
  path validation, symlink rejection, snapshots, and post-reset comparison normally
  bound file access. Cleanup failures remain retryable and prevent accounting release;
  defaults represent unavailable advisory metadata or absent environment values.
  RUST-004 records the remaining actor-conditional race between component validation
  and later path-based worker operations; ordinary same-worker sequencing does not
  overlap mutation/reset with its supervised process.
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

## Workspace isolation and resource-backend audit

The workspace capability lifecycle is preflight manifest discovery, one aggregate
core copy reservation, per-worker materialization, mutation, reset, explicit
original-integrity checkpoints, cleanup, and drop fallback. Preflight canonicalizes
the original root and hashes normalized root-relative manifest entries. Worker
creation atomically binds the preflight ID, reservation ID, aggregate allowance,
and worker slot before copying. A partial copy releases only bytes already charged
and removes only the failed slot, while retaining the reservation binding for a
valid retry. Mutation and reset recheck original content; reset failure removes the
worker from the usable map, and failed discard remains in `pending_cleanup`, which
blocks recreation until deletion succeeds.

Explicit cleanup validates a bound reservation, deletes every active and pending
worker before reporting `CleanupFinished`, and only that completion causes the core
ledger to release the reservation. A failed cleanup retains all maps and reports no
release, so retry is safe and accounting remains exactly once. `WorkerWorkspace`
drop releases its observed copy charge and slot only after the temporary directory
is absent. The focused recovery tests cover partial-copy rollback, original changes,
reset discard/recreate, read-only files and mode-000 directories, symlink rejection,
and reservation mismatch. RUST-004 is the exception to the otherwise normalized
path boundary: public `read`/`write`/`remove`/`exists`, mutation, and reset restore
operations do not share a directory-handle capability across validation and use.
The normal same-worker lifecycle does not race: mutation precedes process execution,
and reset follows termination and reap. A swap therefore requires another worker,
a same-UID external actor, or a descendant that escaped process containment and can
reach the worker tree. The result is a conditional workspace isolation/integrity
risk, not a privilege-escalation or host security boundary.

Resource backend contracts differ as follows:

| Contract item | Linux hard cgroup v2 | Windows hard Job Object | Portable |
| --- | --- | --- | --- |
| capability probe | Resolves the delegated unified hierarchy, enables `memory`/`pids`, creates and migration-tests an owned subtree; unavailable/pending cleanup can select portable only with opt-in. | Construction validates numeric limits and creates/configures the run Job Object and completion port; no separate public probe. | Constructor requires explicit best-effort memory opt-in on Linux/macOS; other targets construct directly. |
| attach timing | Wrapper launcher stops before target exec; parent moves the stopped PID to the root cgroup, then sends `SIGCONT`. | Hard backend spawns suspended, assigns the PID to run-wide and nested jobs, then resumes it. | Unix creates the process group and limits in `pre_exec`, then attach records the PID; Windows portable assigns after spawn and retains the RUST-001 race. |
| memory accounting | Run-wide `memory.max`; event-counter deltas attribute an observed violation to roots active at refresh. | Run-wide `JOB_OBJECT_LIMIT_JOB_MEMORY`; completion-port job-memory notification marks roots active when drained. | Linux uses per-process `RLIMIT_AS`; macOS does not enforce memory; other portable targets have no memory accounting. |
| process accounting | Run-wide `pids.max`; `pids.events` deltas mark active roots. | Run-wide active-process limit; completion-port notification marks active roots. | No process-count enforcement or violation accounting. |
| descendant termination | `cgroup.kill`, with verified process-group/member-PID fallback, waits for the owned subtree to empty. | Nested root Job Object terminates each tree; run Job Object terminates all trees on close. | Unix kills the process group; Windows portable terminates its kill-on-close job; unsupported targets supervise only the root. |
| classification | New memory events take precedence over process events, otherwise preserves the observed termination. | Job memory notification takes precedence over active-process notification, otherwise preserves the observed termination. | Preserves the observed termination without resource-violation reclassification. |
| cleanup retry | Failed setup returns an owned `PendingCgroupCleanup`; root/run close retain entries and retry, combining accounting and cleanup errors. | Failed close marks the run closed but not terminated, so a later close retries; owning handles provide kill-on-close fallback. | Backend close is a no-op; supervisor drop retries termination but does not surface that fallback error. |
| reported mode | `hard` only after a successful delegated probe; fallback reports the portable mode and diagnostic. | `hard`. | `best-effort`. |

The semantic gaps above are public guarantees rather than interchangeable
implementations: portable mode explicitly reports weaker memory/process enforcement
and classification. Windows hard attach is suspended and therefore does not
duplicate RUST-001, which applies to Windows *portable* attach after spawn.
RUST-003 remains the post-attach cancellation/timeout branch where a termination
error skips explicit root reap; this task adds no duplicate process-lifecycle lead.
Delegated Linux cgroup v2 and Windows Job Object behavior remain `limited` because
they were reviewed statically but were not dynamically executed on this macOS host.

Focused evidence is `.audit/rust-codebase/workspace-tests.log` (30 tests passed:
15 `workspace_handler`, 15 `workspace_recovery`) and
`.audit/rust-codebase/portable-resource-tests.log` (14 portable
`process_handler` tests passed).

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

## Persistence, plan, fingerprint, and target-input audit

Session schema setup applies a five-second busy timeout, enables foreign keys and WAL,
and advances both schema versions inside transactions. The v1-to-v2 evidence covers
preserved rows, idempotent reopen, and rollback of both the new index and
`user_version` after a failed upgrade. Run creation, each result write, inconclusive
replacement, and finish are transactional. Deferred foreign keys make a result for a
missing run fail at commit and restore any deleted prior result; determinate results
and completed runs reject writes. Candidate, result, and diagnostic rows are keyed by
the same `(run_id, mutant_id)`, so a single result-shape query cannot mix runs.
Lookup checks `runs.complete` and reads the result in two autocommit statements, so
another connection can commit completion between their snapshots. That observation
is recorded as rejected RUST-006: completion does not mutate candidate/result rows,
the returned value is the same value available immediately before completion, and
lookup can linearize at its first read. An explicit WAL read transaction could also
legitimately retain a pre-completion snapshot; no stronger response-time finality
contract or caller-visible harm is established.

Plan creation resolves fingerprint inputs and normalized targets before discovery,
records exact source hashes, and serializes candidate descriptors. Verification
rejects unknown fields, headers, incoherent roots, unsafe or duplicate records,
malformed/duplicate candidate IDs, missing or excessive requested IDs, changed source
or fingerprint records, invalid stable descriptors, and candidates no longer
discoverable under the recorded configuration. Only after this preparation does the
CLI hand the reconstructed configuration to `run_selected_loop`, which executes one
fresh baseline and only the requested candidates without session persistence.
RUST-005 is the exception: `PlanConfig` and its nested normalized types deserialize
without replaying the cross-field and nonzero-duration validation performed for CLI
configuration, and the selected-run path deliberately skips normal preparation.

Fingerprint globs ignore ignore files by policy, reject unmatched/unsafe patterns,
hash only sorted root-relative regular UTF-8 paths, and do not follow symlinks.
Exact files treat glob metacharacters literally, override glob deduplication, and
reject missing, unsafe, directory, or symlink inputs. Filesystem target discovery
uses normal Git/ignore policy, permits explicit includes to restore ignored files,
applies excludes last, does not follow symlinks, and rejects non-UTF-8 paths.

Changed-target Git commands pin color, prefixes, text conversion, external diffs,
rename detection, hunk context, diff algorithm, indentation heuristic, and rename
limit. A supplied revision is resolved with `--end-of-options` to a full commit ID
before use. The combined worktree diff covers staged plus unstaged changes; untracked
non-ignored files are added separately, while deleted, binary, empty, ignored, and
non-UTF-8-path inputs fail closed or are excluded according to target policy.
Renamed Python destinations and hostile repository diff configuration are covered.
No additional fingerprint/target lead remains beyond the actor-conditional pathname
race already owned by RUST-004.

Task 7 therefore retains one lead, RUST-005, and rejects the reviewed RUST-006
observation.

Focused evidence is `.audit/rust-codebase/persistence-input-tests.log`: 59 tests
passed (10 `session_handler`, 17 `plan`, 13 `fingerprint_inputs`, and 19
`target_handler`) on macOS with the installed Git and SQLite/WAL implementations.
