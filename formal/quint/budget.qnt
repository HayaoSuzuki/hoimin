// A bounded workload, not a bound imposed on the production allocator.
module BudgetModel {
  const ALLOW_OVERDRAFT: bool
  const MAX_RESERVATIONS: int
  type Reservation = { id: int, kind: int, amount: int }
  var active: Set[Reservation]
  var released: Set[int]
  var nextId: int
  var op: str
  var kind: int
  var amount: int
  var id: int
  var result: str

  pure def total(rows: Set[Reservation], k: int): int =
    rows.filter(r => r.kind == k).fold(0, (sum, r) => sum + r.amount)

  action init = all {
    active' = Set(), released' = Set(), nextId' = 0,
    op' = "init", kind' = 0, amount' = 0, id' = 0, result' = "init",
  }

  action reserve = {
    nondet k = Set(0, 1, 2).oneOf()
    nondet n = Set(0, 1, 2, 3).oneOf()
    val accepted = n <= 2 - total(active, k) or ALLOW_OVERDRAFT
    all {
      // Stop adding IDs at the workload boundary; this is not exhaustion.
      nextId < MAX_RESERVATIONS or not(accepted),
      active' = if (accepted) active.union(Set({ id: nextId, kind: k, amount: n })) else active,
      released' = released,
      nextId' = if (accepted) nextId + 1 else nextId,
      op' = "reserve", kind' = k, amount' = n,
      id' = nextId, result' = if (accepted) "ok" else "limit",
    }
  }

  action release = {
    nondet i = 0.to(MAX_RESERVATIONS).oneOf()
    val found = active.exists(r => r.id == i)
    all {
      active' = active.filter(r => r.id != i),
      released' = if (found) released.union(Set(i)) else released,
      nextId' = nextId,
      op' = "release", kind' = 0, amount' = 0, id' = i,
      result' = if (found) "ok" else if (released.contains(i)) "released" else "unknown",
    }
  }

  action step = any { reserve, release }
  val Safety = all {
    Set(0, 1, 2).forall(k => total(active, k) <= 2),
    active.forall(r => not(released.contains(r.id)) and r.id < nextId),
    released.forall(i => i < nextId),
    active.map(r => r.id).size() == active.size(),
  }
}

module Budget { import BudgetModel(ALLOW_OVERDRAFT = false, MAX_RESERVATIONS = 2).* }
module BudgetExtended { import BudgetModel(ALLOW_OVERDRAFT = false, MAX_RESERVATIONS = 4).* }
module BudgetOverdraft { import BudgetModel(ALLOW_OVERDRAFT = true, MAX_RESERVATIONS = 2).* }
