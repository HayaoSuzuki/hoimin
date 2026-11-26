// One active candidate in an ordinary run; at most one effect failure.
module WorkerFailureModel {
  const DROP_EXECUTED_RESULT: bool
  type Phase = Apply | Start | Running | Finish | Reset | NextRead
    | ReplacementStart | ReplacementFinish | Diagnostic | Cleanup | Report | Done
  type Failure = NoFailure | EffectFailure | DeliveryFailure
  type Outcome = Unclassified | Killed | NotRun
  type Result = Accepted | Duplicate
  type Command = Init | Ack | Fail(Failure) | RetryFailed | Idle
  type Snapshot = {phase:Phase, failure:Failure, outcome:Outcome, killed:int,
    notRun:int, started:bool, report:bool, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {phase:Apply, failure:NoFailure, outcome:Unclassified, killed:0, notRun:0,
      started:false, report:false, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  pure def nextPhase(phase:Phase, failure:Failure):Phase = match phase {
    | Apply => Start
    | Start => Running
    | Running => Finish
    | Finish => Reset
    | Reset => NextRead
    | ReplacementStart => ReplacementFinish
    | ReplacementFinish => Diagnostic
    | Diagnostic => Cleanup
    | Cleanup => if (failure == DeliveryFailure) Done else Report
    | _ => Done
  }

  action acknowledge = all {
    s.phase != Done and s.phase != NextRead,
    val phase = nextPhase(s.phase, s.failure)
    val executed = s.phase == Running
    val notRun = s.phase == ReplacementFinish and s.outcome == NotRun
    publish({ ...s, phase:phase, started:s.started or s.phase == Start or s.phase == ReplacementStart,
      outcome:if (executed) Killed else s.outcome,
      killed:s.killed + (if (executed) 1 else 0), notRun:s.notRun + (if (notRun) 1 else 0),
      report:s.report or phase == Report, result:Accepted }, Ack),
  }

  action inject(failure:Failure):bool = all {
    s.failure == NoFailure,
    Set(Apply, Start, Running, Finish).contains(s.phase),
    failure != DeliveryFailure or s.phase == Start or s.phase == Finish,
    val phase = if (failure == DeliveryFailure) Diagnostic
      else if (s.started) ReplacementFinish else ReplacementStart
    val outcome = if (failure == DeliveryFailure) s.outcome
      else if (s.phase == Finish and not(DROP_EXECUTED_RESULT)) Killed else NotRun
    publish({ ...s, phase:phase, failure:failure, outcome:outcome, result:Accepted }, Fail(failure)),
  }

  action retry = all {
    s.failure != NoFailure,
    publish(s.with("result", Duplicate), RetryFailed),
  }

  action idle = publish(s, Idle)
  action step = any {
    acknowledge,
    inject(EffectFailure),
    inject(DeliveryFailure),
    retry,
    all { s.phase == Done or s.phase == NextRead, idle },
  }

  action failAt(target:Phase, failure:Failure):bool =
    if (s.phase == target and s.failure == NoFailure) inject(failure)
    else if (dispatch == Fail(failure)) retry
    else if (s.phase == Done) idle else acknowledge
  action applyFailureStep = failAt(Apply, EffectFailure)
  action startFailureStep = failAt(Start, EffectFailure)
  action runFailureStep = failAt(Running, EffectFailure)
  action finishFailureStep = failAt(Finish, EffectFailure)
  action reportStartFailureStep = failAt(Start, DeliveryFailure)
  action reportFinishFailureStep = failAt(Finish, DeliveryFailure)

  val Safety = all {
    s.killed + s.notRun <= 1,
    s.killed == 1 implies s.outcome == Killed,
    s.phase == ReplacementFinish implies s.started,
    s.phase == ReplacementStart implies not(s.started),
    s.failure != NoFailure implies s.phase != Running and s.phase != NextRead,
    s.failure == DeliveryFailure implies
      not(s.report) and not(Set(ReplacementStart, ReplacementFinish).contains(s.phase)),
    s.phase == Done and s.failure == EffectFailure implies s.killed + s.notRun == 1,
  }
}

module WorkerFailure { import WorkerFailureModel(DROP_EXECUTED_RESULT=false).* }
module WorkerFailureDropsResult { import WorkerFailureModel(DROP_EXECUTED_RESULT=true).* }
