# Runtime preparation deadline review — issue #1034

Base: `05c909c54a3bdf3485db2c833456fabe83c7dfaf`. This record concerns the
shared-worktree implementation of #1034 and its shell integration with #1032.
No compatibility mode was added.

## Design self-review passes

1. **Measurement boundary.** Traced `run_loop_with_control`,
   `prepare_run_config`, `run_verified`, and `run_loop_context`. The original run
   performed environment capture/fingerprint reads before starting its clock;
   common setup also preceded that clock. Chose one `PreparationToken` deadline
   before run capture, and after `VerifiedPlan::into_execution` for verify.
   Standalone plan preparation remains synchronous and outside this contract.
2. **Stop propagation.** Traced existing `RunControl` / `ProcessStartGate` and
   interrupt ownership. Reused that gate, recorded its first cancellation time,
   and carried the original interrupt monitor into execution. Preparation checks
   both sides of each setup boundary, inside fingerprint traversal, and inside
   controlled janitor selection/cleanup. No preparation-only cancellation flag
   or remaining-time ledger was added.
3. **Blocking ownership.** Examined `spawn_blocking` cancellation semantics and
   `SetupRollback` field/drop order. A worker owns its closures, leases, handlers,
   and returned values until actual completion. A oneshot receiver expiry only
   bounds the wait; failed delivery destroys the result on the blocking owner.
   A result racing cancellation is disposed on a blocking thread.
4. **Fixed grace and rollback.** Found the old per-root 60-second cleanup calls.
   Replaced those with the remaining shared absolute shutdown deadline; marking
   cleanup-ready and child-capability release precede cleanup. Late completion
   cannot obtain another grace. Marker/cleanup failures are retained; safe
   deferrals remain governed by the existing managed-root ownership/janitor rules.
5. **Output and initial monitor.** Review found initial disk sampling awaited
   before the loop without stop selection. Split construction from first sample
   so the monitor handle remains owned, sample wait observes preparation stop,
   and stop/join shares the remaining grace. Preparation diagnostics use owned
   writers with bounded write and flush; no baseline/report is fabricated.

## Implementation self-review passes

1. **Entry-point audit.** Confirmed production run bypasses the unmeasured
   `prepare_run_config` helper, verify creates the token after saved-plan
   verification, and `run_loop_context` uses `timing.deadline` rather than `now +
   timeout`. Tests using the prepared-config helper establish their deadline at
   their own execution entry. The successful path does not replenish time.
2. **Setup admission audit.** Checked roots, disk policy, workspace, backend,
   process handler, analyzer, and report boundaries. Fourteen parameterized
   stage × stop-reason cases verify that cancellation/expiry admits no later
   boundary and that rollback removes ordinary fixture roots.
3. **Error-race audit.** Review identified two receipt races: a completed real
   preparation failure could be hidden by a stop, and nested blocking join
   failures could be ignored. `accept_result` preserves real failures and
   `dispose` matches both timeout and `JoinError`. Deterministic receipt-race and
   panic-on-drop regressions exercise those paths.
4. **Ownership/finalization audit.** Checked delayed worker delivery, abandoned
   receiver, initial measurement, marker errors, and each rollback root. Shared
   deadline access remains live in late owners. Monitor startup failure cannot
   certify monitor join; unjoined measurements retain their roots and produce an
   infrastructure error.
5. **Git integration audit.** Shell passes the existing execution cancellation
   capability to the owned Git runner and drains it before both cleanup paths.
   Successful `target.cancelled` is replaced only by the already-observed stop;
   termination/reap failures remain infrastructure errors. Serial selection
   prioritizes stop observation before completed target effects. Runner spawn
   admission also uses the run's existing gate and original deadline.

## Verification self-review passes

1. **Red/green baseline.** Added and ran
   `preparation_cancellation_skips_invalid_fingerprint_inputs` before production
   changes. It failed with `fingerprint.file.not_found: missing.toml` rather than
   cancellation `130`, then passed after admission moved before capture/reads.
2. **Controlled clock boundaries.** Paused-clock tests cover before deadline,
   equality, after deadline, remaining time after ten seconds of preparation,
   first-request cancellation grace, repeated cancellation, and cancellation
   after timeout without grace extension. Setup stage tests use controlled clock
   advancement rather than fixed sleeps.
3. **Started-owner evidence.** Start notifications and release channels verify
   that a cancelled blocked worker retains its owner after the wait expires.
   The first disk measurement test holds real monitor startup, cancels it, and
   checks bounded return and empty stdout before releasing the operation.
   Its first attempted harness exposed the prior non-Send future; the final
   regression passes after monitor construction was separated from the sample.
4. **Failure evidence.** Tests cover completed preparation failure racing
   cancellation, rollback panic, diagnostic write failure, diagnostic flush
   failure, and a diagnostic writer retained beyond shutdown expiry. The
   production output path observes both write and flush results.
5. **Integrated verification.** Focused preparation command passed 26 tests at
   the first combined checkpoint; subsequent additional cases and final shell,
   lint, and workspace results are recorded below or by the coordinating parent.
   Git public cancellation/environment/scope suites passed in the Git review
   record. Janitor pre-stopped selection regression was observed red before the
   selection deadline was connected; detailed mid-selection review/fix belongs
   to the janitor/Git agent's record.

## Limits and remaining verification

This does not promise interruption of an already-started OS operation or prove
all filesystem interleavings. A blocking operation can retain ownership after
its wait has failed, and runtime teardown can still wait for blocking workers.
The implementation provides bounded async waits and preserves resource safety;
process-start-to-OS-quiescence within the exact configured seconds is outside
the issue contract. Platform-specific resource backend code was not exercised
on Linux/Windows by this macOS session. Saved-plan read/validation/reanalysis
limits and `plan`/dry-run contracts remain outside #1034.

## Final runtime checks and review follow-ups

- `CARGO_TARGET_DIR=/Users/hayao/RustroverProjects/hoimin/target cargo test -p
  hoimin-cli --lib shell::`: **156 passed, 0 failed, 2 ignored**. The ignored
  cases are existing isolated subprocess fixtures. This final run includes the
  direct orderly preparation-timeout `Ok(4)` check.
- `git diff --check` for shell, preparation, fingerprint inputs, disk monitor,
  usage, and this evidence file: passed.
- Runtime-specific Clippy findings were resolved: the timed preparation entries
  are boxed so callers do not inherit oversized futures; the original loop
  arguments explicitly include the prepared monitor/deadline ownership; new
  tests follow the repository lint rules. The coordinating parent runs the
  final complete workspace/all-features lint and test gates.
- A final review found that cancellation arriving between preparation admission
  and loop observation could award a new grace interval. All execution event
  budget creation now goes through `RunControl`, which uses the first recorded
  request time. A paused handoff regression verifies request + two seconds.
- A second delayed-observation case had deadline `t + 1`, cancellation at `t +
  2`, and no earlier stop observation. Its new regression was observed **red**:
  shutdown ended at `t + 4` instead of `t + 3`. The adapter now uses the original
  timeout shutdown budget when cancellation is requested at/after that deadline;
  cancellation event/exit-code precedence is retained. The regression and final
  shell suite are **green**.
- Successful Git cancellation can race stop readiness between `select!` polls.
  The completed-target branch now reconciles only `target.cancelled` with the
  existing gate/deadline; real termination/reap failures remain failures. Three
  deterministic regressions cover cancellation, timeout, and retained reap error.
- Initial monitor capability cloning and construction also run on the owned
  preparation blocking worker. Construction admits no filesystem measurements:
  the monitor stays idle until the explicitly bounded first sample. Failed
  delivery drops the idle monitor before setup; its retained leases remain live
  until its thread exits. Cancellation during an active first sample explicitly
  requests stop and joins within the existing grace, or fails safely with owned
  roots retained.
- Controlled janitor selection/deletion review and the broader owned-root suite
  are recorded in the adjacent [Git and janitor record](git.md). The janitor
  agent reports 86 passed and 3 ignored isolated fixtures after meaningful
  mid-selection and mid-deletion red/green regressions.
- The full workspace run exposed an execution-timeout integration test whose
  100 ms budget could now expire during preparation. It expected an execution
  report, which preparation correctly does not fabricate. The test now waits
  for the baseline child's start marker, then advances Tokio's clock to the
  original deadline. Its exit 4, `run_finished`, and exported metrics assertions
  are unchanged; the focused regression passed before the final workspace run.
