// One claimed root with a nested payload and two control markers.
module JanitorRemoval {
  type Entry = Payload | Branch | Heartbeat | Lease | Root
  type Binding = Original | Replacement
  type Limit = FullSlice | OneExamined | StopBefore | StopAfterPayload |
    StopAfterBranch | StopAfterHeartbeat
  type Command = Start | RunSlice(Limit) | ReplaceName
  type Result = NotRun | Exhausted | Deferred | Removed | Refused
  type State = {
    remaining: Set[Entry],
    binding: Binding,
    remainingAtReplacement: Set[Entry],
    acknowledged: int,
    operations: int,
    sliceRemoved: int,
    command: Command,
    result: Result,
  }

  pure val entries = Set(Payload, Branch, Heartbeat, Lease, Root)
  pure val maxOperations = 6
  var s: State

  action init = s' = {
    remaining: entries, binding: Original, remainingAtReplacement: Set(),
    acknowledged: 0, operations: 0, sliceRemoved: 0, command: Start, result: NotRun,
  }

  action recordSlice(limit: Limit, removed: Set[Entry], result: Result): bool = all {
    s.operations < maxOperations,
    s.remaining.contains(Root),
    s' = { ...s,
      remaining: s.remaining.exclude(removed),
      acknowledged: s.acknowledged + removed.size(),
      operations: s.operations + 1,
      sliceRemoved: removed.size(), command: RunSlice(limit), result: result,
    },
  }

  action runSlice(limit: Limit): bool =
    if (limit == StopBefore) recordSlice(limit, Set(), Exhausted)
    else if (s.binding == Replacement) all {
      Set(FullSlice, OneExamined).contains(limit),
      recordSlice(limit, Set(), Refused),
    }
    else match limit {
      | OneExamined => recordSlice(limit, Set(), Exhausted)
      | StopAfterPayload => all {
        s.remaining.contains(Payload),
        recordSlice(limit, Set(Payload), Deferred),
      }
      | StopAfterBranch => all {
        s.remaining.contains(Branch),
        recordSlice(limit, s.remaining.intersect(Set(Payload, Branch)), Deferred),
      }
      | StopAfterHeartbeat => all {
        not(s.remaining.contains(Branch)),
        s.remaining.contains(Heartbeat),
        recordSlice(limit, Set(Heartbeat), Deferred),
      }
      | _ => recordSlice(limit, s.remaining, Removed)
    }

  action replaceBinding = all {
    s.operations < maxOperations,
    s.binding == Original,
    s' = { ...s, binding: Replacement, remainingAtReplacement: s.remaining,
      operations: s.operations + 1,
      sliceRemoved: 0, command: ReplaceName, result: NotRun },
  }

  action replace = all { s.remaining.contains(Root), replaceBinding }

  action step = any {
    replace,
    {
      nondet limit = Set(FullSlice, OneExamined, StopBefore, StopAfterPayload,
        StopAfterBranch, StopAfterHeartbeat).oneOf()
      runSlice(limit)
    },
  }

  action limitsStep =
    if (s.operations == 0) runSlice(OneExamined)
    else if (s.operations == 1) runSlice(StopBefore)
    else if (s.operations == 2) runSlice(StopAfterPayload)
    else runSlice(FullSlice)

  action finalizationStep =
    if (s.operations == 0) runSlice(StopAfterBranch)
    else if (s.operations == 1) runSlice(StopAfterHeartbeat)
    else if (s.operations == 2) runSlice(OneExamined)
    else runSlice(FullSlice)

  action replacementStep =
    if (s.operations == 0) runSlice(StopAfterPayload)
    else if (s.operations == 1) replace
    else all { s.operations == 2, runSlice(FullSlice) }

  // An acknowledged Windows root unlink consumes the handle. A later visit
  // to a same-name replacement still lacks authority, even with its identity.
  action consumedReplacementStep =
    if (s.operations == 0) runSlice(FullSlice)
    else if (s.operations == 1) replaceBinding
    else all {
      s.operations == 2,
      s.binding == Replacement and s.remaining == Set(),
      s' = { ...s, operations: s.operations + 1, command: RunSlice(FullSlice),
        result: Refused, sliceRemoved: 0 },
    }

  action removeLeaseEarlyStep = recordSlice(StopAfterPayload, Set(Lease), Deferred)

  val Safety = all {
    s.acknowledged + s.remaining.size() == entries.size(),
    s.remaining.contains(Payload) implies s.remaining.contains(Branch),
    s.remaining.contains(Branch) implies Set(Heartbeat, Lease).subseteq(s.remaining),
    s.remaining.contains(Heartbeat) implies s.remaining.contains(Lease),
    not(s.remaining.contains(Root)) implies s.remaining == Set(),
    (s.binding == Replacement) implies s.remaining == s.remainingAtReplacement,
    Set(NotRun, Exhausted, Refused).contains(s.result) implies s.sliceRemoved == 0,
    (s.result == Deferred) implies s.sliceRemoved > 0 and s.remaining.contains(Root),
    (s.result == Removed) implies s.remaining == Set() and s.sliceRemoved > 0,
  }
}
