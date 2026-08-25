# Windows Root Exit Barrier Implementation Plan

**Spec:** `docs/superpowers/specs/2026-08-25-windows-root-exit-barrier-design.md`

## Global Constraints

- Do not change public APIs, configuration formats, dependencies, or timeout values.
- Do not add retries, longer fixed sleeps, or test serialization.
- Keep Job Object notifications as the resource-limit classification channel.
- Treat the owned process handle as authoritative proof of exit for its exact root generation.
- Preserve UUID generation identity and retain the owned handle until a delayed notification consumes a detached generation or the run backend is dropped.
- Do not make natural Job Object notification delivery or ordering a pass condition for real-process cleanup fixtures.
- Do not modify `.idea/`.
- Keep this branch independent; do not configure `main` as its upstream.

## Task 1: Implement and verify the Windows root exit barrier

**Files:**

- Modify `crates/hoimin-cli/src/resource/windows.rs`.
- Add `docs/superpowers/specs/2026-08-25-windows-root-exit-barrier-design.md`.
- Add `docs/superpowers/plans/2026-08-25-windows-root-exit-barrier.md`.

**Steps:**

1. Add `signaled_root_without_exit_notification_classifies_and_retains_generation` using a real exited Windows process handle and a completion port that has no exit notification. The test must independently assert that the handle is signaled, then call the production classification path, expect the original termination, and assert that the generation remains active with `signal == None` and `process.is_some()`.
2. Run only that regression and record RED as `root exit notification timed out`.
3. Add an internal `RootExitBarrier` outcome and make the run-wide barrier consume a queued notification before falling back to `WaitForSingleObject` on the generation's owned handle.
4. On `NotificationConsumed`, remove the classified generation. On `ProcessSignaled`, detach it while retaining UUID, PID, and process handle. Preserve the existing timeout error when neither condition establishes exit.
5. Add or adjust focused state tests so notification-consumed generations are removed, unnotified signaled generations are retained, delayed notifications consume detached generations before reused-PID generations, and owned handles remain present while detached.
6. Run the new test, all Windows resource tests, abnormal-root and root-before-descendant tests, and the `jobs_four_reaches_a_cross_process_barrier` E2E test.
7. Repeat each flaky Windows regression 20 times locally and require zero failures. Do not add repetition to CI.
8. Run `cargo fmt --all -- --check`, Clippy with warnings denied, `cargo test --workspace`, and the independent `cargo test -p hoimin-cli --test run_e2e` command.
9. Run focused mutation testing for the changed Windows production lines and record killed, unviable, timed-out, and missed mutants. Manually verify counterfactuals if platform-specific mutation generation cannot express a required state transition.
10. Self-review the complete spec, plan, production diff, tests, verification evidence, branch tracking, and `.idea/` status; commit the implementation and documentation.

## CI Follow-up: Natural Notification Independence

Real-process timeout and attach-cleanup fixtures prove process exit plus detached generation identity and owned-handle retention. They assert that the exact retained handle is signaled and do not wait for natural Job Object notification delivery. Deterministic state-machine tests remain responsible for notification-first removal, delayed-generation consumption, and reused-PID ordering.

Repeat each revised real-process regression 20 times locally and require zero failures. Do not add retries or repetition to CI. Exact test targets:

    cargo test -p hoimin-cli --lib resource::windows::tests::attach_failure_cleanup_retains_signaled_assigned_generation -- --exact --nocapture
    cargo test -p hoimin-cli --lib resource::windows::tests::timeout_cleanup_retains_signaled_detached_generation -- --exact --nocapture

### Local Execution Evidence (Not CI)

Both revised exact regressions passed 20/20. The default-parallel `cargo test -p hoimin-cli --lib --quiet` suite passed 20/20, with 372 passed and 8 ignored in every run. Three focused counterfactual mutants were caught 3/3: detach deletion at line 521, PID comparison reversal at line 422, and `root_process_signaled` returning false at line 438.
