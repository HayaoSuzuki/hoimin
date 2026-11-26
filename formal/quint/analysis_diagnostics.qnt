// One final target in an ordinary run, through the first post-analysis request.
module AnalysisDiagnosticsModel {
  const BYPASS_WARNINGS: bool
  type Spool = Absent | Empty | Populated | Truncated
  type Analysis = {spool:Spool, diagnostics:List[int]}
  type Phase = Analyzing | Warning | Reading | Verifying | Cancelled | ReportingFailure
  type Command = Init | Finish(Analysis) | Ack(int) | Cancel | FailOutput | Idle
  type Result = Accepted | Duplicate | Late
  type Snapshot = {phase:Phase, analysis:Analysis, issued:List[int], acknowledged:int,
    exit:int, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {phase:Analyzing, analysis:{spool:Absent, diagnostics:[]}, issued:[],
      acknowledged:0, exit:0, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  pure def handoff(snapshot:Snapshot):Snapshot = snapshot.with("phase",
    if (snapshot.analysis.spool == Populated) Reading else Verifying)

  pure def warn(snapshot:Snapshot):Snapshot =
    snapshot.with("phase", Warning).with("issued",
      snapshot.issued.append(snapshot.analysis.diagnostics.nth(snapshot.acknowledged)))

  action finish(analysis:Analysis):bool = {
    val incomplete = analysis.diagnostics.length() > 0 or analysis.spool == Truncated
    val evaluated = s.with("analysis", analysis).with("exit", if (incomplete) 4 else 0)
    publish(if (analysis.diagnostics.length() == 0) handoff(evaluated)
      else if (BYPASS_WARNINGS) handoff(warn(evaluated))
      else warn(evaluated), Finish(analysis))
  }

  action acknowledge(index:int):bool =
    if (index < s.acknowledged or s.phase == ReportingFailure)
      publish(s.with("result", Duplicate), Ack(index))
    else if (s.phase == Cancelled) publish(s.with("result", Late), Ack(index))
    else {
      val confirmed = s.with("acknowledged", s.acknowledged + 1).with("result", Accepted)
      publish(if (confirmed.acknowledged == s.analysis.diagnostics.length())
        handoff(confirmed) else warn(confirmed), Ack(index))
    }

  action interrupt(command:Command):bool =
    publish(s.with("phase", if (command == Cancel) Cancelled else ReportingFailure)
      .with("exit", if (command == Cancel) 130 else 2).with("result", Accepted), command)

  action idle = publish(s, Idle)
  action replayAck = nondet index = 0.to(s.issued.length()-1).oneOf()
    acknowledge(index)

  action step = if (s.phase == Analyzing) {
    nondet spool = Set(Absent, Empty, Populated, Truncated).oneOf()
    nondet diagnostics = Set([], [1], [1,2], [1,1]).oneOf()
    finish({spool:spool, diagnostics:diagnostics})
  } else if (s.phase == Warning) any {
    replayAck,
    interrupt(Cancel),
    interrupt(FailOutput),
  } else if (s.issued.length() > 0) replayAck else idle

  action drain(spool:Spool, diagnostics:List[int]):bool =
    if (s.phase == Analyzing) finish({spool:spool, diagnostics:diagnostics})
    else if (s.phase == Warning) acknowledge(s.acknowledged)
    else if (s.issued.length() > 0) acknowledge(0) else idle

  action fifoStep =
    if (s.phase == Warning and s.acknowledged == 1 and s.result == Accepted) acknowledge(0)
    else drain(Populated, [1,2])
  action repeatedStep = drain(Empty, [1,1])
  action truncatedStep = drain(Truncated, [2])
  action noSpoolStep = drain(Absent, [])
  action cancelStep =
    if (s.phase == Analyzing) finish({spool:Populated, diagnostics:[1,2]})
    else if (s.phase == Warning and s.acknowledged == 0) acknowledge(0)
    else if (s.phase == Warning) interrupt(Cancel)
    else acknowledge(1)
  action failureStep =
    if (s.phase == Analyzing) finish({spool:Populated, diagnostics:[1,2]})
    else if (s.phase == Warning) interrupt(FailOutput)
    else acknowledge(0)

  val Safety = all {
    s.acknowledged <= s.issued.length(),
    s.issued == s.analysis.diagnostics.slice(0, s.issued.length()),
    s.issued.length() <= s.analysis.diagnostics.length(),
    s.phase == Warning implies s.issued.length() == s.acknowledged + 1,
    (s.phase == Reading or s.phase == Verifying) implies
      s.acknowledged == s.analysis.diagnostics.length(),
    s.phase == Reading implies s.analysis.spool == Populated,
  }
}

module AnalysisDiagnostics { import AnalysisDiagnosticsModel(BYPASS_WARNINGS=false).* }
module AnalysisDiagnosticsBypassesOutput { import AnalysisDiagnosticsModel(BYPASS_WARNINGS=true).* }
