# Reservation ID Exhaustion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent active budget reservations from ever sharing an identifier and return a typed, atomic error after the `u64` reservation-ID space is exhausted.

**Architecture:** Store the next allocatable identifier as `Option<u64>`, issue `u64::MAX` once, and represent permanent exhaustion as `None`. Add a reserve-specific error and translate it through workspace copy reservation without changing normal limit, grant, release, or scheduling behavior.

**Tech Stack:** Rust 2024, `thiserror`, contract invariants, Cargo tests, cargo-mutants

## Global Constraints

- Work only in `.worktrees/issue-26-reservation-id-exhaustion` on branch `fix/issue-26-reservation-id-exhaustion`.
- Keep the design and implementation plan in the same branch and worktree as the implementation.
- `u64::MAX` is a valid reservation identifier and may be issued exactly once.
- Exhaustion is permanent; released reservation IDs are never reused.
- An exhaustion error must not mutate allocator state, active reservations, released reservations, or reserved totals.
- Existing budget-limit validation keeps precedence over allocator exhaustion.
- Do not redesign budget kinds, grant policy, cleanup effects, or state-machine scheduling.
- Do not change persisted schemas or serialized events.
- Mutation testing is limited to reservation allocation, reserve error propagation, and workspace error-code mapping.

---

## File Structure

- Modify `crates/hoimin-core/src/budget.rs`: define `ReserveError`, represent allocator exhaustion, allocate IDs atomically, strengthen the invariant, translate workspace reserve failures, and hold private boundary tests.
- Modify `crates/hoimin-core/tests/budget.rs`: update public limit-error assertions and verify workspace exhaustion code behavior that is observable through public APIs.
- No CLI, state-machine, schema, or dependency file changes are expected.

### Task 1: Make Reservation ID Allocation Unique and Exhaustible

**Files:**
- Modify: `crates/hoimin-core/src/budget.rs:31-145`
- Test: `crates/hoimin-core/src/budget.rs`
- Test: `crates/hoimin-core/tests/budget.rs`

**Interfaces:**
- Consumes: existing `LimitReached`, `ReservationId`, `Reservation`, and `BudgetLedger` APIs.
- Produces:
  - `pub enum ReserveError { LimitReached(LimitReached), ReservationIdsExhausted }`
  - `BudgetLedger::reserve(...) -> Result<ReservationId, ReserveError>`
  - private allocator state `next_id: Option<u64>`
  - private `BudgetLedger::allocate_reservation_id(&mut self) -> Result<ReservationId, ReserveError>`

- [ ] **Step 1: Add a private boundary test that demonstrates the duplicate-ID bug**

At the end of `crates/hoimin-core/src/budget.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::{
        BudgetKind, BudgetLedger, ReservationId, ReserveError, RunBudgets,
    };

    fn ledger() -> BudgetLedger {
        BudgetLedger::new(RunBudgets {
            memory: 8,
            copy: 8,
            processes: 8,
        })
    }

    #[test]
    fn allocator_exhaustion_preserves_the_last_active_reservation() {
        let mut ledger = ledger();
        ledger.next_id = Some(u64::MAX);

        let last = ledger.reserve(BudgetKind::Copy, 1).unwrap();
        assert_eq!(last, ReservationId(u64::MAX));
        assert_eq!(ledger.reserved(BudgetKind::Copy), 1);

        let error = ledger.reserve(BudgetKind::Copy, 1).unwrap_err();
        assert_eq!(error, ReserveError::ReservationIdsExhausted);
        assert_eq!(ledger.reserved(BudgetKind::Copy), 1);
        assert_eq!(ledger.reservation(last).unwrap().amount, 1);

        ledger.release(last).unwrap();
        assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
        assert_eq!(
            ledger.reserve(BudgetKind::Copy, 1),
            Err(ReserveError::ReservationIdsExhausted)
        );
    }
}
```

The initial test will require the intended `Option<u64>` and `ReserveError`
surface to compile. For the pre-fix behavioral RED, temporarily seed the
current `u64` field to `u64::MAX` and assert that two reservations have
different IDs and a total of two. Record the failure showing both IDs equal
`ReservationId(u64::MAX)` and the total remains one, then restore the intended
test above before implementation.

- [ ] **Step 2: Run the pre-fix boundary test to verify RED**

Run:

```bash
cargo test -p hoimin-core budget::tests::allocator_exhaustion_preserves_the_last_active_reservation -- --exact --nocapture
```

Expected pre-fix evidence: FAIL because the saturating allocator reuses
`ReservationId(u64::MAX)` and `BTreeMap::insert` replaces the first active
reservation.

- [ ] **Step 3: Define the typed reserve error**

After `LimitReached`, add:

```rust
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ReserveError {
    #[error(transparent)]
    LimitReached(#[from] LimitReached),
    #[error("reservation identifier space is exhausted")]
    ReservationIdsExhausted,
}
```

Do not add exhaustion to the release-specific `BudgetError`.

- [ ] **Step 4: Represent and allocate the final identifier exactly once**

Change the ledger field and constructor:

```rust
next_id: Option<u64>,
```

```rust
next_id: Some(0),
```

Add:

```rust
fn allocate_reservation_id(&mut self) -> Result<ReservationId, ReserveError> {
    let value = self
        .next_id
        .ok_or(ReserveError::ReservationIdsExhausted)?;
    let id = ReservationId(value);
    assert!(
        !self.reservations.contains_key(&id) && !self.released.contains(&id),
        "next reservation identifier must be globally unused"
    );
    self.next_id = value.checked_add(1);
    Ok(id)
}
```

`checked_add` returns `None` only after issuing `u64::MAX`.

- [ ] **Step 5: Update reserve without changing limit precedence**

Change the signature and implementation:

```rust
pub fn reserve(
    &mut self,
    kind: BudgetKind,
    amount: u64,
) -> Result<ReservationId, ReserveError> {
    let reserved = self.reserved(kind);
    let available = self.limit(kind).saturating_sub(reserved);
    if amount > available {
        self.check_invariant();
        return Err(LimitReached {
            kind,
            requested: amount,
            available,
        }
        .into());
    }
    let id = match self.allocate_reservation_id() {
        Ok(id) => id,
        Err(error) => {
            self.check_invariant();
            return Err(error);
        }
    };
    let replaced = self.reservations.insert(id, Reservation { kind, amount });
    assert!(
        replaced.is_none(),
        "allocated reservation identifier must be unique"
    );
    self.check_invariant();
    Ok(id)
}
```

The exhaustion branch checks but does not mutate active accounting.

- [ ] **Step 6: Strengthen the ledger invariant**

Replace the invariant body with:

```rust
fn invariant(&self) -> bool {
    let totals_fit = [BudgetKind::Memory, BudgetKind::Copy, BudgetKind::Processes]
        .into_iter()
        .all(|kind| self.reserved(kind) <= self.limit(kind));
    let active_and_released_are_disjoint = self
        .reservations
        .keys()
        .all(|id| !self.released.contains(id));
    let ids_precede_frontier = self.next_id.is_none_or(|next| {
        self.reservations
            .keys()
            .chain(self.released.iter())
            .all(|id| id.0 < next)
    });
    totals_fit && active_and_released_are_disjoint && ids_precede_frontier
}
```

Do not require a numeric frontier after exhaustion; `None` means all `u64`
values are behind the allocator.

- [ ] **Step 7: Update public limit-error assertions**

In `crates/hoimin-core/tests/budget.rs`, import `ReserveError` and replace direct
field access on the reserve error with:

```rust
let error = ledger.reserve(BudgetKind::Copy, 5).unwrap_err();
assert_eq!(
    error,
    ReserveError::LimitReached(hoimin_core::LimitReached {
        kind: BudgetKind::Copy,
        requested: 5,
        available: 4,
    })
);
```

Keep every existing success, release, and independent-kind assertion.

- [ ] **Step 8: Run focused tests to verify GREEN**

Run:

```bash
cargo test -p hoimin-core budget::tests::allocator_exhaustion_preserves_the_last_active_reservation -- --exact
cargo test -p hoimin-core --test budget
cargo test -p hoimin-core --features contracts
```

Expected: all PASS. The boundary test issues `u64::MAX` once, rejects later
reservations without changing totals, and releases the final reservation.

- [ ] **Step 9: Verify formatting and commit**

Run:

```bash
cargo fmt --all -- --check
cargo clippy -p hoimin-core --all-targets --all-features -- -D warnings
git diff --check
```

Expected: all PASS.

Commit:

```bash
git add crates/hoimin-core/src/budget.rs crates/hoimin-core/tests/budget.rs
git commit -m "fix: reject exhausted reservation identifiers"
```

### Task 2: Propagate Exhaustion Through Workspace Reservation

**Files:**
- Modify: `crates/hoimin-core/src/budget.rs:154-275`
- Test: `crates/hoimin-core/src/budget.rs`
- Test: `crates/hoimin-core/tests/budget.rs`

**Interfaces:**
- Consumes: Task 1 `ReserveError` and `BudgetLedger::reserve`.
- Produces:
  - `WorkspaceBudgetError::Reserve(ReserveError)`
  - stable code `workspace.reservation_id.exhausted`
  - unchanged limit code `workspace.copy.limit`

- [ ] **Step 1: Add failing workspace error-code contracts**

Inside the private `budget.rs` test module, add:

```rust
use super::{
    PreflightCompleted, WorkspaceBudgetError, reserve_workspace_copy,
};
use crate::EffectId;

#[test]
fn workspace_reservation_reports_identifier_exhaustion_without_accounting() {
    let mut ledger = ledger();
    ledger.next_id = None;
    let preflight = PreflightCompleted {
        id: EffectId(90),
        per_worker_logical_bytes: 1,
        requested_workers: 1,
        aggregate_logical_bytes: 1,
        fingerprint: None,
    };

    let error = reserve_workspace_copy(&mut ledger, &preflight).unwrap_err();

    assert_eq!(
        error,
        WorkspaceBudgetError::Reserve(ReserveError::ReservationIdsExhausted)
    );
    assert_eq!(error.code(), "workspace.reservation_id.exhausted");
    assert_eq!(ledger.reserved(BudgetKind::Copy), 0);
}

#[test]
fn workspace_limit_code_is_preserved_after_reserve_error_wrapping() {
    let mut ledger = BudgetLedger::new(RunBudgets {
        memory: 8,
        copy: 0,
        processes: 8,
    });
    let preflight = PreflightCompleted {
        id: EffectId(91),
        per_worker_logical_bytes: 1,
        requested_workers: 1,
        aggregate_logical_bytes: 1,
        fingerprint: None,
    };

    let error = reserve_workspace_copy(&mut ledger, &preflight).unwrap_err();
    assert!(matches!(
        error,
        WorkspaceBudgetError::Reserve(ReserveError::LimitReached(_))
    ));
    assert_eq!(error.code(), "workspace.copy.limit");
}
```

- [ ] **Step 2: Run the new tests to verify RED**

Run:

```bash
cargo test -p hoimin-core budget::tests::workspace_ -- --nocapture
```

Expected: compilation/test failure because `WorkspaceBudgetError::Reserve` and
the exhaustion code do not exist.

- [ ] **Step 3: Replace direct limit conversion with reserve conversion**

In `WorkspaceBudgetError`, replace:

```rust
#[error(transparent)]
LimitReached(#[from] LimitReached),
```

with:

```rust
#[error(transparent)]
Reserve(#[from] ReserveError),
```

Update `code`:

```rust
Self::Reserve(ReserveError::LimitReached(_)) => "workspace.copy.limit",
Self::Reserve(ReserveError::ReservationIdsExhausted) => {
    "workspace.reservation_id.exhausted"
}
```

Update rustdoc for `reserve_workspace_copy` to name
`WorkspaceBudgetError::Reserve` and both possible reserve failures.

- [ ] **Step 4: Run workspace and core regressions**

Run:

```bash
cargo test -p hoimin-core budget::tests::workspace_ -- --nocapture
cargo test -p hoimin-core --test budget
cargo test -p hoimin-core --features contracts
cargo test -p hoimin-cli --test workspace_recovery
```

Expected: all PASS. Existing workspace copy limit behavior and cleanup behavior
remain unchanged.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Expected: all PASS.

Commit:

```bash
git add crates/hoimin-core/src/budget.rs crates/hoimin-core/tests/budget.rs
git commit -m "fix: propagate reservation ID exhaustion"
```

### Task 3: Focused Mutation, Final Verification, and Pull Request

**Files:**
- Modify only if a survivor exposes missing coverage:
  `crates/hoimin-core/src/budget.rs`
  `crates/hoimin-core/tests/budget.rs`
- Create ignored reports:
  `.superpowers/sdd/issue-26-task-3-report.md`
  `.superpowers/sdd/issue-26-pr-body.md`

**Interfaces:**
- Consumes: Tasks 1 and 2 completed behavior.
- Produces: focused mutation evidence, independently reviewed branch, and one PR that closes Issue #26.

- [ ] **Step 1: Enumerate only important mutants**

Run:

```bash
cargo mutants \
  --package hoimin-core \
  --file crates/hoimin-core/src/budget.rs \
  --re "BudgetLedger::reserve|BudgetLedger::allocate_reservation_id|WorkspaceBudgetError::code" \
  --list
```

Expected: inventory contains only the allocator, reserve path, and workspace
error-code mapping. Record the exact mutant count before execution.

- [ ] **Step 2: Execute the focused mutation profile**

Run:

```bash
cargo mutants \
  --package hoimin-core \
  --jobs 4 \
  --file crates/hoimin-core/src/budget.rs \
  --re "BudgetLedger::reserve|BudgetLedger::allocate_reservation_id|WorkspaceBudgetError::code" \
  -- --lib --test budget
```

Expected: every viable mutant is caught, with zero missed and zero timeout.
Inspect every outcome, diff, and log. Record non-compiling mutants separately
as unviable; do not count them as caught.

- [ ] **Step 3: Address genuine survivors with minimal tests**

For each survivor, first reproduce it with the smallest focused test. Required
observable contracts include:

```rust
assert_eq!(last, ReservationId(u64::MAX));
assert_eq!(ledger.reserved(BudgetKind::Copy), 1);
assert_eq!(
    ledger.reserve(BudgetKind::Copy, 1),
    Err(ReserveError::ReservationIdsExhausted)
);
assert_eq!(error.code(), "workspace.reservation_id.exhausted");
```

Do not add exclusions without proving equivalence from the exact mutant diff.
Re-run the individual survivor, then run the entire focused profile fresh.

- [ ] **Step 4: Commit mutation-driven test improvements if needed**

If tracked tests changed:

```bash
git add crates/hoimin-core/src/budget.rs crates/hoimin-core/tests/budget.rs
git commit -m "test: strengthen reservation exhaustion coverage"
```

If no tracked change was needed, do not create an empty commit.

- [ ] **Step 5: Run fresh final verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
git diff --check origin/main..HEAD
git status -sb
```

Expected: all commands PASS and the tracked worktree is clean. If the full
suite reports a missing controlled `.venv`, run `uv sync --frozen` and repeat
the exact full test command without modifying source.

- [ ] **Step 6: Obtain independent whole-branch review**

The reviewer compares `origin/main..HEAD` with Issue #26, the design, and this
plan. Required review points:

- `u64::MAX` is issued no more than once;
- no active/released ID can be issued again;
- exhaustion is typed and atomic;
- normal limit precedence and fields remain unchanged;
- workspace codes distinguish limit and allocator exhaustion;
- release behavior, serialized schemas, and state-machine scheduling are not
  changed;
- focused mutation evidence matches the exact functions and outcomes.

Expected: no Critical or Important findings. Fix findings with TDD and repeat
the relevant mutation and verification commands.

- [ ] **Step 7: Prepare and create the dedicated PR**

Write `.superpowers/sdd/issue-26-pr-body.md`:

```markdown
## Summary

- issue `u64::MAX` once and represent permanent reservation-ID exhaustion explicitly
- return a typed reserve error without changing active accounting
- preserve workspace limit behavior while exposing an exhaustion-specific code
- include the approved design and implementation plan

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features`
- focused mutation result: see task report
- independent final review: approved

Closes #26
```

Run:

```bash
git push -u origin fix/issue-26-reservation-id-exhaustion
gh pr create \
  --base main \
  --head fix/issue-26-reservation-id-exhaustion \
  --title "fix: reject exhausted reservation identifiers" \
  --body-file .superpowers/sdd/issue-26-pr-body.md
```

- [ ] **Step 8: Let code-change CI complete**

Run:

```bash
gh pr checks --watch
```

Expected: all required jobs PASS. Do not cancel this code-change CI. Keep the
issue worktree for PR feedback.
