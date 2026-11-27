// Directory entries have distinct ranks and stay stable within each scan.
// The coordinator serializes scans. The available names can change between scans.
module JanitorSelection {
  type Phase = Idle | Scanning | Eof | BeforePersist | Published | PersistFailed |
    PersistTorn | PersistSyncFailed |
    ExpiredStartup | ExpiredScan | ExpiredDuringEnumeration | ExpiredEof | ExpiredPersist |
    NamespaceLimited | CoordinatorBusy | SelectionReadFailed | EnumerationFailed | EnumerationInterrupted
  type Scan = { phase: Phase, listing: Set[int], seen: Set[int], chosen: List[int],
    rawEntries: int, complete: bool, limited: bool, cursorDeferred: bool, scans: int }
  type Cursor = { name: int, generation: int }

  pure val pageLimit = 256
  pure val namespaceLimit = 100000
  pure val names = range(1, 2 * pageLimit + 2)
  pure val maxScans = 3
  pure val emptyCursor = { name: 0, generation: 0 }
  pure val emptyScan = { phase: Idle, listing: Set(), seen: Set(), chosen: [],
    rawEntries: 0, complete: false, limited: false, cursorDeferred: false, scans: 0 }

  var scan: Scan
  var saved: Cursor
  var startCursor: Cursor

  action init = all {
    scan' = emptyScan,
    saved' = emptyCursor,
    startCursor' = emptyCursor,
  }

  pure def terminal(phase: Phase): bool =
    Set(Published, PersistFailed, PersistTorn, PersistSyncFailed, ExpiredStartup, ExpiredScan,
      ExpiredDuringEnumeration, ExpiredEof, ExpiredPersist, NamespaceLimited, CoordinatorBusy,
      SelectionReadFailed, EnumerationFailed, EnumerationInterrupted)
      .contains(phase)

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

  pure def bucketOverflow(observed: Set[int], cursor: Cursor): bool = {
    val after = observed.filter(name => name > cursor.name).size()
    after > pageLimit or observed.size() - after > pageLimit
  }

  pure def observed(current: Scan, namesSeen: Set[int], cursor: Cursor): Scan =
    { ...current, seen: namesSeen, rawEntries: namesSeen.size(),
      limited: bucketOverflow(namesSeen, cursor) }

  action observe = {
    nondet name = scan.listing.exclude(scan.seen).oneOf()
    publish(observed(scan, scan.seen.union(Set(name)), saved), saved)
  }

  action finishEnumeration = all {
    scan.phase == Scanning and scan.seen == scan.listing,
    publish({ ...scan, phase: Eof, complete: true }, saved),
  }

  action scanNext = all {
    scan.phase == Scanning,
    if (scan.seen == scan.listing) finishEnumeration else observe,
  }

  pure def afterCursor(listing: Set[int], cursor: Cursor): Set[int] =
    listing.filter(name => name > cursor.name)

  pure def page(listing: Set[int], cursor: Cursor): List[int] = {
    val after = afterCursor(listing, cursor)
    val eligible = if (after.size() > 0) after else listing
    val ordered = names.select(name => eligible.contains(name))
    ordered.slice(0, if (ordered.length() > pageLimit) pageLimit else ordered.length())
  }

  pure def afterSelection(current: Scan, cursor: Cursor): Scan = {
    val chosen = page(current.seen, cursor)
    { ...current, phase: BeforePersist, chosen: chosen,
      cursorDeferred: {
        val after = afterCursor(current.seen, cursor).size()
        after > 0 and after < current.seen.size()
      } }
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

  action failPersist(failure: Phase): bool = all {
    scan.phase == BeforePersist and scan.chosen.length() > 0,
    publish({ ...scan, phase: failure },
      if (failure == PersistSyncFailed) advance(saved, scan.chosen) else saved),
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

  // The second page matches a recovery history that falls back to cursor 1.
  action recoveredCursorStep =
    if (scan.phase == Idle) begin(Set(1)) else advanceScenario

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

  action interruptedScan(failure: Phase): bool =
    if (scan.scans == 2 and scan.phase == BeforePersist) failPersist(failure)
    else advanceScenario

  action persistFailureStep = interruptedScan(PersistFailed)
  action tornPersistStep = interruptedScan(PersistTorn)
  action syncFailureStep = interruptedScan(PersistSyncFailed)

  // Record the final scan evidence once for each large listing.
  action boundaryScan(listing: Set[int], rawCount: int): bool = {
    val current = observed({ ...emptyScan, listing: listing, scans: scan.scans + 1 },
      listing, saved)
    val prepared = { ...current, rawEntries: rawCount, complete: rawCount <= namespaceLimit }
    val finalScan = if (prepared.complete) { ...afterSelection(prepared, saved), phase: Published }
      else { ...prepared, phase: NamespaceLimited }
    all {
      scan.scans < maxScans,
      scan' = finalScan,
      saved' = advance(saved, finalScan.chosen),
      startCursor' = saved,
    }
  }

  action capacityStep =
    if (scan.scans == 0) boundaryScan(1.to(pageLimit), pageLimit)
    else if (scan.scans == 1) boundaryScan((pageLimit + 1).to(2 * pageLimit + 1), pageLimit + 1)
    else boundaryScan(1.to(pageLimit + 1).union(Set(2 * pageLimit + 1)), pageLimit + 2)

  action namespaceLimitStep = all {
    scan.scans < 2,
    boundaryScan(Set(), namespaceLimit + scan.scans),
  }

  action unavailableStep =
    if (scan.phase == Scanning and scan.scans == 2)
      publish({ ...scan, phase: CoordinatorBusy }, saved)
    else if (scan.phase == Scanning and scan.scans == 3)
      publish({ ...scan, phase: SelectionReadFailed }, saved)
    else advanceScenario

  action startupStopsStep =
    if (scan.phase == Scanning and scan.scans == 2)
      publish({ ...scan, phase: ExpiredStartup }, saved)
    else if (scan.phase == Scanning and scan.scans == 3)
      publish({ ...scan, phase: ExpiredDuringEnumeration }, saved)
    else advanceScenario

  action enumerationErrorStep =
    if (scan.phase == Scanning and scan.scans == 2)
      publish({ ...scan, phase: EnumerationFailed }, saved)
    else if (scan.phase == Scanning and scan.scans == 3 and scan.seen == scan.listing)
      publish({ ...scan, phase: EnumerationInterrupted }, saved)
    else advanceScenario

  action partialEnumerationStep =
    if (scan.phase == Scanning and scan.scans == 2 and scan.seen.size() == 1)
      publish({ ...scan, phase: EnumerationInterrupted }, saved)
    else advanceScenario

  val publishedAfterCursor = scan.phase == Published and scan.cursorDeferred
  val wrapped = scan.phase == Published and scan.chosen.length() > 0 and
    scan.chosen.select(name => name <= startCursor.name).length() > 0
  val timedOutAfterEof = scan.phase == ExpiredEof
  val timedOutBeforeWrite = scan.phase == ExpiredPersist
  val timedOutBeforeEntry = scan.phase == ExpiredScan
  val completedEmptyScan = scan.phase == Published and scan.chosen.length() == 0

  val selectionIsExact = {
    val hasSelection = Set(BeforePersist, Published, PersistFailed, PersistTorn, PersistSyncFailed,
      ExpiredPersist).contains(scan.phase)
    val after = scan.seen.filter(name => name > startCursor.name)
    val eligible = if (after.size() == 0) scan.seen else after
    val count = scan.chosen.length()
    if (hasSelection) all {
      count <= pageLimit,
      count <= eligible.size(),
      count == pageLimit or count == eligible.size(),
      scan.chosen.select(name => eligible.contains(name)) == scan.chosen,
      0.to(count - 2).forall(i => scan.chosen.nth(i) < scan.chosen.nth(i + 1)),
      if (count == 0) eligible.size() == 0 else
        eligible.filter(name => name <= scan.chosen.nth(count - 1)).size() == count,
      scan.cursorDeferred == (after.size() > 0 and after.size() < scan.seen.size()),
    } else scan.chosen == [] and not(scan.cursorDeferred)
  }

  val Safety = all {
    scan.seen.subseteq(scan.listing),
    scan.rawEntries >= scan.seen.size(),
    (scan.phase == NamespaceLimited) == (scan.rawEntries > namespaceLimit),
    scan.rawEntries <= namespaceLimit + 1,
    Set(ExpiredStartup, ExpiredDuringEnumeration, CoordinatorBusy, SelectionReadFailed,
      EnumerationFailed).contains(scan.phase) implies scan.rawEntries == 0,
    scan.complete == Set(Eof, BeforePersist, Published, PersistFailed, PersistTorn, PersistSyncFailed,
      ExpiredEof, ExpiredPersist).contains(scan.phase),
    scan.complete implies scan.seen == scan.listing,
    scan.limited == bucketOverflow(scan.seen, startCursor),
    selectionIsExact,
    if (Set(Published, PersistSyncFailed).contains(scan.phase) and scan.chosen.length() > 0)
      saved.name == scan.chosen.nth(scan.chosen.length() - 1) and
        saved.generation == startCursor.generation + 1
    else saved == startCursor,
  }
}
