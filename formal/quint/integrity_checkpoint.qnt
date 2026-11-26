// Two integrity barriers in an ordinary run; a deadline can reissue the final request.
module IntegrityCheckpointModel {
  const ACCEPT_WRONG_FINAL: bool
  type Checkpoint = PostMaterialization | PreFinalReport | PreAnalysis | Periodic | CleanupCheck
  type Request = AfterCopy | BeforeReport | Replacement
  type Phase = Pending(Request) | Running | Cleaning | Diagnostic
  type Reply = {request:Request, checkpoint:Checkpoint}
  type Command = Init | ReplyTo(Reply) | RunToFinal | Deadline | FailVerification | Idle
  type Result = Accepted | Mismatch | Duplicate | Late
  type Snapshot = {phase:Phase, completed:Set[Request], retired:Set[Request],
    evidence:Set[Checkpoint], exit:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {phase:Pending(AfterCopy), completed:Set(), retired:Set(), evidence:Set(),
      exit:0, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  pure def expected(request:Request):Checkpoint =
    if (request == AfterCopy) PostMaterialization else PreFinalReport

  action answer(request:Request, checkpoint:Checkpoint):bool = {
    val command = ReplyTo({request:request, checkpoint:checkpoint})
    if (s.completed.contains(request)) publish(s.with("result", Duplicate), command)
    else if (s.retired.contains(request)) publish(s.with("result", Late), command)
    else all {
      s.phase == Pending(request),
      if (checkpoint == expected(request) or (ACCEPT_WRONG_FINAL and request != AfterCopy))
        publish({ ...s, phase:if (request == AfterCopy) Running else Cleaning,
          completed:s.completed.union(Set(request)), evidence:s.evidence.union(Set(checkpoint)),
          result:Accepted }, command)
      else publish(s.with("result", Mismatch), command),
    }
  }

  action finishWork = all {
    s.phase == Running,
    publish({ ...s, phase:Pending(BeforeReport), result:Accepted }, RunToFinal),
  }

  action deadline = all {
    s.phase == Pending(BeforeReport),
    publish({ ...s, phase:Pending(Replacement), retired:Set(BeforeReport),
      exit:4, result:Accepted }, Deadline),
  }

  action verificationFailure(request:Request):bool = all {
    s.phase == Pending(request),
    publish({ ...s, phase:Diagnostic, completed:s.completed.union(Set(request)),
      exit:2, result:Accepted }, FailVerification),
  }

  action idle = publish(s, Idle)
  action step = any {
    nondet request = Set(AfterCopy, BeforeReport, Replacement).oneOf()
    nondet checkpoint = Set(PostMaterialization, PreFinalReport, PreAnalysis, Periodic, CleanupCheck).oneOf()
    answer(request, checkpoint),
    nondet request = Set(AfterCopy, BeforeReport, Replacement).oneOf()
    verificationFailure(request),
    finishWork,
    deadline,
    all { s.phase == Cleaning or s.phase == Diagnostic, idle },
  }

  action reachFinal =
    if (s.phase == Pending(AfterCopy)) answer(AfterCopy, PostMaterialization) else finishWork
  action bothMismatchStep =
    if (s.phase == Pending(AfterCopy) and s.result == Accepted) answer(AfterCopy, Periodic)
    else if (s.phase == Pending(AfterCopy) or s.phase == Running) reachFinal
    else if (s.phase == Pending(BeforeReport) and s.result == Accepted) answer(BeforeReport, PostMaterialization)
    else answer(BeforeReport, PreFinalReport)
  action deadlineStep =
    if (s.phase == Pending(AfterCopy) or s.phase == Running) reachFinal
    else if (s.phase == Pending(BeforeReport)) deadline
    else if (dispatch == Deadline) answer(BeforeReport, PreFinalReport)
    else answer(Replacement, PreFinalReport)
  action invalidFinalStep =
    if (s.phase == Pending(AfterCopy) or s.phase == Running) reachFinal
    else if (s.phase == Pending(BeforeReport) and s.result == Accepted) answer(BeforeReport, CleanupCheck)
    else if (s.phase == Pending(BeforeReport)) verificationFailure(BeforeReport)
    else answer(BeforeReport, PreFinalReport)

  val Safety = all {
    s.phase == Cleaning implies s.evidence.contains(PreFinalReport),
    s.phase == Running implies s.evidence.contains(PostMaterialization),
    s.retired.intersect(s.completed).size() == 0,
    s.phase == Diagnostic implies s.exit == 2,
  }
}

module IntegrityCheckpointProtocol { import IntegrityCheckpointModel(ACCEPT_WRONG_FINAL=false).* }
module IntegrityCheckpointAcceptsWrongFinal { import IntegrityCheckpointModel(ACCEPT_WRONG_FINAL=true).* }
