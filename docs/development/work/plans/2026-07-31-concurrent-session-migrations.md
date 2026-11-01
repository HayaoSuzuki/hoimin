# Concurrent Session Migration Implementation Plan

> **Issue:** #143 — make session schema migration safe under concurrent opens

## Goal

Allow multiple processes to open the same fresh or legacy session database
concurrently without racing non-idempotent schema migrations. Preserve atomic
rollback, existing data, future-version rejection, and the fast path for an
already-current database.

## Design

Keep the initial autocommit `user_version` read as a fast path. If migration
may be required, start a `TransactionBehavior::Immediate` transaction and
re-read `user_version` inside it before making any migration decision. The
writer reservation serializes competing migrators; the second opener waits
through the configured busy timeout, then observes the committed current
version and performs no DDL.

Apply all required migrations inside that one transaction and commit once.
Keep non-idempotent DDL rather than adding `IF NOT EXISTS`, so a version/schema
mismatch remains a visible integrity failure. Configure busy timeout, foreign
keys, and WAL before entering the migration transaction.

This issue changes only schema initialization. Issue #140 separately changes
runtime session write transactions.

## Task 1: Add deterministic concurrent migration regressions

**Files:**

- Modify: `crates/hoimin-cli/src/session/schema.rs`

Add a test-only observer around the authoritative version read so two
connections can be synchronized without sleeps. First demonstrate the current
race by pausing two threads after both autocommit reads.

Cover:

1. Two connections concurrently configure a fresh database and both succeed.
2. Two connections concurrently upgrade a version-1 database, both succeed,
   preserve the legacy data, and leave version 2.

Keep observer/barrier use panic-safe so neither thread remains blocked when an
assertion fails. Open connections within their worker threads and drop them
before temporary-directory cleanup for Windows compatibility.

## Task 2: Serialize and re-check migrations

**Files:**

- Modify: `crates/hoimin-cli/src/session/mod.rs`
- Modify: `crates/hoimin-cli/src/session/schema.rs`

Allow schema configuration to borrow the connection mutably. For databases
that may need migration:

1. Begin an immediate transaction.
2. Re-read and validate `user_version` inside the transaction.
3. Apply each required migration through functions that accept the existing
   transaction and neither start nor commit their own transactions.
4. Commit once after the complete migration chain.

Retain the current-version fast path outside a write transaction. Re-check a
future version inside the locked transaction and return the typed
`FutureVersion` error without changing the database.

Run the focused fresh/v1 concurrency tests, existing rollback and schema
tests, session handler tests, formatting, Clippy, full workspace tests,
contract-feature tests, and `git diff --check`. Request an independent review
before creating the PR.
