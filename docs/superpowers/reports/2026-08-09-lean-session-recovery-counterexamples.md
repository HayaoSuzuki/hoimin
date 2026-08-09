# Lean Session Recovery Counterexample Ledger

## Resolution: Issue #281

Resolved on 2026-08-09. `SessionHandler::finish` now rejects a handler that
does not own the run before any SQLite transaction begins. The rejected event
returns `session.finish.owner`, preserves the incomplete run row, and leaves
the real owner's lock intact. The strict Lean/Rust case was renamed to
`non_owner_incomplete_finish_is_rejected` and now matches; the complete corpus
has 18 matches and zero mismatches.

The original counterexample below remains as historical evidence.

## Non-owner incomplete finish leaves the run locked

claim:
: An accepted incomplete finish releases run ownership and leaves the run
  immediately resumable.

mode:
: `strict` — public `SessionHandler` methods, two handlers, one valid-schema
  temporary database, and real ownership files.

model boundary:
: Public events are atomic. No test hook or internal transition is used.

finite domain or theorem premises:
: Handler roles `h0`/`h1`, run `r0`, fingerprint `f0`, and five public events.
  The expected behavior comes from the correct Lean transition and generated
  corpus; the mismatch is observed through the public Rust API.

minimal event trace:
: `open:h0 -> open:h1 -> begin:h0:r0:f0 -> finish:h1:r0:false -> load:h1:f0`

intermediate durable and ownership states:
: After `begin`, SQLite contains incomplete `r0` and `h0` owns its file lock.
  `finish` through `h1` succeeds and writes `finished=1, complete=0`. Because
  `h1` has no corresponding entry in its private ownership map, that finish
  removes no lock; `h0` still owns `r0`.

expected observation:
: The finish is accepted with no owner, then `load:h1:f0` is accepted and
  selects `r0`, establishing `h1` as owner.

actual observation:
: The finish is accepted while `h0` remains the effective owner. The following
  load is rejected with `session.resume.active`.

classification:
: Confirmed implementation bug. The persistent state promises resumption while
  the surviving process still holds a lock that prevents it.

impact:
: A caller using multiple live `SessionHandler` instances can strand an
  incomplete run until the original handler drops or the process exits. The
  normal CLI path may use one handler, which limits current reachability, but
  the public API accepts the unsafe cross-handler sequence.

owner question:
: Should `finish` require that its handler owns the run (recommended), or should
  the ownership layer support safe cross-handler release/invalidation?

reproduction command:
: `HOIMIN_SESSION_ORACLE_CASE=non_owner_incomplete_finish_releases_for_resume cargo test -p hoimin-cli --test lean_session_oracle oracle_correspondence -- --exact --nocapture`
