// Preflight through admission of one workspace worker, with session I/O as effects.
module SessionStartupModel {
  const REUSE_SKIPS_REPORT_ACK: bool
  type Mode = Disabled | FreshMode | ResumeMode
  type Phase = Ready | Preflight | Loading | Beginning | Reporting | Copying | Verifying | Diagnostic
  type Reason = NoPriorRun | MatchingRunComplete | BudgetDecreased | FingerprintMismatch
    | NoIncompleteRun | CandidateChanged
  type Selection = Reuse | FallBack(Reason)
  type Provenance = NoProvenance | Resumed | Fresh(Reason)
  type ExitCause = Healthy | DeadlineStop | IoFailure
  type Command = Init | Configure(Mode) | Measure(int) | Load(Selection) | Begin
    | AckReport | Deadline | Fail | Probe | Idle
  type Result = Accepted | MissingFingerprint | Late | Duplicate
  type Snapshot = {phase:Phase, mode:Mode, fingerprint:int, runNumber:int, provenance:Provenance,
    cause:ExitCause, acknowledged:bool, result:Result}
  var s:Snapshot
  var dispatch:Command

  action init = all {
    s' = {phase:Ready, mode:Disabled, fingerprint:0, runNumber:1, provenance:NoProvenance,
      cause:Healthy, acknowledged:false, result:Accepted},
    dispatch' = Init,
  }

  action publish(following:Snapshot, command:Command):bool = all {
    s' = following,
    dispatch' = command,
  }

  action configure(mode:Mode):bool = all {
    s.phase == Ready,
    publish({ ...s, phase:Preflight, mode:mode }, Configure(mode)),
  }

  action measure(fingerprint:int):bool = all {
    s.phase == Preflight,
    if (s.mode != Disabled and fingerprint == 0)
      publish(s.with("result", MissingFingerprint), Measure(fingerprint))
    else {
      val phase = match s.mode {
        | Disabled => Reporting
        | FreshMode => Beginning
        | ResumeMode => Loading
      }
      publish({ ...s, phase:phase, fingerprint:fingerprint, result:Accepted }, Measure(fingerprint))
    },
  }

  action load(selection:Selection):bool = all {
    s.phase == Loading,
    val following = match selection {
      | Reuse => { ...s, phase:if (REUSE_SKIPS_REPORT_ACK) Copying else Reporting,
        runNumber:2, provenance:Resumed, result:Accepted }
      | FallBack(reason) => { ...s, phase:Beginning, provenance:Fresh(reason), result:Accepted }
    }
    publish(following, Load(selection)),
  }

  action begin = all {
    s.phase == Beginning,
    publish({ ...s, phase:Reporting, result:Accepted }, Begin),
  }

  action acknowledge = all {
    s.phase == Reporting,
    val phase = match s.cause {
      | Healthy => Copying
      | DeadlineStop => Verifying
      | IoFailure => Diagnostic
    }
    publish({ ...s, phase:phase, acknowledged:true, result:Accepted }, AckReport),
  }

  action interrupt(command:Command):bool = all {
    s.phase == Loading or s.phase == Beginning,
    publish({ ...s, phase:Reporting, cause:if (command == Deadline) DeadlineStop else IoFailure,
      result:Accepted }, command),
  }

  action probe = all {
    s.cause != Healthy,
    publish(s.with("result", if (s.cause == DeadlineStop) Late else Duplicate), Probe),
  }

  action idle = publish(s, Idle)
  action step = any {
    nondet mode = Set(Disabled, FreshMode, ResumeMode).oneOf()
    configure(mode),
    nondet fingerprint = Set(0, 1, 2).oneOf()
    measure(fingerprint),
    nondet reason = Set(NoPriorRun, MatchingRunComplete, BudgetDecreased, FingerprintMismatch,
      NoIncompleteRun, CandidateChanged).oneOf()
    load(FallBack(reason)),
    load(Reuse),
    begin,
    acknowledge,
    interrupt(Deadline),
    interrupt(Fail),
    probe,
    all { Set(Copying, Verifying, Diagnostic).contains(s.phase), idle },
  }

  action regular(mode:Mode, selection:Selection):bool = match s.phase {
    | Ready => configure(mode)
    | Preflight => measure(if (mode == Disabled) 0 else 2)
    | Loading => load(selection)
    | Beginning => begin
    | Reporting => acknowledge
    | _ => idle
  }
  action disabledStep = regular(Disabled, Reuse)
  action resumedStep = regular(ResumeMode, Reuse)
  action fallbackStep = regular(ResumeMode, FallBack(CandidateChanged))
  action missingFingerprintStep =
    if (s.phase == Preflight and s.result == Accepted) measure(0)
    else regular(FreshMode, Reuse)
  action deadlineLoadStep =
    if (s.phase == Loading) interrupt(Deadline)
    else if (s.phase == Verifying) probe
    else regular(ResumeMode, Reuse)
  action failedBeginStep =
    if (s.phase == Beginning) interrupt(Fail)
    else if (s.phase == Diagnostic) probe
    else regular(ResumeMode, FallBack(BudgetDecreased))

  val Safety = all {
    s.phase == Copying implies s.acknowledged and s.cause == Healthy,
    (s.phase == Loading or s.phase == Beginning) implies s.fingerprint != 0,
    (s.runNumber == 2) == (s.provenance == Resumed),
    s.mode != ResumeMode implies s.provenance == NoProvenance,
    s.cause != Healthy implies s.runNumber == 1,
  }
}

module SessionStartup { import SessionStartupModel(REUSE_SKIPS_REPORT_ACK=false).* }
module SessionStartupSkipsReportAck { import SessionStartupModel(REUSE_SKIPS_REPORT_ACK=true).* }
