// One shell invocation with up to two selected roots. The selector and each
// cleanup slice supply observations, as checked by the separate janitor models.
// Local janitor limits can yield a report while the shell still has time to
// publish. The shell's preparation stop is a separate transition.
module JanitorStartup {
  type PreparationStop = Cancelled | TotalDeadline
  type Phase = BeforeStartup | Selecting | Cleaning | Retrying | Finishing | Published |
    Stopped(PreparationStop)
  type CursorWrite = Unattempted | Committed | WriteReportedFailure
  type Reason = Young | ActiveLease | RetainMarker | CoordinatorBusy | MutationBarrierActive |
    InvalidCandidate | CandidateInspectionFailed | IdentityInspectionFailed | LeaseInvalid |
    LeaseLockFailed | MarkerInvalid | HeartbeatInvalid | AgeUnverifiable | UnsafeStaging |
    CoordinatorFailed | MutationBarrierFailed | ClaimFailed | ClaimedLeaseUnavailable |
    DeleteFailed | CleanupIncomplete | SelectionFailed | SelectionDeadline |
    SelectionChildLimit | CursorPersistenceFailed
  type ScanStop = NoScanStop | ScanStopped(Reason)
  type Scan = {
    complete: bool, observedMatching: int, selected: List[int],
    cursorWrite: CursorWrite, stop: ScanStop, cursorDeferred: bool,
  }
  type Outcome = Uninspected | Pending | Absent | Preserved(Reason) | Unresolved(Reason) | Reclaimed
  type Candidate = { outcome: Outcome, examined: int, removed: int }
  type Summary = {
    inspected: int, absent: int, reclaimed: int, preserved: int, unresolved: int,
    examined: int, removed: int, uninspected: int, omitted: int,
  }
  type Report = { scan: Scan, summary: Summary, diagnostics: List[Reason] }
  type Handoff = NoReport | Ready(Report) | Delivered(Report)
  type Startup = {
    phase: Phase, scan: Scan, candidates: int -> Candidate, pending: List[int],
    diagnostics: List[Reason], handoff: Handoff, roundRemaining: int, roundProgress: bool,
  }

  pure val roots = Set(1, 2)
  pure val emptyCandidate = { outcome: Uninspected, examined: 0, removed: 0 }
  pure val emptyScan = { complete: false, observedMatching: 0, selected: [],
    cursorWrite: Unattempted, stop: NoScanStop, cursorDeferred: false }
  var s: Startup

  action init = s' = {
    phase: BeforeStartup, scan: emptyScan, candidates: roots.mapBy(_ => emptyCandidate),
    pending: [], diagnostics: [], handoff: NoReport, roundRemaining: 0, roundProgress: false,
  }

  pure def unseen(state: Startup): List[int] =
    state.scan.selected.select(id => state.candidates.get(id).outcome == Uninspected)

  pure def count(state: Startup, predicate: Outcome => bool): int =
    state.scan.selected.select(id => predicate(state.candidates.get(id).outcome)).length()

  pure def report(state: Startup): Report = {
    val missing = unseen(state).length()
    val selected = state.scan.selected
    {
      scan: state.scan, diagnostics: state.diagnostics,
      summary: {
        inspected: selected.length() - missing, uninspected: missing,
        absent: count(state, outcome => outcome == Absent),
        reclaimed: count(state, outcome => outcome == Reclaimed),
        preserved: count(state, outcome => match outcome { | Preserved(_) => true | _ => false }),
        unresolved: count(state, outcome => match outcome { | Unresolved(_) => true | _ => false }),
        examined: selected.foldl(0, (total, id) => total + state.candidates.get(id).examined),
        removed: selected.foldl(0, (total, id) => total + state.candidates.get(id).removed),
        omitted: state.scan.observedMatching - selected.length(),
      },
    }
  }

  action begin = all { s.phase == BeforeStartup, s' = { ...s, phase: Selecting } }

  action selectPage(observed: int, selected: List[int], cursor: CursorWrite): bool = all {
    s.phase == Selecting,
    s' = { ...s, phase: Cleaning,
      scan: { complete: true, observedMatching: observed, selected: selected, cursorWrite: cursor,
        stop: NoScanStop, cursorDeferred: selected.length() < observed },
      diagnostics: if (cursor == WriteReportedFailure) [CursorPersistenceFailed] else [] },
  }

  action abortScan(reason: Reason, observed: int): bool = all {
    s.phase == Selecting,
    s' = { ...s, phase: Cleaning, scan: { ...emptyScan, observedMatching: observed,
      stop: ScanStopped(reason) },
      diagnostics: if (reason == CoordinatorBusy) [] else [reason] },
  }

  pure def withOutcome(state: Startup, id: int, evidence: Candidate): Startup = {
    val rest = state.pending.select(root => root != id)
    val details = match evidence.outcome {
      | Preserved(reason) => state.diagnostics.append(reason)
      | Unresolved(reason) => state.diagnostics.append(reason)
      | _ => state.diagnostics
    }
    { ...state, candidates: state.candidates.set(id, evidence), diagnostics: details,
      pending: if (evidence.outcome == Pending) rest.append(id) else rest }
  }

  action inspect(evidence: Candidate): bool = all {
    s.phase == Cleaning, s.scan.stop == NoScanStop, unseen(s).length() > 0,
    s' = withOutcome(s, unseen(s).head(), evidence),
  }

  action startRound = all {
    s.phase == Cleaning, unseen(s) == [], s.pending.length() > 0,
    s' = { ...s, phase: Retrying, roundRemaining: s.pending.length(), roundProgress: false },
  }

  action retry(evidence: Candidate): bool = all {
    s.phase == Retrying, s.roundRemaining > 0, s.pending.length() > 0,
    val id = s.pending.head()
    val previous = s.candidates.get(id)
    all {
      evidence.examined >= previous.examined and evidence.removed >= previous.removed,
      s' = { ...withOutcome(s, id, evidence), roundRemaining: s.roundRemaining - 1,
        roundProgress: s.roundProgress or
          (evidence.outcome == Pending and evidence.removed > previous.removed) },
    },
  }

  action nextRound = all {
    s.phase == Retrying, s.roundRemaining == 0, s.roundProgress and s.pending.length() > 0,
    s' = { ...s, roundRemaining: s.pending.length(), roundProgress: false },
  }

  pure def finishState(state: Startup): Startup = {
    val settled = state.pending.foldl(state, (work, id) =>
      withOutcome(work, id, { ...work.candidates.get(id), outcome: Unresolved(CleanupIncomplete) }))
    { ...settled, phase: Finishing, handoff: Ready(report(settled)),
      roundRemaining: 0, roundProgress: false }
  }

  action finish = all {
    (s.phase == Cleaning and unseen(s) == [] and s.pending == []) or
      (s.phase == Retrying and s.roundRemaining == 0 and (s.pending == [] or not(s.roundProgress))),
    s' = finishState(s),
  }

  action expireCleanup = all {
    Set(Cleaning, Retrying).contains(s.phase), s.scan.stop == NoScanStop,
    unseen(s).length() > 0 or s.pending.length() > 0,
    // The scan stop describes an unvisited selected root. An expired retry
    // without unvisited roots only contributes the deferred-root details.
    val scan = if (unseen(s).length() > 0) { ...s.scan, stop: ScanStopped(CleanupIncomplete) } else s.scan
    s' = finishState({ ...s, scan: scan }),
  }

  pure def deliver(handoff: Handoff): Handoff = match handoff {
    | Ready(value) => Delivered(value)
    | _ => handoff
  }

  action publish = all {
    s.phase == Finishing,
    s' = { ...s, phase: Published, handoff: deliver(s.handoff) },
  }

  action stop(cause: PreparationStop): bool = all {
    Set(BeforeStartup, Finishing).contains(s.phase),
    s' = { ...s, phase: Stopped(cause) },
  }

  pure val removedRoot = { outcome: Reclaimed, examined: 0, removed: 1 }
  pure val youngRoot = { outcome: Preserved(Young), examined: 0, removed: 0 }
  pure val absentRoot = { outcome: Absent, examined: 0, removed: 0 }
  pure val failedSlice = { outcome: Unresolved(DeleteFailed), examined: 4, removed: 1 }
  pure val initialSlice = { outcome: Pending, examined: 3, removed: 1 }
  pure val stalledSlice = { outcome: Pending, examined: 3, removed: 0 }
  pure val finishedRetry = { outcome: Reclaimed, examined: 5, removed: 4 }
  pure val progressingRetry = { outcome: Pending, examined: 4, removed: 2 }
  pure val stalledRetry = { outcome: Pending, examined: 6, removed: 0 }

  action step = any {
    begin,
    selectPage(0, [], Unattempted),
    selectPage(3, [1, 2], Committed),
    selectPage(3, [1, 2], WriteReportedFailure),
    abortScan(SelectionFailed, 3),
    abortScan(SelectionDeadline, 0),
    abortScan(CoordinatorBusy, 0),
    inspect(removedRoot), inspect(youngRoot), inspect(absentRoot), inspect(failedSlice),
    inspect(initialSlice), inspect(stalledSlice),
    startRound, retry(finishedRetry), retry(progressingRetry), retry(stalledRetry),
    retry(failedSlice), nextRound,
    expireCleanup, finish, publish, stop(Cancelled), stop(TotalDeadline),
  }

  action ordinary = any {
    begin, selectPage(3, [1, 2], Committed),
    if (unseen(s).length() == 2) inspect(removedRoot) else inspect(youngRoot),
    finish,
  }

  action publishedStep = any { ordinary, publish }
  action cancelledBeforeStep = stop(Cancelled)
  action finishThenStop(cause: PreparationStop): bool = any {
    ordinary,
    all { s.phase == Finishing, stop(cause) },
  }
  action cancelledAfterStep = finishThenStop(Cancelled)
  action timedOutAfterStep = finishThenStop(TotalDeadline)

  action selectionFailureStep = any { begin, abortScan(SelectionFailed, 3), finish, publish }
  action cursorFailureStep = any {
    begin, selectPage(3, [1, 2], WriteReportedFailure), inspect(removedRoot), finish, publish,
  }
  action cleanupDeadlineStep = any {
    begin, selectPage(3, [1, 2], Committed),
    all { unseen(s).length() == 2, inspect(removedRoot) },
    all { unseen(s).length() == 1, expireCleanup },
    publish,
  }
  action deferredStep = any {
    begin, selectPage(3, [1, 2], Committed),
    if (unseen(s).length() == 2) inspect(initialSlice) else inspect(stalledSlice),
    startRound,
    if (s.roundRemaining == 2) retry(finishedRetry) else retry(stalledRetry),
    finish, publish,
  }

  // A faulty shell can publish an empty report after a preparation stop.
  action publishAfterStopStep =
    if (s.phase == BeforeStartup) stop(Cancelled)
    else s' = { ...s, handoff: Delivered(report(s)) }

  pure def coherent(value: Report): bool = all {
    value.summary.inspected + value.summary.uninspected == value.scan.selected.length(),
    value.summary.inspected ==
      value.summary.reclaimed + value.summary.preserved + value.summary.unresolved + value.summary.absent,
    value.summary.omitted + value.scan.selected.length() == value.scan.observedMatching,
    value.summary.removed >= value.summary.reclaimed,
  }

  val Safety = all {
    s.scan.selected.length() <= s.scan.observedMatching,
    not(s.scan.complete) implies s.scan.selected == [],
    (s.scan.cursorWrite == WriteReportedFailure) implies
      s.diagnostics.select(reason => reason == CursorPersistenceFailed).length() == 1,
    roots.forall(id =>
      s.scan.selected.select(root => root == id).length() <= 1 and
      (s.scan.selected.select(root => root == id).length() == 0 implies
        s.candidates.get(id) == emptyCandidate)),
    roots.forall(id => {
      val candidate = s.candidates.get(id)
      val queued = s.pending.select(root => root == id).length()
      queued <= 1 and (candidate.outcome == Pending) == (queued == 1) and
      (candidate.outcome == Uninspected implies candidate == emptyCandidate)
    }),
    s.roundRemaining >= 0 and s.roundRemaining <= s.pending.length(),
    match s.handoff {
      | NoReport => s.phase != Published and s.phase != Finishing
      | Ready(value) => s.phase != Published and s.pending == [] and
        value == report(s) and coherent(value)
      | Delivered(value) => s.phase == Published and s.pending == [] and
        value == report(s) and coherent(value)
    },
  }
}
