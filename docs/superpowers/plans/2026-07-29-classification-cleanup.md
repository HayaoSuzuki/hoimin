# Classification Failure Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Always terminate supervised descendants before output draining, even when normal-exit resource classification fails.

**Architecture:** Replace the conditional classification/termination chain with one helper that executes both operations and composes their errors. Add a deterministic portable-backend classification fault seam for cross-platform integration coverage.

**Tech Stack:** Rust 2024, Tokio process tests, platform resource supervisors.

## Global Constraints

- Preserve classification as the primary error when classification and termination both fail.
- Attempt tree termination before output draining on every successfully attached normal-exit path.
- Keep successful classification and cancellation/timeout behavior unchanged.
- Follow test-driven development and verify the failure before production changes.

---

### Task 1: Reproduce classification cleanup failure

**Files:**
- Modify: `crates/hoimin-cli/src/resource/portable.rs`
- Modify: `crates/hoimin-cli/src/resource/mod.rs`
- Modify: `crates/hoimin-cli/tests/process_handler.rs`

**Interfaces:**
- Produces: hidden portable classification-failure constructors and `ProcessSupervisor::classify` fault
- Consumes: existing portable process-group/Job Object termination

- [ ] **Step 1: Add the test-only classification fault counter**

Add `classification_failures: Arc<AtomicU8>` to `PortableBackend` and `PortableSupervisor`. Production/default construction initializes zero. Hidden test constructors initialize one classification failure, optionally with one termination failure.

- [ ] **Step 2: Make portable classification consume the injected fault**

Expose a portable supervisor method:

```rust
fn classify(
    &mut self,
    termination: ProcessTermination,
) -> Result<ProcessTermination, ResourceError>
```

It returns an injected `ResourceError::io("classify portable process", ...)` once, otherwise the original termination. Route `ProcessSupervisor::classify` through it.

- [ ] **Step 3: Add failing descendant integration tests**

Run a root that spawns a long-lived descendant inheriting pipes, writes the PID, and exits. With classification failure injected, assert `process.resource.classify`, descendant termination, and cleanup elapsed below 900 ms. With both faults injected, also assert the classification message contains the appended supervisor termination failure.

- [ ] **Step 4: Verify RED**

```console
cargo test -p hoimin-cli --test process_handler classification_failure -- --nocapture
```

Expected: the descendant/output path takes the one-second grace because explicit termination is skipped; cleanup-composition assertion also fails.

### Task 2: Always terminate after classification

**Files:**
- Modify: `crates/hoimin-cli/src/process/mod.rs`

**Interfaces:**
- Consumes: `ProcessSupervisor::classify`, `terminate_supervised`
- Produces: `classify_and_terminate(...) -> Result<ProcessTermination, EffectFailed>`

- [ ] **Step 1: Implement the helper**

Call classification and termination unconditionally, then combine:

```rust
match (classification, termination) {
    (Ok(classified), Ok(())) => Ok(classified),
    (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
    (Err(mut primary), Err(cleanup)) => {
        append_cleanup_failure(
            &mut primary,
            "supervised termination also failed",
            &cleanup,
        );
        Err(primary)
    }
}
```

- [ ] **Step 2: Replace the `and_then` normal-exit chain**

Map classification errors with the existing `process.resource.classify` context, invoke the helper, and leave wait-error/cancel/timeout branches unchanged.

- [ ] **Step 3: Verify GREEN**

```console
cargo test -p hoimin-cli --test process_handler classification_failure -- --nocapture
cargo test -p hoimin-cli --test process_handler
```

Expected: all focused and process-handler tests pass.

### Task 3: Full verification and PR

**Files:**
- Verify all modified sources, tests, design, and plan.

**Interfaces:**
- Produces: branch and PR closing issue #54

- [ ] **Step 1: Run formatting, lint, workspace, contract, and Python gates**

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --features contracts
uv run --frozen python -m unittest discover -s tests -p 'test_*.py' -v
```

- [ ] **Step 2: Review error precedence and platform compilation**

Confirm classification remains primary, termination always runs before output draining, and portable production construction remains fault-free.

- [ ] **Step 3: Commit, push, and open the PR**

```console
git add docs/superpowers/specs/2026-07-29-classification-cleanup-design.md \
  docs/superpowers/plans/2026-07-29-classification-cleanup.md \
  crates/hoimin-cli/src/process/mod.rs crates/hoimin-cli/src/resource/mod.rs \
  crates/hoimin-cli/src/resource/portable.rs crates/hoimin-cli/tests/process_handler.rs
git commit -m "fix: clean up after classification failure"
git push -u origin fix/issue-54-classification-cleanup
gh pr create --base main --head fix/issue-54-classification-cleanup
```
