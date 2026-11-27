// A stable staging or deleting directory encountered after an owner stopped.
// The coordinator and deletion are available when the evidence admits a root.
module JanitorNamespaceRecovery {
  type Namespace = Staging | Deleting
  type Layout = Empty | UnmarkedPayload | LeaseOnly | LeaseWithHeartbeat |
    LeaseWithPayload | MarkersWithPayload
  type Age = BeforeThreshold | AtThreshold | AgeReadError
  type Observation = { namespace: Namespace, layout: Layout, age: Age }
  type Reason = Young | LeaseInvalid | UnsafeStaging | HeartbeatInvalid | AgeUnverifiable
  type Outcome = Reclaimed | Held(Reason)
  type Report = { outcome: Outcome, details: List[Reason] }
  type Evaluation = Pending | Evaluated({ observation: Observation, report: Report })
  type State = { checked: int, last: Evaluation }

  pure val maxChecks = 4
  var s: State

  action init = s' = { checked: 0, last: Pending }

  pure val reclaimed: Report = { outcome: Reclaimed, details: [] }
  pure def held(reason: Reason): Report = { outcome: Held(reason), details: [reason] }

  pure def emptyRoot(input: Observation): Report =
    if (input.namespace == Deleting) reclaimed
    else match input.age {
      | BeforeThreshold => held(Young)
      | AtThreshold => reclaimed
      | AgeReadError => { outcome: Held(LeaseInvalid), details: [AgeUnverifiable, LeaseInvalid] }
    }

  pure def byAge(age: Age): Report = match age {
    | BeforeThreshold => held(Young)
    | AtThreshold => reclaimed
    | AgeReadError => held(AgeUnverifiable)
  }

  pure def decide(input: Observation): Report = match input.layout {
    | Empty => emptyRoot(input)
    | UnmarkedPayload => held(LeaseInvalid)
    | LeaseOnly => if (input.namespace == Deleting) reclaimed else byAge(input.age)
    | LeaseWithHeartbeat => byAge(input.age)
    | LeaseWithPayload =>
      if (input.namespace == Staging) held(UnsafeStaging) else held(HeartbeatInvalid)
    | MarkersWithPayload =>
      if (input.namespace == Staging) held(UnsafeStaging) else byAge(input.age)
  }

  action record(input: Observation, report: Report): bool = all {
    s.checked < maxChecks,
    s' = { checked: s.checked + 1,
      last: Evaluated({ observation: input, report: report }) },
  }

  action step = {
    nondet ns = Set(Staging, Deleting).oneOf()
    nondet layout = Set(Empty, UnmarkedPayload, LeaseOnly, LeaseWithHeartbeat,
      LeaseWithPayload, MarkersWithPayload).oneOf()
    val hasTimestampRead = Set(Empty, LeaseWithHeartbeat, MarkersWithPayload).contains(layout)
    nondet age = (if (hasTimestampRead) Set(BeforeThreshold, AtThreshold, AgeReadError)
      else Set(BeforeThreshold, AtThreshold)).oneOf()
    evaluate(ns, layout, age)
  }

  action evaluate(ns: Namespace, layout: Layout, age: Age): bool = {
    val input = { namespace: ns, layout: layout, age: age }
    record(input, decide(input))
  }

  action unmarkedStep =
    if (s.checked == 0) evaluate(Staging, Empty, BeforeThreshold)
    else if (s.checked == 1) evaluate(Staging, Empty, AtThreshold)
    else if (s.checked == 2) evaluate(Staging, Empty, AgeReadError)
    else evaluate(Deleting, Empty, BeforeThreshold)

  action markersStep =
    if (s.checked == 0) evaluate(Staging, LeaseOnly, BeforeThreshold)
    else if (s.checked == 1) evaluate(Staging, LeaseOnly, AtThreshold)
    else if (s.checked == 2) evaluate(Deleting, LeaseOnly, BeforeThreshold)
    else evaluate(Staging, LeaseWithHeartbeat, AtThreshold)

  action payloadStep =
    if (s.checked == 0) evaluate(Staging, UnmarkedPayload, AtThreshold)
    else if (s.checked == 1) evaluate(Deleting, UnmarkedPayload, AtThreshold)
    else if (s.checked == 2) evaluate(Staging, MarkersWithPayload, AtThreshold)
    else evaluate(Deleting, MarkersWithPayload, AtThreshold)

  action fallbackStep =
    if (s.checked == 0) evaluate(Deleting, LeaseWithPayload, AtThreshold)
    else if (s.checked == 1) evaluate(Deleting, LeaseWithHeartbeat, BeforeThreshold)
    else if (s.checked == 2) evaluate(Deleting, LeaseWithHeartbeat, AgeReadError)
    else evaluate(Staging, LeaseWithHeartbeat, AgeReadError)

  action ignoreStagingAgeStep =
    record({ namespace: Staging, layout: Empty, age: BeforeThreshold }, reclaimed)

  pure def safe(input: Observation, report: Report): bool = {
    val stagingAdmitted = input.age == AtThreshold and
      Set(Empty, LeaseOnly, LeaseWithHeartbeat).contains(input.layout)
    val deletingAdmitted = Set(Empty, LeaseOnly).contains(input.layout) or
      (input.age == AtThreshold and Set(LeaseWithHeartbeat, MarkersWithPayload).contains(input.layout))
    all {
      (report.outcome == Reclaimed) ==
        (if (input.namespace == Staging) stagingAdmitted else deletingAdmitted),
      match report.outcome {
        | Reclaimed => report.details == []
        | Held(reason) => report.details.length() > 0 and report.details[report.details.length() - 1] == reason
      },
      (input.namespace == Staging and input.layout == Empty and input.age == AgeReadError)
        implies report.details == [AgeUnverifiable, LeaseInvalid],
      (input.layout == UnmarkedPayload) implies report.outcome == Held(LeaseInvalid),
      (input.namespace == Staging and Set(LeaseWithPayload, MarkersWithPayload).contains(input.layout))
        implies report.outcome == Held(UnsafeStaging),
      (input.namespace == Deleting and input.layout == LeaseWithPayload)
        implies report.outcome == Held(HeartbeatInvalid),
    }
  }

  val Safety = match s.last {
    | Pending => true
    | Evaluated(value) => safe(value.observation, value.report)
  }
}
