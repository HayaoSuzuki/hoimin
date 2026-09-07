# Resource wait isolation implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove bounded OS waits from shared registry critical sections and Tokio dispatcher execution.

**Architecture:** Per-root Linux cleanup gates protect physical deletion; Windows classification retains a process handle across unlocked waits; an owned blocking-operation adapter protects supervisor lifetime.

**Tech Stack:** Rust, Tokio, cgroup v2, Windows Job Objects.

**Spec:** `docs/superpowers/specs/2026-09-08-issue-342-resource-waits-design.md`

## Global constraints

Rust 1.88; no added dependencies; preserve accounting-primary and process-primary errors, retryability, cleanup ownership, and late-notification generation safety. Source comments remain limited to safety or non-obvious contracts.

### Task 1: Linux cleanup isolation

**Files:** `crates/hoimin-cli/src/resource/linux.rs` and Linux verification report.
**Interfaces:** Keep `LinuxSupervisor` and backend public interfaces unchanged; private root cleanup gate and close gate own serialization.

- [x] Add a cleanup callback seam and deterministic tests that pause deletion and probe `run.state.try_lock()` and sibling termination.
- [x] Establish failure on the previous lock scope with Linux execution.
- [x] Reserve root gates outside the registry mutex; set `entry.active = false` before unlocked deletion, remove on success and restore on failure. Close publishes `state.closed = true` before acquiring root gates.
- [x] Verify counter reads skip the deletion/commit gap and accounting errors retain priority while cleanup still runs.
- [x] Run Linux resource regressions and record exact evidence.

### Task 2: Windows exit barrier isolation

**Files:** `crates/hoimin-cli/src/resource/windows.rs`, `.github/workflows/ci.yml`.
**Interfaces:** Preserve `classify_root` result and generation handling; retain `Arc<OwnedHandle>` during the unlocked process wait.

- [x] Add a paused wait test that asserts `state.try_lock().is_ok()` and retains the process handle while a notification removes its registration.
- [x] Replace nonzero completion-port waits inside the mutex with an owned per-process wait outside it; reacquire the lock and drain notifications before choosing the fallback.
- [ ] Check Windows compilation and run resource/process regressions in a dedicated Windows CI job (the existing matrix is Linux-only).

### Task 3: Owned blocking supervisor operations

**Files:** `crates/hoimin-cli/src/process/blocking.rs`, `process/mod.rs`.
**Interfaces:** `BlockingOwner<T>::run` transfers and returns the sole `T`; `finish` destroys it on the blocking pool. No detached clone may race supervisor destruction.

- [x] Add current-thread tests with an externally released blocking operation: dispatcher progress before release, no premature drop on cancelled await, and blocking-thread destruction.
- [x] Observe the synchronous implementation fail the progress test.
- [x] Move the owner through `spawn_blocking`, map join failures to the original effect ID, and route classification/termination/quiescence through the adapter.
- [x] Preserve direct child kill/reap fallback and complete lifecycle guards only after awaited supervisor destruction.
- [x] Run macOS process regressions, workspace all-feature tests, formatting and Clippy.

Review added two regressions: completed-but-unconsumed task output must retain the drop-routing wrapper, and Unix reaping must disarm process-group signaling before the first await. The latter also changes `resource/mod.rs` and `resource/portable.rs`. Windows CI needs platform-specific Clippy allowances in `portable_path.rs` and the reap transition; neither allowance changes runtime behavior.

### Task 4: Review and handoff

- [x] Record the ownership/interleaving audit and actual platform evidence, with model/implementation boundaries explicit.
- [ ] Independently review changes, resolve findings, commit code and all documentation in this worktree.
- [ ] Push and create a technical-only PR closing #342; check CI on the final pushed SHA.
