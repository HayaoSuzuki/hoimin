# Issue #221 Windows Abnormal Root Exit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove on Windows that a real abnormally terminating root crosses the production Job Object completion port, is classified as its platform exit status rather than a timeout, and is cleaned within six seconds.

**Architecture:** Add one Windows-only behavioral test inside the backend's existing test module so the fixture uses the full `ProcessHandler` path while the test can query the private production Job Object. Keep a real sibling assigned during the crash to prevent `ACTIVE_PROCESS_ZERO` from masking a broken PID-specific abnormal-notification branch.

**Tech Stack:** Rust 2024, Tokio, `windows-sys`, Windows Job Objects, Python process fixtures, Cargo.

## Global Constraints

- Start the real roots through `ProcessHandler` and the production `WindowsBackend`.
- Atomically publish fixture readiness with a pending file followed by `os.replace`.
- Expect `ProcessTermination::Exit(-1_073_740_791)`, the signed Windows representation of `0xC0000409`, and never `ProcessTermination::Timeout`.
- Bound the complete fixture flow and the individual abnormal `handle` and `close` operations to six seconds.
- Verify the real Job Object has one assigned sibling before close and zero assigned processes after close.
- Do not call `record_notification`, mutate `RunState`, signal a numeric PID, or expose a production test hook.
- Mutate changed Rust production code only; report mutation testing as non-applicable if the final Rust diff remains entirely inside `#[cfg(test)]`.

---

### Task 1: Add the behavioral test and capture RED

**Files:**
- Modify: `crates/hoimin-cli/src/resource/windows.rs` (`#[cfg(test)] mod tests` only)
- Test: `crates/hoimin-cli/src/resource/windows.rs`

**Interfaces:**
- Consumes: `ProcessHandler::handle(RunProcess)`, `ProcessHandler::close()`, `WindowsBackend::new`, and private `active_process_count(HANDLE)`.
- Produces: Windows-only test `abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds` plus test utilities that do not mutate backend state.

- [ ] **Step 1: Add non-fixture test support**

Import `Arc`, `ProcessTermination`, and `active_process_count`. Add a test-only
RAII guard whose `Drop` calls the idempotent `ProcessHandler::close`, an async
readiness reader that accepts only a positive PID from the atomically published
file, and an async Job Object polling helper that waits for `ActiveProcesses ==
0` without changing `RunState`.

```rust
struct HandlerCloseGuard(Arc<ProcessHandler>);

impl Drop for HandlerCloseGuard {
    fn drop(&mut self) {
        let _ = self.0.close();
    }
}

async fn wait_for_ready_pid(path: &Utf8Path) -> u32 {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|pid| *pid != 0)
        {
            return pid;
        }
        assert!(tokio::time::Instant::now() < deadline, "fixture did not publish readiness");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
```

- [ ] **Step 2: Write the test against not-yet-implemented fixture constructors**

Start `sleeping_fixture` in a spawned task, wait for its real PID, then call
`abnormal_fixture`. Keep the sibling alive until the abnormal handle returns.
Assert the exact exit status, both readiness files, exactly one active process
before close, and zero after close. Enclose the full async body in a six-second
timeout and check the individual elapsed durations.

```rust
#[tokio::test]
async fn abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds() {
    let temporary = tempfile::tempdir().unwrap();
    let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
    let sibling_ready = output_dir.join("abnormal-sibling.ready");
    let abnormal_ready = output_dir.join("abnormal-root.ready");
    let backend = WindowsBackend::new(&run_limits()).unwrap();
    let handler = Arc::new(ProcessHandler::new(
        ResourceBackend::Windows(backend.clone()),
        output_dir.to_owned(),
    ));
    let _cleanup = HandlerCloseGuard(Arc::clone(&handler));

    tokio::time::timeout(Duration::from_secs(6), async {
        let sibling_handler = Arc::clone(&handler);
        let sibling = tokio::spawn(async move {
            sibling_handler.handle(sleeping_fixture(260, &sibling_ready)).await
        });
        let sibling_pid = wait_for_ready_pid(output_dir.join("abnormal-sibling.ready").as_ref()).await;
        assert_ne!(sibling_pid, 0);

        let handle_started = Instant::now();
        let abnormal = handler.handle(abnormal_fixture(261, &abnormal_ready)).await.unwrap();
        assert!(handle_started.elapsed() < Duration::from_secs(6));
        assert_eq!(abnormal.termination, ProcessTermination::Exit(-1_073_740_791));
        assert_ne!(wait_for_ready_pid(&abnormal_ready).await, 0);
        assert_eq!(active_process_count(backend.inner.job.raw()).unwrap(), 1);

        let close_started = Instant::now();
        handler.close().unwrap();
        assert!(close_started.elapsed() < Duration::from_secs(6));
        sibling.await.unwrap().unwrap();
        wait_for_job_empty(&backend).await;
    })
    .await
    .expect("abnormal Job Object fixture exceeded six seconds");
}
```

- [ ] **Step 3: Run the focused test and record RED**

Run:

```console
cargo test -p hoimin-cli --lib resource::windows::tests::abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds -- --exact --nocapture
```

Expected: compilation fails because `sleeping_fixture` and
`abnormal_fixture` do not exist. This is the missing real fixture boundary;
do not alter production classification code.

### Task 2: Implement the minimal real fixtures and reach GREEN

**Files:**
- Modify: `crates/hoimin-cli/src/resource/windows.rs` (`#[cfg(test)] mod tests` only)
- Test: `crates/hoimin-cli/src/resource/windows.rs`

**Interfaces:**
- Consumes: existing `python()` and `arg()` test helpers.
- Produces: `sleeping_fixture(u64, &Utf8Path) -> RunProcess` and `abnormal_fixture(u64, &Utf8Path) -> RunProcess`.

- [ ] **Step 1: Add a fixture request constructor**

Create a test-only `fixture_request` that passes the Python source and readiness
path directly as argv, uses the current directory, a 30-second process timeout,
and the existing resource limits.

```rust
fn fixture_request(id: u64, code: &str, ready: &Utf8Path) -> RunProcess {
    RunProcess {
        id: EffectId(id),
        worker: None,
        run_id: None,
        mutant_id: None,
        argv: vec![python(), arg("-c"), arg(code), arg(ready.as_std_path())],
        cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
        limits: ProcessLimits {
            timeout: Duration::from_secs(30),
            max_output_bytes: 64,
            max_memory_bytes: 256 * 1024 * 1024,
            max_processes: 8,
        },
    }
}
```

- [ ] **Step 2: Add the two atomic-readiness fixture modes**

The sibling writes its actual PID through `pending` plus `os.replace` and
sleeps. The abnormal fixture does the same, configures exact `kernel32` ctypes
signatures, and terminates itself with `0xC0000409`.

```rust
fn sleeping_fixture(id: u64, ready: &Utf8Path) -> RunProcess {
    fixture_request(
        id,
        "import os,pathlib,sys,time; ready=pathlib.Path(sys.argv[1]); pending=ready.with_suffix('.pending'); pending.write_text(str(os.getpid())); os.replace(pending,ready); time.sleep(30)",
        ready,
    )
}

fn abnormal_fixture(id: u64, ready: &Utf8Path) -> RunProcess {
    fixture_request(
        id,
        "import ctypes,ctypes.wintypes as w,os,pathlib,sys; ready=pathlib.Path(sys.argv[1]); pending=ready.with_suffix('.pending'); pending.write_text(str(os.getpid())); os.replace(pending,ready); kernel32=ctypes.WinDLL('kernel32',use_last_error=True); kernel32.GetCurrentProcess.restype=w.HANDLE; kernel32.TerminateProcess.argtypes=(w.HANDLE,w.UINT); kernel32.TerminateProcess.restype=w.BOOL; ok=kernel32.TerminateProcess(kernel32.GetCurrentProcess(),0xC0000409); raise ctypes.WinError(ctypes.get_last_error()) if not ok else SystemExit(99)",
        ready,
    )
}
```

- [ ] **Step 3: Run the focused test to verify GREEN**

Run the exact Task 1 command. Expected: one test passes, the abnormal
termination is `Exit(-1_073_740_791)`, and Job Object accounting reaches zero.

- [ ] **Step 4: Format and rerun the focused test**

Run:

```console
cargo fmt --all
cargo test -p hoimin-cli --lib resource::windows::tests::abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds -- --exact --nocapture
```

Expected: formatting succeeds and the focused test still passes.

### Task 3: Verify scope and quality

**Files:**
- Verify: `crates/hoimin-cli/src/resource/windows.rs`
- Verify: `docs/superpowers/specs/2026-08-04-issue-221-windows-abnormal-root-exit-design.md`
- Verify: `docs/superpowers/plans/2026-08-04-issue-221-windows-abnormal-root-exit.md`

**Interfaces:**
- Consumes: completed issue #221 test and repository verification commands.
- Produces: a committed branch with exact runtime and mutation-applicability evidence.

- [ ] **Step 1: Run all relevant tests and lint checks**

```console
cargo test -p hoimin-cli --all-targets --all-features
cargo test --workspace --all-targets --all-features
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: every command exits zero with no failed tests or warnings.

- [ ] **Step 2: Establish mutation applicability from the final diff**

```console
git diff --unified=0 origin/main...HEAD -- crates/hoimin-cli/src/resource/windows.rs
git diff --name-only origin/main...HEAD
```

If every Rust addition is within `#[cfg(test)] mod tests`, do not run
`cargo-mutants` against test-only helpers. Record that no changed Rust
production candidate exists. If production Rust changed, run a fresh focused
`cargo mutants --workspace --file <changed-production-file>` inventory without
`--iterate` and resolve every survivor before continuing.

- [ ] **Step 3: Check the final diff and acceptance criteria**

```console
git diff --check origin/main...HEAD
git status --short
```

Confirm the test uses real PIDs from atomically published fixture files, never
calls `record_notification`, never mutates `RunState`, keeps the sibling active
through classification, checks exact non-timeout termination, bounds handle
and close, and observes zero active Job Object processes.

- [ ] **Step 4: Commit the implementation**

```console
git add crates/hoimin-cli/src/resource/windows.rs
git commit -m "test: exercise real Windows abnormal Job exit"
```
