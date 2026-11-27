// A stable staging or deleting directory encountered after an owner stopped.
// Admission is followed by one claim and removal attempt.
module JanitorNamespaceRecovery {
  type Namespace = Staging | Deleting
  type Layout = Empty | UnmarkedPayload | LeaseOnly | LeaseWithHeartbeat |
    LeaseWithPayload | MarkersWithPayload
  type Age = BeforeThreshold | AtThreshold | AgeReadError
  type Interruption = Uninterrupted | RenameError | UnlockError | PayloadUnlinkError | PayloadDeadline
  type Observation = { namespace: Namespace, layout: Layout, age: Age, interruption: Interruption }
  type Reason = Young | LeaseInvalid | UnsafeStaging | HeartbeatInvalid | AgeUnverifiable |
    ClaimFailed | CoordinatorFailed | DeleteFailed | CleanupIncomplete
  type Outcome = Reclaimed | Held(Reason)
  type Placement = OriginalName | DeletingName | RemovedName
  type Report = { outcome: Outcome, details: List[Reason], placement: Placement, removed: int }
  type Evaluation = Pending | Evaluated({ observation: Observation, report: Report })
  type State = { checked: int, last: Evaluation }

  pure val maxChecks = 4
  var s: State

  action init = s' = { checked: 0, last: Pending }

  pure def reclaimed(layout: Layout): Report = {
    val removed = match layout {
      | Empty => 1
      | LeaseOnly => 2
      | LeaseWithHeartbeat => 3
      | MarkersWithPayload => 4
      | _ => 0
    }
    { outcome: Reclaimed, details: [], placement: RemovedName, removed: removed }
  }
  pure def held(reason: Reason): Report =
    { outcome: Held(reason), details: [reason], placement: OriginalName, removed: 0 }

  pure def emptyRoot(input: Observation): Report =
    if (input.namespace == Deleting) reclaimed(Empty)
    else match input.age {
      | BeforeThreshold => held(Young)
      | AtThreshold => reclaimed(Empty)
      | AgeReadError => { ...held(LeaseInvalid), details: [AgeUnverifiable, LeaseInvalid] }
    }

  pure def byAge(age: Age, layout: Layout): Report = match age {
    | BeforeThreshold => held(Young)
    | AtThreshold => reclaimed(layout)
    | AgeReadError => held(AgeUnverifiable)
  }

  pure def admittedResult(input: Observation): Report = match input.layout {
    | Empty => emptyRoot(input)
    | UnmarkedPayload => held(LeaseInvalid)
    | LeaseOnly => if (input.namespace == Deleting) reclaimed(LeaseOnly)
      else byAge(input.age, LeaseOnly)
    | LeaseWithHeartbeat => byAge(input.age, LeaseWithHeartbeat)
    | LeaseWithPayload =>
      if (input.namespace == Staging) held(UnsafeStaging) else held(HeartbeatInvalid)
    | MarkersWithPayload =>
      if (input.namespace == Staging) held(UnsafeStaging) else byAge(input.age, MarkersWithPayload)
  }

  pure def decide(input: Observation): Report = {
    val admission = admittedResult(input)
    if (admission.outcome != Reclaimed) admission
    else match input.interruption {
      | RenameError =>
        if (input.namespace == Staging and input.layout != Empty) held(ClaimFailed) else admission
      | UnlockError =>
        if (input.layout == Empty) { ...admission, details: [CoordinatorFailed] }
        else { ...held(CoordinatorFailed),
          placement: if (input.namespace == Staging) DeletingName else OriginalName }
      | PayloadUnlinkError => held(DeleteFailed)
      | PayloadDeadline => { ...held(CleanupIncomplete), removed: 1 }
      | Uninterrupted => admission
    }
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
    attempt(ns, layout, age, Uninterrupted)
  }

  action attempt(ns: Namespace, layout: Layout, age: Age, interruption: Interruption): bool = {
    val input = { namespace: ns, layout: layout, age: age, interruption: interruption }
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

  action claimFailureStep =
    if (s.checked == 0) attempt(Staging, LeaseOnly, AtThreshold, RenameError)
    else if (s.checked == 1) attempt(Staging, LeaseWithHeartbeat, AtThreshold, UnlockError)
    else if (s.checked == 2) attempt(Deleting, Empty, BeforeThreshold, UnlockError)
    else attempt(Deleting, LeaseOnly, BeforeThreshold, RenameError)

  action sliceFailureStep =
    if (s.checked == 0) attempt(Deleting, MarkersWithPayload, AtThreshold, PayloadUnlinkError)
    else if (s.checked == 1) attempt(Deleting, MarkersWithPayload, AtThreshold, PayloadDeadline)
    else if (s.checked == 2) attempt(Deleting, MarkersWithPayload, BeforeThreshold, PayloadUnlinkError)
    else attempt(Staging, MarkersWithPayload, AtThreshold, PayloadUnlinkError)

  action ignoreStagingAgeStep =
    record({ namespace: Staging, layout: Empty, age: BeforeThreshold, interruption: Uninterrupted },
      reclaimed(Empty))

  pure def safe(input: Observation, report: Report): bool = {
    val stagingAdmitted = input.age == AtThreshold and
      Set(Empty, LeaseOnly, LeaseWithHeartbeat).contains(input.layout)
    val deletingAdmitted = Set(Empty, LeaseOnly).contains(input.layout) or
      (input.age == AtThreshold and Set(LeaseWithHeartbeat, MarkersWithPayload).contains(input.layout))
    val admitted = if (input.namespace == Staging) stagingAdmitted else deletingAdmitted
    val blockingFailure = match input.interruption {
      | Uninterrupted => false
      | RenameError => input.namespace == Staging and input.layout != Empty
      | UnlockError => input.layout != Empty
      | _ => true
    }
    all {
      (report.outcome == Reclaimed) == (admitted and not(blockingFailure)),
      (report.placement == RemovedName) == (report.outcome == Reclaimed),
      (report.placement == DeletingName) ==
        (admitted and input.namespace == Staging and input.layout != Empty and input.interruption == UnlockError),
      not(admitted) implies report.placement == OriginalName and report.removed == 0,
      (report.outcome != Reclaimed) implies report.removed ==
        (if (admitted and input.interruption == PayloadDeadline) 1 else 0),
      match report.outcome {
        | Reclaimed => report.removed == reclaimed(input.layout).removed and
          report.details == (if (input.interruption == UnlockError) [CoordinatorFailed] else [])
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
