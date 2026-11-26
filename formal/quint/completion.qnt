// Full consumed sets are the oracle for the production floor/gaps compression.
module CompletionModel {
  const RESURRECT_RETIRED: bool
  // 0: budget, 1: reset, 2: generic failure reply, 3: cleanup.
  type Request = { id: int, kind: int, reservations: List[int] }
  pure val CleanupReservations = List(1, 2)
  pure val CleanupReplies = Set(List(), List(1), List(1, 3), List(1, 2), List(2, 1), List(1, 1, 2))
  var pending: Set[Request]
  var completed: Set[int]
  var retired: Set[int]
  var nextId: int
  var floor: int
  var gaps: Set[int]
  var op: str
  var id: int
  var kind: int
  var reservations: List[int]
  var result: str

  pure def prefix(done: Set[int]): int =
    Set(0, 1, 2, 3, 4)
      .filter(n => Set(1, 2, 3, 4).filter(i => i <= n).forall(i => done.contains(i)))
      .fold(0, (best, n) => if (n > best) n else best)

  pure def matchesReservations(requested: List[int], released: List[int]): bool = {
    val expected = requested.indices().map(i => requested.nth(i))
    val received = released.indices().map(i => released.nth(i))
    expected == received and expected.size() == requested.length() and received.size() == released.length()
  }

  action init = all {
    pending' = Set(), completed' = Set(), retired' = Set(),
    nextId' = 1, floor' = 0, gaps' = Set(),
    op' = "init", id' = 0, kind' = 0, reservations' = List(), result' = "init",
  }
  action issueKind(requestKind: int): bool = {
    val requested = if (requestKind == 3) CleanupReservations else List()
    all {
      nextId <= 4,
      requestKind != 3 or nextId == 1,
      pending' = pending.union(Set({ id: nextId, kind: requestKind, reservations: requested })),
      completed' = completed, retired' = retired,
      nextId' = nextId + 1, floor' = floor, gaps' = gaps,
      op' = "issue", id' = nextId, kind' = requestKind, reservations' = requested, result' = "issued",
    }
  }
  action completeWith(completionId: int, completionKind: int, released: List[int]): bool = {
    val matching = pending.filter(request => request.id == completionId)
    val classification =
      if (retired.contains(completionId))
        if (RESURRECT_RETIRED) "ok" else "retired"
      else if (completionId <= floor or completed.contains(completionId)) "duplicate"
      else if (matching.size() == 0) "unknown"
      else if (completionKind != 2 and not(matching.exists(request => request.kind == completionKind))) "wrong"
      else if (completionKind == 3 and not(matching.exists(request => matchesReservations(request.reservations, released))))
        "reservation_mismatch"
      else "ok"
    val newCompleted = if (classification == "ok") completed.union(Set(completionId)) else completed
    val done = newCompleted.union(retired)
    val newFloor = prefix(done)
    all {
      pending' = if (classification == "ok") pending.filter(request => request.id != completionId) else pending,
      completed' = newCompleted, retired' = retired, nextId' = nextId,
      floor' = newFloor, gaps' = done.filter(i => i > newFloor),
      op' = "complete", id' = completionId, kind' = completionKind, reservations' = released, result' = classification,
    }
  }
  action retire = {
    val newRetired = retired.union(pending.map(request => request.id))
    all {
      pending' = Set(), completed' = completed, retired' = newRetired,
      nextId' = nextId, floor' = nextId - 1, gaps' = Set(),
      op' = "retire", id' = 0, kind' = 0, reservations' = List(), result' = "retired_all",
    }
  }
  action issue = {
    nondet requestKind = (if (nextId == 1) Set(0, 1, 3) else Set(0, 1)).oneOf()
    issueKind(requestKind)
  }
  action complete = {
    nondet completionId = Set(0, 1, 2, 3, 4, 5).oneOf()
    nondet completionKind = Set(0, 1, 2, 3).oneOf()
    val hasCleanup = pending.exists(request => request.id == completionId and request.kind == 3)
    nondet released = (if (completionKind == 3 and hasCleanup) CleanupReplies else Set(List())).oneOf()
    completeWith(completionId, completionKind, released)
  }
  action step = any { issue, complete, retire }

  // These schedules constrain inputs only; the same actions compute expectations.
  action gapStep =
    if (nextId <= 4) issueKind(0)
    else if (pending.size() > 0) {
      val largest = pending.map(r => r.id).fold(0, (best, i) => if (i > best) i else best)
      completeWith(largest, 0, List())
    } else completeWith(4, 0, List())
  action wrongKindStep =
    if (nextId == 1) issueKind(0)
    else if (op == "issue") completeWith(1, 1, List())
    else completeWith(1, 2, List())
  action cleanupStep =
    if (nextId == 1) issueKind(3)
    else if (nextId == 2) issueKind(0)
    else if (op == "issue") completeWith(2, 0, List())
    else if (id == 2) completeWith(1, 3, List(1, 1, 2))
    else if (reservations == List(1, 1, 2)) completeWith(1, 3, List(1))
    else if (reservations == List(1)) completeWith(1, 3, List(1, 3))
    else completeWith(1, 3, List(2, 1))
  val issued = Set(1, 2, 3, 4).filter(i => i < nextId)
  val pendingIds = pending.map(request => request.id)
  val done = completed.union(retired)
  val duplicateReservationsRejected = result == "reservation_mismatch" and reservations == List(1, 1, 2)
  val missingReservationRejected = result == "reservation_mismatch" and reservations == List(1)
  val unexpectedReservationRejected = result == "reservation_mismatch" and reservations == List(1, 3)
  val reversedReservationsAccepted = kind == 3 and result == "ok" and reservations == List(2, 1)
  val mismatchPreservesGap = duplicateReservationsRejected and floor == 0 and gaps == Set(2)
  val cleanupClosesGap = reversedReservationsAccepted and floor == 2 and gaps == Set()
  val Safety = all {
    nextId >= 1, nextId <= 5,
    pending.forall(request => Set(0, 1, 3).contains(request.kind) and
      (if (request.kind == 3) request.id == 1 and request.reservations == CleanupReservations
       else request.reservations == List())),
    pending.size() == pendingIds.size(),
    pendingIds.union(done) == issued,
    pendingIds.intersect(completed) == Set(),
    pendingIds.intersect(retired) == Set(),
    completed.intersect(retired) == Set(),
    floor == prefix(done), floor >= 0, floor < nextId,
    gaps == done.filter(i => i > floor),
    (kind == 3 and result == "ok") implies Set(CleanupReservations, List(2, 1)).contains(reservations),
    result == "reservation_mismatch" implies all {
      kind == 3,
      pending.contains({ id: id, kind: 3, reservations: CleanupReservations }),
      not(done.contains(id)),
      not(Set(CleanupReservations, List(2, 1)).contains(reservations)),
    },
  }
}
module Completion { import CompletionModel(RESURRECT_RETIRED = false).* }
module CompletionResurrectRetired { import CompletionModel(RESURRECT_RETIRED = true).* }
