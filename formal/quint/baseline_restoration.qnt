// A successful baseline issues one output and one reset independently.
module BaselineRestorationModel {
  const RELEASE_ON_OUTPUT: bool
  type Slot = Pending | Completed | Retired
  type Phase = Restoring | Analyzing | Verifying | ReportingFailure
  type Command = Init | Reset | Output | WrongWorker | WrongKind
    | Cancel | FailReset | FailOutput
  type Result = Accepted | Duplicate | Late | WorkerMismatch | KindMismatch
  type Snapshot = {reset:Slot, output:Slot, phase:Phase, analyses:int, exit:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {reset:Pending, output:Pending, phase:Restoring, analyses:0, exit:0, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action reject(result:Result, command:Command):bool =
    publish(s.with("result", result), command)

  action restore(command:Command):bool = match s.reset {
    | Completed => reject(Duplicate, command)
    | Retired => reject(Late, command)
    | Pending =>
      if (command == WrongWorker) reject(WorkerMismatch, command)
      else if (command == WrongKind) reject(KindMismatch, command)
      else publish(s.with("reset", Completed).with("phase", Analyzing)
        .with("analyses", s.analyses + 1).with("result", Accepted), command)
  }

  action deliver = match s.output {
    | Completed => reject(Duplicate, Output)
    | Retired => reject(Late, Output)
    | Pending =>
      val acknowledged = s.with("output", Completed).with("result", Accepted)
      publish(if (RELEASE_ON_OUTPUT and s.phase == Restoring)
        acknowledged.with("phase", Analyzing).with("analyses", s.analyses + 1)
        else acknowledged, Output)
  }

  pure def retire(slot:Slot):Slot = if (slot == Pending) Retired else slot

  action stop(command:Command):bool = all {
    s.phase == Restoring,
    command != FailOutput or s.output == Pending,
    val reset = if (command == FailReset) Completed else retire(s.reset)
    val output = if (command == FailOutput) Completed else retire(s.output)
    val phase = if (command == Cancel) Verifying else ReportingFailure
    val exit = if (command == Cancel) 130 else 2
    publish({reset:reset, output:output, phase:phase, analyses:s.analyses, exit:exit,
      result:Accepted}, command),
  }

  action step = any {
    nondet command = Set(Reset, WrongWorker, WrongKind).oneOf()
    restore(command),
    deliver,
    nondet command = Set(Cancel, FailReset, FailOutput).oneOf()
    stop(command),
  }

  action outputFirstStep =
    if (s.output == Pending) deliver else restore(Reset)
  action resetFirstStep =
    if (s.reset == Pending) restore(Reset) else deliver
  action rejectionsStep = match dispatch {
    | Init => restore(WrongKind)
    | WrongKind => restore(WrongWorker)
    | WrongWorker => restore(Reset)
    | _ => restore(Reset)
  }
  action failureStep =
    if (s.phase == Restoring) stop(FailReset)
    else if (dispatch == Output) restore(Reset) else deliver
  action stopStep =
    if (s.phase == Restoring) stop(Cancel)
    else if (dispatch == Output) restore(Reset) else deliver

  val Safety = all {
    s.analyses == (if (s.phase == Analyzing) 1 else 0),
    s.analyses > 0 implies s.reset == Completed and s.exit == 0,
    s.phase == Restoring implies s.reset == Pending,
    s.exit != 0 implies s.reset != Pending and s.output != Pending,
  }
}

module BaselineRestoration { import BaselineRestorationModel(RELEASE_ON_OUTPUT=false).* }
module BaselineRestorationUsesOutput { import BaselineRestorationModel(RELEASE_ON_OUTPUT=true).* }
