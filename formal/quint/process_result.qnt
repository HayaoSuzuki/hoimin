// One session candidate, through result persistence and final verification.
module ProcessResultModel {
  const IGNORE_OUTPUT_TIMEOUT: bool
  type Phase = Running | Persisting | Finishing | Reset | Reading | Verifying
  type Termination = NoTermination | ExitZero | ExitOne | TimedOut | MemoryExhausted
    | ProcessCountExhausted | CancelledProcess
  type Status = Unclassified | Survived | Killed | Timeout | OutOfMemory | ProcessLimit | NotRun | Error
  type Slot = Absent | Pending | Completed | Retired
  type Identity = Correct | WrongRunId | WrongMutantId
  type Result = Accepted | RunMismatch | MutantMismatch | Duplicate | Late
  type Evidence = {termination:Termination, closeTimedOut:bool}
  type Command = Init | Finish(Evidence) | Persist(Identity) | Cancel | Ack | EndOfSpool | Idle
  type Snapshot = {phase:Phase, evidence:Evidence, status:Status, persistence:Slot,
    counted:int, cancelled:bool, exit:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {phase:Running, evidence:{termination:NoTermination, closeTimedOut:false},
      status:Unclassified, persistence:Absent, counted:0, cancelled:false, exit:0, result:Accepted},
    dispatch' = Init,
  }

  pure def exitCode(snapshot:Snapshot):int =
    if (snapshot.cancelled) 130
    else if (snapshot.counted == 0) 0
    else if (snapshot.status == Error) 2
    else if (Set(Timeout, OutOfMemory, ProcessLimit, NotRun).contains(snapshot.status)) 4
    else 0

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following.with("exit", exitCode(following)),
    dispatch' = command,
  }

  pure def classify(evidence:Evidence):Status =
    if (evidence.closeTimedOut and not(IGNORE_OUTPUT_TIMEOUT)) Error
    else match evidence.termination {
      | ExitZero => Survived
      | ExitOne => Killed
      | TimedOut => Timeout
      | MemoryExhausted => OutOfMemory
      | ProcessCountExhausted => ProcessLimit
      | CancelledProcess => NotRun
      | NoTermination => Unclassified
    }

  action finish(termination:Termination, closeTimedOut:bool):bool = all {
    s.phase == Running,
    val evidence = {termination:termination, closeTimedOut:closeTimedOut}
    publish({ ...s, phase:Persisting, evidence:evidence, status:classify(evidence),
      persistence:Pending, result:Accepted }, Finish(evidence)),
  }

  action persist(identity:Identity):bool = all {
    s.persistence != Absent,
    val result = if (s.persistence == Completed) Duplicate
      else if (s.persistence == Retired) Late
      else if (identity == WrongRunId) RunMismatch
      else if (identity == WrongMutantId) MutantMismatch else Accepted
    publish(if (result == Accepted) { ...s, phase:Finishing, persistence:Completed,
      counted:s.counted+1, result:result } else s.with("result", result), Persist(identity)),
  }

  action cancel = all {
    not(s.cancelled) and (s.phase == Persisting or s.phase == Finishing),
    publish({ ...s, phase:Finishing, cancelled:true, counted:1, result:Accepted,
      persistence:if (s.persistence == Pending) Retired else Completed }, Cancel),
  }

  action acknowledge = all {
    s.phase == Finishing or s.phase == Reset,
    val phase = if (s.phase == Finishing and not(s.cancelled)) Reset else Reading
    publish({ ...s, phase:phase, result:Accepted }, Ack),
  }

  action eof = all {
    s.phase == Reading,
    publish({ ...s, phase:Verifying, result:Accepted }, EndOfSpool),
  }

  action idle = publish(s, Idle)
  action step = any {
    nondet termination = Set(ExitZero, ExitOne, TimedOut, MemoryExhausted,
      ProcessCountExhausted, CancelledProcess).oneOf()
    nondet closeTimedOut = Bool.oneOf()
    finish(termination, closeTimedOut),
    nondet identity = Set(Correct, WrongRunId, WrongMutantId).oneOf()
    persist(identity),
    cancel,
    acknowledge,
    eof,
    all { s.phase == Verifying, idle },
  }

  action drain =
    if (s.phase == Finishing or s.phase == Reset) acknowledge
    else if (s.phase == Reading) eof else idle
  action regular(termination:Termination, closeTimedOut:bool):bool =
    if (s.phase == Running) finish(termination, closeTimedOut)
    else if (s.phase == Persisting) persist(Correct) else drain
  action outputTimeoutStep = regular(TimedOut, true)
  action processTimeoutStep = regular(TimedOut, false)
  action memoryStep = regular(MemoryExhausted, false)
  action processLimitStep = regular(ProcessCountExhausted, false)
  action processLimitOutputTimeoutStep = regular(ProcessCountExhausted, true)
  action processCancelledStep = regular(CancelledProcess, false)
  action processCancelledOutputTimeoutStep = regular(CancelledProcess, true)
  action identitiesStep =
    if (s.phase == Running) finish(ExitOne, false)
    else if (s.phase == Persisting and s.result == Accepted) persist(WrongRunId)
    else if (s.phase == Persisting and s.result == RunMismatch) persist(WrongMutantId)
    else if (s.phase == Persisting) persist(Correct)
    else if (dispatch == Persist(Correct) and s.result == Accepted) persist(Correct)
    else drain
  action cancelDuringPersist(termination:Termination, closeTimedOut:bool):bool =
    if (s.phase == Running) finish(termination, closeTimedOut)
    else if (s.phase == Persisting) cancel
    else if (dispatch == Cancel) persist(Correct) else drain
  action cancelPersistStep = cancelDuringPersist(TimedOut, true)
  action processCancelledThenStopStep = cancelDuringPersist(CancelledProcess, false)
  action cancelFinishStep =
    if (s.phase == Running) finish(ExitZero, true)
    else if (s.phase == Persisting) persist(Correct)
    else if (s.phase == Finishing and not(s.cancelled)) cancel
    else if (dispatch == Cancel) persist(Correct) else drain

  val Safety = all {
    s.evidence.closeTimedOut implies s.status == Error,
    (s.evidence.termination == ProcessCountExhausted and not(s.evidence.closeTimedOut))
      implies s.status == ProcessLimit,
    (s.evidence.termination == CancelledProcess and not(s.evidence.closeTimedOut))
      implies s.status == NotRun,
    s.counted <= 1,
    s.persistence == Pending implies s.phase == Persisting and s.counted == 0,
    (s.persistence == Completed or s.persistence == Retired) implies s.counted == 1,
    s.persistence == Retired implies s.cancelled,
    s.cancelled implies s.exit == 130 and s.phase != Reset,
    s.counted == 1 and not(s.cancelled) and s.evidence.closeTimedOut implies s.exit == 2,
    (s.counted == 1 and not(s.cancelled) and Set(ProcessLimit, NotRun).contains(s.status))
      implies s.exit == 4,
  }
}

module ProcessResult { import ProcessResultModel(IGNORE_OUTPUT_TIMEOUT=false).* }
module ProcessResultIgnoresOutputTimeout { import ProcessResultModel(IGNORE_OUTPUT_TIMEOUT=true).* }
