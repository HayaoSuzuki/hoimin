// The startup janitor observes at most two selected roots from three matches.
// Each step is one ReclaimLedger call. Paths, locks and actual deletion are external.
module JanitorReclaim {
  type Outcome = Uninspected | Pending | Preserved | Failed | Absent | Reclaimed
  type Evidence = { outcome: Outcome, examined: int, removed: int }
  type Removal = { root: int, examined: int, removed: int, deleted: bool, absent: bool }
  type Command = Init | Inspect(int) | Hold(int) | Fail(int) | NoticeAbsence(int) | Remove(Removal)
  type Summary = { inspected: int, reclaimed: int, absent: int,
    preserved: int, unresolved: int, deleteFailed: int, cleanupIncomplete: int,
    examined: int, removed: int, uninspected: int, omitted: int }

  pure val roots = Set(0, 1)
  pure val matching = 3
  pure val empty = { outcome: Uninspected, examined: 0, removed: 0 }
  var evidence: int -> Evidence
  var command: Command
  var summary: Summary

  pure def count(entries: int -> Evidence, outcome: Outcome): int =
    roots.filter(root => entries.get(root).outcome == outcome).size()

  pure def project(entries: int -> Evidence): Summary = {
    val uninspected = count(entries, Uninspected)
    val incomplete = count(entries, Pending)
    val failed = count(entries, Failed)
    { inspected: roots.size() - uninspected, reclaimed: count(entries, Reclaimed),
      absent: count(entries, Absent), preserved: count(entries, Preserved),
      unresolved: incomplete + failed, deleteFailed: failed, cleanupIncomplete: incomplete,
      examined: roots.fold(0, (total, root) => total + entries.get(root).examined),
      removed: roots.fold(0, (total, root) => total + entries.get(root).removed),
      uninspected: uninspected, omitted: matching - roots.size() }
  }

  action publish(entries: int -> Evidence, input: Command): bool = all {
    evidence' = entries,
    summary' = project(entries),
    command' = input,
  }

  action init = publish(roots.mapBy(_ => empty), Init)

  action inspect = {
    val root = roots.size() - count(evidence, Uninspected)
    all {
      roots.contains(root),
      publish(evidence.set(root, { ...empty, outcome: Pending }), Inspect(root)),
    }
  }

  action classify(root: int, outcome: Outcome, input: Command): bool = all {
    evidence.get(root).outcome == Pending,
    publish(evidence.set(root, { ...evidence.get(root), outcome: outcome }), input),
  }

  pure def canRecord(previous: Evidence, observation: Removal): bool = all {
    Set(Pending, Failed).contains(previous.outcome),
    observation.examined >= previous.examined,
    observation.removed >= previous.removed,
    observation.deleted or observation.absent or observation.examined > previous.examined
      or observation.removed > previous.removed,
  }

  pure def recorded(previous: Evidence, observation: Removal, acknowledged: bool): Evidence =
    { outcome: if (acknowledged) Reclaimed
        else if (observation.absent) Absent else previous.outcome,
      examined: observation.examined, removed: observation.removed }

  action recordWithAcknowledgement(observation: Removal, acknowledged: bool): bool = {
    val previous = evidence.get(observation.root)
    all {
      canRecord(previous, observation),
      publish(evidence.set(observation.root, recorded(previous, observation, acknowledged)),
        Remove(observation)),
    }
  }

  action record(observation: Removal): bool = recordWithAcknowledgement(observation, observation.deleted)

  action recordProgress(root: int): bool = {
    nondet examined = Set(0, 1, 2).oneOf()
    nondet removed = Set(0, 1, 2).oneOf()
    nondet deleted = Bool.oneOf()
    nondet absent = Bool.oneOf()
    record({ root: root, examined: examined, removed: removed, deleted: deleted, absent: absent })
  }

  action step = {
    nondet root = roots.oneOf()
    any {
      inspect,
      classify(root, Preserved, Hold(root)),
      classify(root, Failed, Fail(root)),
      classify(root, Absent, NoticeAbsence(root)),
      recordProgress(root),
    }
  }

  action acknowledgementScenario(acknowledged: bool): bool =
    if (summary.inspected == 0) inspect
    else if (evidence.get(0).outcome == Pending) classify(0, Failed, Fail(0))
    else if (evidence.get(0).outcome == Failed)
      recordWithAcknowledgement(
        { root: 0, examined: 1, removed: 1, deleted: true, absent: true }, acknowledged)
    else if (summary.inspected == 1) inspect
    else classify(1, Absent, NoticeAbsence(1))

  action acknowledgedStep = acknowledgementScenario(true)
  action acknowledgementLostStep = acknowledgementScenario(false)

  action partialStep =
    if (summary.inspected == 0) inspect
    else if (evidence.get(0).outcome == Pending) classify(0, Failed, Fail(0))
    else record({ root: 0, examined: 2, removed: 1, deleted: false, absent: false })

  // The first selected root is acknowledged as removed. The second can
  // remain uninspected at this prefix or be observed absent on a later visit.
  action selectedRootsScenario(second: Outcome, afterInspection: Command): bool =
    if (summary.inspected == 0) inspect
    else if (evidence.get(0).outcome == Pending)
      record({ root: 0, examined: 0, removed: 1, deleted: true, absent: false })
    else if (summary.inspected == 1) inspect
    else classify(1, second, afterInspection)

  action selectedRootsStep = selectedRootsScenario(Absent, NoticeAbsence(1))
  action shellReportStep = selectedRootsScenario(Preserved, Hold(1))

  val inspectedSecond = summary.inspected == 2
  val preservedRoot = summary.preserved > 0
  val observedAbsence = summary.absent > 0
  val acknowledgedRemoval = summary.reclaimed > 0
  val failedWithProgress = roots.exists(root => {
    val entry = evidence.get(root)
    entry.outcome == Failed and entry.removed > 0
  })
  val acknowledgementWins = match command {
    | Remove(observation) =>
      observation.deleted implies evidence.get(observation.root).outcome == Reclaimed
    | _ => true
  }

  val Safety = all {
    summary.inspected == summary.reclaimed + summary.absent + summary.preserved + summary.unresolved,
    summary.inspected + summary.uninspected == roots.size(),
    summary.unresolved == summary.deleteFailed + summary.cleanupIncomplete,
    acknowledgementWins,
    match command {
      | Remove(observation) =>
        not(observation.deleted) and observation.absent implies
          evidence.get(observation.root).outcome == Absent
      | _ => true
    },
  }
}
