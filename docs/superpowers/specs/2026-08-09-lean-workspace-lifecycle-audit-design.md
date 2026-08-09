# Lean workspace lifecycle audit design

## Objective

Audit the ownership lifecycle of worker workspaces across `WorkspaceHandler`,
owned blocking tasks, cleanup recovery, and the shell completion boundary. The
audit is intended to find implementation bugs, not merely prove an abstract
model. It therefore separates model properties, bounded counterexample search,
and observations of the Rust implementation under matching premises.

Production behavior is not changed by this audit. If correspondence exposes a
bug, the audit retains a minimal reproducer, records the affected invariant and
exact implementation path, and opens a separate repair issue so the fix can be
reviewed in its own worktree.

## Owned contract

For each logical worker generation, ownership is in exactly one location:

- active in `WorkspaceHandler::workers`;
- owned by one prepared or completed `WorkspaceTask`;
- retained in `WorkspaceHandler::pending_cleanup`;
- absent because it has not been created or cleanup completed.

The contract expands into these properties:

- a generation is never duplicated across active, task-owned, and pending
  locations;
- removing an active worker during task preparation transfers, rather than
  loses, ownership;
- every apply completion returns the same generation to active ownership,
  regardless of mutation success;
- reset success returns the same generation active; reset failure either
  removes it after successful discard or retains it pending after failed
  discard;
- create consumes a matching pending generation before creating a new one and
  cannot make both generations observable;
- a rejected prepare operation preserves all ownership locations;
- a rejected completion cannot overwrite the currently registered generation;
- a completion from an older lifecycle epoch cannot resurrect or overwrite a
  worker after cleanup or replacement;
- successful cleanup leaves no active or pending worker and invalidates every
  earlier task completion;
- failed cleanup retains every not-yet-cleaned generation in a retryable
  location and never reports its reservation released;
- shell scheduling never makes `WorkspaceHandler` unavailable to a workspace
  completion, and finalization drains owned work before releasing workspace
  reservations.

The model excludes filesystem atomicity below `WorkerWorkspace`, allocator
failure, poisoned locks, process crashes inside an individual filesystem call,
and deliberately forged private Rust values. Those are infrastructure or
separate component contracts.

## Same-premise correspondence worksheet

Every comparison uses one of the exact modes below. A different-premise result
is never reported as an implementation mismatch.

| Behavior | Mode | Premise and evidence |
| --- | --- | --- |
| Synchronous create/apply/reset/cleanup through public `WorkspaceHandler` methods | `strict` | Public API, isolated temporary project, valid preflight and reservation |
| Cleanup validation rejection | `strict` | Public API with a wrong or malformed reservation grant |
| Prepare, execute, and accept of apply/reset/create tasks | `internal-fixture` | Crate-private API exercised by a `#[cfg(test)]` adapter |
| Duplicate or late completion against an independently installed generation | `internal-fixture` | Synthetic crate-local setup is needed because completions are affine Rust values |
| Shell completion and cleanup interleavings permitted by the real core state machine | `strict` | Existing shell entry point with public configuration and controlled blocking hooks |
| Calling `WorkspaceHandler::handle_cleanup` while a crate-private task remains externally owned | `model-only` | The shell drains owned blocking work before enqueueing stop-produced finalization/cleanup effects; the handler alone does not track external task ownership |
| Arbitrary completion delivery not emitted by the core state machine | `model-only` | Useful for sensitivity, but not a production scheduling premise |
| Failure between individual filesystem syscalls | `model-only` | No hook exposes every syscall boundary |
| Corpus parse, Lean execution, temporary filesystem, or adapter panic | `infrastructure-error` | Harness failure, never a semantic mismatch |

The audit report records which rows received executable correspondence. Any
unexercised row remains an explicit limitation.

The cleanup gate is therefore a shell protocol obligation, not a self-contained
`WorkspaceHandler` check. The model retains the stronger task-empty guard so a
future caller or scheduler reordering is detected, while direct handler/task
composition under that impossible production schedule is classified
`model-only` rather than forced into correspondence.

## Formal model

Add an independent `WorkspaceAudit` namespace to the pinned
`formal/HoiminOracle` project. The finite model contains two worker roles, two
generation roles, two task roles, one handler lifecycle epoch, active and
pending ownership maps, and affine task/completion records. Task records carry
operation kind (`create`, `apply`, or `reset`), worker, generation, origin
epoch, and execution outcome.

Events cover preflight readiness, installing an initial worker, preparing each
task kind, executing success and failure paths, accepting a completion,
cleanup success/failure, and advancing the lifecycle epoch. Observations
contain verdict, stable error class, active ownership, task ownership, pending
ownership, current epoch, and released-cleanup status.

Correct transitions reject stale-epoch completions transactionally. The model
uses stable event order and breadth-first exploration with state deduplication
only after all outgoing events have been checked. The default search bound is
eight transitions; reachable-state and checked-transition counts are reported
as bounded evidence, never as proof of Rust.

## Proof obligations

Lightweight Lean modules prove transition-local and trace-lifted claims:

- ownership locations are pairwise disjoint;
- each `(worker, generation)` has at most one owner;
- prepare success transfers exactly one owner;
- prepare rejection preserves the complete state;
- accepted apply/reset/create outcomes match their ownership postconditions;
- rejected or stale completion preserves active and pending ownership;
- cleanup success empties handler ownership and advances the epoch;
- cleanup failure preserves retryable ownership;
- every correct transition preserves the invariant;
- arbitrary traces from the initial state preserve the invariant.

Large `native_decide` checks, BFS, shrinking, statistics, and corpus generation
remain in a non-imported executable module so ordinary `lake build` stays
predictable.

## Refutation and sensitivity

Refutation precedes proof. Three deliberately broken transition families must
produce stable minimal witnesses:

- **atomicity/transactionality:** remove a worker during a rejected prepare or
  failed cleanup without returning it to a retryable location;
- **uniqueness/idempotency:** accept a completion into an already occupied
  worker slot and overwrite the registered generation;
- **boundary/precedence:** accept a completion from an earlier cleanup epoch
  and resurrect the old generation.

The executable checks fixed witnesses and performs shortest-first bounded
search. Failure to detect any broken family fails corpus generation.

## Lean-owned corpus and Rust adapters

Lean generates deterministic JSONL cases with schema version, case ID,
correspondence mode, schedule, and complete expected observations. `strict`
cases are replayed by an integration test through public APIs. Cases requiring
crate-private task boundaries are replayed by a small `#[cfg(test)]` module in
the workspace implementation. `model-only` cases stay in the sensitivity
ledger and are not presented as Rust mismatches.

Adapters translate semantic worker/task/generation roles into real requests
and observe real handler state. They do not recompute expected semantics.
Unknown schemas, modes, events, observations, or duplicate IDs are rejected.
Single-case replay is supported, and corpus freshness is verified by
regeneration followed by an empty diff.

## Deliverables

- `WorkspaceModel.lean`: finite state, events, observations, correct and
  deliberately broken transitions;
- `WorkspaceProofs.lean`: explicit-premise invariant proofs;
- `WorkspaceCases.lean`: stable schedules and JSON encoding;
- `WorkspaceAuditMain.lean`: BFS, shrinking, sensitivity gates, statistics,
  and corpus generation;
- `corpus/workspace-lifecycle.jsonl`: Lean-generated expectations;
- Rust strict and internal-fixture correspondence adapters;
- a self-contained report under `docs/superpowers/reports/`;
- a detailed counterexample ledger and follow-up issue if a real mismatch is
  found;
- this design and its implementation plan in the same worktree and PR.

## Verification and result handling

Independent gates are the pinned Lean build, theorem checks, sensitivity
witnesses, deterministic corpus freshness, strict correspondence,
internal-fixture correspondence, focused existing workspace/shell tests, Rust
formatting and Clippy, full workspace tests, and diff hygiene.

A mismatch is classified as `confirmed bug`, `specification ambiguity`,
`model defect`, or `infrastructure error`. The model is not silently weakened
to match Rust, and the report never claims that Lean proves the production
implementation.
