# Findings Ledger

## Status vocabulary

- `lead`: requires semantic review
- `accepted`: actionable root cause
- `rejected`: not actionable, with rationale
- `issue_created`: accepted and linked to GitHub

Classification accepts only `confirmed bug`, `high-risk design`, or `maintainability`.
Severity accepts only `P0`, `P1`, `P2`, or `P3`.

| ID | Area | Classification | Severity | Status | Locations | Evidence | Root cause / boundary | Disposition |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| RUST-001 | isolation | high-risk design | P1 | lead | `crates/hoimin-cli/src/process/mod.rs:341`; `crates/hoimin-cli/src/resource/portable.rs:106-121` | `Command::spawn` returns a running child before `PortableSupervisor::attach` assigns it to the kill-on-close Windows Job Object. The source itself documents that the child can execute in this interval. | On Windows portable mode, process creation is not suspended or otherwise atomic with Job Object assignment. A fast child can perform work or spawn descendants before the process tree is placed under the configured lifetime/resource boundary; attach failure only kills the root handle and cannot prove pre-assignment descendants are contained. | Confirm with a Windows stress test using a child that immediately spawns a detached descendant, then design suspended startup/assignment before resuming. |
| RUST-002 | core | maintainability | P3 | lead | `crates/hoimin-core/src/budget.rs:91-93` | `BudgetLedger::reserve` assigns `ReservationId(self.next_id)`, advances with `saturating_add(1)`, then unconditionally inserts into the active-reservation map. No boundary test exercises ID exhaustion. | Once `next_id` reaches `u64::MAX`, the first reservation at that ID leaves `next_id` saturated. A later successful reservation reuses `ReservationId(u64::MAX)` and `BTreeMap::insert` replaces the still-active entry. `reserved()` then undercounts the grants and one release loses the other obligation. The current machine caller creates only one copy reservation per run, so this is dormant there, but the public ledger API does not enforce that bound. | Replace saturation with checked allocation and a typed exhaustion error (or make uniqueness an explicit bounded construction invariant); add a focused boundary unit test that seeds the allocator at `u64::MAX`. |

## Task 3 core lead disposition

Task 3 retained only `RUST-001` in the CLI isolation area. It produced no
`hoimin-core` lead requiring accepted or rejected classification in Task 4.
