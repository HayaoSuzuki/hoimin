# Task 4: early copy admission (#994)

## Requirements review

1. Compared issue #994 and accepted design: static over-cap inventory must reject before any snapshot content write or test-command dispatch. Known exhaustion may precede later content validation.
2. Checked both resource domains: copy counts unique logical bytes multiplied by requested workers, accepts equality, and does not charge the pristine snapshot; owned disk includes that snapshot and rejects equality.
3. Traced mutable ownership: core BudgetLedger alone creates the final reservation/grant. Admission receives immutable configuration, not a mutable remaining balance.
4. Enumerated source races: initial metadata growth, growth between chunks, added paths, duplicate includes, and final source verification. Rejection must retain existing pending-owner cleanup.
5. Confirmed exclusions and validation: no CLI option, additional content pass, timing threshold, or broad workspace run; bounded public CLI marker and existing test-only I/O counters provide evidence.

## Design review

1. Selected PreflightLimits to carry the two immutable ceilings through existing handler preflight and snapshot creation without extending an already seven-argument API.
2. Share checked worker multiplication and the strict `>` copy comparison with core reservation. Keep the owned disk `>=` policy independent.
3. Retain logical byte accumulation and path deduplication in SnapshotWriter; admission checks derived totals without tracking a second allowance or charging a reservation.
4. Run inventory whenever either cap is configured, before constructing the pending snapshot; keep source snapshot/hash and final verification as the existing two content passes.
5. Check initial file metadata and every streamed chunk before destination writes. Keep existing PendingOwnedWorkspace rollback and duplicate-entry source-consistency verification.

## Implementation review

1. Followed configuration through shell setup, both WorkspaceHandler preflight paths, inventory, and SnapshotWriter. Added a shell setup regression that exercises admission without calling the final core reservation, so omitting cap forwarding cannot hide behind late rejection.
2. Checked arithmetic and ownership: core `check_workspace_copy` owns checked multiplication; it and BudgetLedger share the strict amount comparison. Final grant still validates reported totals before reserving, with no new mutable counter or snapshot charge.
3. Reviewed source races: initial metadata and every chunk check accumulated unique bytes before writing; duplicate paths retain content-drift verification and never charge twice. The worker count is `u32`, so converting to `u64` and adding the one owned snapshot cannot overflow.
4. Traced pending snapshot ownership on both initial-size rejection and a later added file. Existing rollback drops the partial managed child; admitted final verification and no-follow paths remain unchanged.
5. Reviewed API and observability scope: narrowed the preflight method to its actual workspace visibility; existing workspace errors map to the established CopyLimit effect. Changed only test-only metrics to count successful chunk writes, so a rejected partial file is no longer invisible to I/O counters.

## Tests review

1. Established behavioral RED before inventory enforcement: both static over-cap cases wrote `(1, 5)` instead of `(0, 0)`. An initial const-constructor scaffold compile error was corrected before collecting this RED; compile failure is not counted as behavioral evidence.
2. Established streaming RED while only the old owned-disk checks remained: post-inventory growth wrote five bytes; streamed growth wrote 131,072 instead of 65,536 bytes. Then enabled copy checks before metadata admission and each chunk write.
3. Checked independent expectations and scenario isolation: separate macro-generated tests cover worker multiplicity and equality; duplicate includes, overflow, validation precedence, normal two-pass reads, partial rollback, and owned equality each have one function. Core tests use literal totals and include the `u64::MAX` boundary.
4. Checked integration oracles: public CLI uses a marker command and null baseline, while the shell setup regression independently proves early cap propagation. Corrected the initial test-only assumption that diagnostics were in report JSON; actual report contract writes diagnostic JSONL to stderr.
5. Reviewed counters and regression coverage: assertions now observe actual chunk writes, source reads/hashes, unchanged original bytes, no published manifest after failure, and managed-child cleanup. Existing workspace/grant/stream tests cover source drift, retry, no-follow, cancellation, and disk policy. No timing thresholds or added content pass.

## Validation and evidence

All commands used:

```sh
CARGO_TARGET_DIR=/tmp/hoimin-issues-992-998/target
CARGO_BUILD_JOBS=2
CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0
RUST_TEST_THREADS=2
```

The variables were exported for each command, not merely set in a separate
shell. Cargo runs were sequential and offline. Commit hooks additionally use
`PREK_CONCURRENCY=1` and `CARGO_NET_OFFLINE=true`.

- RED: `cargo test --offline -p hoimin-cli --lib workspace::copy::admission_tests -- --nocapture` — 4 passed, 2 failed: actual snapshot counters `(1, 5)`, expected `(0, 0)`. Log: `/tmp/hoimin-issues-992-998/task-4-red.log`.
- First GREEN: same command — 6 passed. An initial visibility warning was removed before final checks.
- Streaming RED: same command — 8 passed, 2 failed: post-inventory growth `(1, 5)` vs `(0, 0)`; stream destination 131,072 vs 65,536 bytes. Log: `/tmp/hoimin-issues-992-998/task-4-stream-red.log`.
- `cargo test --offline -p hoimin-cli --lib workspace::` — 208 passed, 7 existing ignored. Log: `/tmp/hoimin-issues-992-998/task-4-workspace.log`.
- `cargo test --offline -p hoimin-core budget` — 14 matching tests passed across core targets; then `cargo test --offline -p hoimin-core --test budget` ran all 16 budget integration tests, including five new admission boundaries. Logs: `task-4-core.log`, `task-4-core-boundary.log` under the same evidence directory.
- `cargo test --offline -p hoimin-cli --test disk_workspace --test workspace_handler --test workspace_stream --test workspace_empty_directories` — focused CLI integrations passed, including the public CLI's 1 KiB padding case: exit 2, `workspace.copy.limit`, baseline null, command marker absent, original source unchanged. Log: `task-4-integration.log`.

Verified I/O outcomes: inventory refusal has zero content reads, hashes and
snapshot writes; admitted five-byte source has two reads/two hashes, ten bytes
each; duplicate includes produce one five-byte snapshot write and a ten-byte
grant for two workers; streaming growth for three workers permits one 65,536-byte
chunk at equality and rejects the next; added-file overflow writes only the
first four bytes and removes the partial snapshot.

Full workspace tests remain assigned to integration Task 8. This task's new
checks use ordinary host filesystem capabilities; they do not establish new
cross-platform filesystem claims.
- Final `cargo test --offline -p hoimin-cli --lib workspace::copy::` — 39 passed, 1 existing ignored, including all 12 new admission tests. Log: `task-4-copy-final.log`.
- `cargo test --offline -p hoimin-cli --lib shell_setup_passes_copy_cap_to_workspace_admission` — 1 passed. Log: `task-4-shell.log`.
- Focused CLI integration total: 47 passed (disk 3, empty directories 6, handler 36, stream 2).
- `cargo fmt --all` and `git diff --check` succeeded. Repository hooks run on the scoped commit; their result is recorded in the task report.

## Integration follow-up: controlled admission failure metrics

Task 8's all-features run found one stale integration scenario:
`metrics_retains_incomplete_warning_after_later_preflight_budget_failure`.
Its 32-byte source and `--max-copy-size 1B` now reject during immutable
inventory admission. Previously preflight succeeded and the final
`reserve_workspace_copy` rejected inside the core transition, producing
`MachineError::Budget` and an outer Rust `Err`. That outer error caused
`metrics.incomplete` and withheld the sidecar. Admission now emits the existing
`EffectFailed(CopyLimit)` event; the run completes its controlled failure
report and cleanup, returning exit 2 with `summary.complete = false`.

This is an intended consequence of #994, not a successful mutation run or an
output-safety defect. Metrics describe the completed portions of a controlled
failure: targets and cleanup, zero candidates/executions, no worker process,
and the same run ID as the failure report. The rejected preflight is omitted
from completed stage timings. The issue 484 destination design explicitly
preserves authorized metrics for modeled failures. No product code, final
reservation authority, output authorization, or lifecycle semantics changed.

The integration test now checks those outcomes, the copy-limit diagnostic,
absence of misleading metrics warnings, original bytes, and no baseline
marker. Three additional public CLI scenarios pair the same copy cap with
metrics destinations colliding with selected source, an unopened session DB,
and an existing session ownership lock. Each confirms collision rejection,
byte/hash preservation, no baseline, and clean execution-root cleanup; the
lock case also proves no session DB was created. Destination inspection occurs
before inventory and grants no finalizer capability after collision.

Genuine outer-error coverage remains in
`metrics::tests::failed_run_skips_the_sidecar_and_queues_an_explicit_diagnostic`;
it supplies an authorized output and a transition error, then asserts no file
and the exact `metrics.incomplete` warning while preserving earlier warnings.
`expired_shutdown_skips_metrics_with_an_incomplete_warning` also remains intact.
Both executed in the focused all-features library run.

### Requirements self-review (five passes)

1. Compared the earlier admission policy with the stale late-reservation trigger;
   retained exit 2, zero baseline work, and the copy-limit diagnostic.
2. Distinguished report `complete = false` from successful completion of failure
   reporting/cleanup; metrics must never imply a completed preflight or mutation.
3. Checked issue 484's explicit requirement to preserve authorized modeled-failure
   metrics; restoring outer errors would contradict that requirement.
4. Enumerated concrete early-finalization overwrite risks: source, session DB,
   and session lock; each has a separate acceptance guard under the same cap.
5. Kept BudgetLedger as sole reservation authority and preserved source/session
   protection; no compatibility adapter, new option, or broad audit was needed.

### Design self-review (five passes)

1. Traced destination validation, preflight completion transfer, core transition,
   cleanup, and finalization; the changed outcome originates at admission.
2. Verified the retained parent/entry capability is obtained before inventory;
   collision returns no Ready destination, closing the finalizer write gate.
3. Reused existing fixture/run/collision helpers; introduced no new production
   abstraction or duplicated allowance state.
4. Chose separate public behavior tests for each protected resource rather than
   fixture timing, source mutation races, or artificial outer failures.
5. Reviewed scope using software-design-philosophy: 10/10 for the reviewed
   boundary, all eight diagnostics satisfied, no failed rows requiring change.
   Existing modules retain their responsibility and hide output authorization.

### Implementation self-review (five passes)

1. Confirmed only the metrics integration test and this durable record are
   deliverables; production admission and finalization remain untouched.
2. Checked failure JSON parsing and typed RunMetrics validation together, including
   shared run identity and literal zero-work expectations.
3. Checked completed stage assertions: cleanup/targets are present; preflight,
   baseline, copy, analysis and mutants are never reported as completed.
4. Checked session sentinel is intentionally unopened, so any attempted recovery
   or overwrite cannot be masked by SQLite behavior; owned-lock contents remain.
5. Snapshotted all 20 pre-existing Task 8 dirty files by SHA-256 before work and
   verified unchanged after formatting and around commit hooks. Only two intended
   files are staged; the ignored scratch report is never staged.

### Test self-review (five passes)

1. Reproduced the original failure unchanged at the expected `!metrics.exists()`
   assertion; no product fix was written against an unverified hypothesis.
2. Named observable breaks: suppressing valid failure metrics, publishing false
   stage/work counts, skipping collision checks, overwriting protected entries,
   running baseline after rejection, or omitting cleanup.
3. Used literal expected values and actual CLI files/reports, without mocks,
   implementation-derived expected totals, or timing thresholds.
4. Retained genuine incomplete-warning tests and ran them with all features;
   did not manufacture a replacement failure solely to preserve the old name.
5. Ran the entire metrics destination target, including existing alias, missing
   parent, safe-link, and parent-replacement controls; full workspace rerun stays
   assigned to Task 8 under the agreed single-validation-process resource policy.

### Follow-up validation

All Cargo commands used the prescribed target directory, two build jobs,
disabled incremental/debug info, two test threads, offline mode, plus
`PREK_CONCURRENCY=1` and `CARGO_NET_OFFLINE=true` for hooks.

- Original RED: `cargo test --offline -p hoimin-cli --all-features --test metrics_destinations metrics_retains_incomplete_warning_after_later_preflight_budget_failure -- --exact`
  — one expected failure at the stale sidecar-absence assertion. Evidence:
  `/tmp/hoimin-issues-992-998/task-4-integration-red.log`.
- `cargo test --offline -p hoimin-cli --all-features --test metrics_destinations`
  — 29 passed, including the three new protected-output cases. Evidence:
  `/tmp/hoimin-issues-992-998/task-4-integration-metrics.log`.
- `cargo test --offline -p hoimin-cli --all-features --lib metrics`
  — 29 passed, including transition-error and expired-shutdown incomplete-warning
  coverage. Evidence: `/tmp/hoimin-issues-992-998/task-4-integration-metrics-unit.log`.
- `cargo fmt --all` and `git diff --check` passed. The 20 unrelated files retained
  their original SHA-256 values. Repository commit hooks are required and their
  actual outcome is recorded in the integration fix report.

No unresolved product defect was found. Native integration evidence is macOS;
no additional cross-platform claim is made by this follow-up.

The first commit hook run rejected the new `assert!(workers.is_empty())` under
Clippy's `assert_is_empty` lint. Changed it to `assert_eq!(workers, Vec::new())`
so failures print the worker list, then reran the full destination target (29
passed) and all commit hooks. The original 20 unrelated file hashes remained
unchanged after the rejected hook run.
