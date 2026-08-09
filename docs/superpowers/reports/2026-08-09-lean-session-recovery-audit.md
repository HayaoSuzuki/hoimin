# Lean Session Persistence, Recovery, and Ownership Audit Report

## Result

The audit found one strict public-API mismatch: an incomplete finish performed
by a handler that does not own the run commits the resumable database state but
cannot release the actual owner's file lock. The run is then temporarily
unresumable. The complete witness and classification are recorded in
`2026-08-09-lean-session-recovery-counterexamples.md`.

The other 17 strict corpus cases matched `SessionHandler`. Existing process
death, contention, and replacement rollback fixtures also passed. No production
code was changed by this audit.

## Resolution: Issue #281

The confirmed mismatch was repaired on 2026-08-09 by requiring the calling
`SessionHandler` to own the run before `finish` starts its SQLite transaction.
A rejected call now returns `session.finish.owner` without changing durable
state or the actual owner's lock. A successful finish releases ownership, so
that handler must load and own the run again before another finish.

The Lean model now rejects non-owner finish without changing state and includes
the theorem `non_owner_finish_is_rejected_without_state_change`. The strict
case is now `non_owner_incomplete_finish_is_rejected`; the former idempotency
case is `released_handler_cannot_finish_again`. The regenerated 18-case corpus
matches the Rust public API in all 18 cases, and the Rust adapter contains no
known-mismatch exemption.

Current focused reproduction:

```bash
HOIMIN_SESSION_ORACLE_CASE=non_owner_incomplete_finish_is_rejected \
  cargo test -p hoimin-cli --test lean_session_oracle oracle_correspondence -- --exact --nocapture
```

The remaining sections preserve the original audit result and evidence as the
historical record that led to Issue #281.

## Claim and excluded scope

The audited contract combines valid-schema SQLite state and live run ownership:
a run has at most one live owner; completion is final; determinate results are
immutable; and rejected or failed operations do not partially change durable
state.

The audit excludes schema migration and deliberately corrupt rows, storage
durability below SQLite, kernel/filesystem lock defects, hardware power loss,
allocator failure, and poisoned synchronization primitives. Public operations
are atomic except for the explicitly model-only internal boundaries.

## Same-premise correspondence worksheet

| Evidence | Mode | Result |
| --- | --- | --- |
| 18 Lean-generated schedules through public `SessionHandler` APIs | `strict` | 17 matches; 1 confirmed bug |
| Two public handlers sharing one database and ownership directory | `strict` | Ownership conflict and independent-run cases matched |
| Handler drop followed by public resume | `strict` | Matched |
| Child-process death followed by immediate resume | `internal-fixture` | Passed existing fixture |
| SQLite contention matrix | `internal-fixture` | Passed existing fixture |
| Failed inconclusive replacement | `internal-fixture` | Passed existing fixture |
| Split load and replacement transaction boundaries | `model-only` | Used only by Lean model/sensitivity checks |
| Parser, SQLite setup, mapping, or adapter panic | `infrastructure-error` | Kept separate from semantic observations |

## Formal state and event model

`SessionModel.lean` models two handlers, runs, fingerprints, mutants, and
payload roles; all seven mutation statuses; durable runs/results; a list of
logical owners; and pending split-load or replacement work. Public events model
open, begin, load, lookup, persist, finish, drop, and crash. Internal events
expose only the load recheck and replacement commit/rollback boundaries.

The owner collection is intentionally a list, making duplicate ownership a
detectable state rather than impossible by representation. Correct replacement
keeps the prior committed row until commit, and a split load does not install a
logical owner until its post-lock eligibility recheck succeeds.

## What Lean proved

Lean checked the following theorem bodies without `sorry`, `admit`, or custom
axioms:

- rejected public events preserve the modeled durable projection;
- determinate stored results are immutable;
- invalid replacements retain the old result;
- accepted inconclusive replacements equal the canonical one-row replacement;
- handler drop and crash remove that handler's ownerships;
- an accepted complete finish leaves the run with no owner;
- the structural ordinal invariant is preserved by one step and arbitrary
  traces from the initial state.

The trace-lifted `Invariant` deliberately covers `nextOrdinal = runs.length`,
not every clause in the executable `safe` predicate. Ownership uniqueness,
referential consistency, completed-run ownership, and pending-state consistency
were checked over the bounded reachable state space and by focused local
theorems where listed above. They are not claimed as universal proofs about the
production implementation.

## Bounded exploration and cost

The deterministic breadth-first explorer used depth 8 and a fixed alphabet of
31 events. It visited 2,965 unique states and checked 55,583 outgoing
transitions. It deduplicates states only after producing and checking all
outgoing transitions for the current frontier. The corpus contains 18 strict
cases.

Measured on 2026-08-09 in the audit worktree:

| Command | Real time |
| --- | ---: |
| `lake build` | 2.38 s |
| corpus freshness check | 0.92 s |
| bounded statistics/safety gate | 0.92 s |
| three-family sensitivity gate | 0.94 s |

The executable owns breadth-first exploration, shrinking, statistics, and JSON
generation. The proof library does not import the executable, so ordinary Lean
module builds do not evaluate the explorer.

## Broken-variant sensitivity

All three deliberately broken families were detected:

- Atomicity: `open:h0 -> begin:h0:r0:f0 ->
  persist:h0:r0:m0:timeout:p0:valid ->
  persist:h0:r0:m0:killed:p1:invalid_diagnostic` exposes premature deletion.
- Uniqueness: `open:h0 -> open:h1 -> begin:h0:r0:f0 -> load:h1:f0`
  produces two owners in the broken transition.
- Boundary: `open:h0 -> open:h1 -> begin:h0:r0:f0 ->
  internal-load-read:h1:f0 -> finish:h0:r0:true ->
  internal-load-acquire:h1 -> internal-load-recheck:h1` returns a stale
  completed candidate when the recheck is broken.

Sensitivity work also found two defects in the audit model itself before the
implementation comparison: missing parentheses made list-based safety clauses
bind inside preceding lambdas, and a second public persist could interleave
with a model-only pending SQLite transaction. Both were corrected before corpus
generation, and all sensitivity gates were rerun.

## Strict implementation correspondence

The Rust adapter rejects unknown JSON fields, schemas, modes, events, roles,
statuses, error codes, schedule lengths, and duplicate IDs. Each case uses an
isolated temporary database and public `SessionHandler` methods. Durable rows
are queried independently after each operation; API errors remain semantic
observations, while harness errors and panics are infrastructure failures.

Seventeen cases matched exactly, including newest compatible selection,
competing ownership, drop release, completion finality, incomplete-finish
idempotency, both determinate statuses, all five inconclusive replacement
statuses, rollback after invalid diagnostics, missing-run rollback, and
independent owners for different runs.

The one mismatch is pinned by
`HOIMIN_SESSION_ORACLE_CASE=non_owner_incomplete_finish_releases_for_resume`.
The test requires that this named mismatch remains explicit: an unexpected new
mismatch, infrastructure failure, or disappearance of the known mismatch fails
the test instead of silently changing the model.

## Internal-fixture evidence

The following existing integration tests passed:

- `process_death_releases_run_ownership_for_immediate_resume`;
- `session_operations_complete_under_contention_matrix`;
- `failed_inconclusive_replacement_restores_the_previous_result`.

These support process-lock cleanup, real SQLite contention, and transactional
rollback, but are not presented as public-operation Lean correspondence.

## Minimal witnesses and classifications

One strict witness remains, classified as a confirmed implementation bug. See
the counterexample ledger for intermediate observations and impact. There were
no model defects or infrastructure failures remaining in the generated corpus
run.

## Model and adapter limitations

The finite model has two values per semantic role and explores only to depth 8.
The adapter's owner labels record successful ownership-establishing public
calls; explicit competing `load` outcomes provide the actual file-lock
evidence. The adapter does not inspect private lock objects. SQLite rows are
observed, but filesystem durability and process scheduling are outside scope.

## Owner decisions

Production behavior was intentionally not changed in this audit. A follow-up
must choose and document one coherent contract: either reject `finish` from a
non-owner, or make finishing through another handler safely release/invalidate
the live ownership. The former is smaller and preserves the single-owner
boundary; the latter requires cross-handler ownership coordination.

## Exact reproduction commands

```bash
cd formal/HoiminOracle
lake build
lake exe generate_session -- --check corpus/session-recovery.jsonl
lake exe generate_session -- --stats
lake exe generate_session -- --sensitivity

cd ../..
cargo test -p hoimin-cli --test lean_session_oracle -- --nocapture
HOIMIN_SESSION_ORACLE_CASE=non_owner_incomplete_finish_releases_for_resume \
  cargo test -p hoimin-cli --test lean_session_oracle oracle_correspondence -- --exact --nocapture
cargo test -p hoimin-cli --test session_handler \
  process_death_releases_run_ownership_for_immediate_resume -- --exact --nocapture
cargo test -p hoimin-cli --test session_handler \
  session_operations_complete_under_contention_matrix -- --exact --nocapture
cargo test -p hoimin-cli --test session_handler \
  failed_inconclusive_replacement_restores_the_previous_result -- --exact --nocapture
```
