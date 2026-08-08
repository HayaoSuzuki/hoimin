# Lean budget and cleanup formal audit design

## Objective

Audit the existing workspace-copy reservation and cleanup lifecycle for latent
accounting bugs. The audit must distinguish properties proved inside Lean from
behavior observed through Hoimin's public Rust APIs. It must not change
production behavior.

## Owned contract

The audit treats the following repository behavior as owned:

- reservations of different budget kinds are accounted independently;
- a successful reservation never exceeds its kind's run-wide limit;
- an unsuccessful reserve or release leaves the ledger unchanged;
- a cleanup acknowledgement releases exactly the active reservation IDs it
  names, atomically and at most once;
- duplicate IDs, previously released IDs, and unknown IDs are rejected without
  partially releasing other reservations;
- reservation identifier exhaustion never reuses an active or released ID;
- a workspace copy grant carries the exact aggregate allowance and rejects
  worker indexes outside the preflight worker count.

The model excludes filesystem deletion, wall-clock timing, process supervision,
allocator memory failure, poisoned mutexes, and cleanup handler I/O. Those are
separate infrastructure and handler contracts. The Rust correspondence adapter
uses `BudgetLedger`, `reserve_workspace_copy`, `release_workspace_copy`, and
`WorkspaceCopyGrant`; it does not fake their accounting behavior.

## Formal model

Add a small dependency-free namespace to the existing pinned
`formal/HoiminOracle` project. The state contains finite per-kind limits, active
reservations, released IDs, and the next-ID frontier. Events cover reserve and
atomic cleanup release. Verdicts retain typed rejection categories.

The explorer enumerates shortest traces first in stable event order. For the
state invariant only, it retains the first shortest trace for each identical
semantic state after checking all outgoing events; fixed broken witnesses are
not deduplicated. The report must disclose this reduction and count reachable
states and checked transitions. Its finite domain is:

- budget kinds: memory, copy, processes;
- limits and requested amounts: semantic representatives `0`, `1`, and `2`;
- cleanup lists: empty, one ID, two distinct IDs, duplicate IDs, a released ID,
  and an unknown ID;
- trace depth: through 6 transitions.

The report will describe this as bounded exploration, never as a proof.

After the bounded search stabilizes, Lean theorems will state explicit
premises for:

- per-kind totals never exceed limits in reachable model states;
- rejected transitions preserve the complete state;
- successful cleanup removes all and only requested active IDs and records
  them as released;
- active and released IDs remain disjoint;
- allocated IDs are globally fresh and remain below the frontier.

## Sensitivity

The formal project must retain deliberately broken transitions for at least:

- partial release before discovering an invalid cleanup ID;
- released-ID reuse after identifier wrap/exhaustion;
- cross-kind accounting that incorrectly shares or ignores a limit.

Fixed witnesses or the bounded explorer must detect every broken variant before
the audit is considered trustworthy.

## Implementation correspondence

Lean owns deterministic cases and expected observations. A Rust adapter reads
the generated corpus and applies each case to the real public accounting API.
Expected values must not be duplicated in Rust. Cases execute independently and
are classified as `match`, `mismatch`, or `infrastructure error`.

Strict correspondence covers currently owned behavior. Any mismatch remains a
fixed witness and is classified as a confirmed bug, specification ambiguity,
model defect, or infrastructure error. Production code is not repaired as part
of this audit.

## Deliverables

- Lean model, explorer, theorems, and broken witnesses in
  `formal/HoiminOracle`;
- a deterministic generated corpus for budget/cleanup cases;
- a public-API Rust correspondence test;
- a self-contained report under `docs/superpowers/reports/` with exact commands,
  finite bounds, minimal witnesses, correspondence status, and owner decisions;
- a counterexample ledger if any mismatch is found.

## Verification

The audit runs these independent gates:

1. pinned Lean build and theorem checks;
2. corpus freshness check;
3. Rust correspondence test in strict and single-case modes;
4. deliberately broken-model witnesses;
5. focused existing budget and workspace recovery tests;
6. formatting, Clippy, and diff hygiene for audit artifacts.

## Follow-up boundary

After this focused audit is complete, a later broad audit may reuse the same
workflow for session ownership and scheduler interleavings. It will use separate
claims, bounds, models, and correspondence ledgers rather than widening this
model until its conclusions become ambiguous.
