# Session Run Ownership Implementation Plan

> **Issue:** #142 — prevent resuming a session run that another process is executing

## Goal

Ensure an incomplete session row can be resumed only when no live process owns
that run. A competing resume must fail immediately with a clear typed error,
while process exit or crash must release ownership without a lease timeout.

## Design

Use a nonblocking, run-scoped advisory lock in a sidecar lock file. Keep the
lock handle in `SessionHandler` for the lifetime of the active run. Kernel
release on handle drop or process death makes crashed runs immediately
resumable without PID-reuse checks, heartbeat tasks, wall-clock leases, or a
schema migration.

Derive the sidecar name from the canonical session database path and a stable
hash of `run_id`; never place raw run IDs in filesystem paths. Do not lock the
SQLite database or WAL files and do not unlink lock files after use, because
unlinking a live Unix lock can split ownership across inodes.

`load` selects the newest incomplete row and then attempts its run lock. Lock
contention returns `session.resume.active` rather than skipping to an older
run. After acquiring the lock, re-read the row to close the select/finish race.
`begin` acquires ownership before committing a new row. `finish` releases the
lock only after its transaction commits; both complete and intentionally
incomplete finishes then permit the documented next action.

## Task 1: Add ownership regressions

**Files:**

- Modify: `crates/hoimin-cli/tests/session_handler.rs`
- Modify: session unit tests as appropriate

Add two-connection tests against a preconfigured database:

1. An owner begins a run and a second handler immediately receives
   `session.resume.active` when loading the same fingerprint.
2. Owner `finish(false)` or owner drop releases the lock and permits the second
   handler to load the same `run_id`.
3. Owner `finish(true)` releases ownership and removes the run from incomplete
   selection.
4. Different runs/fingerprints can be owned concurrently.

Add a bounded subprocess crash test. A child test process opens and begins a
run, signals readiness, and waits. The parent verifies contention, terminates
the child, waits for process death, then verifies immediate resume. Avoid a
full mutation worker in this fixture so termination cannot orphan subprocesses.

Record the existing behavior where the second connection loads the live row
before implementing the lock.

## Task 2: Implement a cross-platform advisory lock

**Files:**

- Modify: `crates/hoimin-cli/Cargo.toml` if a focused lock dependency is chosen
- Add/Modify: session lock implementation files

Provide nonblocking acquire and RAII release on supported Unix and Windows
hosts. Prefer a small, well-audited file-lock abstraction compatible with the
workspace MSRV; otherwise use the already-supported native platform APIs
behind one internal interface.

Create the lock directory/file safely beside the canonical database, hash the
run ID for the filename, and retain the open locked file. Map contention to a
dedicated session error code and preserve unrelated I/O diagnostics.

## Task 3: Bind lock lifetime to session lifecycle

**Files:**

- Modify: `crates/hoimin-cli/src/session/mod.rs`
- Modify: CLI/session dispatcher tests if needed

Acquire before `begin` commits and when `load` claims an incomplete row.
Re-check the selected row after acquisition. Store ownership by run ID in the
handler so tests and future multi-run use cannot accidentally overwrite a live
handle.

Release only after successful `finish` commit. Ensure failed begin/finish
transactions keep ownership and database state coherent, while handler drop
and process crash release the OS lock automatically.

Run focused two-connection and subprocess tests, the session handler and CLI
suites, formatting, Clippy, full workspace tests, contract-feature tests, and
`git diff --check`. Request an independent review before creating the PR.
