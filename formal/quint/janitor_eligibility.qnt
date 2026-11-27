// One active root with a well-formed lease marker, evaluated per observation.
// The root lease and marker contents stand for stable evidence during a scan.
module JanitorEligibility {
  type Lease = Locked | Available
  type Retention = NoRetain | Retained | RetainReadError
  type Ready = NoReady | ReadyMatches | ReadyMismatch | ReadyIdentityError
  type Heartbeat = BeforeStale | AtStale | AfterStale | HeartbeatMismatch
  type Observation = { lease: Lease, retention: Retention, ready: Ready, heartbeat: Heartbeat }
  type Reason = ActiveLease | RetainMarker | CandidateInspectionFailed |
    MarkerInvalid | IdentityInspectionFailed | HeartbeatInvalid | Young
  type Outcome = Reclaimed | Held(Reason)
  type Report = { outcome: Outcome, details: List[Reason] }
  type Evaluation = Pending | Evaluated({ observation: Observation, report: Report })
  type State = { checked: int, last: Evaluation }

  pure val maxChecks = 4
  var s: State

  action init = s' = { checked: 0, last: Pending }

  pure def held(reason: Reason, earlier: List[Reason]): Report =
    { outcome: Held(reason), details: earlier.append(reason) }

  pure def readyDiagnostics(ready: Ready): List[Reason] = match ready {
    | ReadyMismatch => [MarkerInvalid]
    | ReadyIdentityError => [IdentityInspectionFailed]
    | _ => []
  }

  pure def byHeartbeat(heartbeat: Heartbeat, earlier: List[Reason]): Report = match heartbeat {
    | BeforeStale => held(Young, earlier)
    | HeartbeatMismatch => held(HeartbeatInvalid, earlier)
    | _ => { outcome: Reclaimed, details: earlier }
  }

  pure def decide(input: Observation, bypassRetention: bool): Report =
    if (input.lease == Locked) held(ActiveLease, [])
    else if (input.retention == Retained and not(bypassRetention)) held(RetainMarker, [])
    else if (input.retention == RetainReadError) held(CandidateInspectionFailed, [])
    else if (input.ready == ReadyMatches) { outcome: Reclaimed, details: [] }
    else byHeartbeat(input.heartbeat, readyDiagnostics(input.ready))

  action evaluate(input: Observation, bypassRetention: bool): bool = all {
    s.checked < maxChecks,
    s' = { checked: s.checked + 1,
      last: Evaluated({ observation: input, report: decide(input, bypassRetention) }) },
  }

  action step = {
    nondet lease = Set(Locked, Available).oneOf()
    nondet retention = Set(NoRetain, Retained, RetainReadError).oneOf()
    nondet ready = Set(NoReady, ReadyMatches, ReadyMismatch, ReadyIdentityError).oneOf()
    nondet heartbeat = Set(BeforeStale, AtStale, AfterStale, HeartbeatMismatch).oneOf()
    evaluate({ lease: lease, retention: retention, ready: ready, heartbeat: heartbeat }, false)
  }

  pure val retainedReady = {
    lease: Available, retention: Retained, ready: ReadyMatches, heartbeat: AfterStale
  }

  action priorityStep =
    if (s.checked == 0) evaluate({ ...retainedReady, lease: Locked }, false)
    else if (s.checked == 1) evaluate(retainedReady, false)
    else if (s.checked == 2) evaluate({ ...retainedReady, retention: RetainReadError }, false)
    else evaluate({ ...retainedReady, retention: NoRetain, heartbeat: HeartbeatMismatch }, false)

  pure val invalidReady = {
    lease: Available, retention: NoRetain, ready: ReadyMismatch, heartbeat: BeforeStale
  }

  action fallbackStep =
    if (s.checked == 0) evaluate(invalidReady, false)
    else if (s.checked == 1) evaluate({ ...invalidReady, heartbeat: AtStale }, false)
    else if (s.checked == 2)
      evaluate({ ...invalidReady, ready: ReadyIdentityError, heartbeat: HeartbeatMismatch }, false)
    else evaluate({ ...invalidReady, ready: NoReady, heartbeat: AfterStale }, false)

  action retentionIgnoredStep = evaluate(retainedReady, true)

  pure def usesHeartbeat(input: Observation): bool =
    input.lease == Available and input.retention == NoRetain and input.ready != ReadyMatches

  pure def safe(input: Observation, report: Report): bool = all {
    input.lease == Locked implies
      report.outcome == Held(ActiveLease) and report.details == [ActiveLease],
    (input.lease == Available and input.retention == Retained) implies
      report.outcome == Held(RetainMarker) and report.details == [RetainMarker],
    (input.lease == Available and input.retention == RetainReadError) implies
      report.outcome == Held(CandidateInspectionFailed) and report.details == [CandidateInspectionFailed],
    (input.lease == Available and input.retention == NoRetain and input.ready == ReadyMatches)
      implies report.outcome == Reclaimed and report.details == [],
    (usesHeartbeat(input) and input.heartbeat == BeforeStale) implies report.outcome == Held(Young),
    (usesHeartbeat(input) and input.heartbeat == HeartbeatMismatch) implies
      report.outcome == Held(HeartbeatInvalid),
    (usesHeartbeat(input) and Set(AtStale, AfterStale).contains(input.heartbeat)) implies
      report.outcome == Reclaimed,
  }

  val Safety = match s.last {
    | Pending => true
    | Evaluated(value) => safe(value.observation, value.report)
  }

  val activeLeaseWins = match s.last {
    | Evaluated(value) => value.report.outcome == Held(ActiveLease) and value.observation.ready == ReadyMatches
    | _ => false
  }
  val retentionWins = match s.last {
    | Evaluated(value) => value.report.outcome == Held(RetainMarker) and value.observation.ready == ReadyMatches
    | _ => false
  }
  val retentionInspectionFailed = match s.last {
    | Evaluated(value) => value.report.outcome == Held(CandidateInspectionFailed)
    | _ => false
  }
  val readyOverridesHeartbeat = match s.last {
    | Evaluated(value) => value.report.outcome == Reclaimed and value.observation.heartbeat == HeartbeatMismatch
    | _ => false
  }
  val admittedAtThreshold = match s.last {
    | Evaluated(value) => usesHeartbeat(value.observation) and value.observation.heartbeat == AtStale
      and value.report.outcome == Reclaimed
    | _ => false
  }
  val invalidReadyThenYoung = match s.last {
    | Evaluated(value) => value.report.details == [MarkerInvalid, Young]
    | _ => false
  }
  val failedIdentityThenInvalidHeartbeat = match s.last {
    | Evaluated(value) => value.report.details == [IdentityInspectionFailed, HeartbeatInvalid]
    | _ => false
  }
}
