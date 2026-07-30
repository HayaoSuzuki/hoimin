# Async Session Dispatch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run every SQLite open/read/write/commit operation off async scheduler workers.

**Architecture:** A cloneable `SessionDispatcher` serializes the existing synchronous `SessionHandler` behind an `Arc<Mutex<_>>` and invokes it only through `spawn_blocking`. `ShellContext` lazily initializes one dispatcher and awaits its typed methods.

**Tech Stack:** Rust 2024, Tokio blocking pool, rusqlite bundled SQLite.

## Global Constraints

- Preserve the five-second SQLite busy timeout.
- Preserve transaction rollback, primary error codes, and run finality.
- Use only platform-neutral synchronization APIs.
- Do not introduce concurrent access to one rusqlite connection.

---

### Task 1: Reproduce scheduler blocking with a real database lock

**Files:**
- Modify: `crates/hoimin-cli/src/session/mod.rs`

**Interfaces:**
- Produces: a deterministic test that holds `BEGIN IMMEDIATE`, starts a conflicting session operation, and races it with a 200ms deadline

- [ ] **Step 1: Add the real-lock regression and assert the deadline branch wins.**
- [ ] **Step 2: Run the focused test and verify RED against direct synchronous dispatch.**

### Task 2: Add the blocking dispatcher

**Files:**
- Modify: `crates/hoimin-cli/src/session/mod.rs`

**Interfaces:**
- Produces: `SessionDispatcher::open` and async `load`, `lookup`, `begin`, `persist`, `finish`
- Consumes: unchanged `SessionHandler` synchronous transaction methods

- [ ] **Step 1: Implement blocking open and an internal typed call helper.**
- [ ] **Step 2: Map join and poisoned-lock failures to `session.dispatch`.**
- [ ] **Step 3: Add async forwarding methods and verify the lock regression GREEN.**

### Task 3: Route shell session effects through the dispatcher

**Files:**
- Modify: `crates/hoimin-cli/src/shell.rs`

**Interfaces:**
- Replaces: `Option<SessionHandler>` with `Option<SessionDispatcher>`
- Preserves: all `RunEvent` variants and effect IDs

- [ ] **Step 1: Make lazy session initialization async and offloaded.**
- [ ] **Step 2: Await every session effect through the dispatcher.**
- [ ] **Step 3: Run shell and session tests.**

### Task 4: Verify and publish

**Files:**
- Modify: only files above

- [ ] **Step 1: Run format, clippy, and the full workspace test suite.**
- [ ] **Step 2: Commit and push `codex/issue-87`.**
- [ ] **Step 3: Create a PR closing Issue #87 and monitor Linux, macOS, and Windows CI.**
