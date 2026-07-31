# Second Ctrl+C Forced Exit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep the first Ctrl+C on hoimin's orderly cancellation path, while making a second Ctrl+C terminate immediately with exit code 130 even when process drain, workspace cleanup, or session finalization is blocked.

**Architecture:** Add one process-wide interrupt monitor owned by the CLI shell boundary. It installs and owns Tokio's Ctrl+C handler, forwards only the first successful signal to the existing scheduler as `CancellationRequested`, and waits for the second signal in an independent Tokio task that invokes a production terminator (`std::process::exit(130)`) without requiring the scheduler future to be polled. The core state machine continues to own orderly cancellation/report semantics and never receives escalation.

**Tech Stack:** Rust 1.88, Tokio 1.47 (`signal`, `sync`, `rt-multi-thread`, `time`, `process`), libc on Unix, rusqlite integration fixtures, Cargo workspace tests.

## Global Constraints

- Preserve each biased scheduler select's current first-stop ordering: ready serial effect, injected cancellation, deadline, first Ctrl+C, then process completion where applicable.
- The first successful Ctrl+C maps to exactly one `RunEvent::CancellationRequested`; it retains orderly tree reaping, cleanup, incomplete report/session finalization, and exit 130.
- The second successful Ctrl+C has unconditional process-level precedence and exits 130. It must not wait for another transition, report/metrics write, drain, cleanup, session operation, or destructor.
- `RunControl::cancel()`, deadlines, and `EffectFailed` do not count as Ctrl+C signals. Two real Ctrl+C events are still required for forced escalation.
- A handler installation/read failure before the first signal remains an infrastructure error containing `install Ctrl+C handler`; it is not cancellation.
- Keep all public run-loop signatures, JSON/JSONL schemas, and `hoimin-core` events unchanged. Add no dependency.
- Do not add a third-signal/default-handler protocol; deterministic termination on signal two is the requested escape hatch.
- Keep #153 open for complete cross-platform real-console-signal coverage. This change adds a Unix subprocess proof for #109 and platform-neutral monitor unit tests, not Windows `GenerateConsoleCtrlEvent` coverage.
- Keep #164 outside this change. Its state-machine model represents only the orderly first signal as `CancellationRequested`; forced escalation is intentionally above `transition`.

---

## Root Cause and Design Decision

`run_loop_prepared` creates one pinned `tokio::signal::ctrl_c()` future. Its first poll installs Tokio's process-wide handler, replacing default termination. Once `stop_signalled` is true, `execute_effect_with_cancellation(...).await`, `completion_rx.recv().await`, and all `drain_processes(...).await` paths stop polling it. A second signal is retained by Tokio but has neither a consumer nor the original default action. This is a signal lifetime/ownership bug in the shell, not a `RunState` transition bug.

Three approaches were evaluated:

1. Add second-signal selects around every stopped await. This duplicates precedence/error handling and still cannot preempt synchronous workspace deletion or backend close performed during a future poll.
2. Add a forced-cancellation event to `hoimin-core`. The state machine would still need to schedule and poll effects, so it cannot escape wedged cleanup.
3. **Selected:** an independent monitor delivers signal one to the shell and itself terminates on signal two. It remains live while the scheduler is blocked and preserves all first-signal state-machine contracts.

The monitor is run-scoped and aborts on ordinary drop. Its terminator is injected only at a private construction boundary: production supplies `std::process::exit`, unit tests supply a channel-recording closure. Do not expose a public test hook or environment variable.

## File Map

- Create `crates/hoimin-cli/src/interrupt.rs`: Ctrl+C installation, first delivery, second escalation, private injection boundary, unit tests.
- Modify `crates/hoimin-cli/src/lib.rs`: declare the private module.
- Modify `crates/hoimin-cli/src/shell.rs`: consume the monitor's first notification in the existing select positions.
- Modify `crates/hoimin-cli/tests/run_e2e.rs`: Unix real-SIGINT regression with blocked session finalization.

**TDD execution order:** Perform Task 3 Steps 1-2 first to capture the real-process RED, then Task 1, Task 2, Task 3 Steps 3-5, and Task 4. The numbered sections group changes by reviewable component; this explicit order preserves both end-to-end and unit-level RED/GREEN evidence.

### Task 1: Build the independently-owned interrupt monitor

**Files:**
- Create: `crates/hoimin-cli/src/interrupt.rs`
- Modify: `crates/hoimin-cli/src/lib.rs:1-16`

**Interfaces:**
- Consumes: `tokio::signal::ctrl_c()` and production `std::process::exit(130) -> !`.
- Produces: private `InterruptMonitor::spawn() -> InterruptMonitor` and async `InterruptMonitor::first(&mut self) -> Result<(), String>`.
- Produces: private `spawn_monitor(raw_signals, terminate, producer_task)` used by production and this module's tests.

- [ ] **Step 1: Declare the module and write the failing first-signal test**

Add `mod interrupt;` in `lib.rs`. In `interrupt.rs`, create a test adapter backed by `tokio::sync::mpsc::unbounded_channel` for signals and `oneshot` for termination. The adapter must invoke the same generic monitor loop as production.

```rust
#[tokio::test]
async fn first_signal_is_forwarded_without_forcing_exit() {
    let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
    let (forced_tx, mut forced_rx) = tokio::sync::oneshot::channel();
    let mut monitor = spawn_test_monitor(signal_rx, move |code| {
        let _ = forced_tx.send(code);
    });

    signal_tx.send(Ok(())).unwrap();
    assert_eq!(monitor.first().await, Ok(()));
    assert!(forced_rx.try_recv().is_err());
}
```

- [ ] **Step 2: Run it and verify RED**

Run: `cargo test -p hoimin-cli interrupt::tests::first_signal_is_forwarded_without_forcing_exit -- --exact`

Expected: compilation fails because the monitor and test adapter do not exist.

- [ ] **Step 3: Implement first-signal ownership minimally**

Use this concrete ownership shape:

```rust
pub(crate) struct InterruptMonitor {
    first: tokio::sync::oneshot::Receiver<Result<(), String>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl InterruptMonitor {
    pub(crate) fn spawn() -> Self {
        let (raw_tx, raw_rx) = tokio::sync::mpsc::unbounded_channel();
        let producer = tokio::spawn(async move {
            loop {
                let signal = tokio::signal::ctrl_c()
                    .await
                    .map_err(|error| format!("install Ctrl+C handler: {error}"));
                let failed = signal.is_err();
                if raw_tx.send(signal).is_err() || failed {
                    break;
                }
            }
        });
        spawn_monitor(raw_rx, |code| std::process::exit(code), Some(producer))
    }

    pub(crate) async fn first(&mut self) -> Result<(), String> {
        (&mut self.first).await.unwrap_or_else(|_| {
            Err("install Ctrl+C handler: monitor stopped".to_owned())
        })
    }
}

impl Drop for InterruptMonitor {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
```

`spawn_monitor` accepts `mpsc::UnboundedReceiver<Result<(), String>>`, `Terminate: FnOnce(i32) + Send + 'static`, and the optional production task. Its own task awaits the first raw value, sends it through the oneshot, returns if it was an error, then awaits exactly one more raw value. A successful second value calls `terminate(130)`; a second error or closed channel returns without forcing. `spawn_test_monitor` passes its test receiver and `None`, so production and tests share the counting/escalation task. The separate production task begins polling and rearming `ctrl_c()` immediately.

- [ ] **Step 4: Verify first-signal GREEN**

Run: `cargo test -p hoimin-cli interrupt::tests::first_signal_is_forwarded_without_forcing_exit -- --exact`

Expected: PASS; signal one reaches the shell receiver and no termination code is observed.

- [ ] **Step 5: Add failing second-signal and handler-error tests**

```rust
#[tokio::test]
async fn second_signal_forces_130_without_waiting_for_first_consumer() {
    let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
    let (forced_tx, forced_rx) = tokio::sync::oneshot::channel();
    let _monitor = spawn_test_monitor(signal_rx, move |code| {
        let _ = forced_tx.send(code);
    });

    signal_tx.send(Ok(())).unwrap();
    signal_tx.send(Ok(())).unwrap();
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(1), forced_rx)
            .await
            .expect("second signal must not depend on scheduler polling")
            .unwrap(),
        130
    );
}

#[tokio::test]
async fn first_handler_failure_is_reported_and_never_escalates() {
    let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();
    let (forced_tx, mut forced_rx) = tokio::sync::oneshot::channel();
    let mut monitor = spawn_test_monitor(signal_rx, move |code| {
        let _ = forced_tx.send(code);
    });

    signal_tx
        .send(Err("install Ctrl+C handler: fixture".to_owned()))
        .unwrap();
    let error = monitor.first().await.unwrap_err();
    assert!(error.contains("install Ctrl+C handler"), "{error}");
    assert!(error.contains("fixture"), "{error}");
    assert!(forced_rx.try_recv().is_err());
}
```

The second-signal test intentionally never calls `monitor.first()`. That models shell execution stuck in synchronous cleanup or a bare drain await.

- [ ] **Step 6: Run monitor tests and verify GREEN**

Run: `cargo test -p hoimin-cli interrupt::tests -- --nocapture`

Expected: all tests PASS; signal two records 130 within one second, and first-handler failure preserves the diagnostic prefix without terminating.

- [ ] **Step 7: Commit**

```bash
git add crates/hoimin-cli/src/lib.rs crates/hoimin-cli/src/interrupt.rs
git commit -m "fix: own repeated interrupts independently"
```

### Task 2: Route the first signal through orderly cancellation

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs:790-1210`
- Test: `crates/hoimin-cli/src/shell.rs:1457-end`

**Interfaces:**
- Consumes: `crate::interrupt::InterruptMonitor` from Task 1.
- Produces: the existing `RunEvent::CancellationRequested` and `signal_failure` behavior; public results remain `Result<i32, String>`.

- [ ] **Step 1: Write the failing conversion test**

Replace the old `ctrl_c_handler_failure_is_infrastructure_not_cancellation` test with:

```rust
#[test]
fn first_interrupt_maps_success_to_cancellation_and_preserves_failure() {
    assert!(matches!(
        first_interrupt_event(Ok(())),
        Ok(RunEvent::CancellationRequested)
    ));
    let error = first_interrupt_event(Err(
        "install Ctrl+C handler: fixture".to_owned()
    ))
    .unwrap_err();
    assert_eq!(error, "install Ctrl+C handler: fixture");
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p hoimin-cli shell::tests::first_interrupt_maps_success_to_cancellation_and_preserves_failure -- --exact`

Expected: compile failure because `first_interrupt_event` has not replaced `ctrl_c_event`.

- [ ] **Step 3: Integrate the monitor without changing precedence**

Replace the local pinned future with:

```rust
let mut interrupts = crate::interrupt::InterruptMonitor::spawn();
```

In both current non-stopped `tokio::select!` blocks, retain `biased;` and the signal branch's position, but await `interrupts.first()`. Preserve the existing `cancellation.cancel()`, `stop_signalled = true`, and `signal_failure` assignments:

```rust
signal = interrupts.first() => {
    cancellation.cancel();
    stop_signalled = true;
    match first_interrupt_event(signal) {
        Ok(event) => event,
        Err(error) => {
            signal_failure = Some(error);
            RunEvent::CancellationRequested
        }
    }
}
```

Use the existing `ShellCompletion` wrapper in the completion-select arm. Rename the converter to:

```rust
fn first_interrupt_event(signal: Result<(), String>) -> Result<RunEvent, String> {
    signal.map(|()| RunEvent::CancellationRequested)
}
```

Do not add signal branches to stopped serial effects or `drain_processes`; the monitor is deliberately the only escalation owner. Do not notify it for `RunControl`, deadline, or failure stops.

- [ ] **Step 4: Verify GREEN and adjacent first-stop behavior**

Run:

```bash
cargo test -p hoimin-cli shell::tests::first_interrupt_maps_success_to_cancellation_and_preserves_failure -- --exact
cargo test -p hoimin-cli --test run_e2e injected_ctrl_c_uses_the_production_cancel_path_and_finishes_session_incomplete -- --exact
cargo test -p hoimin-cli --test run_e2e serial_output_that_requests_stop_is_accepted_before_cancellation -- --exact
```

Expected: all PASS. Injected cancellation still exits 130, emits `complete:false`, marks the session incomplete, and reaps descendants; biased branch order is unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/src/shell.rs
git commit -m "fix: keep second interrupt independent of cleanup"
```

### Task 3: Prove real SIGINT escapes blocked session finalization

**Files:**
- Modify: `crates/hoimin-cli/tests/run_e2e.rs:1-30,1122-1270,1880-2025`

**Interfaces:**
- Consumes: `CARGO_BIN_EXE_hoimin`, Unix `libc::kill`, existing long-running descendant fixture, and a retained rusqlite write transaction.
- Produces: Unix-only `second_sigint_forces_130_while_session_finish_is_blocked` and local JSONL/wait helpers.

- [ ] **Step 1: Write the Unix-only failing subprocess regression**

Implement these deterministic phases:

1. Reuse the long-running parent/descendant command from `injected_ctrl_c_uses_the_production_cancel_path_and_finishes_session_incomplete`.
2. Spawn the real binary with `--session`, `--format jsonl`, `--jobs 1`, piped stdout/stderr, and `kill_on_drop(true)`.
3. Read complete stdout lines until `kind == "mutant_started"`, then wait for `descendant_ready` and open its PID. This proves the session row exists and the process tree is actually live before cancellation.
4. Open another `rusqlite::Connection`, retry `BEGIN IMMEDIATE` until it succeeds, and retain that connection across the rest of the test. It blocks hoimin's later `FinishSession` write.
5. Send the first SIGINT to only hoimin's PID. Wait until the already-open descendant stops, proving orderly first-signal drain/reap completed while the database lock stays held.
6. Send signal two and require the subprocess to exit within one second.
7. Assert exit code 130, sub-second escalation, retained connection still not autocommit, all complete stdout lines parse as JSON, and no `run_finished` event was emitted while `FinishSession` was blocked.

After constructing `child`, `stdout`, `session`, and `descendant_ready` exactly as specified in phases 1-2, the signal/lock portion is:

```rust
    wait_for_jsonl_kind(&mut stdout, "mutant_started", Duration::from_secs(15)).await;
    let descendant =
        wait_for_descendant_process(&descendant_ready, Duration::from_secs(15)).await;
    let lock = begin_immediate_with_retry(&session, Duration::from_secs(5)).await;

    let pid = i32::try_from(child.id().expect("running hoimin pid")).unwrap();
    assert_eq!(unsafe { libc::kill(pid, libc::SIGINT) }, 0);
    assert!(descendant.wait_until_stops(Duration::from_secs(5)).await);

    let forced_at = Instant::now();
    assert_eq!(unsafe { libc::kill(pid, libc::SIGINT) }, 0);
    let status = tokio::time::timeout(Duration::from_secs(1), child.wait())
        .await
        .expect("second SIGINT must bypass blocked FinishSession")
        .unwrap();
    assert_eq!(status.code(), Some(130));
    assert!(forced_at.elapsed() < Duration::from_secs(1));
    assert!(!lock.is_autocommit());
```

Use an owned connection with `execute_batch("BEGIN IMMEDIATE")`, not a borrowing `Transaction`. Timeout/panic teardown must call `start_kill()` and `wait()` so a RED test leaves no child or descendant.

- [ ] **Step 2: Verify the regression is RED on the original implementation**

At the start of implementation, execute Task 3 Steps 1-2 before Task 1, keep the failing test unstaged, then execute Tasks 1-2 and return to Task 3 Step 3. This preserves a genuine end-to-end RED without reverting committed work:

`cargo test -p hoimin-cli --test run_e2e second_sigint_forces_130_while_session_finish_is_blocked -- --exact --nocapture`

Expected on old code: FAIL at the one-second child wait because the installed handler consumes signal two while no `ctrl_c` future is polled. Teardown kills and reaps the child.

- [ ] **Step 3: Verify GREEN with the monitor**

Run: `cargo test -p hoimin-cli --test run_e2e second_sigint_forces_130_while_session_finish_is_blocked -- --exact --nocapture`

Expected: PASS. Signal one reaps the descendant; signal two exits 130 within one second despite the SQLite lock.

- [ ] **Step 4: Re-run adjacent session/cancellation tests**

Run:

```bash
cargo test -p hoimin-cli --test run_e2e injected_ctrl_c_uses_the_production_cancel_path_and_finishes_session_incomplete -- --exact
cargo test -p hoimin-cli session -- --nocapture
```

Expected: PASS; ordinary first-signal/injected cancellation still completes its session as incomplete.

- [ ] **Step 5: Commit**

```bash
git add crates/hoimin-cli/tests/run_e2e.rs
git commit -m "test: cover second SIGINT during session cleanup"
```

### Task 4: Full verification

**Files:**
- Verify: `crates/hoimin-cli/src/interrupt.rs`
- Verify: `crates/hoimin-cli/src/lib.rs`
- Verify: `crates/hoimin-cli/src/shell.rs`
- Verify: `crates/hoimin-cli/tests/run_e2e.rs`

**Interfaces:**
- Consumes: Tasks 1-3.
- Produces: review-ready #109 branch without claiming #153 or #164.

- [ ] **Step 1: Format and lint**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit 0 with no diff/diagnostic. Keep the monitor private; do not add broad lint allows.

- [ ] **Step 2: Run all tests and contracts**

Run: `cargo test --workspace --all-targets --all-features`

Expected: PASS. Unix runs the real-SIGINT test; Windows excludes it but runs platform-neutral monitor tests. Core contract tests remain unchanged because escalation never enters `hoimin-core`.

- [ ] **Step 3: Check final diff**

```bash
git diff --check origin/main...HEAD
git status --short
```

Expected: diff check exits 0. Status contains only intended tracked files and any pre-existing worktree-local `.venv`; never stage `.venv` or `.serena`.

- [ ] **Step 4: Commit verification corrections only if needed**

```bash
git add crates/hoimin-cli/src/interrupt.rs crates/hoimin-cli/src/lib.rs crates/hoimin-cli/src/shell.rs crates/hoimin-cli/tests/run_e2e.rs
git commit -m "chore: finalize interrupt escalation checks"
```

Expected: no commit when verification made no tracked correction.

## Acceptance Checklist

- One Ctrl+C retains orderly cancellation and exit 130.
- Two Ctrl+C events exit 130 even if the shell never consumes signal one's notification and while session finalization is blocked.
- Signal two cannot be downgraded to exit 2 by cleanup/session/metrics errors.
- First handler failure remains infrastructure failure.
- The monitor is aborted on ordinary completion.
- #153 remains for Windows console-event and broader real-signal coverage; #164 remains for state-machine interleavings.
