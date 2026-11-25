// Full consumed sets are the oracle for the production floor/gaps compression.
module CompletionModel {
  const RESURRECT_RETIRED: bool
  type Request = { id: int, kind: int }
  var pending: Set[Request]
  var completed: Set[int]
  var retired: Set[int]
  var nextId: int
  var floor: int
  var gaps: Set[int]
  var op: str
  var id: int
  var kind: int
  var result: str

  pure def prefix(done: Set[int]): int =
    Set(0, 1, 2, 3, 4)
      .filter(n => Set(1, 2, 3, 4).filter(i => i <= n).forall(i => done.contains(i)))
      .fold(0, (best, n) => if (n > best) n else best)

  action init = all {
    pending' = Set(), completed' = Set(), retired' = Set(),
    nextId' = 1, floor' = 0, gaps' = Set(),
    op' = "init", id' = 0, kind' = 0, result' = "init",
  }
  action issueKind(requestKind: int): bool = {
    all {
      nextId <= 4,
      pending' = pending.union(Set({ id: nextId, kind: requestKind })),
      completed' = completed, retired' = retired,
      nextId' = nextId + 1, floor' = floor, gaps' = gaps,
      op' = "issue", id' = nextId, kind' = requestKind, result' = "issued",
    }
  }
  action completeWith(completionId: int, completionKind: int): bool = {
    val matching = pending.filter(request => request.id == completionId)
    val classification =
      if (retired.contains(completionId))
        if (RESURRECT_RETIRED) "ok" else "retired"
      else if (completionId <= floor or completed.contains(completionId)) "duplicate"
      else if (matching.size() == 0) "unknown"
      else if (completionKind != 2 and not(matching.exists(request => request.kind == completionKind))) "wrong"
      else "ok"
    val newCompleted = if (classification == "ok") completed.union(Set(completionId)) else completed
    val done = newCompleted.union(retired)
    val newFloor = prefix(done)
    all {
      pending' = if (classification == "ok") pending.filter(request => request.id != completionId) else pending,
      completed' = newCompleted, retired' = retired, nextId' = nextId,
      floor' = newFloor, gaps' = done.filter(i => i > newFloor),
      op' = "complete", id' = completionId, kind' = completionKind, result' = classification,
    }
  }
  action retire = {
    val newRetired = retired.union(pending.map(request => request.id))
    all {
      pending' = Set(), completed' = completed, retired' = newRetired,
      nextId' = nextId, floor' = nextId - 1, gaps' = Set(),
      op' = "retire", id' = 0, kind' = 0, result' = "retired_all",
    }
  }
  action issue = {
    nondet requestKind = Set(0, 1).oneOf()
    issueKind(requestKind)
  }
  action complete = {
    nondet completionId = Set(0, 1, 2, 3, 4, 5).oneOf()
    nondet completionKind = Set(0, 1, 2).oneOf()
    completeWith(completionId, completionKind)
  }
  action step = any { issue, complete, retire }

  // These schedules constrain inputs only; the same actions compute expectations.
  action gapStep =
    if (nextId <= 4) issueKind(0)
    else if (pending.size() > 0) {
      val largest = pending.map(r => r.id).fold(0, (best, i) => if (i > best) i else best)
      completeWith(largest, 0)
    } else completeWith(4, 0)
  action wrongKindStep =
    if (nextId == 1) issueKind(0)
    else if (result == "issued") completeWith(1, 1)
    else completeWith(1, 2)
  val issued = Set(1, 2, 3, 4).filter(i => i < nextId)
  val pendingIds = pending.map(request => request.id)
  val done = completed.union(retired)
  val Safety = all {
    nextId >= 1, nextId <= 5,
    pending.forall(request => Set(0, 1).contains(request.kind)),
    pending.size() == pendingIds.size(),
    pendingIds.union(done) == issued,
    pendingIds.intersect(completed) == Set(),
    pendingIds.intersect(retired) == Set(),
    completed.intersect(retired) == Set(),
    floor == prefix(done), floor >= 0, floor < nextId,
    gaps == done.filter(i => i > floor),
  }
}
module Completion { import CompletionModel(RESURRECT_RETIRED = false).* }
module CompletionResurrectRetired { import CompletionModel(RESURRECT_RETIRED = true).* }
