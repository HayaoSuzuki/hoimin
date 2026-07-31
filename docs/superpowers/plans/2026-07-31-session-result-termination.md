# Session Result Termination Preservation Implementation Plan

> **Issue:** #151 — preserve mutant termination in session-backed runs

## Goal

Make a freshly executed mutant emit the same process termination details
whether or not session persistence is enabled. Persist the typed termination
alongside the result so the session path does not silently replace exit,
timeout, OOM, process-limit, or cancellation details with `null`.

## Design

Add an optional `ProcessTermination` to `MutantResult` and make it the single
source for finished report output. Fresh process results store `Some(...)`.
Legacy database rows and results reused without executing a process remain
`None`, preserving the useful provenance distinction between fresh and reused
results.

After Issue #143 is merged, add schema version 3 within its serialized,
immediate migration transaction. Store termination as a constrained kind plus
an optional exit code. Do not infer missing legacy termination from status.
Keep status/termination semantic coherence in Issue #114; this change enforces
only the database representation's shape.

## Task 1: Preserve termination through the core machine

**Files:**

- Modify: `crates/hoimin-core/src/resume.rs`
- Modify: `crates/hoimin-core/src/machine.rs`
- Modify: `crates/hoimin-core/tests/machine.rs`

Add `#[serde(default)] termination: Option<ProcessTermination>` to
`MutantResult` for backward effect-payload compatibility. Populate it from
fresh `ProcessFinished` values and have sessionless, persisted-session, and
stopped-finished output paths read the same typed field. Synthetic and reused
results keep `None`.

Extend the machine regression so both the value sent to `PersistResult` and
the `MutantFinished` emitted after `ResultPersisted` retain the exact
termination.

## Task 2: Add an atomic schema-v3 migration

**Files:**

- Modify: `crates/hoimin-cli/src/session/schema.rs`

Bump the private session schema to version 3. Add nullable
`termination_kind` and `termination_exit_code` columns to `results` through a
new migration applied inside Issue #143's existing immediate transaction.

Use a `CASE`-based `CHECK` constraint so SQLite's three-valued NULL logic
cannot accept inconsistent pairs:

- both columns null for unavailable termination;
- `exit` requires a signed 32-bit exit code;
- timeout, out-of-memory, process-limit, and cancelled kinds require a null
  code;
- unknown kinds are rejected.

Test v1 and v2 upgrades, legacy NULL preservation, idempotent reopen, rollback
of a failed migration, future-version handling, and concurrent migration to
the final version.

## Task 3: Encode and persist typed termination

**Files:**

- Modify: `crates/hoimin-cli/src/session/mod.rs`
- Modify: `crates/hoimin-cli/tests/session_handler.rs`

Encode `ProcessTermination` into the database-specific kind/code pair and
decode it for contract postcondition reads. Reject unknown kinds,
inconsistent kind/code pairs, and out-of-range exit codes as corrupt data.
Keep the database representation independent from the public JSON enum
encoding.

Cover every termination variant plus `None`, inspect raw stored columns, and
verify typed contract round trips. Use status-compatible fixture values so the
later Issue #114 coherence checks can be added without rewriting these tests.

## Task 4: Prove session/sessionless report parity

**Files:**

- Modify: `crates/hoimin-cli/tests/run_e2e.rs`

Run the same deterministic mutant once without a session and once with a fresh
session. Assert identical stable mutant fields and an exact concrete
termination in both reports. Normalize invocation-specific `run_id`,
`elapsed_ms`, and `output.token`; continue comparing retained/observed output,
candidate, status, resource mode, and termination.

Also assert that a subsequently reused result reports null termination because
no process ran in that invocation.

Run core machine, session schema/handler, and end-to-end suites, formatting,
Clippy, full workspace tests, contract-feature tests, and `git diff --check`.
Request an independent review before creating the PR.
