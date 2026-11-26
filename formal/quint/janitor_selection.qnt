// Directory entries have distinct ranks and stay stable within each scan.
// The coordinator serializes scans. The available names can change between scans.
module JanitorSelection {
  type Phase = Idle | Scanning | Eof | BeforePersist | Published |
    ExpiredScan | ExpiredEof | ExpiredPersist
  type Scan = { phase: Phase, listing: Set[int], seen: Set[int], chosen: List[int],
    complete: bool, cursorDeferred: bool, scans: int }
  type Cursor = { name: int, generation: int }

  pure val names = [1, 2, 3, 4]
  pure val maxScans = 3
  pure val emptyCursor = { name: 0, generation: 0 }
  pure val emptyScan = { phase: Idle, listing: Set(), seen: Set(), chosen: [],
    complete: false, cursorDeferred: false, scans: 0 }

  var scan: Scan
  var saved: Cursor
  var startCursor: Cursor

  action init = all {
    scan' = emptyScan,
    saved' = emptyCursor,
    startCursor' = emptyCursor,
  }

  pure def terminal(phase: Phase): bool =
    Set(Published, ExpiredScan, ExpiredEof, ExpiredPersist).contains(phase)

  action begin(listing: Set[int]): bool = all {
    scan.scans < maxScans and (scan.phase == Idle or terminal(scan.phase)),
    scan' = { ...emptyScan, phase: Scanning, listing: listing, scans: scan.scans + 1 },
    saved' = saved,
    startCursor' = saved,
  }

  action publish(following: Scan, cursor: Cursor): bool = all {
    scan' = following,
    saved' = cursor,
    startCursor' = startCursor,
  }

  action observe = {
    nondet name = scan.listing.exclude(scan.seen).oneOf()
    publish({ ...scan, seen: scan.seen.union(Set(name)) }, saved)
  }

  action finishEnumeration = all {
    scan.phase == Scanning and scan.seen == scan.listing,
    publish({ ...scan, phase: Eof, complete: true }, saved),
  }

  action scanNext = all {
    scan.phase == Scanning,
    if (scan.seen == scan.listing) finishEnumeration else observe,
  }

  pure def page(listing: Set[int], cursor: Cursor): List[int] = {
    val ordered = names.select(name => listing.contains(name))
    val after = ordered.select(name => name > cursor.name)
    if (after.length() > 0) after else ordered
  }

  pure def afterSelection(current: Scan, cursor: Cursor): Scan = {
    val chosen = page(current.seen, cursor)
    { ...current, phase: BeforePersist, chosen: chosen,
      cursorDeferred: chosen.length() < current.seen.size() }
  }

  action choosePage = all {
    scan.phase == Eof,
    publish(afterSelection(scan, saved), saved),
  }

  pure def advance(cursor: Cursor, chosen: List[int]): Cursor =
    if (chosen.length() == 0) cursor
    else { name: chosen.nth(chosen.length() - 1), generation: cursor.generation + 1 }

  action persist = all {
    scan.phase == BeforePersist,
    publish({ ...scan, phase: Published }, advance(saved, scan.chosen)),
  }

  action expireBeforeScan = all {
    scan.phase == Scanning and scan.seen == Set(),
    publish({ ...scan, phase: ExpiredScan }, saved),
  }

  action expireAtEof(advanceEarly: bool): bool = all {
    scan.phase == Eof,
    publish({ ...scan, phase: ExpiredEof },
      if (advanceEarly) advance(saved, page(scan.seen, saved)) else saved),
  }

  action expireBeforePersist = all {
    scan.phase == BeforePersist,
    publish({ ...scan, phase: ExpiredPersist }, saved),
  }

  action chooseListing = {
    nondet listing = Set(Set(), Set(1, 2), Set(1, 2, 3), Set(1, 3, 4), Set(1, 2, 3, 4)).oneOf()
    begin(listing)
  }

  action step =
    if (scan.phase == Scanning) any { scanNext, expireBeforeScan }
    else if (scan.phase == Eof) any { choosePage, expireAtEof(false) }
    else if (scan.phase == BeforePersist) any { persist, expireBeforePersist }
    else chooseListing

  action advanceScenario =
    if (scan.phase == Scanning) scanNext
    else if (scan.phase == Eof) choosePage
    else if (scan.phase == BeforePersist) persist
    else begin(if (scan.scans == 0) Set(1, 2) else Set(1, 2, 3))

  action wrapStep = advanceScenario

  action emptyPageStep =
    if (scan.scans == 1 and scan.phase == Published) begin(Set()) else advanceScenario

  action eofScenario(advanceEarly: bool): bool =
    if (scan.scans == 2 and scan.phase == Eof) expireAtEof(advanceEarly)
    else advanceScenario

  action eofDeadlineStep = eofScenario(false)
  action prematureCursorStep = eofScenario(true)

  action persistDeadlineStep =
    if (scan.scans == 2 and scan.phase == BeforePersist) expireBeforePersist
    else advanceScenario

  val publishedAfterCursor = scan.phase == Published and scan.cursorDeferred
  val wrapped = scan.phase == Published and scan.chosen.length() > 0 and
    scan.chosen.select(name => name <= startCursor.name).length() > 0
  val timedOutAfterEof = scan.phase == ExpiredEof
  val timedOutBeforeWrite = scan.phase == ExpiredPersist
  val timedOutBeforeEntry = scan.phase == ExpiredScan
  val completedEmptyScan = scan.phase == Published and scan.chosen.length() == 0

  val selectionIsExact = {
    val hasSelection = Set(BeforePersist, Published, ExpiredPersist).contains(scan.phase)
    val after = scan.seen.filter(name => name > startCursor.name)
    val eligible = if (after.size() == 0) scan.seen else after
    if (hasSelection) all {
      scan.chosen.length() == eligible.size(),
      scan.chosen.select(name => eligible.contains(name)) == scan.chosen,
      0.to(scan.chosen.length() - 2).forall(i => scan.chosen.nth(i) < scan.chosen.nth(i + 1)),
      scan.cursorDeferred == (eligible.size() < scan.seen.size()),
    } else scan.chosen == [] and not(scan.cursorDeferred)
  }

  val Safety = all {
    scan.seen.subseteq(scan.listing),
    scan.complete == Set(Eof, BeforePersist, Published, ExpiredEof, ExpiredPersist).contains(scan.phase),
    scan.complete implies scan.seen == scan.listing,
    selectionIsExact,
    if (scan.phase == Published and scan.chosen.length() > 0)
      saved.name == scan.chosen.nth(scan.chosen.length() - 1) and
        saved.generation == startCursor.generation + 1
    else saved == startCursor,
  }
}
