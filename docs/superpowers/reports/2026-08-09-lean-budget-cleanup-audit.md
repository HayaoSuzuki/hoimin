# Lean Budget and Cleanup Formal Audit Report

## Result

No production defect was found in the focused budget-reservation and cleanup-release audit. Lean proves the declared model invariant for arbitrary traces, bounded exploration found no unsafe state, all three deliberately broken models were detected, and all 10 same-premise strict corpus cases matched Hoimin's public Rust API.

Two report-only allocator-exhaustion cases intentionally do not match the public adapter. Their Lean premise sets `max_id = 0`, while `BudgetLedger::new` uses the production `u64` frontier and exposes no public frontier-injection seam. This is classified as **specification ambiguity** at the correspondence boundary, not as a production counterexample. Existing crate-internal Rust tests exercise the actual `u64::MAX` frontier.

## Claim and model boundary

The audit covers `BudgetLedger`-style memory, workspace-copy, and process reservations; monotonically allocated reservation IDs; and atomic cleanup release. The model observes typed acceptance or rejection, stable error-code classes, allocated roles, active reservations, released roles, and per-kind totals.

Lean establishes properties of `HoiminOracle.BudgetAudit`, not of compiled Rust. Rust correspondence is established separately by replaying Lean-owned JSON Lines through public APIs: `BudgetLedger::reserve`, `reserve_workspace_copy`, `WorkspaceCopyGrant::reservation_id`, and `release_workspace_copy`.

The audit does not model concurrent access, integer widths above the declared finite values, process execution, filesystem cleanup, or the full run state machine. Those remain outside this focused claim.

## Declared intent versus implicit behavior

The modeled intent is:

- memory, copy, and process limits are independent;
- limit rejection precedes identifier-exhaustion rejection;
- successful zero-amount reservations still consume a fresh identifier;
- identifiers are never reused after release;
- cleanup validates its complete identifier list before changing accounting;
- duplicate, already released, or unknown identifiers reject without partial release;
- a successful cleanup removes exactly the requested active reservations and no others.

The important implicit behavior made explicit by the audit is that reservation IDs form one global frontier across all budget kinds, while totals remain per-kind.

## Finite exploration

The explorer used limits and reservation amounts `0`, `1`, and `2` for all three kinds. Its stable event alphabet contains nine reserve events and eight cleanup shapes: empty, singleton IDs `0`/`1`/`2`, ordered and reversed two-ID lists, a duplicate list, and an active-plus-unknown list.

The exact run was:

```text
depth=6 alphabet=17 states=10341 transitions=175338
```

Breadth-first exploration started from all 27 limit combinations. It retained the first shortest trace for each identical semantic state. Every outgoing transition of a retained state was checked before successor deduplication. This identical-state reduction is valid for the state invariant only; the result is a finite check, not an unbounded proof.

## What Lean established

The Lean sources contain no `sorry` or audit-specific axioms. The main theorems are:

- `rejected_preserves_state`: every rejected modeled event preserves the complete state;
- `step_preserves_invariant`: one event preserves per-kind bounds, uniqueness, disjointness, and allocator-frontier validity;
- `run_preserves_invariant`: any finite event trace preserves that invariant from any state satisfying its explicit premise;
- `successful_release_is_exact`: accepted cleanup preserves limits/frontier, filters exactly the requested active IDs, and prepends exactly those IDs to released history;
- `reachable_is_safe`: every arbitrary trace from the named audit initial state remains safe.

The symbolic trace theorem is stronger than the depth-six search for model safety. The bounded search remains useful as executable sensitivity and corpus-domain evidence.

## Broken-variant sensitivity

All three fixed witnesses were detected before corpus generation was allowed:

- partial release before discovering an unknown ID: reserve copy `1`, reserve copy `1`, release `[0, 2]`;
- reuse of a released identifier: reserve copy `1`, release `[0]`, reserve copy `1` with a one-ID frontier;
- cross-kind pooling instead of independent limits: with limits memory `0`, copy `2`, processes `0`, reserve memory `1`.

These witnesses are evaluated directly and are not removed by state deduplication.

## Implementation correspondence

The Lean-owned corpus has 12 independent cases. Ten strict cases matched completely, including verdict, normalized typed error, allocated role, active entries, released roles, and all three totals. There were no panics, parse failures, setup failures, or other infrastructure errors.

The two report-only cases use a reduced `max_id = 0` solely to make exhaustion executable. The public Rust constructor cannot reproduce that premise, so Rust correctly allocates role `1` rather than exhausting after role `0`. The adapter preserves and prints these differences instead of weakening the Lean model or reaching into Rust private state.

Crate-internal tests separately confirm that production allocation of `u64::MAX` is the last successful allocation, the next reservation is rejected as exhausted, limit rejection retains precedence, and cleanup after exhaustion remains valid.

## Minimal witnesses

No same-premise production counterexample exists, so no counterexample ledger was created. The shortest report-only boundary witness is:

```text
limits = {memory: 2, copy: 2, processes: 2}
model max_id = 0
reserve copy 0  -> accepted role 0
reserve copy 0  -> Lean: exhausted; public Rust constructor: accepted role 1
```

This trace demonstrates the different allocator-frontier premises, not divergent behavior under equal premises.

## Classification and impact

Classification: **specification ambiguity**.

The production behavior examined by equal-premise cases agrees with the model. The remaining ambiguity is whether allocator exhaustion should have a supported public test seam for external correspondence tests. Current risk is low because the frontier is `u64::MAX`, the allocator uses `checked_add`, and crate-internal tests cover the boundary. No production change is recommended from this focused audit.

## Model and adapter limitations

- `Nat` is used in Lean; Rust uses `u64`. Overflow behavior is represented only by explicit frontier cases and existing Rust tests.
- The adapter maps model roles to real IDs after successful allocation and uses a guaranteed-unmapped high ID for unknown-role cleanup.
- Released history is adapter-observed from successful public calls because `BudgetLedger` does not expose its released set.
- Active state is observed only with allocated IDs and `BudgetLedger::reservation`; totals use `BudgetLedger::reserved`.
- The two reduced-frontier cases are report-only and are not implementation equivalence claims.
- Filesystem cleanup and asynchronous run-machine interleavings belong to the broader follow-up audit, not this focused model.

## Owner decisions

No immediate owner decision is required. A future testability decision is whether to expose a narrowly scoped allocator-frontier fixture outside crate-internal tests. This audit does not recommend expanding the production API solely for that purpose.

## Exact reproduction commands

```bash
cd formal/HoiminOracle
lake build
lake exe generate_budget -- --check corpus/budget-cleanup.jsonl
lake exe generate_budget -- --stats

cd ../..
cargo test -p hoimin-core --test lean_budget_oracle -- --nocapture
HOIMIN_BUDGET_ORACLE_CASE=mixed_unknown_cleanup_is_atomic cargo test -p hoimin-core --test lean_budget_oracle oracle_correspondence -- --exact --nocapture
cargo test -p hoimin-core --test budget
cargo test -p hoimin-cli --test workspace_recovery
```
