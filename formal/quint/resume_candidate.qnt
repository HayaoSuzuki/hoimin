// One worker and one loaded candidate in a resumed session.
// Output-failure paths continue through cleanup and, when deliverable, session finish.
module ResumeCandidateModel {
  const RERUN_REUSED: bool

  type Stage = LookupPending | ApplyPending | StartPending | FinishPending | NextCandidate
    | DiagnosticPending | CleanupPending | ReportPending | SessionPending | Finished
  type Outcome = NoOutcome | CachedKilled | CachedSurvived | NotRun
  type Failure = NoFailure | EffectFailure | DeliveryFailure
  type Command = Init | Reply({ status: int, identityOk: bool }) | Ack | Cancel | Fail(bool) | Late
  type State = {
    stage: Stage, stored: int, outcome: Outcome, count: int,
    cancelled: bool, failure: Failure, lateChecked: bool, result: str,
  }
  var s: State
  var dispatch: Command

  // Stored status: absent 0, killed 1, survived 2, timeout 3, OOM 4,
  // process limit 5, error 6, not run 7, lookup not yet accepted 8.
  pure def reusable(status: int): bool = Set(1, 2).contains(status)
  pure def syntheticFor(status: int): Outcome =
    if (status == 1) CachedKilled else if (status == 2) CachedSurvived else NotRun

  action init = all {
    s' = { stage: LookupPending, stored: 8, outcome: NoOutcome, count: 0,
      cancelled: false, failure: NoFailure, lateChecked: false, result: "init" },
    dispatch' = Init,
  }

  action reply(status: int, identityOk: bool): bool = {
    val reuse = reusable(status) and not(RERUN_REUSED)
    all {
      s.stage == LookupPending, status != 0 or identityOk,
      s' = if (identityOk) { ...s, stored: status, result: "accepted",
        stage: if (reuse) StartPending else ApplyPending,
        outcome: if (reuse) syntheticFor(status) else NoOutcome }
      else { ...s, result: "identity" },
      dispatch' = Reply({ status: status, identityOk: identityOk }),
    }
  }

  pure def acknowledge(current: State): State = match current.stage {
    | StartPending => { ...current, stage: FinishPending }
    | FinishPending => { ...current, count: current.count + 1,
        stage: if (current.failure == EffectFailure) DiagnosticPending else NextCandidate }
    | DiagnosticPending => { ...current, stage: CleanupPending }
    | CleanupPending => { ...current,
        stage: if (current.failure == DeliveryFailure) Finished else ReportPending }
    | ReportPending => { ...current, stage: SessionPending }
    | SessionPending => { ...current, stage: Finished }
    | _ => current
  }

  action ack = all {
    Set(StartPending, FinishPending, DiagnosticPending, CleanupPending, ReportPending, SessionPending).contains(s.stage),
    s' = { ...acknowledge(s), result: "accepted" },
    dispatch' = Ack,
  }

  action cancel = all {
    not(s.cancelled), s.failure == NoFailure, Set(LookupPending, StartPending, FinishPending).contains(s.stage),
    s' = if (s.stage == LookupPending)
        { ...s, cancelled: true, stage: StartPending, outcome: NotRun, result: "accepted" }
      else { ...s, cancelled: true, result: "accepted" },
    dispatch' = Cancel,
  }

  action failOutput(delivery: bool): bool = all {
    not(s.cancelled), s.failure == NoFailure, Set(StartPending, FinishPending).contains(s.stage),
    s' = if (delivery) { ...s, failure: DeliveryFailure, stage: DiagnosticPending, result: "accepted" }
      else { ...s, failure: EffectFailure, result: "accepted" },
    dispatch' = Fail(delivery),
  }

  action late = all {
    s.cancelled or s.failure != NoFailure, not(s.lateChecked),
    s' = { ...s, lateChecked: true, result: if (s.cancelled) "retired" else "duplicate" },
    dispatch' = Late,
  }

  action step = {
    nondet status = 0.to(7).oneOf()
    nondet identityOk = Set(false, true).oneOf()
    nondet delivery = Set(false, true).oneOf()
    any { reply(status, identityOk), ack, cancel, failOutput(delivery), late }
  }

  action reuseStep = if (s.stage == LookupPending) reply(1, true) else ack
  action rerunStep = if (dispatch == Init) reply(1, false) else reply(3, true)
  action cancelledLookupStep =
    if (s.stage == LookupPending) cancel else if (not(s.lateChecked)) late else ack
  action cancelReuseAt(stage: Stage): bool =
    if (s.stage == LookupPending) reply(2, true)
    else if (not(s.cancelled)) { if (s.stage == stage) cancel else ack }
    else if (not(s.lateChecked) and (stage == StartPending or s.stage == NextCandidate)) late else ack
  action cancelledStartStep = cancelReuseAt(StartPending)
  action cancelledFinishStep = cancelReuseAt(FinishPending)

  action failReuseAt(stage: Stage, delivery: bool): bool =
    if (s.stage == LookupPending) reply(2, true)
    else if (s.failure == NoFailure) { if (s.stage == stage) failOutput(delivery) else ack }
    else if (not(s.lateChecked) and (not(delivery) or s.stage == Finished)) late else ack
  action failedStartStep = failReuseAt(StartPending, false)
  action failedFinishStep = failReuseAt(FinishPending, false)
  action reportIoStep = failReuseAt(FinishPending, true)

  val killedCounted = s.stage == NextCandidate and s.outcome == CachedKilled and s.count == 1
  val timeoutRerun = s.stage == ApplyPending and s.stored == 3
  val wrongIdentityRejected = s.result == "identity" and s.stage == LookupPending
  val stoppedLookupCounted = s.stage == NextCandidate and s.outcome == NotRun and s.lateChecked
  val stoppedReuseCounted = s.stage == NextCandidate and s.outcome == CachedSurvived and s.lateChecked
  val genericFailureDrained = s.stage == Finished and s.failure == EffectFailure and s.count == 1 and s.lateChecked
  val reportIoAbandoned = s.stage == Finished and s.failure == DeliveryFailure and s.count == 0 and s.lateChecked
  val failureStages = Set(DiagnosticPending, CleanupPending, ReportPending, SessionPending, Finished)

  val Safety = all {
    s.count == (if (s.stage == NextCandidate or (s.failure == EffectFailure and failureStages.contains(s.stage))) 1 else 0),
    s.stage == LookupPending implies (s.stored == 8 and s.outcome == NoOutcome),
    s.stage == ApplyPending implies (s.stored != 8 and not(reusable(s.stored)) and s.outcome == NoOutcome),
    Set(StartPending, FinishPending, NextCandidate).union(failureStages).contains(s.stage) implies
      ((reusable(s.stored) or (s.cancelled and s.stored == 8)) and s.outcome == syntheticFor(s.stored)),
    s.lateChecked implies (s.cancelled or s.failure != NoFailure),
    failureStages.contains(s.stage) implies s.failure != NoFailure,
    s.failure == DeliveryFailure implies Set(DiagnosticPending, CleanupPending, Finished).contains(s.stage),
  }
}
module ResumeCandidate { import ResumeCandidateModel(RERUN_REUSED = false).* }
module ResumeCandidateRerunsCached { import ResumeCandidateModel(RERUN_REUSED = true).* }
