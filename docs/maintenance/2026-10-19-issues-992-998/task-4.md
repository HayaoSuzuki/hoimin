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
