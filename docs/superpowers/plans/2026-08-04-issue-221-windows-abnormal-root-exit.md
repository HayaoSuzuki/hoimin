# Issue #221 Windows Abnormal Root Exit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove on Windows that a real abnormally terminating root crosses the production Job Object completion port, is classified as its platform exit status rather than a timeout, and is cleaned within six seconds.

**Architecture:** Add one Windows-only behavioral test inside the backend's existing test module so the fixture uses the full `ProcessHandler` path while the test can query the private production Job Object. Keep a real sibling assigned during the crash to prevent `ACTIVE_PROCESS_ZERO` from masking a broken PID-specific abnormal-notification branch.

**Tech Stack:** Rust 2024, Tokio, `windows-sys`, Windows Job Objects, Python process fixtures, Cargo.

## Global Constraints

- Start the real roots through `ProcessHandler` and the production `WindowsBackend`.
- Bypass the Windows virtualenv launcher so the atomically published PID belongs to the assigned abnormal root.
- Atomically publish abnormal-root readiness with a pending file followed by `os.replace`, then verify that PID in the Job Object process list before releasing the crash.
- Expect `ProcessTermination::Exit(-1_073_741_819)`, the signed Windows representation of `STATUS_ACCESS_VIOLATION` (`0xC0000005`), and never `ProcessTermination::Timeout`.
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
file, and an async Job Object polling helper that waits for an expected
`ActiveProcesses` value without changing `RunState`. Add a read-only
fixed-capacity query for the two fixture process IDs.

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

async fn wait_for_job_process_count(backend: &WindowsBackend, expected: u32) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let active = active_process_count(backend.inner.job.raw()).unwrap();
        if active == expected {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn job_process_ids(backend: &WindowsBackend) -> Vec<u32> {
    #[repr(C)]
    struct ProcessIds {
        assigned: u32,
        count: u32,
        values: [usize; 16],
    }
    let mut ids = ProcessIds {
        assigned: 0,
        count: 0,
        values: [0; 16],
    };
    let ok = unsafe {
        windows_sys::Win32::System::JobObjects::QueryInformationJobObject(
            backend.inner.job.raw(),
            windows_sys::Win32::System::JobObjects::JobObjectBasicProcessIdList,
            (&raw mut ids).cast(),
            u32::try_from(std::mem::size_of::<ProcessIds>()).unwrap(),
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0);
    ids.values[..usize::try_from(ids.count).unwrap()]
        .iter()
        .map(|pid| u32::try_from(*pid).unwrap())
        .collect()
}
```

- [ ] **Step 2: Write the test against not-yet-implemented fixture constructors**

Start the native `sleeping_fixture` in a spawned task and wait for one assigned
process. Start `abnormal_fixture`, wait for its atomic readiness PID, verify
that exact PID in the Job Object process list, then publish the release file.
Keep the sibling alive until the abnormal handle returns. Assert the exact exit
status, exactly one active process before close, and zero after close. Enclose
the full async body in a six-second timeout and check the individual elapsed
durations.

```rust
#[tokio::test]
async fn abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds() {
    let temporary = tempfile::tempdir().unwrap();
    let output_dir = Utf8Path::from_path(temporary.path()).unwrap();
    let abnormal_ready = output_dir.join("abnormal-root.ready");
    let abnormal_release = output_dir.join("release-abnormal-root");
    let backend = WindowsBackend::new(&run_limits()).unwrap();
    let handler = Arc::new(ProcessHandler::new(
        ResourceBackend::Windows(backend.clone()),
        output_dir.to_owned(),
    ));
    let _cleanup = HandlerCloseGuard(Arc::clone(&handler));
    let started = Instant::now();

    tokio::time::timeout(Duration::from_secs(6), async {
        let sibling_handler = Arc::clone(&handler);
        let sibling = tokio::spawn(async move {
            sibling_handler.handle(sleeping_fixture(260)).await
        });
        wait_for_job_process_count(&backend, 1).await;

        let handle_started = Instant::now();
        let abnormal_handler = Arc::clone(&handler);
        let fixture_ready = abnormal_ready.clone();
        let fixture_release = abnormal_release.clone();
        let abnormal = tokio::spawn(async move {
            abnormal_handler
                .handle(abnormal_fixture(261, &fixture_ready, &fixture_release))
                .await
        });
        let abnormal_pid = wait_for_ready_pid(&abnormal_ready).await;
        wait_for_job_process_count(&backend, 2).await;
        assert!(job_process_ids(&backend).contains(&abnormal_pid));
        std::fs::write(&abnormal_release, b"abort").unwrap();
        let abnormal = abnormal.await.unwrap().unwrap();
        assert!(handle_started.elapsed() < Duration::from_secs(6));
        assert_eq!(abnormal.termination, ProcessTermination::Exit(-1_073_741_819));
        wait_for_job_process_count(&backend, 1).await;

        let close_started = Instant::now();
        handler.close().unwrap();
        assert!(close_started.elapsed() < Duration::from_secs(6));
        sibling.await.unwrap().unwrap();
        wait_for_job_process_count(&backend, 0).await;
    })
    .await
    .expect("abnormal Job Object fixture exceeded six seconds");
    assert!(started.elapsed() < Duration::from_secs(6));
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
- Consumes: existing `arg()` test helper and the uv-created `.venv/pyvenv.cfg`.
- Produces: `sleeping_fixture(u64) -> RunProcess` and `abnormal_fixture(u64, &Utf8Path, &Utf8Path) -> RunProcess`.

- [ ] **Step 1: Add shared fixture limits and resolve the base interpreter**

Create test-only shared limits. Resolve `home` from `.venv/pyvenv.cfg` and
append `python.exe`; do not use the virtualenv launcher because it makes the
readiness-publishing interpreter a descendant rather than the assigned root.

```rust
fn fixture_limits() -> ProcessLimits {
    ProcessLimits {
        timeout: Duration::from_secs(30),
        max_output_bytes: 64,
        max_memory_bytes: 256 * 1024 * 1024,
        max_processes: 8,
    }
}

fn fixture_python() -> CommandArg {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let configuration = std::fs::read_to_string(workspace.join(".venv/pyvenv.cfg")).unwrap();
    let home = configuration
        .lines()
        .find_map(|line| line.strip_prefix("home = "))
        .expect("virtual environment records its base interpreter");
    let executable = std::path::Path::new(home).join("python.exe");
    assert!(executable.is_file());
    arg(executable)
}
```

- [ ] **Step 2: Add the native sibling and atomic-readiness abnormal fixture**

The sibling runs native `ping.exe` long enough to prevent
`ACTIVE_PROCESS_ZERO`. The abnormal base interpreter writes its actual PID
through `pending` plus `os.replace`, waits for a release file, configures the
exact `kernel32` ctypes signatures, and calls `TerminateProcess` on
`GetCurrentProcess` with `STATUS_ACCESS_VIOLATION`. That status is in
Microsoft's documented list for `JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS`, and
direct termination does not invoke Windows Error Reporting or a JIT debugger.

```rust
fn sleeping_fixture(id: u64) -> RunProcess {
    RunProcess {
        id: EffectId(id),
        worker: None,
        run_id: None,
        mutant_id: None,
        argv: vec![arg("ping.exe"), arg("-n"), arg("30"), arg("127.0.0.1")],
        cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
        limits: fixture_limits(),
    }
}

fn abnormal_fixture(id: u64, ready: &Utf8Path, release: &Utf8Path) -> RunProcess {
    RunProcess {
        id: EffectId(id),
        worker: None,
        run_id: None,
        mutant_id: None,
        argv: vec![
            fixture_python(),
            arg("-c"),
            arg(
                "import ctypes,ctypes.wintypes as w,os,pathlib,sys,time; ready=pathlib.Path(sys.argv[1]); release=pathlib.Path(sys.argv[2]); pending=ready.with_suffix('.pending'); pending.write_text(str(os.getpid())); os.replace(pending,ready);\nwhile not release.exists(): time.sleep(0.005)\nkernel32=ctypes.WinDLL('kernel32',use_last_error=True); kernel32.GetCurrentProcess.argtypes=(); kernel32.GetCurrentProcess.restype=w.HANDLE; kernel32.TerminateProcess.argtypes=(w.HANDLE,w.UINT); kernel32.TerminateProcess.restype=w.BOOL; STATUS_ACCESS_VIOLATION=0xC0000005; kernel32.TerminateProcess(kernel32.GetCurrentProcess(),STATUS_ACCESS_VIOLATION)",
            ),
            arg(ready.as_std_path()),
            arg(release.as_std_path()),
        ],
        cwd: Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap(),
        limits: fixture_limits(),
    }
}
```

- [ ] **Step 3: Run the focused test to verify GREEN**

Run the exact Task 1 command. Expected: one test passes, the abnormal
termination is `Exit(-1_073_741_819)`, and Job Object accounting reaches zero.

- [ ] **Step 4: Format and rerun the focused test**

Run:

```console
cargo fmt --all
cargo test -p hoimin-cli --lib resource::windows::tests::abnormal_runtime_root_crosses_real_job_notification_and_cleans_up_within_six_seconds -- --exact --nocapture
```

Expected: formatting succeeds and the focused test still passes.

- [ ] **Step 5: Prove the abnormal-notification dependency with a semantic RED**

Temporarily remove only `JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS` from the
production `record_notification` match. Leave
`JOB_OBJECT_MSG_EXIT_PROCESS` and
`JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO` unchanged, then run the focused command
from Step 3. Restore the production match exactly and rerun the same command.

Observed on 2026-08-04:

- RED: zero passed and one failed. The fixture reached classification, which
  returned `EffectFailed` with code `process.resource.classify` and message
  `root exit notification timed out`.
- GREEN after restoring the branch: one passed, zero failed, and 177 filtered
  out.
- The final diff against the pre-review implementation contains no production
  notification-match change.

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
