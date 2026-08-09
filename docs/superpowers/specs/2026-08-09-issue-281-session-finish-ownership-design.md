# Issue #281 Session Finish Ownership Design

## Context

The Lean session-recovery audit found that one `SessionHandler` can finish a
run whose process lock is owned by another handler. The database is updated,
but the real owner retains its lock. The persisted state and process-local
ownership state therefore describe different lifecycle states.

The confirmed trace is:

1. `h0` and `h1` open the same database.
2. `h0` begins `r0` and owns its lock.
3. `h1` calls `finish(r0, incomplete)`.
4. The database records a finished incomplete run, while `h0` still owns it.
5. `h1` cannot resume the run because the lock remains active.

## Decision

`SessionHandler::finish` must reject a caller that does not own the requested
run. The check happens before opening the SQLite transaction, so a rejected
finish cannot modify durable state. The failure is an existing
`EffectFailure::SessionDatabase` with the new stable code
`session.finish.owner`, operation `validate session lifecycle`, and a message
that the handler does not own the run.

An owner may finish a run as complete or incomplete. A successful finish
commits first and then removes the caller's in-memory ownership, which
preserves the current release behavior. Because ownership is the authorization
to finish, a later call through that handler is rejected until it explicitly
loads and owns the run again. This deliberately tightens the earlier
incomplete-finish idempotency claim: retaining a reusable authorization after
releasing the lock would let a stale handler finish a run claimed by another
handler.

## Alternatives considered

- **Reject the non-owner before persistence (selected).** This keeps one
  handler responsible for both the lock and the durable transition and is the
  smallest change that restores the invariant.
- **Transfer or release another handler's ownership.** This would require a
  cross-handler ownership registry and safe lock transfer. It expands the
  synchronization surface without a current product requirement.
- **Allow the database update and tolerate a stale lock.** This preserves the
  observed inconsistency and can make a resumable row temporarily impossible
  to resume.

## Formal contract

The Lean session model gains a `notOwner` rejection. `finishRun` rejects when
`owns state run handler` is false and otherwise performs the existing durable
finish plus ownership release. The strict corpus keeps the two-handler trace
but renames it to describe rejection. The former incomplete-finish idempotency
case becomes `released_handler_cannot_finish_again`. The Rust adapter must have
no reviewed mismatch exemption after the repair.

## Tests and acceptance

- A public `SessionHandler` regression test observes a rejected non-owner
  finish with code `session.finish.owner`.
- The test verifies the owner remains active and that dropping the owner makes
  the still-incomplete run resumable, proving the rejected call did not mutate
  durable state.
- Existing finish coverage verifies that a successful finish removes the
  handler's authority and a later finish is rejected with
  `session.finish.owner`.
- The Lean model builds, its corpus is regenerated and fresh, sensitivity
  remains effective, and all 18 strict cases match Rust.
- Session integration tests and the Rust workspace test suite pass.

## Scope

This change governs `finish` only. Ownership requirements for lookup or result
persistence are not changed because they were not part of the confirmed
counterexample and need an independent audit and design decision.
