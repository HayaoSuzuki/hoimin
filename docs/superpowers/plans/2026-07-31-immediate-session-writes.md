# Immediate Session Write Transactions Implementation Plan

> **Issue:** #140 — prevent `SQLITE_BUSY_SNAPSHOT` during session writes

## Goal

Make session write operations reserve SQLite's writer slot before reading any
state that controls later writes. A concurrent writer must either wait through
the configured busy timeout or fail with ordinary `SQLITE_BUSY`; it must not
leave `persist` trying to promote a stale read snapshot.

## Design

Use `rusqlite::TransactionBehavior::Immediate` explicitly for `begin`,
`persist`, and `finish`. The important behavioral fix is in `persist`, whose
current deferred transaction reads the run and result shape before issuing its
first write. In WAL mode, another connection can commit between those steps and
make promotion fail immediately with extended error
`SQLITE_BUSY_SNAPSHOT`, bypassing the busy timeout.

Keep the behavior local to these three write paths. Do not change the
connection-wide default or schema migration transactions; Issue #143 owns the
open-time migration race. Rebase onto Issue #142 first so run ownership and
database writer serialization remain separate layers.

## Task 1: Add a deterministic two-connection regression

**Files:**

- Modify: `crates/hoimin-cli/src/session/mod.rs`

Add a test-only hook immediately after `persist` has completed the reads that
determine its writes. Synchronize it with a two-party barrier:

1. Start `persist` on the owning handler and pause after its reads.
2. On a raw second connection with a zero busy timeout, update a different run
   and attempt to commit.
3. Release `persist`, join the thread, and assert both outcomes.

With the current deferred transaction, the competitor commits and `persist`
fails with `SQLITE_BUSY_SNAPSHOT`. With an immediate transaction, `persist`
already owns the writer reservation, so the competitor receives
`SQLITE_BUSY` and `persist` succeeds. Structure the synchronization so the
paused thread is always released before any assertion can panic.

Run the focused test before production changes and record the expected failure.

## Task 2: Reserve the writer slot for session writes

**Files:**

- Modify: `crates/hoimin-cli/src/session/mod.rs`

Import `TransactionBehavior` and replace the deferred transactions in
`SessionHandler::begin`, `SessionHandler::persist`, and
`SessionHandler::finish` with explicit immediate transactions. Do not alter
readonly lookups, postcondition reads, or schema migration code.

Run the focused regression, all session tests, formatting, Clippy, full
workspace tests, contract-feature tests, and `git diff --check`. Request an
independent review before creating the PR.
