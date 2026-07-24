# Reap After Termination Errors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure cancellation and timeout always perform bounded root reaping and a final process-tree cleanup attempt when supervisor termination fails.

**Architecture:** Add a one-shot portable termination-failure seam, then route cancellation and timeout through one asynchronous cleanup helper. The helper preserves the first supervisor error, attempts direct root kill, retries tree termination once, waits explicitly for the root, and appends later cleanup failures without changing error precedence.

**Tech Stack:** Rust 1.85, Tokio processes/timeouts, portable Unix process groups and Windows Job Objects, existing process integration fixtures, cargo-mutants 27.1.0.

## Global Constraints

- Work only in branch `fix/issue-24-reap-after-termination-errors` and its dedicated worktree.
- Keep the first `process.resource.terminate` error primary.
- Attempt direct root kill, one tree-termination retry, and bounded root wait even after earlier cleanup failures.
- Preserve cancellation/timeout selection and process-vs-output error precedence.
- Do not change Issue #23's pre-attachment Windows containment boundary.
- Use TDD and limit mutation testing to the new cleanup, bounded-reap, and error-aggregation functions.

---

### Task 1: Add a One-Shot Portable Termination-Failure Seam

**Files:**
- Modify: `crates/hoimin-cli/src/resource/portable.rs`
- Modify: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Produces: hidden public `PortableBackend::for_tests_with_termination_failure() -> Self`.
- Carries: one shared `Arc<AtomicU8>` failure budget from backend clones into each prepared supervisor.
- Preserves: `PortableBackend::new` and `PortableBackend::for_tests` behavior.

- [ ] **Step 1: Write a failing real-handler cancellation test**

Add a helper:

```rust
fn portable_handler_with_termination_failure(output_dir: &Utf8Path) -> ProcessHandler {
    ProcessHandler::new(
        ResourceBackend::Portable(
            PortableBackend::for_tests_with_termination_failure(),
        ),
        output_dir.to_owned(),
    )
}
```

Add a cancellation regression modeled on
`cancellation_terminates_descendants`. It must spawn a descendant, wait for its
PID file, cancel, and initially assert only the injected primary error:

```rust
let failure = event.expect_err("injected supervisor failure remains observable");
assert!(matches!(
    failure.failure,
    EffectFailure::Io { ref code, ref message, .. }
        if code == "process.resource.terminate"
            && message.contains("injected portable termination failure")
));
```

Keep the `FixtureChildGuard`; the descendant-liveness assertion is added in
Task 2 after cleanup is implemented.

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p hoimin-cli --test process_handler \
  portable::cancellation_reports_injected_termination_failure -- --exact
```

Expected: compile failure because the test constructor does not exist.

- [ ] **Step 3: Implement the one-shot failure state**

In `portable.rs`, import:

```rust
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
```

Extend both structs:

```rust
pub struct PortableBackend {
    diagnostic: Option<String>,
    termination_failures: Arc<AtomicU8>,
}

pub(crate) struct PortableSupervisor {
    // existing fields
    termination_failures: Arc<AtomicU8>,
}
```

Initialize `termination_failures` to zero in ordinary constructors and add:

```rust
#[doc(hidden)]
#[must_use]
pub fn for_tests_with_termination_failure() -> Self {
    Self {
        diagnostic: None,
        termination_failures: Arc::new(AtomicU8::new(1)),
    }
}
```

Pass the shared counter through `prepare` and `PortableSupervisor::new`.
At the start of `terminate`, after the existing `self.terminated` check:

```rust
if self
    .termination_failures
    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
        remaining.checked_sub(1)
    })
    .is_ok()
{
    return Err(ResourceError::io(
        "terminate portable supervisor",
        io::Error::other("injected portable termination failure"),
    ));
}
```

This fails before signalling the group/job and leaves `terminated` false so a
retry can perform real cleanup.

- [ ] **Step 4: Run the focused test and verify GREEN for injection**

Run the same focused command. Expected: the handler returns
`process.resource.terminate` containing the injected message. The test may
still rely on drop cleanup; Task 2 makes cleanup ordering explicit.

- [ ] **Step 5: Run existing portable tests**

```bash
cargo test -p hoimin-cli --test process_handler portable::
```

Expected: all pass, proving ordinary `for_tests` behavior is unchanged.

- [ ] **Step 6: Commit the deterministic seam**

```bash
git add crates/hoimin-cli/src/resource/portable.rs \
  crates/hoimin-cli/tests/process_handler.rs
git commit -m "test: inject portable termination failures"
```

---

### Task 2: Make Tree Retry and Root Reaping Unconditional

**Files:**
- Modify: `crates/hoimin-cli/src/process/mod.rs`
- Modify: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Produces: `terminate_and_reap(id, supervisor, child) -> Result<(), EffectFailed>`.
- Produces: strict two-phase `wait_after_termination(id, child) -> Result<(), EffectFailed>`.
- Preserves: `terminate_supervised`, `combine_process_and_output`, and public handler signatures.

- [ ] **Step 1: Strengthen cancellation and add timeout RED tests**

In the Task 1 cancellation test, assert:

```rust
assert!(wait_until_process_stops(child_pid).await);
```

Add the timeout equivalent using
`portable_handler_with_termination_failure`, a one-second process timeout, and
the existing descendant PID fixture. It must expect the injected primary error
and assert the descendant stops.

Add a timing assertion that each handler completes within
900 milliseconds after cancellation is requested or the timeout fires. The
current error branch waits for the one-second output grace before supervisor
drop retries termination; the explicit cleanup path stops the process tree and
closes descendant pipes before that grace expires. Record an `Instant`
immediately before calling `cancellation.cancel()` in the cancellation helper,
and subtract the requested process timeout from total elapsed time in the
timeout test.

- [ ] **Step 2: Run both regressions and verify RED**

```bash
cargo test -p hoimin-cli --test process_handler \
  portable::cancellation_reaps_after_injected_termination_failure -- --exact
cargo test -p hoimin-cli --test process_handler \
  portable::timeout_reaps_after_injected_termination_failure -- --exact
```

Expected: both fail the sub-900-millisecond cleanup assertion against the
current branch. The initial termination error leaves descendant output pipes
open until the one-second output-drain grace expires and supervisor drop
finally retries termination. Do not replace this assertion with a
multi-second bound that would accept destructor-only cleanup.

- [ ] **Step 3: Add primary-error aggregation helpers**

Add:

```rust
fn failure_message(failure: &EffectFailed) -> &str {
    match &failure.failure {
        EffectFailure::Io { message, .. } | EffectFailure::Other { message, .. } => message,
        _ => "non-process cleanup failure",
    }
}

fn append_cleanup_failure(
    primary: &mut EffectFailed,
    label: &str,
    cleanup: &EffectFailed,
) {
    let detail = failure_message(cleanup);
    match &mut primary.failure {
        EffectFailure::Io { message, .. } | EffectFailure::Other { message, .. } => {
            message.push_str("; ");
            message.push_str(label);
            message.push_str(": ");
            message.push_str(detail);
        }
        _ => unreachable!("process cleanup produces only I/O or other failures"),
    }
}
```

Add a unit test using three `EffectFailed` values. Assert the original ID,
code, operation, and leading message are unchanged and both cleanup labels
appear in execution order.

- [ ] **Step 4: Upgrade bounded root wait**

Refactor `wait_after_termination`:

```rust
pub(crate) async fn wait_after_termination(
    id: EffectId,
    child: &mut Child,
) -> Result<(), EffectFailed> {
    if let Ok(result) =
        tokio::time::timeout(POST_TERMINATION_GRACE, child.wait()).await
    {
        return result.map(|_| ()).map_err(|error| {
            io_failure(id, "process.wait", "wait after process termination", None, &error)
        });
    }

    let kill_failure = child.start_kill().err().map(|error| {
        io_failure(id, "process.kill", "kill root after wait timeout", None, &error)
    });
    let wait_result =
        tokio::time::timeout(POST_TERMINATION_GRACE, child.wait()).await;
    let wait_failure = match wait_result {
        Ok(Ok(_)) => None,
        Ok(Err(error)) => Some(io_failure(
            id,
            "process.wait",
            "wait after root kill",
            None,
            &error,
        )),
        Err(_) => Some(EffectFailed::other(
            id,
            "process.wait.timeout",
            "timed out reaping root after kill",
        )),
    };

    match (kill_failure, wait_failure) {
        (None, None) => Ok(()),
        (Some(error), None) | (None, Some(error)) => Err(error),
        (Some(mut primary), Some(cleanup)) => {
            append_cleanup_failure(&mut primary, "root wait also failed", &cleanup);
            Err(primary)
        }
    }
}
```

This second wait is mandatory; `start_kill` alone is not a reap.

- [ ] **Step 5: Implement shared termination cleanup**

Add:

```rust
async fn terminate_and_reap(
    id: EffectId,
    supervisor: &mut ProcessSupervisor,
    child: &mut Child,
) -> Result<(), EffectFailed> {
    match terminate_supervised(id, supervisor, true) {
        Ok(()) => wait_after_termination(id, child).await,
        Err(mut primary) => {
            let root_kill = child.start_kill().err().map(|error| {
                io_failure(
                    id,
                    "process.kill",
                    "kill root after supervisor termination failure",
                    None,
                    &error,
                )
            });
            let tree_retry = terminate_supervised(id, supervisor, true).err();
            let root_wait = wait_after_termination(id, child).await.err();

            for (label, cleanup) in [
                ("direct root kill failed", root_kill),
                ("supervisor termination retry failed", tree_retry),
                ("root reap failed", root_wait),
            ] {
                if let Some(cleanup) = cleanup {
                    append_cleanup_failure(&mut primary, label, &cleanup);
                }
            }
            Err(primary)
        }
    }
}
```

Replace both cancellation and timeout matches with:

```rust
ProcessSelection::Cancelled => terminate_and_reap(
    id,
    &mut supervisor,
    &mut child,
)
.await
.map(|()| ProcessTermination::Cancelled),
ProcessSelection::Timeout => terminate_and_reap(
    id,
    &mut supervisor,
    &mut child,
)
.await
.map(|()| ProcessTermination::Timeout),
```

- [ ] **Step 6: Run regressions and full process suites**

```bash
cargo test -p hoimin-cli --test process_handler \
  portable::cancellation_reaps_after_injected_termination_failure -- --exact
cargo test -p hoimin-cli --test process_handler \
  portable::timeout_reaps_after_injected_termination_failure -- --exact
cargo test -p hoimin-cli --test process_handler
cargo test -p hoimin-cli --test run_e2e
```

Expected: all pass. Both injected cases return the first supervisor error,
observe a cleanup retry, and leave no live descendant.

- [ ] **Step 7: Commit unconditional cleanup**

```bash
git add crates/hoimin-cli/src/process/mod.rs \
  crates/hoimin-cli/tests/process_handler.rs
git commit -m "fix: reap roots after termination errors"
```

---

### Task 3: Mutation-Test the Critical Cleanup Boundary

**Files:**
- Test: `crates/hoimin-cli/src/process/mod.rs`
- Test: `crates/hoimin-cli/tests/process_handler.rs`
- Local only: `mutants.out/`

**Interfaces:**
- Exercises: `terminate_and_reap`, `wait_after_termination`, `append_cleanup_failure`.
- Produces: focused mutation evidence without a workspace-wide mutant inventory.

- [ ] **Step 1: List the exact mutation scope**

```bash
cargo mutants --workspace \
  --file crates/hoimin-cli/src/process/mod.rs \
  --re '(terminate_and_reap|wait_after_termination|append_cleanup_failure)' \
  --list
```

Expected: only the three named functions and their nested expressions. Narrow
the regex if any unrelated function appears.

- [ ] **Step 2: Clear ignored prior output**

```bash
rm -rf mutants.out mutants.out.old
```

Expected: only ignored cargo-mutants diagnostics are removed.

- [ ] **Step 3: Run a fresh focused mutation test**

```bash
cargo mutants --workspace --jobs 4 \
  --file crates/hoimin-cli/src/process/mod.rs \
  --re '(terminate_and_reap|wait_after_termination|append_cleanup_failure)' \
  -- --test process_handler
```

Do not use `--iterate` or `--in-place`. The focused process suite is selected
to keep the runtime bounded.

- [ ] **Step 4: Resolve outcomes**

```bash
test -f mutants.out/missed.txt && sed -n '1,240p' mutants.out/missed.txt
test -f mutants.out/timeout.txt && sed -n '1,240p' mutants.out/timeout.txt
test -f mutants.out/unviable.txt && sed -n '1,240p' mutants.out/unviable.txt
```

Every viable survivor requires the smallest new behavior test and another
fresh focused run. A genuine equivalent exception requires an exact anchored
`exclude_re` with a reason in `.cargo/mutants.toml`; never exclude an entire
function.

- [ ] **Step 5: Assert acceptance and commit mutation-driven tests**

```bash
test ! -s mutants.out/missed.txt
test ! -s mutants.out/timeout.txt
git status --short
```

If tests changed:

```bash
git add crates/hoimin-cli/src/process/mod.rs \
  crates/hoimin-cli/tests/process_handler.rs
git commit -m "test: strengthen termination cleanup coverage"
```

Do not create an empty commit when no viable mutant survives.

---

### Task 4: Verify, Review, and Open the Dedicated Pull Request

**Files:**
- Verify only: Issue #24 design, plan, process/resource implementation, and tests.

**Interfaces:**
- Consumes: Tasks 1–3 commits and mutation evidence.
- Produces: a reviewed PR that closes GitHub Issue #24.

- [ ] **Step 1: Provision the controlled Python environment**

```bash
uv sync --frozen
```

Expected: `.venv/bin/python` or `.venv/Scripts/python.exe` exists inside this
worktree for the full Rust integration suite.

- [ ] **Step 2: Run all local gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff main...HEAD --check
git status --short
```

Expected: all commands succeed and the worktree is clean.

- [ ] **Step 3: Request independent whole-branch review**

Review `main...HEAD` against Issue #24, the design, and this plan. Critical and
Important findings must be fixed and all gates rerun.

- [ ] **Step 4: Push and create the PR**

```bash
git push -u origin fix/issue-24-reap-after-termination-errors
gh pr create --repo tokyogas-tech/hoimin --base main \
  --head fix/issue-24-reap-after-termination-errors \
  --title "fix: reap roots after termination errors" \
  --body-file .superpowers/issue-24-pr-body.md
```

The body must contain `Fixes #24`, summarize primary-error preservation and
bounded cleanup, and list tests plus focused mutation outcomes. Preserve the
worktree for PR feedback and allow normal code-change CI to run.
