// One worker and one loaded candidate in a resumed session.
// The scope ends at ApplyMutation or after the synthetic finish is acknowledged.
module ResumeCandidateModel {
  const RERUN_REUSED: bool

  type Stage = LookupPending | ApplyPending | StartPending | FinishPending | NextCandidate
  type Outcome = NoOutcome | CachedKilled | CachedSurvived | NotRun
  type Command = Init | Reply({ status: int, identityOk: bool }) | Ack | Cancel | Late
  type State = {
    stage: Stage, stored: int, outcome: Outcome, count: int,
    cancelled: bool, lateChecked: bool, result: str,
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
      cancelled: false, lateChecked: false, result: "init" },
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

  action ack = all {
    Set(StartPending, FinishPending).contains(s.stage),
    s' = if (s.stage == StartPending) { ...s, stage: FinishPending, result: "accepted" }
      else { ...s, stage: NextCandidate, count: s.count + 1, result: "accepted" },
    dispatch' = Ack,
  }

  action cancel = all {
    not(s.cancelled), Set(LookupPending, StartPending, FinishPending).contains(s.stage),
    s' = if (s.stage == LookupPending)
        { ...s, cancelled: true, stage: StartPending, outcome: NotRun, result: "accepted" }
      else { ...s, cancelled: true, result: "accepted" },
    dispatch' = Cancel,
  }

  action late = all {
    s.cancelled, not(s.lateChecked),
    s' = { ...s, lateChecked: true, result: "retired" },
    dispatch' = Late,
  }

  action step = {
    nondet status = 0.to(7).oneOf()
    nondet identityOk = Set(false, true).oneOf()
    any { reply(status, identityOk), ack, cancel, late }
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

  val killedCounted = s.stage == NextCandidate and s.outcome == CachedKilled and s.count == 1
  val timeoutRerun = s.stage == ApplyPending and s.stored == 3
  val wrongIdentityRejected = s.result == "identity" and s.stage == LookupPending
  val stoppedLookupCounted = s.stage == NextCandidate and s.outcome == NotRun and s.lateChecked
  val stoppedReuseCounted = s.stage == NextCandidate and s.outcome == CachedSurvived and s.lateChecked

  val Safety = all {
    s.count == (if (s.stage == NextCandidate) 1 else 0),
    s.stage == LookupPending implies (s.stored == 8 and s.outcome == NoOutcome),
    s.stage == ApplyPending implies (s.stored != 8 and not(reusable(s.stored)) and s.outcome == NoOutcome),
    Set(StartPending, FinishPending, NextCandidate).contains(s.stage) implies
      ((reusable(s.stored) or (s.cancelled and s.stored == 8)) and s.outcome == syntheticFor(s.stored)),
    s.lateChecked implies s.cancelled,
  }
}
module ResumeCandidate { import ResumeCandidateModel(RERUN_REUSED = false).* }
module ResumeCandidateRerunsCached { import ResumeCandidateModel(RERUN_REUSED = true).* }
