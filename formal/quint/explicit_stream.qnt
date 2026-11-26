// One worker and two explicit IDs. Complete covers the ordinary worker lifecycle.
module ExplicitStreamModel {
  const REAPPLY_DUPLICATE: bool
  type Phase = Reading | Applying(int) | Verifying
  type Command = Init | Supply(int) | Complete | EndOfSpool | WrongWorker | Idle
  type Result = Accepted | Missing(int) | WorkerMismatch
  type Snapshot = {phase:Phase, consumed:int, admitted:List[int], seen:Set[int],
    killed:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  pure val selected = Set(1, 3)
  pure val maxRecords = 4

  action init = all {
    s' = {phase:Reading, consumed:0, admitted:[], seen:Set(), killed:0, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action supply(candidate:int):bool = all {
    s.phase == Reading and s.consumed < maxRecords,
    val matched = selected.contains(candidate) and
      (REAPPLY_DUPLICATE or not(s.seen.contains(candidate)))
    val consumed = { ...s, consumed:s.consumed+1, result:Accepted }
    publish(if (matched) { ...consumed, phase:Applying(candidate),
      admitted:s.admitted.append(candidate), seen:s.seen.union(Set(candidate)) }
      else consumed, Supply(candidate)),
  }

  action complete = all {
    s.phase != Reading and s.phase != Verifying,
    publish({ ...s, phase:Reading, killed:s.killed+1, result:Accepted }, Complete),
  }

  action eof = all {
    s.phase == Reading,
    val unmatched = selected.exclude(s.seen)
    if (unmatched.size() == 0)
      publish({ ...s, phase:Verifying, result:Accepted }, EndOfSpool)
    else publish(s.with("result", Missing(if (unmatched.contains(1)) 1 else 3)), EndOfSpool),
  }

  action wrongWorker = all {
    s.phase == Reading,
    publish(s.with("result", WorkerMismatch), WrongWorker),
  }

  action idle = publish(s, Idle)
  action step = any {
    nondet candidate = Set(1, 2, 3).oneOf()
    supply(candidate),
    complete,
    eof,
    wrongWorker,
    all { s.phase == Verifying, idle },
  }

  action filteredStep =
    if (s.phase == Verifying) idle
    else if (s.phase != Reading) complete
    else if (s.consumed < maxRecords) supply([2,3,3,1].nth(s.consumed))
    else eof
  action partialStep =
    if (s.phase == Verifying) idle
    else if (s.phase != Reading) complete
    else if (s.consumed == 0) supply(1)
    else if (s.consumed == 1 and dispatch != EndOfSpool) eof
    else if (s.consumed == 1) supply(3)
    else eof
  action workerStep =
    if (dispatch == Init) wrongWorker else filteredStep

  val Safety = all {
    s.seen.subseteq(selected),
    s.admitted.length() == s.seen.size(),
    s.killed + (if (s.phase == Reading or s.phase == Verifying) 0 else 1)
      == s.admitted.length(),
    s.phase == Verifying implies s.seen == selected,
    s.admitted.length() <= s.consumed,
  }
}

module ExplicitStream { import ExplicitStreamModel(REAPPLY_DUPLICATE=false).* }
module ExplicitStreamReappliesDuplicate { import ExplicitStreamModel(REAPPLY_DUPLICATE=true).* }
