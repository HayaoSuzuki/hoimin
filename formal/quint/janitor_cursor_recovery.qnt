// Model the records visible when reopening the coordinator file after each event.
// The coordinator serializes updates; cursor names are represented by ranks.
module JanitorCursorRecovery {
  type Record = { generation: int, cursor: int }
  type Slot = Invalid | Valid(Record)
  type Reading = Unusable | Readable({ index: int, record: Record })
  type WriteMode = Commit | BeforeWrite | Torn | SyncError
  type Request = { cursor: int, mode: WriteMode }
  type Command = Init | Save(Request) | Damage({ index: int, wasCurrent: bool }) |
    InstallConflict(Record) | InstallMaximum(Record) | RejectOverflow(int)
  type Journal = { slots: int -> Slot, observed: Reading, command: Command, operations: int }

  pure val slotIds = Set(0, 1)
  pure val maxOperations = 8
  pure val maxGeneration = 18446744073709551615
  pure val initial = { generation: 0, cursor: 0 }
  var s: Journal

  action init = s' = {
    slots: slotIds.mapBy(_ => Valid(initial)),
    observed: Readable({ index: 0, record: initial }),
    command: Init, operations: 0,
  }

  pure def generation(slot: Slot): int = match slot {
    | Invalid => -1
    | Valid(record) => record.generation
  }

  pure def currentSlot(slots: int -> Slot): int =
    if (generation(slots.get(0)) >= generation(slots.get(1))) 0 else 1

  pure def conflictingPair(slots: int -> Slot): bool =
    generation(slots.get(0)) == generation(slots.get(1)) and slots.get(0) != slots.get(1)

  pure def read(slots: int -> Slot, preferOlder: bool): Reading = {
    val newest = currentSlot(slots)
    val fallback = 1 - newest
    val index = if (preferOlder and slots.get(fallback) != Invalid) fallback else newest
    if (conflictingPair(slots)) Unusable
    else match slots.get(index) {
        | Invalid => Unusable
        | Valid(record) => Readable({ index: index, record: record })
    }
  }

  action publish(slots: int -> Slot, command: Command, preferOlder: bool): bool =
    s' = { slots: slots, observed: read(slots, preferOlder),
      command: command, operations: s.operations + 1 }

  pure def written(slots: int -> Slot, request: Request): int -> Slot = {
    val active = currentSlot(slots)
    val record = { generation: generation(slots.get(active)) + 1, cursor: request.cursor }
    if (request.mode == BeforeWrite or generation(slots.get(active)) == maxGeneration) slots
    else slots.set(1 - active, if (request.mode == Torn) Invalid else Valid(record))
  }

  action save(request: Request, preferOlder: bool): bool = all {
    s.operations < maxOperations and s.observed != Unusable,
    publish(written(s.slots, request), Save(request), preferOlder),
  }

  action damage(index: int): bool = all {
    s.operations < maxOperations and s.slots.get(index) != Invalid,
    publish(s.slots.set(index, Invalid),
      Damage({ index: index, wasCurrent: index == currentSlot(s.slots) }), false),
  }

  action chooseSave: bool = {
    nondet cursor = Set(1, 2, 3).oneOf()
    nondet mode = Set(Commit, BeforeWrite, Torn, SyncError).oneOf()
    save({ cursor: cursor, mode: mode }, false)
  }

  action chooseDamage: bool = {
    nondet index = slotIds.oneOf()
    damage(index)
  }

  action step = any { chooseSave, chooseDamage }

  action recoverStep =
    if (s.operations == 0) save({ cursor: 1, mode: Commit }, false)
    else if (s.operations == 1) save({ cursor: 2, mode: Commit }, false)
    else if (s.operations == 2) damage(0)
    else if (s.operations == 3) save({ cursor: 3, mode: Commit }, false)
    else if (s.operations == 4) damage(1)
    else damage(0)

  action interruptedWriteStep =
    if (s.operations == 0) save({ cursor: 1, mode: Commit }, false)
    else if (s.operations == 1) save({ cursor: 2, mode: Torn }, false)
    else if (s.operations == 2) save({ cursor: 2, mode: BeforeWrite }, false)
    else if (s.operations == 3) save({ cursor: 3, mode: Commit }, false)
    else if (s.operations == 4) save({ cursor: 1, mode: SyncError }, false)
    else if (s.operations == 5) damage(1)
    else if (s.operations == 6) save({ cursor: 2, mode: Commit }, false)
    else damage(0)

  action preferOlderStep = save({ cursor: 1, mode: Commit }, true)

  action conflictingGenerationStep = {
    val conflicting = { generation: 0, cursor: 1 }
    all {
      s.operations == 0,
      publish(s.slots.set(1, Valid(conflicting)), InstallConflict(conflicting), false),
    }
  }

  val rejectedConflict = conflictingPair(s.slots) and s.observed == Unusable
  action installMaximum = {
    val record = { generation: maxGeneration, cursor: 1 }
    all {
      s.operations == 0,
      publish(s.slots.set(1, Valid(record)), InstallMaximum(record), false),
    }
  }

  action attemptOverflow = all {
    s.operations == 1,
    publish(written(s.slots, { cursor: 2, mode: Commit }), RejectOverflow(2), false),
  }

  action overflowStep = if (s.operations == 0) installMaximum else attemptOverflow

  val rejectedOverflow = s.command == RejectOverflow(2) and
    s.observed == Readable({ index: 1, record: { generation: maxGeneration, cursor: 1 } })
  val savedCursor = s.observed == Readable({ index: 1, record: { generation: 1, cursor: 1 } })
  val fellBackAfterDamage = match s.command {
    | Damage(event) => event.wasCurrent and s.observed != Unusable
    | _ => false
  }
  val rejectedBothSlots = s.observed == Unusable
  val tornWrite = match s.command {
    | Save(request) => request.mode == Torn and s.observed != Unusable
    | _ => false
  }
  val failedBeforeWrite = match s.command {
    | Save(request) => request.mode == BeforeWrite
    | _ => false
  }
  val fullWriteWithSyncError = match s.command {
    | Save(request) => request.mode == SyncError
    | _ => false
  }

  val readingIsCurrent = match s.observed {
    | Unusable => conflictingPair(s.slots) or slotIds.forall(index => s.slots.get(index) == Invalid)
    | Readable(entry) => all {
      not(conflictingPair(s.slots)),
      s.slots.get(entry.index) == Valid(entry.record),
      slotIds.forall(index => generation(s.slots.get(index)) <= entry.record.generation),
      (generation(s.slots.get(0)) == generation(s.slots.get(1))) implies entry.index == 0,
    }
  }
  val Safety = readingIsCurrent and
    slotIds.forall(index => generation(s.slots.get(index)) <= maxGeneration)
}
