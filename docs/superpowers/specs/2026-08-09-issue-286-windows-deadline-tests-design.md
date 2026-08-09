# Issue #286 Windows Deadline Test Design

## Goal

Make the Windows shutdown-deadline tests measure the semantic operation they
name, without weakening production timeout or shutdown-grace behavior.

## Evidence and root cause

PR #285 exposed two independent failures in CI run `31313554038`:

- `total_timeout_exits_after_grace_when_session_finish_is_locked` successfully
  observed the child exit, then included up to two seconds of descendant-stop
  observation in the elapsed value checked against the child-exit deadline.
- `total_timeout_bounds_paused_materialization_at_first_shutdown_deadline`
  successfully completed `timeout(500ms, run)`, then included pause-controller
  release and join time in a second assertion against the same 500 ms bound.

Both failures occurred outside the already bounded operation. The first test
had passed earlier in the same Windows job, and the retry failed at the second
test instead. The analyzer-only changes in PR #285 do not participate in either
path.

PR #287 then exposed the same setup defect in
`shutdown_budget_preempts_an_owned_blocking_close`: its 20 ms budget started
before `spawn_blocking` acquired a thread. Under Windows runner load, the
budget expired before the operation sent its readiness signal, so the test
failed with `RecvError` instead of exercising an already-owned close.

## Options considered

1. Measure semantic milestones. Record child-exit elapsed immediately after
   `child.wait()`, and let the explicit Tokio timeout be the sole wall-clock
   assertion around the paused run. Keep cleanup and controller joins as
   separate correctness checks. This is the selected option.
2. Increase all thresholds. This reduces failures but weakens the regression
   signal and still mixes unrelated teardown with the deadline.
3. Relax assertions only on Windows. This hides the symptom while giving the
   same contract different meanings by platform.

## Design

### Locked-session total timeout E2E

Capture `child_exit_elapsed` immediately after the bounded `child.wait()`
returns. Return that value from the scenario and assert it is below nine
seconds. Continue to wait for the descendant to stop and assert that it did,
but do not charge that observation interval to the child-exit contract.

The nine-second outer timeout, exit code, diagnostics, incomplete session,
missing `run_finished`, retained database lock, and descendant cleanup checks
remain unchanged.

### Paused materialization unit test

Keep `tokio::time::timeout(Duration::from_millis(500), &mut run)` as the bound
on the operation under test. Its `Ok` branch proves the run future completed
within that interval. Release and join the blocking pause controller afterward
for leak-free teardown, without rechecking wall time that now includes that
teardown.

The error class and outstanding blocking-I/O count assertions remain
unchanged.

### Already-owned blocking close

Extract the existing join-handle wait and expiry classification from
`run_owned_blocking_until` into a private `await_owned_blocking_until` helper.
The production wrapper still creates the same task, performs the same
pre-deadline operation guard, and delegates to exactly the same wait logic.

The test creates that shaped blocking task first, observes its readiness, and
only then constructs the 20 ms shutdown budget and calls the extracted wait.
This makes "owned" a configured premise rather than a scheduler assumption.
On expiry, releasing the blocking operation remains explicit so the test does
not leak work.

## Test strategy

- Run each focused test repeatedly to exercise scheduling variation.
- Run the already-owned blocking-close test repeatedly with its deterministic
  readiness-before-budget ordering.
- Run the complete CLI library and `run_e2e` suites.
- Run formatting, Clippy, and the workspace test suite.
- Require the Windows CI job to pass before squash merge.

The private helper extraction does not change production behavior. No timeout
value, shutdown grace, public API, or report format changes.
