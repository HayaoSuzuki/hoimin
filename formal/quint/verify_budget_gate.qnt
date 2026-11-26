// One worker, 30 selected candidates, and 35 seconds per candidate.
module VerifyBudgetGateModel {
  const WARN_AT_EQUALITY: bool
  type Phase = Observing | Warning | Analyzing | Verifying | ReportingFailure
  type Slot = Absent | Pending | Completed | Retired
  type Command = Init | Observe(int) | Ack | Cancel | Deadline | FailRequest
  type Result = Accepted | Duplicate | Late
  type Snapshot = {phase:Phase, observation:Slot, warning:Slot, remaining:int, exit:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  pure val projected = 30 * 35 * 1000

  action init = all {
    s' = {phase:Observing, observation:Pending, warning:Absent,
      remaining:0, exit:0, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action observe(remaining:int):bool =
    if (s.observation == Completed) publish(s.with("result", Duplicate), Observe(remaining))
    else if (s.observation == Retired) publish(s.with("result", Late), Observe(remaining))
    else {
      val shortfall = remaining < projected or (WARN_AT_EQUALITY and remaining == projected)
      publish({ ...s, phase:if (shortfall) Warning else Analyzing,
        observation:Completed, warning:if (shortfall) Pending else Absent,
        remaining:remaining, result:Accepted }, Observe(remaining))
    }

  action acknowledge = all {
    s.warning != Absent,
    if (s.warning == Pending)
      publish({ ...s, phase:Analyzing, warning:Completed, result:Accepted }, Ack)
    else publish(s.with("result", if (s.warning == Completed) Duplicate else Late), Ack),
  }

  pure def close(slot:Slot, failed:bool):Slot =
    if (slot == Pending) (if (failed) Completed else Retired) else slot

  action stop(command:Command):bool = all {
    s.phase == Observing or s.phase == Warning,
    val failed = command == FailRequest
    val exit = if (failed) 2 else if (command == Cancel) 130 else 4
    publish({ ...s, phase:if (failed) ReportingFailure else Verifying,
      observation:close(s.observation, failed), warning:close(s.warning, failed),
      exit:exit, result:Accepted }, command),
  }

  action step = any {
    nondet remaining = Set(0, projected-1, projected, projected+1).oneOf()
    observe(remaining),
    acknowledge,
    nondet command = Set(Cancel, Deadline, FailRequest).oneOf()
    stop(command),
  }

  action belowStep =
    if (s.phase == Observing) observe(projected-1)
    else if (dispatch == Observe(projected-1) and s.result == Accepted) observe(projected-1)
    else acknowledge
  action equalStep = observe(projected)
  action aboveStep = observe(projected+1)
  action cancelWarningStep =
    if (s.phase == Observing) observe(0) else if (s.phase == Warning) stop(Cancel) else acknowledge
  action deadlineObservationStep =
    if (s.phase == Observing) stop(Deadline) else observe(projected)
  action failWarningStep =
    if (s.phase == Observing) observe(0)
    else if (s.phase == Warning) stop(FailRequest) else acknowledge

  val Safety = all {
    s.phase == Warning implies
      s.warning == Pending and s.observation == Completed and s.remaining < projected,
    s.phase == Analyzing implies s.exit == 0 and s.observation == Completed and
      (if (s.remaining < projected) s.warning == Completed else s.warning == Absent),
    s.exit != 0 implies s.observation != Pending and s.warning != Pending,
  }
}

module VerifyBudgetGate { import VerifyBudgetGateModel(WARN_AT_EQUALITY=false).* }
module VerifyBudgetGateWarnsAtEquality { import VerifyBudgetGateModel(WARN_AT_EQUALITY=true).* }
