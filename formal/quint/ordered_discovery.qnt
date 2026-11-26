// Two idle workers, requested order [3, 1], and up to four sequential spool replies.
// Ends at ordered handoff (two pending applies) or at a missing-selection error.
module OrderedDiscoveryModel {
  const WRONG_ORDER: bool

  type ReadTarget = Scan | Fetch(int) | NoRead
  type Command = Init | Discover(int) | EndOfSpool | Body(int)
  type State = {
    found: Set[int], scanned: int, read: ReadTarget, applied: List[int],
    missing: int, mismatch: bool,
  }
  var s: State
  var dispatch: Command

  pure val requested = List(3, 1)
  pure val selected = Set(1, 3)

  pure def firstMissing(found: Set[int]): int =
    if (not(found.contains(3))) 3 else if (not(found.contains(1))) 1 else 0

  val dispatchOrder = if (WRONG_ORDER) List(1, 3) else requested

  action init = all {
    s' = { found: Set(), scanned: 0, read: Scan, applied: List(), missing: 0, mismatch: false },
    dispatch' = Init,
  }

  action discover(id: int): bool = all {
    s.read == Scan, s.scanned < 4,
    s' = { ...s, found: s.found.union(Set(id).intersect(selected)), scanned: s.scanned + 1 },
    dispatch' = Discover(id),
  }

  action endOfSpool = {
    val missing = firstMissing(s.found)
    all {
      s.read == Scan,
      s' = { ...s, read: if (missing == 0) Fetch(dispatchOrder.head()) else NoRead, missing: missing },
      dispatch' = EndOfSpool,
    }
  }

  val indexedId = match s.read {
    | Fetch(id) => id
    | _ => 0
  }

  // The rejected callback leaves the pending read available for a corrected reply.
  action body(id: int): bool = all {
    indexedId != 0,
    s' = if (id != indexedId) { ...s, mismatch: true }
      else {
        val applied = s.applied.append(id)
        val read = if (applied.length() < dispatchOrder.length()) Fetch(dispatchOrder.nth(applied.length())) else NoRead
        { ...s, applied: applied, read: read, mismatch: false }
      },
    dispatch' = Body(id),
  }

  action step = {
    nondet discovered = Set(1, 2, 3).oneOf()
    nondet loaded = 0.to(3).oneOf()
    any { discover(discovered), endOfSpool, body(loaded) }
  }

  action orderedStep =
    if (s.read == Scan) {
      if (s.scanned < 3) discover(List(1, 2, 1).nth(s.scanned))
      else if (s.scanned == 3) discover(3) else endOfSpool
    } else if (s.read == Fetch(3)) {
      if (dispatch == EndOfSpool) body(0)
      else if (dispatch == Body(0)) body(1)
      else body(3)
    } else body(1)

  action missingStep =
    if (s.scanned < 2) discover(s.scanned + 1) else endOfSpool

  val handedOffInOrder = s.applied == requested and s.read == NoRead
  val missingAtEof = s.missing == 3 and s.applied == List()
  val absentBodyRejected = s.mismatch and dispatch == Body(0)
  val wrongBodyRejected = s.mismatch and dispatch == Body(1) and s.read == Fetch(3)

  val Safety = all {
    s.found.subseteq(selected),
    s.applied.length() <= requested.length(),
    s.applied.indices().forall(i => s.applied.nth(i) == requested.nth(i)),
    s.read == Scan implies s.applied == List(),
    s.applied.length() > 0 implies s.found == selected,
    if (indexedId != 0) s.found == selected and s.applied.length() < requested.length() and
      indexedId == requested.nth(s.applied.length()) else true,
    s.missing != 0 implies (s.read == NoRead and s.applied == List() and s.missing == firstMissing(s.found)),
  }
}
module OrderedDiscovery { import OrderedDiscoveryModel(WRONG_ORDER = false).* }
module OrderedDiscoveryWrongOrder { import OrderedDiscoveryModel(WRONG_ORDER = true).* }
