# Lean schema migration concurrency audit report

## Result

No Hoimin implementation mismatch was found in the audited SQLite schema
migration concurrency contract. The Lean state machine, unbounded invariant
proofs, bounded two-actor state audit, fixed broken-model witnesses, generated
corpus, public `SessionHandler::open` adapter, and existing deterministic schema
unit tests agree.

This is scoped evidence, not a proof of rusqlite or SQLite. Lean proves the
independent transition model. The Rust adapter checks only the public premises
and observations listed below; exact private transaction timing is not inferred
from a successful public run.

## Scope and correspondence

The audit owns the current-version fast path, immediate writer serialization,
version re-check after writer acquisition, cumulative v0/v1/v2-to-v3 migration,
atomic rollback, legacy marker preservation, and future-version precedence.

| Case | Mode | Production premise and observation | Result |
| --- | --- | --- | --- |
| `concurrent_fresh_open` | `strict` | two public opens released together against a fresh path; both succeed and final `user_version` is 3 | match |
| `concurrent_v1_upgrade` | `strict` | two public opens released together against the owned v1 golden database; both succeed, final version is 3, and the diagnostic marker remains | match |
| `forced_stale_reread` | `model-only` | both model actors definitely retain v1 observations before either lock; public API cannot force or expose this timing | pass in model |
| `failed_v1_upgrade_rollback` | `strict` | public open of a malformed owned v1 fixture fails during migration; version 1 and its marker row remain | match |
| `future_version_precedence` | `strict` | public open of version 4 returns the typed future-version error; version and marker table remain | match |

The public concurrency adapter synchronizes invocation start but does not claim
that both connections completed the initial autocommit read before either
acquired SQLite's writer reservation. The existing owned unit tests independently
force that stronger schedule through private `configure_observed`; the corpus
correctly keeps it model-only.

## Lean evidence

The model has two actors, one optional writer owner, one transaction-local staged
version, committed versions `fresh`, `v1`, `v2`, `current`, and `future`, and an
abstract legacy marker. With local `maxHeartbeats 100000`, Lean proves:

- `step_preserves` and `run_preserves`: every correct transition and arbitrary
  trace preserve the success/current invariant;
- `successful_actor_has_current_schema`: any successful actor observes a
  committed current schema;
- `step_preserves_legacy_data` and `run_preserves_legacy_data`: migration,
  interleaving, failure, and invalid events never lose the marker;
- `failure_rolls_back_committed_state`: an injected transaction failure leaves
  the committed version unchanged;
- `lock_has_at_most_one_owner`: the state cannot represent two writer owners;
- `step_preserves_future_version` and `run_preserves_future_version`: correct
  execution never overwrites a future schema.

The executable separately explores the deduplicated state graph for exactly two
actors, ten event labels, and depth 9. It reaches 87 states under a hard ceiling
of 128. Invalid events are total no-ops, so the depth-9 layer contains every
state reachable at any shorter depth. This finite check supplements rather than
replaces the unbounded theorems.

## Refutation sensitivity

All three applicable broken families were distinguished before corpus output:

| Family | Broken behavior | Fixed witness and intermediate state | Detected |
| --- | --- | --- | --- |
| atomicity/transactionality | publish the staged version when migration fails | committed v1; begin; stage v2; fail: correct remains v1, broken publishes v2 | yes |
| uniqueness/idempotency | trust the outer v1 observation after acquiring the writer lock | both observe v1; left commits v3; right locks: correct re-reads v3 and succeeds, broken attempts stale DDL and fails | yes |
| boundary/precedence | begin migration before rejecting a future version | version 4; observe; begin: correct returns `futureRejected`, broken reaches generic `migrationFailed` | yes |

The sensitivity command reports `atomicity_detected=true`,
`stale_reread_detected=true`, and `future_precedence_detected=true`.

## Resource observations

Lean commands ran one at a time under a 20-second external alarm. Imported
theorems use `maxHeartbeats 100000`; no unlimited setting was used. The focused
proof build completed in about one second, the proof consumer in about 0.2
seconds, and the bounded statistics command in under one second after build.
The finite explorer stopped at the planned depth 9 and 87 states; no depth or
heartbeat escalation, swapping, or abnormal memory growth was observed.

## Verification record

| Command | Result |
| --- | --- |
| proof consumer importing the new theorem module under alarm | pass |
| `lake -Kjobs=1 build` under alarm | pass |
| `lake exe generate_schema_migration -- --check corpus/schema-migration-concurrency.jsonl` under alarm | pass/fresh |
| `lake exe generate_schema_migration -- --stats` under alarm | 2 actors, 10 events, depth 9, 87/128 states, 5 cases |
| `lake exe generate_schema_migration -- --sensitivity` under alarm | all three broken families detected |
| `cargo test -p hoimin-cli --test lean_schema_migration_oracle` | 3 passed |
| `cargo test -p hoimin-cli session::schema:: --lib` | 10 passed |
| `cargo clippy -p hoimin-cli --test lean_schema_migration_oracle -- -D warnings` | pass |
| `cargo test --workspace --all-features` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo fmt --all -- --check` and `git diff --check` | pass |

## Limitations and next audit

- The model treats successful `BEGIN IMMEDIATE` acquisition as a serialized
  event. It does not prove SQLite locking or rusqlite transaction behavior.
- The WAL enablement retry loop, five-second busy timeout, scheduler fairness,
  process crashes, filesystem durability, and power-loss semantics are outside
  this contract.
- The finite explorer has two actors and depth 9. The writer-uniqueness and
  success/current properties are unbounded theorems, but liveness for arbitrary
  process counts is not claimed.
- Schema contents are abstracted to version stages plus one marker. Concrete DDL,
  v3 termination constraints, and golden database compatibility remain covered
  by Rust schema tests.
- The adapter observes public return values, final versions, and marker rows. It
  does not expose private version reads or transaction-local schema state.

The next independent formal audit should target AST fact-flow joins, especially
scope-sensitive binding propagation across control-flow joins. That surface does
not share SQLite premises and should use a separate worktree and corpus ledger.
