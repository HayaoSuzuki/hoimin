# Reservation ID Exhaustion Design

## Context

`BudgetLedger::reserve` currently stores the next reservation identifier as a
`u64` and advances it with `saturating_add(1)`. Once the allocator reaches
`u64::MAX`, every later reservation receives the same identifier. Inserting the
duplicate into `reservations` replaces the active entry, undercounts the
reserved total, and loses a release obligation.

This design resolves GitHub Issue #26 and audit finding `RUST-AUDIT-002`.
It changes only reservation identity allocation and its typed error surface.
Budget kinds, limits, grant policy, and state-machine scheduling are unchanged.

## Decision

Represent allocator state as `Option<u64>`:

- `Some(id)` means `id` is the next identifier that may be issued;
- after issuing an identifier smaller than `u64::MAX`, store
  `Some(id + 1)`;
- after issuing `u64::MAX`, store `None`;
- `None` means the identifier space is exhausted permanently.

`u64::MAX` remains a valid reservation identifier and may be issued exactly
once. A later reservation attempt returns a typed exhaustion error before
inserting a reservation or changing any active accounting.

Identifiers are never reused, including after release. This preserves the
existing distinction between an already-released identifier and an unknown
identifier and avoids making stale cleanup capabilities valid again.

## Error Model

Add a reserve-specific error:

```rust
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ReserveError {
    #[error(transparent)]
    LimitReached(#[from] LimitReached),
    #[error("reservation identifier space is exhausted")]
    ReservationIdsExhausted,
}
```

`BudgetLedger::reserve` returns `Result<ReservationId, ReserveError>`.
Existing limit failures retain their `LimitReached` value and display text
through the transparent variant.

`WorkspaceBudgetError` gains a transparent conversion from `ReserveError`
rather than directly from `LimitReached`:

```rust
#[error(transparent)]
Reserve(#[from] ReserveError),
```

Its stable code mapping is:

- `ReserveError::LimitReached(_)` -> `workspace.copy.limit`;
- `ReserveError::ReservationIdsExhausted` ->
  `workspace.reservation_id.exhausted`.

This keeps ordinary workspace limit behavior unchanged while making allocator
exhaustion machine-readable. `BudgetError` remains release-specific and is not
expanded with a reserve failure.

## Reservation Ordering and Atomicity

Reservation follows this order:

1. compute the currently reserved amount for the requested kind;
2. reject an amount above the remaining budget with `LimitReached`;
3. read the next identifier, returning `ReservationIdsExhausted` for `None`;
4. compute and store the successor state with `checked_add`;
5. insert the new reservation under the unique identifier;
6. check the ledger invariant and return the identifier.

The existing budget-limit error therefore keeps precedence when a request is
both over budget and made after identifier exhaustion. This preserves existing
normal validation behavior.

The allocator state advances immediately before insertion only after all
fallible checks have passed. `BTreeMap::insert` must be asserted to return
`None`; an occupied identifier would violate the allocator's internal
uniqueness contract. The public exhaustion path itself performs no mutation.

The ledger invariant is strengthened to require that no active or released
identifier is at or above the next allocatable identifier while the allocator
is `Some`. When it is `None`, all numeric identifiers are considered past the
allocation frontier. Map and set uniqueness remain provided by their
collections.

## Boundary Behavior

A focused private unit test seeds `next_id` to `Some(u64::MAX)`:

1. reserve one byte and receive `ReservationId(u64::MAX)`;
2. verify the copy total is one byte;
3. attempt a second one-byte reservation and receive
   `ReserveError::ReservationIdsExhausted`;
4. verify the copy total and the first reservation are unchanged;
5. release the first reservation successfully;
6. verify the total returns to zero;
7. attempt another reservation and receive the same exhaustion error.

This proves that the last identifier is usable once, exhaustion is permanent,
failure is atomic, totals remain correct, and release behavior is preserved.

Additional tests verify:

- ordinary sequential reservations remain distinct;
- limit failures still expose the original kind/requested/available values;
- workspace reservation maps exhaustion to the stable typed code;
- contract-enabled tests retain invariant correctness at the boundary.

No public test constructor or allocator-seeding API is added. The boundary test
lives in `budget.rs`, where its test module can set private allocator state.

## Focused Mutation Testing

Mutation testing targets only:

- `BudgetLedger::reserve`;
- the reserve-error-to-workspace-code mapping; and
- any small allocator successor helper introduced by implementation.

The boundary tests must catch viable mutations that:

- reuse `u64::MAX`;
- report success when the allocator is `None`;
- mutate accounting on exhaustion;
- change the last valid identifier;
- collapse exhaustion into the ordinary budget-limit code.

The run must not include unrelated state-machine or workspace scheduling code.
Every viable survivor is inspected and addressed with the smallest behavioral
test. Non-compiling mutants are recorded separately and are not reported as
caught.

## Compatibility

This is a source-level API change for callers that directly name the error type
of `BudgetLedger::reserve`. Repository callers use `?`, `unwrap`, `is_err`, or
field assertions in tests and will be updated to match `ReserveError`.

Serialized events and persisted schemas do not contain `ReserveError`,
`LimitReached`, or allocator state, so no data migration or schema version
change is required. Normal reservation IDs and release results are unchanged.

## Non-goals

- Do not reuse released reservation IDs.
- Do not widen identifiers beyond `u64`.
- Do not redesign budget categories, limits, grants, or cleanup effects.
- Do not alter state-machine resource scheduling.
- Do not serialize allocator state or reserve errors.
- Do not perform workspace-wide mutation testing.
