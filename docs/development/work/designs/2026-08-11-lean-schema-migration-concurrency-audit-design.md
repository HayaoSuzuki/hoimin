# Lean Schema Migration Concurrency Audit Design

## Goal

Audit Hoimin's SQLite schema configuration contract under concurrent opens:
writer serialization, version re-check after acquiring the writer reservation,
atomic rollback, legacy-data preservation, current-schema completion, and
future-version precedence.

## Correspondence worksheet

| Premise or observation | Lean representation | Production configuration | Public observation | Mode |
| --- | --- | --- | --- | --- |
| Two opens start against a fresh database | two actors observe `fresh` | barrier releases two `SessionHandler::open` calls | both return success; final `user_version = 3` | `strict` |
| Two opens start against a v1 database | two actors observe `v1` | copy the owned schema-v1 golden fixture, then release two opens | both return success; final version is 3 and marker data remains | `strict` |
| Both actors definitely read the old version before either locks | both actor phases contain the same old observation | private `configure_observed` can force this, but public API cannot | model result only | `model-only` |
| Migration failure rolls back committed state | `fail` while a transaction has a staged version | malformed owned v1 SQLite fixture causes migration DDL failure | open fails; committed version and marker schema remain unchanged | `strict` |
| Future schema is rejected first | initial `future` version | public open of a database with `user_version = 4` and a marker table | typed future-version error; marker and version remain | `strict` |

The adapter does not claim which public thread read which intermediate version.
Exact stale-read scheduling remains model-only even though the owned Rust unit test
independently forces it through a private observer.

## Lean model

Use two actors and a single database state. An actor can observe the autocommit
version, acquire the immediate writer transaction, advance the staged migration
one schema version, commit, fail, or finish through the current fast path. The
correct `begin` transition ignores the earlier observation and re-reads committed
version while holding the sole lock.

The model represents schema versions `fresh`, `v1`, `v2`, `current`, and `future`.
Staged versions are transaction-local and become committed only on `commit`.
Legacy marker data is never rewritten. Invalid events are no-ops, making arbitrary
event traces total without inventing production failures.

Kernel-checked theorems establish:

- every step and trace preserves the state invariant;
- a successful actor implies a current committed schema;
- legacy marker data is preserved for every trace;
- an injected migration failure leaves committed version and data unchanged;
- the single optional lock cannot represent two simultaneous owners.

## Bounded audit and sensitivity

The executable explores the deduplicated state graph for two actors only, through
depth 9, sufficient for the longest fixed fresh migration schedule. It records a
hard state-count ceiling and fails rather than increasing depth. Fixed witnesses
must distinguish:

- atomicity: a broken failure publishes a staged partial version;
- uniqueness/idempotency: a broken begin trusts the stale outer read, so the
  second non-idempotent migrator fails after the first commits;
- boundary/precedence: a broken begin treats a future version as migratable and
  returns a generic migration failure instead of the typed future-version result.

All imported proofs use `maxHeartbeats 100000`. Executable Lean commands run one at
a time under a 20-second external deadline. A timeout or memory symptom stops the
audit; bounds are not raised.

## Production adapter and deliverables

Generate a deterministic JSONL corpus from Lean. A Rust integration test parses it
strictly, runs only strict cases through public `SessionHandler::open`, and labels
setup, panic, timeout, or parsing failures as infrastructure errors rather than
semantic mismatches. Add the model, proofs, cases, executable, corpus, adapter,
design, implementation plan, and final audit report in one isolated worktree.
