// Two targets, one worker, no diagnostics. A normal non-final reply has no spool.
module AnalysisSpoolModel {
  const READ_ORDINARY_TRUNCATION: bool
  type Mode = Ordinary | Explicit
  type Spool = Absent | Empty | Populated
  type Reply = {target:int, spool:Spool, truncated:bool}
  type Phase = Ready | Analyzing | Reading | Verifying | Stopped
  type Command = Init | Start(Mode) | Finish(Reply) | Deadline
  type Result = Accepted | Duplicate | Late | MissingSpool | MissingCandidate
  type Snapshot = {mode:Mode, phase:Phase, target:int, completed:Set[int],
    truncated:bool, exit:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {mode:Ordinary, phase:Ready, target:0, completed:Set(),
      truncated:false, exit:0, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action start(mode:Mode):bool =
    publish({ ...s, mode:mode, phase:Analyzing, target:1 }, Start(mode))

  action reject(reply:Reply, result:Result):bool =
    publish(s.with("result", result), Finish(reply))

  action finish(reply:Reply):bool =
    if (s.completed.contains(reply.target)) reject(reply, Duplicate)
    else if (s.phase == Stopped) reject(reply, Late)
    else {
      val nextFile = reply.spool == Absent and not(reply.truncated) and s.target == 1
      val evaluated = { ...s, completed:s.completed.union(Set(s.target)), truncated:reply.truncated,
        exit:if (reply.truncated) 4 else 0, result:Accepted }
      if (nextFile) publish(evaluated.with("target", 2), Finish(reply))
      else if (s.mode == Explicit and reply.spool == Absent) reject(reply, MissingSpool)
      else if (s.mode == Explicit and reply.spool == Empty) reject(reply, MissingCandidate)
      else {
        val canRead = reply.spool == Populated and
          (not(reply.truncated) or s.mode == Explicit or READ_ORDINARY_TRUNCATION)
        publish(evaluated.with("phase", if (canRead) Reading else Verifying), Finish(reply))
      }
    }

  action answer(target:int, spool:Spool, truncated:bool):bool =
    finish({target:target, spool:spool, truncated:truncated})

  action deadline =
    publish({ ...s, phase:Stopped, exit:4, result:Accepted }, Deadline)

  action step = if (s.phase == Ready) {
    nondet mode = Set(Ordinary, Explicit).oneOf()
    start(mode)
  } else any {
    nondet replyTarget = 1.to(s.target).oneOf()
    nondet spool = Set(Absent, Empty, Populated).oneOf()
    nondet truncated = Bool.oneOf()
    all {
      replyTarget == 2 or truncated or spool == Absent,
      answer(replyTarget, spool, truncated),
    },
    all { s.phase == Analyzing, deadline },
  }

  action scan(mode:Mode):bool =
    if (s.phase == Ready) start(mode) else answer(s.target, Absent, false)

  action ordinaryTruncatedStep =
    if (s.phase == Ready) start(Ordinary) else answer(1, Populated, true)
  action explicitTruncatedStep =
    if (s.phase == Ready) start(Explicit) else answer(1, Populated, true)
  action ordinaryEmptyStep =
    if (s.target < 2) scan(Ordinary) else answer(2, Empty, false)
  action ordinaryNoSpoolStep = scan(Ordinary)
  action explicitRetryStep =
    if (s.target < 2) scan(Explicit)
    else if (dispatch == Finish({target:1, spool:Absent, truncated:false}))
      answer(2, Absent, false)
    else if (s.result == MissingSpool) answer(2, Empty, false)
    else answer(2, Populated, false)
  action deadlineStep =
    if (s.target < 2) scan(Explicit)
    else if (s.phase == Analyzing) deadline
    else answer(2, Populated, false)

  val Safety = all {
    s.target == 2 implies s.completed.contains(1),
    s.phase == Reading implies s.completed.contains(s.target) and
      (s.target == 2 or s.truncated) and (s.mode == Explicit or not(s.truncated)),
    s.phase == Verifying implies s.mode == Ordinary and s.completed.contains(s.target),
    s.phase == Stopped implies not(s.completed.contains(s.target)) and s.exit == 4,
    s.truncated implies s.exit == 4,
  }
}

module AnalysisSpool { import AnalysisSpoolModel(READ_ORDINARY_TRUNCATION=false).* }
module AnalysisSpoolReadsTruncation { import AnalysisSpoolModel(READ_ORDINARY_TRUNCATION=true).* }
